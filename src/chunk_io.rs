//! Bounded asynchronous disk I/O for independently stored terrain chunks.
//!
//! Workers only move bytes. The save subsystem remains responsible for validating
//! chunk headers, coordinates, versions, and checksums before edits reach gameplay.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
};

use bevy::prelude::Resource;

use crate::terrain::ChunkCoord;

const WORKER_COUNT: usize = 2;
const REQUEST_CAPACITY: usize = 16;
const COMPLETION_CAPACITY: usize = 16;
pub(crate) const MAX_CHUNK_IO_BYTES: u64 = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChunkIoOperation {
    Read,
    Write,
}

#[derive(Clone, Debug)]
pub(crate) struct ChunkIoResult {
    pub coord: ChunkCoord,
    pub operation: ChunkIoOperation,
    pub result: Result<Vec<u8>, String>,
}

enum ChunkIoRequest {
    Read { coord: ChunkCoord, path: PathBuf },
    Write {
        coord: ChunkCoord,
        temp_path: PathBuf,
        final_path: PathBuf,
        bytes: Vec<u8>,
    },
}

impl ChunkIoRequest {
    fn coord(&self) -> ChunkCoord {
        match self {
            Self::Read { coord, .. } | Self::Write { coord, .. } => *coord,
        }
    }

    fn operation(&self) -> ChunkIoOperation {
        match self {
            Self::Read { .. } => ChunkIoOperation::Read,
            Self::Write { .. } => ChunkIoOperation::Write,
        }
    }
}

#[derive(Resource)]
pub struct ChunkIoQueue {
    sender: mpsc::SyncSender<ChunkIoRequest>,
    receiver: Mutex<mpsc::Receiver<ChunkIoResult>>,
    pending_reads: HashSet<ChunkCoord>,
    pending_writes: HashSet<ChunkCoord>,
}

impl Default for ChunkIoQueue {
    fn default() -> Self {
        let (request_sender, request_receiver) = mpsc::sync_channel::<ChunkIoRequest>(REQUEST_CAPACITY);
        let (completion_sender, completion_receiver) =
            mpsc::sync_channel::<ChunkIoResult>(COMPLETION_CAPACITY);
        let shared_requests = Arc::new(Mutex::new(request_receiver));

        for worker_id in 0..WORKER_COUNT {
            let requests = Arc::clone(&shared_requests);
            let completions = completion_sender.clone();
            let spawn = thread::Builder::new()
                .name(format!("astro-chunk-io-{worker_id}"))
                .spawn(move || loop {
                    // The mutex is held only while receiving one job, not while doing I/O.
                    let request = {
                        let Ok(receiver) = requests.lock() else {
                            break;
                        };
                        receiver.recv()
                    };
                    let Ok(request) = request else {
                        break;
                    };
                    let coord = request.coord();
                    let operation = request.operation();
                    let result = std::panic::catch_unwind(|| execute_request(request))
                        .unwrap_or_else(|_| Err("Chunk I/O worker panicked".into()));
                    if completions.send(ChunkIoResult {
                        coord,
                        operation,
                        result,
                    }).is_err() {
                        break;
                    }
                });
            if let Err(error) = spawn {
                bevy::log::error!("Could not start chunk I/O worker {worker_id}: {error}");
            }
        }

        Self {
            sender: request_sender,
            receiver: Mutex::new(completion_receiver),
            pending_reads: HashSet::new(),
            pending_writes: HashSet::new(),
        }
    }
}

impl ChunkIoQueue {
    /// Enqueue one disk read. The bounded queue rejects overload without blocking.
    pub(crate) fn request_read(&mut self, coord: ChunkCoord, path: PathBuf) -> bool {
        if !coord.is_representable()
            || self.pending_writes.contains(&coord)
            || !self.pending_reads.insert(coord)
        {
            return false;
        }
        match self.sender.try_send(ChunkIoRequest::Read { coord, path }) {
            Ok(()) => true,
            Err(_) => {
                self.pending_reads.remove(&coord);
                false
            }
        }
    }

    /// Enqueue an immutable chunk-file write. The caller must update the manifest
    /// only after receiving a successful completion for this record.
    pub(crate) fn request_write(
        &mut self,
        coord: ChunkCoord,
        temp_path: PathBuf,
        final_path: PathBuf,
        bytes: Vec<u8>,
    ) -> bool {
        if !coord.is_representable()
            || temp_path == final_path
            || bytes.is_empty()
            || bytes.len() as u64 > MAX_CHUNK_IO_BYTES
            || self.pending_reads.contains(&coord)
            || !self.pending_writes.insert(coord)
        {
            return false;
        }
        match self.sender.try_send(ChunkIoRequest::Write {
            coord,
            temp_path,
            final_path,
            bytes,
        }) {
            Ok(()) => true,
            Err(_) => {
                self.pending_writes.remove(&coord);
                false
            }
        }
    }

    /// Collect a bounded number of completions to cap per-frame work.
    pub(crate) fn collect_completed(&mut self, limit: usize) -> Vec<ChunkIoResult> {
        let mut completed = Vec::with_capacity(limit.min(COMPLETION_CAPACITY));
        let Ok(receiver) = self.receiver.lock() else {
            return completed;
        };
        for _ in 0..limit {
            match receiver.try_recv() {
                Ok(result) => {
                    match result.operation {
                        ChunkIoOperation::Read => { self.pending_reads.remove(&result.coord); }
                        ChunkIoOperation::Write => { self.pending_writes.remove(&result.coord); }
                    }
                    completed.push(result);
                }
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        }
        completed
    }

    pub(crate) fn is_read_pending(&self, coord: ChunkCoord) -> bool {
        self.pending_reads.contains(&coord)
    }

    pub(crate) fn is_write_pending(&self, coord: ChunkCoord) -> bool {
        self.pending_writes.contains(&coord)
    }

    pub(crate) fn pending_count(&self) -> usize {
        self.pending_reads.len() + self.pending_writes.len()
    }
}

fn execute_request(request: ChunkIoRequest) -> Result<Vec<u8>, String> {
    match request {
        ChunkIoRequest::Read { path, .. } => read_bounded(&path),
        ChunkIoRequest::Write { temp_path, final_path, bytes, .. } => {
            let result = (|| {
                if let Some(parent) = final_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| format!("Cannot create chunk directory: {error}"))?;
                }
                let mut file = File::create(&temp_path)
                    .map_err(|error| format!("Cannot create temporary chunk file: {error}"))?;
                file.write_all(&bytes)
                    .and_then(|_| file.sync_all())
                    .map_err(|error| format!("Cannot flush temporary chunk file: {error}"))?;
                drop(file);
                fs::rename(&temp_path, &final_path)
                    .map_err(|error| format!("Cannot commit immutable chunk file: {error}"))?;
                Ok(Vec::new())
            })();
            if result.is_err() {
                let _ = fs::remove_file(&temp_path);
            }
            result
        }
    }
}

fn read_bounded(path: &PathBuf) -> Result<Vec<u8>, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("Cannot inspect chunk file: {error}"))?;
    if metadata.len() > MAX_CHUNK_IO_BYTES {
        return Err("Chunk file exceeds the asynchronous I/O limit".into());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| format!("Cannot read chunk file: {error}"))?;
    if bytes.len() as u64 > MAX_CHUNK_IO_BYTES {
        return Err("Chunk file grew beyond the asynchronous I/O limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_starts_empty() {
        let queue = ChunkIoQueue::default();
        assert_eq!(queue.pending_count(), 0);
        let coord = ChunkCoord { x: 0, y: 0, z: 0 };
        assert!(!queue.is_read_pending(coord));
        assert!(!queue.is_write_pending(coord));
    }

    #[test]
    fn bounded_reader_rejects_missing_files() {
        let result = read_bounded(&PathBuf::from("this-file-should-not-exist.astrochunk"));
        assert!(result.is_err());
    }

    #[test]
    fn write_requests_reject_empty_and_oversized_payloads() {
        let mut queue = ChunkIoQueue::default();
        let coord = ChunkCoord { x: 1, y: 2, z: 3 };
        let temp = PathBuf::from("unused.chunk.tmp");
        let final_path = PathBuf::from("unused.chunk");
        assert!(!queue.request_write(coord, temp.clone(), final_path.clone(), Vec::new()));
        assert!(!queue.request_write(
            coord,
            temp,
            final_path,
            vec![0; MAX_CHUNK_IO_BYTES as usize + 1],
        ));
        assert_eq!(queue.pending_count(), 0);
    }

    #[test]
    fn writes_commit_only_after_sync_and_rename() {
        let root = std::env::temp_dir().join(format!(
            "astro-rust-chunk-io-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let final_path = root.join("chunks/test.chunk");
        let temp_path = root.join("chunks/test.chunk.tmp");
        let coord = ChunkCoord { x: 1, y: 2, z: 3 };
        let result = execute_request(ChunkIoRequest::Write {
            coord,
            temp_path: temp_path.clone(),
            final_path: final_path.clone(),
            bytes: vec![1, 2, 3, 4],
        });
        assert!(result.is_ok());
        assert_eq!(fs::read(&final_path).unwrap(), vec![1, 2, 3, 4]);
        assert!(!temp_path.exists());
        let _ = fs::remove_dir_all(root);
    }
}
