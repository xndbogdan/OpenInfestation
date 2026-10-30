//! Actor-specific portions of 00460120, with explicit construction inputs.

use std::sync::{Arc, Weak};

use inf_formats::{
    native_basis::NativeBasisMath, ActorConstructorPayload, GrantTable, GRANT_MASK_WORDS,
};

use super::{
    run_commands, CommandPolicy, ObjectConstructionError, ObjectConstructionOutcome,
    ObjectConstructionRequest,
};
use crate::object_runtime::{
    ActorCommandContext, ActorConstructionRequest, ActorInitializationRequest,
    CreationListenerEffects, CreationListeners, Family4236d0InitializationRequest, ObjectArena,
    ObjectId, RotatingVisualContext, RuntimeAllocation, Selector12GrantContext,
    Selector12InitializationRequest, Selector34InitializationRequest,
    Selector35InitializationRequest,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::{LoadedOverlay, ResidentRuntime};

/// Dependencies of a real actor factory, separate from authored placement data.
pub struct ActorFactoryInputs<'a> {
    pub allocation: ActorConstructionRequest<'a>,
    pub rotation_words: Option<[u16; 3]>,
    pub local_owner: u8,
    pub first_player_grants: Option<&'a [u32; GRANT_MASK_WORDS]>,
    /// Executable dependency records required by selector 12's embedded grant owner.
    pub grant_table: Option<&'a GrantTable>,
    pub math: &'a NativeBasisMath,
    /// Required by 00425E60 and its concrete extensions; selectors 53/88/90/93
    /// and shared selectors 6/30 do not consume this extra derived phase.
    pub derived: Option<DerivedActorFactoryInputs<'a>>,
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    /// Native 0055F3A0, tested by +40=0047E2E0 only for local placed actors.
    pub network_world_active: bool,
    /// Current H12 surface for commands 20/21 in the 00428650 family.
    pub terrain: Option<&'a crate::terrain_runtime::TerrainRuntime>,
    /// Real mutable world owner consumed by selector 30 postinitialization.
    pub world_start: Option<crate::world_start::WorldStartContext<'a>>,
    /// Live settings +68/+6C read by the selector-16 camera constructor.
    pub camera_settings: Option<crate::object_runtime::CameraSettings>,
    /// Factory mode 1 (0046A890's vehicle spawn): 004600D0 calls 004625D0
    /// on the new actor's handle before class initialization.
    pub spawn_mode_1: bool,
    /// Factory mode 2, a remote actor (0047E600): 004600D0's 0047E2B0 gives
    /// the new actor this handle before class initialization.
    pub network_handle: Option<crate::object_runtime::ActorHandle>,
    /// Selector 2's own inputs.
    pub vehicle_table: Option<VehicleFactoryInputs<'a>>,
}

/// What selector 2's constructor `004156E0` and guard `0041A620` read.
#[derive(Clone, Copy, Debug)]
pub struct VehicleFactoryInputs<'a> {
    /// Executable 00530158 descriptors the guard copies.
    pub table: &'a [u32; inf_formats::VEHICLE_BEHAVIOR_TABLE_DWORDS],
    /// Settings +00..+4F where 0055F448 is set: both read the mode +10, the
    /// constructor the vehicle +18 and the arena +1C.
    pub network_settings: Option<&'a [u32; 20]>,
}

pub(in crate::object_runtime) struct ActorInitializationPlan {
    pub selector: u16,
    pub model_word: u16,
    pub payload: Arc<ActorConstructorPayload>,
    pub allocation: Arc<RuntimeAllocation>,
    pub payload_offset: u32,
}

pub struct DerivedActorFactoryInputs<'a> {
    /// Signed H14/0 +0E/+0F values read by 0048E120.
    pub world_mask_limits: [i8; 2],
    /// Shared process state at 0055F18C, consumed in constructor order.
    pub rng: &'a mut u32,
}

/// The admitted actor's exact virtual +40 suffix after the shared policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::object_runtime) enum ActorCompletionClass {
    Shared,
    /// 004025B0 adds dirty WORD bit 0200 after returning from 0047E2E0.
    Selector11,
    /// 00413120, the Boulders', adds dirty bit 0010 after it.
    Selector59,
    /// 00403CF0, the ball's: 00413120, then its own (`start_network_ball`).
    Ball,
    /// 004863C0, the Lasers', cannon shots' and banner's, adds dirty bits 0280.
    Shot,
    /// 00410F50, the camera's, adds dirty bit 8000.
    Camera,
}

/// Each final table's +40: 004025B0 (11), 00413120 (the Boulders', shared by
/// 25, 28, 59, 61, 68 and 82), 004863C0 (31, 32 and the banner, 40), else
/// 0047E2E0.
pub(in crate::object_runtime) fn actor_completion_class(selector: u16) -> ActorCompletionClass {
    match selector {
        11 => ActorCompletionClass::Selector11,
        25 | 28 | 59 | 61 | 68 | 82 => ActorCompletionClass::Selector59,
        91 => ActorCompletionClass::Ball,
        31 | 32 | 40 => ActorCompletionClass::Shot,
        _ => ActorCompletionClass::Shared,
    }
}

/// Network inputs are consumed only after the creation-listener walk.
#[derive(Clone, Copy)]
pub(in crate::object_runtime) enum ActorCompletionNetwork<'a> {
    /// Existing generic factories retain their caller-supplied bool policies.
    CapturedShared {
        active: bool,
        world_active: bool,
        class: ActorCompletionClass,
    },
    /// Direct shared-actor completion reads the current process DWORD at
    /// 00460288. This borrow does not authorize arbitrary native global writes.
    CurrentShared {
        process: &'a ParticleFactoryProcessState,
        world_active: bool,
    },
}

pub(in crate::object_runtime) struct ActorCompletionRequest<'a> {
    pub listeners: &'a mut CreationListeners,
    pub listener_effects: &'a mut dyn CreationListenerEffects,
    pub network: ActorCompletionNetwork<'a>,
}

/// Successful 004602D0/00460120 tail, after commands and the actual +1C.
/// Failure retains earlier callback effects; allocation cleanup belongs to
/// each enclosing factory and is deliberately absent from this shared phase.
pub(in crate::object_runtime) fn complete_actor_initialization(
    arena: &mut ObjectArena,
    object: ObjectId,
    request: ActorCompletionRequest<'_>,
) -> Result<(), ObjectConstructionError> {
    arena.clear_dirty(object)?;
    // The native moving cursor observes removals, appends and nested walks.
    // An empty list invokes no interface; a reached callback remains required.
    request
        .listeners
        .notify(object, arena, request.listener_effects)?;
    let network = match request.network {
        ActorCompletionNetwork::CapturedShared {
            active,
            world_active,
            class,
        } => active.then_some((world_active, class)),
        ActorCompletionNetwork::CurrentShared {
            process,
            world_active,
        } => (process.network_flag != 0).then_some((world_active, ActorCompletionClass::Shared)),
    };
    if let Some((world_active, class)) = network {
        initialize_network_actor(arena, object, world_active)?;
        match class {
            ActorCompletionClass::Selector11 => {
                arena.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            ActorCompletionClass::Selector59 => {
                arena.owned_mut(object)?.state.base.dirty_word |= 0x0010;
            }
            ActorCompletionClass::Ball => arena.start_network_ball(object).map_err(Box::new)?,
            ActorCompletionClass::Shot => {
                arena.owned_mut(object)?.state.base.dirty_word |= 0x0280;
            }
            ActorCompletionClass::Camera => {
                arena.owned_mut(object)?.state.base.dirty_word |= 0x8000;
            }
            ActorCompletionClass::Shared => {}
        }
    }
    arena.mark_initialized(object)?;
    Ok(())
}

#[derive(Clone, Debug, Default)]
pub(in crate::object_runtime) struct ActorPayloadCache(Vec<ActorPayloadEntry>);

#[derive(Clone, Debug)]
struct ActorPayloadEntry {
    source: Weak<LoadedOverlay>,
    region: usize,
    payload_start: usize,
    payload: Weak<ActorConstructorPayload>,
}

impl ActorPayloadCache {
    pub(super) fn retain(
        &mut self,
        source: &Arc<LoadedOverlay>,
        region: usize,
        value: &ActorConstructorPayload,
    ) -> Arc<ActorConstructorPayload> {
        self.0
            .retain(|entry| entry.source.strong_count() != 0 && entry.payload.strong_count() != 0);
        for entry in &self.0 {
            if entry.region == region
                && entry.payload_start == value.prefix_span.start
                && Weak::ptr_eq(&entry.source, &Arc::downgrade(source))
            {
                if let Some(payload) = entry.payload.upgrade() {
                    return payload;
                }
            }
        }
        let payload = Arc::new(value.clone());
        self.0.push(ActorPayloadEntry {
            source: Arc::downgrade(source),
            region,
            payload_start: value.prefix_span.start,
            payload: Arc::downgrade(&payload),
        });
        payload
    }
}

pub(super) fn initialize_actor(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    object: ObjectId,
    plan: &ActorInitializationPlan,
    request: &mut ObjectConstructionRequest<'_>,
    position_source: super::ConstructionPositionSource,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    let selector = plan.selector;
    let inputs = request
        .actor
        .as_mut()
        .ok_or(ObjectConstructionError::MissingActorInputs { selector })?;
    if inputs.spawn_mode_1 {
        arena.mark_actor_handle_mode1(object)?;
    }
    if let Some(handle) = inputs.network_handle {
        arena.reassign_actor_handle(object, handle)?;
    }
    position_source.apply(arena, object, &mut request.context)?;
    let initialization = ActorInitializationRequest {
        context: RotatingVisualContext {
            base: request.context,
            model_argument: Some(u32::from(plan.model_word)),
            rotation_words: inputs.rotation_words,
        },
        payload: &plan.payload,
        local_owner: inputs.local_owner,
    };
    // Selectors 6, 17, 18, 44, 52, 53, 55, 56, 57, 61, 62, 66, 69, 73, 78, 84,
    // 85, 88, 90 and 93 share the 0047AD70 wrapper with no 00426C10 extension: bare
    // shared-actor initialization, then suffix commands. (Selector 6's
    // commships reach their models through code-2 overrides.)
    if matches!(
        selector,
        6 | 17
            | 18
            | 44
            | 52
            | 53
            | 55
            | 56
            | 57
            | 61
            | 62
            | 66
            | 69
            | 73
            | 78
            | 84
            | 85
            | 88
            | 90
            | 93
    ) {
        arena.initialize_actor(object, initialization, runtime, inputs.math)?;
    } else if matches!(selector, 40 | 41) {
        // 0045B9A0 and 0045BEE0: 0047AD70's two calls, then the ground
        // 00479DD0 (the banner's home).
        arena.initialize_actor(object, initialization, runtime, inputs.math)?;
        let terrain = inputs
            .terrain
            .ok_or(ObjectConstructionError::MissingTerrain { selector })?;
        arena.initialize_banner_payload(object, terrain)?;
    } else if selector == 74 {
        // 0042D760: +08 = 0047A610 and 0047AC10, as 0047AD70, then 0042D780's
        // extents.
        arena.initialize_actor(object, initialization, runtime, inputs.math)?;
        arena.initialize_selector74_payload(object, plan.payload.selector74.as_deref())?;
    } else if matches!(selector, 59 | 82 | 91) {
        // 00405DE0: 0047AD70's two calls, then the rest timer and the
        // sleeping flags (selector 82's table keeps the Boulders' +0C).
        arena.initialize_selector59_actor(object, initialization, runtime, inputs.math)?;
        if selector == 91 {
            // The ball's 004038E0 runs 00405DE0 first.
            arena.finish_ball_payload(object)?;
        }
    } else if crate::object_runtime::BEHAVIOR_RESOURCE_SELECTORS.contains(&selector) {
        // 00485460/00455720/004557D0/00456840/00458380: +08 = 0047A610 and
        // 0047AC10, as 0047AD70, then the class's payload reads.
        arena.initialize_actor(object, initialization, runtime, inputs.math)?;
        arena.initialize_behavior_resource_payload(
            object,
            plan.payload.behavior_resource.as_deref(),
        )?;
    } else if selector == crate::object_runtime::BOMB_SELECTOR {
        // 00458CA0: 00455720's base initialization and payload copy, then
        // 00479DD0 drops the Bomb onto the ground under it (00450A30).
        arena.initialize_actor(object, initialization, runtime, inputs.math)?;
        arena.initialize_behavior_resource_payload(
            object,
            plan.payload.behavior_resource.as_deref(),
        )?;
        let terrain = inputs
            .terrain
            .ok_or(ObjectConstructionError::MissingTerrain { selector })?;
        arena.drop_bomb29(object, terrain)?;
    } else if selector == 30 {
        arena.initialize_selector30_actor(object, initialization, runtime, inputs.math)?;
    } else if selector == 15 {
        arena.initialize_selector15_actor(object, initialization, runtime, inputs.math)?;
    } else if matches!(selector, 37 | 39 | 47 | 49) {
        arena.initialize_family40d800_actor(object, initialization, runtime, inputs.math)?;
    } else if selector == 12 {
        arena.initialize_selector12_actor(
            object,
            Selector12InitializationRequest {
                actor: initialization,
                allocation: &plan.allocation,
                payload_offset: plan.payload_offset,
            },
            runtime,
            inputs.math,
        )?;
    } else if selector == 34 {
        arena.initialize_selector34_actor(
            object,
            Selector34InitializationRequest {
                actor: initialization,
                allocation: &plan.allocation,
                payload_offset: plan.payload_offset,
            },
            runtime,
            inputs.math,
        )?;
    } else if selector == 35 {
        arena.initialize_selector35_actor(
            object,
            Selector35InitializationRequest {
                actor: initialization,
                allocation: &plan.allocation,
                payload_offset: plan.payload_offset,
            },
            runtime,
            inputs.math,
        )?;
    } else if matches!(selector, 9 | 10 | 21 | 36) {
        let request = Family4236d0InitializationRequest {
            actor: initialization,
            allocation: &plan.allocation,
            payload_offset: plan.payload_offset,
        };
        match selector {
            // 004239C0 itself: the virtual context, then 00423850.
            9 => arena.initialize_family4236d0_actor(object, request, runtime, inputs.math)?,
            // Selector 21 keeps 00425270.
            10 | 21 => arena.initialize_selector10_actor(object, request, runtime, inputs.math)?,
            _ => arena.initialize_selector36_actor(object, request, runtime, inputs.math)?,
        }
    } else {
        arena.initialize_derived_actor(object, initialization, runtime, inputs.math)?;
    }
    let policy = if selector == 88 {
        CommandPolicy::Selector88 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 78 {
        CommandPolicy::Selector78 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 53 {
        CommandPolicy::Selector53 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 52 {
        CommandPolicy::Selector52 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == crate::object_runtime::BOMB_SELECTOR {
        CommandPolicy::Selector29 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 74 {
        CommandPolicy::Selector74 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 84 {
        CommandPolicy::Selector84 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if matches!(selector, 59 | 82 | 91) {
        // Selectors 82's and 91's +04 is the Boulders' 00405D10.
        CommandPolicy::Selector59 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 18 {
        CommandPolicy::Selector18 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 69 {
        CommandPolicy::Selector69 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if matches!(
        selector,
        6 | 17 | 40 | 41 | 44 | 55 | 56 | 57 | 61 | 62 | 66 | 73 | 85 | 90 | 93
    ) || crate::object_runtime::BEHAVIOR_RESOURCE_SELECTORS.contains(&selector)
    {
        // +04 = 0047ADA0 and +38 = 00401370, with no derived guard. The
        // Remote Beamer's 00473F70 and the Prison Unit's 0046E4F0 pass
        // codes through 15 on.
        CommandPolicy::SharedActor {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
        }
    } else if selector == 30 {
        CommandPolicy::Selector30 {
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
            terrain: inputs.terrain,
        }
    } else if matches!(selector, 15 | 37 | 39 | 47 | 49) {
        CommandPolicy::Family40d800 {
            selector,
            first_player_grants: inputs.first_player_grants,
            grant_table: inputs.grant_table,
            allocation: request.command_allocation,
            terrain: inputs.terrain,
        }
    } else {
        CommandPolicy::DerivedActor {
            selector,
            first_player_grants: inputs.first_player_grants,
            allocation: request.command_allocation,
            terrain: inputs.terrain,
            vehicle_table: inputs.vehicle_table,
        }
    };
    if let Some(rejected) = run_commands(
        arena,
        runtime,
        object,
        policy,
        request.resource_argument,
        request.suffix,
    )? {
        return Ok(ObjectConstructionOutcome::Rejected {
            command_index: rejected.index,
            code: rejected.code,
        });
    }
    match selector {
        2 => arena.finish_selector2_initialization(object)?,
        // The family's own +1C.
        9 => arena.finish_family4236d0_initialization(object)?,
        10 | 21 => arena.finish_selector10_initialization(object)?,
        11 => arena.finish_selector11_initialization(object)?,
        12 => arena.finish_selector12_initialization(
            object,
            inputs.terrain,
            Selector12GrantContext {
                table: inputs
                    .grant_table
                    .ok_or(ObjectConstructionError::MissingGrantTable { selector })?,
                first_player_grants: inputs.first_player_grants,
            },
        )?,
        30 => arena.finish_selector30_initialization(
            object,
            ActorCommandContext {
                runtime,
                first_player_grants: inputs.first_player_grants,
                terrain: inputs.terrain,
            },
            inputs
                .world_start
                .as_mut()
                .ok_or(ObjectConstructionError::MissingWorldStart { selector })?,
        )?,
        15 => arena.finish_selector15_initialization(
            object,
            runtime,
            inputs
                .grant_table
                .ok_or(ObjectConstructionError::MissingGrantTable { selector })?,
        )?,
        // Both final tables keep the family +1C = 0040E000.
        37 | 39 | 47 => arena.finish_family40d800_initialization(object)?,
        34 => arena.finish_selector34_initialization(object, inputs.terrain)?,
        35 => arena.finish_selector35_initialization(object)?,
        36 => arena.finish_selector36_initialization(object)?,
        // 00405DC0 in place of the shared 0047AAD0.
        // Selectors 82's and 91's tables keep the Boulders' +1C.
        59 | 82 | 91 => arena.finish_selector59_initialization(object)?,
        // 0042D950 in place of the shared 0047AAD0.
        74 => arena.finish_selector74_initialization(object)?,
        // 0043C110, then the shared 0047AAD0.
        84 => arena.finish_selector84_initialization(object)?,
        // 00408090 for all three tables.
        77 | 79 | 80 => arena.finish_selector77_initialization(object)?,
        // 00411150 replaces the shared +1C; a zero length or bolt count
        // disposes the barrier, and the factory's tail still runs.
        69 => {
            arena
                .finish_selector69_initialization(object, inputs.terrain)
                .map_err(ObjectConstructionError::Selector69)?;
        }
        // 00476470, then the family's 0040E000.
        49 => {
            arena
                .finish_selector49_initialization(object, inputs.terrain)
                .map_err(ObjectConstructionError::Selector49)?;
        }
        _ => arena.finish_actor_initialization(object)?,
    }
    complete_actor_initialization(
        arena,
        object,
        ActorCompletionRequest {
            listeners: &mut *inputs.listeners,
            listener_effects: &mut *inputs.listener_effects,
            network: ActorCompletionNetwork::CapturedShared {
                active: request.network_active,
                world_active: inputs.network_world_active,
                class: actor_completion_class(selector),
            },
        },
    )?;
    Ok(ObjectConstructionOutcome::Constructed(object))
}

/// Selector 16 through 00460120: 00479DB0 context and flags, no commands
/// (preflighted), shared +1C 0047AAD0, dirty clear, listeners, initialized.
pub(super) fn initialize_camera(
    arena: &mut ObjectArena,
    runtime: &ResidentRuntime,
    object: ObjectId,
    model_word: u16,
    flags: u32,
    request: &mut ObjectConstructionRequest<'_>,
) -> Result<ObjectConstructionOutcome, ObjectConstructionError> {
    let inputs = request
        .actor
        .as_mut()
        .ok_or(ObjectConstructionError::MissingActorInputs { selector: 16 })?;
    arena.initialize_rotating_visual_context(
        object,
        RotatingVisualContext {
            base: request.context,
            model_argument: Some(u32::from(model_word)),
            rotation_words: inputs.rotation_words,
        },
        runtime,
        inputs.math,
    )?;
    arena.apply_payload_flags(object, flags)?;
    // 00479E10 still stores the resource WORD; guard +38 is 00401370 and the
    // preflighted suffix is absent, so no command is read.
    run_commands(
        arena,
        runtime,
        object,
        CommandPolicy::Constructor(16),
        request.resource_argument,
        None,
    )?;
    arena.finish_actor_initialization(object)?;
    complete_actor_initialization(
        arena,
        object,
        ActorCompletionRequest {
            listeners: &mut *inputs.listeners,
            listener_effects: &mut *inputs.listener_effects,
            network: ActorCompletionNetwork::CapturedShared {
                active: request.network_active,
                world_active: inputs.network_world_active,
                class: ActorCompletionClass::Camera,
            },
        },
    )?;
    Ok(ObjectConstructionOutcome::Constructed(object))
}

/// The admitted actors' virtual +40, 0047E2E0, distinct from nonactor dirty clear.
pub(in crate::object_runtime) fn initialize_network_actor(
    arena: &mut ObjectArena,
    object: ObjectId,
    world_active: bool,
) -> Result<(), crate::object_runtime::ObjectError> {
    arena.clear_dirty(object)?;
    let base = &mut arena.owned_mut(object)?.state.base;
    if base.flags & 0x8000_0000 != 0 {
        return Ok(());
    }
    if base.placement_word == u16::MAX {
        base.dirty_word = 0x80;
    } else if !world_active {
        base.dirty_word = 0x8000;
        arena.dispose(object)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "selector88_native_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "actor_completion_tests.rs"]
mod completion_tests;

#[cfg(test)]
#[path = "actor_payload_cache_tests.rs"]
mod payload_cache_tests;
