//! SDL-independent orbit camera state used by the model diagnostic view.

use std::f32::consts::FRAC_PI_2;

const MIN_PITCH: f32 = -FRAC_PI_2 + 0.02;
const MAX_PITCH: f32 = FRAC_PI_2 - 0.02;
const MIN_ZOOM: f32 = 0.05;
const MAX_ZOOM: f32 = 40.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OrbitCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        Self {
            yaw: -0.65,
            pitch: 0.35,
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

impl OrbitCamera {
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw = (self.yaw + dx * 0.01).rem_euclid(std::f32::consts::TAU);
        self.pitch = (self.pitch + dy * 0.01).clamp(MIN_PITCH, MAX_PITCH);
    }

    pub fn pan(&mut self, dx: f32, dy: f32) {
        self.pan_x += dx;
        self.pan_y += dy;
    }

    pub fn zoom_by_wheel(&mut self, wheel_y: i32) {
        self.zoom = (self.zoom * 1.12_f32.powi(wheel_y)).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub fn has_pan(&self) -> bool {
        self.pan_x.abs() > f32::EPSILON || self.pan_y.abs() > f32::EPSILON
    }

    /// Reset only the position offset. Orbit and zoom are useful preferences
    /// that deliberately survive model changes.
    pub fn reset_position(&mut self) {
        self.pan_x = 0.0;
        self.pan_y = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orbit_and_zoom_are_bounded() {
        let mut camera = OrbitCamera::default();
        camera.orbit(0.0, 100_000.0);
        assert_eq!(camera.pitch, MAX_PITCH);
        camera.orbit(0.0, -200_000.0);
        assert_eq!(camera.pitch, MIN_PITCH);

        camera.zoom_by_wheel(1_000);
        assert_eq!(camera.zoom, MAX_ZOOM);
        camera.zoom_by_wheel(-1_000);
        assert_eq!(camera.zoom, MIN_ZOOM);
    }

    #[test]
    fn position_reset_only_clears_pan() {
        let mut camera = OrbitCamera::default();
        camera.orbit(13.0, -9.0);
        camera.zoom_by_wheel(2);
        let yaw = camera.yaw;
        let pitch = camera.pitch;
        let zoom = camera.zoom;
        camera.pan(24.0, -17.0);
        assert!(camera.has_pan());

        camera.reset_position();

        assert!(!camera.has_pan());
        assert_eq!((camera.yaw, camera.pitch, camera.zoom), (yaw, pitch, zoom));
    }
}
