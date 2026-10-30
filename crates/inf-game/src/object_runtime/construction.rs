//! Concrete selector-1/2/6/7/9/10/11/12/14/15/16/18/19/21/22/30/33/34/35/36/37/39/47/51/52/53/54/57/58/59/60/61/62/66/77/79/80/81/86/87/88/89/90/93 construction in world mode 0.
//!
//! These profiles preserve their admission guards, actor postinitialization
//! and creation-listener notification. Construction does not
//! run later lighting, drawing, update or loader virtual-+20 passes.

use std::sync::Arc;

use inf_formats::{
    decode_instance_command, parse_constructor_asset_bank, AssetRelocation, ByteSpan,
    ConstructorAssetError, ConstructorPayloadView, InstanceAssetError, InstanceCommandPayload,
    InstanceCommandRequest, InstanceCommandTarget, PlacementInstanceAssets, GRANT_MASK_WORDS,
};
use thiserror::Error;

use super::{
    ActorCommandContext, ActorCommandError, BaseContext, DerivedActorConstructionRequest,
    DerivedActorError, ObjectArena, ObjectClass, ObjectClassKind, ObjectError, ObjectId,
    RuntimeAllocation, RuntimeAllocationError, Selector12GrantContext, Selector60Error,
    TerrainLight,
};
use crate::ResidentRuntime;

#[path = "actor_factory.rs"]
pub(in crate::object_runtime) mod actor_factory;
#[path = "command_reads.rs"]
mod command_reads;
use super::{
    ActorConstructionError, CreationListenerError, InstanceCommandAllocation,
    InstanceCommandAllocationError,
};
pub(super) use actor_factory::ActorPayloadCache;
pub use actor_factory::{ActorFactoryInputs, DerivedActorFactoryInputs, VehicleFactoryInputs};

/// H14 command ownership. Each command is decoded only when the native walk
/// reaches it; rejected commands do not read any later payloads.
#[derive(Clone, Copy, Debug)]
pub struct ObjectInstanceCommands<'a> {
    pub placement: &'a PlacementInstanceAssets,
    pub source: &'a [u8],
    pub main_payload: ByteSpan,
    pub relocations: &'a [AssetRelocation],
}

/// Inputs consumed by the implemented profiles in the world loader's mode 0.
/// H13 header +2 supplies each visual's initial model; the full resource DWORD
/// survives signed-low-WORD lookup. Actors require their additional dependencies.
pub struct ObjectConstructionRequest<'a> {
    pub resource_argument: u32,
    pub context: BaseContext,
    pub suffix: Option<ObjectInstanceCommands<'a>>,
    /// Shared H14 image for retained commands. Each reached command sees prior
    /// writes; the suffix must borrow this owner's original evidence bytes.
    pub command_allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    /// 0055F448: nonactors clear dirtiness through 0047E320; admitted actors run
    /// 0047E2E0 using the separately supplied 0055F3A0 input.
    pub network_active: bool,
    /// Required for actor selectors; nonactor profiles never consume it.
    pub actor: Option<ActorFactoryInputs<'a>>,
}

/// Position pointer custody at virtual+C. Existing callers supply value
/// context;47AAF0 instead lends the parent's live+32 through allocation.
#[derive(Clone, Copy)]
pub(crate) enum ConstructionPositionSource {
    Context,
    ActorPosition(ObjectId),
    /// 0048D820 points +0C at eight unwritten stack bytes; 00479D40 copies
    /// the first six to +32..+36, so the position becomes unknown.
    Unwritten,
}

#[derive(Clone, Copy)]
pub(crate) enum ConstructionFailurePolicy {
    Cleanup,
    RetainOnError,
}

impl ConstructionPositionSource {
    fn apply(
        self,
        arena: &mut ObjectArena,
        object: ObjectId,
        context: &mut BaseContext,
    ) -> Result<(), ObjectError> {
        match self {
            Self::Context => {}
            Self::ActorPosition(parent) => {
                context.position_words = Some(
                    arena
                        .object(parent)?
                        .base
                        .position_words
                        .ok_or(ObjectError::ActorPositionUnknown(parent))?,
                );
            }
            Self::Unwritten => {
                context.position_words = None;
                arena.forget_position_words(object)?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectConstructionOutcome {
    Constructed(ObjectId),
    /// 004602D0 deletes the already-published object at the first false
    /// virtual +04 result. No live handle remains for this attempt.
    Rejected {
        command_index: u32,
        code: i32,
    },
}

#[derive(Debug, Error)]
pub enum ObjectConstructionError {
    /// Boxed: the trigger's command errors reach player and construction errors.
    #[error(transparent)]
    Selector50(Box<super::Selector50Error>),
    #[error(transparent)]
    Selector48(Box<super::Selector48Error<std::convert::Infallible>>),
    #[error(
        "H13 argument {resource_argument} has constructor selector {selector}, not the ribbon's 67"
    )]
    FixedClassResource {
        resource_argument: u32,
        selector: u16,
    },
    #[error("H13 argument {resource_argument} selects class {selector}, which only its own factory constructs")]
    FixedClassFactory {
        resource_argument: u32,
        selector: u16,
    },
    #[error("dynamic construction retained object {object:?} after reached failure: {source}")]
    Retained {
        object: ObjectId,
        source: Box<ObjectConstructionError>,
    },
    #[error("H13 argument {resource_argument:#010x} has negative signed slot {signed_slot}")]
    NegativeResourceArgument {
        resource_argument: u32,
        signed_slot: i16,
    },
    #[error("H13 argument {resource_argument:#010x} is outside its {capacity}-slot table")]
    ResourceOutOfRange {
        resource_argument: u32,
        capacity: usize,
    },
    #[error("H13 argument {resource_argument:#010x} is not resident; constructor lazy loading is not implemented")]
    NotResident { resource_argument: u32 },
    #[error("H13 argument {resource_argument:#010x} does not resolve to its registered constructor record")]
    Registration { resource_argument: u32 },
    #[error("H13 argument {resource_argument:#010x} selects unsupported constructor {selector}; only selectors 1, 2, 6, 7, 9, 10, 11, 12, 14, 15, 16, 17, 18, 19, 21, 22, 30, 33, 34, 35, 36, 37, 39, 42, 47, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 64, 69, 73, 74, 75, 77, 78, 79, 80, 81, 82, 84, 85, 86, 87, 88, 89, 90 and 93 are implemented")]
    UnsupportedSelector {
        resource_argument: u32,
        selector: u16,
    },
    #[error("instance suffix declares {count} commands but only {available} are available")]
    MissingCommands { count: i32, available: usize },
    #[error("cleanup of failed construction left object {object:?} published: {source}")]
    Cleanup {
        object: ObjectId,
        source: ObjectError,
    },
    #[error("constructor {selector} requires explicit actor construction inputs")]
    MissingActorInputs { selector: u16 },
    #[error(
        "constructor {selector} requires explicit derived actor world parameters and process RNG"
    )]
    MissingDerivedActorInputs { selector: u16 },
    #[error("constructor {selector} requires the current world-start state")]
    MissingWorldStart { selector: u16 },
    #[error("constructor {selector}'s initializer reads the ground (00450A30) without H12")]
    MissingTerrain { selector: u16 },
    #[error("the selector-2 vehicle guard requires the 00530158 descriptor table")]
    MissingVehicleTable,
    #[error("the selector-2 vehicle in a network game requires the session's settings")]
    MissingVehicleNetworkSettings,
    #[error("the selector-16 camera requires the live First Person/Camera Style settings")]
    MissingCameraSettings,
    #[error("the selector-43 spawner's network +40 004542E0 is not ported")]
    Selector43NetworkBoundary,
    #[error("the selector-16 camera's 0040F7D0 commands are not ported")]
    Selector16Boundary,
    #[error("constructor {selector} requires the executable grant dependency table")]
    MissingGrantTable { selector: u16 },
    #[error("selector 12 command 23 requires an owned H14 allocation")]
    MissingSelector12CommandOwner,
    #[error("selector 35 command 23 requires an owned H14 allocation")]
    MissingSelector35CommandOwner,
    #[error("selector 34 command 23 requires an owned H14 allocation")]
    MissingSelector34CommandOwner,
    #[error("selector {selector} command 24 requires an owned H14 allocation")]
    MissingFamily4236d0CommandOwner { selector: u16 },
    #[error(transparent)]
    Selector30(#[from] super::Selector30Error),
    #[error(transparent)]
    Selector69(super::Selector69Error<std::convert::Infallible>),
    /// The ball's network start (00403CF0).
    #[error(transparent)]
    Ball(#[from] Box<super::BallError<std::convert::Infallible>>),
    #[error(transparent)]
    Selector49(super::Selector49Error<std::convert::Infallible>),
    #[error(transparent)]
    Constructor(#[from] ConstructorAssetError),
    #[error(transparent)]
    Instance(#[from] InstanceAssetError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    ActorCommand(#[from] ActorCommandError),
    #[error(transparent)]
    ActorConstruction(#[from] ActorConstructionError),
    #[error(transparent)]
    DerivedActor(#[from] DerivedActorError),
    #[error(transparent)]
    CreationListener(#[from] CreationListenerError),
    #[error(transparent)]
    InstanceAllocation(#[from] InstanceCommandAllocationError),
    #[error(transparent)]
    RuntimeAllocation(#[from] RuntimeAllocationError),
}

enum ConstructionPlan {
    Base {
        flags: u32,
    },
    /// 005286D8's 00479DB0: base context, then payload flags.
    Selector67 {
        flags: u32,
    },
    MapAnnotation {
        flags: u32,
    },
    TerrainLight {
        model_word: u16,
        flags: u32,
        light: TerrainLight,
    },
    /// Selector 54: 00464A20 runs +08 = 0047A610, then 004649E0.
    RotatingTerrainLight {
        model_word: u16,
        flags: u32,
        light: TerrainLight,
    },
    RouteNode {
        model_word: u16,
        flags: u32,
    },
    TimedPickup {
        model_word: u16,
        flags: u32,
    },
    /// Selector 14: 0046D840 with the same 0047A050/00479DB0 initializers.
    PlacedPickup {
        model_word: u16,
        flags: u32,
    },
    /// Selector 58: 004753E0; 00479DB0 runs +08 = 0047A610, then flags.
    Selector58 {
        model_word: u16,
        flags: u32,
    },
    /// The sinking patch: the rotating context, then 0043C3A0 keeps the
    /// payload and 00479D70 applies its flags.
    Selector83 {
        model_word: u16,
        flags: u32,
        table: super::Selector83Table,
    },
    /// Selector 45: 0045BF00, then 0045BFA0's 0047A610 and 0045BFC0.
    Selector45 {
        model_word: u16,
        payload: inf_formats::Selector45ConstructorPayload,
    },
    /// Selector 46: 00475610, then 004756F0's 0047A610 and 00475710.
    Selector46 {
        model_word: u16,
        payload: inf_formats::Selector46ConstructorPayload,
    },
    /// Selector 70: 0046E770, then 0046E860's 0047A610 and 0046E830, which
    /// reads no flags.
    Selector70 {
        model_word: u16,
        payload: Box<inf_formats::PointTableConstructorPayload>,
    },
    /// Selectors 71 and 72: 0046F7F0 or 0046F9E0, then 0046F870's or
    /// 0046FB00's 0047A610 and 0046F890 or 0046FB20.
    RaceMarker {
        selector: u16,
        model_word: u16,
        payload: inf_formats::FlagsWordsConstructorPayload,
    },
    /// Selector 92: 00403EC0, then 00403EF0's 0047A610 and 00403F10.
    Goal {
        model_word: u16,
        payload: inf_formats::GoalConstructorPayload,
    },
    /// Selector 76: 00431ED0, then 00479DB0 as selector 58.
    Selector76 {
        model_word: u16,
        flags: u32,
    },
    /// 00479DB0 runs +08 = 0047A050, then payload flags.
    Selector50 {
        model_word: u16,
        flags: u32,
    },
    /// The altitude ceiling, built as selector 50 is.
    Selector48 {
        model_word: u16,
        flags: u32,
    },
    /// The timed spawner, built as selector 50 is.
    Selector43 {
        model_word: u16,
        flags: u32,
    },
    Selector42 {
        model_word: u16,
        flags: u32,
    },
    Selector64 {
        model_word: u16,
        flags: u32,
    },
    Selector22 {
        model_word: u16,
        flags: u32,
    },
    Selector65 {
        model_word: u16,
        flags: u32,
    },
    Selector86 {
        flags: u32,
        allocation: Arc<RuntimeAllocation>,
        data_offset: u32,
    },
    /// Selector 75: the base object's 00479D40/00479DB0 and payload flags.
    Selector75 {
        flags: u32,
    },
    /// Selector 81, the same, then its +1C 00409360.
    /// Selector 94, the missile launcher: the same base object.
    Selector94 {
        flags: u32,
    },
    Selector81 {
        flags: u32,
    },
    Actor(actor_factory::ActorInitializationPlan),
    /// Selector 16: 00479DB0 runs +08 = 0047A610, then only payload flags.
    Camera {
        model_word: u16,
        flags: u32,
    },
}

impl ConstructionPlan {
    fn selector(&self) -> u16 {
        match self {
            Self::Base { .. } => 1,
            Self::Selector67 { .. } => 67,
            Self::MapAnnotation { .. } => 87,
            Self::TerrainLight { .. } => 19,
            Self::RotatingTerrainLight { .. } => 54,
            Self::RouteNode { .. } => 51,
            Self::TimedPickup { .. } => 89,
            Self::PlacedPickup { .. } => 14,
            Self::Selector58 { .. } => 58,
            Self::Selector83 { .. } => 83,
            Self::Selector45 { .. } => 45,
            Self::Selector46 { .. } => 46,
            Self::Selector70 { .. } => 70,
            Self::RaceMarker { selector, .. } => *selector,
            Self::Goal { .. } => 92,
            Self::Selector76 { .. } => 76,
            Self::Selector22 { .. } => 22,
            Self::Selector65 { .. } => 65,
            Self::Selector50 { .. } => 50,
            Self::Selector48 { .. } => 48,
            Self::Selector43 { .. } => 43,
            Self::Selector42 { .. } => 42,
            Self::Selector64 { .. } => 64,
            Self::Selector86 { .. } => 86,
            Self::Selector75 { .. } => 75,
            Self::Selector81 { .. } => 81,
            Self::Selector94 { .. } => 94,
            Self::Actor(plan) => plan.selector,
            Self::Camera { .. } => 16,
        }
    }

    fn class(&self) -> ObjectClassKind {
        match self {
            Self::Base { .. } => ObjectClassKind::Base,
            Self::Selector67 { .. } => unreachable!("rejected before allocation"),
            Self::MapAnnotation { .. } => ObjectClassKind::MapAnnotation,
            Self::TerrainLight { .. } => ObjectClassKind::TerrainLight,
            Self::RotatingTerrainLight { .. } => ObjectClassKind::RotatingTerrainLight,
            Self::RouteNode { .. } => ObjectClassKind::RouteNode,
            Self::TimedPickup { .. } => ObjectClassKind::TimedPickup,
            Self::PlacedPickup { .. } => ObjectClassKind::PlacedPickup,
            Self::Selector58 { .. } => ObjectClassKind::Selector58,
            Self::Selector83 { .. } => ObjectClassKind::Selector83,
            Self::Selector45 { .. } => ObjectClassKind::Selector45,
            Self::Selector46 { .. } => ObjectClassKind::Selector46,
            Self::Selector70 { .. } => ObjectClassKind::Selector70,
            Self::RaceMarker { selector: 71, .. } => ObjectClassKind::Selector71,
            Self::RaceMarker { .. } => ObjectClassKind::Selector72,
            Self::Goal { .. } => ObjectClassKind::Selector92,
            Self::Selector76 { .. } => ObjectClassKind::Selector76,
            Self::Selector22 { .. } => ObjectClassKind::Selector22,
            Self::Selector65 { .. } => ObjectClassKind::Selector65,
            Self::Selector50 { .. } => ObjectClassKind::Selector50,
            Self::Selector48 { .. } => ObjectClassKind::Selector48,
            Self::Selector43 { .. } => ObjectClassKind::Selector43,
            Self::Selector42 { .. } => ObjectClassKind::Selector42,
            Self::Selector64 { .. } => ObjectClassKind::Selector64,
            Self::Selector86 { .. } => ObjectClassKind::Selector86,
            Self::Selector75 { .. } => ObjectClassKind::Selector75,
            Self::Selector81 { .. } => ObjectClassKind::Selector81,
            Self::Selector94 { .. } => ObjectClassKind::Selector94,
            Self::Actor(_) | Self::Camera { .. } => {
                unreachable!("actor construction requires explicit inputs")
            }
        }
    }
}

/// Resolve current H13 custody, allocate/publish the exact class, then execute
/// initialization and ordered command completion. Unsupported classes fail
/// before allocation. Failure after publication attempts real class destruction.
/// If that cleanup reaches an unsupported retained-data boundary, `Cleanup`
/// identifies the still-published object and preserves its partial state.
pub fn construct_world_object(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    request: ObjectConstructionRequest<'_>,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    construct_planned(
        arena,
        runtime,
        request,
        ConstructionTarget::AuthoredClass,
        ConstructionPositionSource::Context,
        ConstructionFailurePolicy::Cleanup,
    )
}

/// The child factories' construction (0047AAF0, an actor's +4C, among
/// them). Besides the authored classes it builds the cannon's shot.
pub(crate) fn construct_world_object_from_source(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    request: ObjectConstructionRequest<'_>,
    position_source: ConstructionPositionSource,
    failure_policy: ConstructionFailurePolicy,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    construct_planned(
        arena,
        runtime,
        request,
        ConstructionTarget::ActorChild,
        position_source,
        failure_policy,
    )
}

/// 00460310 from 0048D8D3: the behavior registry's resource with a null
/// command list, an unwritten position, placement -1, no rotation and
/// factory mode 0. Only this path admits the registry's resource classes.
pub(crate) fn construct_behavior_resource(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    request: ObjectConstructionRequest<'_>,
    failure_policy: ConstructionFailurePolicy,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    construct_planned(
        arena,
        runtime,
        request,
        ConstructionTarget::BehaviorResource,
        ConstructionPositionSource::Unwritten,
        failure_policy,
    )
}

fn construct_planned(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    mut request: ObjectConstructionRequest<'_>,
    target: ConstructionTarget,
    position_source: ConstructionPositionSource,
    failure_policy: ConstructionFailurePolicy,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    if crate::event_log::enabled() {
        // 00460310's entry; 0047AAF0's child factories call it too.
        let position = match position_source {
            ConstructionPositionSource::Context => request.context.position_words,
            ConstructionPositionSource::ActorPosition(parent) => arena
                .object(parent)
                .ok()
                .and_then(|o| o.base.position_words),
            ConstructionPositionSource::Unwritten => None,
        };
        let unwritten = matches!(position_source, ConstructionPositionSource::Unwritten);
        crate::event_log::spawn(
            request.resource_argument,
            position,
            unwritten,
            request.context.placement_index,
        );
    }
    let plan = construction_plan(arena, runtime, request.resource_argument, target)?;
    // 00479C60 publishes all three nodes before virtual +0C initialization.
    let object = if let ConstructionPlan::Actor(plan) = &plan {
        let selector = plan.selector;
        let inputs = request
            .actor
            .as_mut()
            .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
        if selector == 6 {
            //004604F5 allocates138 and calls47A7A0 only. The actual shared
            //class must not enter425E60 or consume its mask/RNG phase.
            arena.allocate_actor(inputs.allocation)?
        } else if selector == 66 {
            arena.allocate_selector66(inputs.allocation)?
        } else if selector == 88 {
            arena.allocate_selector88(inputs.allocation)?
        } else if selector == 90 {
            arena.allocate_selector90(inputs.allocation)?
        } else if selector == 93 {
            arena.allocate_selector93(inputs.allocation)?
        } else if selector == 53 {
            arena.allocate_selector53(inputs.allocation)?
        } else if selector == 52 {
            arena.allocate_selector52(inputs.allocation)?
        } else if selector == 73 {
            arena.allocate_selector73(inputs.allocation)?
        } else if selector == 17 {
            arena.allocate_selector17(inputs.allocation, super::actor::PRISON_RADIUS)?
        } else if selector == 55 {
            arena.allocate_selector55(inputs.allocation, super::actor::BEAMER_RANGE)?
        } else if selector == 56 {
            arena.allocate_selector56(inputs.allocation)?
        } else if selector == 74 {
            arena.allocate_selector74(inputs.allocation)?
        } else if selector == 61 {
            // 004128C0 draws its keep distance +144 from the process RNG.
            let derived = inputs
                .derived
                .as_mut()
                .ok_or(ObjectConstructionError::MissingDerivedActorInputs { selector })?;
            arena.allocate_selector61(inputs.allocation, derived.rng)?
        } else if selector == 62 {
            arena.allocate_selector62(inputs.allocation)?
        } else if selector == 84 {
            arena.allocate_selector84(inputs.allocation)?
        } else if selector == 85 {
            arena.allocate_selector85(inputs.allocation)?
        } else if selector == 57 {
            arena.allocate_selector57(inputs.allocation)?
        } else if selector == 40 {
            arena.allocate_selector40(inputs.allocation)?
        } else if selector == 41 {
            arena.allocate_selector41(inputs.allocation)?
        } else if selector == 44 {
            arena.allocate_selector44(inputs.allocation)?
        } else if selector == 59 {
            arena.allocate_selector59(inputs.allocation)?
        } else if selector == 82 {
            arena.allocate_selector82(inputs.allocation)?
        } else if selector == 91 {
            arena.allocate_selector91(inputs.allocation)?
        } else if selector == 18 {
            arena.allocate_selector18(inputs.allocation, &mut *inputs.listeners)?
        } else if selector == 78 {
            arena.allocate_selector78(inputs.allocation, &mut *inputs.listeners)?
        } else if selector == 69 {
            arena.allocate_selector69(inputs.allocation)?
        } else if selector == 49 {
            arena.allocate_selector49(inputs.allocation)?
        } else if selector == 47 {
            arena.allocate_selector47(inputs.allocation)?
        } else if super::BEHAVIOR_RESOURCE_SELECTORS.contains(&selector)
            || selector == super::BOMB_SELECTOR
        {
            arena.allocate_behavior_resource(inputs.allocation, selector)?
        } else if selector == 15 {
            arena.allocate_selector15(inputs.allocation)?
        } else if selector == 37 {
            arena.allocate_selector37(inputs.allocation)?
        } else if selector == 39 {
            arena.allocate_selector39(inputs.allocation)?
        } else if selector == 30 {
            if inputs.world_start.is_none() {
                return Err(ObjectConstructionError::MissingWorldStart { selector });
            }
            arena.allocate_selector30_actor(inputs.allocation)?
        } else {
            // Selector12 reads grants during allocation and terrain during
            // postinitialization. Missing host owners fail before publication.
            if selector == 12 {
                if inputs.grant_table.is_none() {
                    return Err(ObjectConstructionError::MissingGrantTable { selector });
                }
                if inputs.terrain.is_none() {
                    return Err(DerivedActorError::MissingSelector12Terrain.into());
                }
            }
            let derived = inputs
                .derived
                .as_mut()
                .ok_or(ObjectConstructionError::MissingDerivedActorInputs { selector })?;
            let allocation = DerivedActorConstructionRequest {
                actor: inputs.allocation,
                world_mask_limits: derived.world_mask_limits,
            };
            match selector {
                2 => {
                    let network = if request.network_active {
                        let settings = inputs
                            .vehicle_table
                            .and_then(|vehicle| vehicle.network_settings)
                            .ok_or(ObjectConstructionError::MissingVehicleNetworkSettings)?;
                        Some(settings)
                    } else {
                        None
                    };
                    let object = arena.allocate_selector2_actor(allocation, derived.rng)?;
                    if let Some(settings) = network {
                        arena.set_selector2_network_form(object, settings)?;
                    }
                    object
                }
                // Case 9: 0x268 bytes and 004236D0 alone, the family's
                // own class (vtable 00520B84).
                9 => arena.allocate_family4236d0_actor(allocation, derived.rng)?,
                10 => arena.allocate_selector10_actor(allocation, derived.rng)?,
                21 => arena.allocate_selector21_actor(allocation, derived.rng)?,
                11 => arena.allocate_selector11_actor(allocation, derived.rng)?,
                12 => arena.allocate_selector12_actor(
                    allocation,
                    derived.rng,
                    Selector12GrantContext {
                        table: inputs
                            .grant_table
                            .expect("preflight requires the grant table"),
                        first_player_grants: inputs.first_player_grants,
                    },
                )?,
                34 => arena.allocate_selector34_actor(allocation, derived.rng)?,
                35 => arena.allocate_selector35_actor(allocation, derived.rng)?,
                36 => arena.allocate_selector36_actor(allocation, derived.rng)?,
                60 => arena.allocate_selector60_actor(allocation, derived.rng)?,
                77 => arena.allocate_selector77_actor(allocation, derived.rng)?,
                79 => arena.allocate_selector79_actor(allocation, derived.rng)?,
                80 => arena.allocate_selector80_actor(allocation, derived.rng)?,
                _ => arena.allocate_derived_actor(allocation, derived.rng)?,
            }
        }
    } else if let ConstructionPlan::Camera { .. } = &plan {
        if request.suffix.is_some() {
            return Err(ObjectConstructionError::Selector16Boundary);
        }
        let inputs = request
            .actor
            .as_mut()
            .ok_or(ObjectConstructionError::MissingActorInputs { selector: 16 })?;
        let settings = inputs
            .camera_settings
            .ok_or(ObjectConstructionError::MissingCameraSettings)?;
        arena.allocate_selector16(inputs.allocation, settings)?
    } else if let ConstructionPlan::Selector67 { .. } = &plan {
        // 00486600 constructs the ribbon itself; no placement selects it.
        return Err(ObjectConstructionError::FixedClassFactory {
            resource_argument: request.resource_argument,
            selector: 67,
        });
    } else {
        arena.allocate(plan.class())
    };
    let result = initialize_object(arena, runtime, object, plan, &mut request, position_source);
    if matches!(failure_policy, ConstructionFailurePolicy::RetainOnError) {
        if let Err(source) = result {
            arena.stop_object_construction(object)?;
            return Err(ObjectConstructionError::Retained {
                object,
                source: Box::new(source),
            });
        }
    }
    if !matches!(result, Ok(ObjectConstructionOutcome::Constructed(_))) {
        if let Some(inputs) = request.actor.as_mut() {
            inputs.listeners.unregister_owner(object);
        }
        if arena.object(object).is_ok() {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: object,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .map_err(|source| ObjectConstructionError::Cleanup { object, source })?;
        }
    }
    result
}

/// 004600F0 mode 0 for a ribbon 00486600 already constructed: 00460120 with
/// a NULL position and placement -1. The H13 entry, selector 67's own,
/// reaches 00479DB0 (context, then payload flags) and its commands the base
/// handler 00479E70. The caller owns a rejected ribbon's deletion.
pub(super) fn admit_constructed_selector67(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    object: ObjectId,
    resource_argument: u32,
    network_active: bool,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    let plan = construction_plan(
        arena,
        runtime,
        resource_argument,
        ConstructionTarget::AuthoredClass,
    )?;
    let ConstructionPlan::Selector67 { .. } = plan else {
        return Err(ObjectConstructionError::FixedClassResource {
            resource_argument,
            selector: plan.selector(),
        });
    };
    let mut request = ObjectConstructionRequest {
        resource_argument,
        context: BaseContext {
            position_words: None,
            placement_index: u32::MAX,
        },
        suffix: None,
        command_allocation: None,
        network_active,
        actor: None,
    };
    initialize_object(
        arena,
        runtime,
        object,
        plan,
        &mut request,
        ConstructionPositionSource::Context,
    )
}

fn initialize_object(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    object: ObjectId,
    plan: ConstructionPlan,
    request: &mut ObjectConstructionRequest<'_>,
    position_source: ConstructionPositionSource,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    let selector = plan.selector();
    match plan {
        ConstructionPlan::Actor(plan) => {
            return actor_factory::initialize_actor(
                arena,
                runtime,
                object,
                &plan,
                request,
                position_source,
            );
        }
        ConstructionPlan::Camera { model_word, flags } => {
            if request
                .actor
                .as_ref()
                .is_some_and(|inputs| inputs.spawn_mode_1)
            {
                arena.mark_actor_handle_mode1(object)?;
            }
            if let Some(handle) = request
                .actor
                .as_ref()
                .and_then(|inputs| inputs.network_handle)
            {
                arena.reassign_actor_handle(object, handle)?;
            }
            position_source.apply(arena, object, &mut request.context)?;
            return actor_factory::initialize_camera(
                arena, runtime, object, model_word, flags, request,
            );
        }
        ConstructionPlan::Base { flags }
        | ConstructionPlan::Selector67 { flags }
        | ConstructionPlan::MapAnnotation { flags }
        | ConstructionPlan::Selector75 { flags }
        | ConstructionPlan::Selector81 { flags }
        | ConstructionPlan::Selector94 { flags } => {
            position_source.apply(arena, object, &mut request.context)?;
            arena.initialize_context(object, request.context)?;
            arena.apply_payload_flags(object, flags)?;
        }
        ConstructionPlan::Selector86 {
            flags,
            allocation,
            data_offset,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            arena.initialize_context(object, request.context)?;
            arena.apply_payload_flags(object, flags)?;
            arena.set_selector86_data(object, &allocation, data_offset)?;
        }
        ConstructionPlan::TerrainLight {
            model_word,
            flags,
            light,
        } => {
            // 00448200 runs virtual +08 before 004481C0. Payload flags can
            // overwrite the flag-0x4 clear performed by visual initialization.
            position_source.apply(arena, object, &mut request.context)?;
            arena.initialize_visual_context(
                object,
                request.context,
                Some(u32::from(model_word)),
                runtime,
            )?;
            arena.initialize_terrain_light_payload(object, flags, light)?;
        }
        ConstructionPlan::RotatingTerrainLight {
            model_word,
            flags,
            light,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.initialize_terrain_light_payload(object, flags, light)?;
        }
        ConstructionPlan::TimedPickup { model_word, flags }
        | ConstructionPlan::PlacedPickup { model_word, flags }
        | ConstructionPlan::Selector22 { model_word, flags }
        | ConstructionPlan::Selector65 { model_word, flags }
        | ConstructionPlan::Selector50 { model_word, flags }
        | ConstructionPlan::Selector48 { model_word, flags }
        | ConstructionPlan::Selector43 { model_word, flags }
        | ConstructionPlan::Selector42 { model_word, flags } => {
            position_source.apply(arena, object, &mut request.context)?;
            arena.initialize_visual_context(
                object,
                request.context,
                Some(u32::from(model_word)),
                runtime,
            )?;
            arena.apply_payload_flags(object, flags)?;
        }
        ConstructionPlan::Selector58 { model_word, flags }
        | ConstructionPlan::Selector64 { model_word, flags }
        | ConstructionPlan::Selector76 { model_word, flags } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.apply_payload_flags(object, flags)?;
        }
        ConstructionPlan::Selector45 {
            model_word,
            ref payload,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.apply_payload_flags(object, payload.flags.value)?;
            arena.initialize_selector45_payload(object, payload)?;
        }
        ConstructionPlan::Selector46 {
            model_word,
            ref payload,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.apply_payload_flags(object, payload.flags.value)?;
            arena.initialize_selector46_payload(object, payload)?;
        }
        ConstructionPlan::Selector70 {
            model_word,
            ref payload,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.initialize_race_payload(object, payload)?;
        }
        ConstructionPlan::RaceMarker {
            model_word,
            ref payload,
            ..
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.apply_payload_flags(object, payload.flags.value)?;
            arena.initialize_race_marker_payload(object, payload)?;
        }
        ConstructionPlan::Goal {
            model_word,
            ref payload,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.apply_payload_flags(object, payload.flags.value)?;
            arena.initialize_goal_payload(object, payload)?;
        }
        ConstructionPlan::Selector83 {
            model_word,
            flags,
            table,
        } => {
            position_source.apply(arena, object, &mut request.context)?;
            let inputs = request
                .actor
                .as_ref()
                .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
            arena.initialize_rotating_visual_context(
                object,
                super::RotatingVisualContext {
                    base: request.context,
                    model_argument: Some(u32::from(model_word)),
                    rotation_words: inputs.rotation_words,
                },
                runtime,
                inputs.math,
            )?;
            arena.set_selector83_table(object, table)?;
            arena.apply_payload_flags(object, flags)?;
        }
        ConstructionPlan::RouteNode { model_word, flags } => {
            // 00479DB0 invokes final-vtable context 00404C30 first.
            position_source.apply(arena, object, &mut request.context)?;
            arena.initialize_route_context(
                object,
                request.context,
                Some(u32::from(model_word)),
                runtime,
            )?;
            arena.apply_payload_flags(object, flags)?;
        }
    }
    if let Some(rejected) = run_commands(
        arena,
        runtime,
        object,
        if selector == 89 || selector == 14 {
            CommandPolicy::TimedPickup {
                selector,
                network_active: request.network_active,
                allocation: request.command_allocation,
            }
        } else if let Some(allocation) = request.command_allocation {
            CommandPolicy::OwnedConstructor {
                selector,
                allocation,
            }
        } else {
            CommandPolicy::Constructor(selector)
        },
        request.resource_argument,
        request.suffix,
    )? {
        return Ok(ObjectConstructionOutcome::Rejected {
            command_index: rejected.index,
            code: rejected.code,
        });
    }
    // Selector89 +1C = 0046E0C0 activates updates; selector 14's 0046D7D0
    // takes the world pickup serial. The other nonactor profiles have RET
    // leaves. 004602D0 then clears both dirty bytes.
    if selector == 89 {
        arena.set_flag1(object, true)?;
    } else if selector == 14 {
        let host = request
            .actor
            .as_ref()
            .is_some_and(|inputs| inputs.network_world_active);
        arena.activate_placed_pickup(object, request.network_active, host)?;
    } else if selector == 75 {
        // 00435310 takes the world serial and tests its completion bit.
        arena.finish_selector75(object)?;
    } else if selector == 81 {
        arena.finish_selector81(object)?;
    }
    arena.clear_dirty(object)?;
    if request.network_active {
        // 00525730's +40 is its own 004542E0, which can dispose it.
        if selector == 43 {
            return Err(ObjectConstructionError::Selector43NetworkBoundary);
        }
        arena.clear_dirty(object)?; // Exact +40 = 0047E320.
    }
    // These actor queries return null, so no creation listener is visited.
    arena.mark_initialized(object)?;
    Ok(ObjectConstructionOutcome::Constructed(object))
}

impl From<super::Selector50Error> for ObjectConstructionError {
    fn from(error: super::Selector50Error) -> Self {
        Self::Selector50(Box::new(error))
    }
}

impl From<super::Selector48Error<std::convert::Infallible>> for ObjectConstructionError {
    fn from(error: super::Selector48Error<std::convert::Infallible>) -> Self {
        Self::Selector48(Box::new(error))
    }
}

/// The first false command in 00479E10, before enclosing failure cleanup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjectCommandRejection {
    pub index: u32,
    pub code: i32,
}

#[derive(Clone, Copy)]
pub(super) enum CommandPolicy<'a> {
    TimedPickup {
        /// 14 or 89; both bind 0046D780.
        selector: u16,
        network_active: bool,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    Constructor(u16),
    OwnedConstructor {
        selector: u16,
        allocation: &'a Arc<InstanceCommandAllocation>,
    },
    SharedActor {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The pickup spawner's 00454A30 over the shared 0047ADA0; +38 is the
    /// shared 00401370.
    Selector53 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The mining rig's 00454880 over the shared 0047ADA0; +38 is the
    /// shared 00401370.
    Selector52 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The Bomb's 00458C40 over the shared 0047ADA0; +38 is the shared
    /// 00401370.
    Selector29 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The zone's 0042D6F0 over the shared 0047ADA0; +38 is the shared
    /// 00401370.
    Selector74 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The capture node's 0043C2A0 over the shared 0047ADA0; +38 is the
    /// shared 00401370.
    Selector84 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The Boulders' 00405D10 over the shared 0047ADA0; +38 is the shared
    /// 00401370.
    Selector59 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The Force Field's 00478860 over the shared 0047ADA0; +38 is the
    /// shared 00401370.
    Selector18 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The barrier's 00411100 over the shared 0047ADA0; +38 is the shared
    /// 00401370.
    Selector69 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    Selector88 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    /// The ending director's 00406800 over the shared 0047ADA0: command
    /// 16 keeps its operand's address, as selector 88's does.
    Selector78 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
    },
    DerivedActor {
        selector: u16,
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
        terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
        /// Selector 2's guard copies these executable descriptors.
        vehicle_table: Option<VehicleFactoryInputs<'a>>,
    },
    Selector30 {
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
        terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
    },
    /// Selectors 15 (0042F100), 37 (0040A1D0) and 39 (0043B7D0) over
    /// 0040D9C0. Like selector 30, +38 is the shared 00401370 with no
    /// derived guard. Selector 15's grant commands read the grant table.
    Family40d800 {
        selector: u16,
        first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
        grant_table: Option<&'a inf_formats::GrantTable>,
        allocation: Option<&'a Arc<InstanceCommandAllocation>>,
        terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
    },
}

/// 00479E10 stores the low WORD before the class-specific guard, then
/// reads each reached command. Missing later entries cannot preempt rejection.
pub(super) fn run_commands(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    object: ObjectId,
    policy: CommandPolicy<'_>,
    resource_argument: u32,
    suffix: Option<ObjectInstanceCommands<'_>>,
) -> Result<Option<ObjectCommandRejection>, ObjectConstructionError> {
    arena.owned_mut(object)?.state.base.resource_word = resource_argument as u16;
    // Every family-428650 table (selectors 9, 10, 12, 21, 34, 35 and 36)
    // binds +38 to 004270E0, which copies the home position +222.
    if matches!(
        policy,
        CommandPolicy::DerivedActor {
            selector: 9 | 10 | 12 | 21 | 34 | 35 | 36,
            ..
        }
    ) {
        arena.admit_family428650_actor(object)?;
    } else if let CommandPolicy::DerivedActor {
        selector: 2,
        vehicle_table,
        ..
    } = policy
    {
        // Selector 2's +38 is 0041A620, not the shared 00426C80.
        let vehicle = vehicle_table.ok_or(ObjectConstructionError::MissingVehicleTable)?;
        let mode = vehicle.network_settings.map(|settings| settings[0x10 / 4]);
        arena.admit_selector2_actor(object, vehicle.table, mode)?;
    } else if let CommandPolicy::DerivedActor { .. } = policy {
        arena.admit_derived_actor(object)?;
    }
    let Some(suffix) = suffix else {
        return Ok(None);
    };
    let allocation = match policy {
        CommandPolicy::OwnedConstructor { allocation, .. } => Some(allocation),
        CommandPolicy::SharedActor { allocation, .. }
        | CommandPolicy::Selector53 { allocation, .. }
        | CommandPolicy::Selector52 { allocation, .. }
        | CommandPolicy::Selector29 { allocation, .. }
        | CommandPolicy::Selector74 { allocation, .. }
        | CommandPolicy::Selector84 { allocation, .. }
        | CommandPolicy::Selector59 { allocation, .. }
        | CommandPolicy::Selector18 { allocation, .. }
        | CommandPolicy::Selector69 { allocation, .. }
        | CommandPolicy::Selector88 { allocation, .. }
        | CommandPolicy::Selector78 { allocation, .. }
        | CommandPolicy::Selector30 { allocation, .. }
        | CommandPolicy::Family40d800 { allocation, .. }
        | CommandPolicy::TimedPickup { allocation, .. }
        | CommandPolicy::DerivedActor { allocation, .. } => allocation,
        _ => None,
    };
    if let Some(allocation) = allocation {
        allocation.validate_commands(suffix)?;
    }
    let mut next_index = 0_u32;
    loop {
        // Native rereads the signed count after every accepted command. The
        // unowned path retains its explicit caller-supplied immutable suffix.
        let count = if let Some(allocation) = allocation {
            allocation
                .runtime_allocation()
                .dword(suffix.placement.suffix_offset)? as i32
        } else {
            suffix.placement.count.value
        };
        if next_index >= count.max(0) as u32 {
            break;
        }
        let index = next_index;
        next_index += 1;
        let command_offset = if let Some(allocation) = allocation {
            let field = suffix
                .placement
                .suffix_offset
                .checked_add(4)
                .and_then(|offset| offset.checked_add(index.checked_mul(4)?))
                .ok_or(InstanceAssetError::IntegerOverflow(
                    "instance command pointer",
                ))?;
            if !allocation
                .relocations()
                .iter()
                .any(|entry| entry.field_offset == field)
            {
                return Err(InstanceAssetError::MissingCommandRelocation(field).into());
            }
            allocation.runtime_allocation().dword(field)?
        } else {
            suffix
                .placement
                .commands
                .get(index as usize)
                .ok_or(ObjectConstructionError::MissingCommands {
                    count,
                    available: suffix.placement.commands.len(),
                })?
                .allocation_offset
        };
        // Decode afresh after every prior handler. A retained-data write may
        // overlap a later command, including its code or signed operand.
        let current_source = if let Some(allocation) = allocation {
            allocation.validate_commands(suffix)?;
            allocation
                .runtime_allocation()
                .validate_scalar_read(command_offset, 4)?;
            Some(allocation.runtime_allocation().source_snapshot())
        } else {
            None
        };
        let view = decode_instance_command(InstanceCommandRequest {
            read_scope: inf_formats::InstanceCommandReadScope::Dispatch,
            source: current_source.as_deref().unwrap_or(suffix.source),
            main_payload: suffix.main_payload,
            relocations: suffix.relocations,
            command_offset,
            target: match policy {
                CommandPolicy::Constructor(selector)
                | CommandPolicy::OwnedConstructor { selector, .. } => {
                    InstanceCommandTarget::Constructor(selector)
                }
                CommandPolicy::SharedActor { .. } => InstanceCommandTarget::SharedActor,
                CommandPolicy::Selector53 { .. } => InstanceCommandTarget::Constructor(53),
                CommandPolicy::Selector52 { .. } => InstanceCommandTarget::Constructor(52),
                CommandPolicy::Selector29 { .. } => InstanceCommandTarget::Constructor(29),
                CommandPolicy::Selector74 { .. } => InstanceCommandTarget::Constructor(74),
                CommandPolicy::Selector84 { .. } => InstanceCommandTarget::Constructor(84),
                CommandPolicy::Selector59 { .. } => InstanceCommandTarget::Constructor(59),
                CommandPolicy::Selector18 { .. } => InstanceCommandTarget::Constructor(18),
                CommandPolicy::Selector69 { .. } => InstanceCommandTarget::Constructor(69),
                CommandPolicy::TimedPickup { selector, .. } => {
                    InstanceCommandTarget::Constructor(selector)
                }
                CommandPolicy::Selector88 { .. } => InstanceCommandTarget::Constructor(88),
                CommandPolicy::Selector78 { .. } => InstanceCommandTarget::Constructor(78),
                CommandPolicy::Selector30 { .. } => InstanceCommandTarget::Constructor(30),
                CommandPolicy::Family40d800 { selector, .. } => {
                    InstanceCommandTarget::Constructor(selector)
                }
                CommandPolicy::DerivedActor { selector, .. } => {
                    InstanceCommandTarget::Constructor(selector)
                }
            },
        })?;
        if let Some(allocation) = allocation {
            command_reads::validate(
                arena,
                object,
                policy,
                allocation.runtime_allocation(),
                suffix.main_payload,
                &view,
            )?;
        }
        if let CommandPolicy::Selector88 {
            first_player_grants,
            allocation,
        } = policy
        {
            if view.code.value == 16 {
                if let Some(allocation) = allocation {
                    allocation.validate_commands(suffix)?;
                }
            }
            if !arena.apply_selector88_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
                allocation,
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index: index as u32,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector78 {
            first_player_grants,
            allocation,
        } = policy
        {
            if view.code.value == 16 {
                if let Some(allocation) = allocation {
                    allocation.validate_commands(suffix)?;
                }
            }
            if !arena.apply_selector78_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
                allocation,
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index: index as u32,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::DerivedActor {
            first_player_grants,
            selector,
            allocation,
            terrain,
            ..
        } = policy
        {
            let context = ActorCommandContext {
                runtime,
                first_player_grants,
                terrain,
            };
            let accepted = if view.target == InstanceCommandTarget::Constructor(11) {
                arena.apply_selector11_command(object, &view, context)?
            } else if selector == 60 && view.code.value == 19 {
                let allocation = allocation.ok_or(ActorCommandError::Selector60(
                    Selector60Error::MissingCommandOwner,
                ))?;
                let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } =
                    view.payload
                else {
                    return Err(ActorCommandError::Payload(view.code.value).into());
                };
                arena
                    .set_selector60_spawn_list(object, allocation, allocation_offset)
                    .map_err(ActorCommandError::from)?;
                true
            } else if matches!(selector, 9 | 10 | 21 | 36) && view.code.value == 24 {
                let allocation = allocation
                    .ok_or(ObjectConstructionError::MissingFamily4236d0CommandOwner { selector })?;
                let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } =
                    view.payload
                else {
                    return Err(ActorCommandError::Payload(view.code.value).into());
                };
                arena.set_family4236d0_retained(
                    object,
                    allocation.runtime_allocation(),
                    allocation_offset,
                )?;
                true
            } else if selector == 12 && view.code.value == 23 {
                let allocation =
                    allocation.ok_or(ObjectConstructionError::MissingSelector12CommandOwner)?;
                let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } =
                    view.payload
                else {
                    return Err(ActorCommandError::Payload(view.code.value).into());
                };
                arena.set_selector12_retained(
                    object,
                    allocation.runtime_allocation(),
                    allocation_offset,
                )?;
                true
            } else if selector == 34 && view.code.value == 23 {
                let allocation =
                    allocation.ok_or(ObjectConstructionError::MissingSelector34CommandOwner)?;
                let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } =
                    view.payload
                else {
                    return Err(ActorCommandError::Payload(view.code.value).into());
                };
                arena.set_selector34_retained(
                    object,
                    allocation.runtime_allocation(),
                    allocation_offset,
                )?;
                true
            } else if selector == 35 && view.code.value == 23 {
                let allocation =
                    allocation.ok_or(ObjectConstructionError::MissingSelector35CommandOwner)?;
                let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } =
                    view.payload
                else {
                    return Err(ActorCommandError::Payload(view.code.value).into());
                };
                arena.set_selector35_retained(
                    object,
                    allocation.runtime_allocation(),
                    allocation_offset,
                )?;
                true
            } else if matches!(selector, 10 | 21) {
                arena.apply_selector10_command(object, &view, context)?
            } else if selector == 12 {
                arena.apply_selector12_command(object, &view, context)?
            } else if matches!(selector, 9 | 36) {
                arena.apply_family4236d0_command(object, &view, context)?
            } else if selector == 34 {
                arena.apply_selector34_command(object, &view, context)?
            } else if selector == 35 {
                arena.apply_selector35_command(object, &view, context)?
            } else if selector == 79 {
                arena.apply_selector79_command(object, &view, context)?
            } else {
                arena.apply_derived_actor_command(object, &view, context)?
            };
            if !accepted {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector30 {
            first_player_grants,
            terrain,
            ..
        } = policy
        {
            if !arena.apply_selector30_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Family40d800 {
            selector,
            first_player_grants,
            grant_table,
            allocation,
            terrain,
        } = policy
        {
            let context = ActorCommandContext {
                runtime,
                first_player_grants,
                terrain,
            };
            let accepted = if selector == 15 {
                arena.apply_selector15_command(object, &view, context, grant_table)?
            } else if selector == 37 {
                arena.apply_selector37_command(object, &view, context)?
            } else if selector == 49 {
                arena.apply_selector49_command(object, &view, context)?
            } else if selector == 47 {
                arena.apply_selector47_command(object, &view, context)?
            } else {
                arena.apply_selector39_command(object, &view, context, allocation)?
            };
            if !accepted {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector53 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector53_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector84 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector84_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector74 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector74_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector29 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_bomb29_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector52 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector52_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector59 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector59_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector18 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector18_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::Selector69 {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_selector69_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if let CommandPolicy::SharedActor {
            first_player_grants,
            ..
        } = policy
        {
            if !arena.apply_actor_command(
                object,
                &view,
                ActorCommandContext {
                    runtime,
                    first_player_grants,
                    terrain: None,
                },
            )? {
                return Ok(Some(ObjectCommandRejection {
                    index: index as u32,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(87) {
            if !arena.apply_map_annotation_command(object, &view)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        // 00445900 code 2 keeps the operand address at +40.
        if view.target == InstanceCommandTarget::Constructor(94) && view.code.value == 2 {
            let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } = view.payload
            else {
                unreachable!("selector 94 code 2 decodes as a retained pointer")
            };
            let allocation = allocation.ok_or(InstanceCommandAllocationError::MissingOwner)?;
            arena.set_selector94_zones(
                object,
                crate::object_runtime::RetainedInstanceCommand::new(
                    Arc::clone(allocation),
                    allocation_offset,
                ),
            )?;
            continue;
        }
        // 004407E0 code 3 keeps the operand address at +74.
        if view.target == InstanceCommandTarget::Constructor(64) && view.code.value == 3 {
            let InstanceCommandPayload::RetainedPayloadPointer { allocation_offset } = view.payload
            else {
                unreachable!("selector 64 code 3 decodes as a retained pointer")
            };
            let allocation = allocation.ok_or(InstanceCommandAllocationError::MissingOwner)?;
            arena.set_selector64_link(object, allocation, allocation_offset)?;
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(42) && matches!(view.code.value, 3..=6)
        {
            if !arena.apply_selector42_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(81) && view.code.value > 1 {
            if !arena.apply_selector81_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(43) && matches!(view.code.value, 3..=7)
        {
            if !arena.apply_selector43_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(45) && matches!(view.code.value, 3..=5)
        {
            if !arena.apply_selector45_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(46) && matches!(view.code.value, 3..=4)
        {
            if !arena.apply_selector46_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if matches!(
            (view.target, view.code.value),
            (InstanceCommandTarget::Constructor(71), 3..=4)
                | (InstanceCommandTarget::Constructor(72), 3..=6)
                | (InstanceCommandTarget::Constructor(92), 3)
        ) {
            if !arena.apply_race_command(object, view.code.value, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(48) && matches!(view.code.value, 3..=6)
        {
            if !arena.apply_selector48_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        if view.target == InstanceCommandTarget::Constructor(50) && matches!(view.code.value, 3..=6)
        {
            if !arena.apply_selector50_command(object, &view.payload)? {
                return Ok(Some(ObjectCommandRejection {
                    index,
                    code: view.code.value,
                }));
            }
            continue;
        }
        match view.payload {
            InstanceCommandPayload::NoPayloadRead => {}
            InstanceCommandPayload::PickupKind(value) => {
                let CommandPolicy::TimedPickup { network_active, .. } = policy else {
                    unreachable!("pickup command on non-pickup constructor");
                };
                if !arena.select_pickup_kind(object, value.value, network_active, runtime)? {
                    return Ok(Some(ObjectCommandRejection {
                        index,
                        code: view.code.value,
                    }));
                }
            }
            InstanceCommandPayload::PickupValue(value) => {
                arena.timed_pickup_mut(object)?.value_46 = value.value;
            }
            InstanceCommandPayload::Selector86Data {
                allocation_offset, ..
            } => {
                let allocation =
                    allocation.ok_or(InstanceCommandAllocationError::MissingSelector86Owner)?;
                arena.set_selector86_data(
                    object,
                    allocation.runtime_allocation(),
                    allocation_offset,
                )?;
            }
            InstanceCommandPayload::ModelSlot(model) => {
                arena.set_visual_model(object, model.raw.value, runtime)?;
            }
            InstanceCommandPayload::TerrainLightChannel { channel, value } => {
                terrain_light_mut(arena, object)?.channels[usize::from(channel)] = value.value;
            }
            InstanceCommandPayload::TerrainLightDrawGate(value) => {
                terrain_light_mut(arena, object)?.draw_gate = value.value;
            }
            InstanceCommandPayload::TerrainLightRadius(value) => {
                terrain_light_mut(arena, object)?.radius = value.value;
            }
            InstanceCommandPayload::RouteNodePlacementReference(value) => {
                arena.route_mut(object)?.predecessor_word = value.value.wrapping_sub(1);
            }
            InstanceCommandPayload::RouteNodeMode(value) if value.value <= 1 => {
                if value.value == 0 {
                    arena.route_mut(object)?.flags |= 1;
                }
            }
            InstanceCommandPayload::RouteNodeFlagOr { mask } => {
                arena.route_mut(object)?.flags |= mask;
            }
            _ => {
                return Ok(Some(ObjectCommandRejection {
                    index: index as u32,
                    code: view.code.value,
                }));
            }
        }
    }
    Ok(None)
}

fn terrain_light_mut(
    arena: &mut ObjectArena,
    object: ObjectId,
) -> Result<&mut TerrainLight, ObjectError> {
    match &mut arena.owned_mut(object)?.state.class {
        ObjectClass::TerrainLight { light, .. }
        | ObjectClass::RotatingTerrainLight { light, .. } => light
            .as_mut()
            .ok_or(ObjectError::UninitializedTerrainLight(object)),
        _ => Err(ObjectError::NotTerrainLight(object)),
    }
}

/// Generic 004603B0 selects the authored class. Direct 004400D0 has already
/// published constructor93 before 00460120 reads the same resource shape.
/// Decoded nonactor payload layouts remain outside that direct typed domain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstructionTarget {
    AuthoredClass,
    PublishedSelector93,
    /// 0048D820's 00460310: the only path for the registry's resource classes.
    BehaviorResource,
    /// A child factory: the authored classes, and selector 32, the cannon's
    /// shot, whose update, draw and +98 are ported (shot32).
    ActorChild,
}

/// Shared retained H13 custody for dynamic factory dependency selection.
/// This does not allocate an object or read its context position pointer.
/// It answers for the classes the child factories build.
pub(crate) fn resident_constructor_selector(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    resource_argument: u32,
) -> Result<u16, ObjectConstructionError> {
    Ok(construction_plan(
        arena,
        runtime,
        resource_argument,
        ConstructionTarget::ActorChild,
    )?
    .selector())
}

pub(super) fn selector93_initialization_plan(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
) -> Result<actor_factory::ActorInitializationPlan, ObjectConstructionError> {
    const RESOURCE_ARGUMENT: u32 = 0x34;
    match construction_plan(
        arena,
        runtime,
        RESOURCE_ARGUMENT,
        ConstructionTarget::PublishedSelector93,
    )? {
        ConstructionPlan::Actor(plan) => Ok(plan),
        _ => Err(ObjectConstructionError::Registration {
            resource_argument: RESOURCE_ARGUMENT,
        }),
    }
}

fn construction_plan(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    resource_argument: u32,
    target: ConstructionTarget,
) -> Result<ConstructionPlan, ObjectConstructionError> {
    let signed_slot = resource_argument as u16 as i16;
    let slot = u16::try_from(signed_slot).map_err(|_| {
        ObjectConstructionError::NegativeResourceArgument {
            resource_argument,
            signed_slot,
        }
    })?;
    let capacity = runtime.resource_capacity(13).unwrap_or(0);
    // Native compares inclusively against the raw allocated count and can
    // admit a one-past slot. The portable table rejects that invalid address.
    if usize::from(slot) >= capacity {
        return Err(ObjectConstructionError::ResourceOutOfRange {
            resource_argument,
            capacity,
        });
    }
    let resource = runtime
        .resource(13, slot)
        .ok_or(ObjectConstructionError::NotResident { resource_argument })?;
    let registration = || ObjectConstructionError::Registration { resource_argument };
    let region = resource
        .overlay
        .layout()
        .regions
        .get(resource.registration.region_index)
        .filter(|region| region.handler.get() == 13)
        .ok_or_else(registration)?;
    let original_bank = parse_constructor_asset_bank(resource.overlay.bytes(), region)?;
    let allocation = arena.constructor_allocations.retain(
        resource.overlay,
        resource.registration.region_index,
        &original_bank,
    )?;
    let current_source = allocation.source_snapshot();
    let bank = parse_constructor_asset_bank(&current_source, region)?;
    let local = usize::from(resource.registration.local_index);
    let (prefix, asset) = bank
        .prefixes
        .entries
        .get(local)
        .zip(bank.assets.get(local))
        .filter(|(prefix, asset)| prefix.local_index == asset.local_index)
        .ok_or_else(registration)?;
    // No admitted initializer changes residency. Actor payload sharing is keyed
    // by actual source allocation identity, never by a reusable resource slot.
    let selector = match target {
        ConstructionTarget::AuthoredClass
        | ConstructionTarget::BehaviorResource
        | ConstructionTarget::ActorChild => prefix.constructor_selector,
        // 004400D0 calls 00440010 directly; header +0 never selects its class.
        ConstructionTarget::PublishedSelector93 => 93,
    };
    let behavior_resource = target == ConstructionTarget::BehaviorResource;
    let actor_child = target == ConstructionTarget::ActorChild;
    match (selector, &asset.payload) {
        (1, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::Base { flags: flags.value })
        }
        (67, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::Selector67 { flags: flags.value })
        }
        (87, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::MapAnnotation { flags: flags.value })
        }
        (75, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::Selector75 { flags: flags.value })
        }
        (94, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::Selector94 { flags: flags.value })
        }
        (81, ConstructorPayloadView::Flags(flags)) => {
            Ok(ConstructionPlan::Selector81 { flags: flags.value })
        }
        (19, ConstructorPayloadView::TerrainLight(payload)) => Ok(ConstructionPlan::TerrainLight {
            model_word: prefix.argument_word,
            flags: payload.flags.value,
            light: TerrainLight {
                channels: payload.channels.value,
                draw_gate: payload.draw_gate.value,
                radius: payload.radius.value,
            },
        }),
        (54, ConstructorPayloadView::TerrainLight(payload)) => {
            Ok(ConstructionPlan::RotatingTerrainLight {
                model_word: prefix.argument_word,
                flags: payload.flags.value,
                light: TerrainLight {
                    channels: payload.channels.value,
                    draw_gate: payload.draw_gate.value,
                    radius: payload.radius.value,
                },
            })
        }
        (16, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Camera {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (51, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::RouteNode {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (89, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::TimedPickup {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (14, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::PlacedPickup {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (45, ConstructorPayloadView::Selector45(payload)) => Ok(ConstructionPlan::Selector45 {
            model_word: prefix.argument_word,
            payload: payload.clone(),
        }),
        (46, ConstructorPayloadView::Selector46(payload)) => Ok(ConstructionPlan::Selector46 {
            model_word: prefix.argument_word,
            payload: payload.clone(),
        }),
        (70, ConstructorPayloadView::PointTable16(payload)) => Ok(ConstructionPlan::Selector70 {
            model_word: prefix.argument_word,
            payload: payload.clone(),
        }),
        (selector @ (71 | 72), ConstructorPayloadView::FlagsWords(payload)) => {
            Ok(ConstructionPlan::RaceMarker {
                selector,
                model_word: prefix.argument_word,
                payload: payload.clone(),
            })
        }
        (92, ConstructorPayloadView::Goal(payload)) => Ok(ConstructionPlan::Goal {
            model_word: prefix.argument_word,
            payload: payload.clone(),
        }),
        (83, ConstructorPayloadView::Selector83(payload)) => Ok(ConstructionPlan::Selector83 {
            model_word: prefix.argument_word,
            flags: payload.flags.value,
            table: super::Selector83Table {
                extent: payload.extent.map(|field| field.value),
                depth: payload.depth.value,
                travel: payload.travel.value,
                risen: payload.risen.value,
                sunk: payload.sunk.value,
            },
        }),
        (58, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector58 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (64, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector64 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (42, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector42 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (50, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector50 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (48, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector48 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (43, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector43 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (22, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector22 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (65, ConstructorPayloadView::Flags(flags)) => Ok(ConstructionPlan::Selector65 {
            model_word: prefix.argument_word,
            flags: flags.value,
        }),
        (76, ConstructorPayloadView::Flags(flags)) if behavior_resource => {
            Ok(ConstructionPlan::Selector76 {
                model_word: prefix.argument_word,
                flags: flags.value,
            })
        }
        (
            selector @ (25 | 26 | 27 | 28 | 29 | 31 | 32 | 38 | 68),
            ConstructorPayloadView::SharedActor(payload),
        ) if (behavior_resource && selector != 29)
            || (actor_child && matches!(selector, 25 | 26 | 27 | 28 | 29 | 31 | 32 | 38 | 68))
            // 004603B0's case 0x26 (00460CA9) builds a placed mine as it
            // builds the resource factory's: Kankutan places seven. Its case
            // 0x1D (00460AA5) builds the Bomb, which Altair Prime and
            // Helniazine place and the factories sell.
            || (matches!(selector, 29 | 38) && target == ConstructionTarget::AuthoredClass) =>
        {
            Ok(ConstructionPlan::Actor(
                actor_factory::ActorInitializationPlan {
                    selector,
                    model_word: prefix.argument_word,
                    payload: arena.actor_payloads.retain(
                        resource.overlay,
                        resource.registration.region_index,
                        payload,
                    ),
                    allocation,
                    payload_offset: prefix.payload_offset,
                },
            ))
        }
        (86, ConstructorPayloadView::Selector86(payload)) => Ok(ConstructionPlan::Selector86 {
            flags: payload.flags.value,
            allocation,
            data_offset: payload.retained_offset,
        }),
        (
            selector @ (2 | 6 | 7 | 9 | 10 | 11 | 12 | 15 | 17 | 18 | 21 | 30 | 33 | 34 | 35 | 36
            | 37 | 39 | 40 | 41 | 44 | 47 | 49 | 52 | 53 | 55 | 56 | 57 | 59 | 60 | 61
            | 62 | 66 | 69 | 73 | 74 | 77 | 78 | 79 | 80 | 82 | 84 | 85 | 88 | 90 | 91
            | 93),
            ConstructorPayloadView::SharedActor(payload),
        ) => Ok(ConstructionPlan::Actor(
            actor_factory::ActorInitializationPlan {
                selector,
                model_word: prefix.argument_word,
                payload: arena.actor_payloads.retain(
                    resource.overlay,
                    resource.registration.region_index,
                    payload,
                ),
                allocation,
                payload_offset: prefix.payload_offset,
            },
        )),
        (
            1 | 2 | 6 | 7 | 9 | 10 | 11 | 12 | 14 | 15 | 16 | 17 | 18 | 19 | 21 | 22 | 30 | 33 | 34
            | 35 | 36 | 37 | 39 | 44 | 47 | 49 | 51 | 52 | 53 | 54 | 55 | 56 | 57 | 58 | 59 | 60
            | 61 | 62 | 66 | 69 | 73 | 74 | 75 | 77 | 78 | 79 | 80 | 81 | 82 | 84 | 85 | 86 | 87
            | 88 | 89 | 90 | 91 | 92 | 93,
            _,
        ) => Err(registration()),
        (selector, _) => Err(ObjectConstructionError::UnsupportedSelector {
            resource_argument,
            selector,
        }),
    }
}

#[cfg(test)]
#[path = "construction_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "light_native_tests.rs"]
mod light_native_tests;

#[cfg(test)]
#[path = "route_construction_tests.rs"]
mod route_construction_tests;

#[cfg(test)]
#[path = "actor_factory_tests.rs"]
mod actor_factory_tests;

#[cfg(test)]
#[path = "selector6_factory_tests.rs"]
mod selector6_factory_tests;

#[cfg(test)]
#[path = "selector60_factory_tests.rs"]
mod selector60_factory_tests;

#[cfg(test)]
#[path = "selector30_35_factory_tests.rs"]
mod selector30_35_factory_tests;

#[cfg(test)]
#[path = "selector34_factory_tests.rs"]
mod selector34_factory_tests;

#[cfg(test)]
#[path = "selector36_factory_tests.rs"]
mod selector36_factory_tests;

#[cfg(test)]
#[path = "selector10_factory_tests.rs"]
mod selector10_factory_tests;

#[cfg(test)]
#[path = "map_annotation/factory_tests.rs"]
mod map_annotation_factory_tests;
