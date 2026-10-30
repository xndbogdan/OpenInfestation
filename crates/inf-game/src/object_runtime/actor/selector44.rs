//! Selector 44, the falling rock: the shared 0047A7A0 with the final table
//! 00525648 (resource 0x18, 0x140 bytes, constructor 00453E70). Its table is
//! the shared actor's 00527C00 but for +00 (00453EA0, the same 0047AA60
//! teardown), +24 (the update 00453ED0), +28 (its glow 00454040), +2C (its
//! draw 00454000), +74 (00404610, which answers itself), +94 (00458C30),
//! +98 (00454160), +A4 (00454180), +A8 (00454190) and +AC (its death
//! 00453F30).
//!
//! The timed spawner (selector 43) throws them. A rock moves under the
//! shared physics, with gravity only when it was thrown upward, and smokes
//! (0047DFD0). It lights the ground below it, more dimly the higher it is,
//! and draws through the fog. The ground, an actor or furniture it touches
//! and message 5 retire it (0047CE90). Its death blasts twice, bursts,
//! builds the visuals 0x36 and 0x37 and disposes of it.

use thiserror::Error;

use super::{
    ActorConstructionError, ActorConstructionRequest, ActorExtension, ActorHandle,
    ActorMotionError, ActorPhysicsRequest,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::process_particles::births::ParticleBurst;
use crate::terrain_runtime::TerrainRuntime;
use crate::world_lighting::TerrainLightRequest;

/// 00453EF0 and 00453EFC: the first update's masks, with gravity for a rock
/// thrown upward, position and world spin for every rock.
const RISING_MASK: u32 = 0x19;
const THROWN_MASK: u32 = 0x18;

/// 005483FC, the glow's colour (110, 76, 32), set by the rock's module
/// start 00453E30. WinMain's game module (00438E10) acquires every object
/// module, the rock's among them, before its message loop; the fourth byte
/// is the BSS zero.
pub const ROCK_GLOW: u32 = 0x0020_4c6e;

/// 00454160's one arm: message 5 retires the rock.
pub const ROCK_RETIRE_MESSAGE: i32 = 5;

/// 00453F53 and 00453F6D: the death's radius blocks, +4 and +14 of the
/// static 00537680 whose module 00453E30 starts.
pub const ROCK_BLASTS: [u32; 2] = [0x0053_7688, 0x0053_7698];

/// 00453FD3 and 00453FE0: the death's visuals, through +4C (0047AAF0).
pub const ROCK_VISUALS: [u32; 2] = [0x36, 0x37];

/// 00453F9C..00453FBA: sixteen type-4 debris round a type-2 centre within
/// 20, speed 8, no flags.
fn rock_burst(position: [u16; 3]) -> ParticleBurst {
    ParticleBurst {
        debris_count: 0x10,
        position,
        placement_radius: 0x14,
        central_type: 2,
        debris_types: [4, 4],
        speed: 8,
        flags: 0,
    }
}

/// 00454010: the draw's fade bounds, which leave the rock unfogged nearer
/// than 0x3FFF.
pub const ROCK_DRAW_FADE: i32 = 0x3fff;

/// 00453E70's fields after 0047A7A0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector44State {
    /// +138: the first update has chosen the mask.
    pub started_138: u8,
    /// +139: it chose gravity.
    pub rising_139: u8,
    /// +13C: the physics mask; the constructor's 1 until the first update.
    pub mask_13c: u32,
}

impl Selector44State {
    fn new() -> Self {
        Self {
            started_138: 0,
            rising_139: 0,
            mask_13c: 1,
        }
    }
}

#[derive(Debug, Error)]
pub enum Selector44Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("rock {0:?} physics 0047B1E0")]
    Physics(ObjectId, #[source] ActorMotionError),
    #[error("rock {0:?} has no position")]
    Position(ObjectId),
    /// 004540DA: the height above the ground squares to zero (a multiple of
    /// 0x10000), and IDIV faults.
    #[error("rock {0:?}'s glow divides by its squared height, zero (00454040)")]
    GlowDivide(ObjectId),
    #[error("rock {rock:?} service at {address:08X}: {error}")]
    Host {
        rock: ObjectId,
        address: u32,
        error: E,
    },
}

/// What the update reaches.
pub trait Selector44Host {
    type Error;
    /// `0047DFD0(delta)`, the status visual, every update.
    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        delta: u32,
    ) -> Result<(), Self::Error>;
}

/// What the death reaches, in order.
pub trait Selector44DeathHost {
    type Error;
    /// World `004887D0(block, &+32, +84, 1)` from the rock's live position.
    fn blast(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        block_address: u32,
        source: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// `0048A8B0`, a particle burst.
    fn burst(&mut self, arena: &mut ObjectArena, burst: ParticleBurst) -> Result<(), Self::Error>;
    /// `+4C(0, resource, 0)`, the shared child factory `0047AAF0`.
    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        rock: ObjectId,
        resource: u32,
    ) -> Result<(), Self::Error>;
}

impl ObjectArena {
    /// Resource 0x18's case: 0x140 bytes, 0047A7A0, then +13C = 1 and
    /// +138 = +139 = 0.
    pub fn allocate_selector44(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector44(Box::new(Selector44State::new()));
        Ok(object)
    }

    pub fn selector44(&self, rock: ObjectId) -> Result<&Selector44State, ObjectError> {
        match &self.actor(rock)?.extension {
            ActorExtension::Selector44(state) => Ok(state),
            _ => Err(ObjectError::NotSelector44(rock)),
        }
    }

    fn selector44_mut(&mut self, rock: ObjectId) -> Result<&mut Selector44State, ObjectError> {
        match &mut self.actor_mut(rock)?.extension {
            ActorExtension::Selector44(state) => Ok(state),
            _ => Err(ObjectError::NotSelector44(rock)),
        }
    }

    /// 00453ED0, the rock's +24. The first update keeps gravity only for a
    /// rock moving up (signed WORD +90 above zero, which sets +139 too);
    /// every update runs 0047B1E0(delta, +13C), then 0047DFD0(delta).
    pub fn tick_selector44<H: Selector44Host>(
        &mut self,
        rock: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector44Error<H::Error>> {
        if self.selector44(rock)?.started_138 == 0 {
            let rising = self.actor(rock)?.words_8e_92[1] as i16 > 0;
            let state = self.selector44_mut(rock)?;
            if rising {
                state.rising_139 = 1;
                state.mask_13c = RISING_MASK;
            } else {
                state.mask_13c = THROWN_MASK;
            }
            state.started_138 = 1;
        }
        let mask = self.selector44(rock)?.mask_13c;
        self.tick_actor_physics(
            rock,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: mask as u8,
                sine_quarter,
            },
        )
        .map_err(|error| Selector44Error::Physics(rock, error))?;
        host.status_visual(self, rock, delta)
            .map_err(|error| Selector44Error::Host {
                rock,
                address: 0x0045_3f22,
                error,
            })
    }

    /// 00453F30, the rock's +AC. Unless flag bit 31 is set, the world radius
    /// effect of both blocks from its position with its own handle (each
    /// read at its call); then the burst at its position, the visuals 0x36
    /// and 0x37 and its disposal (00479FA0).
    pub fn retire_selector44<H: Selector44DeathHost>(
        &mut self,
        rock: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector44Error<H::Error>> {
        self.selector44(rock)?;
        let fail = |address| {
            move |error| Selector44Error::Host {
                rock,
                address,
                error,
            }
        };
        if self.object(rock)?.base.flags & 0x8000_0000 == 0 {
            for (block, address) in ROCK_BLASTS.into_iter().zip([0x0045_3f58, 0x0045_3f72]) {
                let source = self.actor(rock)?.handle;
                host.blast(self, rock, block, source)
                    .map_err(fail(address))?;
            }
        }
        let position = self
            .object(rock)?
            .base
            .position_words
            .ok_or(Selector44Error::Position(rock))?;
        host.burst(self, rock_burst(position))
            .map_err(fail(0x0045_3fca))?;
        for (resource, address) in ROCK_VISUALS.into_iter().zip([0x0045_3fd9, 0x0045_3fe6]) {
            host.visual(self, rock, resource).map_err(fail(address))?;
        }
        self.dispose(rock)?;
        Ok(())
    }

    /// 00454040, the rock's +28: the height above the higher of the ground
    /// cell and the water plane (signed WORDs), plus 0x400 (a zero is 1),
    /// is the radius; each of `glow`'s three low bytes, shifted up 10 and
    /// divided by that radius squared, saturates to a signed byte. The
    /// fourth byte is kept, and a nonzero DWORD lights the ground at the
    /// rock's X/Z (0045B1A0).
    pub fn selector44_glow<E>(
        &self,
        rock: ObjectId,
        terrain: &TerrainRuntime,
        glow: u32,
    ) -> Result<Option<TerrainLightRequest>, Selector44Error<E>> {
        self.selector44(rock)?;
        let [x, y, z] = self
            .object(rock)?
            .base
            .position_words
            .ok_or(Selector44Error::Position(rock))?;
        let ground = terrain.raw_height_at([x, z]);
        let plane = (terrain.plane_base() >> terrain.plane_shift()) as i16;
        let top = i32::from(ground.max(plane));
        let mut radius = i32::from(y as i16).wrapping_sub(top).wrapping_add(0x400);
        if radius == 0 {
            radius = 1;
        }
        let square = radius.wrapping_mul(radius);
        if square == 0 {
            return Err(Selector44Error::GlowDivide(rock));
        }
        let mut bytes = glow.to_le_bytes();
        for byte in &mut bytes[..3] {
            let quotient = (i32::from(*byte as i8) << 10) / square;
            *byte = if quotient == i32::from(quotient as i8) {
                quotient as u8
            } else if quotient > 0 {
                0x7f
            } else {
                0x80
            };
        }
        if u32::from_le_bytes(bytes) == 0 {
            return Ok(None);
        }
        Ok(Some(TerrainLightRequest {
            world_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius,
            rgb: [bytes[0] as i8, bytes[1] as i8, bytes[2] as i8],
        }))
    }
}

#[cfg(test)]
#[path = "selector44_tests.rs"]
mod tests;
