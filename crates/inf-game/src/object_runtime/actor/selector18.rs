//! Selector 18, the Force Field: the shield over a site, which keeps actors
//! on their side of it until its area is secured. 00476750 over the shared
//! 0047A7A0, final table 005279E0, case 0x12, 0x184 bytes. The table is the
//! shared actor's 00527C00 but for +00 (004768B0), +04 (the commands
//! 00478860), +24 (the update 00476AE0), +28 (the light 00478910), +2C (the
//! draw 00477370), +8C (the capture hook 004771D0), +98 (the messages
//! 004787E0) and +CC (the scan score 004769C0). The interface at +138
//! (005279DC) is a creation listener: 00477350 tests every actor built after
//! the field.
//!
//! The field is an ellipsoid of radius +154, its height scaled by
//! +154 / +158; above its centre it is a dome, below a cylinder. Each field
//! holds one bit of a pool of 32 (0047DE30) and marks the actors inside it
//! with that bit in their +6C. While it is on, every update keeps each
//! collidable actor on its side: one crossing the edge is pushed back by
//! the speed it carried over the edge plus 600 (a player's unit) or 200,
//! and hears message 5, with a position correction when its +8C is 1. The
//! vehicle takes message 5 as 400 damage. Near the edge an actor of the
//! field's own team (with +178 set), or one whose player owns the field's
//! key grant +17A, switches it off for 2 s (5 s for an alien field).
//!
//! A player's vehicle near a field of another team hears whether the area
//! is secured: no team-0x12 actor in state 3 within twice radius + 0x500.
//! Secured, it is told once ("Area secured. Shields will now deactivate on
//! approach") and the field switches off for 2 s whenever the vehicle
//! reaches its edge; otherwise it hears "Area not secured - shields cannot
//! be lowered" and hint 0xF every 10 s at the edge.

use inf_formats::{
    native_basis::isqrt_48efc0, InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView,
};
use thiserror::Error;

use super::{
    derived::native_trunc_low32 as ftol, ActorCommandContext, ActorCommandError,
    ActorConstructionError, ActorConstructionRequest, ActorExtension,
};
use crate::object_runtime::{
    CreationListenerInterface, CreationListeners, ObjectArena, ObjectError, ObjectId, PlayerId,
};

/// Case 0x12's 00476750(0x400, 0x300): the radius and the height.
pub const FORCE_FIELD_RADIUS: u32 = 0x400;
pub const FORCE_FIELD_HEIGHT: u32 = 0x300;
/// 00476B00: the fade +160 climbs to half a second.
pub const FORCE_FIELD_FADE: i32 = 500_000;
/// The band each side of the edge: the passage test (004769F0) and the
/// player's warnings.
const EDGE_BAND: i32 = 0x300;
/// 00476D30 and 00476C17: what a bounce adds to the speed over the edge,
/// for a player's unit and for anything else.
const PLAYER_PUSH: i32 = 600;
const OTHER_PUSH: i32 = 200;
/// The bounce's message code; the vehicle's +98 takes it as 400 damage on
/// channel 7.
pub const FORCE_FIELD_BOUNCE_CODE: i32 = 5;
/// 0047704C: the player hears whether the area is secured within radius
/// + 0xA00.
const TELL_RANGE: i32 = 0xa00;
/// The scan's reach: twice radius + 0x500, as a WORD.
const SCAN_REACH: u16 = 0x500;
/// The scanner's flags: 0x20 required, 0x80 excluded.
const SCAN_REQUIRED: u32 = 0x20;
const SCAN_EXCLUDED: u32 = 0x80;
/// 004771AA and 00476A7B: off for 2 s to let something through; 5 s when an
/// alien field's own team passes (004C4B40).
pub const FORCE_FIELD_PASS_OFF: i32 = 2_000_000;
pub const FORCE_FIELD_ALIEN_PASS_OFF: i32 = 5_000_000;
/// 00477169: the warning repeats after 10 s.
const WARNING_INTERVAL: u32 = 10_000_000;
/// H0 0x141 "Area secured. Shields will now deactivate on approach" and
/// 0x142 "Area not secured - shields cannot be lowered".
pub const FORCE_FIELD_SECURED_TEXT: u16 = 0x141;
pub const FORCE_FIELD_NOT_SECURED_TEXT: u16 = 0x142;
/// 00477186: the hint the warning sends.
pub const FORCE_FIELD_NOT_SECURED_HINT: u32 = 0xf;
const ALIEN_TEAM: u32 = 0x12;
/// 004787F3: messages 0xB and 0xC switch a field below this radius off.
const SMALL_FIELD: u32 = 1000;
/// 004771E7: a released field comes back on after 2 s.
const RELEASE_OFF: i32 = 2_000_000;
/// Selector18ScanResidueApproximation: the best-actor slot +8 of the
/// scanner the passage (004769F0) and the secured notice (0047705A) build
/// on the stack. 00475F50 writes it only for a candidate, so when no alien
/// is near they test whatever an earlier call left in that stack DWORD.
/// At Infest03's and Infest04's 4,690 field updates it held 0047FD0F (a
/// return address) in 2,144, 1E848 in 2,180 and zero in 83 (`ff_scans.js`
/// residue); every field there is alien (team 0x12), so neither test ran.
/// Of the 26 placed fields only world 3's placement 12 (team 1) reaches the
/// notice, and none the passage's scan (it needs +178 and another team).
/// Retail tells that area is secured only on an update whose leftover is
/// zero; the port reads zero, as for a scan that found nothing.
pub const SELECTOR18_SCAN_RESIDUE_APPROXIMATION: u32 = 0;

fn log_scan_residue_approximation() {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("Selector18ScanResidueApproximation: a Force Field's scan found no alien and 004769F0/0047705A test the scanner's unwritten +8; the port reads 0 (SELECTOR18_RUNTIME.md)");
    }
}

/// 0055EF2C's three bytes as 00476750 writes them, the signed RGB the
/// light 00478910 deposits: green 0xF.
pub const FORCE_FIELD_LIGHT_RGB: [i8; 3] = [0, 0xf, 0];

/// 0055F0E4 (the bits in use) and 0053B8B0 (the bit last given, 1 at
/// startup). 0047DE30 gives each field the next free bit of 32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForceFieldBits {
    pub used: u32,
    pub cursor: u32,
}

impl Default for ForceFieldBits {
    fn default() -> Self {
        Self { used: 0, cursor: 1 }
    }
}

impl ForceFieldBits {
    /// 0047DE30: rotate the cursor left (1 after the top bit) to the next
    /// free bit; none once 32 tries find all taken.
    pub fn allocate(&mut self) -> u32 {
        let mut tries = 0;
        loop {
            self.cursor <<= 1;
            if self.cursor == 0 {
                self.cursor = 1;
            }
            if self.cursor & self.used == 0 {
                break;
            }
            tries += 1;
            if tries > 0x1f {
                return 0;
            }
        }
        self.used |= self.cursor;
        self.cursor
    }

    /// 0047DE80: give a bit back.
    pub fn release(&mut self, bit: u32) {
        self.used &= !bit;
    }
}

/// 00476750's fields past the shared actor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector18State {
    /// +154: the radius; command 0x10 also stores its WORD at +28.
    pub radius_154: u32,
    /// +158: the height the vertical offset is scaled against.
    pub height_158: u32,
    /// +15C: microseconds until the field switches back on, 0 for none.
    pub off_timer_15c: i32,
    /// +160: the fade, which climbs while +177 is set and falls otherwise.
    pub fade_160: i32,
    /// +164: the field's clock, the sum of its updates' deltas.
    pub clock_164: u32,
    /// +168: zeroed by the constructor; nothing else in the class reads it.
    pub dword_168: u32,
    /// +16C: the field's bit (0047DE30).
    pub bit_16c: u32,
    /// +170: the clock at the last "not secured" warning.
    pub warned_170: u32,
    /// BYTE +174: the player has heard the area is secured.
    pub secured_told_174: bool,
    /// BYTE +175: a link (message 7) reached the field.
    pub linked_175: bool,
    /// BYTE +176: the field is on and keeps actors to their side.
    pub on_176: bool,
    /// BYTE +177: the field shows; the fade follows it.
    pub shown_177: bool,
    /// BYTE +178: its own team passes (command 0x13).
    pub team_passes_178: bool,
    /// WORD +17A: the grant whose player passes (command 0x12).
    pub key_grant_17a: u16,
    /// +17C: the links counted by messages 7 and 8.
    pub links_17c: i32,
    /// +180: the hint command a released field sends (command 0x14).
    pub hint_180: u32,
}

impl Selector18State {
    fn new(bit: u32) -> Self {
        Self {
            radius_154: FORCE_FIELD_RADIUS,
            height_158: FORCE_FIELD_HEIGHT,
            off_timer_15c: 0,
            fade_160: 0,
            clock_164: 0,
            dword_168: 0,
            bit_16c: bit,
            warned_170: 0,
            secured_told_174: false,
            linked_175: false,
            on_176: false,
            shown_177: false,
            team_passes_178: false,
            key_grant_17a: 0,
            links_17c: 0,
            hint_180: 0,
        }
    }
}

/// The services the field's update reaches outside the arena.
pub trait Selector18Host {
    type Error;
    /// 0046B090 of the actor's handle: the player whose unit it is.
    fn find_player(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<Option<PlayerId>, Self::Error>;
    /// 00488AA0: the first player.
    fn first_player(&mut self, arena: &mut ObjectArena) -> Result<Option<PlayerId>, Self::Error>;
    /// 0046B030: the first player's vehicle.
    fn player_vehicle(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error>;
    /// 0046BDC0: whether the player owns the grant.
    fn owns_grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<bool, Self::Error>;
    /// 004750C0 on the player's grants +9C.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        grant: u16,
    ) -> Result<(), Self::Error>;
    /// 0046C760(command, 0) on the player.
    fn player_command(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        command: u32,
    ) -> Result<(), Self::Error>;
    /// The bounced actor's +98 (field, 0, 5, argument): the argument points
    /// at the WORD correction for an actor whose +8C is 1, else it is NULL.
    fn bounce(
        &mut self,
        arena: &mut ObjectArena,
        field: ObjectId,
        actor: ObjectId,
        correction: Option<[u16; 3]>,
    ) -> Result<(), Self::Error>;
    /// 0047DD80(vehicle, 0, 4, text): the vehicle's +98 with code 4.
    fn tell(
        &mut self,
        arena: &mut ObjectArena,
        field: ObjectId,
        vehicle: ObjectId,
        text: u16,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector18Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("force field {0:?} has no height to scale by")]
    ZeroHeight(ObjectId),
    /// The field's bit in the actor's +6C came from 0048D820's stack residue.
    #[error("force field {field:?}'s bit in actor {actor:?}'s +6C is stack residue")]
    UnknownMark { field: ObjectId, actor: ObjectId },
    #[error("force field host: {0:?}")]
    Host(E),
}

fn position(arena: &ObjectArena, object: ObjectId) -> Result<[u16; 3], ObjectError> {
    arena
        .object(object)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(object))
}

/// 00417DA0: the unit vector of a WORD offset, each component a binary32,
/// from the squares summed z, y, x at the FPU's 53 bits; (1, 0, 0) for none.
pub fn force_field_normal(offset: [i16; 3]) -> [f32; 3] {
    let [x, y, z] = offset.map(|value| f64::from(f32::from(value)));
    let length = ((z * z + y * y) + x * x).sqrt();
    if length == 0.0 {
        return [1.0, 0.0, 0.0];
    }
    [
        (x / length) as f32,
        (y / length) as f32,
        (z / length) as f32,
    ]
}

/// `k` times each component of the normal, truncated to WORDs.
fn scaled(normal: [f32; 3], k: i32) -> [u16; 3] {
    normal.map(|component| ftol(f64::from(k) * f64::from(component)) as u16)
}

impl ObjectArena {
    /// 00476750 after the shared 0047A7A0: the defaults, the field's bit,
    /// +28 = the radius's WORD, then 004772B0(1, 0), which marks the actors
    /// inside the field at its pre-context position, and last its +138
    /// listener binding at the tail of 0054E878.
    pub fn allocate_selector18(
        &mut self,
        request: ActorConstructionRequest<'_>,
        listeners: &mut CreationListeners,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let bit = self.force_field_bits.allocate();
        self.actor_mut(object)?.extension =
            ActorExtension::Selector18(Box::new(Selector18State::new(bit)));
        self.owned_mut(object)?.state.base.word_28 = FORCE_FIELD_RADIUS as u16;
        self.switch_selector18(object, true, 0)?;
        listeners.register(object, CreationListenerInterface::Selector18);
        Ok(object)
    }

    pub fn selector18(&self, object: ObjectId) -> Result<&Selector18State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector18(state) => Ok(state),
            _ => Err(ObjectError::NotSelector18(object)),
        }
    }

    fn selector18_mut(&mut self, object: ObjectId) -> Result<&mut Selector18State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector18(state) => Ok(state),
            _ => Err(ObjectError::NotSelector18(object)),
        }
    }

    /// 00478860: the shared 0047ADA0 through code 15; 0x10 stores the
    /// radius (and its WORD at +28), 0x11 the height, 0x12 the key grant,
    /// 0x13 BYTE != 0 at +178, 0x14 the hint DWORD; anything higher refuses.
    pub fn apply_selector18_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(18) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector18(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        match (code, &command.payload) {
            (0x10, InstanceCommandPayload::Selector18Radius(value)) => {
                self.selector18_mut(object)?.radius_154 = u32::from(value.value);
                self.owned_mut(object)?.state.base.word_28 = value.value;
            }
            (0x11, InstanceCommandPayload::Selector18Height(value)) => {
                self.selector18_mut(object)?.height_158 = u32::from(value.value);
            }
            (0x12, InstanceCommandPayload::Selector18KeyGrant(value)) => {
                self.selector18_mut(object)?.key_grant_17a = value.value;
            }
            (0x13, InstanceCommandPayload::Selector18TeamPasses(value)) => {
                self.selector18_mut(object)?.team_passes_178 = value.value != 0;
            }
            (0x14, InstanceCommandPayload::Selector18Hint(value)) => {
                self.selector18_mut(object)?.hint_180 = value.value;
            }
            (0x10..=0x14, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 00477210(actor, radius²): clear the field's bit in the actor's +6C,
    /// then set it again for an actor inside, unless its flags keep it out
    /// (0x80 when 0x40 is set; else 8, or no 4). Below the centre the height
    /// counts for nothing.
    fn mark_selector18(
        &mut self,
        field: ObjectId,
        actor: ObjectId,
        radius_squared: u32,
    ) -> Result<(), ObjectError> {
        let state = self.selector18(field)?;
        let (bit, radius, height) = (state.bit_16c, state.radius_154, state.height_158);
        let flags = self.object(actor)?.base.flags & 0xcc;
        let mask = self.actor(actor)?.word_6c & !bit;
        let state = self.actor_mut(actor)?;
        state.word_6c = mask;
        state.unknown_6c &= !bit;
        let admitted = if flags & 0x40 != 0 {
            flags & 0x80 == 0
        } else {
            flags & 8 == 0 && flags & 4 != 0
        };
        if !admitted {
            return Ok(());
        }
        let center = position(self, field)?;
        let Some(other) = self.object(actor)?.base.position_words else {
            // 0048D820's temporary stands at stack residue: the test's
            // outcome, this bit, is unknown. 0048D820 disposes it in the
            // same call, which takes it off every walk that reads the bit.
            self.actor_mut(actor)?.unknown_6c |= bit;
            return Ok(());
        };
        // 00477243..00477259: the full difference, then idiv.
        let rise = i32::from(center[1] as i16).wrapping_sub(i32::from(other[1] as i16));
        let height = height as i32;
        if height == 0 {
            return Err(ObjectError::ActorPositionUnknown(field));
        }
        let mut dy = (rise.wrapping_mul(radius as i32) / height) as i16;
        if dy > 0 {
            dy = 0;
        }
        let dx = i32::from(center[0].wrapping_sub(other[0]) as i16);
        let dz = i32::from(center[2].wrapping_sub(other[2]) as i16);
        let dy = i32::from(dy);
        let squared = (dx.wrapping_mul(dx))
            .wrapping_add(dy.wrapping_mul(dy))
            .wrapping_add(dz.wrapping_mul(dz)) as u32;
        if squared < radius_squared {
            self.actor_mut(actor)?.word_6c = mask | bit;
        }
        Ok(())
    }

    /// 004772B0(on, timer): on marks every actor of the list inside the
    /// field; off stores the timer and clears the field's bit everywhere.
    /// Both store +176 and +177.
    pub fn switch_selector18(
        &mut self,
        field: ObjectId,
        on: bool,
        timer: i32,
    ) -> Result<(), ObjectError> {
        if on {
            let radius = self.selector18(field)?.radius_154;
            let squared = radius.wrapping_mul(radius);
            for actor in self.actor_list().to_vec() {
                self.mark_selector18(field, actor, squared)?;
            }
        } else {
            let state = self.selector18_mut(field)?;
            state.off_timer_15c = timer;
            let bit = state.bit_16c;
            for actor in self.actor_list().to_vec() {
                let state = self.actor_mut(actor)?;
                state.word_6c &= !bit;
                state.unknown_6c &= !bit;
            }
        }
        let state = self.selector18_mut(field)?;
        state.on_176 = on;
        state.shown_177 = on;
        Ok(())
    }

    /// 00477350, the creation listener: a new actor is marked against the
    /// field as the switch's walk marks the others.
    pub fn selector18_listener(
        &mut self,
        field: ObjectId,
        actor: ObjectId,
    ) -> Result<(), ObjectError> {
        let radius = self.selector18(field)?.radius_154;
        self.mark_selector18(field, actor, radius.wrapping_mul(radius))
    }

    /// The fade +160 a field reaches after half a second of updates, for
    /// draw fixtures that skip them.
    #[cfg(test)]
    pub(crate) fn supply_selector18_fade(
        &mut self,
        field: ObjectId,
        fade: i32,
    ) -> Result<(), ObjectError> {
        self.selector18_mut(field)?.fade_160 = fade;
        Ok(())
    }

    /// 004787E0, the field's +98: 7 links it (+17C up, +175 set) and
    /// switches it on; 8 unlinks; 0xB and 0xC switch a field below radius
    /// 1000 off for 5 s. It returns 0.
    pub fn selector18_message(&mut self, field: ObjectId, code: i32) -> Result<u32, ObjectError> {
        match code {
            7 => {
                let state = self.selector18_mut(field)?;
                state.linked_175 = true;
                state.links_17c = state.links_17c.wrapping_add(1);
                self.switch_selector18(field, true, 0)?;
            }
            8 => {
                let state = self.selector18_mut(field)?;
                state.links_17c = state.links_17c.wrapping_sub(1);
            }
            0xb | 0xc => {
                if self.selector18(field)?.radius_154 < SMALL_FIELD {
                    self.switch_selector18(field, false, FORCE_FIELD_ALIEN_PASS_OFF)?;
                }
            }
            _ => {}
        }
        Ok(0)
    }

    /// 004771D0's own part before the shared capture/release 0047D7C0: a
    /// capture switches the field off, a release sets it to come back on
    /// after 2 s.
    pub fn selector18_capture(
        &mut self,
        field: ObjectId,
        capture: bool,
    ) -> Result<(), ObjectError> {
        if capture {
            self.switch_selector18(field, false, 0)
        } else {
            self.selector18_mut(field)?.off_timer_15c = RELEASE_OFF;
            Ok(())
        }
    }

    /// 004769C0, the field's +CC and the scan's filter: an alien (team
    /// 0x12) in state 3 scores 0x7FFFFFFF less its squared distance.
    pub fn selector18_score(&self, candidate: ObjectId, squared: u32) -> Result<u32, ObjectError> {
        let actor = self.actor(candidate)?;
        Ok(if actor.word_c4 == 3 && actor.word_b4 == ALIEN_TEAM {
            0x7fff_ffff_u32.wrapping_sub(squared)
        } else {
            0
        })
    }

    /// 00475F50 with the field's scanner: of the actors with flag 0x20 and
    /// without 0x80, other than the field, nearer in X/Z than twice (radius
    /// + 0x500) (a WORD, squared), the first with the highest score
    /// (004769C0), if any scores.
    pub fn selector18_scan(&self, field: ObjectId) -> Result<Option<ObjectId>, ObjectError> {
        let radius = self.selector18(field)?.radius_154 as u16;
        let reach = i32::from((radius.wrapping_add(SCAN_REACH)).wrapping_shl(1) as i16);
        let range = reach.wrapping_mul(reach) as u32;
        let center = position(self, field)?;
        let (mut best, mut best_score) = (None, 0);
        for candidate in self.actor_list().to_vec() {
            let flags = self.object(candidate)?.base.flags;
            if flags & SCAN_EXCLUDED != 0 || flags & SCAN_REQUIRED != SCAN_REQUIRED {
                continue;
            }
            let other = position(self, candidate)?;
            let dx = i32::from(center[0].wrapping_sub(other[0]) as i16);
            let dz = i32::from(center[2].wrapping_sub(other[2]) as i16);
            let squared = (dz.wrapping_mul(dz) as u32).wrapping_add(dx.wrapping_mul(dx) as u32);
            if squared >= range || candidate == field {
                continue;
            }
            let score = self.selector18_score(candidate, squared)?;
            if score > best_score {
                (best, best_score) = (Some(candidate), score);
            }
        }
        Ok(best)
    }

    /// The passage's and the secured notice's tests: the scanner's best
    /// actor +8, which 00475F50 writes only for a candidate.
    fn selector18_alien_found(&self, field: ObjectId) -> Result<bool, ObjectError> {
        if self.selector18_scan(field)?.is_some() {
            return Ok(true);
        }
        log_scan_residue_approximation();
        Ok(SELECTOR18_SCAN_RESIDUE_APPROXIMATION != 0)
    }

    /// 004768D0 before the shared destructor: off, then the bit back to the
    /// pool (0047DE80). The caller unregisters the listener.
    pub fn retire_selector18(&mut self, field: ObjectId) -> Result<(), ObjectError> {
        self.switch_selector18(field, false, 0)?;
        let state = self.selector18_mut(field)?;
        let bit = state.bit_16c;
        state.bit_16c = 0;
        self.force_field_bits.release(bit);
        Ok(())
    }

    /// 004769F0(actor) near the edge: with +178 set and the actor of the
    /// field's team (or 1 against 2), the field switches off for 5 s when
    /// it is alien, else for 2 s once no alien stands near. Then, with a key
    /// grant, an actor whose player owns it switches it off for 2 s.
    fn selector18_passage<H: Selector18Host>(
        &mut self,
        field: ObjectId,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector18Error<H::Error>> {
        if self.selector18(field)?.team_passes_178 {
            let team = self.actor(field)?.word_b4;
            let other = self.actor(actor)?.word_b4;
            if other == team || (other == 1 && team == 2) {
                if team == ALIEN_TEAM {
                    self.switch_selector18(field, false, FORCE_FIELD_ALIEN_PASS_OFF)?;
                } else if !self.selector18_alien_found(field)? {
                    self.switch_selector18(field, false, FORCE_FIELD_PASS_OFF)?;
                }
            }
        }
        let grant = self.selector18(field)?.key_grant_17a;
        if grant != 0 {
            if let Some(player) = host
                .find_player(self, actor)
                .map_err(Selector18Error::Host)?
            {
                if host
                    .owns_grant(self, player, grant)
                    .map_err(Selector18Error::Host)?
                {
                    self.switch_selector18(field, false, FORCE_FIELD_PASS_OFF)?;
                }
            }
        }
        Ok(())
    }

    /// 00476AE0, the field's +24.
    pub fn tick_selector18<H: Selector18Host>(
        &mut self,
        field: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector18Error<H::Error>> {
        let step = delta as i32;
        // 00476AF0..00476B35: the fade.
        let state = self.selector18_mut(field)?;
        state.fade_160 = if state.shown_177 {
            if state.fade_160 < FORCE_FIELD_FADE {
                state.fade_160.wrapping_add(step)
            } else {
                FORCE_FIELD_FADE
            }
        } else if state.fade_160 > step {
            state.fade_160 - step
        } else {
            0
        };
        // 00476B35..00476BAA: a linked field whose links are all gone grants
        // its +7A and sends its hint, then goes off (with a hint) or away.
        if state.linked_175 && state.links_17c == 0 {
            let hint = state.hint_180;
            if let Some(player) = host.first_player(self).map_err(Selector18Error::Host)? {
                let grant = self.actor(field)?.word_7a;
                if grant != 0 {
                    host.grant(self, player, grant)
                        .map_err(Selector18Error::Host)?;
                }
                if hint != 0 {
                    host.player_command(self, player, hint)
                        .map_err(Selector18Error::Host)?;
                }
            }
            if hint != 0 {
                self.switch_selector18(field, false, 0)?;
            } else {
                self.dispose(field)?;
            }
            return Ok(());
        }
        // 00476BAD..00476BE6: the clock, then the off timer.
        let state = self.selector18_mut(field)?;
        state.clock_164 = state.clock_164.wrapping_add(delta);
        if state.off_timer_15c > 0 {
            let left = state.off_timer_15c.wrapping_sub(step);
            state.off_timer_15c = left;
            if left <= 0 {
                state.off_timer_15c = 0;
                self.switch_selector18(field, true, 0)?;
            }
        }
        if !self.selector18(field)?.on_176 {
            return Ok(());
        }
        // 00476BF4..00476FDF: each actor of the list while the field is on.
        for actor in self.actor_list().to_vec() {
            if !self.selector18(field)?.on_176 {
                break;
            }
            self.selector18_keep(field, actor, host)?;
        }
        self.selector18_tell(field, host)
    }

    /// 00476C14..00476FC5, one actor of the update's walk.
    fn selector18_keep<H: Selector18Host>(
        &mut self,
        field: ObjectId,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector18Error<H::Error>> {
        let flags = self.object(actor)?.base.flags;
        if flags & 0x88 != 0 || flags & 4 == 0 {
            return Ok(());
        }
        let state = self.selector18(field)?;
        let (bit, radius, height) = (
            state.bit_16c,
            state.radius_154 as i32,
            state.height_158 as i32,
        );
        if height == 0 {
            return Err(Selector18Error::ZeroHeight(field));
        }
        if self.actor(actor)?.unknown_6c & bit != 0 {
            return Err(Selector18Error::UnknownMark { field, actor });
        }
        let inside = self.actor(actor)?.word_6c & bit == bit;
        let extent = i32::from(self.object(actor)?.base.word_28);
        let center = position(self, field)?;
        let other = position(self, actor)?;
        // 00476C56..00476C98: the WORD difference scaled, then nothing below.
        let rise = i32::from(other[1].wrapping_sub(center[1]) as i16);
        let mut dy = rise.wrapping_mul(radius) / height;
        if (dy as i16) < 0 {
            dy = 0;
        }
        let dx = other[0].wrapping_sub(center[0]) as i16;
        let dz = other[2].wrapping_sub(center[2]) as i16;
        let offset = [dx, dy as i16, dz];
        let squared = offset
            .iter()
            .map(|&value| i32::from(value).wrapping_mul(i32::from(value)))
            .fold(0_i32, i32::wrapping_add) as u32;
        let edge = if inside {
            radius.wrapping_sub(extent)
        } else {
            radius.wrapping_add(extent)
        };
        let band = if inside {
            edge.wrapping_sub(EDGE_BAND)
        } else {
            edge.wrapping_add(EDGE_BAND)
        };
        let band_squared = band.wrapping_mul(band) as u32;
        let near = if inside {
            squared > band_squared
        } else {
            squared < band_squared
        };
        if near {
            self.selector18_passage(field, actor, host)?;
        }
        if !self.selector18(field)?.on_176 {
            return Ok(());
        }
        let edge_squared = edge.wrapping_mul(edge) as u32;
        let crossed = if inside {
            squared > edge_squared
        } else {
            squared < edge_squared
        };
        if !crossed {
            return Ok(());
        }
        // 00476D12..00476DC1: back across the edge by the speed over it.
        let normal = force_field_normal(offset);
        let push = if host
            .find_player(self, actor)
            .map_err(Selector18Error::Host)?
            .is_some()
        {
            PLAYER_PUSH
        } else {
            OTHER_PUSH
        };
        let [vx, vy, vz] = self
            .actor(actor)?
            .words_8e_92
            .map(|word| f64::from(word as i16));
        let along = ftol(
            (vz * f64::from(normal[2]) + vy * f64::from(normal[1])) + vx * f64::from(normal[0]),
        );
        let k = if inside {
            i32::from((along.wrapping_add(push)).wrapping_neg() as i16)
        } else {
            i32::from((push.wrapping_sub(along)) as i16)
        };
        let change = scaled(normal, k);
        let velocity = &mut self.actor_mut(actor)?.words_8e_92;
        for (word, add) in velocity.iter_mut().zip(change) {
            *word = word.wrapping_add(add);
        }
        // 00476DC6..00476E3E: a +8C of 1 hears where the edge is.
        let correction = if self.actor(actor)?.word_8c == 1 {
            let distance = isqrt_48efc0(squared as i32) as i32;
            let gap = if inside {
                edge.wrapping_sub(distance).wrapping_sub(1)
            } else {
                edge.wrapping_sub(distance).wrapping_add(1)
            };
            Some(scaled(normal, i32::from(gap as i16)))
        } else {
            None
        };
        host.bounce(self, field, actor, correction)
            .map_err(Selector18Error::Host)
    }

    /// 00476FE5..004771B8: what the player's vehicle near the field hears.
    fn selector18_tell<H: Selector18Host>(
        &mut self,
        field: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector18Error<H::Error>> {
        let Some(vehicle) = host.player_vehicle(self).map_err(Selector18Error::Host)? else {
            return Ok(());
        };
        if self.actor(field)?.word_b4 == ALIEN_TEAM {
            return Ok(());
        }
        let center = position(self, field)?;
        let other = position(self, vehicle)?;
        let delta = |axis: usize| i32::from(center[axis].wrapping_sub(other[axis]) as i16);
        let squared = (delta(0).wrapping_mul(delta(0)))
            .wrapping_add(delta(1).wrapping_mul(delta(1)))
            .wrapping_add(delta(2).wrapping_mul(delta(2))) as u32;
        let state = self.selector18(field)?;
        let radius = state.radius_154 as i32;
        let tell = radius.wrapping_add(TELL_RANGE);
        if !state.secured_told_174
            && squared < tell.wrapping_mul(tell) as u32
            && !self.selector18_alien_found(field)?
        {
            host.tell(self, field, vehicle, FORCE_FIELD_SECURED_TEXT)
                .map_err(Selector18Error::Host)?;
            self.selector18_mut(field)?.secured_told_174 = true;
        }
        let outer = radius.wrapping_add(EDGE_BAND);
        let inner = radius.wrapping_sub(EDGE_BAND);
        if squared >= outer.wrapping_mul(outer) as u32
            || squared <= inner.wrapping_mul(inner) as u32
        {
            return Ok(());
        }
        // 00477155 tests the best score +C, which the scan zeroes first.
        let aliens = self.selector18_scan(field)?.is_some();
        let state = self.selector18(field)?;
        if squared < radius.wrapping_mul(radius) as u32 || !aliens {
            self.switch_selector18(field, false, FORCE_FIELD_PASS_OFF)?;
            return Ok(());
        }
        if state.clock_164 <= state.warned_170.wrapping_add(WARNING_INTERVAL) {
            return Ok(());
        }
        host.tell(self, field, vehicle, FORCE_FIELD_NOT_SECURED_TEXT)
            .map_err(Selector18Error::Host)?;
        if let Some(player) = host.first_player(self).map_err(Selector18Error::Host)? {
            host.player_command(self, player, FORCE_FIELD_NOT_SECURED_HINT)
                .map_err(Selector18Error::Host)?;
        }
        let state = self.selector18_mut(field)?;
        state.warned_170 = state.clock_164;
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector18_tests.rs"]
mod tests;
