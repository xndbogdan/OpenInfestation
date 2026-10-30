//! Pickups: placed selector 14 (0046D840/0046D7D0/0046D950, final 00527208)
//! and timed selector 89 (0046E060/0046E0C0/0046E0D0, final 00527254). Both
//! share 0046D780 commands, 0046D500/0046D870/0046DAF0 policies and the
//! 0046E080 destructor. The shared visual remains a nonactor.

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, VisualObject};
use crate::ResidentRuntime;

mod birth;
mod draw;
#[cfg(test)]
mod tests;
pub use birth::{PickupBirthHost, PickupBirthRequest};
pub use draw::{PickupCollectionEffect, PickupDrawHost, PickupDrawRequest, PickupNetwork};

#[derive(Debug, thiserror::Error)]
pub enum PickupError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("pickup {object:?} requires field +{offset:X}")]
    RequiredField { object: ObjectId, offset: u16 },
    #[error("0046D870 requires current process NULL handle 0054E894")]
    MissingProcessNull,
    #[error(
        "0046DAF0's network game needs the host flag, settings +20 and the world's pickup words"
    )]
    Network,
    #[error("pickup {0:?} previously stopped after a reached draw prefix")]
    PreviouslyStopped(ObjectId),
    #[error("pickup service {address:08X} failed: {error}")]
    Service { address: u32, error: E },
}

/// The two final pickup tables differ only at +1C and +24.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickupClass {
    /// Selector 14, placed by a world: +1C 0046D7D0 assigns the world
    /// pickup serial; +24 0046D950 is the network respawn update.
    Placed,
    /// Selector 89, born from a dying actor: +1C 0046E0C0 enters the update
    /// list; +24 0046E0D0 counts its 10 s life down.
    Timed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimedPickupState {
    pub class: PickupClass,
    /// Portable no-retry custody; not a native payload byte. Collection,
    /// particle births and renderer publication may precede a failed suffix.
    pub draw_stopped: bool,
    pub visual: VisualObject,
    ///46D840 leaves +44 unwritten;46D500 publishes the actual low type WORD.
    pub kind_44: Option<u16>,
    pub value_46: u16,
    pub save_index_48: u32,
    pub clock_50: u32,
    pub pending_54: u8,
    /// Selector 89's 0046E060 writes +58 = 10 s; 0046D840 leaves it unwritten.
    pub remaining_58: Option<u32>,
}

impl TimedPickupState {
    pub(super) fn new(class: PickupClass) -> Self {
        Self {
            class,
            draw_stopped: false,
            visual: VisualObject {
                radius_word: 0,
                model_word: u16::MAX,
            },
            kind_44: None,
            value_46: 1,
            save_index_48: u32::MAX,
            clock_50: 0,
            pending_54: 0,
            remaining_58: (class == PickupClass::Timed).then_some(10_000_000),
        }
    }
}

impl ObjectArena {
    pub fn timed_pickup(&self, object: ObjectId) -> Result<&TimedPickupState, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::TimedPickup(state) => Ok(state),
            _ => Err(ObjectError::NotTimedPickup(object)),
        }
    }

    pub(in crate::object_runtime) fn timed_pickup_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut TimedPickupState, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::TimedPickup(state) => Ok(state),
            _ => Err(ObjectError::NotTimedPickup(object)),
        }
    }

    ///0046E0D0 keeps low32 subtraction, then tests its sign. Exactly zero
    ///survives until the next positive delta; disposal owns list publication.
    pub fn tick_timed_pickup(
        &mut self,
        object: ObjectId,
        microseconds: u32,
    ) -> Result<(), ObjectError> {
        let state = self.timed_pickup_mut(object)?;
        if state.class == PickupClass::Placed {
            // Offline 0046D7D0 leaves a placed pickup off the update list;
            // the network host's runs 0046D950 (`tick_placed_pickup`).
            return Err(ObjectError::PlacedPickupNetworkUpdate(object));
        }
        let remaining = state
            .remaining_58
            .expect("0046E060 writes the timed pickup's +58")
            .wrapping_sub(microseconds);
        state.remaining_58 = Some(remaining);
        if (remaining as i32) < 0 {
            self.dispose(object)?;
        }
        Ok(())
    }

    /// 0046D950, a placed pickup's +24, which only the network game's host
    /// lists. While its clock +50 is 0, a serial the mission records as
    /// collected (0048E930) sets +50 to the clock plus 30 (200 for any kind
    /// but 0x77), flashes, sounds twice and, without respawning power-ups
    /// (settings +20), disposes it. Once the clock passes +50 the bit clears
    /// (0048E9C0), it flashes and sounds again and +50 returns to 0. The
    /// effects go to `effect` in order.
    pub fn tick_placed_pickup(
        &mut self,
        object: ObjectId,
        clock: u32,
        respawning: bool,
        mission: &mut crate::world_mission::WorldMission,
        effect: &mut dyn FnMut(
            &mut ObjectArena,
            ObjectId,
            PickupCollectionEffect,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        let state = self.timed_pickup(object).map_err(|e| e.to_string())?;
        let serial = state.save_index_48;
        let kind = state
            .kind_44
            .ok_or_else(|| format!("{object:?}: 0046D950 reads the unwritten kind +44"))?;
        let word = (mission.current_world_a0 as usize)
            .wrapping_mul(4)
            .wrapping_add(serial as usize / 32);
        let bit = 1u32.wrapping_shl(serial & 31);
        let mut flash = |arena: &mut ObjectArena| -> Result<(), String> {
            effect(arena, object, PickupCollectionEffect::Particle)?;
            for add in [0, 2] {
                let pitch = ((u32::from(kind) % 6 + add) << 16) >> 3;
                effect(
                    arena,
                    object,
                    PickupCollectionEffect::Sound {
                        sound: 2,
                        volume: 0x1_0000,
                        pitch,
                    },
                )?;
            }
            Ok(())
        };
        if state.clock_50 == 0 {
            let collected = *mission.b0_span_word_mut(word).map_err(|e| e.to_string())? & bit != 0;
            if collected {
                let wait = if kind != 0x77 { 0xaa } else { 0 } + 0x1e;
                self.timed_pickup_mut(object)
                    .map_err(|e| e.to_string())?
                    .clock_50 = clock.wrapping_add(wait);
                flash(self)?;
                if !respawning {
                    self.dispose(object).map_err(|e| e.to_string())?;
                }
            }
        } else if (state.clock_50 as i32) < clock as i32 {
            *mission.b0_span_word_mut(word).map_err(|e| e.to_string())? &= !bit;
            flash(self)?;
            self.timed_pickup_mut(object)
                .map_err(|e| e.to_string())?
                .clock_50 = 0;
        }
        Ok(())
    }

    /// 0046D7D0, placed +1C after construction commands. A placement word of
    /// -1 does nothing. Otherwise +48 takes the world pickup serial 0054ED48
    /// (post-increment); a serial the mission records as collected
    /// (0048E930 over W+B0) disposes through 00479FA0. The network game's
    /// host puts the pickup on the update list (00479F20(1)), where its
    /// respawn runs (0046D950); elsewhere 00479F20(0) keeps it off, and it
    /// collects only when drawn.
    pub(crate) fn activate_placed_pickup(
        &mut self,
        object: ObjectId,
        network_active: bool,
        host: bool,
    ) -> Result<(), ObjectError> {
        if self.object(object)?.base.placement_word as i16 == -1 {
            return Ok(());
        }
        let serial = self.world_load.pickup_serial_54ed48;
        self.world_load.pickup_serial_54ed48 = serial.wrapping_add(1);
        self.timed_pickup_mut(object)?.save_index_48 = serial;
        if self.world_load.pickup_collected(object, serial)? {
            return self.dispose(object);
        }
        self.set_flag1(object, network_active && host)
    }

    ///0046D500 writes type before selection, and writes model/dirty before
    ///metadata refresh. Unknown selectors return the actual network flag.
    pub fn select_pickup_kind(
        &mut self,
        object: ObjectId,
        kind: u16,
        network_active: bool,
        residency: &ResidentRuntime,
    ) -> Result<bool, ObjectError> {
        let Some(model) = self.write_pickup_kind(object, kind)? else {
            return Ok(network_active);
        };
        self.timed_pickup_mut(object)?.visual.model_word = model;
        self.refresh_visual_model(object, residency)?;
        Ok(true)
    }

    fn write_pickup_kind(
        &mut self,
        object: ObjectId,
        kind: u16,
    ) -> Result<Option<u16>, ObjectError> {
        self.timed_pickup_mut(object)?.kind_44 = Some(kind);
        let model = match kind {
            3 | 0x77 => 0x107,
            4 | 0x83 => 0xa8,
            7 => 0x3b,
            0x0e => 0xfb,
            0x0f => 0xfa,
            0x10 => 0xf9,
            0x1a => 0x271,
            0x1c => 0xaa,
            0x1d => 3,
            0x25 => 0x51,
            0xa8 => 0x53,
            0xa9 => 0x50,
            0xaa => 0x54,
            0xab => 0x56,
            0xad => 0x52,
            _ => return Ok(None),
        };
        if matches!(kind, 4 | 0x1a) {
            let position = self
                .owned_mut(object)?
                .state
                .base
                .position_words
                .as_mut()
                .ok_or(ObjectError::ActorPositionUnknown(object))?;
            position[1] = position[1].wrapping_add(100);
        }
        self.owned_mut(object)?.state.base.dirty_word |= 0x100;
        self.timed_pickup_mut(object)?.visual.model_word = model;
        Ok(Some(model))
    }
}
