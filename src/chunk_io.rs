//! Bounded asynchronous disk reads for independently stored terrain chunks.
//!
//! This queue is infrastructure for the disk-backed terrain cache. It deliberately
//! returns raw bytes: the save subsystem remains the single authority for validating
//! chunk headers, coordinates, versions, and checksums before edits reach gameplay.

use std::{
    collections::HashSet,
    fs::File,
    io::Read,
    path::PathBuf,
    sync::{mpsc, Arc, Mutex},
    thread,
};

use bevy::prelude::Resource;

use crate::terrain::ChunkCoord;

const WORKER_COUNT: usize = 2;
const REQUEST_CAPACITY: usize = 16;
const COMPLETION_CAPACITY: usize = 16;
pub(crate) const MAX_CHUNK_READ_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct ChunkReadResult {
    pub coord: ChunkCoord,
    pub result: Result<Vec<u8>, String>,
}

struct ReadRequest {
    coord: ChunkCoord,
    path: PathBuf,
}

#[derive(Resource)]
pub struct ChunkIoQueue {
    sender: mpsc::SyncSender<ReadRequest>,
    receiver: Mutex<mpsc::Receiver<ChunkReadResult>>,
    pending: HashSet<ChunkCoord>,
}

impl Default for ChunkIoQueue {
    fn default() -> Self {
        let (request_sender, request_receiver) = mpsc::sync_channel::<ReadRequest>(REQUEST_CAPACITY);
        let (completion_sender, completion_receiver) =
            mpsc::sync_channel::<ChunkReadResult>(COMPLETION_CAPACITY);
        let shared_requests = Arc::new(Mutex::new(request_receiver));

        for worker_id in 0..WORKER_COUNT {
            let requests = Arc::clone(&shared_requests);
            let completions = completion_sender.clone();
            let spawn = thread::Builder::new()
                .name(format!("astro-chunk-io-{worker_id}"))
                .spawn(move || loop {
                    // Release the receiver lock immediately after receiving a job so
                    // another worker can receive and process a different file.
                    let request = {
                        let Ok(receiver) = requests.lock() else {
                            break;
                        };
                        receiver.recv()
                    };
                    let Ok(request) = request else {
                        break;
                    };

                    let result = std::panic::catch_unwind(|| read_bounded(&request.path))
                        .unwrap_or_else(|_| Err("Chunk read worker panicked".into()));
                    if completions.send(ChunkReadResult {
                        coord: request.coord,
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
            pending: HashSet::new(),
        }
    }
}

impl ChunkIoQueue {
    /// Enqueue one disk read. Returns false if it is already pending or the
    /// bounded request queue is full; callers may retry on a later frame.
    pub(crate) fn request_read(&mut self, coord: ChunkCoord, path: PathBuf) -> bool {
        if !coord.is_representable() || !self.pending.insert(coord) {
            return false;
        }

        match self.sender.try_send(ReadRequest { coord, path }) {
            Ok(()) => true,
            Err(mpsc::TrySendError::Full(_)) => {
                self.pending.remove(&coord);
                false
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.pending.remove(&coord);
                false
            }
        }
    }

    /// Collects at most `limit` completions so disk bursts cannot monopolize a frame.
    pub(crate) fn collect_completed(&mut self, limit: usize) -> Vec<ChunkReadResult> {
        let mut completed = Vec::with_capacity(limit.min(COMPLETION_CAPACITY));
        let Ok(receiver) = self.receiver.lock() else {
            return completed;
        };
        for _ in 0..limit {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending.remove(&result.coord);
                    completed.push(result);
                }
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        }
        completed
    }

    pub(crate) fn is_pending(&self, coord: ChunkCoord) -> bool {
        self.pending.contains(&coord)
    }

    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

fn read_bounded(path: &PathBuf) -> Result<Vec<u8>, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| format!("Cannot inspect chunk file: {error}"))?;
    if metadata.len() > MAX_CHUNK_READ_BYTES {
        return Err("Chunk file exceeds the asynchronous read limit".into());
    }

    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    File::open(path)
        .and_then(|mut file| file.read_to_end(&mut bytes))
        .map_err(|error| format!("Cannot read chunk file: {error}"))?;
    if bytes.len() as u64 > MAX_CHUNK_READ_BYTES {
        return Err("Chunk file grew beyond the asynchronous read limit".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_starts_with_no_pending_requests() {
        let queue = ChunkIoQueue::default();
        assert_eq!(queue.pending_count(), 0);
        assert!(!queue.is_pending(ChunkCoord { x: 0, y: 0, z: 0 }));
    }

    #[test]
    fn bounded_reader_rejects_missing_files() {
        let result = read_bounded(&PathBuf::from("this-file-should-not-exist.astrochunk"));
        assert!(result.is_err());
    }
}
