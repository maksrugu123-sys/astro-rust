//! Local surface coordinate frames for movement on spherical planets.

use bevy::prelude::Vec3;

/// An orthonormal frame attached to a point on a spherical surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TangentFrame {
    /// Outward direction from the planet center.
    pub up: Vec3,
    /// Forward direction projected onto the local tangent plane.
    pub forward: Vec3,
    /// Right direction along the local tangent plane.
    pub right: Vec3,
}

/// Builds a stable local frame. If the preferred forward vector is parallel
/// to the surface normal, a fallback axis is selected.
pub fn tangent_frame(position: Vec3, preferred_forward: Vec3) -> Option<TangentFrame> {
    let length_squared = position.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return None;
    }

    let up = position / length_squared.sqrt();
    let projected = preferred_forward - up * preferred_forward.dot(up);
    let forward = if projected.length_squared() > 1.0e-6 {
        projected.normalize()
    } else {
        let fallback = if up.dot(Vec3::Y).abs() < 0.9 {
            Vec3::Y
        } else {
            Vec3::X
        };
        (fallback - up * fallback.dot(up)).normalize()
    };
    let right = up.cross(forward).normalize();

    Some(TangentFrame { up, forward, right })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_is_orthonormal() {
        let frame = tangent_frame(Vec3::Y * 4.0, Vec3::Z).unwrap();
        assert!((frame.up.length() - 1.0).abs() < 1.0e-6);
        assert!((frame.forward.length() - 1.0).abs() < 1.0e-6);
        assert!((frame.right.length() - 1.0).abs() < 1.0e-6);
        assert!(frame.up.dot(frame.forward).abs() < 1.0e-6);
        assert!(frame.up.dot(frame.right).abs() < 1.0e-6);
        assert!(frame.forward.dot(frame.right).abs() < 1.0e-6);
    }

    #[test]
    fn frame_handles_parallel_preferred_forward() {
        let frame = tangent_frame(Vec3::Y, Vec3::Y).unwrap();
        assert!(frame.up.dot(frame.forward).abs() < 1.0e-6);
        assert!(frame.right.is_finite());
    }

    #[test]
    fn frame_rejects_planet_center() {
        assert!(tangent_frame(Vec3::ZERO, Vec3::Z).is_none());
    }
}
