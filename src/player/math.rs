//! Local surface frames and deterministic spherical-player physics helpers.

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

/// Returns gravity directed toward the planet center.
pub fn radial_gravity(position: Vec3, strength: f32) -> Option<Vec3> {
    let length_squared = position.length_squared();
    if !length_squared.is_finite()
        || length_squared <= f32::EPSILON
        || !strength.is_finite()
        || strength < 0.0
    {
        return None;
    }

    Some(-position / length_squared.sqrt() * strength)
}

/// Resolves collision against a spherical planet boundary. The player is
/// clamped to the boundary and inward radial velocity is removed on contact.
pub fn resolve_spherical_contact(
    position: Vec3,
    velocity: Vec3,
    surface_radius: f32,
) -> (Vec3, Vec3, bool) {
    let distance_squared = position.length_squared();
    if !distance_squared.is_finite()
        || distance_squared <= f32::EPSILON
        || !surface_radius.is_finite()
        || surface_radius <= 0.0
    {
        return (position, velocity, false);
    }

    let distance = distance_squared.sqrt();
    if distance > surface_radius {
        return (position, velocity, false);
    }

    let up = position / distance;
    let corrected_velocity = if velocity.dot(up) < 0.0 {
        velocity - up * velocity.dot(up)
    } else {
        velocity
    };

    (up * surface_radius, corrected_velocity, true)
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

    #[test]
    fn gravity_points_toward_planet_center() {
        let gravity = radial_gravity(Vec3::Y * 4.0, 9.8).unwrap();
        assert!(gravity.dot(Vec3::Y) < 0.0);
        assert!((gravity.length() - 9.8).abs() < 1.0e-5);
    }

    #[test]
    fn gravity_rejects_invalid_inputs() {
        assert!(radial_gravity(Vec3::ZERO, 9.8).is_none());
        assert!(radial_gravity(Vec3::Y, -1.0).is_none());
    }

    #[test]
    fn contact_clamps_position_and_removes_inward_velocity() {
        let (position, velocity, grounded) = resolve_spherical_contact(
            Vec3::Y * 3.5,
            Vec3::new(2.0, -4.0, 0.0),
            3.62,
        );
        assert!(grounded);
        assert!((position.length() - 3.62).abs() < 1.0e-5);
        assert!(velocity.dot(Vec3::Y).abs() < 1.0e-5);
        assert!((velocity.x - 2.0).abs() < 1.0e-5);
    }

    #[test]
    fn contact_does_not_pull_airborne_player_down() {
        let position = Vec3::Y * 5.0;
        let velocity = Vec3::new(1.0, 2.0, 0.0);
        let (corrected_position, corrected_velocity, grounded) =
            resolve_spherical_contact(position, velocity, 3.62);
        assert!(!grounded);
        assert_eq!(corrected_position, position);
        assert_eq!(corrected_velocity, velocity);
    }
}
