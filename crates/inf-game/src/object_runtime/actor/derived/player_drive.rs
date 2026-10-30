//! What a family actor's player-held +44 adds to its own phases, which the
//! Cranial Implanter's takeover reaches: selector 36's 00481A50 runs its
//! water (00429D60), the block's intents (00428490), its weapon (0048D030;
//! with Fire held and its selected entry ready, 0048D0D0 along its nose),
//! its motion on the intents (+118, 00481C10), its counters (00481180), its
//! engine sound (00481B10) and its status (0047C990). The world sequences
//! those phases; this module owns the intents and the weapon.

use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;

use super::kinematics::ftol;
use super::selector36_motion::table_dwords;
use crate::object_runtime::{
    ActorBehaviorTickError, ActorWeaponFireContext, ActorWeaponFireError, ObjectArena, ObjectError,
    ObjectId, PlayerControlBlock,
};

/// 0042849E..004284DD: Thrust or the pitch drive it full ahead or back.
const FULL_AHEAD: i32 = 0x7_ffff;
const FULL_BACK: i32 = -0x7fff;
/// 004284F8: the analog forward axis's dead zone.
const FORWARD_DEAD_ZONE: i32 = 0x2000;
/// 0042860E: the analog turn's dead zone.
const TURN_DEAD_ZONE: i32 = 0x9c4;
/// 004285E9..004285F0: the digital turn.
const TURN: i32 = 0x1_0000;
/// 00481AB0: a selected entry at this pressure or above does not fire.
const FIRE_PRESSURE: u32 = 0x5b_8d80;

#[derive(Debug, Error)]
pub enum FamilyIntentError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
}

#[derive(Debug, Error)]
pub enum FamilyPlayerWeaponError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Tick(#[from] ActorBehaviorTickError),
    #[error(transparent)]
    Fire(#[from] ActorWeaponFireError),
}

impl ObjectArena {
    /// 00428490 over the player's block (player +4): `[turn, forward]`.
    /// Thrust gives full ahead while positive and full back while
    /// negative; without it, the pitch gives full ahead while negative and
    /// full back while positive; without either, an analog forward axis
    /// (+8) beyond 0x2000 gives the excess times the speed table's maximum
    /// (or minimum, backwards) times the WORD +28, `>> 12`, over 0xE000;
    /// the table is the caller's: the family's +254 (00424C40, 00481A50),
    /// or a scientist's +290 (00467DA0);
    /// otherwise the speed along the nose (+60..+68). The turn is -0x10000
    /// while positive, 0x10000 while negative, or the analog turn axis (+4)
    /// beyond 2500, else 0.
    pub fn family_player_intents(
        &self,
        actor: ObjectId,
        block: &PlayerControlBlock,
        executable: FrontendExecutable<'_>,
    ) -> Result<[i32; 2], FamilyIntentError> {
        let forward = if block.thrust_1c != 0 {
            if block.thrust_1c > 0 {
                FULL_AHEAD
            } else {
                FULL_BACK
            }
        } else if block.pitch_18 != 0 {
            if block.pitch_18 < 0 {
                FULL_AHEAD
            } else {
                FULL_BACK
            }
        } else {
            let axis = block.axes_08[1];
            if block.analog_04 != 0 && axis.wrapping_abs() > FORWARD_DEAD_ZONE {
                let pointer = match self.selector12_actor(actor) {
                    Ok(scientist) => scientist.table_290,
                    Err(_) => self.family4236d0_actor(actor)?.table_254,
                };
                let table = table_dwords(executable, pointer, 2)?;
                let size = i32::from(self.object(actor)?.base.word_28);
                let (beyond, limit) = if axis > 0 {
                    (axis - FORWARD_DEAD_ZONE, size.wrapping_mul(table[0]))
                } else {
                    (-FORWARD_DEAD_ZONE - axis, size.wrapping_mul(table[1]))
                };
                (limit >> 12).wrapping_mul(beyond) / 0xe000
            } else {
                // 0042855D..004285CE: (Z * Z' + Y * Y') + X * X' in x87.
                let state = self.actor(actor)?;
                let row =
                    |index: usize| f64::from(f32::from_bits(state.rotating.basis.words[index]));
                let [x, y, z] = state.words_8e_92.map(|word| f64::from(word as i16));
                ftol((z * row(9) + y * row(8)) + x * row(7))
            }
        };
        let turn = if block.turn_14 != 0 {
            if block.turn_14 > 0 {
                -TURN
            } else {
                TURN
            }
        } else if block.analog_04 != 0 && block.axes_08[0].wrapping_abs() > TURN_DEAD_ZONE {
            block.axes_08[0]
        } else {
            0
        };
        Ok([turn, forward])
    }

    /// 00481A8B..00481AC9: with Fire held and the selected entry of the
    /// weapon behavior (+140) ready (+14 zero, +0 nonzero, +1C below
    /// 6,000,000), what 0048D0D0 queues: the held time and the nose row
    /// (+60), fraction 0.
    pub fn family_player_fire(
        &self,
        actor: ObjectId,
        block: &PlayerControlBlock,
    ) -> Result<Option<(u32, [u32; 3])>, ActorWeaponFireError> {
        if block.fire_20 == 0 {
            return Ok(None);
        }
        let entry = i32::from(self.weapon_behavior(actor)?.byte_10 as i8).wrapping_mul(32);
        if self.weapon_entry(actor, entry.wrapping_add(0x14))? != 0
            || self.weapon_entry(actor, entry)? == 0
            || self.weapon_entry(actor, entry.wrapping_add(0x1c))? >= FIRE_PRESSURE
        {
            return Ok(None);
        }
        let words = self.actor(actor)?.rotating.basis.words;
        Ok(Some((block.fire_20 as u32, [words[7], words[8], words[9]])))
    }

    /// 00481A7B..00481AD1: with a weapon behavior (+140), its tick
    /// (0048D030), then what [`Self::family_player_fire`] decides, queued
    /// (0048D0D0).
    pub fn tick_family_player_weapon(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        block: &PlayerControlBlock,
        context: &mut ActorWeaponFireContext<'_>,
    ) -> Result<(), FamilyPlayerWeaponError> {
        if self.derived_actor(actor)?.behavior_140.is_none() {
            return Ok(());
        }
        self.tick_actor_behavior(actor, microseconds)?;
        if let Some((held, direction)) = self.family_player_fire(actor, block)? {
            self.queue_actor_weapon_direction(actor, held, direction, 0, context)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "player_drive_tests.rs"]
mod tests;
