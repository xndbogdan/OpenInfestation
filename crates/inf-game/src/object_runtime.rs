//! Live common objects and ordered membership from 00479C60..00479FA0.
//!
//! Each object owns three distinct embedded nodes. Native insertion uses a
//! tail-sentinel predecessor, so these lists preserve append/remove order;
//! parking must not collapse multiple nodes belonging to the same object.
//! Paired heap-list owners can swap without changing physical membership.
//! The visual and rotating prefixes, selector-19 lights and selector-51 route
//! groups, selector-86 emitters and shared/derived actor construction are concrete.
//! The shared Flag0 pass owns admitted +28 light callbacks; other manager passes,
//! model drawing and unowned derived continuations retain separate boundaries.

use std::sync::atomic::{AtomicU64, Ordering};

use thiserror::Error;

mod sound_destruction;
pub use sound_destruction::{ObjectDestructionRequest, SoundTeardownService};
mod pickup;
pub use pickup::{
    PickupBirthHost, PickupBirthRequest, PickupClass, PickupCollectionEffect, PickupDrawHost,
    PickupDrawRequest, PickupError, PickupNetwork, TimedPickupState,
};
mod actor;
pub mod actor_record;
pub use actor::{
    ActorAttachmentError, ActorAttachmentHost, Selector66Error, Selector66Host, Selector66Particle,
    Selector66State,
};
pub use actor::{ActorMessageError, ActorMessageReply, ActorMessageWalk};
pub use actor::{
    Bomb29Message, BombLifecycleHost, BOMB_DROPPED_HINT, BOMB_SELECTOR, BOMB_TAKEN_HINT,
};
#[cfg(test)]
mod class_destructors;
mod disposal;
pub use actor::{
    ActorChildError, ActorChildFactoryCall, ActorChildHost, ActorChildRequest, ActorChildResource,
};
pub use actor::{
    ActorWeaponFireContext, ActorWeaponFireError, ActorWeaponFireMode, ActorWeaponFireRequest,
};
pub use actor::{
    DetonatorPulseHost, WeaponEmissionError, WeaponEmissionLimit, WeaponEmissionRequest,
    WeaponFiringContext, WeaponFiringDrainError, WeaponFiringDrainSummary, WeaponFiringEmitter,
    WeaponFiringRuntime,
};
pub use actor::{
    FamilyDyingAudio, FamilyDyingEffect, FamilyDyingError, FamilyDyingHost, FamilyDyingRequest,
    FamilyDyingSpawnRequest,
};
pub use actor::{FamilyPairReactionError, FamilyPairReactionRequest, FamilyRescueAccept};
pub use actor::{Selector12DyingError, Selector12DyingHost, Selector12DyingRequest};
pub use disposal::{
    ObjectDisposalError, ObjectDisposalFailurePolicy, ObjectDisposalPhase, ObjectDisposalReport,
    ObjectDisposalRequest, ObjectDisposalState, ObjectDisposalStop,
};
mod actor_handles;
mod actor_terrain;
mod allocation;
mod attract_placement;
mod construction;
mod creation_listeners;
mod furniture_collision;
mod grants;
pub(crate) mod h0_text;
mod light_pass;
mod map_annotation;
mod model_callback;
mod model_custom_vertex;
#[cfg(test)]
mod native_model_fixture;
mod object_draw;
mod player;
mod profiling;
mod race;
mod rotating;
mod selector45;
mod selector46;
mod selector58;
pub use race::{
    encode_race_records, gate_beam, gate_sprite_slot, native_qsort, receive_race_records,
    vehicle_network, GateState, GoalState, RaceError, RaceObject, RacePart, RaceRequest, RaceState,
    RacerRecord, WrongWayState, POST_SINK, RACERS,
};
pub use selector45::Selector45State;
pub use selector46::{Selector46State, TerritoryError, TerritoryRequest};
pub use selector58::{Selector58Error, Selector58Host, Selector58State, PAD_SOUND};
mod selector83;
pub use selector83::{Selector83Error, Selector83Host, Selector83State, Selector83Table};
mod selector76;
pub use selector76::{Selector76Error, Selector76Host, Selector76Light, Selector76State};
mod route;
mod selector22;
mod selector42;
mod selector64;
mod selector65;
pub use selector42::Selector42State;
pub use selector64::{
    Selector64PassError, Selector64PassHost, Selector64State, SELECTOR64_LINK_MESSAGE,
};
mod selector43;
pub use selector43::{
    Selector43Error, Selector43Host, Selector43State, ERUPTION_EVENT, ROCK_RESOURCE,
};
mod selector48;
pub use selector48::{Selector48Error, Selector48Host, Selector48State, CEILING_HINT};
mod selector81;
pub use selector81::{Selector81Error, Selector81Host, Selector81State, MEMBER_RESOURCE};
mod selector94;
pub use selector94::{Selector94Error, Selector94Host, Selector94State};
mod selector50;
pub use selector50::{selector50_step, Selector50Error, Selector50State, Selector50Step};
mod selector67;
mod selector75;
pub use selector75::{Selector75Cleanup, Selector75Error, Selector75State};
mod selector86;
mod selector93_factory;
mod targets;
mod visual;
pub(crate) use visual::{resident_model_metadata, resident_model_resource, ResidentModelBinding};
mod world_cleanup;
mod world_contact;
mod world_load;
mod world_load_attract;
pub(crate) use actor::common_update::{RngRemoteHost, StatusTrailHost};
pub(crate) use actor::native_trunc_low32;
pub use actor::{
    apply_contact_geometry, terrain_furniture_grant, terrain_sphere_contact, CollisionContact,
    ContactAxisNormals, ContactGeometryRequest, ContactSphere, FamilyAttackUpdateError,
    FamilyAttackUpdateHost, FamilyAttackUpdateRequest, FamilyAttackUpdateServices,
    FamilyDescriptorScoreError, FamilyDescriptorScoreKind, FamilyDescriptorScoreRequest,
    FamilyDescriptorScoreRng, FamilyDormantError, FamilyDormantRequest, FamilyEvadeEntryKind,
    FamilyEvadeEntryRequest, FamilyFollowPlayerError, FamilyGrantGridError, FamilyGrantGridHost,
    FamilyGrantGridPointRequest, FamilyHelperHistoryError, FamilyHelperHistoryInputs,
    FamilyHelperHistoryRecord, FamilyHelperQueryRequest, FamilyHelperRecordRequest,
    FamilyHerdingError, FamilyObjectTargetChangeError, FamilyObjectTargetChangeRequest,
    FamilyObjectTargetUpdateError, FamilyPointTargetChangeError, FamilyPointTargetChangeKind,
    FamilyPointTargetChangeRequest, FamilyRoamUpdateError, FamilyRoamUpdateRequest,
    FamilyTerrainGrantRequest, FamilyTimedDescriptorError, FamilyTimedDescriptorKind,
    FamilyTimedDescriptorRequest, Selector12DescriptorChangeError, Selector12DescriptorChangeKind,
    Selector12DescriptorChangeRequest, Selector12GrantGridRequest, Selector12PointUpdateError,
    Selector12PointUpdateRequest, Selector12PreferredUpdateError, Selector12PreferredUpdateRequest,
    Selector12TailError, Selector12TailProbeError, Selector12TailRequest, TerrainSphereError,
    TerrainSphereGrid,
};
pub use actor::{
    encode_ball, read_ball_sync, BallError, BallHost, BallState, BallSync, GOAL_WON,
    SOUND_DOUBLE_GOAL, SOUND_GOAL, SOUND_KICK_OFF,
};
pub use actor::{
    force_field_dome, force_field_normal, laser31_draw, selector69_beam_width, selector69_beams,
    selector69_draw, Actor40d800State, ActorAngularAxis, ActorAngularMotionRequest,
    ActorAngularMotionTable, ActorAngularResponseRequest, ActorBehaviorGroup,
    ActorBehaviorPositionError, ActorBehaviorPositionRequest, ActorBehaviorState,
    ActorBehaviorTable, ActorBehaviorTickError, ActorCommandContext, ActorCommandError,
    ActorConstructionError, ActorConstructionRequest, ActorConstructorNull, ActorDamageCallback,
    ActorDamageError, ActorDamageHost, ActorDamageOperation, ActorDamagePacket,
    ActorDamageReactionEffect, ActorDamageReactionError, ActorDamageReactionHost,
    ActorDamageRequest, ActorDamageTable, ActorExtension, ActorFamilyDamageError,
    ActorFamilyDamageHost, ActorFamilyDamageOperation, ActorFamilyTargetError,
    ActorFamilyTargetRequest, ActorForwardMotionRequest, ActorForwardMotionTable,
    ActorInitializationRequest, ActorLateralMotionRequest, ActorLateralMotionTable, ActorMessage,
    ActorMotionError, ActorMotionTables, ActorPhysicsRequest, ActorRegistryConfig,
    ActorRetaliationError, ActorState, ActorStateChangeEffect, ActorStateChangeError,
    ActorStateChangeHost, ActorStateChangeQuery, ActorStateChangeRequest, ActorTerrainHost,
    ActorTimedDamageError, ActorTimedDamageHost, ActorTimedDamageRequest, ActorWaterDamage,
    ActorWaterPlane, ArenaIntroError, ArenaIntroHost, ArenaIntroInputs, ArenaIntroRequest,
    ArenaIntroServices, ArenaIntroTickError, BannerError, BannerHost, BannerRequest,
    BehaviorResourceClass, BehaviorResourceState, BoulderParticle, CameraDerivedComponent,
    CameraDerivedTargetMode, CameraDerivedTargetRequest, CameraDrawRequest, CameraGlobalState,
    CameraHeightRequest, CameraParameterBlock, CameraParameterState, CameraPose, CameraPoseError,
    CameraPoseHost, CameraPoseTickRequest, CameraSettings, CameraSlewError, CameraSlewRequest,
    DerivedActorClass, DerivedActorConstructionRequest, DerivedActorError, DerivedActorState,
    DomePoint, DomeQuad, Family4236d0Class, Family4236d0InitializationRequest, Family4236d0State,
    Family428650Class, Family428650State, Family429e20Class, Family429e20State, Family455560State,
    Family455560Tail, Family485f50State, FamilyAttackError, FamilyAttackHost, FamilyAvoidanceError,
    FamilyAvoidanceHost, FamilyBehaviorDescriptor, FamilyBehaviorDispatchRequest,
    FamilyBehaviorError, FamilyBehaviorHost, FamilyBehaviorProfile, FamilyCollisionGuardError,
    FamilyCollisionGuardHost, FamilyCollisionGuardRequest, FamilyHandleEntry, FamilyHelper482ff0,
    FamilyIntentError, FamilyObstacleProbeHost, FamilyObstacleScanError,
    FamilyPeriodicProbeRequest, FamilyPeriodicRecord, FamilyPlayerWeaponError,
    FamilyRadiusScoreInput, FamilyTargetProducerError, FamilyTargetProducerHost,
    FamilyTargetProducerMode, FamilyTargetProducerRequest, FamilyTargetSlot,
    FamilyTerrainProbeError, FamilyTerrainProbeMissError, FamilyTerrainProbeMissHost,
    FamilyUpdateClass, FamilyUpdateEffect, FamilyUpdateError, FamilyUpdateHost,
    FamilyUpdateProgress, FamilyUpdateRequest, ForceFieldBits, ForceFieldDome, ForceFieldDrawError,
    GateEntry, GateRecord, Grenade26Error, Grenade26Host, Grenade26Light, HeldCameraError,
    HeldCameraHost, ImplantTakeHost, InstanceCommandAllocation, InstanceCommandAllocationError,
    IntroAdmissionError, IntroAdmissionInputs, IntroCallbackHost, Laser31Beam, Laser31DrawError,
    MissileError, MissileHost, MissileLifecycleHost, MissileLight, MissileSteerHost,
    NativeCameraHost, NativeCameraInputs, NativeCameraServices, NativeFamilyUpdateError,
    NativeFamilyUpdateHost, NativeIntroServices, NativeIntroServicesError, PrisonHost,
    RetainedActorAllocation, RetainedInstanceCommand, RetainedSelector34Data,
    RetainedSelector35Data, RetainedSpawnList, Selector10Helper, Selector10HelperChild,
    Selector10HelperChildClass, Selector10HelperClass, Selector10State, Selector11RadiusBlock,
    Selector11RadiusError, Selector11RadiusOrigin, Selector11RetirementError,
    Selector11RetirementHost, Selector11ScanHost, Selector11State, Selector11TickRequest,
    Selector11UpdateError, Selector11UpdateHost, Selector12GrantContext,
    Selector12GrantDifferenceError, Selector12GrantDifferenceRequest,
    Selector12InitializationRequest, Selector12MotionError, Selector12MotionRequest,
    Selector12State, Selector12TerrainHost, Selector16State, Selector17Error, Selector17State,
    Selector18Error, Selector18Host, Selector18State, Selector2State, Selector30Error,
    Selector30PassError, Selector30PassHost, Selector30State, Selector30TickRequest,
    Selector30UpdateError, Selector30UpdateHost, Selector34AttackContext, Selector34AttackError,
    Selector34ControlsError, Selector34ControlsRequest, Selector34DyingEffect,
    Selector34DyingError, Selector34DyingHost, Selector34DyingRequest,
    Selector34InitializationRequest, Selector34MotionControls, Selector34MotionError,
    Selector34MotionHost, Selector34MotionRequest, Selector34PersistentAudioError, Selector34State,
    Selector35ClearanceHost, Selector35DescriptorContext, Selector35DescriptorError,
    Selector35DescriptorRequest, Selector35InitializationRequest, Selector35LifecycleEffect,
    Selector35LifecycleError, Selector35LifecycleHost, Selector35LifecycleRequest,
    Selector35MotionControls, Selector35MotionError, Selector35MotionRequest,
    Selector35PersistentAudioError, Selector35PickupError, Selector35RouteProcessState,
    Selector35State, Selector35TailError, Selector35TailRequest, Selector36State, Selector40State,
    Selector44DeathHost, Selector44Error, Selector44Host, Selector44State, Selector47Error,
    Selector47Host, Selector47State, Selector49DeathHost, Selector49Error, Selector49State,
    Selector49UpdateHost, Selector52Error, Selector52Host, Selector52State, Selector53Error,
    Selector53State, Selector55State, Selector57DeathHost, Selector57Error, Selector59ContactError,
    Selector59ContactHost, Selector59Error, Selector59State, Selector60Error, Selector60Row,
    Selector60State, Selector69Bolt, Selector69DrawError, Selector69Error, Selector69Fit,
    Selector69Geometry, Selector69Host, Selector69Message, Selector69Projected, Selector69Quad,
    Selector69State, Selector69UnwrittenFog, Selector73State, Selector77State, Selector79Message,
    Selector79State, Selector80State, Selector82Host, Selector88IntroError, Selector88IntroScript,
    Selector88State, Selector90Byte, Selector90Error, Selector90MotionMode, Selector90State,
    Selector90TickRequest, Selector90Word, Selector93State, Shot32Error, Shot32Host,
    TerrainProbeAllocation, TerrainProbeCache, TerrainProbeError, VehicleCarryContext,
    VehicleCarryEffect, VehicleCarryError, VehicleCarryHost, VehicleDamageError, VehicleDamageHost,
    VehicleDeathError, VehicleDriveError, VehicleDriveHost, VehicleDriveRequest, VehicleDriveState,
    VehicleDropHost, VehicleDyingHost, VehicleDyingRequest, VehicleGround, VehicleHandleArray,
    VehicleMessageEffect, VehicleMessageError, VehicleNetwork, VehiclePositionStep,
    VehicleUpdateHost, VehicleUpdateReport, VehicleUpdateRequest, VehicleUpdateState,
    VehicleWreckPending, WakeParticle, BARRIER_AGE_RESIDUE_APPROXIMATION, BARRIER_MODE_0052F628,
    BARRIER_TRACE_RESOURCE, CAPTURED_STATUS_TEXT, ECM_FLAG, ECM_GLOW_PHASE_APPROXIMATION,
    FORCE_FIELD_BAND_SPRITE, FORCE_FIELD_BOTTOM_SPRITE, FORCE_FIELD_BOUNCE_CODE,
    FORCE_FIELD_DECAL_SPRITE, FORCE_FIELD_FADE, FORCE_FIELD_LIGHT_GREY, FORCE_FIELD_LIGHT_RGB,
    FORCE_FIELD_MODE, FORCE_FIELD_NOT_SECURED_HINT, FORCE_FIELD_NOT_SECURED_TEXT,
    FORCE_FIELD_SECURED_TEXT, LASER_MODE, LASER_NEGATIVE_TEXTURE_SLOT, LASER_TEXTURE_SLOT,
    LASER_WIDTH_RESIDUE_APPROXIMATION, POWERBALL_PARTNER_RESOURCE, POWERBALL_RETIRE_MESSAGE,
    ROCK_BLASTS, ROCK_GLOW, ROCK_RETIRE_MESSAGE, ROCK_VISUALS, SELECTOR52_MINE_CALL,
    SHOT32_LIFE_MICROS, SIREN_SOUND, VEHICLE_COLLISION_PITCH, VEHICLE_COLLISION_SOUND,
    VEHICLE_HIT_FLASH_RADIUS,
};
pub use actor::{
    segment_probe_43a380, selector10_custom_425870, SegmentProbe, Selector10CustomInputs,
    Selector10CustomServices, Selector10LegError, Selector10LegGround, WheelGround,
};
pub use actor::{
    selector74_point_height, Selector74Error, Selector74Host, Selector74State, SELECTOR74_PARTICLE,
};
pub use actor::{ActorAllocationPolicy, ActorAllocationPolicyError};
pub use actor::{
    ActorAudioCameraHost, ActorContactEnvironment, ActorContactError, ActorContactHeaderHost,
    ActorContactOutput, ActorContactRequest, ActorContactStackPolicy, Selector36MotionError,
    Selector36MotionNextPhase, Selector36MotionRequest, Selector36PersistentAudioError,
    Selector36PersistentAudioRequest,
};
pub use actor::{
    ActorCaptureContext, ActorCaptureError, ActorCaptureHost, ActorCapturePlayerCommand,
    ActorCaptureRequest, ActorCaptureSavedFlags,
};
pub use actor::{
    ActorComponentQueryMode, ActorComponentQueryRequest, ActorInteractionError,
    ActorInteractionNotificationContext, ActorInteractionNotificationRequest,
    ActorInteractionRelationRequest,
};
pub use actor::{
    ActorGrantDifferenceDelivery, ActorGrantDifferenceNotificationError,
    ActorGrantDifferenceNotificationRequest, ActorVehicleCleanupNotificationError,
    ActorVehicleCleanupNotificationRequest,
};
pub use actor::{
    AlienBrainWords, Selector85Error, Selector85Host, Selector85State, BRAIN_INVULNERABILITY,
};
pub use actor::{
    BehaviorActorResourceRequest, BehaviorCacheStorageError, BehaviorComponentPacketSourceRequest,
    BehaviorComponentQuery, BehaviorCreationRequest, BehaviorPacketCopy, BehaviorPacketSlot,
    BehaviorPacketSourceRequest, BehaviorParticleAdapter, BehaviorParticleAdapterError,
    BehaviorParticleCreationSources, BehaviorParticleEffectRequest, BehaviorParticleServiceContext,
    BehaviorParticleServices, BehaviorPredicateContext, BehaviorPredicateError,
    BehaviorPredicateHost, BehaviorPredicateRequest, BehaviorRowSelection, BehaviorTemporary,
    FamilyCandidateGateError, FamilyCandidateGateRequest, ProcessActorBehaviorTables,
};
pub use actor::{
    BridgeTerrain, FactoryError, FactoryHost, FactoryStaffHost, FamilyDeathError, FamilyDeathHost,
    FamilyLinkChange, Selector37Error, Selector37Host, Selector37State, Selector39AfterLoad,
    Selector39Countdown, Selector39DeathHost, Selector39Error, Selector39Host,
    Selector39LinkDelivery, Selector39MissionWrite, Selector39State, Selector39UpdateHost,
    StaffReleaseHost, UnitBroadcast, UnitBroadcastCode, UnitBroadcastOutcome, BRIDGE_SOUND,
    FUSE_SOUND,
};
pub use actor::{
    EyeDeathError, EyeDeathHost, Selector77Shot, Selector77TickRequest, Selector77UpdateError,
    Selector77UpdateHost, Selector93TickRequest, Selector93UpdateError, EYE_DEATH_CHILD,
    EYE_STALK_MODEL,
};
pub use actor::{Family4236d0MotionError, Family4236d0MotionRequest};
pub use actor::{
    FamilyActorListSelectionError, FamilySelectionCandidate, FamilySelectionError,
    FamilySelectionHost, FamilySelectionRequest, FamilySelectionServices, FamilyTargetScanError,
    FamilyTargetScanOutputs, FamilyTargetScanRequest, FamilyTargetSelection, FamilyTargetTable,
    FamilyTargetTableError,
};
pub use actor::{
    ProcessActorMotionState, Selector36ChildServiceError, Selector36TailNextPhase,
    Selector36TailPrefixError, Selector36TailPrefixRequest,
};
pub use actor::{
    Selector10HelperTickError, Selector10HelperTickRequest, Selector10MotionError,
    Selector10MotionNextPhase, Selector10MotionRequest, Selector10PersistentAudioError,
    Selector10TailNextPhase, Selector10TailPrefixError, Selector10TailPrefixRequest,
};
pub use actor::{
    Selector61DeathHost, Selector61Error, Selector61Host, Selector61State, Selector61UpdateRequest,
    PURSUIT_SHIP_BLAST, PURSUIT_SHIP_EXPLOSION, PURSUIT_SHIP_VISUAL,
};
pub use actor::{
    Selector62Error, Selector62Host, Selector62State, SELECTOR62_BLAST, SELECTOR62_BLOCKS,
    SELECTOR62_DURATIONS, SELECTOR62_LEAVING_BLAST,
};
pub use actor::{Selector78Error, Selector78Host, Selector78State, ENDING_COMPLETION_BONUS};
pub use actor_handles::{
    ActorHandle, ActorHandleCache, ActorHandleError, ActorHandleOwner, ActorHandleSlot,
    ActorHandles,
};
pub use attract_placement::{
    setup_attract_actor, AttractActorSetupError, AttractActorSetupRequest, AttractActorSetupState,
};
pub use furniture_collision::{
    collide_furniture_prefix, FurnitureCollisionPair, FurnitureCollisionPairState,
    FurnitureCollisionPrefixError, FurnitureCollisionPrefixHost, FurnitureCollisionPrefixRequest,
};
pub(crate) use grants::grant_record_text;
pub use grants::{
    GrantOwner, LiveGrantContext, LiveGrantError, LiveGrantOperation, LiveGrantRequest,
};
pub use model_callback::{guided_mount_deploy, ObjectModelCallbackError, ObjectModelCallbacks};
pub use model_custom_vertex::{
    ObjectCustomVertexEffects, ObjectCustomVertexError, ObjectCustomVertexRequest,
    ObjectLegServices,
};
pub use object_draw::{
    CulledObject, Flag0ObjectDrawError, Flag0ObjectDrawFailurePolicy, Flag0ObjectDrawHost,
    Flag0ObjectDrawOmission, Flag0ObjectDrawOutcome, Flag0ObjectDrawRequest,
    ObjectDrawCallbackBoundary, ObjectDrawCone, ObjectDrawContext, ObjectDrawLighting,
    ObjectDrawProcessState, ObjectDrawServiceError, ObjectModelDrawSubmission, ObjectModelPacket,
    VehicleNameTag,
};
pub use profiling::{ProcessProfiling, ProfilingError, ProfilingPhase, ProfilingRequest};
pub use world_cleanup::{WorldCleanupError, WorldCleanupRequest};
pub use world_load_attract::WorldLoadAttractRequest;
// Re-exported for the world-load robustness probe, which is the only
// consumer. Clippy's unused-imports lint does not see test-only uses of a
// `pub(crate)` re-export, hence the targeted allow.
pub(crate) use actor::BEHAVIOR_RESOURCE_SELECTORS;
pub use actor::{
    Selector60SpawnResult, Selector60SpawnRow, Selector60UpdateError, Selector60UpdateHost,
};
#[allow(unused_imports)]
pub(crate) use actor_terrain::plane_height_at;
pub use actor_terrain::{
    classify_actor_contact, classify_actor_terrain, ActorTerrainClassification, ActorTerrainError,
    ActorTerrainInputs, ActorTerrainSurface,
};
pub(crate) use allocation::refusing_writes;
pub use allocation::{RuntimeAllocation, RuntimeAllocationError, RuntimePointer, Selector60RowId};
pub(crate) use construction::{
    construct_behavior_resource, construct_world_object_from_source, resident_constructor_selector,
    ConstructionFailurePolicy, ConstructionPositionSource,
};
pub use construction::{
    construct_world_object, ActorFactoryInputs, DerivedActorFactoryInputs, ObjectCommandRejection,
    ObjectConstructionError, ObjectConstructionOutcome, ObjectConstructionRequest,
    ObjectInstanceCommands, VehicleFactoryInputs,
};
pub use creation_listeners::{
    CreationListener, CreationListenerEffects, CreationListenerError, CreationListenerId,
    CreationListenerInterface, CreationListeners,
};
pub use light_pass::{
    Flag0LightFailurePolicy, Flag0LightPassError, Flag0LightPassOmission, Flag0LightPassOutcome,
    Flag0LightPassRequest, ObjectLightCallbackBoundary, ObjectLightPolicySource, RearLamps,
    VehicleLamp,
};
pub use map_annotation::MapAnnotationState;
#[cfg(test)]
pub(crate) use player::player_analog_fixture;
pub(crate) use player::WorldCameraLookup;
pub use player::{
    is_player_key_callback, PlayerControlsUpdate, PlayerKeyError, PlayerKeyEvent, PlayerKeyInputs,
    PlayerKeyOmission,
};
pub use player::{
    CameraFactoryInputs, GrantNotification, LoaderPlayerSetup, NetworkRespawn, PickupDelivery,
    PickupDeliveryError, PickupMessage, PickupMessageError, PickupOutcome, PickupSound,
    PickupTables, PlayerActorLookupContext, PlayerActorLookupError, PlayerActorLookupRequest,
    PlayerAttachError, PlayerCommandContext, PlayerCommandError, PlayerCommandOperation,
    PlayerCommandOutcome, PlayerCommandRequest, PlayerControlBlock, PlayerEquipment,
    PlayerEquipmentHost, PlayerEquipmentWorld, PlayerId, PlayerInputConsumers, PlayerMessage88,
    PlayerMessageError, PlayerMessageOperation, PlayerMessageRequest, PlayerMessageState,
    PlayerNetworkState, PlayerRespawn, PlayerRespawnInputs, PlayerSpawnRequest, PlayerUpdateError,
    PlayerUpdateInputs, PlayerVehicleCall, RespawnMessage, WorldCameraRefresh, WorldPlayer,
};
pub use player::{
    ComputerCheats, DatabaseEntry, FreeLookPick, FreeLookSource, HudReticleSettings, MessageBox,
    MessageBoxKind, MissileViewSource, PlayerHudError, PlayerHudUnit, PlayerMessageBoxes,
    PlayerMessageDraw, PlayerMessageEffect, PlayerMessageEnvironment, PlayerMessageMenus,
    ShownMenu, ShownMenuAction, ShownMenuOwner, DATABASE_ENTRIES, DATABASE_SELECT, HINT_COUNT,
};
pub use player::{
    PlayerConstructionError, PlayerConstructionPhase, PlayerDestructionError,
    PlayerDestructionPhase, PlayerInputDescriptor, PlayerInputDescriptorKind,
    PlayerInputDescriptorRequest, PlayerInputDescriptorSource, PlayerInputError,
    PlayerInputReleaseError, WorldPlayerConstruction,
};
pub use rotating::{RotatingVisual, RotatingVisualContext};
pub use route::{RouteGroup, RouteGroupId, RouteNode};
pub use selector22::Selector22State;
pub use selector65::{Selector65Error, Selector65Host, Selector65State, SELECTOR65_LIMIT};
pub use selector67::{
    construct_selector67_ribbon, ribbon_sort_key, ribbon_sort_word, RibbonCorner, RibbonDrawError,
    RibbonDrawInput, RibbonNode, RibbonQuad, RibbonSortError, Selector67Error,
    Selector67FactoryError, Selector67FactoryRequest, Selector67Host, Selector67State,
    RIBBON_MODE_0053C568, SELECTOR67_POSITION_APPROXIMATION,
};
pub use selector86::{
    RetainedSelector86Data, Selector86State, Selector86Trail, Selector86UpdateError,
    Selector86UpdateHost,
};
pub use selector93_factory::{
    construct_selector93_direct, Selector93FactoryError, Selector93FactoryRequest,
};
#[cfg(test)]
pub use targets::FixtureActorServiceAllocation;
pub use targets::{
    ActorFollowTarget, FollowTargetPoint, FollowTargetRequest, NativeHeapRef, NativeTarget,
    NativeTargetClass, NativeTargetError, NativeWeaponFireNode, NativeWeightedTarget,
    ObjectPositionTarget, TargetNullHandleSource, WeightedTargetConstructorNull,
    WeightedTargetReferent, WeightedTargetRequest,
};
pub use targets::{
    ActorServiceAllocationPolicy, ActorServiceAllocationRequest, ActorServiceHeap,
    ActorServiceHeapError, ActorServiceResourceKind, ActorServiceResourceState,
    FixedPositionTarget, FurniturePositionTarget, NativeTargetQueryError,
    PROCESS_VELOCITY_0055F0F8,
};
pub use world_contact::{
    ActorPairError, ActorPairHost, FurnitureContactResponse, FurnitureResponseError,
    FurnitureResponseHost, NetworkImpactGate, TerrainDestructionEffect, WorldContactError,
    WorldContactHost, WorldContactPhases, WorldPairHost,
};
pub use world_load::{
    PickupCompletionWords, WorldLoadDisposal, WorldLoadError, WorldLoadState, AFTER_LOAD_FLAG,
};

static NEXT_ARENA: AtomicU64 = AtomicU64::new(1);

pub const INITIALIZED_FLAG: u32 = 0x0010_0000;

/// A live allocation, distinct across both arena lifetimes and reused slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    arena: u64,
    slot: usize,
    generation: u64,
}

impl ObjectId {
    /// The arena slot, as logs name it.
    pub fn slot(self) -> usize {
        self.slot
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ObjectError {
    #[error("actor {0:?} left the actor list while a message walk stood on it")]
    LeftActorList(ObjectId),
    #[error("flamethrower {0:?} reads its thrower +6C before 00431EF0 wrote it")]
    UnarmedFlamethrower(ObjectId),
    #[error("object {object:?} disposal previously stopped during {phase:?}")]
    StoppedDisposal {
        object: ObjectId,
        phase: ObjectDisposalPhase,
    },
    #[error(transparent)]
    Sound(#[from] crate::process_sound::ProcessSoundRecordError),
    #[error(transparent)]
    ActorServiceHeap(#[from] ActorServiceHeapError),
    #[error(transparent)]
    Selector30(#[from] Selector30Error),
    #[error("object {0:?} is not in the 00428650 actor family")]
    NotFamily428650(ObjectId),
    #[error("object {0:?} is not in the 00429E20 actor family")]
    NotFamily429e20(ObjectId),
    #[error("object {0:?} is not in the 004236D0 actor family")]
    NotFamily4236d0(ObjectId),
    #[error("actor {0:?} requires the selected world's four completion words (an arena world's are game fields past the mission)")]
    MissingWorldCompletion(ObjectId),
    #[error(transparent)]
    Selector60(#[from] Selector60Error),
    #[error("unknown or expired object {0:?}")]
    UnknownObject(ObjectId),
    #[error("object {0:?} has no visual state")]
    NotVisual(ObjectId),
    #[error("object {0:?} is not a timed pickup")]
    NotTimedPickup(ObjectId),
    #[error("object {0:?} is not a 0040D800-family actor")]
    NotFamily40d800(ObjectId),
    #[error("family actor {0:?} lacks the 0040D950 payload extension")]
    MissingFamily40d950Payload(ObjectId),
    #[error("object {0:?} is not a selector-58 pad")]
    NotSelector58(ObjectId),
    #[error("object {0:?} is not a selector-45 spawn marker")]
    NotSelector45(ObjectId),
    #[error("object {0:?} is not a selector-46 territory")]
    NotSelector46(ObjectId),
    #[error("object {0:?} is not the race's selector 70, 71 or 72")]
    NotRaceObject(ObjectId),
    #[error("object {0:?} is not a selector-83 sinking patch")]
    NotSelector83(ObjectId),
    #[error("object {0:?} is not a selector-15 factory")]
    NotSelector15(ObjectId),
    #[error("object {0:?} is not a selector-37 actor")]
    NotSelector37(ObjectId),
    #[error("object {0:?} is not a selector-39 actor")]
    NotSelector39(ObjectId),
    #[error("link message to {0:?} reaches an unowned +98 receiver")]
    LinkMessageBoundary(ObjectId),
    #[error("placed pickup {0:?} reached its network-only 0046D7D0/0046D950 update path")]
    PlacedPickupNetworkUpdate(ObjectId),
    #[error("placed pickup {0:?} needs the loader's mission pickup words for 0048E930")]
    PickupMissionWordsUnknown(ObjectId),
    #[error("timed pickup {0:?} requires delta-aware 0046E0D0 update")]
    TimedPickupUpdateBoundary(ObjectId),
    #[error("timed pickup {0:?} destruction requires its initialized +44 kind")]
    MissingPickupKind(ObjectId),
    #[error("object {0:?} has no rotating visual state")]
    NotRotatingVisual(ObjectId),
    #[error("object {0:?} has no actor state")]
    NotActor(ObjectId),
    #[error("object {0:?} is not a selector-88 actor")]
    NotSelector88(ObjectId),
    #[error("object {0:?} is not a selector-90 helper")]
    NotSelector90(ObjectId),
    #[error("object {0:?} is not selector66")]
    NotSelector66(ObjectId),
    #[error("object {0:?} is not the constructor-93 demo actor")]
    NotSelector93(ObjectId),
    #[error("object {0:?} is not a selector-53 pickup spawner")]
    NotSelector53(ObjectId),
    #[error("object {0:?} is not a selector-52 mining rig")]
    NotSelector52(ObjectId),
    #[error("object {0:?} is not a selector-57 Powerball")]
    NotSelector57(ObjectId),
    #[error("object {0:?} is not a selector-40 banner")]
    NotSelector40(ObjectId),
    #[error("object {0:?} is not a selector-44 falling rock")]
    NotSelector44(ObjectId),
    #[error("object {0:?} is not a selector-73 ECM Unit")]
    NotSelector73(ObjectId),
    #[error("object {0:?} is not a selector-55 Remote Beamer")]
    NotSelector55(ObjectId),
    #[error("object {0:?} is not a selector-17 Prison Unit")]
    NotSelector17(ObjectId),
    #[error("Prison Unit {0:?}'s message 1 drops its prisoner (00412880), which the message path does not lend the hold's owners for")]
    PrisonerDrop(ObjectId),
    #[error("actor {actor:?} and {other:?} compare their +6C identities, whose bits retail sets from stack residue")]
    UnknownIdentity6c { actor: ObjectId, other: ObjectId },
    #[error("object {0:?} is not a selector-74 zone")]
    NotSelector74(ObjectId),
    #[error("object {0:?} is not a selector-61 Pursuit Ship")]
    NotSelector61(ObjectId),
    #[error("object {0:?} is not a selector-62 attack")]
    NotSelector62(ObjectId),
    #[error("zone {0:?} reaches its commands or finish before 0042D780 wrote its extents")]
    Selector74ExtentsUnwritten(ObjectId),
    #[error("zone {0:?} lacks 0042D780's payload reads")]
    MissingSelector74Payload(ObjectId),
    #[error("object {0:?} is not a selector-75 furniture marker")]
    NotSelector75(ObjectId),
    #[error("object {0:?} is not a selector-84 capture node")]
    NotSelector84(ObjectId),
    #[error("object {0:?} is not a selector-85 brain")]
    NotSelector85(ObjectId),
    #[error("object {0:?} is not a selector-78 ending director")]
    NotSelector78(ObjectId),
    /// 00407382: the listener writes helper +164, which state 1 never built.
    #[error("ending director {0:?}'s listener reaches its unbuilt helper +164")]
    Selector78Helper(ObjectId),
    #[error("furniture marker {0:?}'s finish 00435310 tests a completion bit, which needs the loader's world completion words")]
    Selector75CompletionUnknown(ObjectId),
    #[error("furniture marker {0:?} has no position")]
    Selector75Position(ObjectId),
    #[error("object {0:?} is not a selector-59 boulder")]
    NotSelector59(ObjectId),
    #[error("object {0:?} is not Powerball's ball (selector 91)")]
    NotBall(ObjectId),
    #[error("object {0:?} is not a selector-18 Force Field")]
    NotSelector18(ObjectId),
    /// The Force Field's 004768D0 and the ending director's 00406750
    /// unbind the object's creation listeners.
    #[error("{0:?}'s destruction needs the creation-listener list its destructor unbinds")]
    ListenerOwnerBoundary(ObjectId),
    #[error("object {0:?} is not a selector-47 Siren")]
    NotSelector47(ObjectId),
    #[error("object {0:?} is not a selector-69 barrier")]
    NotSelector69(ObjectId),
    #[error("object {0:?} is not a selector-49 Seismic Stabiliser")]
    NotSelector49(ObjectId),
    #[error("object {0:?} is not one of the behavior registry's resource classes")]
    NotBehaviorResource(ObjectId),
    #[error(
        "behavior resource {object:?} (constructor {selector}) has another initializer's payload"
    )]
    BehaviorResourcePayload { object: ObjectId, selector: u8 },
    #[error("behavior resource {0:?} reached an unported destructor effect")]
    BehaviorResourceTeardown(ObjectId),
    #[error("behavior resource {object:?} reached its unported virtual +{slot:02X}")]
    BehaviorResourceVirtual { object: ObjectId, slot: u8 },
    #[error("object {0:?} is not a derived actor")]
    NotDerivedActor(ObjectId),
    #[error("vehicle {0:?} holds a +54C sound record only an unported mode creates")]
    VehicleSound54c(ObjectId),
    #[error("object {0:?} is not a selector-86 emitter")]
    NotSelector86(ObjectId),
    #[error("object {0:?} is not a selector-22 fading visual")]
    NotSelector22(ObjectId),
    #[error("object {0:?} is not selector 65")]
    NotSelector65(ObjectId),
    #[error("object {0:?} is not a selector-43 spawner")]
    NotSelector43(ObjectId),
    #[error("object {0:?} is not a selector-81 spawner")]
    NotSelector81(ObjectId),
    #[error("object {0:?} is not the missile launcher (selector 94)")]
    NotSelector94(ObjectId),
    #[error("object {0:?} is not a selector-48 altitude ceiling")]
    NotSelector48(ObjectId),
    #[error("object {0:?} is not a selector-50 hint trigger")]
    NotSelector50(ObjectId),
    #[error("object {0:?} is not a selector-42 wake trigger")]
    NotSelector42(ObjectId),
    #[error("object {0:?} is not a selector-64 course gate")]
    NotSelector64(ObjectId),
    #[error("object {0:?} has no position for the selector-64 gate test")]
    Selector64PositionUnknown(ObjectId),
    #[error("course gate {0:?} cannot read or resolve its linked operand")]
    Selector64Link(ObjectId),
    /// 00440520 with no live actor and an extent above 0x7FFF transforms a
    /// stale stack offset.
    #[error("course gate {0:?} would transform a stale offset")]
    Selector64StaleOffset(ObjectId),
    #[error("object {0:?} has no position for the selector-42 ground test")]
    Selector42PositionUnknown(ObjectId),
    #[error("object {0:?} is not a selector-67 ribbon")]
    NotSelector67(ObjectId),
    #[error("selector-67 ribbon {0:?} requires its own +24 004866A0")]
    Selector67UpdateBoundary(ObjectId),
    #[error("object {0:?} is not a selector-87 map annotation")]
    NotMapAnnotation(ObjectId),
    #[error("selector-86 object {0:?} requires its effects-spawning update policy")]
    Selector86UpdateBoundary(ObjectId),
    #[error("actor {0:?} requires its own update policy and explicit time delta")]
    ActorUpdateBoundary(ObjectId),
    #[error("actor {0:?} requires its derived after-load policy")]
    ActorAfterLoadBoundary(ObjectId),
    #[error("object {0:?} is not the selector-16 camera")]
    NotSelector16(ObjectId),
    #[error("actor {0:?} has no initialized position")]
    ActorPositionUnknown(ObjectId),
    #[error("target referent {0:?} has no initialized common-object position")]
    TargetObjectPositionUnknown(ObjectId),
    #[error("this arena has no configured actor registry")]
    MissingActorRegistry,
    #[error("disposed actor {0:?} requires its virtual +30 policy before destruction")]
    ActorDisposalBoundary(ObjectId),
    #[error("actor {0:?} requires its descriptor change callback before destruction")]
    FamilyBehaviorDestructionBoundary(ObjectId),
    #[error("object {0:?} is not a terrain light")]
    NotTerrainLight(ObjectId),
    #[error("object {0:?} has not initialized its terrain-light payload")]
    UninitializedTerrainLight(ObjectId),
    #[error("object {0:?} is not a route node")]
    NotRouteNode(ObjectId),
    #[error("object {0:?} is not a selector12 actor")]
    NotSelector12(ObjectId),
    #[error("unknown or expired route group {0:?}")]
    UnknownRouteGroup(RouteGroupId),
    #[error("route predecessor recursion cycles at object {0:?}")]
    RouteCycle(ObjectId),
    #[error("route node {0:?} has no initialized position for segment calculation")]
    RoutePositionUnknown(ObjectId),
    #[error("route node {0:?} has no finalized group policy")]
    RouteUnfinalized(ObjectId),
    #[error("H8 slot {slot} does not resolve to its registered model")]
    ModelRegistration { slot: u16 },
    #[error("H8 slot {slot} is at the one-past boundary of its {capacity}-slot table")]
    ModelSlotBoundary { slot: u16, capacity: usize },
}

/// The manager's current list roles; G denotes [0055F160].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ObjectList {
    /// Heap list at G+10, paired with G+14 by 00479500/00479570.
    Flag0Primary,
    Flag0Secondary,
    /// Heap list at G+08, paired with G+0C by 00478F20.
    Flag1Primary,
    Flag1Secondary,
    /// Embedded head at G+30; constructor lifecycle and disabled flag nodes.
    Parking,
    /// Embedded head at G+18; lifecycle nodes awaiting destruction.
    Disposal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum ObjectNodeKind {
    /// Object +04, moved by 00479E90.
    Flag0,
    /// Object +10, moved by 00479F20.
    Flag1,
    /// Object +1C, moved by 00479FA0.
    Lifecycle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectNode {
    pub object: ObjectId,
    pub kind: ObjectNodeKind,
}

/// Initialized fields of the native 0x40-byte base. Access through the arena
/// is read-only so scalar updates cannot bypass membership/dirty policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseObject {
    pub word_28: u16,
    pub placement_word: u16,
    pub flags: u32,
    pub resource_word: u16,
    /// 00479C60 leaves +32..+37 untouched. None is unknown, not [0; 3].
    pub position_words: Option<[u16; 3]>,
    pub dirty_word: u16,
    pub word_3c: u32,
}

/// Only the fields consumed by base context initializer 00479D40.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BaseContext {
    /// A null native position pointer preserves any previous position.
    pub position_words: Option<[u16; 3]>,
    pub placement_index: u32,
}

/// Explicit world state consumed by class-specific loader virtual +34 calls.
#[derive(Clone, Copy, Debug, Default)]
pub struct ObjectAfterLoadInputs<'a> {
    pub world_completion: Option<&'a [u32; 4]>,
}

/// Concrete constructor policies. The H13 factory selects these from the
/// resource itself; a caller cannot substitute one for a different selector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectClassKind {
    Base,
    /// 0047A030's intermediate class; no H13 selector ends at its vtable.
    VisualBase,
    /// Selector89,0046E060,final527254.
    TimedPickup,
    /// Selector 14, 0046D840, final 00527208: the world-placed pickup.
    PlacedPickup,
    /// Selector 58, 004753E0, final 00527838: the rotating-visual pad.
    Selector58,
    /// Selector 45, 0045BF00, final 0052616C: an arena's spawn marker.
    Selector45,
    /// Selector 46, 00475610, final 005278A0: an arena's territory.
    Selector46,
    /// Selectors 70 (0046E770, final 00527390), 71 (0046F7F0, 005273DC)
    /// and 72 (0046F9E0, 00527428): the race, its wrong-way marker and a
    /// gate; selector 92 (00403EC0, 0051F2B0), Powerball's goal.
    Selector70,
    Selector71,
    Selector72,
    Selector92,
    /// Selector 83, 0043C300, final 00522570: the sinking patch of ground.
    Selector83,
    /// Selector 76, 00431ED0, final 00521A40: built only by 0048D820.
    Selector76,
    /// 0047A490's intermediate class, final vtable 00527BB4. This does not
    /// admit a derived actor or its additional constructor/lifetime policies.
    RotatingVisualBase,
    /// Selector 19, final vtable 00526978.
    TerrainLight,
    /// Selector 54, 0047A490 then final vtable 005265FC (0x74 bytes).
    RotatingTerrainLight,
    /// Selector 51, final vtable 0051F658 and a fourth embedded list node.
    RouteNode,
    /// Selector 86, nonvisual emitter, final vtable 005218CC.
    Selector86,
    /// Selector 22, fading visual, final vtable 00521830.
    Selector22,
    Selector65,
    /// Selector 43, timed spawner, final vtable 00525730.
    Selector43,
    /// Selector 81, world 19's spawner over the base object, final vtable
    /// 0051FE44.
    Selector81,
    Selector94,
    /// Selector 48, altitude ceiling, final vtable 005225D0.
    Selector48,
    /// Selector 75, 004352F0, final vtable 00522008: the furniture marker.
    Selector75,
    /// Selector 50, hint trigger, final vtable 00522888.
    Selector50,
    /// Selector 42, wake trigger, final vtable 0051F018.
    Selector42,
    /// Selector 64, course gate, final vtable 00522A28.
    Selector64,
    /// Selector 87, final vtable 00528AB0 and separate embedded +40 node.
    MapAnnotation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualObject {
    /// Model header WORD +0E, copied to object +40 without scaling.
    pub radius_word: u16,
    /// Raw signed H8 argument at object +42; never replaced with a host pointer.
    pub model_word: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainLight {
    /// Signed RGB additions at +44..+46, consumed by 0045B1A0.
    pub channels: [i8; 3],
    /// Raw byte +4A; nonzero enables the optional model marker.
    pub draw_gate: u8,
    /// Unsigned WORD +48, passed separately from the visual model's radius.
    pub radius: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectClass {
    Base,
    Selector86(Selector86State),
    MapAnnotation(MapAnnotationState),
    Visual(VisualObject),
    TimedPickup(TimedPickupState),
    RotatingVisual(RotatingVisual),
    Selector58(Selector58State),
    Selector45(Selector45State),
    Selector46(Selector46State),
    /// Selectors 70, 71 and 72.
    Race(Box<RaceObject>),
    Selector83(Selector83State),
    Selector76(Selector76State),
    /// Shared 0047A7A0 prefix with an explicit final-class extension. Actor
    /// allocation requires registry, terrain and allocation-position inputs.
    Actor(Box<ActorState>),
    TerrainLight {
        visual: VisualObject,
        /// 0047A030 leaves all light-specific fields untouched. 004481C0
        /// initializes them together after visual context and common flags.
        light: Option<TerrainLight>,
    },
    /// Selector 54: selector 19's light over the rotating visual, at
    /// +6C..+72. Only its placement turns the basis.
    RotatingTerrainLight {
        rotating: RotatingVisual,
        /// 0047A490 leaves these untouched; 004649E0 writes them.
        light: Option<TerrainLight>,
    },
    RouteNode {
        visual: VisualObject,
        route: RouteNode,
    },
    Selector22(Selector22State),
    Selector65(Selector65State),
    Selector43(Selector43State),
    Selector81(Selector81State),
    Selector94(Selector94State),
    Selector48(Selector48State),
    Selector75(Selector75State),
    Selector50(Selector50State),
    Selector42(Selector42State),
    Selector64(Selector64State),
    /// 005286D8, the Selector34 ribbon: the base class with its own
    /// destructor, +24 and +2C.
    Selector67(Selector67State),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectState {
    pub base: BaseObject,
    pub class: ObjectClass,
}

#[derive(Clone, Debug)]
struct OwnedObject {
    state: ObjectState,
    /// Port containment after a typed construction stop. Native fields and
    /// memberships remain available for inspection and later destruction.
    construction_stopped: bool,
    /// Portable custody for a failed disposal prefix, checked before every
    /// later cleanup callback or deleting destructor. Not a native field.
    disposal_stop: Option<ObjectDisposalStop>,
    /// Physical list indices; owner-pointer swaps must not alter these.
    memberships: [usize; 3],
}

#[derive(Clone, Debug)]
struct ObjectSlot {
    generation: u64,
    value: Option<OwnedObject>,
}

/// Concrete object allocations and their ordered nodes. It intentionally has no
/// successful stand-in callbacks for classes or manager passes not yet ported.
/// A copy keeps the arena's identity and shares its runtime allocations,
/// so it may never live on beside its original: the world draw's view shown
/// draws on one while those allocations refuse writes, then drops it
/// ([`refusing_writes`]).
#[derive(Clone, Debug)]
pub struct ObjectArena {
    identity: u64,
    slots: Vec<ObjectSlot>,
    free: Vec<usize>,
    live_count: usize,
    lists: [Vec<ObjectNode>; 6],
    list_owners: [usize; 6],
    routes: route::Routes,
    actors: Vec<ObjectId>,
    /// Forward order of the process annotation list at 0055F170, which
    /// starts empty (0048C6C0).
    map_annotations: Vec<ObjectId>,
    /// The spawn list 0054E2F8 of selector 45's markers, in link order.
    spawn_markers: Vec<ObjectId>,
    /// The territory list `0055EF10`.
    territories: Vec<ObjectId>,
    /// `0054ED70`, the race; `0054ED58`, its gates, newest first.
    race: Option<ObjectId>,
    race_gates: Vec<ObjectId>,
    /// `0054ED74`, the race's microseconds: a process global, carried like
    /// the Force Fields' bits.
    race_clock: u32,
    /// `005468D0`, Powerball's ball; `0052E528`/`0052E529`, each side's
    /// goals (bit 7 a won side); `0052E52C`/`0052E52D`, the counts last said.
    powerball: Option<ObjectId>,
    powerball_goals: [u8; 2],
    powerball_seen: [u8; 2],
    /// Messages world code sends (`00492500`) for the next network tick.
    network_sends: Vec<crate::network::OrderedSend>,
    /// The game protocol's state while the world updates, which a player's
    /// vehicle hit or killed tells (`0045DA50`, `0045D8F0`).
    network_game: Option<Box<crate::network::notices::NetworkGameHold>>,
    actor_handles: Option<ActorHandles>,
    actor_allocation: actor::ActorAllocationService,
    actor_payloads: construction::ActorPayloadCache,
    constructor_allocations: allocation::ConstructorAllocations,
    actor_services: ActorServiceHeap,
    selector60_rows: actor::Selector60Rows,
    players: player::WorldPlayers,
    world_load: world_load::WorldLoadState,
    /// Gameplay G+5B4: each kind-7 pickup the shared destructor 0046E0A0
    /// destroys adds one. 00487540 zeroes it at the phase-8 transition before
    /// 0048E890 retires the world, so results draw 0042B510 counts the kind-7
    /// pickups left uncollected.
    pickup_kind7_5b4: u32,
    /// 0055F0E4 and 0053B8B0, the Force Fields' bits (0047DE30). They are
    /// process globals; the arena a retained load keeps carries them from
    /// world to world, and a new arena starts them afresh. Only a bit's
    /// equality and masks are ever read, never its value.
    force_field_bits: actor::ForceFieldBits,
    /// 00547620 and 00547624, the capture nodes by team: process globals,
    /// carried like the Force Fields' bits; the nodes' own finish and
    /// teardown keep them.
    capture_nodes: actor::CaptureNodeCounts,
    /// 00546910 and 00546914, the ending's phase and the brain's restart:
    /// process globals, carried like the Force Fields' bits.
    alien_brain: actor::AlienBrainWords,
    /// The cleanup walk's head whose +30 callback ran before it paused for
    /// that object's destructor exit (`WorldCleanupError::DestructionExitPending`).
    cleanup_exit_pending: Option<ObjectId>,
}

impl Default for ObjectArena {
    fn default() -> Self {
        Self::new()
    }
}

impl ObjectArena {
    /// The manager's constructor 00478A90: four allocated list headers and
    /// the embedded lists, all empty.
    pub fn new() -> Self {
        let identity = NEXT_ARENA
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .expect("object arena identity exhausted");
        Self {
            identity,
            slots: Vec::new(),
            free: Vec::new(),
            live_count: 0,
            lists: std::array::from_fn(|_| Vec::new()),
            list_owners: std::array::from_fn(|index| index),
            routes: route::Routes::default(),
            actors: Vec::new(),
            map_annotations: Vec::new(),
            spawn_markers: Vec::new(),
            territories: Vec::new(),
            race: None,
            race_gates: Vec::new(),
            race_clock: 0,
            powerball: None,
            powerball_goals: [0; 2],
            powerball_seen: [0; 2],
            network_sends: Vec::new(),
            network_game: None,
            actor_handles: None,
            actor_allocation: actor::ActorAllocationService::default(),
            actor_payloads: construction::ActorPayloadCache::default(),
            constructor_allocations: allocation::ConstructorAllocations::default(),
            actor_services: ActorServiceHeap::new(
                identity,
                ActorServiceAllocationPolicy::PortableFallibleActorHeapApproximation,
            ),
            selector60_rows: actor::Selector60Rows::default(),
            players: player::WorldPlayers::default(),
            world_load: world_load::WorldLoadState::default(),
            pickup_kind7_5b4: 0,
            force_field_bits: actor::ForceFieldBits::default(),
            capture_nodes: actor::CaptureNodeCounts::default(),
            alien_brain: actor::AlienBrainWords::default(),
            cleanup_exit_pending: None,
        }
    }

    /// Gameplay G+5B4, the destroyed kind-7 pickup count.
    pub fn pickup_kind7_count(&self) -> u32 {
        self.pickup_kind7_5b4
    }

    /// 00487540's G+5B4 = 0, before its 0048E890 world retirement.
    pub fn reset_pickup_kind7_count(&mut self) {
        self.pickup_kind7_5b4 = 0;
    }

    pub fn len(&self) -> usize {
        self.live_count
    }

    pub fn is_empty(&self) -> bool {
        self.live_count == 0
    }

    pub fn object(&self, object: ObjectId) -> Result<&ObjectState, ObjectError> {
        Ok(&self.owned(object)?.state)
    }

    pub fn construction_is_stopped(&self, object: ObjectId) -> Result<bool, ObjectError> {
        Ok(self.owned(object)?.construction_stopped)
    }

    pub(crate) fn stop_object_construction(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        self.owned_mut(object)?.construction_stopped = true;
        Ok(())
    }

    pub fn list(&self, role: ObjectList) -> &[ObjectNode] {
        &self.lists[self.list_owners[role as usize]]
    }

    /// 00479C60 publishes all three memberships before class initialization.
    pub fn allocate(&mut self, class: ObjectClassKind) -> ObjectId {
        let memberships = [
            self.list_owners[ObjectList::Flag0Primary as usize],
            self.list_owners[ObjectList::Flag1Primary as usize],
            self.list_owners[ObjectList::Parking as usize],
        ];
        let value = Some(OwnedObject {
            construction_stopped: false,
            disposal_stop: None,
            state: ObjectState {
                base: BaseObject {
                    word_28: 0,
                    placement_word: u16::MAX,
                    flags: 0x0800_0003,
                    resource_word: 0,
                    position_words: None,
                    dirty_word: 0,
                    word_3c: 0,
                },
                class: match class {
                    ObjectClassKind::Base => ObjectClass::Base,
                    ObjectClassKind::Selector86 => ObjectClass::Selector86(Selector86State::new()),
                    ObjectClassKind::Selector75 => ObjectClass::Selector75(Selector75State::new()),
                    ObjectClassKind::MapAnnotation => {
                        ObjectClass::MapAnnotation(MapAnnotationState::new())
                    }
                    ObjectClassKind::VisualBase => ObjectClass::Visual(VisualObject {
                        radius_word: 0,
                        model_word: u16::MAX,
                    }),
                    ObjectClassKind::TimedPickup => {
                        ObjectClass::TimedPickup(TimedPickupState::new(PickupClass::Timed))
                    }
                    ObjectClassKind::PlacedPickup => {
                        ObjectClass::TimedPickup(TimedPickupState::new(PickupClass::Placed))
                    }
                    ObjectClassKind::RotatingVisualBase => {
                        ObjectClass::RotatingVisual(RotatingVisual::new())
                    }
                    ObjectClassKind::Selector58 => ObjectClass::Selector58(Selector58State::new()),
                    ObjectClassKind::Selector45 => ObjectClass::Selector45(Selector45State::new()),
                    // 00475610 numbers it one past the count 0055EF28.
                    ObjectClassKind::Selector46 => ObjectClass::Selector46(Selector46State::new(
                        self.territories.len() as u32 + 1,
                    )),
                    ObjectClassKind::Selector70 => ObjectClass::Race(Box::new(RaceObject::new(70))),
                    ObjectClassKind::Selector71 => ObjectClass::Race(Box::new(RaceObject::new(71))),
                    ObjectClassKind::Selector72 => ObjectClass::Race(Box::new(RaceObject::new(72))),
                    ObjectClassKind::Selector92 => ObjectClass::Race(Box::new(RaceObject::new(92))),
                    ObjectClassKind::Selector83 => ObjectClass::Selector83(Selector83State::new()),
                    ObjectClassKind::Selector76 => ObjectClass::Selector76(Selector76State::new()),
                    ObjectClassKind::TerrainLight => ObjectClass::TerrainLight {
                        visual: VisualObject {
                            radius_word: 0,
                            model_word: u16::MAX,
                        },
                        light: None,
                    },
                    ObjectClassKind::RotatingTerrainLight => ObjectClass::RotatingTerrainLight {
                        rotating: RotatingVisual::new(),
                        light: None,
                    },
                    ObjectClassKind::RouteNode => ObjectClass::RouteNode {
                        visual: VisualObject {
                            radius_word: 0,
                            model_word: u16::MAX,
                        },
                        route: RouteNode::new(),
                    },
                    ObjectClassKind::Selector22 => ObjectClass::Selector22(Selector22State::new()),
                    ObjectClassKind::Selector65 => ObjectClass::Selector65(Selector65State::new()),
                    ObjectClassKind::Selector43 => ObjectClass::Selector43(Selector43State::new()),
                    ObjectClassKind::Selector81 => ObjectClass::Selector81(Selector81State::new()),
                    ObjectClassKind::Selector94 => {
                        ObjectClass::Selector94(Selector94State::default())
                    }
                    ObjectClassKind::Selector48 => ObjectClass::Selector48(Selector48State::new()),
                    ObjectClassKind::Selector50 => ObjectClass::Selector50(Selector50State::new()),
                    ObjectClassKind::Selector42 => ObjectClass::Selector42(Selector42State::new()),
                    ObjectClassKind::Selector64 => ObjectClass::Selector64(Selector64State::new()),
                },
            },
            memberships,
        });
        let slot = if let Some(slot) = self.free.pop() {
            self.slots[slot].value = value;
            slot
        } else {
            self.slots.push(ObjectSlot {
                generation: 1,
                value,
            });
            self.slots.len() - 1
        };
        let object = ObjectId {
            arena: self.identity,
            slot,
            generation: self.slots[slot].generation,
        };
        for kind in [
            ObjectNodeKind::Flag1,
            ObjectNodeKind::Flag0,
            ObjectNodeKind::Lifecycle,
        ] {
            self.lists[memberships[kind as usize]].push(ObjectNode { object, kind });
        }
        self.live_count += 1;
        if class == ObjectClassKind::RouteNode {
            // 00404B20 appends the fourth node after the visual constructor.
            self.routes.pending.push(object);
        }
        if class == ObjectClassKind::Selector45 {
            // 0045BF00 appends +6C to the spawn list.
            self.spawn_markers.push(object);
        }
        if class == ObjectClassKind::Selector46 {
            // 00475610 appends +7C to the territory list.
            self.territories.push(object);
        }
        if class == ObjectClassKind::Selector72 {
            // 0046F9E0 inserts +7C at the gate list's head.
            self.race_gates.insert(0, object);
        }
        if class == ObjectClassKind::MapAnnotation {
            // 0048C710 inserts +40 before the current head.next. Unlike
            // common object nodes, annotation visitation is forward.
            self.map_annotations.insert(0, object);
        }
        object
    }

    pub fn initialize_context(
        &mut self,
        object: ObjectId,
        context: BaseContext,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state.base;
        initialize_base_context(state, context);
        Ok(())
    }

    /// 00479E90 always removes/appends the +04 node, even for unchanged values.
    pub fn set_flag0(&mut self, object: ObjectId, enabled: bool) -> Result<(), ObjectError> {
        self.set_membership_flag(
            object,
            enabled,
            ObjectNodeKind::Flag0,
            ObjectList::Flag0Primary,
            1,
        )
    }

    /// 00479F20 always removes/appends the +10 node, even for unchanged values.
    pub fn set_flag1(&mut self, object: ObjectId, enabled: bool) -> Result<(), ObjectError> {
        self.set_membership_flag(
            object,
            enabled,
            ObjectNodeKind::Flag1,
            ObjectList::Flag1Primary,
            2,
        )
    }

    /// 00479D70 changes bit0 membership before bit1. The subsequent full DWORD
    /// merge can add high flags, and does not mark dirty for other changed bits.
    pub fn apply_payload_flags(&mut self, object: ObjectId, flags: u32) -> Result<(), ObjectError> {
        self.set_flag0(object, flags & 1 != 0)?;
        self.set_flag1(object, flags & 2 != 0)?;
        let state = &mut self.owned_mut(object)?.state.base;
        state.flags = (state.flags & 0xff00_0000) | flags;
        Ok(())
    }

    /// 00479FA0 queues disposal; the object remains live. Its setter order is
    /// the reverse of payload initialization and repeats still reorder nodes.
    pub fn dispose(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        if crate::event_log::enabled() {
            crate::event_log::dispose(self.object(object)?.base.resource_word);
        }
        self.set_flag1(object, false)?;
        self.set_flag0(object, false)?;
        let state = &mut self.owned_mut(object)?.state.base;
        let flags = state.flags & 0xffff_fe9b;
        mark_flag_change(state, flags);
        self.move_node(object, ObjectNodeKind::Lifecycle, ObjectList::Disposal);
        Ok(())
    }

    /// Selector 51 first unlinks its route node and any newly empty group;
    /// actors release their handle and actor node. Common 00479D00 then
    /// removes +04, +10, +1C. Rust releases allocation
    /// custody; this does not run a manager's world-cleanup callback pass.
    pub fn destroy(
        &mut self,
        request: ObjectDestructionRequest<'_>,
    ) -> Result<ObjectState, ObjectError> {
        self.destroy_with_process(request, None)
    }

    /// [`Self::destroy`] with the live process, which the charge's
    /// destructor (004562F0) needs to message its thrower.
    pub fn destroy_with_process(
        &mut self,
        mut request: ObjectDestructionRequest<'_>,
        process: Option<&crate::process_particles::factories::ParticleFactoryProcessState>,
    ) -> Result<ObjectState, ObjectError> {
        let object = request.object;
        self.ensure_disposal_active(object)?;
        if let ObjectClass::TimedPickup(pickup) = &self.object(object)?.class {
            match pickup.kind_44 {
                None => return Err(ObjectError::MissingPickupKind(object)),
                // 0046E0A0 counts the kind-7 pickup before 00479D00.
                Some(7) => self.pickup_kind7_5b4 = self.pickup_kind7_5b4.wrapping_add(1),
                _ => {}
            }
        }
        let memberships = self.owned(object)?.memberships;
        if self.selector16(object).is_ok() {
            self.destroy_selector16_targets(object)?;
        }
        if self.selector60_actor(object).is_ok() {
            self.destroy_selector60_rows(object)?;
        }
        if self.selector2_actor(object).is_ok() {
            self.destroy_selector2_owners(object, &mut request.sound)?;
        }
        if self.selector17(object).is_ok() {
            // 0046E710: the hold's 00411E30 disposes the slot's live actor
            // (0047AD90, 00479FA0) before 0047AA60.
            let handle = self.selector17(object)?.hold_144.handles[0];
            if let Some(prisoner) = self
                .resolve_actor_handle(handle)
                .map_err(|_| ObjectError::MissingActorRegistry)?
            {
                self.dispose(prisoner)?;
            }
        }
        if self.selector18(object).is_ok() {
            // 004768D0 before 0047AA60: the field goes off, clearing its bit
            // from every actor, and gives the bit back (0047DE80). Its
            // caller has already unregistered its listener binding.
            self.retire_selector18(object)?;
        }
        if self.behavior_resource(object).is_ok() {
            // 004562F0/00456640/004582D0/00456A70 before 0047AA60.
            self.behavior_resource_teardown(object, process)?;
        }
        if self.selector84(object).is_ok() {
            // 0043C0C0 before 0047AA60: the node leaves its team's count.
            self.release_selector84(object)?;
        }
        if self.is_ball(object) {
            // 004028A0 before the boulder's 00450440.
            self.end_ball(object)?;
        }
        if self.selector61(object).is_ok() {
            // 00412990 deletes its target +148 (the target's +00), then tears
            // its weapon behavior +138 down (0048CD60), before 0047AA60.
            if let Some(reference) = self.selector61_mut(object)?.target_148.take() {
                self.release_actor_service(reference)?;
            }
            self.teardown_actor_behavior(object, &mut request.sound)?;
        }
        if self.selector62(object).is_ok() {
            // 00463FF0 deletes its target +15C before 0047AA60; the ribbons
            // stay to the world's walks.
            if let Some(reference) = self.selector62_mut(object)?.target_15c.take() {
                self.release_actor_service(reference)?;
            }
        }
        if matches!(self.object(object)?.class, ObjectClass::RouteNode { .. }) {
            self.unlink_route(object)?;
        }
        if matches!(self.object(object)?.class, ObjectClass::Selector45(_)) {
            // 0045BF80 unlinks +6C before entering common 00479D00.
            self.spawn_markers.retain(|id| *id != object);
        }
        if matches!(self.object(object)?.class, ObjectClass::Selector46(_)) {
            // 004756C0 counts it out and unlinks +7C.
            self.territories.retain(|id| *id != object);
        }
        if let ObjectClass::Race(race) = &self.object(object)?.class {
            match race.part {
                // 0046E810: the race clears 0054ED70 when it is the race.
                RacePart::Race(_) if self.race == Some(object) => self.race = None,
                // 0046FA40 unlinks +7C.
                RacePart::Gate(_) => self.race_gates.retain(|id| *id != object),
                _ => {}
            }
        }
        if matches!(self.object(object)?.class, ObjectClass::MapAnnotation(_)) {
            // 0048C780 unlinks +40 before entering common 00479D00.
            let index = self
                .map_annotations
                .iter()
                .position(|id| *id == object)
                .expect("annotation node");
            self.map_annotations.remove(index);
        }
        if self.family4236d0_actor(object).is_ok() {
            // Selector10's helper prefix precedes shared 004237D0. That
            // destructor installs its own vtable before descriptor dispatch,
            // including when reached through the Selector36 jump wrapper.
            self.destroy_family4236d0_behavior_prefix(object)?;
        } else if self.selector12_actor(object).is_ok() {
            // 004671E0 exits the selected descriptor before releasing any
            // class target, probe or common resource. This owner needs no
            // update/entry services: the native argument is false.
            self.exit_selector12_behavior_for_destruction(object)?;
        } else if self.selector34_actor(object).is_ok() {
            // 00433B90 exits the selected 005329F8 descriptor (argument
            // false) before delegating to the shared 00428750 destructor.
            self.exit_selector34_behavior_for_destruction(object)?;
        } else if self.selector35_actor(object).is_ok() {
            // 0043D0F0 exits the selected 00533D38 descriptor (argument
            // false) before delegating to the shared 00428750 destructor.
            self.exit_selector35_behavior_for_destruction(object)?;
        }
        if let ObjectClass::Actor(actor) = &mut self.slots[object.slot]
            .value
            .as_mut()
            .expect("validated object")
            .state
            .class
        {
            if let ActorExtension::Selector90(state) = &mut actor.extension {
                if let Some(reference) = state.route_150 {
                    self.actor_services.release(reference)?;
                }
                if let Some(reference) = state.target_154 {
                    self.actor_services.release(reference)?;
                }
            }
            // 0040D870 releases a live +154 through 00487FE0 before 0047AA60.
            // Every class of the family reaches it: the factory's 0042E320,
            // the gate's, bridge's and course target's 0048B220, the Siren's
            // 0047FEE0 and the stabiliser's 004763E0.
            let family = match &mut actor.extension {
                ActorExtension::Selector15(state) => Some(&mut state.intermediate),
                ActorExtension::Selector30(state) => Some(&mut state.intermediate),
                ActorExtension::Selector37(state) => Some(&mut state.intermediate),
                ActorExtension::Selector39(state) => Some(&mut state.intermediate),
                ActorExtension::Selector47(state) => Some(&mut state.intermediate),
                ActorExtension::Selector49(state) => Some(&mut state.intermediate),
                _ => None,
            };
            if let Some(family) = family {
                if let Some(reference) = family.sound_154 {
                    request.sound.delete(reference)?;
                    family.sound_154 = None;
                }
            }
            if let ActorExtension::BehaviorResource(state) = &mut actor.extension {
                // 00456A70 (selectors 25 and 28) and 00461BD0 (68) release a
                // live +154, which the steered flight starts, through
                // 00487FE0 before 0047AA60.
                if let crate::object_runtime::actor::BehaviorResourceClass::Family455560(family) =
                    &mut state.class
                {
                    if let crate::object_runtime::actor::Family455560Tail::Zeroed14c {
                        sound_154,
                        ..
                    }
                    | crate::object_runtime::actor::Family455560Tail::Selector25 {
                        sound_154,
                        ..
                    } = &mut family.tail
                    {
                        if let Some(reference) = sound_154.take() {
                            request.sound.delete(reference)?;
                        }
                    }
                }
            }
            if let ActorExtension::Selector93(state) = &mut actor.extension {
                //00440070 deletes148 before47AA60 without a NULL store.
                if let Some(reference) = state.target_148 {
                    self.actor_services.release(reference)?;
                }
            }
            if let ActorExtension::Derived(derived) = &mut actor.extension {
                if let DerivedActorClass::Family428650(family) = &mut derived.class {
                    if let Family428650Class::Family429e20(probe_family) = &mut family.class {
                        if let Family429e20Class::Family4236d0(_) = &mut probe_family.class {
                            // The helper/004237D0 descriptor prefix completed
                            // before this loan of the lower probe owners.
                            debug_assert!(family.word_184 < 0);
                        } else if let Family429e20Class::Selector12(state) = &mut probe_family.class
                        {
                            // The 004671E0 descriptor exit above completed
                            // before this mutable loan of the lower owners.
                            debug_assert!(family.word_184 < 0);
                            // 004671E0 deletes +284/+27C/+274/+26C before
                            // reaching the lower probe/helper/target owners.
                            for index in (0..state.targets_26c_284.len()).rev() {
                                if let Some(reference) = state.targets_26c_284[index] {
                                    self.actor_services.release(reference)?;
                                }
                            }
                        }
                        // 00429E70 frees inner probe storage, then the probe header.
                        probe_family.probe_234 = None;
                    } else if matches!(
                        family.class,
                        Family428650Class::Selector34(_) | Family428650Class::Selector35(_)
                    ) {
                        // The 00433B90/0043D0F0 descriptor exit above completed
                        // before 00428750 releases the family resources.
                        debug_assert!(family.word_184 < 0);
                    }
                    // 00428750 releases the separate +218 allocation before
                    // entering 00425F70. Retired snapshots do not own freed storage.
                    family.helper_218 = None;
                    // Native 00428750 destroys the six target resources in
                    // reverse field order; referent objects remain live.
                    for index in (0..family.targets_1d4_1fc.len()).rev() {
                        if let Some(reference) = family.targets_1d4_1fc[index] {
                            self.actor_services.release(reference)?;
                        }
                    }
                }
                // 00425F70 destroys +148 then +154 and writes each null.
                if let Some(reference) = derived.target_148 {
                    self.actor_services.release(reference)?;
                }
                derived.target_148 = None;
                if let Some(reference) = derived.target_154 {
                    self.actor_services.release(reference)?;
                }
                derived.target_154 = None;
                // 00425F70 destroys +140 and writes null before calling the
                // shared destructor. The returned retired state must not
                // retain those owned arrays beyond native destruction.
                if let Some(behavior) = derived.behavior_140.as_mut() {
                    behavior.teardown(&mut self.actor_services, &mut request.sound)?;
                }
                derived.behavior_140 = None;
                //425F70:168 teardown follows140 and precedes47AA60.
                if let Some(reference) = derived.sound_168 {
                    request.sound.delete(reference)?;
                    derived.sound_168 = None; //normal-onlyNULL, no earlier pre-unlink.
                }
            }
            let handle = actor.handle;
            // 0047AA60 unregisters +84 before unlinking +A8. Common nodes
            // are removed only after both actor operations have completed.
            self.actor_handles
                .as_mut()
                .expect("actor registry owner")
                .release(handle);
            let index = self
                .actors
                .iter()
                .position(|id| *id == object)
                .expect("actor node");
            self.actors.remove(index);
        }
        for kind in [
            ObjectNodeKind::Flag0,
            ObjectNodeKind::Flag1,
            ObjectNodeKind::Lifecycle,
        ] {
            self.unlink(ObjectNode { object, kind }, memberships[kind as usize]);
        }
        let slot = &mut self.slots[object.slot];
        let value = slot.value.take().expect("validated live object");
        // An exhausted slot is never reused: an old identity cannot revive.
        if let Some(next) = slot.generation.checked_add(1) {
            slot.generation = next;
            self.free.push(object.slot);
        }
        self.live_count -= 1;
        Ok(value.state)
    }

    /// Only the final owner-pointer exchange of 00479500/00479570. Their
    /// transfer/callback loops must be implemented separately before use.
    pub fn swap_flag0_lists(&mut self) {
        self.list_owners.swap(
            ObjectList::Flag0Primary as usize,
            ObjectList::Flag0Secondary as usize,
        );
    }

    /// Only the final owner-pointer exchange of 00478F20. Its transfer order
    /// differs from lighting/draw; this method does not run an update pass.
    pub fn swap_flag1_lists(&mut self) {
        self.list_owners.swap(
            ObjectList::Flag1Primary as usize,
            ObjectList::Flag1Secondary as usize,
        );
    }

    fn owned(&self, object: ObjectId) -> Result<&OwnedObject, ObjectError> {
        self.slots
            .get(object.slot)
            .filter(|slot| object.arena == self.identity && object.generation == slot.generation)
            .and_then(|slot| slot.value.as_ref())
            .ok_or(ObjectError::UnknownObject(object))
    }

    /// Moves `object` to `position`, for tests that stage a scene.
    #[cfg(test)]
    pub(crate) fn place_for_tests(&mut self, object: ObjectId, position: [u16; 3]) {
        self.owned_mut(object).unwrap().state.base.position_words = Some(position);
    }

    /// Sets an actor's lifecycle state +C4 and resource word +30, for tests
    /// that stage a message's receiver and sender.
    #[cfg(test)]
    pub(crate) fn stage_actor_for_tests(&mut self, actor: ObjectId, state: u32, resource: u16) {
        self.actor_mut(actor).unwrap().word_c4 = state;
        self.owned_mut(actor).unwrap().state.base.resource_word = resource;
    }

    fn owned_mut(&mut self, object: ObjectId) -> Result<&mut OwnedObject, ObjectError> {
        self.slots
            .get_mut(object.slot)
            .filter(|slot| object.arena == self.identity && object.generation == slot.generation)
            .and_then(|slot| slot.value.as_mut())
            .ok_or(ObjectError::UnknownObject(object))
    }

    fn set_membership_flag(
        &mut self,
        object: ObjectId,
        enabled: bool,
        kind: ObjectNodeKind,
        active_list: ObjectList,
        bit: u32,
    ) -> Result<(), ObjectError> {
        let state = &mut self.owned_mut(object)?.state.base;
        let flags = (state.flags & !bit) | if enabled { bit } else { 0 };
        mark_flag_change(state, flags);
        self.move_node(
            object,
            kind,
            if enabled {
                active_list
            } else {
                ObjectList::Parking
            },
        );
        Ok(())
    }

    fn move_node(&mut self, object: ObjectId, kind: ObjectNodeKind, role: ObjectList) {
        self.relink_node(object, kind, role, false);
    }

    /// 00478F20's transfer: insert after the list's head sentinel instead
    /// of before its tail.
    fn move_node_front(&mut self, object: ObjectId, kind: ObjectNodeKind, role: ObjectList) {
        self.relink_node(object, kind, role, true);
    }

    fn relink_node(
        &mut self,
        object: ObjectId,
        kind: ObjectNodeKind,
        role: ObjectList,
        front: bool,
    ) {
        let node = ObjectNode { object, kind };
        let old = self.slots[object.slot].value.as_ref().unwrap().memberships[kind as usize];
        self.unlink(node, old);
        let new = self.list_owners[role as usize];
        if front {
            self.lists[new].insert(0, node);
        } else {
            self.lists[new].push(node);
        }
        self.slots[object.slot].value.as_mut().unwrap().memberships[kind as usize] = new;
    }

    fn unlink(&mut self, node: ObjectNode, physical_list: usize) {
        let list = &mut self.lists[physical_list];
        let index = list
            .iter()
            .position(|entry| *entry == node)
            .expect("live embedded node");
        list.remove(index);
    }

    fn clear_dirty(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        self.owned_mut(object)?.state.base.dirty_word = 0;
        Ok(())
    }

    /// 00479D40 copying unwritten stack bytes: the position becomes unknown.
    fn forget_position_words(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        self.owned_mut(object)?.state.base.position_words = None;
        Ok(())
    }

    fn mark_initialized(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        self.owned_mut(object)?.state.base.flags |= INITIALIZED_FLAG;
        Ok(())
    }

    /// 004481C0 (selector 54's 004649E0) copies the light payload after its
    /// common flag policy.
    pub fn initialize_terrain_light_payload(
        &mut self,
        object: ObjectId,
        flags: u32,
        value: TerrainLight,
    ) -> Result<(), ObjectError> {
        if !matches!(
            self.owned(object)?.state.class,
            ObjectClass::TerrainLight { .. } | ObjectClass::RotatingTerrainLight { .. }
        ) {
            return Err(ObjectError::NotTerrainLight(object));
        }
        self.apply_payload_flags(object, flags)?;
        let (ObjectClass::TerrainLight { light, .. }
        | ObjectClass::RotatingTerrainLight { light, .. }) =
            &mut self.owned_mut(object)?.state.class
        else {
            unreachable!("validated terrain-light class");
        };
        *light = Some(value);
        Ok(())
    }

    /// 00401320 calls the final virtual +34 before setting the high flag.
    /// Selector 60 owns row allocation; unsupported callbacks fail first.
    pub fn initialize_after_load(
        &mut self,
        object: ObjectId,
        inputs: ObjectAfterLoadInputs<'_>,
    ) -> Result<(), ObjectError> {
        if self.selector60_actor(object).is_ok() {
            self.initialize_selector60_after_load(object)?;
        }
        if self.selector64(object).is_ok() {
            self.initialize_selector64_after_load(object)?;
        }
        if self.family428650_actor(object).is_ok() {
            self.initialize_family428650_after_load(object)?;
        }
        if self.selector30_actor(object).is_ok() {
            let completion = inputs
                .world_completion
                .ok_or(ObjectError::MissingWorldCompletion(object))?;
            self.initialize_selector30_after_load(object, completion)?;
        }
        // 0051FF54's and 00521940's +34 is the family 0040DFC0.
        if self.selector37(object).is_ok() || self.selector15(object).is_ok() {
            let completion = inputs
                .world_completion
                .ok_or(ObjectError::MissingWorldCompletion(object))?;
            self.family40d800_after_load(object, completion)?;
        }
        // 00527908's and 00527CD4's +34 is 0047FE90: a completed stabiliser
        // or Siren disposes itself.
        if self.selector49(object).is_ok() || self.selector47(object).is_ok() {
            let completion = inputs
                .world_completion
                .ok_or(ObjectError::MissingWorldCompletion(object))?;
            self.selector49_after_load(object, completion)?;
        }
        if let ObjectClass::Actor(actor) = &self.object(object)?.class {
            // The placed mine's 00525914 and the Bomb's 00525AB4 keep the
            // shared +34, 00401310.
            let mine = matches!(
                &actor.extension,
                actor::ActorExtension::BehaviorResource(state) if matches!(state.selector, 29 | 38)
            );
            if !mine
                && !matches!(
                    actor.extension,
                    actor::ActorExtension::Shared
                    | actor::ActorExtension::Derived(_)
                    | actor::ActorExtension::Selector15(_)
                    | actor::ActorExtension::Selector16(_)
                    | actor::ActorExtension::Selector30(_)
                    | actor::ActorExtension::Selector37(_)
                    | actor::ActorExtension::Selector47(_)
                    | actor::ActorExtension::Selector49(_)
                    | actor::ActorExtension::Selector53(_)
                    // 00526714 keeps the shared +34, 00401310.
                    | actor::ActorExtension::Selector52(_)
                    | actor::ActorExtension::Selector73(_)
                    | actor::ActorExtension::Selector74(_)
                    | actor::ActorExtension::Selector61(_)
                    | actor::ActorExtension::Selector62(_)
                    | actor::ActorExtension::Selector84(_)
                    // So do the ending director's 0051FA9C, 00526464,
                    // 00525648, 0051F900 and 005279E0.
                    | actor::ActorExtension::Selector78(_)
                    | actor::ActorExtension::Selector57
                    // The banner's 005260A0 and the base's 005267E0 too.
                    | actor::ActorExtension::Selector40(_)
                    | actor::ActorExtension::Selector41
                    | actor::ActorExtension::Selector44(_)
                    | actor::ActorExtension::Selector59(_)
                    | actor::ActorExtension::Selector18(_)
                    | actor::ActorExtension::Selector69(_)
                    | actor::ActorExtension::Selector66(_)
                    | actor::ActorExtension::Selector90(_)
                    | actor::ActorExtension::Selector93(_)
                )
            {
                return Err(ObjectError::ActorAfterLoadBoundary(object));
            }
        }
        self.set_after_load_flag(object)
    }

    fn set_after_load_flag(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        let base = &mut self.owned_mut(object)?.state.base;
        mark_flag_change(base, base.flags | AFTER_LOAD_FLAG);
        Ok(())
    }

    /// 00405140, shared by +24 and +3C of the represented non-actor classes.
    /// Its argument is unused; even an already-disabled membership is
    /// reappended. Actors require separate time/motion policies.
    pub fn update(&mut self, object: ObjectId) -> Result<(), ObjectError> {
        if matches!(self.object(object)?.class, ObjectClass::TimedPickup(_)) {
            return Err(ObjectError::TimedPickupUpdateBoundary(object));
        }
        if matches!(self.object(object)?.class, ObjectClass::Actor(_)) {
            return Err(ObjectError::ActorUpdateBoundary(object));
        }
        if matches!(self.object(object)?.class, ObjectClass::Selector86(_)) {
            return Err(ObjectError::Selector86UpdateBoundary(object));
        }
        if matches!(self.object(object)?.class, ObjectClass::Selector67(_)) {
            return Err(ObjectError::Selector67UpdateBoundary(object));
        }
        self.set_flag1(object, false)
    }
}

pub(in crate::object_runtime) fn mark_flag_change(state: &mut BaseObject, flags: u32) {
    if (state.flags ^ flags) & 0x1ff != 0 {
        state.dirty_word |= 2;
    }
    state.flags = flags;
}

fn initialize_base_context(state: &mut BaseObject, context: BaseContext) {
    state.placement_word = context.placement_index as u16;
    if let Some(position) = context.position_words {
        state.position_words = Some(position);
    }
}

#[cfg(test)]
#[path = "process_tasks/relation/native_receipts.rs"]
mod task_relation_native_receipts;

#[cfg(test)]
mod tests;
