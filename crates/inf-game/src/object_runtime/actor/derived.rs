//! Actor family 00425E60/00426C10/00426030, final base vtable 00520E58.
//! Its constructor consumes one process RNG draw after shared actor publication.

use inf_formats::{
    native_basis::NativeBasisMath, InstanceCommandPayload, InstanceCommandTarget,
    InstanceCommandView,
};
use thiserror::Error;

use super::allocation::ActorAllocationDwordCarry;
use super::behavior::{build_actor_behavior, ActorBehaviorState};
use super::{
    ActorCommandContext, ActorCommandError, ActorConstructionError, ActorConstructionRequest,
    ActorExtension, ActorInitializationRequest,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::{name_entry::name_entry_rng_next, ResidentRuntime};

mod attack;
mod avoidance;
mod behavior_dispatch;
mod collision_guard;
mod contact;
pub use contact::{
    ActorContactEnvironment, ActorContactError, ActorContactHeaderHost, ActorContactOutput,
    ActorContactRequest, ActorContactStackPolicy,
};
mod selector36_audio;
pub use selector36_audio::{
    ActorAudioCameraHost, Selector36PersistentAudioError, Selector36PersistentAudioRequest,
};
mod selector36_motion;
pub use selector36_motion::{
    Selector36MotionError, Selector36MotionNextPhase, Selector36MotionRequest,
};
mod descriptor_attack_update;
mod descriptor_dormant;
mod descriptor_escort;
mod descriptor_follow_player;
pub use descriptor_follow_player::FamilyFollowPlayerError;
mod descriptor_herding;
pub use descriptor_herding::FamilyHerdingError;
mod descriptor_motion;
pub use descriptor_dormant::{FamilyDormantError, FamilyDormantRequest};
mod descriptor_object_target;
mod descriptor_points;
mod descriptor_reselection;
mod descriptor_roam;
pub use descriptor_reselection::{
    FamilyEvadeEntryKind, FamilyEvadeEntryRequest, FamilyTimedDescriptorError,
    FamilyTimedDescriptorKind, FamilyTimedDescriptorRequest,
};
mod family4236d0;
mod family_dying;
pub use family_dying::{
    FamilyDyingAudio, FamilyDyingEffect, FamilyDyingError, FamilyDyingHost, FamilyDyingRequest,
    FamilyDyingSpawnRequest,
};
mod family428650;
mod family429e20;
mod family_actor_list_selection;
mod family_helper_history;
mod family_pair;
pub use family_pair::{FamilyPairReactionError, FamilyPairReactionRequest, FamilyRescueAccept};
mod family_selection;
mod family_targets;
mod family_update_services;
pub(in crate::object_runtime::actor) mod kinematics;
pub(crate) use kinematics::ftol as native_trunc_low32;
pub(in crate::object_runtime::actor) use kinematics::{
    transform_local_dwords, transform_world_dwords,
};
mod physics;
mod player_drive;
pub use player_drive::{FamilyIntentError, FamilyPlayerWeaponError};
mod probe_miss;
mod selector10;
mod selector10_tail;
pub use selector10_tail::{
    Selector10TailNextPhase, Selector10TailPrefixError, Selector10TailPrefixRequest,
};
mod selector10_motion;
pub use selector10_motion::{
    Selector10MotionError, Selector10MotionNextPhase, Selector10MotionRequest,
};
mod selector10_audio;
pub use selector10_audio::Selector10PersistentAudioError;
pub(crate) use selector10_helper::unit_48f080;
pub use selector10_helper::{
    segment_probe_43a380, selector10_custom_425870, SegmentProbe, Selector10CustomInputs,
    Selector10CustomServices, Selector10HelperTickError, Selector10HelperTickRequest,
    Selector10LegError, Selector10LegGround, WheelGround,
};
mod selector10_helper;
mod selector11;
mod selector12;
mod selector12_descriptor_changes;
mod selector12_point_update;
mod selector12_preferred_update;
pub use selector12_point_update::{Selector12PointUpdateError, Selector12PointUpdateRequest};
pub use selector12_preferred_update::{
    Selector12PreferredUpdateError, Selector12PreferredUpdateRequest,
};
mod selector12_motion;
mod selector12_tail;
pub use selector12_descriptor_changes::{
    Selector12DescriptorChangeError, Selector12DescriptorChangeKind,
    Selector12DescriptorChangeRequest,
};
mod selector2;
mod selector34;
mod selector35;
mod selector36;
mod selector36_child;
mod selector36_tail_prefix;
pub use selector36_tail_prefix::{
    ProcessActorMotionState, Selector36ChildServiceError, Selector36TailNextPhase,
    Selector36TailPrefixError, Selector36TailPrefixRequest,
};
mod selector60;
pub use selector60::{
    Selector60SpawnResult, Selector60SpawnRow, Selector60UpdateError, Selector60UpdateHost,
};
mod selector77;
mod target_producers;
mod terrain_probe;
mod update_prefix;
pub use attack::{FamilyAttackError, FamilyAttackHost};
pub use avoidance::{
    FamilyAvoidanceError, FamilyAvoidanceHost, FamilyObstacleProbeHost, FamilyObstacleScanError,
};
pub use behavior_dispatch::{
    FamilyBehaviorDescriptor, FamilyBehaviorDispatchRequest, FamilyBehaviorError,
    FamilyBehaviorHost, FamilyBehaviorProfile, FamilyPeriodicProbeRequest, FamilyPeriodicRecord,
};
pub use collision_guard::{
    FamilyCollisionGuardError, FamilyCollisionGuardHost, FamilyCollisionGuardRequest,
};
pub use descriptor_attack_update::{
    FamilyAttackUpdateError, FamilyAttackUpdateHost, FamilyAttackUpdateRequest,
};
pub use descriptor_motion::{
    FamilyDescriptorScoreError, FamilyDescriptorScoreKind, FamilyDescriptorScoreRequest,
    FamilyDescriptorScoreRng,
};
pub use descriptor_object_target::{
    FamilyObjectTargetChangeError, FamilyObjectTargetChangeRequest, FamilyObjectTargetUpdateError,
};
pub use descriptor_points::{
    FamilyPointTargetChangeError, FamilyPointTargetChangeKind, FamilyPointTargetChangeRequest,
};
pub use descriptor_roam::{FamilyRoamUpdateError, FamilyRoamUpdateRequest};
pub use family4236d0::{
    Family4236d0Class, Family4236d0InitializationRequest, Family4236d0MotionError,
    Family4236d0MotionRequest, Family4236d0State, RetainedActorAllocation,
};
pub use family428650::{
    Family428650Class, Family428650State, FamilyHandleEntry, FamilyHelper482ff0,
};
pub use family429e20::{Family429e20Class, Family429e20State};
pub use family_actor_list_selection::FamilyActorListSelectionError;
pub use family_helper_history::{
    FamilyAttackUpdateServices, FamilyHelperHistoryError, FamilyHelperHistoryInputs,
    FamilyHelperHistoryRecord, FamilyHelperQueryRequest, FamilyHelperRecordRequest,
};
pub use family_selection::{
    FamilySelectionCandidate, FamilySelectionError, FamilySelectionHost, FamilySelectionRequest,
    FamilySelectionServices, FamilyTargetScanError, FamilyTargetScanOutputs,
    FamilyTargetScanRequest, FamilyTargetSelection, FamilyTargetTable, FamilyTargetTableError,
};
pub use family_targets::FamilyTargetSlot;
pub use family_update_services::{NativeFamilyUpdateError, NativeFamilyUpdateHost};
pub use kinematics::{
    ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorLateralMotionRequest,
    ActorLateralMotionTable, ActorMotionError, ActorMotionTables,
};
pub use physics::ActorPhysicsRequest;
pub use probe_miss::{FamilyTerrainProbeMissError, FamilyTerrainProbeMissHost};
pub(crate) use selector10::selector21_value;
pub use selector10::Selector10State;
pub use selector10_helper::{
    Selector10Helper, Selector10HelperChild, Selector10HelperChildClass, Selector10HelperClass,
};
pub use selector11::{
    Selector11RadiusBlock, Selector11RadiusError, Selector11RadiusOrigin,
    Selector11RetirementError, Selector11RetirementHost, Selector11ScanHost, Selector11State,
    Selector11TickRequest, Selector11UpdateError, Selector11UpdateHost,
};
pub use selector12::{
    Selector12DyingError, Selector12DyingHost, Selector12DyingRequest, Selector12GrantContext,
    Selector12GrantDifferenceError, Selector12GrantDifferenceRequest, Selector12GrantOwner,
    Selector12InitializationRequest, Selector12State,
};
pub use selector12_motion::{
    Selector12MotionError, Selector12MotionRequest, Selector12TerrainHost,
};
pub use selector12_tail::{Selector12TailError, Selector12TailProbeError, Selector12TailRequest};
pub use selector2::{
    ActorMessage, Selector2State, VehicleCarryContext, VehicleCarryEffect, VehicleCarryError,
    VehicleCarryHost, VehicleDamageError, VehicleDamageHost, VehicleDeathError, VehicleDriveError,
    VehicleDriveHost, VehicleDriveRequest, VehicleDriveState, VehicleDropHost, VehicleDyingHost,
    VehicleDyingRequest, VehicleGround, VehicleHandleArray, VehicleMessageEffect,
    VehicleMessageError, VehicleNetwork, VehiclePositionStep, VehicleUpdateHost,
    VehicleUpdateReport, VehicleUpdateRequest, VehicleUpdateState, VehicleWreckPending,
    WakeParticle, CAPTURED_STATUS_TEXT, VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    VEHICLE_HIT_FLASH_RADIUS,
};
pub use selector34::{RetainedSelector34Data, Selector34InitializationRequest, Selector34State};
pub use selector34::{
    Selector34AttackContext, Selector34AttackError, Selector34ControlsError,
    Selector34ControlsRequest, Selector34DyingEffect, Selector34DyingError, Selector34DyingHost,
    Selector34DyingRequest, Selector34MotionControls, Selector34MotionError, Selector34MotionHost,
    Selector34MotionRequest, Selector34PersistentAudioError,
};
pub use selector35::{
    RetainedSelector35Data, Selector35ClearanceHost, Selector35DescriptorContext,
    Selector35DescriptorError, Selector35DescriptorRequest, Selector35InitializationRequest,
    Selector35LifecycleEffect, Selector35LifecycleError, Selector35LifecycleHost,
    Selector35LifecycleRequest, Selector35MotionControls, Selector35MotionError,
    Selector35MotionRequest, Selector35PersistentAudioError, Selector35PickupError,
    Selector35RouteProcessState, Selector35State, Selector35TailError, Selector35TailRequest,
};
pub use selector36::Selector36State;
pub(in crate::object_runtime) use selector60::Selector60Rows;
pub use selector60::{RetainedSpawnList, Selector60Error, Selector60Row, Selector60State};
pub use selector77::{
    EyeDeathError, EyeDeathHost, Selector77Shot, Selector77State, Selector77TickRequest,
    Selector77UpdateError, Selector77UpdateHost, Selector79Message, Selector79State,
    Selector80State, EYE_DEATH_CHILD, EYE_STALK_MODEL,
};
pub use target_producers::{
    terrain_furniture_grant, FamilyGrantGridError, FamilyGrantGridHost,
    FamilyGrantGridPointRequest, FamilyRadiusScoreInput, FamilyTargetProducerError,
    FamilyTargetProducerHost, FamilyTargetProducerMode, FamilyTargetProducerRequest,
    FamilyTerrainGrantRequest, Selector12GrantGridRequest,
};
pub use terrain_probe::{
    FamilyTerrainProbeError, TerrainProbeAllocation, TerrainProbeCache, TerrainProbeError,
};
pub use update_prefix::{
    ActorWaterDamage, ActorWaterPlane, FamilyUpdateClass, FamilyUpdateEffect, FamilyUpdateError,
    FamilyUpdateHost, FamilyUpdateProgress, FamilyUpdateRequest,
};

#[derive(Clone, Copy, Debug)]
pub struct DerivedActorConstructionRequest<'a> {
    pub actor: ActorConstructionRequest<'a>,
    /// Signed H14/0 bytes +0E/+0F returned by 0048E120. Missing world data
    /// cannot be represented as a fabricated default range.
    pub world_mask_limits: [i8; 2],
}

/// The prefix added by 00425E60. Targets +148/+154 and behavior +140 own
/// concrete resources. Sound+168 is a process-qualified record token. Unwritten
/// carries stay unknown in strict arenas. Selected extended allocation backing
/// supplies fresh +14C only; +150/+158/+15C remain unknown until real writers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivedActorState {
    pub class: DerivedActorClass,
    /// Scalar DWORD138:425E60 leaves it unwritten;426270 publishes ratio*100 SAR12.
    pub word_138: Option<u32>,
    pub byte_13c: u8,
    pub byte_13d: Option<u8>,
    pub behavior_140: Option<ActorBehaviorState>,
    pub word_144: i32,
    pub target_148: Option<crate::object_runtime::NativeHeapRef>,
    /// Native425E60 skips14C. Selected fresh backing is separate from later
    /// target geometry publications and never reseeds an existing actor.
    pub word_14c: Option<u32>,
    pub word_150: Option<u32>,
    pub target_154: Option<crate::object_runtime::NativeHeapRef>,
    /// Native constructor425E60 leaves these secondary-target carries unwritten.
    pub word_158: Option<u32>,
    pub word_15c: Option<u32>,
    pub byte_160: u8,
    pub word_164: i32,
    ///425E60 initializes168 toNULL; teardown follows140.
    pub sound_168: Option<crate::process_sound::ProcessSoundRef>,
    ///425E60 leaves16C/170 unwritten; class10 audio424BF7/BFD publishes both.
    pub word_16c: Option<u32>,
    pub word_170: Option<u32>,
    pub word_174: Option<u32>,
    pub word_178: u16,
    pub word_17a: u16,
    pub sound_words_17c_182: Option<[u16; 4]>,
}

impl DerivedActorState {
    pub fn target_148_native_pointer_present(&self) -> bool {
        self.target_148.is_some()
    }
}

/// The concrete class above the common 00425E60 prefix. Selectors 7 and 33
/// share Base; 11, 60 and 77 preserve their own final-class state and callbacks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DerivedActorClass {
    Base,
    Family428650(Box<Family428650State>),
    Selector11(Box<Selector11State>),
    Selector2(Box<Selector2State>),
    Selector60(Box<Selector60State>),
    Selector77(Box<Selector77State>),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DerivedActorError {
    #[error("derived actor {0:?} is not the selector-2 vehicle")]
    NotSelector2(ObjectId),
    #[error("selector-2 admission requires the payload's maximum health for {0:?}")]
    MissingSelector2Health(ObjectId),
    #[error("vehicle {0:?} has no admitted +26C descriptor copy")]
    MissingSelector2Table(ObjectId),
    #[error("vehicle behavior owner {0:?} has fewer than the 16 records 0046B110 writes")]
    ShortVehicleBehavior(ObjectId),
    #[error("derived actor {0:?} is not selector 12")]
    NotSelector12(ObjectId),
    #[error("selector-12 payload lacks the 00468330 retained input")]
    MissingSelector12Payload,
    #[error("selector-12 postinitialization requires a resident terrain surface")]
    MissingSelector12Terrain,
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    #[error("derived actor {0:?} is not selector 10")]
    NotSelector10(ObjectId),
    #[error("actor {0:?} is not selector 21")]
    NotSelector21(ObjectId),
    #[error("derived actor {0:?}'s +24 is not the family's 00424830")]
    NotFamilyUpdate424830(ObjectId),
    #[error("selector-10 payload lacks the 00425290 extension")]
    MissingSelector10Payload,
    #[error("actor {0:?} requires the unsupported selector-10 helper replacement path")]
    Selector10HelperReplacementBoundary(ObjectId),
    #[error("derived actor {0:?} is not selector 36")]
    NotSelector36(ObjectId),
    #[error("actor payload lacks the 00423850 family extension")]
    MissingFamily4236d0Payload,
    #[error("actor {0:?} requires unimplemented family behavior-exit callbacks")]
    FamilyBehaviorBoundary(ObjectId),
    #[error("derived actor {0:?} is not selector 34")]
    NotSelector34(ObjectId),
    #[error("selector-34 payload lacks the 00433C00 retained input")]
    MissingSelector34Payload,
    #[error("selector-34 terrain floor requires a resident terrain surface")]
    MissingSelector34Terrain,
    #[error("derived actor {0:?} is not selector 35")]
    NotSelector35(ObjectId),
    #[error("selector-35 payload lacks the 0043D1C0 extension")]
    MissingSelector35Payload,
    #[error("actor {0:?} requires an initialized position")]
    MissingPosition(ObjectId),
    #[error(transparent)]
    Allocation(#[from] crate::object_runtime::RuntimeAllocationError),
    #[error("derived actor {0:?} is not selector 11")]
    NotSelector11(ObjectId),
    #[error("derived actor {0:?} is not selector 77")]
    NotSelector77(ObjectId),
    #[error("derived actor {0:?} is not selector 79")]
    NotSelector79(ObjectId),
    #[error("derived actor {0:?} is not selector 80")]
    NotSelector80(ObjectId),
    #[error(transparent)]
    Handles(#[from] ActorConstructionError),
    #[error("derived actor payload lacks the 00426C10 extension")]
    MissingPayload,
    #[error("derived actor {0:?} has no initialized guard byte at +13D")]
    MissingGuardMode(ObjectId),
    #[error(transparent)]
    Object(#[from] ObjectError),
}

impl ObjectArena {
    pub fn derived_actor(&self, object: ObjectId) -> Result<&DerivedActorState, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Derived(state) => Ok(state),
            _ => Err(ObjectError::NotDerivedActor(object)),
        }
    }

    // Captured source+10 mutates retained +140 through the same arena owner.
    // Keep this loan inside object_runtime; scene callers cannot bypass it.
    pub(in crate::object_runtime) fn derived_actor_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut DerivedActorState, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Derived(state) => Ok(state),
            _ => Err(ObjectError::NotDerivedActor(object)),
        }
    }

    /// 00425E60. Host preflight failures occur before publication/RNG. A
    /// successful constructor consumes exactly one draw even for equal limits
    /// or a nonpositive computed count; no private actor RNG is introduced.
    pub fn allocate_derived_actor(
        &mut self,
        request: DerivedActorConstructionRequest<'_>,
        rng: &mut u32,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request.actor)?;
        let random = name_entry_rng_next(rng) as u16;
        let [low, high] = request.world_mask_limits.map(i32::from);
        let count = i32::from(((i32::from(random) * (high - low)) >> 15) as i16) + low;
        // Native loops with x86 masked shifts and ORs their low WORDs. Once
        // the first sixteen iterations have run, later iterations add nothing.
        let mask = if count <= 0 {
            0
        } else if count >= 16 {
            u16::MAX
        } else {
            (1_u16 << count) - 1
        };
        let word_14c = self
            .actor_allocation
            .initial_dword(ActorAllocationDwordCarry::Derived14c);
        let word_150 = self
            .actor_allocation
            .initial_dword(ActorAllocationDwordCarry::Derived150);
        let actor = self.actor_mut(object)?;
        actor.word_b4 = 1;
        actor.extension = ActorExtension::Derived(Box::new(DerivedActorState {
            class: DerivedActorClass::Base,
            word_138: None,
            byte_13c: 0,
            byte_13d: None,
            behavior_140: None,
            word_144: 0,
            target_148: None,

            word_14c,
            word_150,
            target_154: None,
            word_158: None,
            word_15c: None,
            byte_160: 0,
            word_164: 2500,
            sound_168: None,
            word_16c: None,
            word_170: None,
            word_174: None,
            word_178: mask,
            word_17a: 0,
            sound_words_17c_182: None,
        }));
        self.owned_mut(object)?.state.base.dirty_word |= 2;
        Ok(object)
    }

    /// 00426BF0: final virtual context initializer first, then 00426C10's
    /// shared actor payload followed by its own extension. Missing typed data
    /// is a portable preflight error, not a native partial-read result.
    pub fn initialize_derived_actor(
        &mut self,
        object: ObjectId,
        request: ActorInitializationRequest<'_>,
        runtime: &ResidentRuntime,
        math: &NativeBasisMath,
    ) -> Result<(), DerivedActorError> {
        self.derived_actor(object)?;
        let payload = request
            .payload
            .derived
            .as_ref()
            .ok_or(DerivedActorError::MissingPayload)?;
        self.initialize_actor(object, request, runtime, math)?;
        let state = self.derived_actor_mut(object)?;
        state.byte_13d =
            Some(payload.byte_44.value | if payload.word_48.value != 0 { 0x80 } else { 0 });
        state.word_164 = i32::from(payload.signed_word_4a.value);
        state.word_174 = Some(payload.word_4c.value);
        state.sound_words_17c_182 = Some(payload.sound_bindings.map(|value| value.raw.value));
        Ok(())
    }

    /// 00426C80 runs before any suffix command. Values outside low-seven-bit
    /// range 1..13 accept without replacing previous +140 custody.
    pub fn admit_derived_actor(&mut self, object: ObjectId) -> Result<bool, DerivedActorError> {
        let mode = self
            .derived_actor(object)?
            .byte_13d
            .ok_or(DerivedActorError::MissingGuardMode(object))?;
        let base = &self.object(object)?.base;
        if let Some(behavior) = build_actor_behavior(mode, base.flags, self.actor(object)?.word_b4)
        {
            self.teardown_actor_behavior(
                object,
                &mut crate::object_runtime::SoundTeardownService::RequiredAtReach,
            )?;
            self.derived_actor_mut(object)?.behavior_140 = Some(behavior);
        }
        Ok(true)
    }

    /// 00426030. A derived target records the actual wrapper; shared commands
    /// still execute the existing 0047ADA0 owner and preserve lazy operands.
    pub fn apply_derived_actor_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        self.derived_actor(object)?;
        if !matches!(
            command.target,
            InstanceCommandTarget::DerivedActor
                | InstanceCommandTarget::Family428650
                | InstanceCommandTarget::Family4236d0
                | InstanceCommandTarget::Constructor(
                    6 | 7 | 9 | 10 | 11 | 12 | 21 | 33 | 34 | 35 | 36 | 60 | 77 | 79 | 80
                )
        ) {
            return Err(ActorCommandError::WrongTarget);
        }
        if command.code.value <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        match (command.code.value, &command.payload) {
            (16, InstanceCommandPayload::NoPayloadRead) => {}
            (17, InstanceCommandPayload::ActorWord164(value)) => {
                self.derived_actor_mut(object)?.word_164 = i32::from(value.value);
            }
            (18, InstanceCommandPayload::ActorNegatedDword144(value)) => {
                let value = (value.value as i32).wrapping_neg();
                self.derived_actor_mut(object)?.word_144 = value;
                if value != 0 {
                    self.owned_mut(object)?.state.base.flags |= 0x0420_0000;
                }
            }
            (16..=18, _) => return Err(ActorCommandError::Payload(command.code.value)),
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;
