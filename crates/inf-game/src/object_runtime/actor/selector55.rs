//! Selector 55, the Remote Beamer: case 0x36 allocates 0x148 bytes and runs
//! 00473D10 with the range 0x200 over the shared 0047A7A0 (final table
//! 005274C4). Its table is the shared actor's but for +00 (00473FE0, the
//! shared teardown through 00474000), +04 (00473F70: code 16 sets the range),
//! +14 (00473D50: query 1 is the charge), +24 (the update 00473D70) and +8C
//! (00473FB0: an attach empties the charge).
//!
//! The update charges `+138` by the delta to 0x6000. Charged, it forgets its
//! receiver and scans the actor list (scanner B 00475F50, filter 00473EE0):
//! the nearest receiver (resource 0x25) becomes `+13C` at distance `+140`
//! and sets flag 0x10000000, and the nearest person (a +5C view) within the
//! range, on the beamer's own identity `+6C` and not of team 0x12, is the
//! find. With a live receiver the person is beamed: the charge empties, the
//! person's view joins team 2 (00468C40) with `+22E` bits 0xC, a flash
//! (00489A90(2)) marks where it stood, it takes the receiver's position, a
//! second flash marks its arrival, and it learns grants 0x6D and 0x6E.
//!
//! Selector 56, the receiver, is 00526530: the shared table but for its
//! destructor (004110E0, the shared body through 004854B0).

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension};
use crate::object_runtime::{
    mark_flag_change, ActorHandle, GrantOwner, LiveGrantContext, LiveGrantOperation,
    LiveGrantRequest, ObjectArena, ObjectError, ObjectId, VehicleCarryContext, VehicleCarryEffect,
    VehicleCarryError, VehicleCarryHost,
};

/// Case 0x36 pushes the range 0x200; 00473D10 keeps its square at `+144`.
pub const BEAMER_RANGE: u16 = 0x200;
/// 00473D97: a full charge.
pub const BEAMER_CHARGE: u16 = 0x6000;
/// 00473DE3..00473DF3: scanner B's required and excluded flags and its range
/// squared.
const SCAN_REQUIRED: u32 = 0x60;
const SCAN_EXCLUDED: u32 = 0x80;
const SCAN_RANGE_SQUARED: u32 = 0x3fff_0001;
/// 00473EEC: the receiver's resource.
pub const RECEIVER_RESOURCE: u16 = 0x25;
/// 00473F21: the flag a found receiver sets.
const RECEIVER_FLAG: u32 = 0x1000_0000;
/// 00473F5B: persons of this team are never beamed.
const ALIEN_TEAM: u32 = 0x12;
/// 00473E56/00473E5D: the beamed person's view.
const BEAMED_BITS_22E: u16 = 0xc;
const BEAMED_TEAM: u32 = 2;
/// 00473E6D: the flash's particle selector.
const FLASH_SELECTOR: u32 = 2;
/// 00473E9E/00473EA7: what a beamed person learns.
const BEAMED_GRANTS: [u32; 2] = [0x6d, 0x6e];

/// Case 0x36's fields after 0047A7A0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector55State {
    /// +138, the charge WORD; 00473D10 zeroes it.
    pub charge_138: u16,
    /// +13C, the nearest receiver's handle; unwritten until a full charge.
    pub receiver_13c: Option<ActorHandle>,
    /// +140, that receiver's distance squared; unwritten until then.
    pub distance_140: Option<u32>,
    /// +144, the range squared.
    pub range_144: u32,
}

/// 00473D70..00473D94: `((delta >> 10) * 0x6000) / 20000` through the
/// multiply-high 0xD1B71759 and `>> 12`, the product wrapping as a DWORD.
fn beamer_charge_step(delta: u32) -> u32 {
    let scaled = ((delta >> 10).wrapping_mul(3)) << 13;
    ((u64::from(scaled) * 0xd1b7_1759) >> 44) as u32
}

/// 00473D9C..00473DAD: the charge after `delta` while it stays below
/// `0x6000 - step` (signed), else None: a full charge.
fn beamer_charged(charge: u16, delta: u32) -> Option<u16> {
    let step = beamer_charge_step(delta);
    ((u32::from(charge) as i32) < (u32::from(BEAMER_CHARGE).wrapping_sub(step) as i32))
        .then(|| charge.wrapping_add(step as u16))
}

/// A candidate as `00473EE0` reads it: its resource WORD `+30`, its handle
/// `+84` and its +5C view's identity `+6C` and team `+B4`.
#[derive(Clone, Copy, Debug)]
struct BeamerCandidate {
    resource_30: u16,
    handle: ActorHandle,
    view: Option<(u32, u32)>,
}

/// `00473EE0`: a receiver nearer than the last (or the first) becomes the
/// beamer's target (true) and scores nothing; any other candidate, a
/// receiver included, scores `!distance` when its view lies within the
/// range on the beamer's identity and is not of team 0x12.
fn beamer_score(
    state: &mut Selector55State,
    identity_6c: u32,
    candidate: BeamerCandidate,
    squared: u32,
) -> (u32, bool) {
    // 00473DC6 writes +140 before every scan.
    let best = state.distance_140.unwrap_or(0);
    if candidate.resource_30 == RECEIVER_RESOURCE && (squared < best || best == 0) {
        state.distance_140 = Some(squared);
        state.receiver_13c = Some(candidate.handle);
        return (0, true);
    }
    match candidate.view {
        Some((view_6c, team))
            if squared < state.range_144 && view_6c == identity_6c && team != ALIEN_TEAM =>
        {
            (!squared, false)
        }
        _ => (0, false),
    }
}

impl ObjectArena {
    /// Case 0x36: 0x148 bytes, the shared 0047A7A0, then 00473D10(range).
    pub fn allocate_selector55(
        &mut self,
        request: ActorConstructionRequest<'_>,
        range: u16,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector55(Box::new(Selector55State {
            charge_138: 0,
            receiver_13c: None,
            distance_140: None,
            range_144: u32::from(range) * u32::from(range),
        }));
        Ok(object)
    }

    pub fn selector55(&self, object: ObjectId) -> Result<&Selector55State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector55(state) => Ok(state),
            _ => Err(ObjectError::NotSelector55(object)),
        }
    }

    pub(in crate::object_runtime) fn selector55_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector55State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector55(state) => Ok(state),
            _ => Err(ObjectError::NotSelector55(object)),
        }
    }

    /// Case 0x37: 0x138 bytes, the shared 0047A7A0 and the receiver's table.
    pub fn allocate_selector56(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector56;
        Ok(object)
    }

    pub fn is_selector56(&self, object: ObjectId) -> bool {
        self.actor(object)
            .is_ok_and(|actor| matches!(actor.extension, ActorExtension::Selector56))
    }

    /// Virtual +5C over the actor list: selector 12's table answers itself
    /// (00404610); every other actor table the port builds binds the shared
    /// NULL (00401300).
    pub(in crate::object_runtime) fn person_view(
        &self,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, ObjectError> {
        self.actor(actor)?;
        Ok(self.selector12_actor(actor).is_ok().then_some(actor))
    }

    /// `00473D70`, the beamer's +24.
    pub fn tick_selector55<H: VehicleCarryHost>(
        &mut self,
        beamer: ObjectId,
        delta: u32,
        context: VehicleCarryContext<'_>,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let charge = self.selector55(beamer)?.charge_138;
        if let Some(charge) = beamer_charged(charge, delta) {
            self.selector55_mut(beamer)?.charge_138 = charge;
            return Ok(());
        }
        let null = context
            .process
            .process_null_handle
            .ok_or(VehicleCarryError::MissingProcessNullHandle)?;
        let state = self.selector55_mut(beamer)?;
        state.charge_138 = BEAMER_CHARGE;
        state.distance_140 = Some(0);
        state.receiver_13c = Some(null);
        let Some(person) = self.beamer_scan(beamer)? else {
            return Ok(());
        };
        let receiver = self.selector55(beamer)?.receiver_13c.unwrap_or(null);
        let Some(receiver) = self.resolve_actor_handle(receiver)? else {
            // 00473EBA: no receiver clears the flag (outside the dirty bits).
            let base = &mut self.owned_mut(beamer)?.state.base;
            mark_flag_change(base, base.flags & !RECEIVER_FLAG);
            return Ok(());
        };
        let Some(view) = self.person_view(person)? else {
            return Ok(());
        };
        self.selector55_mut(beamer)?.charge_138 = 0;
        self.family428650_actor_mut(view)?.word_22e |= BEAMED_BITS_22E;
        self.join_vehicle_team(view, BEAMED_TEAM, context)?;
        self.beamer_flash(person, host, rng)?;
        let arrival = self
            .object(receiver)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(receiver))?;
        self.owned_mut(person)?.state.base.position_words = Some(arrival);
        self.beamer_flash(person, host, rng)?;
        for id in BEAMED_GRANTS {
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Selector12Actor24c(view),
                    operation: LiveGrantOperation::SetRaw { id },
                },
                &LiveGrantContext {
                    process: Some(context.process),
                    executable: Some(context.executable),
                    resident: Some(context.resident),
                    text_data: context.text_data,
                },
            )?;
        }
        Ok(())
    }

    /// `00489A90(2, person +32, 0)`.
    fn beamer_flash<H: VehicleCarryHost>(
        &mut self,
        person: ObjectId,
        host: &mut H,
        rng: &mut u32,
    ) -> Result<(), VehicleCarryError<H::Error>> {
        let position = self
            .object(person)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(person))?;
        host.effect(
            self,
            VehicleCarryEffect::Particle {
                selector: FLASH_SELECTOR,
                position,
                flags: 0,
            },
            rng,
        )
        .map_err(VehicleCarryError::Host)
    }

    /// Scanner B `00475F50` from the beamer with `00473EE0`: actors with
    /// flags 0x60 and without 0x80, nearer than the scan's range in X/Z
    /// (squared, unsigned), other than the beamer, in the actor list's
    /// order; the strictly best score wins.
    fn beamer_scan(&mut self, beamer: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        let origin = self
            .object(beamer)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(beamer))?;
        let mut best: Option<(ObjectId, u32)> = None;
        for candidate in self.actor_list().to_vec() {
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
            if squared >= SCAN_RANGE_SQUARED || candidate == beamer {
                continue;
            }
            let score = self.beamer_filter(beamer, candidate, squared)?;
            if score > best.map_or(0, |(_, best)| best) {
                best = Some((candidate, score));
            }
        }
        Ok(best.map(|(actor, _)| actor))
    }

    /// `00473EE0` on the arena: a view within the range needs both
    /// identities known.
    fn beamer_filter(
        &mut self,
        beamer: ObjectId,
        candidate: ObjectId,
        squared: u32,
    ) -> Result<u32, ObjectError> {
        let resource_30 = self.object(candidate)?.base.resource_word;
        let range = self.selector55(beamer)?.range_144;
        let view = match self.person_view(candidate)? {
            Some(view) => {
                let (view_state, beamer_state) = (self.actor(view)?, self.actor(beamer)?);
                // 00473F51 reads the identities only within the range.
                if squared < range && view_state.unknown_6c | beamer_state.unknown_6c != 0 {
                    return Err(ObjectError::UnknownIdentity6c {
                        actor: view,
                        other: beamer,
                    });
                }
                Some((view_state.word_6c, view_state.word_b4))
            }
            None => None,
        };
        let candidate = BeamerCandidate {
            resource_30,
            handle: self.actor(candidate)?.handle,
            view,
        };
        let identity = self.actor(beamer)?.word_6c;
        let (score, receiver) =
            beamer_score(self.selector55_mut(beamer)?, identity, candidate, squared);
        if receiver {
            let base = &mut self.owned_mut(beamer)?.state.base;
            mark_flag_change(base, base.flags | RECEIVER_FLAG);
        }
        Ok(score)
    }

    /// `00473FB0`'s arm after the shared body admitted an attach: the charge
    /// empties.
    pub(in crate::object_runtime) fn attach_selector55(
        &mut self,
        beamer: ObjectId,
    ) -> Result<(), ObjectError> {
        self.selector55_mut(beamer)?.charge_138 = 0;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector55_tests.rs"]
mod tests;
