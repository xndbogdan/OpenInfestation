//! 004375D0 furniture camera effect. It resolves the actual 00478E00 camera
//! first. A nonzero argument then skips the flash when the unsigned squared
//! WORD distance exceeds the argument squared. Otherwise world flash 00487D70
//! runs on the current stack XYZ with radius F00h and style zero; AL is one.

use super::FurnitureCallbackContext;
use crate::terrain_runtime::TerrainRuntime;

/// Reached-callback loans; the queue's saved-next traversal keeps its owner.
pub struct FurnitureCameraCallbackInputs<'a> {
    pub context: &'a mut FurnitureCallbackContext,
    pub terrain: &'a TerrainRuntime,
    pub rng: &'a mut u32,
}

pub trait FurnitureCameraCallbackHost {
    type Error;
    /// Complete 004375D0 for the current stack XYZ and callback argument.
    fn camera_flash(
        &mut self,
        radius: u32,
        inputs: FurnitureCameraCallbackInputs<'_>,
    ) -> Result<(), Self::Error>;
}

/// 004375EB..00437627: signed WORD deltas and DWORD-wrapping squares are
/// compared unsigned with the wrapping square of the full argument; a zero
/// argument never reads the camera position.
pub fn furniture_camera_flash_admitted(
    position: [u16; 3],
    camera: impl FnOnce() -> Option<[u16; 3]>,
    radius: u32,
) -> Option<bool> {
    if radius == 0 {
        return Some(true);
    }
    let camera = camera()?;
    let squared = (0..3).fold(0u32, |sum, axis| {
        let delta = i32::from(position[axis].wrapping_sub(camera[axis]) as i16);
        sum.wrapping_add(delta.wrapping_mul(delta) as u32)
    });
    Some(squared <= radius.wrapping_mul(radius))
}

#[cfg(test)]
mod tests {
    use super::furniture_camera_flash_admitted;

    #[test]
    fn zero_radius_flashes_without_reading_the_camera() {
        assert_eq!(
            furniture_camera_flash_admitted([1, 2, 3], || panic!("camera read"), 0),
            Some(true)
        );
    }

    #[test]
    fn unsigned_square_gate_includes_the_boundary_and_wraps() {
        let camera = || Some([100, 200, 300]);
        // Distance 5 (3-4-0) against radius 5 and 4.
        assert_eq!(
            furniture_camera_flash_admitted([103, 204, 300], camera, 5),
            Some(true)
        );
        assert_eq!(
            furniture_camera_flash_admitted([103, 204, 300], camera, 4),
            Some(false)
        );
        // WORD deltas wrap: 65535 - 0 is -1, not 65535.
        assert_eq!(
            furniture_camera_flash_admitted([65535, 0, 0], || Some([0; 3]), 1),
            Some(true)
        );
        // A radius whose square wraps past 2^32 compares the low DWORD.
        assert_eq!(
            furniture_camera_flash_admitted([10, 0, 0], || Some([0; 3]), 0x1_0000),
            Some(false)
        );
        // A missing camera is reported, not treated as admitted.
        assert_eq!(furniture_camera_flash_admitted([0; 3], || None, 7), None);
    }
}
