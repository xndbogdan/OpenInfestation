//! Arena-backed effects/queries for selector 88's recovered intro kernel.
//!
//! The retained H14 script, helpers, camera targets, membership and pose writes
//! share the live arena. Factory admission, retirement callbacks and mission
//! completion require the caller's real owners; none have successful defaults.

use crate::object_runtime::FollowTargetRequest;
use thiserror::Error;

use super::{DerivedActorClass, Selector88IntroError, Selector88IntroScript};
use crate::intro_sequence::{
    IntroActorList, IntroByte, IntroEffect, IntroError, IntroHost, IntroMembership, IntroRoute,
    IntroWord,
};
use crate::object_runtime::{
    mark_flag_change, ActorConstructionError, ActorHandle, CameraSettings, ObjectArena,
    ObjectError, ObjectId, PlayerAttachError, Selector90Byte, Selector90Error,
    Selector90MotionMode, Selector90TickRequest, Selector90Word, WorldLoadError,
};
use crate::ResidentRuntime;

mod admission;
pub use admission::{
    IntroAdmissionError, IntroAdmissionInputs, IntroCallbackHost, NativeIntroServices,
    NativeIntroServicesError,
};

/// Current process/settings inputs. These are caller-owned values, not
/// inferred from frame timing or a captured cinematic pose.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArenaIntroInputs {
    pub camera_settings: CameraSettings,
    pub settings_7c: u32,
    pub motion_mode: Selector90MotionMode,
    pub up: [f32; 3],
}

pub trait ArenaIntroServices: crate::object_runtime::TargetNullHandleSource {
    type Error;

    /// 00462750: H13 argument142 through the canonical actor factory.
    fn create_helper(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error>;
    /// 00478E00, including on-demand camera admission when its handle misses.
    fn camera(&mut self, arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error>;
    /// 0047CE90/0047D670 including reached attribution and virtual callbacks.
    fn retire_actor(&mut self, arena: &mut ObjectArena, actor: ObjectId)
        -> Result<(), Self::Error>;
    fn world_present(&mut self) -> Result<bool, Self::Error>;
    /// 00487560 with the live mission/player and linked-record cleanup owners.
    fn complete_mission(
        &mut self,
        arena: &mut ObjectArena,
        selection: u32,
        start_ordinal: u16,
        argument: u32,
    ) -> Result<(), Self::Error>;
}

pub struct ArenaIntroRequest<'a, S: ArenaIntroServices> {
    pub runtime: &'a ResidentRuntime,
    pub inputs: ArenaIntroInputs,
    pub services: &'a mut S,
}

#[derive(Debug, Error)]
pub enum ArenaIntroError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Actor(#[from] ActorConstructionError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Script(#[from] WorldLoadError),
    #[error(transparent)]
    Helper(#[from] Selector90Error),
    #[error(transparent)]
    Target(#[from] crate::object_runtime::NativeTargetError),
    #[error("intro host operation failed")]
    Host(E),
}

#[derive(Debug, Error)]
pub enum ArenaIntroTickError<E> {
    #[error(transparent)]
    Controller(#[from] Selector88IntroError),
    #[error(transparent)]
    Kernel(#[from] IntroError<ArenaIntroError<E>>),
}

pub struct ArenaIntroHost<'a, S: ArenaIntroServices> {
    arena: &'a mut ObjectArena,
    script: Selector88IntroScript,
    request: ArenaIntroRequest<'a, S>,
}

impl<'a, S: ArenaIntroServices> ArenaIntroHost<'a, S> {
    pub fn new(
        arena: &'a mut ObjectArena,
        controller: ObjectId,
        request: ArenaIntroRequest<'a, S>,
    ) -> Result<Self, Selector88IntroError> {
        let script = arena.selector88_intro_script(controller)?;
        Ok(Self {
            arena,
            script,
            request,
        })
    }
}

impl ObjectArena {
    /// The callback loop of 00478F20 followed by its header swap.
    pub fn visit_flag1_update_callbacks(
        &mut self,
        callback: impl FnMut(&mut Self, ObjectId, bool),
    ) -> Result<(), ObjectError> {
        self.visit_flag1_update_list(callback)?;
        self.swap_flag1_lists();
        Ok(())
    }

    /// The callback-loop portion of 00478F20. Move each embedded +10 node to
    /// the front of the secondary owner before dispatch, so callback
    /// membership changes and newly admitted primary nodes affect this same
    /// pass, and the next pass visits in reverse order. The caller owns
    /// disposal, player selection/reordering, the post-player 0046A260, the
    /// header swap and 004791F0 work.
    /// It must execute or explicitly skip/log each virtual callback.
    pub fn visit_flag1_update_list(
        &mut self,
        mut callback: impl FnMut(&mut Self, ObjectId, bool),
    ) -> Result<(), ObjectError> {
        use crate::object_runtime::{ObjectList, ObjectNodeKind};
        while let Some(node) = self.list(ObjectList::Flag1Primary).first().copied() {
            self.move_node_front(
                node.object,
                ObjectNodeKind::Flag1,
                ObjectList::Flag1Secondary,
            );
            // A dynamic factory may have stopped after publication during
            // this same pass. Retain its native node/fields, but never tick
            // the incomplete object as though construction had returned.
            if self.construction_is_stopped(node.object)? {
                continue;
            }
            let alternate_3c = self.object(node.object)?.base.flags & 0x8000_0000 != 0;
            callback(self, node.object, alternate_3c);
        }
        Ok(())
    }

    /// Derived virtual +CC is self (00404610) in 00428650 families including
    /// selectors10/12/34/35/36, then the caller reads that same actor's +22C.
    /// Vtables520E58/520860/51F080/527DE8/51FBB0 return null (00401300).
    pub fn intro_active_component(&self, derived: ObjectId) -> Result<bool, ObjectError> {
        Ok(match &self.derived_actor(derived)?.class {
            DerivedActorClass::Family428650(state) => state.byte_22c != 0,
            DerivedActorClass::Base
            | DerivedActorClass::Selector2(_)
            | DerivedActorClass::Selector11(_)
            | DerivedActorClass::Selector60(_)
            | DerivedActorClass::Selector77(_) => false,
        })
    }

    /// Run 00444930 against live objects, then publish all reached controller
    /// fields even on host failure. Allocation timers must already have an
    /// explicit owner through supply_selector88_intro_allocation.
    ///
    /// Controller state is local during the call. Arbitrary callbacks that
    /// inspect/mutate controller timers mid-call require a further synchronous
    /// publication owner; this adapter alone does not accept that alias domain.
    pub fn tick_selector88_intro_live<S: ArenaIntroServices>(
        &mut self,
        controller: ObjectId,
        microseconds: u32,
        request: ArenaIntroRequest<'_, S>,
    ) -> Result<(), ArenaIntroTickError<S::Error>> {
        let mut sequence = self.selector88_intro_sequence(controller)?;
        let mut host = ArenaIntroHost::new(self, controller, request)?;
        let result = sequence.tick(&mut host, microseconds);
        host.arena
            .publish_selector88_intro_sequence(controller, &sequence)?;
        result?;
        Ok(())
    }
}

impl<S: ArenaIntroServices> IntroHost for ArenaIntroHost<'_, S> {
    type Actor = ObjectId;
    type Error = ArenaIntroError<S::Error>;

    fn first_player_actor(&mut self) -> Result<Option<ObjectId>, Self::Error> {
        match self.arena.first_player() {
            Some(player) => Ok(self.arena.player_actor(player)?),
            None => Ok(None),
        }
    }

    fn route(&mut self, route: IntroRoute) -> Result<Option<ObjectId>, Self::Error> {
        Ok(self
            .arena
            .find_route_node(self.script.route_reference(route)? as u32))
    }

    fn route_successor(&mut self, route: ObjectId) -> Result<Option<ObjectId>, Self::Error> {
        Ok(self.arena.route_next(route)?)
    }

    fn create_helper(&mut self) -> Result<Option<ObjectId>, Self::Error> {
        self.request
            .services
            .create_helper(self.arena)
            .map_err(ArenaIntroError::Host)
    }

    fn camera(&mut self) -> Result<Option<ObjectId>, Self::Error> {
        self.request
            .services
            .camera(self.arena)
            .map_err(ArenaIntroError::Host)
    }

    fn actor_handle(&mut self, actor: ObjectId) -> Result<u32, Self::Error> {
        Ok(self.arena.actor(actor)?.handle.raw())
    }

    fn reference_count(&mut self, list: IntroActorList) -> Result<i32, Self::Error> {
        Ok(self.script.reference_count(list)?)
    }

    fn reference_handle(&mut self, list: IntroActorList, index: i32) -> Result<u32, Self::Error> {
        Ok(self.script.reference_handle(list, index as u32)?)
    }

    fn main_reference_handle(&mut self) -> Result<u32, Self::Error> {
        Ok(self.script.main_reference_handle()?)
    }

    fn resolve_actor(&mut self, handle: u32) -> Result<Option<ObjectId>, Self::Error> {
        Ok(self
            .arena
            .resolve_actor_handle(ActorHandle::from_raw(handle))?)
    }

    fn derived_actor(&mut self, actor: ObjectId) -> Result<Option<ObjectId>, Self::Error> {
        Ok(self.arena.actor_derived_component(actor)?)
    }

    fn active_component(&mut self, derived: ObjectId) -> Result<bool, Self::Error> {
        Ok(self.arena.intro_active_component(derived)?)
    }

    fn actor_state_c4(&mut self, actor: ObjectId) -> Result<i32, Self::Error> {
        Ok(self.arena.actor(actor)?.word_c4 as i32)
    }

    fn position_words(&mut self, actor: ObjectId) -> Result<[u16; 3], Self::Error> {
        Ok(self
            .arena
            .object(actor)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(actor))?)
    }

    fn word(&mut self, actor: ObjectId, field: IntroWord) -> Result<i32, Self::Error> {
        Ok(match field {
            IntroWord::Helper138 => self.arena.selector90(actor)?.word_138,
            IntroWord::Helper13c => self.arena.selector90(actor)?.word_13c,
            IntroWord::DerivedDelay144 => self.arena.derived_actor(actor)?.word_144,
        })
    }

    fn byte(&mut self, actor: ObjectId, field: IntroByte) -> Result<u8, Self::Error> {
        Ok(match field {
            IntroByte::Helper140 => self.arena.selector90(actor)?.byte_140,
            IntroByte::Helper142 => self.arena.selector90(actor)?.byte_142,
            IntroByte::Camera184 => self.arena.selector16(actor)?.byte_184,
            IntroByte::Camera187 => self.arena.selector16(actor)?.byte_187,
        })
    }

    fn world_present(&mut self) -> Result<bool, Self::Error> {
        self.request
            .services
            .world_present()
            .map_err(ArenaIntroError::Host)
    }

    fn effect(&mut self, effect: IntroEffect<ObjectId>) -> Result<(), Self::Error> {
        match effect {
            IntroEffect::Membership {
                actor,
                list,
                enabled,
            } => match list {
                IntroMembership::Flag0 => self.arena.set_flag0(actor, enabled)?,
                IntroMembership::Flag1 => self.arena.set_flag1(actor, enabled)?,
            },
            IntroEffect::Flags {
                actor,
                and_mask,
                or_mask,
            } => {
                let base = &mut self.arena.owned_mut(actor)?.state.base;
                mark_flag_change(base, (base.flags & and_mask) | or_mask);
            }
            IntroEffect::Dirty { actor, bits } => {
                self.arena.owned_mut(actor)?.state.base.dirty_word |= bits;
            }
            IntroEffect::Resource42 { actor, value } => {
                self.arena.actor_mut(actor)?.rotating.visual.model_word = value;
            }
            IntroEffect::RefreshVisual { actor } => self
                .arena
                .refresh_visual_model(actor, self.request.runtime)?,
            IntroEffect::PositionFrom { actor, source } => {
                let position = self.position_words(source)?;
                self.arena.copy_actor_position(actor, position)?;
            }
            IntroEffect::HelperRoute { helper, route } => self.arena.replace_selector90_route(
                helper,
                route,
                crate::object_runtime::WeightedTargetConstructorNull::Process(
                    self.request.services,
                ),
            )?,
            IntroEffect::HelperTarget { helper, handle } => {
                let target =
                    self.arena
                        .follow_target_resource(FollowTargetRequest::resolve_actor(
                            ActorHandle::from_raw(handle),
                        ))?;
                self.arena.replace_selector90_target(helper, target)?;
            }
            IntroEffect::Word {
                actor,
                field,
                value,
            } => match field {
                IntroWord::Helper138 => {
                    self.arena
                        .write_selector90_word(actor, Selector90Word::Approach138, value)?
                }
                IntroWord::Helper13c => {
                    self.arena
                        .write_selector90_word(actor, Selector90Word::Speed13c, value)?
                }
                IntroWord::DerivedDelay144 => self.arena.derived_actor_mut(actor)?.word_144 = value,
            },
            IntroEffect::Byte {
                actor,
                field,
                value,
            } => match field {
                IntroByte::Helper140 => {
                    self.arena
                        .write_selector90_byte(actor, Selector90Byte::Arrival140, value)?
                }
                IntroByte::Helper142 => {
                    self.arena
                        .write_selector90_byte(actor, Selector90Byte::Mode142, value)?
                }
                IntroByte::Camera184 => self.arena.selector16_mut(actor)?.byte_184 = value,
                IntroByte::Camera187 => self.arena.selector16_mut(actor)?.byte_187 = value,
            },
            IntroEffect::CameraPrimary { camera, handle } => {
                let target =
                    self.arena
                        .follow_target_resource(FollowTargetRequest::resolve_actor(
                            ActorHandle::from_raw(handle),
                        ))?;
                self.arena.replace_camera_target(
                    camera,
                    target,
                    self.request.inputs.camera_settings,
                )?;
            }
            IntroEffect::CameraSecondary { camera, handle } => {
                let target =
                    self.arena
                        .follow_target_resource(FollowTargetRequest::resolve_actor(
                            ActorHandle::from_raw(handle),
                        ))?;
                self.arena.replace_camera_secondary_target(camera, target)?;
            }
            IntroEffect::CameraSecondaryFromPrimary { camera } => {
                self.arena.copy_camera_primary_to_secondary(camera)?
            }
            IntroEffect::CameraParameterMode { camera, mode } => {
                self.arena.set_camera_parameter_mode(camera, mode)?
            }
            IntroEffect::DisposeController { actor } => self.arena.dispose(actor)?,
            IntroEffect::RetireSequential { actor } => self
                .request
                .services
                .retire_actor(self.arena, actor)
                .map_err(ArenaIntroError::Host)?,
            IntroEffect::TickHelper {
                helper,
                scheduler_microseconds,
            } => self.arena.tick_selector90(
                helper,
                Selector90TickRequest {
                    microseconds: scheduler_microseconds,
                    motion_mode: self.request.inputs.motion_mode,
                    settings_7c: self.request.inputs.settings_7c,
                    up: self.request.inputs.up,
                },
            )?,
            IntroEffect::PlayerPoseFromHelper { player, helper } => {
                // Helper +48 = 004026B0 always returns its persistent +44.
                self.arena.selector90(helper)?;
                let basis = self.arena.actor(helper)?.rotating.basis;
                let position = self.position_words(helper)?;
                self.arena.owned_mut(player)?.state.base.position_words = Some(position);
                self.arena.actor_mut(player)?.rotating.basis = basis;
            }
            IntroEffect::CompleteMission {
                selection,
                start_ordinal,
                argument,
            } => self
                .request
                .services
                .complete_mission(self.arena, selection, start_ordinal, argument)
                .map_err(ArenaIntroError::Host)?,
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "intro_tests.rs"]
mod tests;
