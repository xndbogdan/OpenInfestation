//! The actor classes the behavior registry 0053DBA8 names at +08: selectors
//! 25, 26, 27, 28, 38 and 68 over 00455560, and selectors 31 and 32 over
//! 00485F50. 0048D820 builds one (00460310 with an unwritten position),
//! copies its damage packet through +6C (00485F50's pair) or +68 (00455560's
//! family) and disposes it in the same call; its update, light and draw
//! lists are left before any pass runs. Their construction and that packet
//! interface are ported for all of them. The weapons' own classes are too:
//! the cannon's shot (32), the Lasers (31), the grenade (26), the row-12
//! charge (27), the mine (38), the missiles (25 and 28) and the Cranial
//! Implanter (68).

use super::{
    ActorConstructionError, ActorConstructionRequest, ActorConstructorNull, ActorExtension,
};
use crate::object_runtime::{ActorHandle, NativeHeapRef, ObjectArena, ObjectError, ObjectId};

/// The class-specific state after 0047A7A0; the shared +D8 store of
/// 00455560 lives in the shared prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BehaviorResourceState {
    pub selector: u8,
    pub class: BehaviorResourceClass,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BehaviorResourceClass {
    Family455560(Family455560State),
    Family485f50(Family485f50State),
}

/// 00455560(0x7D0): +138 = 1, +D8 = 1, +13A = 0x7D0, +13C = 0 and
/// +148 = 0x36, then the final class's stores. 004556F0 later copies the
/// payload's ten bytes at +44 to +13E..+147.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family455560State {
    pub byte_138: u8,
    pub word_13a: u16,
    pub word_13c: u16,
    /// +13E..+147: the packet DWORD +13E that 00458F50 copies, DWORD +142
    /// and WORD +146. Unknown until 004556F0.
    pub bytes_13e: Option<[u8; 10]>,
    pub dword_148: u32,
    pub tail: Family455560Tail,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Family455560Tail {
    /// Selectors 28 (00460A72) and 68 (0046141F): the exhaust ribbon +14C
    /// (00486600's, which the update extends), BYTE +150 (messages 0xB/0xC
    /// set it) and DWORD +154 (a sound handle the destructor 00456A70
    /// stops) zeroed.
    Zeroed14c {
        ribbon_14c: Option<ObjectId>,
        byte_150: u8,
        sound_154: Option<crate::process_sound::ProcessSoundRef>,
    },
    /// Selectors 26 (004609A3) and 27 (00460A02): BYTEs +14E/+14F zeroed and
    /// +150 = 5,000,000. 00455770 then stores BYTEs +14C/+14D. Selector 27
    /// also keeps the process null 0054E894 at +154, where its pair
    /// acceptance 00456010 later puts the handle of the actor it sticks to
    /// and the offset +158..+15C, allocation residue until then.
    Flags14c {
        byte_14c: Option<u8>,
        byte_14d: Option<u8>,
        byte_14e: u8,
        byte_14f: u8,
        dword_150: u32,
        handle_154: Option<ActorHandle>,
        offset_158: Option<[u16; 3]>,
    },
    /// Selector 38 (00460CD8): as selectors 26/27, plus DWORDs +15C (the
    /// type-2 target the update tracks, which the destructor 00456640
    /// deletes) and +160 (its tracked squared distance) zeroed, then
    /// +138 = 0 and +13A = 0x7D0. 00456740 then stores BYTE +154 and DWORD
    /// +158.
    Selector38 {
        byte_14c: Option<u8>,
        byte_14d: Option<u8>,
        byte_14e: u8,
        byte_14f: u8,
        dword_150: u32,
        byte_154: Option<u8>,
        dword_158: Option<u32>,
        object_15c: Option<NativeHeapRef>,
        dword_160: u32,
    },
    /// Selector 29, the Bomb (00457FE0): +138 = 0 and DWORD +14C, the fuse
    /// length, from the WORD +13A (2000). Its command 16 stores both.
    Selector29 { dword_14c: u32 },
    /// Selector 25 (00458250): the ribbon +14C, BYTE +150, DWORD +154
    /// (sound), WORD +15A (the speed), BYTE +15E and the target +160 (which
    /// the destructor 004582D0 deletes) zeroed, BYTEs +15C/+15D = 1, then
    /// +13A = 10,000. 00458330 then stores +15A, +15C, +15D and maybe +15E.
    /// The update writes WORD +158, the target's last distance.
    Selector25 {
        ribbon_14c: Option<ObjectId>,
        byte_150: u8,
        sound_154: Option<crate::process_sound::ProcessSoundRef>,
        word_158: Option<u16>,
        word_15a: u16,
        byte_15c: u8,
        byte_15d: u8,
        byte_15e: u8,
        target_160: Option<NativeHeapRef>,
    },
}

/// 00485F50: +140 = 600,000, +144 = 0, +148 = 6 and +14C = 1. Selector 31's
/// 00485480 then zeroes WORD +150 and DWORD +154 and sets +148 = 8.
/// 00485440 later stores the payload WORD +44 at +146.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Family485f50State {
    /// +138..+13C, the aim the draw 00485F90 writes and the sparks read.
    /// The constructor leaves allocation residue there; unknown until drawn.
    pub words_138: Option<[u16; 3]>,
    pub dword_140: u32,
    pub word_144: u16,
    /// The packet strength 004863D0 copies. Unknown until 00485440.
    pub word_146: Option<u16>,
    /// The packet channel 004863D0 copies.
    pub byte_148: u8,
    pub dword_14c: u32,
    /// Selector 31's WORD +150 and DWORD +154.
    pub selector31: Option<(u16, u32)>,
}

mod bomb29;
mod charge27;
mod grenade26;
mod implant;
mod laser31;
mod mine38;
mod missile;
mod remote;
mod shot32;
#[cfg(test)]
mod thrown_test_host;
pub use bomb29::{
    Bomb29Message, BombLifecycleHost, BOMB_DROPPED_HINT, BOMB_SELECTOR, BOMB_TAKEN_HINT,
};
pub use grenade26::{Grenade26Error, Grenade26Host, Grenade26Light};
pub use implant::{ImplantTakeHost, MissileSteerHost};
pub use laser31::{
    laser31_draw, Laser31Beam, Laser31DrawError, LASER_MODE, LASER_NEGATIVE_TEXTURE_SLOT,
    LASER_TEXTURE_SLOT, LASER_WIDTH_RESIDUE_APPROXIMATION,
};
pub use missile::{MissileError, MissileHost, MissileLifecycleHost, MissileLight};
pub use shot32::{Shot32Error, Shot32Host, SHOT32_LIFE_MICROS};

/// The registry's resource classes construct through this path only.
pub(crate) const BEHAVIOR_RESOURCE_SELECTORS: [u16; 8] = [25, 26, 27, 28, 31, 32, 38, 68];

impl BehaviorResourceState {
    fn new(selector: u16, null_handle: ActorHandle) -> Option<Self> {
        let family = |tail| {
            BehaviorResourceClass::Family455560(Family455560State {
                byte_138: 1,
                word_13a: 0x7d0,
                word_13c: 0,
                bytes_13e: None,
                dword_148: 0x36,
                tail,
            })
        };
        let flags = |handle_154| Family455560Tail::Flags14c {
            byte_14c: None,
            byte_14d: None,
            byte_14e: 0,
            byte_14f: 0,
            dword_150: 5_000_000,
            handle_154,
            offset_158: None,
        };
        let class = match selector {
            25 => {
                let mut class = family(Family455560Tail::Selector25 {
                    ribbon_14c: None,
                    byte_150: 0,
                    sound_154: None,
                    word_158: None,
                    word_15a: 0,
                    byte_15c: 1,
                    byte_15d: 1,
                    byte_15e: 0,
                    target_160: None,
                });
                if let BehaviorResourceClass::Family455560(state) = &mut class {
                    state.word_13a = 10_000;
                }
                class
            }
            26 => family(flags(None)),
            27 => family(flags(Some(null_handle))),
            28 | 68 => family(Family455560Tail::Zeroed14c {
                ribbon_14c: None,
                byte_150: 0,
                sound_154: None,
            }),
            38 => {
                let mut class = family(Family455560Tail::Selector38 {
                    byte_14c: None,
                    byte_14d: None,
                    byte_14e: 0,
                    byte_14f: 0,
                    dword_150: 5_000_000,
                    byte_154: None,
                    dword_158: None,
                    object_15c: None,
                    dword_160: 0,
                });
                if let BehaviorResourceClass::Family455560(state) = &mut class {
                    state.byte_138 = 0;
                }
                class
            }
            29 => {
                let mut class = family(Family455560Tail::Selector29 { dword_14c: 0x7d0 });
                if let BehaviorResourceClass::Family455560(state) = &mut class {
                    state.byte_138 = 0;
                }
                class
            }
            31 | 32 => BehaviorResourceClass::Family485f50(Family485f50State {
                words_138: None,
                dword_140: 600_000,
                word_144: 0,
                word_146: None,
                byte_148: if selector == 31 { 8 } else { 6 },
                dword_14c: 1,
                selector31: (selector == 31).then_some((0, 0)),
            }),
            _ => return None,
        };
        Some(Self {
            selector: selector as u8,
            class,
        })
    }

    /// 004556F0's copy and its callers' reads, or 00485440's WORD.
    fn initialize(
        &mut self,
        object: ObjectId,
        payload: Option<&inf_formats::BehaviorResourcePayload>,
    ) -> Result<(), ObjectError> {
        use inf_formats::{BehaviorResourcePayload as P, BehaviorResourcePayloadTail as T};
        let mismatch = ObjectError::BehaviorResourcePayload {
            object,
            selector: self.selector,
        };
        match (&mut self.class, payload) {
            (BehaviorResourceClass::Family485f50(state), Some(P::Strength44 { word_44 })) => {
                state.word_146 = Some(word_44.value);
            }
            (BehaviorResourceClass::Family455560(state), Some(P::Packet44 { bytes_44, tail })) => {
                state.bytes_13e = Some(bytes_44.value);
                match (&mut state.tail, tail) {
                    (
                        Family455560Tail::Zeroed14c { .. } | Family455560Tail::Selector29 { .. },
                        T::None,
                    ) => {}
                    (
                        Family455560Tail::Flags14c {
                            byte_14c, byte_14d, ..
                        },
                        T::Bytes50And54 { byte_50, byte_54 },
                    ) => {
                        *byte_14d = Some(byte_50.value);
                        *byte_14c = Some(byte_54.value);
                        flag_bits(&mut state.word_13a, &mut state.dword_148, byte_54.value);
                    }
                    (
                        Family455560Tail::Selector38 {
                            byte_14c,
                            byte_14d,
                            byte_154,
                            dword_158,
                            ..
                        },
                        T::Bytes50And54Dwords58And5c {
                            byte_50,
                            byte_54,
                            dword_58,
                            dword_5c,
                        },
                    ) => {
                        *byte_14d = Some(byte_50.value);
                        *byte_14c = Some(byte_54.value);
                        flag_bits(&mut state.word_13a, &mut state.dword_148, byte_54.value);
                        let present = u8::from(dword_58.value != 0);
                        *byte_154 = Some(present);
                        *dword_158 = Some(dword_5c.value);
                        if present == 0 {
                            state.dword_148 = 0x37;
                        }
                    }
                    (
                        Family455560Tail::Selector25 {
                            word_15a,
                            byte_15c,
                            byte_15d,
                            byte_15e,
                            ..
                        },
                        T::Word50Bytes52And53 {
                            word_50,
                            byte_52,
                            byte_53,
                        },
                    ) => {
                        *word_15a = word_50.value;
                        // 00458342 compares the WORD signed.
                        if (word_50.value as i16) < 600 {
                            *byte_15e = 1;
                        }
                        *byte_15c = u8::from(byte_52.value != 0);
                        *byte_15d = u8::from(byte_53.value != 0);
                    }
                    _ => return Err(mismatch),
                }
            }
            _ => return Err(mismatch),
        }
        Ok(())
    }

    /// The damage packet 004863D0 (+148 channel, then +146 strength) or
    /// 00458F50 (the DWORD at +13E) reads.
    pub fn packet_channel_148(&self) -> Option<Option<u8>> {
        match &self.class {
            BehaviorResourceClass::Family485f50(state) => Some(Some(state.byte_148)),
            BehaviorResourceClass::Family455560(_) => None,
        }
    }

    pub fn packet_strength_146(&self) -> Option<[Option<u8>; 2]> {
        match &self.class {
            BehaviorResourceClass::Family485f50(state) => Some(match state.word_146 {
                Some(word) => word.to_le_bytes().map(Some),
                None => [None; 2],
            }),
            BehaviorResourceClass::Family455560(_) => None,
        }
    }

    pub fn packet_13e(&self) -> Option<[Option<u8>; 4]> {
        match &self.class {
            BehaviorResourceClass::Family455560(state) => Some(match state.bytes_13e {
                Some(bytes) => [bytes[0], bytes[1], bytes[2], bytes[3]].map(Some),
                None => [None; 4],
            }),
            BehaviorResourceClass::Family485f50(_) => None,
        }
    }

    /// +14: 004556D0 on 00455560's family returns BYTE +138 for selector 1;
    /// 00485F50's pair keeps the shared 0047A020.
    pub fn model_value_14(&self, selector: u32) -> u16 {
        match &self.class {
            BehaviorResourceClass::Family455560(state) if selector == 1 => {
                u16::from(state.byte_138)
            }
            BehaviorResourceClass::Family455560(_) | BehaviorResourceClass::Family485f50(_) => 0,
        }
    }

    /// +6C: 00404610 (the actor) on 00485F50's pair, 00401300 elsewhere.
    pub fn component_6c(&self) -> bool {
        matches!(self.class, BehaviorResourceClass::Family485f50(_))
    }

    /// +68: 00404610 on 00455560's family, 00401300 on 00485F50's pair.
    pub fn component_68(&self) -> bool {
        matches!(self.class, BehaviorResourceClass::Family455560(_))
    }
}

/// 00455785..004557C3: bit 0 adds 8,000 to +13A; bit 2 then bit 1 replace
/// +148 with 0x37 and 0x39.
fn flag_bits(word_13a: &mut u16, dword_148: &mut u32, byte_54: u8) {
    if byte_54 & 1 != 0 {
        *word_13a = word_13a.wrapping_add(0x1f40);
    }
    if byte_54 & 4 != 0 {
        *dword_148 = 0x37;
    }
    if byte_54 & 2 != 0 {
        *dword_148 = 0x39;
    }
}

impl ObjectArena {
    /// 0046093C (25), 00460974 (26), 004609D3 (27), 00460A43 (28),
    /// 00460B15 (31), 00460B4D (32), 00460CA9 (38) and 004613F0 (68): the
    /// shared 0047A7A0, then the class constructor's stores. The Bomb's
    /// 00460AA5 (29) builds over the same family, though the registry does
    /// not name it.
    pub fn allocate_behavior_resource(
        &mut self,
        request: ActorConstructionRequest<'_>,
        selector: u16,
    ) -> Result<ObjectId, ActorConstructionError> {
        if !BEHAVIOR_RESOURCE_SELECTORS.contains(&selector) && selector != BOMB_SELECTOR {
            return Err(ActorConstructionError::BehaviorResourceSelector(selector));
        }
        let object = self.allocate_actor(request)?;
        // 00460A20 reads the process null after 0047A7A0, as +88 did.
        let null_handle = match request.constructor_null {
            ActorConstructorNull::Process(process) => process
                .process_null_handle
                .ok_or(ActorConstructionError::MissingProcessNull)?,
            ActorConstructorNull::RegistrySnapshotApproximation => self
                .actor_handles
                .as_ref()
                .ok_or(ActorConstructionError::MissingRegistry)?
                .null_handle(),
        };
        let state =
            BehaviorResourceState::new(selector, null_handle).expect("selector admitted above");
        let actor = self
            .actor_mut(object)
            .expect("allocate_actor returns an actor");
        if matches!(state.class, BehaviorResourceClass::Family455560(_)) {
            actor.word_d8 = 1;
        }
        actor.extension = ActorExtension::BehaviorResource(Box::new(state));
        Ok(object)
    }

    pub fn behavior_resource(
        &self,
        object: ObjectId,
    ) -> Result<&BehaviorResourceState, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::BehaviorResource(state) => Ok(state),
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    /// 004557F0's write for an owner's event 0xA (selectors 26, 27, 38).
    pub(in crate::object_runtime) fn mark_behavior_resource_14e(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        if let ActorExtension::BehaviorResource(state) = &mut self.actor_mut(object)?.extension {
            if let BehaviorResourceClass::Family455560(family) = &mut state.class {
                if let Family455560Tail::Flags14c { byte_14e, .. }
                | Family455560Tail::Selector38 { byte_14e, .. } = &mut family.tail
                {
                    *byte_14e = 1;
                    return Ok(());
                }
            }
        }
        Err(ObjectError::NotBehaviorResource(object))
    }

    /// The class tail of virtual +0C (00485460, 00455720, 004557D0,
    /// 00456840 or 00458380), after the shared 0047A610/0047AC10.
    pub(in crate::object_runtime) fn initialize_behavior_resource_payload(
        &mut self,
        object: ObjectId,
        payload: Option<&inf_formats::BehaviorResourcePayload>,
    ) -> Result<(), ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::BehaviorResource(state) => state.initialize(object, payload),
            _ => Err(ObjectError::NotBehaviorResource(object)),
        }
    }

    /// The class destructors before 0047AA60: 004562F0 (27) messages its
    /// +88 owner with 0x12; 00456640 (38) deletes the mine's +15C target
    /// and 004582D0 (25) the missile's +160 target. A live +154 sound, which
    /// the steered flight starts, 00456A70 (25, 28) and 00461BD0 (68) stop
    /// through the destruction's sound teardown. Selector 27's message needs
    /// the live process (`process`), which the world's disposal lends;
    /// without it, or for a reply that would change the world, a resolved
    /// owner is a boundary.
    pub(in crate::object_runtime) fn behavior_resource_teardown(
        &mut self,
        object: ObjectId,
        process: Option<&crate::process_particles::factories::ParticleFactoryProcessState>,
    ) -> Result<(), ObjectError> {
        let state = self.behavior_resource(object)?.clone();
        if let BehaviorResourceClass::Family455560(family) = &state.class {
            match family.tail {
                Family455560Tail::Selector25 { .. } => self.release_missile25_target(object)?,
                Family455560Tail::Selector38 { .. } => self.release_mine38_target(object)?,
                _ => {}
            }
            if state.selector == 27 {
                self.charge27_owner_notice(object, process)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "behavior_resource_tests.rs"]
mod tests;
