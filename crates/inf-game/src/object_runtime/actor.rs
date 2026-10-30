//! Shared actor allocation and lifetime, 0047A7A0/0047AA60.
//!
//! The position consumed by construction precedes placement initialization
//! and must be supplied as explicit allocation state. Concrete extensions
//! preserve their own constructor writes and later callback boundaries.

use thiserror::Error;

mod allocation;
mod angular_response;
mod attachment;
pub use attachment::{ActorAttachmentError, ActorAttachmentHost};
mod behavior;
pub use behavior::{
    ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode, ActorWeaponFireRequest,
};
pub use behavior::{
    DetonatorPulseHost, WeaponEmissionError, WeaponEmissionLimit, WeaponEmissionRequest,
    WeaponFiringContext, WeaponFiringDrainError, WeaponFiringDrainSummary, WeaponFiringEmitter,
    WeaponFiringRuntime,
};
mod capture;
mod child;
pub use child::{
    ActorChildError, ActorChildFactoryCall, ActorChildHost, ActorChildRequest, ActorChildResource,
};
mod collision_geometry;
mod commands;
pub(crate) mod common_update;
mod damage;
mod damage_family;
mod damage_reaction;
mod derived;
mod family40d800;
pub use derived::{
    FamilyDyingAudio, FamilyDyingEffect, FamilyDyingError, FamilyDyingHost, FamilyDyingRequest,
    FamilyDyingSpawnRequest,
};
pub use derived::{FamilyIntentError, FamilyPlayerWeaponError};
pub use derived::{FamilyPairReactionError, FamilyPairReactionRequest, FamilyRescueAccept};
pub use derived::{Selector12DyingError, Selector12DyingHost, Selector12DyingRequest};
pub use derived::{
    Selector60SpawnResult, Selector60SpawnRow, Selector60UpdateError, Selector60UpdateHost,
};
pub use family40d800::{FamilyDeathError, FamilyDeathHost, FamilyLinkChange};
mod behavior_resource;
pub(crate) use behavior_resource::BEHAVIOR_RESOURCE_SELECTORS;
pub use behavior_resource::{
    laser31_draw, BehaviorResourceClass, BehaviorResourceState, Family455560State,
    Family455560Tail, Family485f50State, Grenade26Error, Grenade26Host, Grenade26Light,
    ImplantTakeHost, Laser31Beam, Laser31DrawError, MissileError, MissileHost,
    MissileLifecycleHost, MissileLight, MissileSteerHost, Shot32Error, Shot32Host, LASER_MODE,
    LASER_NEGATIVE_TEXTURE_SLOT, LASER_TEXTURE_SLOT, LASER_WIDTH_RESIDUE_APPROXIMATION,
    SHOT32_LIFE_MICROS,
};
pub use behavior_resource::{
    Bomb29Message, BombLifecycleHost, BOMB_DROPPED_HINT, BOMB_SELECTOR, BOMB_TAKEN_HINT,
};
mod grant_notification;
mod message;
pub use message::{ActorMessageError, ActorMessageReply, ActorMessageWalk};
mod initialization;
mod interactions;
mod intro;
mod retaliation;
mod selector15;
mod selector16;
mod selector30;
pub(crate) use selector15::{selector15_light_is_idle, selector15_value};
pub use selector15::{
    FactoryError, FactoryHost, FactoryStaffHost, Selector15State, StaffReleaseHost, UnitBroadcast,
    UnitBroadcastCode, UnitBroadcastOutcome,
};
mod selector37;
pub use selector37::{
    BridgeTerrain, Selector37Error, Selector37Host, Selector37State, BRIDGE_SOUND,
};
mod selector39;
pub use selector39::{
    Selector39AfterLoad, Selector39Countdown, Selector39DeathHost, Selector39Error, Selector39Host,
    Selector39LinkDelivery, Selector39MissionWrite, Selector39State, Selector39UpdateHost,
    FUSE_SOUND,
};
mod selector53;
pub use selector53::{Selector53Error, Selector53State};
mod selector47;
pub use selector47::{Selector47Error, Selector47Host, Selector47State, SIREN_SOUND};
mod selector52;
pub(crate) use derived::selector21_value;
pub(crate) use selector17::selector17_value;
pub use selector17::{PrisonHost, Selector17Error, Selector17State, PRISON_RADIUS};
pub(crate) use selector52::selector52_value;
pub use selector52::{Selector52Error, Selector52Host, Selector52State, SELECTOR52_MINE_CALL};
pub use selector55::{Selector55State, BEAMER_RANGE};
pub(crate) use selector73::wave_colour;
pub use selector73::{Selector73State, ECM_FLAG, ECM_GLOW_PHASE_APPROXIMATION};
mod selector74;
pub use selector74::{
    selector74_point_height, Selector74Error, Selector74Host, Selector74State, SELECTOR74_PARTICLE,
};
mod selector78;
pub use selector78::{
    Selector78Error, Selector78Host, Selector78State, COMPLETION_BONUS as ENDING_COMPLETION_BONUS,
};
mod selector84;
pub use selector84::{CaptureNodeCounts, Selector84State};
mod selector85;
pub use selector85::{
    AlienBrainWords, Selector85Error, Selector85Host, Selector85State, BRAIN_INVULNERABILITY,
};
mod selector62;
pub use selector62::{
    Selector62Error, Selector62Host, Selector62State, SELECTOR62_BLAST, SELECTOR62_BLOCKS,
    SELECTOR62_DURATIONS, SELECTOR62_LEAVING_BLAST,
};
mod selector61;
pub use selector61::{
    Selector61DeathHost, Selector61Error, Selector61Host, Selector61State, Selector61UpdateRequest,
    PURSUIT_SHIP_BLAST, PURSUIT_SHIP_EXPLOSION, PURSUIT_SHIP_VISUAL,
};
mod selector59;
pub use selector59::{
    BoulderParticle, Selector59ContactError, Selector59ContactHost, Selector59Error,
    Selector59State, Selector82Host,
};
mod selector91;
pub use selector91::{
    encode_ball, read_ball_sync, BallError, BallHost, BallState, BallSync, GOAL_WON,
    SOUND_DOUBLE_GOAL, SOUND_GOAL, SOUND_KICK_OFF,
};
mod selector18;
pub use selector18::{
    force_field_normal, ForceFieldBits, Selector18Error, Selector18Host, Selector18State,
    FORCE_FIELD_BOUNCE_CODE, FORCE_FIELD_FADE, FORCE_FIELD_LIGHT_RGB, FORCE_FIELD_NOT_SECURED_HINT,
    FORCE_FIELD_NOT_SECURED_TEXT, FORCE_FIELD_SECURED_TEXT,
};
mod selector18_draw;
pub use selector18_draw::{
    force_field_dome, DomePoint, DomeQuad, ForceFieldDome, ForceFieldDrawError,
    FORCE_FIELD_BAND_SPRITE, FORCE_FIELD_BOTTOM_SPRITE, FORCE_FIELD_DECAL_SPRITE,
    FORCE_FIELD_LIGHT_GREY, FORCE_FIELD_MODE,
};
mod selector44;
pub use selector44::{
    Selector44DeathHost, Selector44Error, Selector44Host, Selector44State, ROCK_BLASTS,
    ROCK_DRAW_FADE, ROCK_GLOW, ROCK_RETIRE_MESSAGE, ROCK_VISUALS,
};
mod selector17;
mod selector40;
mod selector55;
mod selector57;
mod selector73;
pub use selector40::{BannerError, BannerHost, BannerRequest, Selector40State};
pub use selector57::{
    Selector57DeathHost, Selector57Error, POWERBALL_PARTNER_RESOURCE, POWERBALL_RETIRE_MESSAGE,
};
mod selector49;
pub use selector49::{Selector49DeathHost, Selector49Error, Selector49State, Selector49UpdateHost};
mod selector69;
pub use selector69::{
    selector69_beam_width, selector69_beams, selector69_draw, Selector69Bolt, Selector69DrawError,
    Selector69Error, Selector69Fit, Selector69Geometry, Selector69Host, Selector69Message,
    Selector69Projected, Selector69Quad, Selector69State, Selector69UnwrittenFog,
    BARRIER_AGE_RESIDUE_APPROXIMATION, BARRIER_MODE_0052F628, BARRIER_TRACE_RESOURCE,
};
mod selector66;
pub use selector66::{Selector66Error, Selector66Host, Selector66Particle, Selector66State};
mod selector88;
mod selector90;
mod selector93;
mod state_change;
mod terrain;
pub(in crate::object_runtime) use allocation::{ActorAllocationDwordCarry, ActorAllocationService};
pub use allocation::{ActorAllocationPolicy, ActorAllocationPolicyError};
pub use angular_response::ActorAngularResponseRequest;
pub use behavior::{
    ActorBehaviorGroup, ActorBehaviorPositionError, ActorBehaviorPositionRequest,
    ActorBehaviorState, ActorBehaviorTable, ActorBehaviorTickError, BehaviorActorResourceRequest,
    BehaviorCacheStorageError, BehaviorComponentPacketSourceRequest, BehaviorComponentQuery,
    BehaviorCreationRequest, BehaviorPacketCopy, BehaviorPacketSlot, BehaviorPacketSourceRequest,
    BehaviorParticleAdapter, BehaviorParticleAdapterError, BehaviorParticleCreationSources,
    BehaviorParticleEffectRequest, BehaviorParticleServiceContext, BehaviorParticleServices,
    BehaviorPredicateContext, BehaviorPredicateError, BehaviorPredicateHost,
    BehaviorPredicateRequest, BehaviorRowSelection, BehaviorTemporary, FamilyCandidateGateError,
    FamilyCandidateGateRequest, ProcessActorBehaviorTables,
};
pub use capture::{
    ActorCaptureContext, ActorCaptureError, ActorCaptureHost, ActorCapturePlayerCommand,
    ActorCaptureRequest, ActorCaptureSavedFlags,
};
pub use collision_geometry::{
    apply_contact_geometry, terrain_sphere_contact, CollisionContact, ContactAxisNormals,
    ContactGeometryRequest, ContactSphere, TerrainSphereError, TerrainSphereGrid,
};
pub use commands::{ActorCommandContext, ActorCommandError};
pub use damage::{
    ActorDamageCallback, ActorDamageError, ActorDamageHost, ActorDamageOperation,
    ActorDamagePacket, ActorDamageRequest, ActorTimedDamageError, ActorTimedDamageHost,
    ActorTimedDamageRequest,
};
pub use damage_family::{
    ActorFamilyDamageError, ActorFamilyDamageHost, ActorFamilyDamageOperation,
    ActorFamilyTargetError, ActorFamilyTargetRequest,
};
pub use damage_reaction::{
    ActorDamageReactionEffect, ActorDamageReactionError, ActorDamageReactionHost,
};
pub(crate) use derived::native_trunc_low32;
pub use derived::FamilyActorListSelectionError;
pub(in crate::object_runtime) use derived::Selector60Rows;
pub use derived::{
    segment_probe_43a380, selector10_custom_425870, SegmentProbe, Selector10CustomInputs,
    Selector10CustomServices, Selector10LegError, Selector10LegGround, WheelGround,
};
pub use derived::{
    terrain_furniture_grant, ActorAngularAxis, ActorAngularMotionRequest, ActorAngularMotionTable,
    ActorForwardMotionRequest, ActorForwardMotionTable, ActorLateralMotionRequest,
    ActorLateralMotionTable, ActorMessage, ActorMotionError, ActorMotionTables,
    ActorPhysicsRequest, ActorWaterDamage, ActorWaterPlane, DerivedActorClass,
    DerivedActorConstructionRequest, DerivedActorError, DerivedActorState, Family4236d0Class,
    Family4236d0InitializationRequest, Family4236d0State, Family428650Class, Family428650State,
    Family429e20Class, Family429e20State, FamilyAttackError, FamilyAttackHost,
    FamilyAvoidanceError, FamilyAvoidanceHost, FamilyBehaviorDescriptor,
    FamilyBehaviorDispatchRequest, FamilyBehaviorError, FamilyBehaviorHost, FamilyBehaviorProfile,
    FamilyCollisionGuardError, FamilyCollisionGuardHost, FamilyCollisionGuardRequest,
    FamilyDescriptorScoreError, FamilyDescriptorScoreKind, FamilyDescriptorScoreRequest,
    FamilyDescriptorScoreRng, FamilyGrantGridError, FamilyGrantGridHost,
    FamilyGrantGridPointRequest, FamilyHandleEntry, FamilyHelper482ff0, FamilyObstacleProbeHost,
    FamilyObstacleScanError, FamilyPeriodicProbeRequest, FamilyPeriodicRecord,
    FamilyRadiusScoreInput, FamilyTargetProducerError, FamilyTargetProducerHost,
    FamilyTargetProducerMode, FamilyTargetProducerRequest, FamilyTargetSlot,
    FamilyTerrainGrantRequest, FamilyTerrainProbeError, FamilyTerrainProbeMissError,
    FamilyTerrainProbeMissHost, FamilyUpdateClass, FamilyUpdateEffect, FamilyUpdateError,
    FamilyUpdateHost, FamilyUpdateProgress, FamilyUpdateRequest, NativeFamilyUpdateError,
    NativeFamilyUpdateHost, RetainedActorAllocation, RetainedSelector34Data,
    RetainedSelector35Data, RetainedSpawnList, Selector10Helper, Selector10HelperChild,
    Selector10HelperChildClass, Selector10HelperClass, Selector10State, Selector11RadiusBlock,
    Selector11RadiusError, Selector11RadiusOrigin, Selector11RetirementError,
    Selector11RetirementHost, Selector11ScanHost, Selector11State, Selector11TickRequest,
    Selector11UpdateError, Selector11UpdateHost, Selector12DescriptorChangeError,
    Selector12DescriptorChangeKind, Selector12DescriptorChangeRequest, Selector12GrantContext,
    Selector12GrantDifferenceError, Selector12GrantDifferenceRequest, Selector12GrantGridRequest,
    Selector12InitializationRequest, Selector12MotionError, Selector12MotionRequest,
    Selector12PointUpdateError, Selector12PointUpdateRequest, Selector12PreferredUpdateError,
    Selector12PreferredUpdateRequest, Selector12State, Selector12TailError,
    Selector12TailProbeError, Selector12TailRequest, Selector12TerrainHost, Selector2State,
    Selector34AttackContext, Selector34AttackError, Selector34ControlsError,
    Selector34ControlsRequest, Selector34DyingEffect, Selector34DyingError, Selector34DyingHost,
    Selector34DyingRequest, Selector34InitializationRequest, Selector34MotionControls,
    Selector34MotionError, Selector34MotionHost, Selector34MotionRequest,
    Selector34PersistentAudioError, Selector34State, Selector35ClearanceHost,
    Selector35DescriptorContext, Selector35DescriptorError, Selector35DescriptorRequest,
    Selector35InitializationRequest, Selector35LifecycleEffect, Selector35LifecycleError,
    Selector35LifecycleHost, Selector35LifecycleRequest, Selector35MotionControls,
    Selector35MotionError, Selector35MotionRequest, Selector35PersistentAudioError,
    Selector35PickupError, Selector35RouteProcessState, Selector35State, Selector35TailError,
    Selector35TailRequest, Selector36State, Selector60Error, Selector60Row, Selector60State,
    Selector77State, Selector79Message, Selector79State, Selector80State, TerrainProbeAllocation,
    TerrainProbeCache, TerrainProbeError, VehicleCarryContext, VehicleCarryEffect,
    VehicleCarryError, VehicleCarryHost, VehicleDamageError, VehicleDamageHost, VehicleDeathError,
    VehicleDriveError, VehicleDriveHost, VehicleDriveRequest, VehicleDriveState, VehicleDropHost,
    VehicleDyingHost, VehicleDyingRequest, VehicleGround, VehicleHandleArray, VehicleMessageEffect,
    VehicleMessageError, VehicleNetwork, VehiclePositionStep, VehicleUpdateHost,
    VehicleUpdateReport, VehicleUpdateRequest, VehicleUpdateState, VehicleWreckPending,
    WakeParticle, CAPTURED_STATUS_TEXT, VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    VEHICLE_HIT_FLASH_RADIUS,
};
pub use derived::{
    ActorAudioCameraHost, ActorContactEnvironment, ActorContactError, ActorContactHeaderHost,
    ActorContactOutput, ActorContactRequest, ActorContactStackPolicy, Selector36MotionError,
    Selector36MotionNextPhase, Selector36MotionRequest, Selector36PersistentAudioError,
    Selector36PersistentAudioRequest,
};
pub use derived::{
    EyeDeathError, EyeDeathHost, Selector77Shot, Selector77TickRequest, Selector77UpdateError,
    Selector77UpdateHost, EYE_DEATH_CHILD, EYE_STALK_MODEL,
};
pub use derived::{Family4236d0MotionError, Family4236d0MotionRequest};
pub use derived::{
    FamilyAttackUpdateError, FamilyAttackUpdateHost, FamilyAttackUpdateRequest,
    FamilyAttackUpdateServices, FamilyDormantError, FamilyDormantRequest, FamilyEvadeEntryKind,
    FamilyEvadeEntryRequest, FamilyFollowPlayerError, FamilyHelperHistoryError,
    FamilyHelperHistoryInputs, FamilyHelperHistoryRecord, FamilyHelperQueryRequest,
    FamilyHelperRecordRequest, FamilyHerdingError, FamilyObjectTargetChangeError,
    FamilyObjectTargetChangeRequest, FamilyObjectTargetUpdateError, FamilyPointTargetChangeError,
    FamilyPointTargetChangeKind, FamilyPointTargetChangeRequest, FamilyRoamUpdateError,
    FamilyRoamUpdateRequest, FamilyTimedDescriptorError, FamilyTimedDescriptorKind,
    FamilyTimedDescriptorRequest,
};
pub use derived::{
    FamilySelectionCandidate, FamilySelectionError, FamilySelectionHost, FamilySelectionRequest,
    FamilySelectionServices, FamilyTargetScanError, FamilyTargetScanOutputs,
    FamilyTargetScanRequest, FamilyTargetSelection, FamilyTargetTable, FamilyTargetTableError,
};
pub use derived::{
    ProcessActorMotionState, Selector36ChildServiceError, Selector36TailNextPhase,
    Selector36TailPrefixError, Selector36TailPrefixRequest,
};
pub use derived::{
    Selector10HelperTickError, Selector10HelperTickRequest, Selector10MotionError,
    Selector10MotionNextPhase, Selector10MotionRequest, Selector10PersistentAudioError,
    Selector10TailNextPhase, Selector10TailPrefixError, Selector10TailPrefixRequest,
};
pub use family40d800::Actor40d800State;
pub use grant_notification::{
    ActorGrantDifferenceDelivery, ActorGrantDifferenceNotificationError,
    ActorGrantDifferenceNotificationRequest, ActorVehicleCleanupNotificationError,
    ActorVehicleCleanupNotificationRequest,
};
pub use initialization::{ActorDamageTable, ActorInitializationRequest};
pub use interactions::{
    ActorComponentQueryMode, ActorComponentQueryRequest, ActorInteractionError,
    ActorInteractionNotificationContext, ActorInteractionNotificationRequest,
    ActorInteractionRelationRequest,
};
pub use intro::{
    ArenaIntroError, ArenaIntroHost, ArenaIntroInputs, ArenaIntroRequest, ArenaIntroServices,
    ArenaIntroTickError, IntroAdmissionError, IntroAdmissionInputs, IntroCallbackHost,
    NativeIntroServices, NativeIntroServicesError,
};
pub use retaliation::ActorRetaliationError;
pub use selector16::{
    CameraDerivedComponent, CameraDerivedTargetMode, CameraDerivedTargetRequest, CameraDrawRequest,
    CameraGlobalState, CameraHeightRequest, CameraParameterBlock, CameraParameterState, CameraPose,
    CameraPoseError, CameraPoseHost, CameraPoseTickRequest, CameraSettings, CameraSlewError,
    CameraSlewRequest, HeldCameraError, HeldCameraHost, NativeCameraHost, NativeCameraInputs,
    NativeCameraServices, Selector16State,
};
pub use selector30::{
    GateEntry, GateRecord, Selector30Error, Selector30PassError, Selector30PassHost,
    Selector30State, Selector30TickRequest, Selector30UpdateError, Selector30UpdateHost,
};
pub use selector88::{
    InstanceCommandAllocation, InstanceCommandAllocationError, RetainedInstanceCommand,
    Selector88IntroError, Selector88IntroScript, Selector88State,
};
pub use selector90::{
    Selector90Byte, Selector90Error, Selector90MotionMode, Selector90State, Selector90TickRequest,
    Selector90Word,
};
pub use selector93::{Selector93State, Selector93TickRequest, Selector93UpdateError};
pub use state_change::{
    ActorStateChangeEffect, ActorStateChangeError, ActorStateChangeHost, ActorStateChangeQuery,
    ActorStateChangeRequest,
};
pub use terrain::ActorTerrainHost;

use super::{
    classify_actor_terrain, mark_flag_change, ActorHandle, ActorHandleError, ActorHandles,
    ActorTerrainError, ActorTerrainInputs, ObjectArena, ObjectClass, ObjectClassKind, ObjectError,
    ObjectId, RotatingVisual,
};

#[derive(Clone, Copy, Debug)]
pub struct ActorRegistryConfig {
    pub owner_count: usize,
    pub initial_capacity: usize,
    /// Snapshot used by the registry's own null-resolution policy. Actor +88
    /// construction independently reads its explicit constructor NULL source.
    pub null_handle: ActorHandle,
}

/// Current process NULL0054E894 consumed by0047A885. The registry snapshot is
/// a named legacy approximation for callers without a live process owner.
#[derive(Clone, Copy, Debug)]
pub enum ActorConstructorNull<'a> {
    Process(&'a crate::process_particles::factories::ParticleFactoryProcessState),
    RegistrySnapshotApproximation,
}

#[derive(Clone, Copy, Debug)]
pub struct ActorConstructionRequest<'a> {
    /// Contents of allocation +32/+34/+36 before the constructor. None
    /// remains unknown and is valid only when the native path never reads it.
    pub allocation_position: Option<[u16; 3]>,
    /// Current process byte 0055F2F4; no owner is selected implicitly.
    pub owner: u8,
    pub constructor_null: ActorConstructorNull<'a>,
    pub terrain: ActorTerrainInputs<'a>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorConstructionError {
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
    #[error("this arena has no configured actor registry")]
    MissingRegistry,
    #[error("actor construction requires current process NULL0054E894")]
    MissingProcessNull,
    #[error(transparent)]
    Handles(#[from] ActorHandleError),
    #[error(transparent)]
    Terrain(#[from] ActorTerrainError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("constructor {0} is not one of the behavior registry's resource classes")]
    BehaviorResourceSelector(u16),
}

/// Fields initialized by the shared actor constructor. Unwritten scalar
/// fields that a later payload consumes stay None; omitted native holes have
/// no portable value. Arena access is read-only so handle/list custody cannot
/// be bypassed by changing this record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorState {
    pub extension: ActorExtension,
    pub rotating: RotatingVisual,
    pub word_6c: u32,
    /// The bits of +6C retail sets from stack residue: a Force Field's
    /// creation listener (00477350) testing an actor 0048D820 built at its
    /// eight unwritten stack bytes. A reader that reaches one stops.
    pub unknown_6c: u32,
    /// Native+70, unwritten by 0047A7A0. Shared capture publishes a source
    /// handle; release publishes the actual process NULL only when reached.
    pub capture_handle_70: Option<ActorHandle>,
    pub word_74: u16,
    pub word_76: Option<u16>,
    pub byte_78: u8,
    pub word_7a: u16,
    pub byte_7c: u8,
    pub byte_7d: u8,
    pub word_80: u32,
    /// Native +84. Registration is released by ObjectArena::destroy.
    pub handle: ActorHandle,
    /// Native +88, initialized from the explicit null handle.
    pub owner_handle: ActorHandle,
    pub word_8c: u16,
    pub words_8e_92: [u16; 3],
    pub words_94_9c: [u32; 3],
    pub health: u16,
    pub maximum_health: Option<u16>,
    /// Native +A4 retains shared immutable response storage.
    pub damage_table: ActorDamageTable,
    // +A8/+AC/+B0 are represented by the arena's ordered actor IDs.
    pub word_b4: u32,
    /// +B8/+BC/+C0 are unwritten by 0047A7A0.0047DEA0 stores the
    /// actual caller color/near/far;0047DFA0 rereads them in that order.
    pub flash_far_color_b8: Option<u32>,
    pub flash_near_bc: Option<i32>,
    pub flash_far_c0: Option<i32>,
    /// Signed lifecycle state consumed by 0047D670, independent of +B4 and
    /// the constructor selector. Construction starts at state 3 (alive).
    pub word_c4: u32,
    pub word_c8: u16,
    /// +CA is unwritten by 0047A7A0;004283E0 publishes the hit color55.
    pub word_ca: Option<u16>,
    pub word_cc: u16,
    pub word_ce: u16,
    pub byte_d0: u8,
    /// Native+D4, unwritten by 0047A7A0.0047D821 saves pre-capture flags here.
    /// This is distinct from derived target storage at+1D4.
    pub capture_flags_d4: Option<u32>,
    pub word_d8: u32,
    pub word_dc: u32,
    pub word_e0: u32,
    pub word_e4: u32,
}

impl ActorState {
    fn new(handle: ActorHandle, null_handle: ActorHandle) -> Self {
        Self {
            extension: ActorExtension::Shared,
            rotating: RotatingVisual::new(),
            word_6c: 0,
            unknown_6c: 0,
            capture_handle_70: None,
            word_74: 0,
            word_76: None,
            byte_78: 0xff,
            word_7a: 0,
            byte_7c: 0,
            byte_7d: 0,
            word_80: 0,
            handle,
            owner_handle: null_handle,
            word_8c: 1,
            words_8e_92: [0; 3],
            words_94_9c: [0; 3],
            health: 1000,
            maximum_health: None,
            damage_table: ActorDamageTable::ConstructorDefault,
            word_b4: 0,
            flash_far_color_b8: None,
            flash_near_bc: None,
            flash_far_c0: None,
            word_c4: 3,
            word_c8: 0,
            word_ca: None,
            word_cc: 0,
            word_ce: 0,
            byte_d0: 0,
            capture_flags_d4: None,
            word_d8: 100_000,
            word_dc: 0,
            word_e0: 0,
            word_e4: 4,
        }
    }

    /// Current table selected at native +A4. Each
    /// entry is (threshold, Q8 multiplier), indexed by damage channel 0..9.
    /// Values are read from the retained payload, never resolved again by slot.
    pub fn damage_responses(&self) -> [(u16, u16); 10] {
        self.damage_table.responses()
    }
}

/// The final native class, independent of the shared actor prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorExtension {
    Shared,
    Derived(Box<DerivedActorState>),
    Selector15(Box<Selector15State>),
    Selector16(Box<Selector16State>),
    Selector30(Box<Selector30State>),
    Selector37(Box<Selector37State>),
    Selector39(Box<Selector39State>),
    Selector53(Box<Selector53State>),
    /// The portable mining rig: 00526714 over the shared table.
    Selector52(Box<Selector52State>),
    /// The ECM Unit: 005262CC over the shared table.
    Selector73(Box<Selector73State>),
    /// The Remote Beamer: 005274C4 over the shared table.
    Selector55(Box<Selector55State>),
    /// The Prison Unit: 005272A0 over the shared table, with its one-slot
    /// hold at +144.
    Selector17(Box<Selector17State>),
    /// The Remote Beamer's receiver: 00526530 over the shared table, with
    /// no fields of its own.
    Selector56,
    /// Helniazine's zone: 00521764 over the shared table.
    Selector74(Box<Selector74State>),
    /// The Pursuit Ship: 00520700 over the shared table.
    Selector61(Box<Selector61State>),
    /// The Alien Brain's attack: 00526DC8 over the shared table.
    Selector62(Box<Selector62State>),
    /// The Alien Brain's capture node: 00522488 over the shared table.
    Selector84(Selector84State),
    /// The Alien Brain's ending director: 0051FA9C over the shared table,
    /// with the creation listener 0051FA98 at +138.
    Selector78(Box<Selector78State>),
    /// The Siren: 00527CD4 over the 0040D800 family.
    Selector47(Box<Selector47State>),
    /// The falling rock: 00525648 over the shared table.
    Selector44(Box<Selector44State>),
    /// The Powerball: 00526464 over the shared table, with no fields of
    /// its own.
    Selector57,
    /// Capture the Banner's banner: 005260A0 over the shared table.
    Selector40(Box<Selector40State>),
    /// Capture the Banner's base: 005267E0 over the shared table, with no
    /// fields of its own.
    Selector41,
    /// The Boulders: 0051F900 over the shared table.
    Selector59(Box<Selector59State>),
    /// The Force Field: 005279E0 over the shared table, with the creation
    /// listener 005279DC at +138.
    Selector18(Box<Selector18State>),
    /// The barrier: 005205D8 over the shared table.
    Selector69(Box<Selector69State>),
    /// The Seismic Stabiliser: 00527908 over the 0040D800 family.
    Selector49(Box<Selector49State>),
    Selector66(Box<Selector66State>),
    /// Selectors 25..28, 31, 32, 38 and 68, built only by 0048D820.
    BehaviorResource(Box<BehaviorResourceState>),
    Selector88(Box<Selector88State>),
    Selector90(Box<Selector90State>),
    Selector93(Box<Selector93State>),
}

impl ObjectArena {
    /// Create a manager with explicit actor-registration inputs. This owns
    /// the registry for the whole arena lifetime; it cannot be reconfigured
    /// underneath live actors. Standalone ActorHandles retains native reset
    /// operations for the separately owned manager teardown policy.
    pub fn with_actor_registry(config: ActorRegistryConfig) -> Result<Self, ActorHandleError> {
        let registry = ActorHandles::new(
            config.owner_count,
            config.initial_capacity,
            config.null_handle,
        )?;
        let mut arena = Self::new();
        arena.actor_handles = Some(registry);
        Ok(arena)
    }

    /// Native actor +A8 nodes, ordered from the embedded head at G+48.
    /// Constructors append through G+58, the same list's tail-predecessor link.
    pub fn actor_list(&self) -> &[ObjectId] {
        &self.actors
    }

    pub fn actor_handles(&self) -> Option<&ActorHandles> {
        self.actor_handles.as_ref()
    }

    pub fn actor(&self, object: ObjectId) -> Result<&ActorState, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Actor(actor) => Ok(actor),
            _ => Err(ObjectError::NotActor(object)),
        }
    }

    /// Actor virtual +58 only: 00404610 returns the derived actor itself;
    /// 00401300 returns null for shared/16/30/53/88/90/93. These pure callbacks
    /// consume neither +CC nor +22C, lifecycle, position or behavior state.
    pub fn actor_derived_component(
        &self,
        actor: ObjectId,
    ) -> Result<Option<ObjectId>, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Derived(state) => match &state.class {
                DerivedActorClass::Base
                | DerivedActorClass::Family428650(_)
                | DerivedActorClass::Selector2(_)
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => Some(actor),
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => None,
        })
    }

    /// Whether virtual +64 answers: only a vehicle's does (00404610, self).
    pub fn actor_is_vehicle(&self, actor: ObjectId) -> Result<bool, ObjectError> {
        Ok(self.actor_vehicle_component(actor)?.is_some())
    }

    /// Pure canonical virtual +64: Selector2's 00520860 table uses
    /// 00404610 (self); every other admitted actor uses 00401300 (NULL).
    /// The returned actual payload borrow consumes no vehicle fields, B4,
    /// behavior, registry or process state. Nonactor tables and arbitrary
    /// virtual overrides are outside this class-only policy.
    pub(super) fn actor_vehicle_component(
        &self,
        actor: ObjectId,
    ) -> Result<Option<&Selector2State>, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Derived(state) => match &state.class {
                DerivedActorClass::Selector2(vehicle) => Some(vehicle),
                DerivedActorClass::Base
                | DerivedActorClass::Family428650(_)
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => None,
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => None,
        })
    }

    /// Pure canonical virtual +7C: derived tables use 004027A0's actual
    /// +140 presence; shared/16/30/53/88/90/93 use the null 00401300 body. This
    /// owner preserves full-DWORD presence, without inventing pointer bits
    /// or consuming +CC/+22C. Arbitrary virtual overrides are not admitted.
    pub(in crate::object_runtime) fn actor_behavior_presence_7c(
        &self,
        actor: ObjectId,
    ) -> Result<bool, ObjectError> {
        Ok(match &self.actor(actor)?.extension {
            ActorExtension::Derived(state) => match &state.class {
                DerivedActorClass::Base
                | DerivedActorClass::Family428650(_)
                | DerivedActorClass::Selector2(_)
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => state.behavior_140.is_some(),
            },
            // 00413130 returns the Pursuit Ship's +138.
            ActorExtension::Selector61(ship) => ship.behavior_138.is_some(),
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector66(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => false,
        })
    }

    /// Shared 004404D0: copy six packed XYZ bytes only. The original body
    /// also serves selector90 intro helpers; no basis, velocity, dirty, target
    /// or membership reset belongs to this phase. WORD bit patterns are raw.
    pub fn copy_actor_position(
        &mut self,
        actor: ObjectId,
        position: [u16; 3],
    ) -> Result<(), ObjectError> {
        self.actor(actor)?;
        self.owned_mut(actor)?.state.base.position_words = Some(position);
        Ok(())
    }

    pub(super) fn actor_mut(&mut self, object: ObjectId) -> Result<&mut ActorState, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Actor(actor) => Ok(actor),
            _ => Err(ObjectError::NotActor(object)),
        }
    }

    /// 0047A7A0's completed shared construction. Host evidence errors are
    /// preflighted before publication; this does not reproduce a native fault
    /// on unknown memory or a missing H14 pointer. On handle failure the host
    /// retires its new nodes, retaining any registry growth already performed.
    pub fn allocate_actor(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        self.actor_handles
            .as_ref()
            .ok_or(ActorConstructionError::MissingRegistry)?;
        // Input availability is a portable preflight, like terrain custody.
        // Do not sample the available value until its actual +88 publication.
        if matches!(request.constructor_null, ActorConstructorNull::Process(process)
            if process.process_null_handle.is_none())
        {
            return Err(ActorConstructionError::MissingProcessNull);
        }
        let classification = classify_actor_terrain(request.allocation_position, request.terrain)?;
        // A reached allocation locks the retained policy even if later handle
        // publication fails and this new object is retired.
        self.actor_allocation.begin();
        let object = self.allocate(ObjectClassKind::RotatingVisualBase);
        self.actors.push(object);
        let handle = match self
            .actor_handles
            .as_mut()
            .unwrap()
            .allocate(request.owner, object)
        {
            Ok(handle) => handle,
            Err(error) => {
                self.actors.pop();
                self.destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .expect("new rotating visual allocation");
                return Err(error.into());
            }
        };
        //0047A885 follows handle publication and precedes flags/classification.
        let null_handle = match request.constructor_null {
            ActorConstructorNull::Process(process) => process
                .process_null_handle
                .expect("constructor input availability checked before publication"),
            ActorConstructorNull::RegistrySnapshotApproximation => {
                self.actor_handles.as_ref().unwrap().null_handle()
            }
        };
        let state = &mut self.owned_mut(object)?.state;
        state.base.position_words = request.allocation_position;
        state.class = ObjectClass::Actor(Box::new(ActorState::new(handle, null_handle)));
        let flags = state.base.flags | 0x124;
        mark_flag_change(&mut state.base, flags);
        state.base.flags |= classification.flags_to_or();
        Ok(object)
    }

    /// Resolve through the native cache/slot policy, then authenticate arena
    /// lifetime and class. The packed generation is separate from ObjectId.
    pub fn resolve_actor_handle(
        &mut self,
        handle: ActorHandle,
    ) -> Result<Option<ObjectId>, ActorConstructionError> {
        let object = self
            .actor_handles
            .as_mut()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .resolve(handle);
        if let Some(object) = object {
            self.actor(object)?;
        }
        Ok(object)
    }

    /// Factory mode 1: 004600D0 passes the actor's +84 word to 004625D0,
    /// which marks both that word and the matching registry entry.
    pub(in crate::object_runtime) fn mark_actor_handle_mode1(
        &mut self,
        object: ObjectId,
    ) -> Result<(), ObjectError> {
        let mut handle = self.actor(object)?.handle;
        self.actor_handles
            .as_mut()
            .ok_or(ObjectError::MissingActorRegistry)?
            .mark_mode1(&mut handle);
        self.actor_mut(object)?.handle = handle;
        Ok(())
    }

    /// Factory mode 2: 004600D0's 0047E2B0 releases the actor's handle and
    /// binds the supplied one.
    pub(in crate::object_runtime) fn reassign_actor_handle(
        &mut self,
        object: ObjectId,
        handle: ActorHandle,
    ) -> Result<(), ActorConstructionError> {
        let mut current = self.actor(object)?.handle;
        let result = self
            .actor_handles
            .as_mut()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .reassign(object, &mut current, handle);
        self.actor_mut(object)?.handle = current;
        Ok(result?)
    }

    /// 00462510: the object in the handle's slot, whatever its generation.
    pub fn actor_handle_slot_object(&self, handle: ActorHandle) -> Option<ObjectId> {
        self.actor_handles.as_ref()?.slot_object(handle)
    }

    /// 00462560: an empty slot forgets the handle's generation.
    pub fn forget_actor_handle_slot(&mut self, handle: ActorHandle) -> bool {
        self.actor_handles
            .as_mut()
            .is_some_and(|handles| handles.forget_slot(handle))
    }

    /// 00462480 for factory mode 2: whether the handle's slot is free for it.
    pub fn actor_handle_mode2_admissible(
        &mut self,
        handle: ActorHandle,
    ) -> Result<bool, ActorConstructionError> {
        Ok(self
            .actor_handles
            .as_mut()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .mode2_admissible(handle)?)
    }

    /// 0047F770 scans the actor list from G+48, including disposed actors
    /// until destruction removes their +A8 node. References are signed
    /// one-based DWORDs compared against the sign-extended placement WORD.
    pub fn find_actor_by_placement(&self, reference: i32) -> Option<ObjectId> {
        if reference < 0 {
            return None;
        }
        let wanted = reference - 1;
        self.actors.iter().copied().find(|object| {
            i32::from(
                self.object(*object)
                    .expect("live actor node")
                    .base
                    .placement_word as i16,
            ) == wanted
        })
    }

    /// 0047F7B0 returns the first matching actor's +84, or the explicit
    /// global null handle. This lookup does not touch the handle cache.
    pub fn actor_handle_by_placement(
        &self,
        reference: i32,
    ) -> Result<ActorHandle, ActorConstructionError> {
        let null_handle = self
            .actor_handles
            .as_ref()
            .ok_or(ActorConstructionError::MissingRegistry)?
            .null_handle();
        match self.find_actor_by_placement(reference) {
            Some(object) => Ok(self.actor(object)?.handle),
            None => Ok(null_handle),
        }
    }
}

#[cfg(test)]
mod initialization_native_tests;
#[cfg(test)]
mod initialization_tests;
#[cfg(test)]
pub(super) mod native_tests;
#[cfg(test)]
mod placement_native_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
pub(super) mod vehicle_component_tests;
