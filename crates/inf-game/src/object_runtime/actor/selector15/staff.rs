//! The factory's staff: `+140` scientists out of a capacity `+13C`.
//!
//! Evacuate (`0040DCB0`) sends them out one at a time. Each spawns at the
//! factory's `+146` offset turned by its basis (`004909E0`), as H13
//! resource 2 through the world factory (`00460310`), and leaves the staff
//! (`0040D8D0`). One the docked vehicle has room for takes the vehicle's
//! team and position and boards it (`00421810`); another takes the
//! factory's team where it stands, under the slot flash (`00489A90(2)`).
//! An emptied factory turns team 0.
//!
//! A scientist joins through the factory's `+D0` (`0042F170`), from the
//! shop's Beam down or from the pair pass when a scientist walks in. With
//! free staff (`+CC`, `0040E090`), its grants join the factory's
//! (`00475350`) and the factory takes it (`0040DAB0`): the factory takes
//! its team, flashes at it, disposes of it and gains one staff, and a
//! factory it fills records its placement (`0048EAD0` with `+78`). A bound
//! shop menu is then rebuilt (`00431CE0` with force).
//!
//! A smoke grenade's alert (0xD to team 0x12) clears a factory the mutants
//! hold (`0042F3E7..0042F44B`): the staff go and the factory turns team 0,
//! open to any player's vehicle.

use super::factory::{FactoryError, FactoryStaffHost, StaffReleaseHost};
use crate::object_runtime::{
    GrantOwner, LiveGrantOperation, LiveGrantRequest, ObjectArena, ObjectClass, ObjectError,
    ObjectId, PlayerId,
};

/// A remote actor's flag.
const REMOTE: u32 = 0x8000_0000;

/// `0042F429`: H0 string 0x30C, "Factory neutralised".
pub const FACTORY_NEUTRALISED_TEXT: u16 = 0x30c;

impl ObjectArena {
    /// `0042F3E7..0042F44B`, the factory's `+98` for message 0xD on a
    /// local factory (the caller tests `+2C` bit 31). A staffed factory
    /// loses its whole staff (`0040D8D0` with minus the staff) and the first
    /// player (`00488AA0`), if any, reads [`FACTORY_NEUTRALISED_TEXT`];
    /// staffed or not, the factory marks dirty bit 2 and turns team 0.
    /// Returns the player to tell.
    pub fn smoke_out_factory(
        &mut self,
        factory: ObjectId,
    ) -> Result<Option<PlayerId>, ObjectError> {
        let staff = self.selector15(factory)?.intermediate.word_140;
        let mut told = None;
        if staff != 0 {
            self.adjust_factory_staff(factory, staff.wrapping_neg())?;
            told = self.first_player();
        }
        self.owned_mut(factory)?.state.base.dirty_word |= 2;
        self.actor_mut(factory)?.word_b4 = 0;
        Ok(told)
    }

    /// `0040D8D0`: a nonzero change marks `+39` bit 1, and the staff moves
    /// by `delta`, to 0 below 1 and to the capacity above it.
    pub(super) fn adjust_factory_staff(
        &mut self,
        factory: ObjectId,
        delta: i32,
    ) -> Result<(), ObjectError> {
        if delta != 0 {
            self.owned_mut(factory)?.state.base.dirty_word |= 0x200;
        }
        let state = self.family40d800_mut(factory)?;
        let staff = state.word_140.wrapping_add(delta);
        state.word_140 = if staff < 1 {
            0
        } else if staff > state.word_13c {
            state.word_13c
        } else {
            staff
        };
        Ok(())
    }

    /// `00468160`'s target arm against a factory: the scientist offers
    /// itself to the factory's `+D0`, and a refusal sets its `+22E` bit 0.
    pub fn scientist_reaches_factory<H: FactoryStaffHost>(
        &mut self,
        scientist: ObjectId,
        factory: ObjectId,
        host: &mut H,
    ) -> Result<bool, FactoryError<H::Error>> {
        if self.factory_take_scientist(factory, scientist, host)? {
            return Ok(true);
        }
        self.family428650_actor_mut(scientist)?.word_22e |= 1;
        Ok(false)
    }

    /// `0042F170`, the factory's `+D0`, offered `scientist` (a selector-12
    /// actor). Returns whether the factory took it.
    pub fn factory_take_scientist<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        scientist: ObjectId,
        host: &mut H,
    ) -> Result<bool, FactoryError<H::Error>> {
        if self.free_staff(factory, None)? == 0 {
            return Ok(false);
        }
        self.mutate_live_grants(
            LiveGrantRequest {
                target: GrantOwner::Selector15Factory198(factory),
                operation: LiveGrantOperation::Merge {
                    source: GrantOwner::Selector12Actor24c(scientist),
                },
            },
            &host.grants(),
        )?;
        if !self.take_scientist_staff(factory, scientist, host)? {
            return Ok(false);
        }
        let state = self.selector15(factory)?.word_160;
        self.set_factory_state(factory, state, true, host)?;
        Ok(true)
    }

    /// `0040DAB0`, the family's staff take: the factory's (through
    /// `0042F170`) and the terrain bridge's `+D0`. A local actor with free
    /// staff takes a local scientist.
    pub fn take_scientist_staff<H: FactoryStaffHost>(
        &mut self,
        factory: ObjectId,
        scientist: ObjectId,
        host: &mut H,
    ) -> Result<bool, FactoryError<H::Error>> {
        if self.object(factory)?.base.flags & REMOTE != 0
            || self.object(scientist)?.base.flags & REMOTE != 0
            || self.free_staff(factory, None)? == 0
        {
            return Ok(false);
        }
        self.actor_mut(factory)?.word_b4 = self.actor(scientist)?.word_b4;
        self.owned_mut(factory)?.state.base.dirty_word |= 2;
        let position = self
            .object(scientist)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(scientist))?;
        host.flash(self, position)
            .map_err(|error| FactoryError::Host {
                address: 0x0040_daf1,
                error,
            })?;
        self.dispose(scientist)?;
        // 0040D8D0(1).
        if self.family40d800_mut(factory)?.refill(1) {
            self.owned_mut(factory)?.state.base.dirty_word |= 0x200;
        }
        if self.free_staff(factory, None)? == 0 {
            let index = self.actor(factory)?.byte_78 as i8;
            host.complete_placement(index)
                .map_err(|error| FactoryError::Host {
                    address: 0x0040_db28,
                    error,
                })?;
        }
        Ok(true)
    }

    /// `0040DCB0` with `count` and the docked vehicle, if any, for a factory
    /// or the terrain bridge (`00409ED9`). Returns the scientists spawned.
    pub fn evacuate_factory<H: StaffReleaseHost + ?Sized>(
        &mut self,
        factory: ObjectId,
        count: i32,
        vehicle: Option<ObjectId>,
        host: &mut H,
    ) -> Result<u32, FactoryError<H::ReleaseError>> {
        if self.object(factory)?.base.flags & REMOTE != 0 {
            return Ok(0);
        }
        let mut spawned = 0;
        let mut index = 0;
        while index < count {
            let offset =
                self.family40d800(factory)?
                    .words_146_14a
                    .ok_or(FactoryError::Unwritten {
                        factory,
                        field: "+146 spawn offset",
                    })?;
            let local = self
                .actor(factory)?
                .rotating
                .basis
                .transform_words(offset.map(|word| word as i16));
            let origin = self
                .object(factory)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(factory))?;
            let spawn: [u16; 3] =
                std::array::from_fn(|axis| origin[axis].wrapping_add(local[axis]));
            let created =
                host.release_scientist(self, spawn)
                    .map_err(|error| FactoryError::Host {
                        address: 0x0040_dd2e,
                        error,
                    })?;
            if let Some(created) = created {
                // The created object's +18: an actor answers itself.
                let scientist =
                    matches!(self.object(created)?.class, ObjectClass::Actor(_)).then_some(created);
                self.adjust_factory_staff(factory, -1)?;
                if let Some(scientist) = scientist {
                    self.send_out_scientist(factory, scientist, spawn, vehicle, host)?;
                }
                spawned += 1;
            }
            if self.family40d800(factory)?.word_140 == 0 {
                self.owned_mut(factory)?.state.base.dirty_word |= 2;
                self.actor_mut(factory)?.word_b4 = 0;
                return Ok(spawned);
            }
            index += 1;
        }
        Ok(spawned)
    }

    /// `0040DD51..0040DDCB`: a vehicle with a free slot (`004120E0`) takes
    /// the scientist at its own position; otherwise the scientist stays at
    /// `spawn` on the factory's team.
    fn send_out_scientist<H: StaffReleaseHost + ?Sized>(
        &mut self,
        factory: ObjectId,
        scientist: ObjectId,
        spawn: [u16; 3],
        vehicle: Option<ObjectId>,
        host: &mut H,
    ) -> Result<(), FactoryError<H::ReleaseError>> {
        let room = match vehicle {
            Some(vehicle) => {
                let null = host
                    .release_null()
                    .ok_or(FactoryError::MissingProcessNull(factory))?;
                self.selector2_actor(vehicle)?
                    .handles_1a4
                    .handles
                    .contains(&null)
            }
            None => false,
        };
        self.owned_mut(scientist)?.state.base.dirty_word |= 2;
        match vehicle {
            Some(vehicle) if room => {
                let position = self
                    .object(vehicle)?
                    .base
                    .position_words
                    .ok_or(ObjectError::ActorPositionUnknown(vehicle))?;
                self.actor_mut(scientist)?.word_b4 = self.actor(vehicle)?.word_b4;
                self.owned_mut(scientist)?.state.base.position_words = Some(position);
                // 0040DDA9 ignores the take's result.
                host.release_carry(self, vehicle, scientist)
                    .map_err(|error| FactoryError::Host {
                        address: 0x0040_dda9,
                        error,
                    })?;
            }
            _ => {
                self.actor_mut(scientist)?.word_b4 = self.actor(factory)?.word_b4;
                host.release_flash(self, spawn)
                    .map_err(|error| FactoryError::Host {
                        address: 0x0040_ddcb,
                        error,
                    })?;
            }
        }
        Ok(())
    }
}
