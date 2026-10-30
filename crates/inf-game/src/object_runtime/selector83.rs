//! Selector 83: constructor 0043C300 over the rotating visual base 0047A490,
//! final vtable 00522570, a patch of ground that sinks and rises. Its table
//! is the rotating base's but for the destructor (0043C320), the payload
//! reader +0C (0043C3B0, which keeps the payload at +78) and the update +24
//! (0043C3D0). The first update saves the H12 heights of the cells under
//! the patch (0043C4C0); after that the patch sinks to the water plane less
//! the payload's depth, waits, rises back to the saved heights and waits
//! again, over and over (0043C620 writes each cell through 00450890).

use thiserror::Error;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, RotatingVisual};
use crate::terrain_runtime::TerrainRuntime;

/// `0043C6CD`: the camera within 8000 moves a patch between its ends.
const CAMERA_RANGE_SQUARED: u32 = 0x03d0_9000;
/// 16.16: the patch fully sunk.
const SUNK: u32 = 0x1_0000;

/// The payload's WORDs +4..+E, which +78 keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector83Table {
    /// +4 and +6: the extent along X and along Z.
    pub extent: [i16; 2],
    /// +8: how far below the water plane the patch sinks.
    pub depth: i16,
    /// +A: the sinking and the rising each take this many 1,024 µs steps.
    pub travel: i16,
    /// +C: the steps it stays risen.
    pub risen: i16,
    /// +E: the steps it stays sunk.
    pub sunk: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector83State {
    pub rotating: RotatingVisual,
    /// +6C: microseconds in the current state.
    pub timer_6c: u32,
    /// +70: the saved heights, the raw low byte of each 128-unit cell, row
    /// by row along X; None until the first update saves them.
    pub heights_70: Option<Vec<u8>>,
    /// +74: 0 saving, 3 sinking, 4 sunk, 1 rising, 2 risen.
    pub state_74: u32,
    /// +78: the payload; None before the payload reader runs.
    pub table_78: Option<Selector83Table>,
}

impl Selector83State {
    pub(super) fn new() -> Self {
        Self {
            rotating: RotatingVisual::new(),
            timer_6c: 0,
            heights_70: None,
            state_74: 0,
            table_78: None,
        }
    }
}

/// What 0043C620 reaches beyond the arena.
pub trait Selector83Host {
    type Error: std::fmt::Display;
    /// H12/0 through 0055EE38; None when the world has none.
    fn terrain(&mut self) -> Option<&mut TerrainRuntime>;
    /// 00478E00 on G+8: the camera's position, or None for a camera actor
    /// without a camera (+60 null).
    fn camera_position(&mut self, arena: &mut ObjectArena)
        -> Result<Option<[u16; 3]>, Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector83Error<E: std::fmt::Display> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("patch {0:?} reads its payload +78 before the payload reader stored it")]
    UnreadPayload(ObjectId),
    #[error("patch {0:?} has no position")]
    PositionUnknown(ObjectId),
    /// 0043C534 allocates the cell count's bytes (operator new); a count
    /// that is not positive was never authored.
    #[error("patch {object:?} saves {cells:?} cells, which no placement authors")]
    CellCount { object: ObjectId, cells: [i32; 2] },
    /// 0043C5CE reads the loaded H12 without testing it.
    #[error("patch {0:?} saves its heights without H12")]
    MissingTerrain(ObjectId),
    #[error("patch {object:?} service at {address:08X}: {error}")]
    Host {
        object: ObjectId,
        address: u32,
        error: E,
    },
}

/// The cells under a patch: the counts along X and Z (each extent over 128,
/// toward zero) and the first cell's corner, the position less half the
/// counted extent (toward zero), as WORDs.
fn cells(table: Selector83Table, position: [u16; 3]) -> ([i32; 2], [u16; 2]) {
    let count = table.extent.map(|extent| i32::from(extent) / 128);
    let corner = [0, 1].map(|axis| {
        let half = (count[axis] << 7) / 2;
        position[axis * 2].wrapping_sub(half as u16)
    });
    (count, corner)
}

impl ObjectArena {
    pub fn selector83(&self, object: ObjectId) -> Result<&Selector83State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector83(state) => Ok(state),
            _ => Err(ObjectError::NotSelector83(object)),
        }
    }

    fn selector83_mut(&mut self, object: ObjectId) -> Result<&mut Selector83State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector83(state) => Ok(state),
            _ => Err(ObjectError::NotSelector83(object)),
        }
    }

    /// 0043C3A0: the payload reader keeps the payload at +78.
    pub(super) fn set_selector83_table(
        &mut self,
        object: ObjectId,
        table: Selector83Table,
    ) -> Result<(), ObjectError> {
        self.selector83_mut(object)?.table_78 = Some(table);
        Ok(())
    }

    /// 0043C3D0. The delta adds to the timer, whose steps are its
    /// microseconds over 1024. Saving (0) moves on to sinking once the
    /// heights are saved; sinking (3) and rising (1) move the cells by the
    /// steps over the travel and end with them at the far end; sunk (4) and
    /// risen (2) wait their steps. A change of state restarts the timer.
    pub fn tick_selector83<H: Selector83Host>(
        &mut self,
        object: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector83Error<H::Error>> {
        let state = self.selector83_mut(object)?;
        state.timer_6c = state.timer_6c.wrapping_add(delta);
        let steps = (state.timer_6c >> 10) as i32;
        let current = state.state_74;
        let table = state.table_78;
        let table = |object| table.ok_or(Selector83Error::UnreadPayload(object));
        let next = match current {
            0 => {
                if self.save_selector83(object, host)? {
                    3
                } else {
                    0
                }
            }
            3 => {
                let travel = i32::from(table(object)?.travel);
                if steps >= travel {
                    self.move_selector83(object, SUNK, host)?;
                    4
                } else {
                    self.move_selector83(object, ((steps << 16) / travel) as u32, host)?;
                    3
                }
            }
            1 => {
                let travel = i32::from(table(object)?.travel);
                if steps >= travel {
                    self.move_selector83(object, 0, host)?;
                    2
                } else {
                    let fraction = 0x1_0000 - (steps << 16) / travel;
                    self.move_selector83(object, fraction as u32, host)?;
                    1
                }
            }
            2 if steps >= i32::from(table(object)?.risen) => 3,
            4 if steps >= i32::from(table(object)?.sunk) => 1,
            other => other,
        };
        let state = self.selector83_mut(object)?;
        if state.state_74 != next {
            state.timer_6c = 0;
            state.state_74 = next;
        }
        Ok(())
    }

    /// 0043C4C0: frees any earlier save, then keeps the raw low byte of each
    /// cell under the patch at 128-unit addressing (00450350). Answers
    /// whether the save exists.
    fn save_selector83<H: Selector83Host>(
        &mut self,
        object: ObjectId,
        host: &mut H,
    ) -> Result<bool, Selector83Error<H::Error>> {
        self.selector83_mut(object)?.heights_70 = None;
        let table = self
            .selector83(object)?
            .table_78
            .ok_or(Selector83Error::UnreadPayload(object))?;
        let position = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector83Error::PositionUnknown(object))?;
        let (count, corner) = cells(table, position);
        if count.iter().any(|&count| count <= 0) {
            return Err(Selector83Error::CellCount {
                object,
                cells: count,
            });
        }
        let terrain = host
            .terrain()
            .ok_or(Selector83Error::MissingTerrain(object))?;
        let mut heights = Vec::with_capacity((count[0] * count[1]) as usize);
        for i in 0..count[0] {
            let x = corner[0].wrapping_add((i << 7) as u16);
            for j in 0..count[1] {
                let z = corner[1].wrapping_add((j << 7) as u16);
                heights.push(terrain.cell_at_grid([x >> 7, z >> 7]).raw() as u8);
            }
        }
        self.selector83_mut(object)?.heights_70 = Some(heights);
        Ok(true)
    }

    /// 0043C620: each saved cell moves toward the water plane less the
    /// depth by the 16.16 fraction (the difference times the fraction,
    /// SAR 16, plus the saved height), through 00450890 at 128-unit
    /// addressing. Between the ends only a patch within 8000 of the camera
    /// moves. Nothing moves before the save or without H12.
    fn move_selector83<H: Selector83Host>(
        &mut self,
        object: ObjectId,
        fraction: u32,
        host: &mut H,
    ) -> Result<(), Selector83Error<H::Error>> {
        let state = self.selector83(object)?;
        let Some(heights) = state.heights_70.clone() else {
            return Ok(());
        };
        let table = state
            .table_78
            .ok_or(Selector83Error::UnreadPayload(object))?;
        let position = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector83Error::PositionUnknown(object))?;
        let Some(terrain) = host.terrain() else {
            return Ok(());
        };
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
        let target = i32::from(plane) - i32::from(table.depth);
        if fraction != 0 && fraction != SUNK {
            let camera = host
                .camera_position(self)
                .map_err(|error| Selector83Error::Host {
                    object,
                    address: 0x0043_c68c,
                    error,
                })?;
            let Some(camera) = camera else {
                return Ok(());
            };
            let squared = (0..3).fold(0_i32, |sum, axis| {
                let delta = i32::from(position[axis].wrapping_sub(camera[axis]) as i16);
                sum.wrapping_add(delta.wrapping_mul(delta))
            }) as u32;
            if squared > CAMERA_RANGE_SQUARED {
                return Ok(());
            }
        }
        let Some(terrain) = host.terrain() else {
            return Ok(());
        };
        let shift = terrain.height_shift();
        let (count, corner) = cells(table, position);
        let mut saved = heights.iter();
        for i in 0..count[0] {
            let x = corner[0].wrapping_add((i << 7) as u16);
            for j in 0..count[1] {
                let z = corner[1].wrapping_add((j << 7) as u16);
                let Some(&byte) = saved.next() else {
                    return Err(Selector83Error::CellCount {
                        object,
                        cells: count,
                    });
                };
                let height = i32::from(byte as i8) << shift;
                let moved = (target.wrapping_sub(height).wrapping_mul(fraction as i32) >> 16)
                    .wrapping_add(height);
                terrain.write_raw_height_128([x, z], moved);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector83_tests.rs"]
mod tests;
