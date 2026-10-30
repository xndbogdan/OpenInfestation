//! Selector 17, the Prison Unit: case 0x10 allocates 0x160 bytes and runs
//! 0046E120 with the radius 0x300 over the shared 0047A7A0 (final table
//! 005272A0). It embeds a one-slot hold at +144, the vehicle's hold class
//! (00412880, 004121D0, 004122C0, 004122A0) with byte +0C set: its slot
//! (0052736C) places a held actor at the unit through 00412740 and gives it
//! the unit's basis. The slot's offset is 0054ED4C/50, zero BSS nothing
//! writes, so the turn (0047A660) leaves the unit's own position.
//!
//! The update 0046E230 arms once `+13C` runs out (1 s; 5 s after a reset).
//! Holding someone, it charges `+156` toward 0xFFFF by `delta >> 3`; empty,
//! it lets go of a team's task source it held and drains the charge. Armed,
//! every `(rng & 0xFFFF) * 8 + 1 000 000` microseconds (`+140`) it scans
//! (scanner B 00475F50 within its radius `+138`, filter 0046E450: a
//! person's view of team 0x12, nearest first) and takes the find into its
//! hold with its own handle as source (004122C0). A taken person's team
//! `+B4` becomes `+158` and its `+7A` the kind `+15C`; when the person's
//! view owns grant 0x15 (004752E0) the unit holds it (`+15E`) and names
//! itself that team's task source (00482C60). The hold then runs (00412380).

use thiserror::Error;

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{
    ActorCaptureSavedFlags, ActorHandle, DerivedActorError, FamilyUpdateHost, ObjectArena,
    ObjectError, ObjectId, VehicleDriveError, VehicleHandleArray,
};

/// Case 0x10 pushes the radius 0x300.
pub const PRISON_RADIUS: u32 = 0x300;
/// 0046E141: the first arming delay; 0046E4D2: after a reset.
const ARMING_MICROSECONDS: i32 = 1_000_000;
const RESET_ARMING_MICROSECONDS: i32 = 5_000_000;
/// 0046E2EF: the scan interval's base.
const SCAN_MICROSECONDS: u32 = 1_000_000;
/// 0046E2F6/0046E310: scanner B's required and excluded flags.
const SCAN_REQUIRED: u32 = 0x60;
const SCAN_EXCLUDED: u32 = 0x80;
/// 0046E46A: the filter takes persons of this team.
const PRISONER_TEAM: u32 = 0x12;
/// 0046E378: the grant whose owner the unit holds.
const HOLDING_GRANT: u32 = 0x15;
/// 0046E298: 0055F100 holds nineteen task slots.
const TASK_CLASSES: u32 = 0x13;

/// Case 0x10's fields after 0047A7A0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector17State {
    /// +138, the scan's radius.
    pub radius_138: u32,
    /// +13C, the arming delay in microseconds.
    pub arming_13c: i32,
    /// +140, the time to the next scan.
    pub scan_140: i32,
    /// +144, the one-slot hold, byte +0C (+150) set.
    pub hold_144: VehicleHandleArray,
    /// +154, armed.
    pub armed_154: u8,
    /// +156, the charge.
    pub charge_156: u16,
    /// +158, the taken person's team; unwritten until a take.
    pub team_158: Option<u32>,
    /// +15C, the taken person's +7A.
    pub kind_15c: u16,
    /// +15E, whether the unit names itself its prisoner's task source.
    pub holding_15e: u8,
}

/// What the update reaches beyond the arena: the RNG and the held actors'
/// status timers through the family host, the take (004122C0 on the unit's
/// hold, with its flash) and the team task slots (00482C60).
pub trait PrisonHost: FamilyUpdateHost {
    /// 004122C0 on the unit's hold with its own handle as source: whether
    /// a slot took `person`.
    fn take_prisoner(
        &mut self,
        arena: &mut ObjectArena,
        prison: ObjectId,
        person: ObjectId,
    ) -> Result<bool, Self::Error>;
    /// 00482C60(source, enable) on 0055F100[team].
    fn task_source(
        &mut self,
        team: u32,
        source: ActorHandle,
        enable: bool,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector17Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Hold(#[from] VehicleDriveError<E>),
    #[error("Prison Unit {0:?} needs the process NULL 0054E894")]
    MissingProcessNull(ObjectId),
    #[error("Prison Unit {0:?} reads its unwritten team +158")]
    UnwrittenTeam(ObjectId),
    #[error("Prison Unit {prison:?}'s team {team} has no task slot; 00482C60 dereferences NULL")]
    TaskClass { prison: ObjectId, team: u32 },
    #[error("Prison Unit host: {0}")]
    Host(E),
}

/// 0046E3F5..0046E42B: a holding unit's charge rises by `step` to 0xFFFF.
fn prison_charging(charge: u16, step: u32) -> u16 {
    let raised = u32::from(charge).wrapping_add(step);
    if raised as i32 > 0xffff {
        0xffff
    } else {
        charge.wrapping_add(step as u16)
    }
}

/// 0046E3C1..0046E3D6: an empty unit's charge drains by `step` to zero.
fn prison_draining(charge: u16, step: u32) -> u16 {
    if u32::from(charge) as i32 <= step as i32 {
        0
    } else {
        charge.wrapping_sub(step as u16)
    }
}

/// `0046E450`: a person's view (+5C) of team 0x12 scores `!distance`.
fn prison_score(view_team: Option<u32>, squared: u32) -> u32 {
    match view_team {
        Some(PRISONER_TEAM) => !squared,
        _ => 0,
    }
}

/// `0046E200`, the unit's +14: query 1 is the charge, 2 whether it is
/// armed; any other DWORD comes back as asked.
pub(crate) fn selector17_value(state: &Selector17State, query: u32) -> u16 {
    match query {
        1 => state.charge_156,
        2 => u16::from(state.armed_154),
        _ => query as u16,
    }
}

impl ObjectArena {
    /// Case 0x10: 0x160 bytes, the shared 0047A7A0, then 0046E120(radius):
    /// the hold's slot holds the process NULL, its saved flags and state
    /// unwritten.
    pub fn allocate_selector17(
        &mut self,
        request: ActorConstructionRequest<'_>,
        radius: u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let null = self
            .actor_handles()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .null_handle();
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector17(Box::new(Selector17State {
            radius_138: radius,
            arming_13c: ARMING_MICROSECONDS,
            scan_140: 0,
            hold_144: VehicleHandleArray {
                handles: vec![null],
                saved_flags: vec![ActorCaptureSavedFlags {
                    bytes: Some([None; 4]),
                }],
                states: vec![None],
                byte_0c: 1,
            },
            armed_154: 0,
            charge_156: 0,
            team_158: None,
            kind_15c: 0,
            holding_15e: 0,
        }));
        Ok(object)
    }

    pub fn selector17(&self, object: ObjectId) -> Result<&Selector17State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector17(state) => Ok(state),
            _ => Err(ObjectError::NotSelector17(object)),
        }
    }

    pub(in crate::object_runtime) fn selector17_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector17State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector17(state) => Ok(state),
            _ => Err(ObjectError::NotSelector17(object)),
        }
    }

    /// `0046E230`, the unit's +24.
    pub fn tick_selector17<H: PrisonHost>(
        &mut self,
        prison: ObjectId,
        delta: u32,
        status_load_factor: Option<i32>,
        host: &mut H,
    ) -> Result<(), Selector17Error<H::Error>> {
        let step = delta >> 3;
        let state = self.selector17_mut(prison)?;
        if state.arming_13c > 0 {
            state.arming_13c = state.arming_13c.wrapping_sub(delta as i32);
            if state.arming_13c <= 0 {
                state.arming_13c = 0;
                state.armed_154 = 1;
            }
        }
        let null = host
            .process_null_handle()
            .ok_or(Selector17Error::MissingProcessNull(prison))?;
        if self
            .selector17(prison)?
            .hold_144
            .handles
            .iter()
            .all(|&handle| handle != null)
        {
            let state = self.selector17_mut(prison)?;
            state.charge_156 = prison_charging(state.charge_156, step);
            self.tick_hold(prison, delta, null, status_load_factor, None, host)?;
            return Ok(());
        }
        let own = self.actor(prison)?.handle;
        if self.selector17(prison)?.holding_15e != 0 {
            self.selector17_mut(prison)?.holding_15e = 0;
            self.prison_task_source(prison, own, false, host)?;
        }
        if self.selector17(prison)?.armed_154 != 0 {
            let state = self.selector17_mut(prison)?;
            state.scan_140 = state.scan_140.wrapping_sub(delta as i32);
            if state.scan_140 <= 0 {
                let draw = host.random().map_err(Selector17Error::Host)?;
                self.selector17_mut(prison)?.scan_140 =
                    ((draw & 0xffff) * 8).wrapping_add(SCAN_MICROSECONDS) as i32;
                if let Some(person) = self.prison_scan(prison)? {
                    self.take_prisoner(prison, person, own, host)?;
                }
            }
        }
        let state = self.selector17_mut(prison)?;
        state.charge_156 = prison_draining(state.charge_156, step);
        self.tick_hold(prison, delta, null, status_load_factor, None, host)?;
        Ok(())
    }

    /// `0046E344..0046E3B8`: the take; a taken person's team and `+7A`, and
    /// with grant 0x15 on its view the unit holds it as the team's task
    /// source.
    fn take_prisoner<H: PrisonHost>(
        &mut self,
        prison: ObjectId,
        person: ObjectId,
        own: ActorHandle,
        host: &mut H,
    ) -> Result<(), Selector17Error<H::Error>> {
        if !host
            .take_prisoner(self, prison, person)
            .map_err(Selector17Error::Host)?
        {
            return Ok(());
        }
        let team = self.actor(person)?.word_b4;
        self.selector17_mut(prison)?.team_158 = Some(team);
        let Some(view) = self.person_view(person)? else {
            return Ok(());
        };
        let kind = self.actor(person)?.word_7a;
        self.selector17_mut(prison)?.kind_15c = kind;
        // 004752E0(0x15) on the view's grants +24C: bit 0x15 of its mask.
        let mask = self.selector12_actor(view)?.grants_24c.mask_04[0];
        if mask & (1 << HOLDING_GRANT) != 0 {
            self.selector17_mut(prison)?.holding_15e = 1;
            self.prison_task_source(prison, own, true, host)?;
        }
        Ok(())
    }

    /// 00482C60 on 0055F100[+158]; a team past the nineteen slots is the
    /// NULL retail dereferences.
    fn prison_task_source<H: PrisonHost>(
        &mut self,
        prison: ObjectId,
        source: ActorHandle,
        enable: bool,
        host: &mut H,
    ) -> Result<(), Selector17Error<H::Error>> {
        let team = self
            .selector17(prison)?
            .team_158
            .ok_or(Selector17Error::UnwrittenTeam(prison))?;
        if team >= TASK_CLASSES {
            return Err(Selector17Error::TaskClass { prison, team });
        }
        host.task_source(team, source, enable)
            .map_err(Selector17Error::Host)
    }

    /// Scanner B `00475F50` from the unit within its radius with `0046E450`.
    fn prison_scan(&self, prison: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        let radius = i32::from(self.selector17(prison)?.radius_138 as u16 as i16);
        let range_squared = radius.wrapping_mul(radius) as u32;
        let origin = self
            .object(prison)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(prison))?;
        let mut best: Option<(ObjectId, u32)> = None;
        for &candidate in self.actor_list() {
            let flags = self.object(candidate)?.base.flags;
            if flags & SCAN_EXCLUDED != 0 || flags & SCAN_REQUIRED != SCAN_REQUIRED {
                continue;
            }
            let other = self
                .object(candidate)?
                .base
                .position_words
                .ok_or(ObjectError::ActorPositionUnknown(candidate))?;
            let dx = i32::from(origin[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(origin[2].wrapping_sub(other[2]) as i16);
            let squared = dz.wrapping_mul(dz).wrapping_add(dx.wrapping_mul(dx)) as u32;
            if squared >= range_squared || candidate == prison {
                continue;
            }
            let team = match self.person_view(candidate)? {
                Some(view) => Some(self.actor(view)?.word_b4),
                None => None,
            };
            let score = prison_score(team, squared);
            if score > best.map_or(0, |(_, best)| best) {
                best = Some((candidate, score));
            }
        }
        Ok(best.map(|(actor, _)| actor))
    }

    /// `0046E4A0`, the unit's +98: message 1 drops the hold's first slot
    /// (00412880), disarms it, forgets the kind and the holding, and arms
    /// it again after 5 s; it answers 0. A never-filled slot's drop reads
    /// its unwritten state but has nothing to drop; a held prisoner's drop
    /// is not ported on this path.
    pub(in crate::object_runtime) fn selector17_message(
        &mut self,
        prison: ObjectId,
        code: i32,
    ) -> Result<(), ObjectError> {
        if code != 1 {
            return Ok(());
        }
        let null = self
            .actor_handles()
            .ok_or(ObjectError::MissingActorRegistry)?
            .null_handle();
        if self.selector17(prison)?.hold_144.handles[0] != null {
            return Err(ObjectError::PrisonerDrop(prison));
        }
        let state = self.selector17_mut(prison)?;
        state.armed_154 = 0;
        state.kind_15c = 0;
        state.holding_15e = 0;
        state.arming_13c = RESET_ARMING_MICROSECONDS;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector17_tests.rs"]
mod tests;
