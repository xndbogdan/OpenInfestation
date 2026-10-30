//! Selector 47, the Siren: 0047FA40 over 0040D800 over the shared actor (case
//! 0x2F, 0x170 bytes, final 00527CD4). Its table is the family's 005203C0
//! but for +00 (0047FEC0), +04 (the commands 0047FDC0), +10 (its position as
//! the custom vertex, 0047FA80), +14 (the model WORDs 0047FAA0), +24 (the
//! update 0047FAC0), +34 (the after-load 0047FE90, the stabiliser's), +98
//! (0047FE10) and +AC (its death 0047FAB0).
//!
//! An actor of another side (+B4) as the first player's vehicle within its
//! radius +164, or the loss of the last link holding it, sets off its alarm
//! for +16C seconds: it alerts its side near it to the vehicle (0047C840),
//! spins up (+15A) and sounds 0xE. Messages 0xB and 0xC silence it for 10 s;
//! 0xE sets the alarm. Its death winds the sound's pitch down for two
//! seconds, then it flashes the world and disposes itself.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use thiserror::Error;

use super::{
    family40d800::{Actor40d800State, FamilyLinkChange},
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, ActorHandle, FamilyUpdateError, FamilyUpdateHost,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::process_sound::ProcessSoundRef;

/// 0047FC6A: the alarm's sound.
pub const SIREN_SOUND: u32 = 0xe;
/// 0047FB4C: the world flash at the end of its death.
const FLASH_RADIUS: u32 = 0xf00;
/// 0047FE52: messages 0xB and 0xC silence it this long.
const SILENCE: u32 = 10_000_000;
/// 0047FDA0: a trigger holds the alarm this much past its seconds.
const TRIGGER_EXTRA: u32 = 0x7_a121;
/// 0047FAB0: its death's countdown.
const DYING: u32 = 2_000_000;
/// The sound's full volume and pitch, 1.0 in 16.16.
const FULL: u32 = 0x1_0000;

/// 0047FA40's writes after 0040D800 (0047FA4A..0047FA6C), and the fields its
/// methods own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector47State {
    pub intermediate: Actor40d800State,
    /// WORD +158, zero; the model's selector 0.
    pub word_158: u16,
    /// WORD +15A: the spin, 0..0xFFFF; the model's selector 1.
    pub spin_15a: u16,
    /// WORD +15C: the turn the spin advances.
    pub turn_15c: u16,
    /// WORD +15E, unwritten (heap residue); only the model's selector 3
    /// reads it.
    pub word_15e: Option<u16>,
    /// +160: microseconds of alarm or silence left, signed.
    pub alarm_160: u32,
    /// +164: the trigger radius, 0xA00; command 23 stores a signed WORD.
    pub radius_164: u32,
    /// +168: microseconds of dying left; zero before the death.
    pub dying_168: u32,
    /// BYTE +16C: the alarm's signed seconds, from command 22. The
    /// constructor leaves it unwritten.
    pub seconds_16c: Option<u8>,
    /// BYTE +16D: nonzero while silenced. The constructor leaves it
    /// unwritten; the first update reads it.
    pub silenced_16d: Option<u8>,
}

impl Selector47State {
    fn new() -> Self {
        Self {
            intermediate: Actor40d800State::new(),
            word_158: 0,
            spin_15a: 0,
            turn_15c: 0,
            word_15e: None,
            alarm_160: 0,
            radius_164: 0xa00,
            dying_168: 0,
            seconds_16c: None,
            silenced_16d: None,
        }
    }

    /// 0047FAA0, the model's +14: the WORD at +158 plus twice the selector.
    /// None for a WORD the class never writes or past its 0x170 bytes.
    pub fn model_value(&self, selector: u32) -> Option<u16> {
        let split = |dword: u32| [dword as u16, (dword >> 16) as u16];
        match selector {
            0 => Some(self.word_158),
            1 => Some(self.spin_15a),
            2 => Some(self.turn_15c),
            3 => self.word_15e,
            4 | 5 => Some(split(self.alarm_160)[selector as usize - 4]),
            6 | 7 => Some(split(self.radius_164)[selector as usize - 6]),
            8 | 9 => Some(split(self.dying_168)[selector as usize - 8]),
            10 => Some(u16::from(self.seconds_16c?) | (u16::from(self.silenced_16d?) << 8)),
            _ => None,
        }
    }
}

/// What the update reaches beyond the shared status services.
pub trait Selector47Host: FamilyUpdateHost {
    /// The particle load factor 0047C990 and 0047DFD0 read.
    fn status_load_factor(&self) -> Option<i32>;
    /// Byte 0055F14C: attract playback starts no sound.
    fn attract(&self) -> bool;
    /// 00487F60(0xE, +32, 0x10000, 0x10000, 0); None when the sound has no
    /// resource.
    fn create_alarm(
        &mut self,
        arena: &ObjectArena,
        siren: ObjectId,
    ) -> Result<Option<ProcessSoundRef>, Self::Error>;
    /// 00487FC0 at the Siren's +32.
    fn update_alarm(
        &mut self,
        arena: &ObjectArena,
        siren: ObjectId,
        reference: ProcessSoundRef,
        volume: u32,
        pitch: u32,
    ) -> Result<(), Self::Error>;
    /// 00487FE0.
    fn release_alarm(&mut self, reference: ProcessSoundRef) -> Result<(), Self::Error>;
    /// 0047C840(source) on the Siren: its side's actors near it take the
    /// source as a target.
    fn alert(
        &mut self,
        arena: &mut ObjectArena,
        siren: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error>;
    /// G's 00487D70(position, 0xF00, 0), the world flash.
    fn flash(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        radius: u32,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum Selector47Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Status(FamilyUpdateError<E>),
    #[error("Siren {0:?} reads its alarm seconds +16C, which no command wrote")]
    UnwrittenSeconds(ObjectId),
    #[error(
        "Siren {0:?}'s first update reads its unwritten silence byte +16D where both arms differ"
    )]
    UnwrittenSilence(ObjectId),
    #[error("Siren {0:?} alerts with the process NULL handle, which is not lent")]
    MissingProcessNullHandle(ObjectId),
    #[error("Siren {siren:?} service {address:08X} failed: {error}")]
    Host {
        siren: ObjectId,
        address: u32,
        error: E,
    },
}

impl<E> From<FamilyUpdateError<E>> for Selector47Error<E> {
    fn from(error: FamilyUpdateError<E>) -> Self {
        Self::Status(error)
    }
}

impl ObjectArena {
    /// Case 0x2F (00460EF0..00460F13): 0x170 bytes, then 0047FA40.
    pub fn allocate_selector47(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        let mut state = Selector47State::new();
        state.silenced_16d = self
            .actor_allocation
            .initial_extended_word()
            .map(|word| word as u8);
        self.actor_mut(object)?.extension = ActorExtension::Selector47(Box::new(state));
        Ok(object)
    }

    pub fn selector47(&self, object: ObjectId) -> Result<&Selector47State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector47(state) => Ok(state),
            _ => Err(ObjectError::NotSelector47(object)),
        }
    }

    pub(in crate::object_runtime) fn selector47_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector47State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector47(state) => Ok(state),
            _ => Err(ObjectError::NotSelector47(object)),
        }
    }

    /// 0047FDC0: 0040D9C0 through code 20 (shared commands through 15),
    /// then code 21 accepts, 22 copies the seconds BYTE and 23 sign-extends
    /// the radius WORD; higher codes refuse.
    pub fn apply_selector47_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(47) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector47(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector47_mut(object)?;
        if let Some(result) = state.intermediate.apply_command(code, &command.payload) {
            let dirty_0200 = result.map_err(ActorCommandError::Payload)?;
            if dirty_0200 {
                self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (21, NoPayloadRead) => {}
            (22, Selector47Seconds16c(value)) => state.seconds_16c = Some(value.value),
            (23, Selector47Radius164(value)) => state.radius_164 = i32::from(value.value) as u32,
            (21..=23, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 0047FE10, the Siren's +98: 0xB and 0xC silence it for 10 s, 0xE
    /// sets the alarm to the argument or, when it is zero, to +16C seconds;
    /// every other message reaches the family's 0040DE70. It answers 0.
    pub fn selector47_message(
        &mut self,
        siren: ObjectId,
        code: i32,
        argument: u32,
    ) -> Result<FamilyLinkChange, Selector47Error<std::convert::Infallible>> {
        match code {
            0xb | 0xc => {
                let state = self.selector47_mut(siren)?;
                state.alarm_160 = SILENCE;
                state.silenced_16d = Some(1);
                Ok(FamilyLinkChange::None)
            }
            0xe => {
                let state = self.selector47_mut(siren)?;
                state.alarm_160 = if argument != 0 {
                    argument
                } else {
                    seconds(state, siren)?
                };
                Ok(FamilyLinkChange::None)
            }
            _ => Ok(self.family40d800_message(siren, code)?),
        }
    }

    /// 0047FAB0, the Siren's +AC: its death starts a 2 s countdown.
    pub fn retire_selector47(&mut self, siren: ObjectId) -> Result<(), ObjectError> {
        self.selector47_mut(siren)?.dying_168 = DYING;
        Ok(())
    }

    /// 0047FAC0, the Siren's +24. After the shared status (0047C990): a
    /// dying Siren counts down with the status visual (0047DFD0) and lowers
    /// its sound's pitch, then flashes the world, releases the sound and
    /// disposes itself. A silenced one releases its sound and counts the
    /// silence down. Otherwise, while its family counts match, it alerts with
    /// the process NULL handle when it has a crew; a running alarm counts
    /// down, spins it up and sounds, a spent one spins it down and releases
    /// the sound; the spin turns it; and below its seconds the first
    /// player's vehicle of another side within its radius, or a lost last
    /// link, sets the alarm and alerts its side to the vehicle.
    pub fn tick_selector47<H: Selector47Host>(
        &mut self,
        siren: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector47Error<H::Error>> {
        let load = host.status_load_factor();
        self.tick_actor_status(siren, delta, load, host)?;
        let dying = self.selector47(siren)?.dying_168 as i32;
        if dying != 0 {
            return self.selector47_dying(siren, dying, delta, load, host);
        }
        match self.selector47(siren)?.silenced_16d {
            Some(0) => {}
            Some(_) => return self.selector47_silenced(siren, delta, host),
            None => return self.selector47_first_silence(siren, delta, host),
        }
        self.selector47_alarm(siren, delta, host)
    }

    /// 0047FAE0..0047FB81.
    fn selector47_dying<H: Selector47Host>(
        &mut self,
        siren: ObjectId,
        dying: i32,
        delta: u32,
        load: Option<i32>,
        host: &mut H,
    ) -> Result<(), Selector47Error<H::Error>> {
        if dying > delta as i32 {
            let left = dying.wrapping_sub(delta as i32);
            self.selector47_mut(siren)?.dying_168 = left as u32;
            self.tick_status_visual(siren, delta, load, host)?;
            if let Some(reference) = self.selector47(siren)?.intermediate.sound_154 {
                // 0047FB08..0047FB31: signed /1000, <<16, signed /2000.
                let pitch = ((left / 1000) << 16) / 2000;
                host.update_alarm(self, siren, reference, FULL, pitch as u32)
                    .map_err(host_error(siren, 0x0047_fb3c))?;
            }
            return Ok(());
        }
        let position = siren_position(self, siren)?;
        host.flash(self, position, FLASH_RADIUS)
            .map_err(host_error(siren, 0x0047_fb58))?;
        if let Some(reference) = self.selector47_mut(siren)?.intermediate.sound_154.take() {
            host.release_alarm(reference)
                .map_err(host_error(siren, 0x0047_fb6f))?;
        }
        self.dispose(siren)?;
        Ok(())
    }

    /// 0047FB8C..0047FBCF.
    fn selector47_silenced<H: Selector47Host>(
        &mut self,
        siren: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector47Error<H::Error>> {
        if let Some(reference) = self.selector47_mut(siren)?.intermediate.sound_154.take() {
            host.release_alarm(reference)
                .map_err(host_error(siren, 0x0047_fb9e))?;
        }
        let state = self.selector47_mut(siren)?;
        if state.alarm_160 as i32 > delta as i32 {
            state.alarm_160 = state.alarm_160.wrapping_sub(delta);
        } else {
            state.alarm_160 = 0;
            state.silenced_16d = Some(0);
        }
        Ok(())
    }

    /// The first update reads the silence byte the constructor left
    /// unwritten. A quiet Siren (no alarm, spin or sound) leaves every field
    /// as it was on both arms, unless the alarm arm would alert; both then
    /// leave the byte zero. Any other first update depends on the residue.
    fn selector47_first_silence<H: Selector47Host>(
        &mut self,
        siren: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector47Error<H::Error>> {
        let _ = (delta, &host);
        let state = self.selector47(siren)?;
        if state.alarm_160 != 0 || state.spin_15a != 0 || state.intermediate.sound_154.is_some() {
            return Err(Selector47Error::UnwrittenSilence(siren));
        }
        let family = self.family40d800(siren)?;
        if family.word_13c == family.word_140
            && (family.word_140 != 0 || self.selector47_trigger(siren)?.is_some())
        {
            return Err(Selector47Error::UnwrittenSilence(siren));
        }
        self.selector47_mut(siren)?.silenced_16d = Some(0);
        Ok(())
    }

    /// 0047FBD2..0047FDB3.
    fn selector47_alarm<H: Selector47Host>(
        &mut self,
        siren: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector47Error<H::Error>> {
        let family = self.family40d800(siren)?;
        if family.word_13c != family.word_140 {
            return Ok(());
        }
        if family.word_140 != 0 {
            let null = host
                .process_null_handle()
                .ok_or(Selector47Error::MissingProcessNullHandle(siren))?;
            host.alert(self, siren, null)
                .map_err(host_error(siren, 0x0047_fbf2))?;
        }
        let state = self.selector47_mut(siren)?;
        if state.alarm_160 != 0 {
            let left = (state.alarm_160 as i32).wrapping_sub(delta as i32);
            state.alarm_160 = if left < 0 { 0 } else { left as u32 };
            if state.spin_15a != 0xffff {
                let spin = (delta >> 5).wrapping_add(u32::from(state.spin_15a));
                state.spin_15a = if spin as i32 > 0xffff {
                    0xffff
                } else {
                    spin as u16
                };
            }
            match state.intermediate.sound_154 {
                None if !host.attract() => {
                    let created = host
                        .create_alarm(self, siren)
                        .map_err(host_error(siren, 0x0047_fc6f))?;
                    self.selector47_mut(siren)?.intermediate.sound_154 = created;
                }
                None => {}
                Some(reference) => host
                    .update_alarm(self, siren, reference, FULL, FULL)
                    .map_err(host_error(siren, 0x0047_fc83))?,
            }
        } else {
            if state.spin_15a != 0 {
                let spin = i32::from(state.spin_15a).wrapping_sub((delta >> 5) as i32);
                state.spin_15a = if spin < 0 { 0 } else { spin as u16 };
            }
            if let Some(reference) = state.intermediate.sound_154.take() {
                host.release_alarm(reference)
                    .map_err(host_error(siren, 0x0047_fcbf))?;
            }
        }
        let state = self.selector47_mut(siren)?;
        let turn = (delta >> 4).wrapping_mul(u32::from(state.spin_15a)) >> 16;
        state.turn_15c = state.turn_15c.wrapping_add(turn as u16);
        if let Some((vehicle, limit)) = self.selector47_trigger(siren)? {
            self.selector47_mut(siren)?.alarm_160 = limit.wrapping_add(TRIGGER_EXTRA);
            let source = self.actor(vehicle)?.handle;
            host.alert(self, siren, source)
                .map_err(host_error(siren, 0x0047_fdb3))?;
        }
        Ok(())
    }

    /// 0047FCC7..0047FD80: below its seconds, the first player's vehicle
    /// (0046B030) of another side within the radius (signed squares), or a
    /// linked Siren whose links are gone, triggers. Answers the vehicle and
    /// the seconds in microseconds.
    fn selector47_trigger<E>(
        &mut self,
        siren: ObjectId,
    ) -> Result<Option<(ObjectId, u32)>, Selector47Error<E>> {
        let state = self.selector47(siren)?;
        let limit = seconds::<E>(state, siren)?;
        if state.alarm_160 as i32 >= limit as i32 {
            return Ok(None);
        }
        let radius = state.radius_164 as i32;
        let Some(vehicle) = self.first_player_primary_actor()? else {
            return Ok(None);
        };
        if self.actor(vehicle)?.word_b4 == self.actor(siren)?.word_b4 {
            return Ok(None);
        }
        let from = siren_position(self, siren)?;
        let to = siren_position(self, vehicle)?;
        let squared = (0..3).fold(0_i32, |sum, axis| {
            let delta = i32::from(from[axis].wrapping_sub(to[axis]) as i16);
            sum.wrapping_add(delta.wrapping_mul(delta))
        });
        if squared > radius.wrapping_mul(radius) {
            let links = self.family40d800(siren)?;
            if links.byte_14d == 0 || links.byte_14c != 0 {
                return Ok(None);
            }
        }
        Ok(Some((vehicle, limit)))
    }
}

/// `MOVSX` of +16C times 1,000,000, wrapping.
fn seconds<E>(state: &Selector47State, siren: ObjectId) -> Result<u32, Selector47Error<E>> {
    let seconds = state
        .seconds_16c
        .ok_or(Selector47Error::UnwrittenSeconds(siren))?;
    Ok(i32::from(seconds as i8).wrapping_mul(1_000_000) as u32)
}

fn siren_position<E>(arena: &ObjectArena, actor: ObjectId) -> Result<[u16; 3], Selector47Error<E>> {
    Ok(arena
        .object(actor)?
        .base
        .position_words
        .ok_or(ObjectError::ActorPositionUnknown(actor))?)
}

/// A service failure at `address`, after the reached writes.
fn host_error<E>(siren: ObjectId, address: u32) -> impl FnOnce(E) -> Selector47Error<E> {
    move |error| Selector47Error::Host {
        siren,
        address,
        error,
    }
}

#[cfg(test)]
#[path = "selector47_tests.rs"]
mod tests;
