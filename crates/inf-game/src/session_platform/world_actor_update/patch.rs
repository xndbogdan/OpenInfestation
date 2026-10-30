//! The sinking patch's update `0043C3D0` (selector 83): the world's H12 and
//! the world camera `00478E00`.

use crate::{
    object_runtime::{ObjectArena, Selector83Host, WorldCameraLookup},
    terrain_runtime::TerrainRuntime,
};

pub(crate) struct WorldPatchHost<'a> {
    pub terrain: &'a mut TerrainRuntime,
}

impl Selector83Host for WorldPatchHost<'_> {
    type Error = String;

    fn terrain(&mut self) -> Option<&mut TerrainRuntime> {
        Some(self.terrain)
    }

    fn camera_position(&mut self, arena: &mut ObjectArena) -> Result<Option<[u16; 3]>, String> {
        match arena
            .world_camera_lookup()
            .map_err(|error| error.to_string())?
        {
            WorldCameraLookup::Found(camera) => arena
                .object(camera)
                .map_err(|error| error.to_string())?
                .base
                .position_words
                .map(Some)
                .ok_or_else(|| "the camera has no position".into()),
            WorldCameraLookup::NoCamera => Ok(None),
            // 00478E00 builds a camera when M+74 names none; a world in play
            // always has one.
            WorldCameraLookup::NeedsCamera => Err("00478E00 would build the camera".into()),
        }
    }
}
