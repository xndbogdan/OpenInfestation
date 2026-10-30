//! The bridge's update 00409A20 (+24): it raises a terrain strip once its
//! links let it go.
//!
//! The first update samples the strip's two ends under the temporary fine
//! spacing (00450350, then 00450370 restores it from settings +70). While
//! the bridge has free staff (+CC), or its links hold it (+14D with +14C
//! nonzero), or its player lacks grant +17E, a one-second timer +16C stays
//! full. Once that runs out (with sound 0x17) the bridge counts +170 down
//! over +164 seconds. Each update of it sets the first player's commands
//! +17C and 0x31, then pulls every cell of the strip, 0x80 apart, from its
//! height toward a straight ramp between the two ends by the elapsed share
//! of the time. The last update writes the ramp itself.
//!
//! A water bridge (+166) samples the water plane instead of the strip, and
//! raises the plane rather than the cells. While its released links let the
//! timer run, each update restarts the mission's doom countdown at +164
//! seconds.

use thiserror::Error;

use crate::{
    object_runtime::{GrantOwner, LiveGrantError, ObjectArena, ObjectError, ObjectId, PlayerId},
    terrain_runtime::{ProcessTerrainPolicy, TerrainRuntime},
};

/// 00487F10's sound as the timer +16C runs out.
pub const BRIDGE_SOUND: u32 = 0x17;
/// The timer +16C holds one second.
const HOLD: u32 = 1_000_000;
/// Cells are 0x80 apart along and across the strip.
const STEP: i32 = 0x80;

/// The terrain the bridge reshapes, with the spacing owner its temporary
/// fine scopes borrow.
pub struct BridgeTerrain<'a> {
    pub terrain: &'a mut TerrainRuntime,
    pub policy: &'a mut ProcessTerrainPolicy,
    /// Settings +70, which 00450370 rereads.
    pub detail: u32,
    pub world_flags: Option<u32>,
}

impl BridgeTerrain<'_> {
    /// `00450350`: the temporary fine spacing.
    pub(crate) fn fine(&mut self) {
        self.policy.temporary_fine(self.terrain);
    }

    /// `00450370`: the spacing from settings +70 and the world's flags.
    pub(crate) fn restore(&mut self) {
        self.policy
            .restore_detail(self.detail, self.world_flags, self.terrain);
    }
}

pub trait Selector37Host {
    type Error: std::fmt::Display;
    /// The terrain and the spacing owner the fine scopes borrow.
    fn terrain(&mut self) -> BridgeTerrain<'_>;
    /// `00487F10(0x17, position, 0x10000, 0x4000)`.
    fn sound(&mut self, position: [u16; 3]) -> Result<(), Self::Error>;
    /// `00409DFD..00409E1F`: the mission's doom countdown, W+24 (G+1D4) =
    /// `micros` and W+17 (G+1C7) = 1.
    fn start_doom(&mut self, micros: u32);
    /// `0046C760(raw_id, silent)` on the player.
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        raw_id: u32,
        silent: u8,
    ) -> Result<(), Self::Error>;
    /// `0047C990`, the shared status update.
    fn status(
        &mut self,
        arena: &mut ObjectArena,
        bridge: ObjectId,
        delta: u32,
    ) -> Result<(), Self::Error>;
    /// `00460310` with H13 resource 2: a staff member the rise's end sends
    /// out (`0040DCB0`).
    fn create_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, Self::Error>;
    /// `00489A90(2, position, 0)` where that scientist stands.
    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// `0040DCB0` over the bridge's host: `00409ED5` pushes no vehicle.
struct BridgeRelease<'h, H>(&'h mut H);

impl<H: Selector37Host> crate::object_runtime::StaffReleaseHost for BridgeRelease<'_, H> {
    type ReleaseError = H::Error;
    fn release_scientist(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<Option<ObjectId>, H::Error> {
        self.0.create_scientist(arena, position)
    }
    fn release_flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
    ) -> Result<(), H::Error> {
        self.0.flash(arena, position)
    }
    fn release_null(&self) -> Option<crate::object_runtime::ActorHandle> {
        None
    }
    fn release_carry(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, H::Error> {
        unreachable!("00409ED5 releases the bridge's staff to no vehicle")
    }
}

#[derive(Debug, Error)]
pub enum Selector37Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
    #[error("bridge {bridge:?} service at {address:08X}: {error}")]
    Host {
        bridge: ObjectId,
        address: u32,
        error: E,
    },
    /// A branch the port does not run: a strip whose ends were never
    /// sampled, or a division by zero seconds.
    #[error("bridge {bridge:?} reaches the unported {address:08X}")]
    Boundary { bridge: ObjectId, address: u32 },
    #[error("bridge {0:?} needs a first player (00488AA0)")]
    MissingPlayer(ObjectId),
    #[error("bridge {bridge:?} staff release (0040DCB0): {reason}")]
    Release { bridge: ObjectId, reason: String },
}

impl ObjectArena {
    /// 00409A20, the bridge's +24.
    pub fn tick_selector37<H: Selector37Host>(
        &mut self,
        bridge: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector37Error<H::Error>> {
        let fail = |address| {
            move |error| Selector37Error::Host {
                bridge,
                address,
                error,
            }
        };
        if self.selector37(bridge)?.byte_168 == 0 {
            self.sample_selector37(bridge, &mut host.terrain())?;
        }
        let state = self.selector37(bridge)?;
        let (links, held) = (
            state.intermediate.byte_14d != 0,
            state.intermediate.byte_14c != 0,
        );
        let mut hold = self.free_staff(bridge, None)? != 0;
        if !hold && state.intermediate.word_140 == 0 {
            hold = links && held;
            let gate = state.word_17e;
            if !hold && gate != 0 {
                let player = self
                    .first_player()
                    .ok_or(Selector37Error::MissingPlayer(bridge))?;
                // 004752E0 on the first player's grants +9C.
                hold = !self.live_grant_has(GrantOwner::Player9c(player), gate)?;
            }
            let state = self.selector37(bridge)?;
            if !hold && links && !held && state.byte_166 != 0 && state.word_16c != 0 {
                // 00409DEF: a water bridge (re)starts the mission's doom
                // countdown at +164 seconds while its timer runs.
                host.start_doom(u32::from(state.byte_164) * 1_000_000);
            }
        }
        if hold {
            self.selector37_mut(bridge)?.word_16c = HOLD;
        } else {
            let state = self.selector37_mut(bridge)?;
            let timer = state.word_16c;
            if timer > delta {
                state.word_16c = timer - delta;
            } else if timer != 0 {
                state.word_16c = 0;
                let position = self.selector37_position(bridge)?;
                host.sound(position).map_err(fail(0x0040_9e52))?;
            }
        }
        let state = self.selector37(bridge)?;
        if state.word_170 != 0 && state.word_16c == 0 {
            self.raise_selector37(bridge, delta, host)?;
        }
        host.status(self, bridge, delta).map_err(fail(0x0047_c990))
    }

    /// 00409A37..00409D68: the strip's end heights under the fine spacing,
    /// or a water bridge's plane.
    fn sample_selector37<E>(
        &mut self,
        bridge: ObjectId,
        terrain: &mut BridgeTerrain<'_>,
    ) -> Result<(), Selector37Error<E>> {
        terrain.fine();
        let state = self.selector37(bridge)?;
        if state.byte_166 != 0 {
            // 00409A44..00409A5C: the H12 plane shifted down by 005483BC,
            // as a signed WORD. +178 stays unwritten.
            let plane = terrain
                .terrain
                .plane_base()
                .wrapping_shr(terrain.terrain.plane_shift());
            let state = self.selector37_mut(bridge)?;
            state.word_174 = Some(i32::from(plane as i16));
            state.byte_168 = 1;
            terrain.restore();
            return Ok(());
        }
        let (direction, length, offset) = (state.byte_165, state.word_158 as u16, state.word_160);
        let [x, _, z] = self.selector37_position(bridge)?;
        let ends = match direction {
            0 => Some([[x, z.wrapping_add(0x100)], [x, z.wrapping_add(length)]]),
            1 => Some([[x.wrapping_add(0x100), z], [x.wrapping_add(length), z]]),
            2 => Some([[x, z.wrapping_sub(0x100)], [x, z.wrapping_sub(length)]]),
            3 => Some([[x.wrapping_sub(0x100), z], [x.wrapping_sub(length), z]]),
            _ => None,
        };
        if let Some([start, end]) = ends {
            let sample = |xz| i32::from(terrain.terrain.raw_height_at(xz)).wrapping_add(offset);
            let (start, end) = (sample(start), sample(end));
            let state = self.selector37_mut(bridge)?;
            state.word_174 = Some(start);
            state.word_178 = Some(end);
        }
        self.selector37_mut(bridge)?.byte_168 = 1;
        terrain.restore();
        Ok(())
    }

    /// 00409E82..0040A0C6: the commands, the countdown, then the strip.
    fn raise_selector37<H: Selector37Host>(
        &mut self,
        bridge: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector37Error<H::Error>> {
        let fail = |address| {
            move |error| Selector37Error::Host {
                bridge,
                address,
                error,
            }
        };
        let message = self.selector37(bridge)?.word_17c as i16;
        if (0..0x49).contains(&message) {
            let player = self
                .first_player()
                .ok_or(Selector37Error::MissingPlayer(bridge))?;
            host.player_command(self, player, message as u32, 0)
                .map_err(fail(0x0040_9ea0))?;
            host.player_command(self, player, 0x31, 1)
                .map_err(fail(0x0040_9eb0))?;
        }
        let state = self.selector37_mut(bridge)?;
        if state.word_170 > delta {
            state.word_170 -= delta;
        } else {
            state.word_170 = 0;
            // 00409ECF..00409EF3: 0040DCB0 sends out the staff, then the
            // capacity clears and the staff clamps to it.
            let staff = state.intermediate.word_140;
            self.evacuate_factory(bridge, staff, None, &mut BridgeRelease(&mut *host))
                .map_err(|error| match error {
                    crate::object_runtime::FactoryError::Host { address, error } => {
                        Selector37Error::Host {
                            bridge,
                            address,
                            error,
                        }
                    }
                    crate::object_runtime::FactoryError::Object(error) => {
                        Selector37Error::Object(error)
                    }
                    error => Selector37Error::Release {
                        bridge,
                        reason: error.to_string(),
                    },
                })?;
            let state = self.selector37_mut(bridge)?;
            state.intermediate.word_13c = 0;
            state.intermediate.word_140 = 0;
        }
        let state = self.selector37(bridge)?;
        if state.byte_166 != 0 {
            // 0040A0DD..0040A146: a water bridge raises the plane instead,
            // with neither spacing call: +174 plus +160 times the elapsed
            // microseconds shifted down by 10, over +164 thousands, shifted
            // back up by 005483BC into the H12 plane DWORD.
            let seconds = i32::from(state.byte_164);
            let elapsed = seconds
                .wrapping_mul(1_000_000)
                .wrapping_sub(state.word_170 as i32)
                >> 10;
            let thousands = seconds * 1000;
            if thousands == 0 {
                return Err(Selector37Error::Boundary {
                    bridge,
                    address: 0x0040_a119,
                });
            }
            let start = state.word_174.ok_or(Selector37Error::Boundary {
                bridge,
                address: 0x0040_a11b,
            })?;
            let height = (elapsed.wrapping_mul(state.word_160) / thousands).wrapping_add(start);
            let terrain = host.terrain().terrain;
            terrain.set_plane_base(height.wrapping_shl(terrain.plane_shift()));
            return Ok(());
        }
        if state.byte_167 != 0 {
            return Ok(());
        }
        let ends = state.word_174.zip(state.word_178);
        let total = (i32::from(state.byte_164)).wrapping_mul(1_000_000);
        let remaining = state.word_170 as i32;
        let (direction, length, width) =
            (state.byte_165, state.word_158 as i32, state.word_15c as i32);
        let [x, _, z] = self.selector37_position(bridge)?;
        let boundary = |address| Selector37Error::Boundary { bridge, address };
        let mut terrain = host.terrain();
        terrain.fine();
        // An empty strip reaches none of the cell stops below.
        let mut along = 0_i32;
        while along < length {
            let mut across = width.wrapping_neg();
            while across <= width {
                // 00409F62: the ends a direction above 3 never sampled.
                let (start, end) = ends.ok_or_else(|| boundary(0x0040_9f62))?;
                let ramp = (end.wrapping_sub(start))
                    .wrapping_mul(along)
                    .wrapping_div(length)
                    .wrapping_add(start);
                let (on, off) = (along as u16, across as u16);
                let cell = match direction {
                    0 => [x.wrapping_add(off), z.wrapping_add(on).wrapping_add(0x100)],
                    1 => [x.wrapping_add(on).wrapping_add(0x100), z.wrapping_add(off)],
                    2 => [x.wrapping_add(off), z.wrapping_sub(on).wrapping_sub(0x100)],
                    3 => [x.wrapping_sub(on).wrapping_sub(0x100), z.wrapping_add(off)],
                    // 00409FAC: a direction above 3 writes the previous cell
                    // again, or stack residue.
                    _ => return Err(boundary(0x0040_a01e)),
                };
                let height = i32::from(terrain.terrain.raw_height_at(cell));
                let height = if remaining != 0 {
                    if total == 0 {
                        // 0040A071 divides by the seconds' microseconds.
                        return Err(boundary(0x0040_a071));
                    }
                    let elapsed = total.wrapping_sub(remaining);
                    height.wrapping_add((ramp.wrapping_sub(height)).wrapping_mul(elapsed) / total)
                } else {
                    ramp
                };
                terrain.terrain.write_raw_height(cell, height);
                across = across.wrapping_add(STEP);
            }
            along = along.wrapping_add(STEP);
        }
        terrain.restore();
        Ok(())
    }

    fn selector37_position(&self, bridge: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(bridge)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(bridge))
    }
}

#[cfg(test)]
#[path = "bridge_tests.rs"]
mod tests;
