//! Complete common445CD0/445BC0 and the mode-4 walker 004476D0 (`walker`).
//! The real helper stays installed. Unknown raw child fields are demanded only
//! at their reached native reads, and no child/vector state is seeded here.
mod walker;
use super::super::DerivedActorError;
use super::{
    Selector10Helper, Selector10HelperChild, Selector10HelperChildClass, Selector10HelperClass,
};
use crate::{
    object_runtime::{ObjectArena, ObjectError, ObjectId},
    platform_audio::ResidentSoundResource,
    process_sound::{
        ProcessSoundConstructionError, ProcessSoundRecordConstructionRequest,
        ProcessSoundRecordError, ProcessSoundRecordKind, ProcessSoundRuntime,
    },
    ResidentRuntime,
};
use inf_formats::{FrontendExecutable, FrontendExecutableError};
use thiserror::Error;
pub(crate) use walker::unit_48f080;

pub struct Selector10HelperTickRequest<'a> {
    pub microseconds: u32,
    pub executable: FrontendExecutable<'a>,
    pub random_state: &'a mut u32,
    pub residency: &'a ResidentRuntime,
    pub sounds: &'a mut ProcessSoundRuntime,
}
#[derive(Debug, Error)]
pub enum Selector10HelperTickError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Table(#[from] FrontendExecutableError),
    #[error(transparent)]
    Sound(#[from] ProcessSoundRecordError),
    #[error("actor {actor:?} requires installed26C at {address:08X}")]
    MissingHelper { actor: ObjectId, address: u32 },
    #[error("actor {actor:?} requires helper +{offset:X} at {address:08X}")]
    RequiredHelperField {
        actor: ObjectId,
        offset: u16,
        address: u32,
    },
    #[error("actor {actor:?} divides by zero at {address:08X}")]
    Division { actor: ObjectId, address: u32 },
    #[error("actor {actor:?} child{index} has no extended storage at {address:08X}")]
    BaseChildExtension {
        actor: ObjectId,
        index: u32,
        address: u32,
    },
    #[error("actor {actor:?} reaches NULL/out-of-range helper child {index} at {address:08X}")]
    NullChild {
        actor: ObjectId,
        index: u32,
        address: u32,
    },
    #[error("actor {actor:?} child{index} requires raw+{offset:X} at {address:08X}")]
    RequiredChildField {
        actor: ObjectId,
        index: u32,
        offset: u16,
        address: u32,
    },
}
fn table_word(exe: FrontendExecutable<'_>, address: u32) -> Result<u16, FrontendExecutableError> {
    let b = exe.mapped(address, 2)?.bytes;
    Ok(u16::from_le_bytes([b[0], b[1]]))
}
impl ObjectArena {
    fn selector10_helper_ref(
        &self,
        actor: ObjectId,
        address: u32,
    ) -> Result<&Selector10Helper, Selector10HelperTickError> {
        self.selector10_actor(actor)?
            .helper_26c
            .as_deref()
            .ok_or(Selector10HelperTickError::MissingHelper { actor, address })
    }
    fn selector10_helper_mut(
        &mut self,
        actor: ObjectId,
        address: u32,
    ) -> Result<&mut Selector10Helper, Selector10HelperTickError> {
        self.selector10_actor_mut(actor)?
            .helper_26c
            .as_deref_mut()
            .ok_or(Selector10HelperTickError::MissingHelper { actor, address })
    }
    fn selector10_child(
        &self,
        actor: ObjectId,
        index: u32,
        address: u32,
    ) -> Result<&Selector10HelperChild, Selector10HelperTickError> {
        self.selector10_helper_ref(actor, address)?
            .children_14
            .get(index as usize)
            .and_then(Option::as_deref)
            .ok_or(Selector10HelperTickError::NullChild {
                actor,
                index,
                address,
            })
    }
    fn selector10_child_mut(
        &mut self,
        actor: ObjectId,
        index: u32,
        address: u32,
    ) -> Result<&mut Selector10HelperChild, Selector10HelperTickError> {
        self.selector10_helper_mut(actor, address)?
            .children_14
            .get_mut(index as usize)
            .and_then(Option::as_deref_mut)
            .ok_or(Selector10HelperTickError::NullChild {
                actor,
                index,
                address,
            })
    }
    fn child_raw<const N: usize>(
        &self,
        actor: ObjectId,
        index: u32,
        offset: u16,
        address: u32,
    ) -> Result<[u8; N], Selector10HelperTickError> {
        let child = self.selector10_child(actor, index, address)?;
        let mut bytes = [0; N];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let at = offset as usize + i;
            let value = match at {
                0x04..=0x13 => child.unknown_04_13[at - 0x04],
                0x18..=0x35 => child.unknown_18_35[at - 0x18],
                0x38..=0x4f => child.unknown_38_4f[at - 0x38],
                _ => None,
            };
            *byte = value.ok_or(Selector10HelperTickError::RequiredChildField {
                actor,
                index,
                offset,
                address,
            })?;
        }
        Ok(bytes)
    }
    fn child_store<const N: usize>(
        &mut self,
        actor: ObjectId,
        index: u32,
        offset: u16,
        bytes: [u8; N],
        address: u32,
    ) -> Result<(), Selector10HelperTickError> {
        let child = self.selector10_child_mut(actor, index, address)?;
        for (i, value) in bytes.into_iter().enumerate() {
            //All authenticated common stores lie wholly within18..35.
            child.unknown_18_35[offset as usize + i - 0x18] = Some(value);
        }
        Ok(())
    }
    fn child_word(
        &self,
        actor: ObjectId,
        index: u32,
        offset: u16,
        address: u32,
    ) -> Result<u16, Selector10HelperTickError> {
        Ok(u16::from_le_bytes(
            self.child_raw(actor, index, offset, address)?,
        ))
    }
    fn child_copy_xyz(
        &mut self,
        actor: ObjectId,
        index: u32,
        source: u16,
        destination: u16,
        address: u32,
        z_address: u32,
    ) -> Result<(), Selector10HelperTickError> {
        //DWORD then WORD; a later Z read failure retains the priorXY write.
        let xy = self.child_raw::<4>(actor, index, source, address)?;
        self.child_store(actor, index, destination, xy, address)?;
        let z = self.child_raw::<2>(actor, index, source + 4, z_address)?;
        self.child_store(actor, index, destination + 4, z, z_address)
    }
    /// `00425CD0`, the `+3C` of a selector-10 or 21 actor another machine
    /// owns: only its helper `+26C`'s `+4`, when it has one.
    pub fn tick_remote_selector10(
        &mut self,
        actor: ObjectId,
        request: Selector10HelperTickRequest<'_>,
    ) -> Result<(), Selector10HelperTickError> {
        if self.selector10_actor(actor)?.helper_26c.is_some() {
            self.tick_selector10_helper(actor, request)?;
        }
        Ok(())
    }

    ///Installed +04, called after native425400 physics: 00445CD0, or
    ///004476D0 for mode 4. Normal completion returns the actual count of
    ///completed or landed children.
    pub fn tick_selector10_helper(
        &mut self,
        actor: ObjectId,
        request: Selector10HelperTickRequest<'_>,
    ) -> Result<u32, Selector10HelperTickError> {
        match self.selector10_helper_ref(actor, 0x42572a)?.class {
            Selector10HelperClass::Family445ae0 => self.tick_common_445cd0(actor, request),
            Selector10HelperClass::Family446bc0 { .. } => {
                self.tick_selector10_walker(actor, request)
            }
        }
    }
    ///00445CD0, which 004476D0 also runs at gait 0.
    fn tick_common_445cd0(
        &mut self,
        actor: ObjectId,
        request: Selector10HelperTickRequest<'_>,
    ) -> Result<u32, Selector10HelperTickError> {
        let helper = self.selector10_helper_ref(actor, 0x445cd0)?;
        let table = helper.table_10;
        let count = u32::from(table_word(request.executable, table)?);
        if self.selector10_helper_ref(actor, 0x445ce6)?.byte_0c == 0 {
            for index in 0..count {
                self.selector10_child_mut(actor, index, 0x445f3f)?.word_14 &= !2;
            }
            return Ok(0);
        }
        if count == 2 {
            self.selector10_pair(actor)?;
        }
        let mut completed = 0_u32;
        for index in 0..count {
            let rotated = self
                .selector10_helper_ref(actor, 0x445ea6)?
                .word_08
                .wrapping_add(index);
            let selected = if rotated as i32 >= count as i32 {
                rotated.wrapping_sub(count)
            } else {
                rotated
            };
            if self.selector10_child_step(
                actor,
                selected,
                request.microseconds,
                request.executable,
            )? != 0
            {
                //Same physical local: +0C before PUSH EDI, +10 during loop.
                completed = completed.wrapping_add(1);
                let table = self.selector10_helper_ref(actor, 0x445ec6)?.table_10;
                let sound = table_word(request.executable, table.wrapping_add(2))?;
                if sound != 0 {
                    let random = crate::name_entry::name_entry_rng_next(request.random_state);
                    let pitch = ((random & 0xffff) >> 2).wrapping_add(0x1e000);
                    let resource = ResidentSoundResource::capture(request.residency, sound as i16);
                    let result = request.sounds.records_mut().construct(
                        ProcessSoundRecordConstructionRequest {
                            resource,
                            kind: ProcessSoundRecordKind::OneShot,
                            volume: 0x1000,
                            pitch,
                            control_flag: 0,
                            //Native actor+32 pointer is evaluated before487EC0;
                            //its actual bytes are demanded by47FFF0 after linkage.
                            position: || {
                                self.object(actor)?
                                    .base
                                    .position_words
                                    .ok_or(ObjectError::ActorPositionUnknown(actor))
                            },
                        },
                    );
                    match result {
                        Ok(_) => (),
                        Err(ProcessSoundConstructionError::Record(error)) => {
                            return Err(error.into())
                        }
                        Err(ProcessSoundConstructionError::Source(error)) => {
                            return Err(error.into())
                        }
                    }
                }
            }
        }
        let next = self
            .selector10_helper_ref(actor, 0x445f0c)?
            .word_08
            .wrapping_add(1);
        self.selector10_helper_mut(actor, 0x445f13)?.word_08 = next;
        if next as i32 >= count as i32 {
            self.selector10_helper_mut(actor, 0x445f1a)?.word_08 = 0;
        }
        Ok(completed)
    }
    fn selector10_pair(&mut self, actor: ObjectId) -> Result<(), Selector10HelperTickError> {
        if self.selector10_child(actor, 0, 0x445d06)?.word_36 != 0 {
            return Ok(());
        }
        if self.selector10_child(actor, 1, 0x445d13)?.word_36 != 0 {
            return Ok(());
        }
        let flags0 = self.selector10_child(actor, 0, 0x445d1d)?.word_14;
        if flags0 & 4 != 0 {
            return Ok(());
        }
        let flags1 = self.selector10_child(actor, 1, 0x445d2d)?.word_14;
        if flags1 & 4 != 0 {
            return Ok(());
        }
        let z1 = self.child_word(actor, 1, 0x28, 0x445d3d)?;
        let z0 = self.child_word(actor, 0, 0x28, 0x445d41)?;
        let y0 = self.child_word(actor, 0, 0x26, 0x445d45)?;
        let y1 = self.child_word(actor, 1, 0x26, 0x445d52)?;
        let x1 = self.child_word(actor, 1, 0x24, 0x445d56)?;
        let x0 = self.child_word(actor, 0, 0x24, 0x445d63)?;
        let sum = [
            x0.wrapping_add(x1),
            y0.wrapping_add(y1),
            z0.wrapping_add(z1),
        ];
        let squared = sum.into_iter().fold(0_u32, |a, v| {
            a.wrapping_add(i32::from(v as i16).wrapping_mul(i32::from(v as i16)) as u32)
        });
        let threshold = i32::from_le_bytes(self.child_raw(actor, 0, 0x40, 0x445d83)?) / 8;
        if squared <= threshold as u32 {
            return Ok(());
        }
        let velocity = self.actor(actor)?.words_8e_92.map(|v| i32::from(v as i16));
        let dot = |point: [u16; 3]| {
            point.into_iter().zip(velocity).fold(0_i32, |a, (v, w)| {
                a.wrapping_add(i32::from(v as i16).wrapping_mul(w))
            })
        };
        let chosen = if dot([x0, y0, z0]) < dot([x1, y1, z1]) {
            0
        } else {
            1
        };
        let flags = if chosen == 0 { flags0 } else { flags1 };
        self.selector10_child_mut(actor, chosen, if chosen == 0 { 0x445df9 } else { 0x445e4e })?
            .word_14 = flags | 4;
        let other = 1 - chosen;
        //Each native velocityWORD/sourceWORD pair precedes its store. No
        //generic vector snapshot obscures the reached write/read prefix.
        for (axis, offset) in [0x24_u16, 0x26, 0x28].into_iter().enumerate() {
            let velocity = self.actor(actor)?.words_8e_92[axis] as i16 >> 3;
            let reads = if chosen == 0 {
                [0x445e0d, 0x445e26, 0x445e3f]
            } else {
                [0x445e62, 0x445e7b, 0x445e94]
            };
            let stores = if chosen == 0 {
                [0x445e11, 0x445e2a, 0x445e98]
            } else {
                [0x445e66, 0x445e7f, 0x445e98]
            };
            let value = self.child_word(actor, other, offset, reads[axis])?;
            self.child_store(
                actor,
                chosen,
                offset,
                (velocity as u16).wrapping_sub(value).to_le_bytes(),
                stores[axis],
            )?;
        }
        Ok(())
    }
    fn selector10_child_step(
        &mut self,
        actor: ObjectId,
        index: u32,
        microseconds: u32,
        executable: FrontendExecutable<'_>,
    ) -> Result<u32, Selector10HelperTickError> {
        let table = self.selector10_helper_ref(actor, 0x445bd5)?.table_10;
        let coefficient =
            executable.word(table.wrapping_add(index.wrapping_mul(16)).wrapping_add(12))? as i32;
        let doubled = i64::from(microseconds as i32)
            .wrapping_mul(i64::from(coefficient))
            .wrapping_shl(1);
        let step = (doubled >> 32) as u32;
        let timer = self.selector10_child(actor, index, 0x445bee)?.word_36;
        let mut completed = 0;
        if timer != 0 {
            if u32::from(timer).wrapping_add(step) < 0x8000 {
                self.selector10_child_mut(actor, index, 0x445c0e)?.word_36 =
                    timer.wrapping_add(step as u16);
            } else {
                let flags = self.selector10_child(actor, index, 0x445c17)?.word_14 | 8;
                self.selector10_child_mut(actor, index, 0x445c1a)?.word_36 = 0;
                completed = 1;
                self.selector10_child_mut(actor, index, 0x445c2a)?.word_14 = flags;
                if flags & 2 != 0 {
                    self.child_copy_xyz(actor, index, 0x2a, 0x18, 0x445c3b, 0x445c3f)?;
                } else {
                    self.selector10_child_mut(actor, index, 0x445cb9)?.word_14 = flags & !0x14;
                }
            }
        } else {
            let flags = self.selector10_child(actor, index, 0x445c49)?.word_14;
            if flags & 2 == 0 {
                self.selector10_child_mut(actor, index, 0x445cb9)?.word_14 = flags & !0x14;
            } else if flags & 4 != 0 {
                let table = self.selector10_helper_ref(actor, 0x445c57)?.table_10;
                let count = u32::from(table_word(executable, table.wrapping_add(6))?);
                let mut clear = true;
                for i in 0..count {
                    let source = table
                        .wrapping_add(index.wrapping_mul(16))
                        .wrapping_add(0x14)
                        .wrapping_add(i);
                    let sibling = u32::from(executable.mapped(source, 1)?.bytes[0]);
                    if self.selector10_child(actor, sibling, 0x445c72)?.word_36 != 0 {
                        clear = false;
                        break;
                    }
                }
                if clear {
                    self.child_copy_xyz(actor, index, 0x18, 0x30, 0x445c89, 0x445c8d)?;
                    self.selector10_child_mut(actor, index, 0x445c95)?.word_36 =
                        if step < 0x8000 { step as u16 } else { 0x7fff };
                    self.selector10_child_mut(actor, index, 0x445cb9)?.word_14 = flags & !4;
                }
            }
        }
        self.selector10_child_mut(actor, index, 0x445cc2)?.word_14 &= !2;
        Ok(completed)
    }
}
#[cfg(test)]
mod tests;
