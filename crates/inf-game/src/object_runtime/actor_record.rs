//! Message 1, an actor's state record: `0047E330` writes it (through
//! `0045DBD0`, which keeps its length at `+E4`) and `0047E600` reads it into
//! an actor this machine already has. The record carries the actor's handle,
//! its type (the placement, or `0x4000` and its resource), its dirty word as
//! the record's flags, then the fields those flags name, and last the
//! class's own sync (virtual `+C0`/`+C4`, `network::sync`).

use thiserror::Error;

use inf_formats::native_basis::NativeBasisMath;

use super::actor::{ActorExtension, BehaviorResourceClass, DerivedActorClass, Family455560Tail};
use super::{mark_flag_change, ActorHandle, ObjectArena, ObjectError, ObjectId};
use crate::network::game::{
    pack_dword_vector, pack_word_vector, unpack_dword_vector, unpack_word_vector,
};
use crate::network::sync::{self, ChargeSync, FactorySync, ShotSync, SyncError, VehicleSync};

/// The record's flags: the dirty word `+38` as it goes out.
pub mod flags {
    /// `+E0`, the sender's record count.
    pub const COUNT: u16 = 0x1;
    /// `+2C`'s low WORD, the state `+C4` and the class `+B4`.
    pub const STATE: u16 = 0x2;
    /// `+88`, the owner.
    pub const OWNER: u16 = 0x4;
    /// `+70`, the holder.
    pub const HOLDER: u16 = 0x8;
    /// `+44`, the basis's three angles.
    pub const BASIS: u16 = 0x10;
    /// `+94..+9C`, the velocity.
    pub const VELOCITY: u16 = 0x20;
    /// `+8E..+92`.
    pub const SPIN: u16 = 0x40;
    /// `+32..+37`.
    pub const POSITION: u16 = 0x80;
    /// `+42`, the model.
    pub const MODEL: u16 = 0x100;
    /// The class's sync.
    pub const SYNC: u16 = 0x200;
    /// The receiver sets `+39` bit 2 (`+38` bit 10).
    pub const DRIFT: u16 = 0x400;
    /// The velocity is zero and not carried.
    pub const STILL: u16 = 0x800;
    /// `+8E..+92` are zero and not carried.
    pub const STILL_SPIN: u16 = 0x1000;
    /// `+A0`, health.
    pub const HEALTH: u16 = 0x2000;
    /// `+A2`, the maximum.
    pub const MAXIMUM: u16 = 0x4000;
}

/// The handle's 4 bytes, the type's 2 and the flags' 2.
const HEAD: usize = 8;
/// `0047EA6A..0047EAF4`: a position within this (by `0048EFC0`) eases in.
const EASE_DISTANCE: u32 = 0x80;

#[derive(Debug, Error)]
pub enum ActorRecordError<H> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Sync(#[from] SyncError),
    #[error("actor {actor:?}'s record reads its unwritten {field}")]
    Unwritten {
        actor: ObjectId,
        field: &'static str,
    },
    #[error("a record ends before offset {0}")]
    Short(usize),
    #[error("actor record host: {0:?}")]
    Host(H),
    #[error(transparent)]
    Ball(#[from] super::BallError<std::convert::Infallible>),
}

/// What the record reaches outside the actor.
pub trait ActorRecordHost {
    type Error;
    /// `0047E35B`: a held actor whose holder is gone goes to the ground
    /// (`00479DD0`) and lets go (virtual `+8C(0, NULL, +D4)`).
    fn drop_held(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error>;
    /// `0047D670(0, NULL, state)`.
    fn change_state(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        state: u32,
    ) -> Result<(), Self::Error>;
    /// `0047A110` for the stored `+42`.
    fn refresh_model(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<(), Self::Error>;
    /// Virtual `+BC` of a local holder, then `004120B0`: whether its hold
    /// has the handle (false without a component).
    fn holder_holds(
        &mut self,
        arena: &ObjectArena,
        holder: ObjectId,
        handle: ActorHandle,
    ) -> Result<bool, Self::Error>;
    /// `00492500(1, 9, &pair)`.
    fn send_release(&mut self, pair: [u32; 2]) -> Result<(), Self::Error>;
    /// `00487F10(sound, position, 0x10000, 0x10000)`: the ball's kick-off
    /// as its sync puts it back in play (`00403BC0`).
    fn ball_sound(&mut self, sound: u32, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// The class sync an actor's virtual `+C0`/`+C4` selects.
enum ClassSync {
    /// `00405580`: nothing, length 0.
    Shared,
    /// `0040DEF0`/`0040DF20`.
    Family40d800,
    /// `00486310`/`00486370`.
    Shot,
    /// `00420DA0`/`00420E70`.
    Vehicle,
    /// `0045B820`/`0045B860`.
    Banner,
    /// `00403AE0`/`00403BC0`, Powerball's ball.
    Ball,
    /// `004025C0`/`00402600`, the turret's (selector 11).
    Turret,
    /// `004560D0`/`00456130`, the charge's (selector 27).
    Charge,
    /// `00431AD0`/`00431C00`, the factory's (selector 15).
    Factory,
}

fn take<'p, H>(
    payload: &'p [u8],
    at: usize,
    count: usize,
) -> Result<&'p [u8], ActorRecordError<H>> {
    payload
        .get(at..at + count)
        .ok_or(ActorRecordError::Short(at + count))
}

fn dword<H>(payload: &[u8], at: usize) -> Result<u32, ActorRecordError<H>> {
    Ok(u32::from_le_bytes(
        take(payload, at, 4)?.try_into().unwrap(),
    ))
}

fn word<H>(payload: &[u8], at: usize) -> Result<u16, ActorRecordError<H>> {
    Ok(u16::from_le_bytes(
        take(payload, at, 2)?.try_into().unwrap(),
    ))
}

/// The bytes the fields after the head (and count) take, the class sync's
/// length WORD included, as `0047E600`'s tail adds them for a record it
/// does not apply.
pub fn skipped_length<H>(record: u16, fields: &[u8]) -> Result<usize, ActorRecordError<H>> {
    let mut length = 0;
    for (bit, size) in [
        (flags::STATE, 4),
        (flags::OWNER, 4),
        (flags::HOLDER, 4),
        (flags::BASIS, 4),
    ] {
        if record & bit != 0 {
            length += size;
        }
    }
    if record & (flags::VELOCITY | flags::STILL) == flags::VELOCITY {
        length += 4;
    }
    if record & (flags::SPIN | flags::STILL_SPIN) == flags::SPIN {
        length += 4;
    }
    for (bit, size) in [
        (flags::POSITION, 6),
        (flags::MODEL, 2),
        (flags::HEALTH, 2),
        (flags::MAXIMUM, 2),
    ] {
        if record & bit != 0 {
            length += size;
        }
    }
    if record & flags::SYNC != 0 {
        length += 2 + usize::from(word::<H>(fields, length)?);
    }
    Ok(length)
}

/// The head of a received record: its handle, type, flags, count and where
/// its fields start.
pub fn record_head<H>(payload: &[u8]) -> Result<(u32, u16, u16, u32, usize), ActorRecordError<H>> {
    let handle = dword(payload, 0)?;
    let kind = word(payload, 4)?;
    let record = word(payload, 6)?;
    if record & flags::COUNT != 0 {
        Ok((handle, kind, record, dword(payload, 8)?, HEAD + 4))
    } else {
        Ok((handle, kind, record, 1, HEAD))
    }
}

impl ObjectArena {
    /// `0045C5A0`'s writes of an actor's send countdown `+DC` and count
    /// `+E0`.
    pub fn set_actor_send_timer(
        &mut self,
        actor: ObjectId,
        countdown_dc: u32,
        count_e0: u32,
    ) -> Result<(), ObjectError> {
        let state = self.actor_mut(actor)?;
        state.word_dc = countdown_dc;
        state.word_e0 = count_e0;
        Ok(())
    }

    /// An actor's `+3C`, the clock its next record is due by.
    pub fn set_actor_deadline(&mut self, actor: ObjectId, clock: u32) -> Result<(), ObjectError> {
        self.actor(actor)?;
        self.owned_mut(actor)?.state.base.word_3c = clock;
        Ok(())
    }

    /// `0047AD90`.
    fn resolve_record_handle(
        &mut self,
        handle: ActorHandle,
    ) -> Result<Option<ObjectId>, ObjectError> {
        Ok(self
            .actor_handles
            .as_mut()
            .ok_or(ObjectError::MissingActorRegistry)?
            .resolve(handle))
    }

    fn class_sync(&self, actor: ObjectId) -> Result<ClassSync, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Selector15(_) => ClassSync::Factory,
            ActorExtension::Selector40(_) => ClassSync::Banner,
            ActorExtension::Selector59(state) if state.ball_148.is_some() => ClassSync::Ball,
            ActorExtension::Selector30(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_) => ClassSync::Family40d800,
            ActorExtension::BehaviorResource(state) => match state.selector {
                31 | 32 => ClassSync::Shot,
                27 => ClassSync::Charge,
                _ => ClassSync::Shared,
            },
            ActorExtension::Derived(state) => match state.class {
                DerivedActorClass::Selector2(_) => ClassSync::Vehicle,
                DerivedActorClass::Selector11(_) => ClassSync::Turret,
                _ => ClassSync::Shared,
            },
            _ => ClassSync::Shared,
        })
    }

    fn vehicle_sync<H>(&self, actor: ObjectId) -> Result<VehicleSync, ActorRecordError<H>> {
        let ActorExtension::Derived(state) = &self.actor(actor)?.extension else {
            return Err(ObjectError::NotDerivedActor(actor).into());
        };
        let DerivedActorClass::Selector2(vehicle) = &state.class else {
            return Err(ObjectError::NotDerivedActor(actor).into());
        };
        let unwritten = |field| ActorRecordError::Unwritten { actor, field };
        Ok(VehicleSync {
            mode_1e8: vehicle.dwords_1e8_1f4[0],
            dword_1ec: vehicle.dwords_1e8_1f4[1],
            dword_1fc: vehicle.drive.dword_1fc.ok_or_else(|| unwritten("+1FC"))?,
            byte_208: vehicle.byte_208,
            word_20e: vehicle.word_20e,
            dword_210: vehicle.dwords_210_218[0],
            dword_220: vehicle.dwords_220_224[0],
            dword_238: vehicle.dword_238,
            dword_244: vehicle.dword_244,
            // Only the ground drive writes +58C (0041A086): a vehicle that has
            // only been a skimmer, VTOL or hovercraft sends allocator residue.
            word_58c: vehicle.drive.word_58c.unwrap_or_else(|| {
                static LOGGED: std::sync::Once = std::sync::Once::new();
                LOGGED.call_once(|| {
                    eprintln!(
                        "LocalVehicleBitsApproximation: a vehicle that never drove on the \
                         ground sends its unwritten +58C; retail sends allocator residue, the \
                         port the fresh heap's zero (fresh_heap.rs)"
                    )
                });
                0
            }),
            hold: vehicle
                .handles_1a4
                .handles
                .iter()
                .map(|h| h.raw())
                .collect(),
        })
    }

    /// Virtual `+C0` into `out`: its length, or -1 when it does not fit.
    fn class_sync_out<H>(
        &self,
        actor: ObjectId,
        out: &mut [u8],
        null: ActorHandle,
    ) -> Result<i32, ActorRecordError<H>> {
        let room = out.len() as i32;
        Ok(match self.class_sync(actor)? {
            ClassSync::Shared => 0,
            ClassSync::Turret => {
                let turret = self
                    .selector11_actor(actor)
                    .map_err(|_| ObjectError::NotDerivedActor(actor))?;
                sync::turret_out(out, room, [turret.word_188, turret.word_18c])
            }
            ClassSync::Charge => {
                let fields = self.charge_sync::<H>(actor, null)?;
                sync::charge_out(out, room, fields, null.raw())
            }
            ClassSync::Factory => {
                let factory = self.selector15(actor)?;
                let words = factory.words_178_182.ok_or(ActorRecordError::Unwritten {
                    actor,
                    field: "+17E",
                })?;
                let fields = FactorySync {
                    family: [factory.intermediate.word_13c, factory.intermediate.word_140],
                    state_160: factory.word_160,
                    words_17e: [words[3], words[4], words[5]],
                    handle_188: factory.handle_188.raw(),
                };
                sync::factory_out(out, room, fields, null.raw())
            }
            ClassSync::Family40d800 => {
                let family = self.family40d800(actor)?;
                sync::family40d800_out(out, room, [family.word_13c, family.word_140])
            }
            ClassSync::Shot => {
                let shot = self.shot_sync::<H>(actor)?;
                sync::shot_out(out, room, shot)
            }
            ClassSync::Vehicle => {
                let fields = self.vehicle_sync::<H>(actor)?;
                sync::vehicle_out(out, room, &fields, null.raw())
            }
            ClassSync::Banner => self.banner_sync_out(actor, out)?,
            ClassSync::Ball => super::encode_ball(
                self.ball(actor)?,
                self.powerball_goals(),
                self.powerball() == Some(actor),
                self.powerball().is_some(),
                out,
                out.len(),
            ),
        })
    }

    /// The charge's stick: `+154` (the process null until it sticks) and
    /// the offset `+158`, which only a stuck charge has written.
    fn charge_sync<H>(
        &self,
        actor: ObjectId,
        null: ActorHandle,
    ) -> Result<ChargeSync, ActorRecordError<H>> {
        let Family455560Tail::Flags14c {
            handle_154: Some(handle),
            offset_158,
            ..
        } = &self.thrown(actor)?.tail
        else {
            return Err(ObjectError::NotBehaviorResource(actor).into());
        };
        let offset_158 = match offset_158 {
            Some(offset) => *offset,
            None if *handle == null => [0; 3],
            None => {
                return Err(ActorRecordError::Unwritten {
                    actor,
                    field: "+158",
                })
            }
        };
        Ok(ChargeSync {
            handle_154: handle.raw(),
            offset_158,
        })
    }

    fn shot_sync<H>(&self, actor: ObjectId) -> Result<ShotSync, ActorRecordError<H>> {
        let ActorExtension::BehaviorResource(state) = &self.actor(actor)?.extension else {
            return Err(ObjectError::NotBehaviorResource(actor).into());
        };
        let BehaviorResourceClass::Family485f50(shot) = &state.class else {
            return Err(ObjectError::NotBehaviorResource(actor).into());
        };
        let unwritten = |field| ActorRecordError::Unwritten { actor, field };
        Ok(ShotSync {
            words_138: shot.words_138.ok_or_else(|| unwritten("+138"))?,
            word_144: shot.word_144,
            word_146: shot.word_146.ok_or_else(|| unwritten("+146"))?,
        })
    }

    /// Virtual `+C4` over `length` bytes: the ball answers where its
    /// kick-off sounds, when its sync puts it back in play.
    fn class_sync_in<H>(
        &mut self,
        actor: ObjectId,
        payload: &[u8],
        length: i32,
        null: ActorHandle,
        local_slot: u8,
    ) -> Result<Option<[u16; 3]>, ActorRecordError<H>> {
        match self.class_sync(actor)? {
            ClassSync::Shared => {}
            ClassSync::Ball => {
                let sync = super::read_ball_sync(payload, null)?;
                return Ok(self.apply_ball_sync(actor, sync)?);
            }
            ClassSync::Banner => {
                let carrier = if length != 0 {
                    Some(ActorHandle::from_raw(dword(payload, 0)?))
                } else {
                    None
                };
                self.banner_sync_in(actor, carrier, local_slot, null)?;
            }
            ClassSync::Turret => {
                let turret = self
                    .selector11_mut(actor)
                    .map_err(|_| ObjectError::NotDerivedActor(actor))?;
                let mut fields = [turret.word_188, turret.word_18c];
                sync::turret_in(payload, &mut fields)?;
                [turret.word_188, turret.word_18c] = fields;
            }
            ClassSync::Charge => {
                let Family455560Tail::Flags14c {
                    handle_154: Some(handle),
                    offset_158,
                    ..
                } = &mut self.thrown_mut(actor)?.tail
                else {
                    return Err(ObjectError::NotBehaviorResource(actor).into());
                };
                let mut fields = ChargeSync {
                    handle_154: handle.raw(),
                    offset_158: offset_158.unwrap_or_default(),
                };
                if sync::charge_in(payload, length, &mut fields, null.raw())? != 0 {
                    *offset_158 = Some(fields.offset_158);
                }
                *handle = ActorHandle::from_raw(fields.handle_154);
            }
            ClassSync::Factory => {
                let factory = self.selector15_mut(actor)?;
                let words = factory
                    .words_178_182
                    .as_mut()
                    .ok_or(ActorRecordError::Unwritten {
                        actor,
                        field: "+17E",
                    })?;
                let mut fields = FactorySync {
                    family: [factory.intermediate.word_13c, factory.intermediate.word_140],
                    state_160: factory.word_160,
                    words_17e: [words[3], words[4], words[5]],
                    handle_188: factory.handle_188.raw(),
                };
                sync::factory_in(payload, &mut fields, null.raw())?;
                [factory.intermediate.word_13c, factory.intermediate.word_140] = fields.family;
                factory.word_160 = fields.state_160;
                [words[3], words[4], words[5]] = fields.words_17e;
                factory.handle_188 = ActorHandle::from_raw(fields.handle_188);
            }
            ClassSync::Family40d800 => {
                let family = self.family40d800_mut(actor)?;
                let mut fields = [family.word_13c, family.word_140];
                sync::family40d800_in(payload, &mut fields)?;
                [family.word_13c, family.word_140] = fields;
            }
            ClassSync::Shot => {
                let ActorExtension::BehaviorResource(state) = &mut self.actor_mut(actor)?.extension
                else {
                    return Err(ObjectError::NotBehaviorResource(actor).into());
                };
                let BehaviorResourceClass::Family485f50(shot) = &mut state.class else {
                    return Err(ObjectError::NotBehaviorResource(actor).into());
                };
                let mut fields = ShotSync {
                    words_138: shot.words_138.unwrap_or_default(),
                    word_144: shot.word_144,
                    word_146: shot.word_146.unwrap_or_default(),
                };
                let kept_146 = shot.word_146;
                sync::shot_in(payload, &mut fields)?;
                shot.words_138 = Some(fields.words_138);
                shot.word_144 = fields.word_144;
                // 00486370 writes +146 only for a set third byte.
                shot.word_146 = if payload.get(2).is_some_and(|b| b & 1 != 0) {
                    Some(fields.word_146)
                } else {
                    kept_146
                };
            }
            // 00420E70 leaves the vehicle alone for a short payload, and
            // otherwise writes every field but the hold's, which keeps its
            // slots.
            ClassSync::Vehicle if length < 10 => {}
            ClassSync::Vehicle => {
                let hold = match &self.actor(actor)?.extension {
                    ActorExtension::Derived(state) => match &state.class {
                        DerivedActorClass::Selector2(vehicle) => vehicle
                            .handles_1a4
                            .handles
                            .iter()
                            .map(|handle| handle.raw())
                            .collect(),
                        _ => return Err(ObjectError::NotDerivedActor(actor).into()),
                    },
                    _ => return Err(ObjectError::NotDerivedActor(actor).into()),
                };
                let mut fields = VehicleSync {
                    mode_1e8: 0,
                    dword_1ec: 0,
                    dword_1fc: 0,
                    byte_208: 0,
                    word_20e: 0,
                    dword_210: 0,
                    dword_220: 0,
                    dword_238: 0,
                    dword_244: 0,
                    word_58c: 0,
                    hold,
                };
                sync::vehicle_in(payload, length, &mut fields, null.raw())?;
                let state = self
                    .selector2_actor_mut(actor)
                    .map_err(|error| match error {
                        super::DerivedActorError::Object(error) => error,
                        _ => ObjectError::NotDerivedActor(actor),
                    })?;
                state.dwords_1e8_1f4[0] = fields.mode_1e8;
                state.dwords_1e8_1f4[1] = fields.dword_1ec;
                state.drive.dword_1fc = Some(fields.dword_1fc);
                state.byte_208 = fields.byte_208;
                state.word_20e = fields.word_20e;
                state.dwords_210_218[0] = fields.dword_210;
                state.dwords_220_224[0] = fields.dword_220;
                state.dword_238 = fields.dword_238;
                state.dword_244 = fields.dword_244;
                state.drive.word_58c = Some(fields.word_58c);
                for (slot, handle) in state.handles_1a4.handles.iter_mut().zip(fields.hold) {
                    *slot = ActorHandle::from_raw(handle);
                }
            }
        }
        Ok(None)
    }

    /// `0045DBD0` and `0047E330`: the actor's record into `out` given `room`,
    /// its length or -1 when it does not fit. The state's four bytes go in
    /// unchecked, as retail's do, so `out` must be the whole buffer (0x200). The record's flags
    /// are the dirty word as it stands, less the holder's fields when held by
    /// nothing, with the zero vectors marked rather than carried; the dirty
    /// word gains bit 0 unless `+2C` is exactly 1, and loses bit 10 once
    /// written. A written record's length is kept at `+E4`.
    #[allow(clippy::too_many_arguments)]
    pub fn encode_actor_record<H: ActorRecordHost>(
        &mut self,
        actor: ObjectId,
        out: &mut [u8],
        room: i32,
        null: ActorHandle,
        math: &NativeBasisMath,
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<i32, ActorRecordError<H::Error>> {
        if room < self.actor(actor)?.word_e4 as i32 {
            return Ok(-1);
        }
        let length = self.write_actor_record(actor, out, room, null, math, inverse_sine, host)?;
        if length > 0 {
            self.actor_mut(actor)?.word_e4 = length as u32;
        }
        Ok(length)
    }

    #[allow(clippy::too_many_arguments)]
    fn write_actor_record<H: ActorRecordHost>(
        &mut self,
        actor: ObjectId,
        out: &mut [u8],
        room: i32,
        null: ActorHandle,
        math: &NativeBasisMath,
        inverse_sine: &[i16; 1024],
        host: &mut H,
    ) -> Result<i32, ActorRecordError<H::Error>> {
        if room < 0xc {
            return Ok(-1);
        }
        if self.object(actor)?.base.flags != 1 {
            self.owned_mut(actor)?.state.base.dirty_word |= 1;
        }
        if self.object(actor)?.base.flags & 0x80 != 0 {
            let holder =
                self.actor(actor)?
                    .capture_handle_70
                    .ok_or(ActorRecordError::Unwritten {
                        actor,
                        field: "+70",
                    })?;
            if self.resolve_record_handle(holder)?.is_none() {
                host.drop_held(self, actor)
                    .map_err(ActorRecordError::Host)?;
            }
        }
        let base = self.object(actor)?.base.clone();
        let state = self.actor(actor)?;
        let unwritten = |field| ActorRecordError::Unwritten { actor, field };
        let kind = if (base.placement_word as i16) < 0 {
            base.resource_word.wrapping_add(0x4000)
        } else {
            base.placement_word
        };
        let mut record = base.dirty_word;
        let holder = state.capture_handle_70;
        if record & flags::HOLDER != 0 {
            let holder = holder.ok_or_else(|| unwritten("+70"))?;
            record &= if holder == null { 0xfff7 } else { 0xff0f };
        }
        if record & flags::SPIN != 0 && state.words_8e_92 == [0; 3] {
            record |= flags::STILL_SPIN;
        }
        if record & flags::VELOCITY != 0 && state.words_94_9c == [0; 3] {
            record |= flags::STILL;
        }
        out[..4].copy_from_slice(&state.handle.raw().to_le_bytes());
        out[4..6].copy_from_slice(&kind.to_le_bytes());
        out[6..8].copy_from_slice(&record.to_le_bytes());
        let mut at = HEAD;
        let mut left = (room - HEAD as i32) as u32;
        let mut put = |at: &mut usize, bytes: &[u8]| {
            out[*at..*at + bytes.len()].copy_from_slice(bytes);
            *at += bytes.len();
        };
        if record & flags::COUNT != 0 {
            put(&mut at, &state.word_e0.to_le_bytes());
            left = (room - 0xc) as u32;
        }
        if record & flags::STATE != 0 {
            put(&mut at, &(base.flags as u16).to_le_bytes());
            put(&mut at, &[state.word_c4 as u8, state.word_b4 as u8]);
            left = left.wrapping_sub(4);
        }
        if record & flags::OWNER != 0 {
            if left < 4 {
                return Ok(-1);
            }
            put(&mut at, &state.owner_handle.raw().to_le_bytes());
            left -= 4;
        }
        if record & flags::HOLDER == flags::HOLDER {
            if left < 4 {
                return Ok(-1);
            }
            put(
                &mut at,
                &holder.ok_or_else(|| unwritten("+70"))?.raw().to_le_bytes(),
            );
            left -= 4;
        }
        if record & flags::BASIS != 0 {
            if left < 4 {
                return Ok(-1);
            }
            // 0047EBD0: the basis at +44 through 00491E80.
            let packed = math.packed_angles(&state.rotating.basis, inverse_sine);
            put(&mut at, &packed.to_le_bytes());
            left -= 4;
        }
        if record & (flags::VELOCITY | flags::STILL) == flags::VELOCITY {
            if left < 4 {
                return Ok(-1);
            }
            let packed = pack_dword_vector(state.words_94_9c.map(|word| word as i32));
            put(&mut at, &packed.to_le_bytes());
            left -= 4;
        }
        if record & (flags::SPIN | flags::STILL_SPIN) == flags::SPIN {
            if left < 4 {
                return Ok(-1);
            }
            let packed = pack_word_vector(state.words_8e_92.map(|word| word as i16));
            put(&mut at, &packed.to_le_bytes());
            left -= 4;
        }
        if record & flags::POSITION != 0 {
            if left < 6 {
                return Ok(-1);
            }
            let position = base.position_words.ok_or_else(|| unwritten("+32"))?;
            for word in position {
                put(&mut at, &word.to_le_bytes());
            }
            left -= 6;
        }
        let words = [
            (flags::MODEL, state.rotating.visual.model_word),
            (flags::HEALTH, state.health),
            (flags::MAXIMUM, state.maximum_health.unwrap_or_default()),
        ];
        for (bit, value) in words {
            if record & bit == 0 {
                continue;
            }
            if bit == flags::MAXIMUM && state.maximum_health.is_none() {
                return Err(unwritten("+A2"));
            }
            if left < 2 {
                return Ok(-1);
            }
            put(&mut at, &value.to_le_bytes());
            left -= 2;
        }
        let mut length = at as i32;
        if record & flags::SYNC != 0 {
            if (left as i32) < 8 {
                return Ok(-1);
            }
            let end = at + 2 + (left as usize - 2);
            let written = self.class_sync_out(actor, &mut out[at + 2..end], null)?;
            if written as i16 == -1 {
                return Ok(-1);
            }
            out[at..at + 2].copy_from_slice(&(written as u16).to_le_bytes());
            length += 2 + i32::from(written as i16);
        }
        self.owned_mut(actor)?.state.base.dirty_word &= !flags::DRIFT;
        Ok(length)
    }

    /// `0047E600` for an actor this machine has. Its deadline `+3C` moves to
    /// the clock plus 200. A local actor's record is logged as retail's
    /// `c:\bad_lode.txt` lines and skipped, as is one older than the last
    /// (`+E0` above its count). Otherwise each field the flags name is taken,
    /// in order: the drift bit, the flags' low WORD (moving the list
    /// memberships it changes), the class, the state (through `0047D670`),
    /// owner and holder (NULL when absent; a newly held actor's local holder
    /// that does not hold it is told with message 9), the basis's angles,
    /// the velocity and spin (zero when marked), the position (eased in by
    /// twice the difference through `+8E..+92` when within 0x80, else taken),
    /// the model, health, maximum and the class's sync. Returns the bytes it
    /// used.
    #[allow(clippy::too_many_arguments)]
    pub fn apply_actor_record<H: ActorRecordHost>(
        &mut self,
        actor: ObjectId,
        payload: &[u8],
        null: ActorHandle,
        clock: u32,
        local_slot: u8,
        math: &NativeBasisMath,
        host: &mut H,
    ) -> Result<i32, ActorRecordError<H::Error>> {
        let (handle, _, record, count, start) = record_head::<H::Error>(payload)?;
        self.owned_mut(actor)?.state.base.word_3c = clock.wrapping_add(200);
        let base = &self.object(actor)?.base;
        if base.flags & 0x8000_0000 == 0 {
            let state = self.actor(actor)?;
            let id = |handle: ActorHandle| (handle.raw() >> 16) as u16 >> 11;
            eprintln!("Strange error: Received update for my own object!");
            eprintln!("My netplay id is {local_slot}");
            eprintln!("ghost ID {}", id(ActorHandle::from_raw(handle)));
            eprintln!("wo ID {}", id(state.handle));
            eprintln!(
                "wo->type is {} letternum is {} Received letternum is {count}",
                base.resource_word as i16, state.word_e0
            );
            let fields = &payload[start..];
            return Ok((start + skipped_length::<H::Error>(record, fields)?) as i32);
        }
        if self.actor(actor)?.word_e0 > count {
            let fields = &payload[start..];
            return Ok((start + skipped_length::<H::Error>(record, fields)?) as i32);
        }
        let old_flags = self.object(actor)?.base.flags;
        self.actor_mut(actor)?.word_e0 = count;
        {
            let base = &mut self.owned_mut(actor)?.state.base;
            if record & flags::DRIFT != 0 {
                base.dirty_word |= flags::DRIFT;
            } else {
                base.dirty_word &= !flags::DRIFT;
            }
        }
        let mut at = start;
        if record & flags::STATE != 0 {
            let word = word(payload, at)?;
            let state_byte = take(payload, at + 2, 2)?;
            let (state, class) = (state_byte[0], state_byte[1]);
            at += 4;
            if word != old_flags as u16 {
                let changed = u32::from(word) ^ old_flags;
                if changed & 1 != 0 {
                    self.set_flag0(actor, word & 1 != 0)?;
                }
                if changed & 2 != 0 {
                    self.set_flag1(actor, word & 2 != 0)?;
                }
                let base = &mut self.owned_mut(actor)?.state.base;
                let merged = base.flags | u32::from(word);
                mark_flag_change(base, merged);
                let cleared = base.flags & !(!u32::from(word) & 0xffff);
                mark_flag_change(base, cleared);
            }
            if u32::from(class) != self.actor(actor)?.word_b4 {
                self.owned_mut(actor)?.state.base.dirty_word |= 2;
                self.actor_mut(actor)?.word_b4 = u32::from(class);
            }
            if u32::from(state) != self.actor(actor)?.word_c4 {
                host.change_state(self, actor, u32::from(state))
                    .map_err(ActorRecordError::Host)?;
            }
        }
        let owner = if record & flags::OWNER != 0 {
            at += 4;
            ActorHandle::from_raw(dword(payload, at - 4)?)
        } else {
            null
        };
        self.actor_mut(actor)?.owner_handle = owner;
        let holder = if record & flags::HOLDER != 0 {
            at += 4;
            ActorHandle::from_raw(dword(payload, at - 4)?)
        } else {
            null
        };
        self.actor_mut(actor)?.capture_handle_70 = Some(holder);
        let held = self.object(actor)?.base.flags & 0x80;
        if old_flags & 0x80 != held {
            if let Some(local) = self.resolve_record_handle(holder)? {
                if self.object(local)?.base.flags & 0x8000_0000 == 0 {
                    let handle = ActorHandle::from_raw(handle);
                    if host
                        .holder_holds(self, local, handle)
                        .map_err(ActorRecordError::Host)?
                    {
                        let base = &mut self.owned_mut(actor)?.state.base;
                        let flags = base.flags & !0x20;
                        mark_flag_change(base, flags);
                    } else {
                        host.send_release([holder.raw(), handle.raw()])
                            .map_err(ActorRecordError::Host)?;
                    }
                }
            }
        }
        if record & flags::BASIS != 0 {
            let packed = dword(payload, at)?;
            at += 4;
            // 0047EBE0: the basis at +44 from them through 004911F0.
            let basis = &mut self.actor_mut(actor)?.rotating.basis;
            math.orient_from_packed(basis, packed);
        }
        if record & flags::VELOCITY != 0 {
            let velocity = if record & flags::STILL != 0 {
                [0; 3]
            } else {
                at += 4;
                unpack_dword_vector(dword(payload, at - 4)?).map(|value| value as u32)
            };
            self.actor_mut(actor)?.words_94_9c = velocity;
        }
        if record & flags::SPIN != 0 {
            let spin = if record & flags::STILL_SPIN != 0 {
                [0; 3]
            } else {
                at += 4;
                unpack_word_vector(dword(payload, at - 4)?).map(|value| value as u16)
            };
            self.actor_mut(actor)?.words_8e_92 = spin;
        }
        if record & flags::POSITION != 0 {
            let bytes = take(payload, at, 6)?;
            let position: [u16; 3] = std::array::from_fn(|index| {
                u16::from_le_bytes([bytes[2 * index], bytes[2 * index + 1]])
            });
            at += 6;
            let Some(current) = self.object(actor)?.base.position_words else {
                // An actor built from its resource (0x4000 and up) has no
                // position: retail compares with its allocation's residue.
                static LOGGED: std::sync::atomic::AtomicBool =
                    std::sync::atomic::AtomicBool::new(false);
                if !LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    eprintln!("RemotePositionResidueApproximation: a remote actor built from its resource has no position before its first record; retail eases from its allocation's residue when that lies within 0x80, the port takes the record's position (NETWORK_RUNTIME.md)");
                }
                self.owned_mut(actor)?.state.base.position_words = Some(position);
                return self.finish_record(actor, payload, record, at, null, local_slot, host);
            };
            let delta: [i16; 3] =
                std::array::from_fn(|index| position[index].wrapping_sub(current[index]) as i16);
            let square = delta.iter().fold(0u32, |sum, &d| {
                sum.wrapping_add((i32::from(d) * i32::from(d)) as u32)
            });
            let distance = if square & 0x8000_0000 != 0 {
                inf_formats::native_basis::isqrt_48efc0((square >> 2) as i32) << 1
            } else {
                inf_formats::native_basis::isqrt_48efc0(square as i32)
            };
            if distance > EASE_DISTANCE {
                self.owned_mut(actor)?.state.base.position_words = Some(position);
            } else {
                // 0047B510 adds twice the difference.
                let spin = &mut self.actor_mut(actor)?.words_8e_92;
                for (word, d) in spin.iter_mut().zip(delta) {
                    *word = word.wrapping_add((d as u16).wrapping_shl(1));
                }
            }
        }
        self.finish_record(actor, payload, record, at, null, local_slot, host)
    }

    /// `0047EB2B..`: the model, health, maximum and the class's sync.
    #[allow(clippy::too_many_arguments)]
    fn finish_record<H: ActorRecordHost>(
        &mut self,
        actor: ObjectId,
        payload: &[u8],
        record: u16,
        mut at: usize,
        null: ActorHandle,
        local_slot: u8,
        host: &mut H,
    ) -> Result<i32, ActorRecordError<H::Error>> {
        if record & flags::MODEL != 0 {
            let model = word(payload, at)?;
            at += 2;
            self.owned_mut(actor)?.state.base.dirty_word |= flags::MODEL;
            self.actor_mut(actor)?.rotating.visual.model_word = model;
            host.refresh_model(self, actor)
                .map_err(ActorRecordError::Host)?;
        }
        if record & flags::HEALTH != 0 {
            self.actor_mut(actor)?.health = word(payload, at)?;
            at += 2;
        }
        if record & flags::MAXIMUM != 0 {
            self.actor_mut(actor)?.maximum_health = Some(word(payload, at)?);
            at += 2;
        }
        if record & flags::SYNC == 0 {
            return Ok(at as i32);
        }
        let length = word(payload, at)?;
        let sync = take(payload, at + 2, usize::from(length))?.to_vec();
        if let Some(position) =
            self.class_sync_in(actor, &sync, i32::from(length), null, local_slot)?
        {
            host.ball_sound(super::SOUND_KICK_OFF, position)
                .map_err(ActorRecordError::Host)?;
        }
        Ok((at + 2 + usize::from(length)) as i32)
    }
}

#[cfg(test)]
#[path = "actor_record_tests.rs"]
mod tests;
