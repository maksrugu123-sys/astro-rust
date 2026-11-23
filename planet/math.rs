//! Deterministic spherical-planet math primitives.
//! Positions are expressed in meters; the planet center is the origin.

use bevy::prelude::Vec3;

/// Returns the outward unit direction from the planet center.
/// A zero-length or non-finite input has no defined direction.
pub fn surface_normal(position: Vec3) -> Option<Vec3> {
    let length_squared = position.length_squared();
    if !length_squared.is_finite() || length_squared <= f32::EPSILON {
        return None;
    }
    Some(position / length_squared.sqrt())
}

/// Returns the point on a sphere of radius along direction.
pub fn point_on_sphere(direction: Vec3, radius: f32) -> Option<Vec3> {
    if !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let normal = surface_normal(direction)?;
    Some(normal * radius)
}

/// Returns the gravity direction at a position, pointing toward the center.
pub fn gravity_direction(position: Vec3) -> Option<Vec3> {
    surface_normal(position).map(|normal| -normal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_is_unit_length() {
        let normal = surface_normal(Vec3::new(0.0, 4.0, 0.0)).unwrap();
        assert!((normal.length() - 1.0).abs() < 1.0e-6);
        assert_eq!(normal, Vec3::Y);
    }

    #[test]
    fn projects_point_to_requested_radius() {
        let point = point_on_sphere(Vec3::new(2.0, 0.0, 0.0), 7.5).unwrap();
        assert!((point.length() - 7.5).abs() < 1.0e-5);
        assert_eq!(point, Vec3::X * 7.5);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(surface_normal(Vec3::ZERO).is_none());
        assert!(surface_normal(Vec3::splat(f32::NAN)).is_none());
        assert!(point_on_sphere(Vec3::Y, 0.0).is_none());
        assert!(point_on_sphere(Vec3::ZERO, 2.0).is_none());
    }

    #[test]
    fn gravity_points_inward() {
        assert_eq!(gravity_direction(Vec3::Y * 10.0).unwrap(), -Vec3::Y);
    }
}
