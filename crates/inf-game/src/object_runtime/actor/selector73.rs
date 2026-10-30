//! Selector 73, the ECM Unit: the shared 0047A7A0 with the final table
//! 005262CC (case 0x49, 0x14C bytes, through 00450400, which clears +140).
//! Its table is the shared actor's 00527C00 but for +00 (004057B0, the
//! shared teardown through 00450440), +24 (the update 00450450), +28 (the
//! glow 0042DAE0), +8C (0042D9E0, attach and detach), +90 (0042DA60, the
//! hold) and +AC (0042DA90, the kill).
//!
//! A free unit comes to rest where it lands. Still (where the update last
//! saw it, with no horizontal speed and at most 32 vertical), it adds the
//! delta to its rest +140, and after 500 ms it sets flag 0x4000000 and stops
//! running its physics until something moves it or sets flag 0x400000.
//! Otherwise each update recopies the position, clears the rest and flags
//! 0x4000000, 0x400000 and 0x200000, damps each speed WORD by
//! `speed * (delta >> 5) >> 14` (to zero within 32) and runs 0047B1E0 with
//! gravity and the position step (mask 9).
//!
//! A vehicle's hold takes it (00421810) through the shared capture body
//! 0047D7C0; the unit then names its holder at +144, and every hold tick it
//! sets the holder's flag 0x40000, the ECM: shooters lead the holder the
//! wrong way (00482690) and the vehicle's light pulses (00420080). Released
//! (+8C with operation 0) or killed (+AC), it clears that flag and forgets
//! the holder.

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{
    mark_flag_change, ActorHandle, ActorMotionError, ActorPhysicsRequest, ObjectArena, ObjectError,
    ObjectId,
};
use crate::world_lighting::TerrainLightRequest;

/// 00450615: a still unit settles after resting this long.
const SETTLE_MICROSECONDS: u32 = 500_000;
/// 0045049E, 00450506 and 00450513: a still unit's vertical speed and the
/// damping's dead band.
const STILL_SPEED: i32 = 0x20;
/// 004504D6, 004505ED: moving clears flags 0x4000000, 0x400000, 0x200000.
const MOVING_CLEARS: u32 = 0xfb9f_ffff;
/// 0045062D: a settled unit.
const SETTLED_FLAG: u32 = 0x0400_0000;
/// 004505D9: what wakes a settled unit.
const WAKE_FLAG: u32 = 0x0040_0000;
/// 0047B1E0's mask: gravity and the position step.
const PHYSICS_MASK: u8 = 9;
/// 0042DA6D: the holder's flag while the unit is carried, the ECM.
pub const ECM_FLAG: u32 = 0x0004_0000;
/// 0042DBCC: the glow's radius.
const GLOW_RADIUS: i32 = 0x400;
/// 0042DB40: the glow's channels are the wave's high WORD bits.
const GLOW_SHIFT: u32 = 10;

/// Case 0x49's fields after 0047A7A0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector73State {
    /// +138..+13C, where the update last saw the unit. The constructor
    /// leaves it to allocator residue (Infest03 and Infest04 both held
    /// [-5383, 0, 16000] there), so the first update finds the unit moved
    /// and copies its position, as both recordings' first updates did.
    pub position_138: Option<[u16; 3]>,
    /// +140, how long the unit has rested, in microseconds.
    pub rest_140: u32,
    /// +144, the holder's handle; the case stores the process NULL.
    pub holder_144: ActorHandle,
    /// +148, the glow's phase. Nothing writes it before the glow adds the
    /// world's delta ([`ECM_GLOW_PHASE_APPROXIMATION`]).
    pub phase_148: u32,
}

/// EcmGlowPhaseApproximation: the glow's phase `+148` before its first
/// run. Case 0x49 allocates 0x14C bytes and nothing writes `+148` before
/// `0042DAE0` adds the world's delta, so retail's glow starts from
/// allocator residue (`0x37C0000` in Infest03, `0x21` in Infest04); a fresh
/// heap holds zero. It only shifts where the colour's cycle starts.
pub const ECM_GLOW_PHASE_APPROXIMATION: u32 = 0;

fn log_glow_phase_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("EcmGlowPhaseApproximation: case 0x49 leaves the ECM Unit's glow phase +148 to allocator residue; the port starts it at zero, which only shifts its colour cycle (HOLD_ITEMS.md)");
    }
}

/// `0053DF00`'s quarter wave at `angle`, as `0042DB05..0042DB40` read it:
/// the WORD at the index the second quarter mirrors, negated in the second
/// half, then its low WORD shifted right by `shift`, as a byte.
fn wave_channel(sine: &[u16; 4096], angle: u16, shift: u32) -> i8 {
    let mut index = u32::from(angle) & 0x3ffc;
    if angle & 0x4000 != 0 {
        index = 0x3ffc - index;
    }
    let mut value = i32::from(sine[(index >> 2) as usize]);
    if angle & 0x8000 != 0 {
        value = value.wrapping_neg();
    }
    ((value as i16) >> shift) as i8
}

/// A light's colour from a phase in microseconds, three waves a third of a
/// turn apart at `phase >> 5` (the ECM Unit's glow and the vehicle's pulse).
pub(crate) fn wave_colour(sine: &[u16; 4096], phase: u32, shift: u32) -> [i8; 3] {
    let angle = (phase >> 5) as u16;
    [
        wave_channel(sine, angle, shift),
        wave_channel(sine, angle.wrapping_add(0x5555), shift),
        wave_channel(sine, angle.wrapping_add(0xaaaa), shift),
    ]
}

/// `00450450`'s damping of one speed WORD.
fn damp(speed: u16, step: u32) -> u16 {
    let speed = i32::from(speed as i16);
    if speed == 0 {
        return 0;
    }
    let loss = speed.wrapping_mul(step as i32) >> 14;
    let kept = if speed > 0 {
        speed > STILL_SPEED && speed > loss
    } else {
        speed < -STILL_SPEED && speed < loss
    };
    if kept {
        speed.wrapping_sub(loss) as u16
    } else {
        0
    }
}

impl ObjectArena {
    /// Case 0x49: 0x14C bytes, the shared 0047A7A0, `+140` cleared by
    /// 00450400 and the process NULL at `+144`.
    pub fn allocate_selector73(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let null = self
            .actor_handles()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .null_handle();
        let object = self.allocate_actor(request)?;
        log_glow_phase_approximation();
        self.actor_mut(object)?.extension = ActorExtension::Selector73(Box::new(Selector73State {
            position_138: None,
            rest_140: 0,
            holder_144: null,
            phase_148: ECM_GLOW_PHASE_APPROXIMATION,
        }));
        Ok(object)
    }

    pub fn selector73(&self, object: ObjectId) -> Result<&Selector73State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector73(state) => Ok(state),
            _ => Err(ObjectError::NotSelector73(object)),
        }
    }

    fn selector73_mut(&mut self, object: ObjectId) -> Result<&mut Selector73State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector73(state) => Ok(state),
            _ => Err(ObjectError::NotSelector73(object)),
        }
    }

    /// `00450450`, the unit's +24.
    pub fn tick_selector73(
        &mut self,
        unit: ObjectId,
        delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<(), ActorMotionError> {
        let physics = ActorPhysicsRequest {
            microseconds: delta,
            mask: PHYSICS_MASK,
            sine_quarter,
        };
        let position = self
            .object(unit)?
            .base
            .position_words
            .ok_or(ActorMotionError::MissingPosition(unit))?;
        let moved = self.selector73(unit)?.position_138 != Some(position);
        let [x, y, z] = self.actor(unit)?.words_8e_92;
        if !moved && (x | z) == 0 && i32::from(y as i16).abs() <= STILL_SPEED {
            // 004505D6: still where it was.
            let flags = self.object(unit)?.base.flags;
            if flags & WAKE_FLAG != 0 {
                self.selector73_mut(unit)?.rest_140 = 0;
                mark_flag_change(&mut self.owned_mut(unit)?.state.base, flags & MOVING_CLEARS);
                return self.tick_actor_physics(unit, physics);
            }
            let rest = self.selector73(unit)?.rest_140;
            if rest >= SETTLE_MICROSECONDS {
                return Ok(());
            }
            let rest = rest.wrapping_add(delta);
            self.selector73_mut(unit)?.rest_140 = rest;
            if rest < SETTLE_MICROSECONDS {
                return self.tick_actor_physics(unit, physics);
            }
            mark_flag_change(&mut self.owned_mut(unit)?.state.base, flags | SETTLED_FLAG);
            return Ok(());
        }
        if moved {
            self.selector73_mut(unit)?.position_138 = Some(position);
        }
        if self.selector73(unit)?.rest_140 != 0 {
            self.selector73_mut(unit)?.rest_140 = 0;
            let base = &mut self.owned_mut(unit)?.state.base;
            mark_flag_change(base, base.flags & MOVING_CLEARS);
        }
        let step = delta >> 5;
        let speeds = &mut self.actor_mut(unit)?.words_8e_92;
        *speeds = speeds.map(|speed| damp(speed, step));
        self.tick_actor_physics(unit, physics)
    }

    /// `0042D9E0`'s arm after the shared body `0047D7C0` admitted the
    /// operation: an attach names `source` as the holder.
    pub(in crate::object_runtime) fn attach_selector73(
        &mut self,
        unit: ObjectId,
        source: ActorHandle,
    ) -> Result<(), ObjectError> {
        self.selector73_mut(unit)?.holder_144 = source;
        Ok(())
    }

    /// `0042DA0F..0042DA43` (a detach) and `0042DA93..0042DAC7` (the kill):
    /// the holder `+144` resolves (`0047AD90`), `+144` becomes the process
    /// NULL, and a live holder loses the ECM flag.
    pub(in crate::object_runtime) fn release_selector73_holder(
        &mut self,
        unit: ObjectId,
        null: ActorHandle,
    ) -> Result<(), ActorConstructionError> {
        let holder = self.selector73(unit)?.holder_144;
        let holder = self.resolve_actor_handle(holder)?;
        self.selector73_mut(unit)?.holder_144 = null;
        if let Some(holder) = holder {
            let base = &mut self.owned_mut(holder)?.state.base;
            mark_flag_change(base, base.flags & !ECM_FLAG);
        }
        Ok(())
    }

    /// `0042DA90`, the unit's +AC: the holder loses the ECM flag and `+144`
    /// becomes the process NULL, then the shared disposal `00479FA0`.
    pub fn retire_selector73(&mut self, unit: ObjectId) -> Result<(), ActorConstructionError> {
        let null = self
            .actor_handles()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .null_handle();
        self.release_selector73_holder(unit, null)?;
        self.dispose(unit)?;
        Ok(())
    }

    /// `0042DA60`, the unit's +90 from the hold: the holder takes the ECM
    /// flag, and the unit stays in the hold.
    pub(in crate::object_runtime) fn hold_selector73(
        &mut self,
        holder: ObjectId,
    ) -> Result<bool, ObjectError> {
        let base = &mut self.owned_mut(holder)?.state.base;
        mark_flag_change(base, base.flags | ECM_FLAG);
        Ok(true)
    }

    /// `0042DAE0`, the unit's +28: the phase `+148` gains the world's delta
    /// (`0055F0E0`) and lights the ground at the unit's X/Z within 0x400,
    /// in three waves a third of a turn apart (`>> 10`, so each channel is
    /// -32 to 31). None for another class.
    pub(in crate::object_runtime) fn selector73_glow(
        &mut self,
        unit: ObjectId,
        world_delta: u32,
        sine_quarter: &[u16; 4096],
    ) -> Result<Option<TerrainLightRequest>, ObjectError> {
        if self.selector73(unit).is_err() {
            return Ok(None);
        }
        let state = self.selector73_mut(unit)?;
        state.phase_148 = state.phase_148.wrapping_add(world_delta);
        let phase = state.phase_148;
        let [x, _, z] = self
            .object(unit)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(unit))?;
        Ok(Some(TerrainLightRequest {
            world_xz: [i32::from(x as i16), i32::from(z as i16)],
            radius: GLOW_RADIUS,
            rgb: wave_colour(sine_quarter, phase, GLOW_SHIFT),
        }))
    }
}

#[cfg(test)]
#[path = "selector73_tests.rs"]
mod tests;
