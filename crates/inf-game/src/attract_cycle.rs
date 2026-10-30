//! Complete attract outer cycle 00487982..00487B2E against current owners.
//!
//! The camera was captured by an earlier caller; neither bypass nor live-handle
//! arms validate it. Constructor-backed index/armed/object fields coexist with
//! unavailable timer/handle backing. Exclusive state and immutable process,
//! settings, authored table and resident-source loans exclude synchronous
//! aliases of those owners. Reached arena/service effects survive errors.
//! This kernel does not own initial 00487410 player preparation, world loading,
//! camera capture/lifetime or production attract admission/playback.

use inf_formats::FrontendAttractSelectionTable;
use thiserror::Error;

use crate::{
    object_runtime::{
        setup_attract_actor, ActorConstructionError, ActorTerrainHost, AttractActorSetupError,
        AttractActorSetupRequest, AttractActorSetupState, CameraSettings, ObjectArena, ObjectId,
        Selector93FactoryRequest,
    },
    scheduler::SchedulerTick,
    session::{SessionPhase, SessionState},
    world_completion::{
        complete_world, WorldCompletionError, WorldCompletionHost, WorldCompletionInputs,
        WorldCompletionOutcome, WorldCompletionRequest,
    },
    world_mission::WorldMission,
};

const ADVANCE_AFTER_MICROS: u32 = 30_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AttractCycleState {
    pub publication: AttractActorSetupState,
    pub selection_index_04: u32,
    pub timer_micros_5d4: Option<u32>,
    /// Native BYTE: every nonzero value bypasses the first-arm stores.
    pub armed_5d8: u8,
}

impl AttractCycleState {
    /// Only the proven stores of 00487BE0, not a complete game constructor.
    /// Ordinary nonzeroing heap allocation supplies no timer/handle default.
    pub fn from_native_constructor_fields() -> Self {
        Self {
            publication: AttractActorSetupState {
                object_5e0: None,
                handle_5dc: None,
            },
            selection_index_04: 0,
            timer_micros_5d4: None,
            armed_5d8: 0,
        }
    }
}

/// All current owner inputs of one reached cycle. Merely lending an owner does
/// not validate/read its data before the original branch reaches it.
pub struct AttractCycleRequest<'a> {
    pub state: &'a mut AttractCycleState,
    pub attract: bool,
    pub captured_camera: Option<ObjectId>,
    pub tick: SchedulerTick,
    pub selections: &'a FrontendAttractSelectionTable,
    pub factory: Selector93FactoryRequest<'a>,
    pub settings: CameraSettings,
    pub mission: &'a mut WorldMission,
    pub session: &'a mut SessionState,
}

/// Required actual lower services; there are no successful default methods.
pub struct AttractCycleServices<'a, T, C> {
    pub terrain: &'a mut T,
    /// ProcessSoundRuntime owns the represented flag-bit2 one-shot queue.
    /// A generic native linked list/address domain is not inferred from it.
    pub completion: &'a mut C,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttractCycleOutcome {
    Skipped,
    Waiting,
    Advanced { next_selection: u32 },
}

#[derive(Debug, Error)]
pub enum AttractCycleError<T, C> {
    #[error("attract cycle requires current game handle+5DC")]
    HandleUnavailable,
    #[error("attract cycle requires current game timer+5D4")]
    TimerUnavailable,
    #[error(transparent)]
    Resolve(#[from] ActorConstructionError),
    #[error(transparent)]
    Setup(AttractActorSetupError<T>),
    #[error(transparent)]
    Completion(WorldCompletionError<C>),
}

pub fn tick_attract_cycle<T: ActorTerrainHost, C: WorldCompletionHost>(
    arena: &mut ObjectArena,
    request: AttractCycleRequest<'_>,
    services: AttractCycleServices<'_, T, C>,
) -> Result<AttractCycleOutcome, AttractCycleError<T::Error, C::Error>> {
    if !request.attract {
        return Ok(AttractCycleOutcome::Skipped);
    }
    let Some(camera) = request.captured_camera else {
        return Ok(AttractCycleOutcome::Skipped);
    };
    if request.state.armed_5d8 == 0 {
        request.state.timer_micros_5d4 = Some(0);
        request.state.armed_5d8 = 1;
    }
    let handle = request
        .state
        .publication
        .handle_5dc
        .ok_or(AttractCycleError::HandleUnavailable)?;
    // The process reference is borrowed now; its network DWORD is consumed
    // only at the later wrapper. Setup owns any reached NULL preflight.
    let process = request.factory.process;
    if arena.resolve_actor_handle(handle)?.is_none() {
        setup_attract_actor(
            arena,
            AttractActorSetupRequest {
                publication: &mut request.state.publication,
                // Reborrow the same factory owners for this completed phase,
                // so the publication loan ends before the later timer/index.
                factory: Selector93FactoryRequest {
                    listeners: &mut *request.factory.listeners,
                    listener_effects: &mut *request.factory.listener_effects,
                    ..request.factory
                },
                camera,
                settings: request.settings,
            },
            services.terrain,
        )
        .map_err(AttractCycleError::Setup)?;
    }
    let timer = request
        .state
        .timer_micros_5d4
        .ok_or(AttractCycleError::TimerUnavailable)?;
    // EBP holds the scheduler's first (already capped) argument. Preserve its
    // raw DWORD bits, including supplied signed-negative/wrapping domains.
    let sum = timer.wrapping_add(request.tick.delta_micros as u32);
    request.state.timer_micros_5d4 = Some(sum);
    if sum <= ADVANCE_AFTER_MICROS {
        return Ok(AttractCycleOutcome::Waiting);
    }
    request.state.timer_micros_5d4 = Some(0);
    let next_selection =
        select_next_488c90(&mut request.state.selection_index_04, request.selections);
    let outcome = complete_world(
        request.mission,
        arena,
        WorldCompletionRequest {
            next_selection,
            start_ordinal: 1,
            actor: None,
        },
        WorldCompletionInputs {
            network_active: process.network_flag != 0,
            process: None,
        },
        services.completion,
    )
    .map_err(AttractCycleError::Completion)?;
    match outcome {
        WorldCompletionOutcome::Accepted { request_phase } => {
            let _ = request.session.request_phase(request_phase);
        }
        WorldCompletionOutcome::NetworkIgnored => {}
        WorldCompletionOutcome::ActorRejected => {
            unreachable!("null-actor completion bypasses actor rejection")
        }
    }
    request.state.armed_5d8 = 0;
    let _ = request.session.request_phase(SessionPhase::Phase8);
    Ok(AttractCycleOutcome::Advanced { next_selection })
}

/// Complete shared original selector, used by initial Prepare and later cycles.
/// Only the caller's current index changes; timer/armed custody stays there.
pub(crate) fn select_next_488c90(index: &mut u32, table: &FrontendAttractSelectionTable) -> u32 {
    *index = index.wrapping_add(1);
    if *index >= table.values.len() as u32 {
        *index = 0;
    }
    table.values[*index as usize]
}

#[cfg(test)]
#[path = "attract_cycle_tests.rs"]
mod tests;
