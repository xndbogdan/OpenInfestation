//! Common family behavior dispatcher 004294A0 and embedded target decay
//! 00482DE0. The class virtual +D8 wrappers select executable descriptor
//! tables; descriptor score/change/update, target custody and audio retain
//! required host effects. Duration and bounded +D8 entry prefixes are owned
//! here. This is not a substitute for the enclosing virtual +24.

use thiserror::Error;

use super::{
    DerivedActorError, Family428650Class, Family429e20Class, FamilyHelperHistoryError,
    FamilyHelperHistoryInputs,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};

#[path = "behavior_descriptors.rs"]
mod descriptors;
pub use descriptors::FamilyBehaviorDescriptor;

#[path = "behavior_periodic.rs"]
mod periodic;
pub use periodic::{FamilyPeriodicProbeRequest, FamilyPeriodicRecord};

/// Descriptor and periodic-scan tables selected by the native +D8 wrappers.
/// Addresses identify executable data, never H13 slots or portable pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FamilyBehaviorProfile {
    /// 00425AF0: WORD +266 indexes three pairs at 00530790.
    Family4236d0 { mode: u16 },
    /// 00468D10: virtual +7C is the admitted +140 behavior owner.
    Selector12 { behavior_present: bool },
    /// 0043F880, following its separate +210/+20C timer prefix.
    Selector35,
    /// 00435050: Selector35's +210/+20C prefix over its own tables.
    Selector34,
}

impl FamilyBehaviorProfile {
    pub fn descriptor_table(self) -> Option<(u32, i32)> {
        match self {
            Self::Family4236d0 { mode: 0 } => Some((0x0053_0740, 3)),
            Self::Family4236d0 { mode: 1 } => Some((0x0053_074c, 8)),
            Self::Family4236d0 { mode: 2 } => Some((0x0053_076c, 9)),
            Self::Family4236d0 { .. } => None,
            Self::Selector12 {
                behavior_present: false,
            } => Some((0x0053_a098, 8)),
            Self::Selector12 {
                behavior_present: true,
            } => Some((0x0053_a0b8, 9)),
            Self::Selector35 => Some((0x0053_3d38, 12)),
            Self::Selector34 => Some((0x0053_29f8, 8)),
        }
    }

    pub fn periodic_table(self) -> (u32, i32) {
        match self {
            Self::Family4236d0 { .. } => (0x0053_07b0, 3),
            Self::Selector12 { .. } => (0x0053_a0e0, 5),
            Self::Selector35 => (0x0053_3d80, 3),
            Self::Selector34 => (0x0053_2a20, 3),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FamilyBehaviorDispatchRequest {
    /// Original unsigned scheduler DWORD, in microseconds.
    pub microseconds: u32,
    /// The caller may pass null descriptor/periodic wrapper pointers in
    /// 004294A0. No table is synthesized for those branches.
    pub profile: Option<FamilyBehaviorProfile>,
    pub periodic_scan: bool,
}

/// Required callbacks retain mutable arena custody and observe each preceding
/// native write. There are deliberately no successful default implementations.
pub trait FamilyBehaviorHost {
    type Error;
    /// Copyable custody token for a host-owned temporary allocation. Failed
    /// callbacks retain that allocation until its real owner releases it.
    type PeriodicInput: Copy;

    /// One draw from the process 0048F060 stream, including host callback draws.
    fn random(&mut self) -> Result<u32, Self::Error>;
    /// Optional record+08 factory. Native 00427640 receives the actual actor.
    fn periodic_input_factory(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        factory: u32,
    ) -> Result<Option<Self::PeriodicInput>, Self::Error>;
    /// Required record+00 callback receives actor+record+04 and either the
    /// factory's owned input or native null. Only the low return byte is tested.
    fn periodic_probe(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        request: FamilyPeriodicProbeRequest,
        input: Option<Self::PeriodicInput>,
    ) -> Result<u32, Self::Error>;
    ///005116F4 releases an admitted temporary input after callback success,
    /// including a false callback result. Null factories skip this service.
    fn release_periodic_input(
        &mut self,
        arena: &mut ObjectArena,
        input: Self::PeriodicInput,
    ) -> Result<(), Self::Error>;
    /// Descriptor virtual +04, via 00425C30.
    fn score(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        current: bool,
        bias: i32,
    ) -> Result<i32, Self::Error>;
    /// Unowned actor virtual +110, used only for a noncurrent candidate.
    /// The dispatcher executes recovered family4236d0/selector12/selector35
    /// methods through random and their live retained allocations instead.
    fn candidate_bias(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<i32, Self::Error>;
    /// Descriptor virtual +08, through 0043F990.
    fn change(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        entering: bool,
    ) -> Result<(), Self::Error>;
    /// Descriptor virtual +0C, through 0043F9B0.
    fn update(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        microseconds: u32,
    ) -> Result<(), Self::Error>;
    /// The native positive sound-slot branch checks registration, lazily
    /// acquires it, invokes 00487F10(0x10000,0x10000), then 004923C0.
    fn occasional_sound(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        slot: u16,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FamilyBehaviorError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error("actor {actor:?} requires the unwritten native +{offset:03X} value")]
    Uninitialized { actor: ObjectId, offset: u16 },
    #[error("actor {actor:?} requires process null handle 0054E894 at timer expiry")]
    MissingProcessNullHandle { actor: ObjectId },
    #[error("actor {0:?} has no proven family behavior profile")]
    UnsupportedProfile(ObjectId),
    #[error("actor {0:?} requires a nonnull descriptor wrapper at this callback")]
    MissingDescriptors(ObjectId),
    #[error("actor {actor:?} requires unowned descriptor index {index} in {profile:?}")]
    DescriptorIndex {
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
    },
    #[error("actor {actor:?} requires unowned periodic index {index} in {profile:?}")]
    PeriodicIndex {
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: u16,
    },
    #[error(transparent)]
    Helper(#[from] FamilyHelperHistoryError),
    #[error("actor {actor:?} has unsupported embedded target count {count}")]
    TargetCount { actor: ObjectId, count: u32 },
    #[error("family behavior host failed at {effect}: {error}")]
    Host { effect: &'static str, error: E },
}

impl ObjectArena {
    /// Explicit supplied callback state for intrinsic fixture scheduling only.
    /// Production mutation methods retain their existing visibility and policy.
    #[cfg(test)]
    pub(crate) fn with_supplied_family_dispatch_actor<R>(
        &mut self,
        actor: ObjectId,
        mutate: impl FnOnce(&mut crate::object_runtime::ActorState) -> R,
    ) -> Result<R, ObjectError> {
        Ok(mutate(self.actor_mut(actor)?))
    }

    pub fn family_behavior_profile(
        &self,
        actor: ObjectId,
    ) -> Result<FamilyBehaviorProfile, FamilyBehaviorError<std::convert::Infallible>> {
        self.family_behavior_profile_inner(actor)
    }

    fn family_behavior_profile_inner<E>(
        &self,
        actor: ObjectId,
    ) -> Result<FamilyBehaviorProfile, FamilyBehaviorError<E>> {
        match &self.family428650_actor(actor)?.class {
            Family428650Class::Family429e20(state) => match &state.class {
                Family429e20Class::Family4236d0(state) => {
                    let profile = FamilyBehaviorProfile::Family4236d0 {
                        mode: state.word_266,
                    };
                    if profile.descriptor_table().is_none() {
                        return Err(FamilyBehaviorError::UnsupportedProfile(actor));
                    }
                    Ok(profile)
                }
                Family429e20Class::Selector12(_) => Ok(FamilyBehaviorProfile::Selector12 {
                    behavior_present: self.derived_actor(actor)?.behavior_140.is_some(),
                }),
                _ => Err(FamilyBehaviorError::UnsupportedProfile(actor)),
            },
            Family428650Class::Selector35(_) => Ok(FamilyBehaviorProfile::Selector35),
            Family428650Class::Selector34(_) => Ok(FamilyBehaviorProfile::Selector34),
            _ => Err(FamilyBehaviorError::UnsupportedProfile(actor)),
        }
    }

    /// Actual virtual+D8 entries 00425AF0/00468D10/0043F880. Wrapper tables
    /// are selected from the concrete actor before its helper/timer prefix.
    /// The helper 00483340 prefix fades actual records; constructor-empty
    /// count+08 bypasses every other allocation or process field.
    pub fn dispatch_actor_family_behavior<H: FamilyBehaviorHost>(
        &mut self,
        actor: ObjectId,
        microseconds: u32,
        helper_inputs: FamilyHelperHistoryInputs,
        host: &mut H,
    ) -> Result<(), FamilyBehaviorError<H::Error>> {
        let profile = self.family_behavior_profile_inner(actor)?;
        match profile {
            // 0043F880 and 00435050 share this prefix byte for byte.
            FamilyBehaviorProfile::Selector35 | FamilyBehaviorProfile::Selector34 => {
                let timer = self.family428650_actor(actor)?.word_210;
                if timer != 0 {
                    if (timer as i32) > (microseconds as i32) {
                        self.family428650_actor_mut(actor)?.word_210 =
                            timer.wrapping_sub(microseconds);
                    } else {
                        self.family428650_actor_mut(actor)?.word_210 = 0;
                        // 0043F8D7 clears +210 before 0043F8E1 reads the
                        // actual process 0054E894. Registry lookup has a
                        // separate NULL identity and cannot supply this store.
                        let null = helper_inputs
                            .process_null_handle
                            .ok_or(FamilyBehaviorError::MissingProcessNullHandle { actor })?;
                        self.family428650_actor_mut(actor)?.word_20c = null.raw();
                    }
                }
            }
            FamilyBehaviorProfile::Family4236d0 { .. }
            | FamilyBehaviorProfile::Selector12 {
                behavior_present: true,
            } => {
                if self.family428650_actor(actor)?.helper_218.is_some() {
                    self.fade_family_helper_history(actor, microseconds, helper_inputs)?;
                }
            }
            FamilyBehaviorProfile::Selector12 {
                behavior_present: false,
            } => {}
        }
        self.dispatch_family_behavior(
            actor,
            FamilyBehaviorDispatchRequest {
                microseconds,
                profile: Some(profile),
                periodic_scan: true,
            },
            host,
        )
    }

    /// 004294A0. Native decisions reread mutable actor fields after callbacks;
    /// failures preserve every reached field and process RNG draw. Unknown
    /// allocator values fail only at the read that actually requires them.
    pub fn dispatch_family_behavior<H: FamilyBehaviorHost>(
        &mut self,
        actor: ObjectId,
        request: FamilyBehaviorDispatchRequest,
        host: &mut H,
    ) -> Result<(), FamilyBehaviorError<H::Error>> {
        self.family428650_actor(actor)?;
        let time = request.microseconds;
        let lifecycle = self.actor(actor)?.word_c4;
        let mut selected_bias = None;
        let selected = if lifecycle == 0 {
            0
        } else if lifecycle == 1 {
            1
        } else if self.family428650_actor(actor)?.word_22e & 2 != 0
            && self.family428650_actor(actor)?.word_184 > 1
        {
            self.family428650_actor(actor)?.word_184
        } else {
            let Some(profile) = request.profile else {
                return Ok(());
            };
            let (_, descriptor_count) = profile
                .descriptor_table()
                .ok_or(FamilyBehaviorError::UnsupportedProfile(actor))?;
            if self.family428650_actor(actor)?.word_22e & 4 != 0 {
                let index = self.family428650_actor(actor)?.word_184;
                if index >= 0 {
                    host.change(self, actor, profile, index, false)
                        .map_err(|error| FamilyBehaviorError::Host {
                            effect: "forced behavior exit",
                            error,
                        })?;
                    self.family428650_actor_mut(actor)?.word_184 = -1;
                }
            }
            if self.family428650_actor(actor)?.word_22e & 0x0d != 0 {
                self.family428650_actor_mut(actor)?.word_21c = 0;
            }
            self.decay_family_target_entries(actor, time)?;
            let timer = self.family428650_actor(actor)?.word_21c;
            if timer > time {
                self.family428650_actor_mut(actor)?.word_21c = timer.wrapping_sub(time);
            } else if request.periodic_scan {
                let (_, count) = profile.periodic_table();
                let full_scan = self.family428650_actor(actor)?.word_22e & 8 != 0;
                let random = host.random().map_err(|error| FamilyBehaviorError::Host {
                    effect: "periodic RNG",
                    error,
                })?;
                let state = self.family428650_actor_mut(actor)?;
                if full_scan {
                    state.word_22e |= 1;
                }
                state.word_21c = (random & 0xffff)
                    .wrapping_mul(4)
                    .wrapping_add(if full_scan { 1_000_000 } else { 250_000 });
                for _ in 0..if full_scan { count } else { 1 } {
                    let previous = self.family428650_actor(actor)?.word_220.ok_or(
                        FamilyBehaviorError::Uninitialized {
                            actor,
                            offset: 0x220,
                        },
                    )?;
                    // 004295B3 increments a WORD before zero extension and IDIV.
                    let index = (i32::from(previous.wrapping_add(1)) % count) as u16;
                    self.family428650_actor_mut(actor)?.word_220 = Some(index);
                    if self.family_behavior_periodic_probe(actor, profile, index, host)? {
                        self.family428650_actor_mut(actor)?.word_22e |= 1;
                    }
                }
            }
            let timer = if self.family428650_actor(actor)?.word_22e & 1 == 0 {
                Some(self.family_behavior_word(actor, 0x19c)? as i32)
            } else {
                None
            };
            if timer.is_some_and(|timer| timer > time as i32) {
                self.family428650_actor_mut(actor)?.words_188_19c[5] =
                    Some((timer.unwrap() as u32).wrapping_sub(time));
                self.family428650_actor(actor)?.word_184
            } else {
                let random = host.random().map_err(|error| FamilyBehaviorError::Host {
                    effect: "occasional sound RNG",
                    error,
                })?;
                if random & 3 == 0 {
                    let random = host.random().map_err(|error| FamilyBehaviorError::Host {
                        effect: "sound selection RNG",
                        error,
                    })?;
                    let index = usize::from(random & 1 == 0);
                    let bindings = self.derived_actor(actor)?.sound_words_17c_182.ok_or(
                        FamilyBehaviorError::Uninitialized {
                            actor,
                            offset: 0x17c + (index as u16) * 2,
                        },
                    )?;
                    let slot = bindings[index];
                    if slot as i16 > 0 {
                        host.occasional_sound(self, actor, slot).map_err(|error| {
                            FamilyBehaviorError::Host {
                                effect: "occasional sound",
                                error,
                            }
                        })?;
                    }
                }
                self.family428650_actor_mut(actor)?.word_22e &= 0xfffe;
                let current = self.family428650_actor(actor)?.word_184;
                if current >= 0 {
                    let duration = self.family_behavior_duration(
                        actor,
                        profile,
                        current,
                        "current duration",
                        host,
                    )?;
                    self.family428650_actor_mut(actor)?.words_188_19c[5] = Some(duration);
                }
                let mut selected = -1;
                let mut best = 0;
                for index in 2..descriptor_count {
                    let current = index == self.family428650_actor(actor)?.word_184;
                    let bias = if current {
                        let elapsed = self.family_behavior_word(actor, 0x188)? as i32;
                        if elapsed < 7_500_000 {
                            let retained = self.family_behavior_word(actor, 0x204)? as i32;
                            retained
                                .wrapping_mul(2)
                                .wrapping_sub((elapsed / 234_375).wrapping_mul(retained) >> 4)
                        } else {
                            0
                        }
                    } else {
                        self.family_behavior_candidate_bias(actor, host)?
                    };
                    let score = host
                        .score(self, actor, profile, index, current, bias)
                        .map_err(|error| FamilyBehaviorError::Host {
                            effect: "behavior score",
                            error,
                        })?;
                    if score > best {
                        selected = index;
                        best = score;
                        selected_bias = Some(bias as u32);
                    }
                }
                selected
            }
        };
        self.family428650_actor_mut(actor)?.word_22e &= 0xffe0;
        let current = self.family428650_actor(actor)?.word_184;
        if selected != current {
            let profile = request
                .profile
                .ok_or(FamilyBehaviorError::MissingDescriptors(actor))?;
            if current >= 0 {
                host.change(self, actor, profile, current, false)
                    .map_err(|error| FamilyBehaviorError::Host {
                        effect: "behavior exit",
                        error,
                    })?;
            }
            self.family428650_actor_mut(actor)?.word_228 = if selected < 0 {
                0x0053_1220
            } else {
                0x0053_1230
            };
            self.clear_family_behavior_targets(actor)?;
            if selected < 0 {
                let state = self.family428650_actor_mut(actor)?;
                state.words_204_208[0] = Some(0);
                state.word_184 = -1;
                state.words_188_19c[5] = Some(1_000_000);
            } else {
                // Forced lifecycle 0/1 carries an opaque stack-wrapper pointer
                // at native +204. It is not an invented portable address.
                self.family428650_actor_mut(actor)?.words_204_208[0] = selected_bias;
                host.change(self, actor, profile, selected, true)
                    .map_err(|error| FamilyBehaviorError::Host {
                        effect: "behavior entry",
                        error,
                    })?;
                self.family428650_actor_mut(actor)?.word_184 = selected;
                let duration = self.family_behavior_duration(
                    actor,
                    profile,
                    selected,
                    "entered duration",
                    host,
                )?;
                self.family428650_actor_mut(actor)?.words_188_19c[5] = Some(duration);
            }
            self.family428650_actor_mut(actor)?.words_188_19c[0] = Some(0);
        }
        let current = self.family428650_actor(actor)?.word_184;
        if current >= 0 {
            let profile = request
                .profile
                .ok_or(FamilyBehaviorError::MissingDescriptors(actor))?;
            host.update(self, actor, profile, current, time)
                .map_err(|error| FamilyBehaviorError::Host {
                    effect: "behavior update",
                    error,
                })?;
        }
        let elapsed = self.family_behavior_word(actor, 0x188)?;
        self.family428650_actor_mut(actor)?.words_188_19c[0] = Some(elapsed.wrapping_add(time));
        Ok(())
    }

    /// The supplied wrapper selects duration independently of the actor vtable.
    /// 0043F9D0 uses the actual singleton descriptor virtual+10; its negative
    /// branch skips that table.004690F0 ignores every index. Each reached
    /// policy performs exactly one process draw before its unsigned arithmetic.
    fn family_behavior_duration<H: FamilyBehaviorHost>(
        &mut self,
        actor: ObjectId,
        profile: FamilyBehaviorProfile,
        index: i32,
        effect: &'static str,
        host: &mut H,
    ) -> Result<u32, FamilyBehaviorError<H::Error>> {
        let (base, shift) =
            profile
                .duration_kernel(index)
                .ok_or(FamilyBehaviorError::DescriptorIndex {
                    actor,
                    profile,
                    index,
                })?;
        let random = host
            .random()
            .map_err(|error| FamilyBehaviorError::Host { effect, error })?;
        Ok((random & 0xffff).wrapping_add(base).wrapping_shl(shift))
    }

    /// Actor virtual+110 uses the actual class, regardless of the supplied
    /// descriptor wrapper.004350E0/00468E00/0043F960 share the same arithmetic
    /// but read distinct retained WORDs, after consuming the actual RNG draw.
    fn family_behavior_candidate_bias<H: FamilyBehaviorHost>(
        &mut self,
        actor: ObjectId,
        host: &mut H,
    ) -> Result<i32, FamilyBehaviorError<H::Error>> {
        let recovered = match &self.family428650_actor(actor)?.class {
            Family428650Class::Family429e20(state) => matches!(
                state.class,
                Family429e20Class::Selector12(_) | Family429e20Class::Family4236d0(_)
            ),
            Family428650Class::Selector35(_) | Family428650Class::Selector34(_) => true,
            _ => false,
        };
        if !recovered {
            return host
                .candidate_bias(self, actor)
                .map_err(|error| FamilyBehaviorError::Host {
                    effect: "candidate bias +110",
                    error,
                });
        }
        let random = host.random().map_err(|error| FamilyBehaviorError::Host {
            effect: "candidate bias +110",
            error,
        })?;
        // Reread +23C after random returns: shared allocation bytes may have
        // changed, and missing custody must preserve the preceding draw.
        let (allocation, retained_offset, word_offset, missing) =
            match &self.family428650_actor(actor)?.class {
                Family428650Class::Family429e20(state) => match &state.class {
                    Family429e20Class::Selector12(state) => {
                        let retained = state
                            .retained_23c
                            .as_ref()
                            .ok_or(DerivedActorError::MissingSelector12Payload)?;
                        (
                            retained.allocation(),
                            retained.allocation_offset,
                            0x10,
                            DerivedActorError::MissingSelector12Payload,
                        )
                    }
                    Family429e20Class::Family4236d0(state) => {
                        let retained = state
                            .retained_23c
                            .as_ref()
                            .ok_or(DerivedActorError::MissingFamily4236d0Payload)?;
                        (
                            retained.allocation(),
                            retained.allocation_offset,
                            0x0c,
                            DerivedActorError::MissingFamily4236d0Payload,
                        )
                    }
                    _ => unreachable!("recovered class selected before RNG"),
                },
                Family428650Class::Selector35(state) => {
                    let retained = state
                        .retained_23c
                        .as_ref()
                        .ok_or(DerivedActorError::MissingSelector35Payload)?;
                    (
                        retained.allocation(),
                        retained.allocation_offset,
                        0x0e,
                        DerivedActorError::MissingSelector35Payload,
                    )
                }
                // 004350E0 is 0043F960's twin over retained WORD +0C.
                Family428650Class::Selector34(state) => {
                    let retained = state
                        .retained_23c
                        .as_ref()
                        .ok_or(DerivedActorError::MissingSelector34Payload)?;
                    (
                        retained.allocation(),
                        retained.allocation_offset,
                        0x0c,
                        DerivedActorError::MissingSelector34Payload,
                    )
                }
                _ => unreachable!("recovered class selected before RNG"),
            };
        let offset = retained_offset.checked_add(word_offset).ok_or(missing)?;
        let strength = allocation.word(offset).map_err(DerivedActorError::from)?;
        Ok(((random & 0xffff).wrapping_mul(u32::from(strength)) as i32) >> 16)
    }

    fn family_behavior_word<E>(
        &self,
        actor: ObjectId,
        offset: u16,
    ) -> Result<u32, FamilyBehaviorError<E>> {
        let state = self.family428650_actor(actor)?;
        let value = match offset {
            0x188..=0x19c => state.words_188_19c[usize::from((offset - 0x188) / 4)],
            0x204 => state.words_204_208[0],
            _ => unreachable!("private native field lookup"),
        };
        value.ok_or(FamilyBehaviorError::Uninitialized { actor, offset })
    }

    fn decay_family_target_entries<E>(
        &mut self,
        actor: ObjectId,
        time: u32,
    ) -> Result<(), FamilyBehaviorError<E>> {
        let state = self.family428650_actor_mut(actor)?;
        // 00482DE0 admits signed-positive count only; the fixed embedded
        // allocation holds four twelve-byte entries.
        if state.count_1a0 as i32 <= 0 {
            return Ok(());
        }
        if state.count_1a0 > 4 {
            return Err(FamilyBehaviorError::TargetCount {
                actor,
                count: state.count_1a0,
            });
        }
        for entry in &mut state.entries_1a4[..state.count_1a0 as usize] {
            // The +08 writer precedes +04 in retail.
            entry.word_08 = if entry.word_08 == 0 || entry.word_08 <= time {
                1
            } else {
                entry.word_08 - time
            };
            entry.word_04 = if entry.word_04 == 0 || entry.word_04 <= time {
                1
            } else {
                entry.word_04 - time
            };
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "behavior_dispatch_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "behavior_callback_native_tests.rs"]
mod native_callback_tests;

#[cfg(test)]
#[path = "behavior_descriptor_tests.rs"]
mod native_descriptor_tests;
