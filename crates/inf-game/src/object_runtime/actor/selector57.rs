//! Selector 57, the Powerball: the shared 0047A7A0 with the final table
//! 00526464 (case 0x39, 0x138 bytes, no fields of its own). Its table is the
//! shared actor's 00527C00 but for +00 (004110E0, the same 0047AA60
//! teardown), +24 (the update 004863F0), +98 (004864B0) and +AC (its death
//! 00486420).
//!
//! It rolls under the shared physics and keeps its health full. Message 5
//! retires it, and its sender too when that is resource 0xE. Its death
//! builds the visual 0x36 through its +4C, bursts (0048A8B0), calls the
//! actors within 2000 with message 0xC and disposes of it.

use thiserror::Error;

use super::{
    ActorConstructionError, ActorConstructionRequest, ActorExtension, ActorMotionError,
    ActorPhysicsRequest, FamilyUpdateError, FamilyUpdateHost,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::process_particles::births::ParticleBurst;

/// 004863F6: gravity, linear drag and position integration.
const PHYSICS_MASK: u8 = 0xd;

/// 004864B0's one arm: message 5.
pub const POWERBALL_RETIRE_MESSAGE: i32 = 5;

/// 004864C0: the sender's resource word that message 5 retires too.
pub const POWERBALL_PARTNER_RESOURCE: u16 = 0xe;

/// 00486428: the death's visual, through +4C.
pub const POWERBALL_VISUAL: u32 = 0x36;

/// 00486484..0048648B: the death calls every actor within 2000 with
/// message 0xC and argument 2000 (0047DD20).
pub const POWERBALL_CALL_RADIUS: u32 = 2000;
pub const POWERBALL_CALL_MESSAGE: i32 = 0xc;
pub const POWERBALL_CALL_ARGUMENT: u32 = 2000;

/// 00486455..0048647B: sixteen type-1 debris round a type-1 centre within
/// 20, speed 8, no flags.
fn powerball_burst(position: [u16; 3]) -> ParticleBurst {
    ParticleBurst {
        debris_count: 0x10,
        position,
        placement_radius: 0x14,
        central_type: 1,
        debris_types: [1, 1],
        speed: 8,
        flags: 0,
    }
}

/// What the death reaches, in order.
pub trait Selector57DeathHost {
    type Error;
    /// `+4C(0, resource, 0)`, the shared child factory `0047AAF0`.
    fn visual(
        &mut self,
        arena: &mut ObjectArena,
        ball: ObjectId,
        resource: u32,
    ) -> Result<(), Self::Error>;
    /// `0048A8B0`, a particle burst.
    fn burst(&mut self, arena: &mut ObjectArena, burst: ParticleBurst) -> Result<(), Self::Error>;
    /// `0047DD20(radius, code, argument)` from the ball: every actor within
    /// the radius of its live position.
    fn call(
        &mut self,
        arena: &mut ObjectArena,
        ball: ObjectId,
        radius: u32,
        code: i32,
        argument: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector57Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("Powerball {0:?} physics 0047B1E0")]
    Physics(ObjectId, #[source] ActorMotionError),
    #[error("Powerball {0:?} copies its unwritten maximum health +A2")]
    UnwrittenMaximum(ObjectId),
    #[error(transparent)]
    Status(FamilyUpdateError<E>),
    #[error("Powerball {0:?} has no position")]
    Position(ObjectId),
    #[error("Powerball {ball:?} service at {address:08X}: {error}")]
    Host {
        ball: ObjectId,
        address: u32,
        error: E,
    },
}

impl<E> From<FamilyUpdateError<E>> for Selector57Error<E> {
    fn from(error: FamilyUpdateError<E>) -> Self {
        Self::Status(error)
    }
}

impl ObjectArena {
    /// Case 0x39: 0x138 bytes, the shared 0047A7A0 and the final table.
    pub fn allocate_selector57(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector57;
        Ok(object)
    }

    pub fn selector57(&self, object: ObjectId) -> Result<(), ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector57 => Ok(()),
            _ => Err(ObjectError::NotSelector57(object)),
        }
    }

    /// 004863F0, the Powerball's +24: the shared physics 0047B1E0(delta,
    /// 0xD), its health set to its maximum (+A0 = +A2), then the shared
    /// status 0047C990.
    pub fn tick_selector57<H: FamilyUpdateHost>(
        &mut self,
        ball: ObjectId,
        delta: u32,
        status_load_factor: Option<i32>,
        sine_quarter: &[u16; 4096],
        host: &mut H,
    ) -> Result<(), Selector57Error<H::Error>> {
        self.selector57(ball)?;
        self.tick_actor_physics(
            ball,
            ActorPhysicsRequest {
                microseconds: delta,
                mask: PHYSICS_MASK,
                sine_quarter,
            },
        )
        .map_err(|error| Selector57Error::Physics(ball, error))?;
        let actor = self.actor_mut(ball)?;
        actor.health = actor
            .maximum_health
            .ok_or(Selector57Error::UnwrittenMaximum(ball))?;
        self.tick_actor_status(ball, delta, status_load_factor, host)?;
        Ok(())
    }

    /// 00486420, the Powerball's +AC: the visual 0x36 through +4C, the
    /// burst at its position (read after the visual), message 0xC with
    /// argument 2000 to every actor within 2000 (0047DD20), then its
    /// disposal (00479FA0). No flag gates any of it.
    pub fn retire_selector57<H: Selector57DeathHost>(
        &mut self,
        ball: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector57Error<H::Error>> {
        self.selector57(ball)?;
        let fail = |address| {
            move |error| Selector57Error::Host {
                ball,
                address,
                error,
            }
        };
        host.visual(self, ball, POWERBALL_VISUAL)
            .map_err(fail(0x0048_642e))?;
        let position = self
            .object(ball)?
            .base
            .position_words
            .ok_or(Selector57Error::Position(ball))?;
        host.burst(self, powerball_burst(position))
            .map_err(fail(0x0048_647f))?;
        host.call(
            self,
            ball,
            POWERBALL_CALL_RADIUS,
            POWERBALL_CALL_MESSAGE,
            POWERBALL_CALL_ARGUMENT,
        )
        .map_err(fail(0x0048_6492))?;
        self.dispose(ball)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector57_tests.rs"]
mod tests;
