//! Initial player preparation from 00487410's successful-allocation branch.
//!
//! The caller retains the actual arena, constructor carriers and InputRuntime.
//! Every carrier enters that retained collection before construction can stop;
//! this operation neither drops references nor substitutes a serialized player.
//! Allocation NULL, native unwinding, network mission selection and enclosing
//! load/teardown remain separate domains. The supplied attract value and authored
//! table stay stable through the call. Immutable current source loans exclude
//! synchronous descriptor, process and CRT rebinding.

use inf_formats::{
    FrontendAttractSelectionTable, MissionElapsedPolicy, MissionResetRequest, MissionSaveState,
};
use thiserror::Error;

use crate::{
    attract_cycle::{select_next_488c90, AttractCycleState},
    input::InputRuntime,
    object_runtime::{
        LiveGrantContext, ObjectArena, PlayerConstructionError, PlayerId,
        PlayerInputDescriptorSource, WorldPlayerConstruction,
    },
    offline_prepare::OFFLINE_MISSION_SELECTION,
};

/// Last reached phase, retained by the actual caller on errors. This is not a
/// native field, replay guard or a resumable constructor/mission transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerPreparePhase {
    NotStarted,
    OwnerPublished,
    Constructing,
    SelectingMission,
    Complete,
}

/// The session preparation enclosing this construction. Both set owner byte
/// +5C2 and construct through 00469D60; their mission tails differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerPrepareSource {
    /// 00487410, Frontend to phase 9: armed +5D8 = 0, then the attract or
    /// offline selection through 0048EBA0 with W+34 cleared.
    NewGame,
    /// 00487380, Frontend to phase 3: no +5D8 write, and 0048EBA0(G+1E0, 0)
    /// keeps the current W+30 selection and W+34 elapsed time. The enclosing
    /// caller's 00449B80 then restores both from the working profile.
    FrontendLoad,
}

/// One explicitly supplied successful native allocation. callback_context is
/// the stable logical player identity for the same InputRuntime's whole input
/// lifetime, unique among its simultaneously live players, not a PlayerId cast.
pub struct PlayerPrepareRequest<'a> {
    pub arena: &'a mut ObjectArena,
    pub constructions: &'a mut Vec<WorldPlayerConstruction>,
    pub mission: &'a mut MissionSaveState,
    pub owner: &'a mut bool,
    /// The same actual armed/index/publication/timer owner as the later cycle.
    /// Preparation writes only armed and the reached attract-selection index.
    pub cycle: &'a mut AttractCycleState,
    pub phase: &'a mut PlayerPreparePhase,
    pub callback_context: u32,
    /// Supplied current 0055F14C; read only after complete player construction,
    /// and only by the new-game source.
    pub attract: bool,
    pub source: PlayerPrepareSource,
    /// 00487485's network world: the host's arena's (00488A20 over
    /// settings +1C) or a guest's host world word 0054E814. None where the
    /// caller has no network session to read it from.
    pub network_world: Option<u32>,
}

/// Actual lower owners, borrowed through this operation without preflight.
/// The enclosing caller supplies an ephemeral projection of its retained current
/// classification into grant text data; this owner supplies no literal defaults.
pub struct PlayerPrepareContext<'a, 'g, S> {
    pub input: &'a mut InputRuntime,
    pub descriptors: &'a mut S,
    pub grants: &'a LiveGrantContext<'g>,
    pub selections: &'a FrontendAttractSelectionTable,
}

/// Completed native phases. Saved output/first-player export is a distinct
/// enclosing operation; this receipt never selects a player for that caller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayerPrepareOutcome {
    pub constructed: PlayerId,
    pub selection: u32,
}

#[derive(Debug, Error)]
pub enum PlayerPrepareError<E> {
    #[error(transparent)]
    Construction(#[from] PlayerConstructionError<E>),
    #[error("00487485 requires network mission selection for current DWORD {network:#010x}")]
    NetworkMissionSelection { network: u32 },
}

/// Owner1 -> retained carrier -> complete469D60 -> armed0 -> current selection
/// branch -> reset/Clear. Session elapsed aliases, serialized export and debug
/// mission overrides belong to the enclosing caller.
pub fn prepare_player<S: PlayerInputDescriptorSource>(
    request: PlayerPrepareRequest<'_>,
    context: PlayerPrepareContext<'_, '_, S>,
) -> Result<PlayerPrepareOutcome, PlayerPrepareError<S::Error>> {
    *request.owner = true; // 0048742E, before allocation or lower source reads.
    *request.phase = PlayerPreparePhase::OwnerPublished;
    let construction = request
        .arena
        .reserve_world_player_construction(request.callback_context);
    request.constructions.push(construction);
    *request.phase = PlayerPreparePhase::Constructing;
    let constructed = request.arena.construct_world_player(
        request
            .constructions
            .last_mut()
            .expect("retained allocation"),
        context.input,
        context.descriptors,
        context.grants,
    )?; // 00487453. Typed errors retain the caller's actual carrier/prefix.

    if request.source == PlayerPrepareSource::FrontendLoad {
        // 004873B6..004873C3: 0048EBA0(G+1E0, 0) rewrites the current W+30,
        // clears the status/progress arrays and keeps W+34.
        *request.phase = PlayerPreparePhase::SelectingMission;
        let selection = request.mission.mission_selection;
        request.mission.reset(MissionResetRequest {
            selection,
            elapsed_policy: MissionElapsedPolicy::Preserve,
        });
        *request.phase = PlayerPreparePhase::Complete;
        return Ok(PlayerPrepareOutcome {
            constructed,
            selection,
        });
    }
    request.cycle.armed_5d8 = 0; // 00487458, after completed construction.
    *request.phase = PlayerPreparePhase::SelectingMission;
    let selection = if request.attract {
        // 0048745F..0048747A bypasses the network selection read.
        select_next_488c90(&mut request.cycle.selection_index_04, context.selections)
    } else {
        // Completed construction proved this same immutable process exists.
        // Read its full DWORD here, not before input/grant construction.
        let network = context
            .grants
            .process
            .expect("completed constructor requires the same process owner")
            .network_flag;
        // 0048747C..004874A7.
        if network != 0 {
            request
                .network_world
                .ok_or(PlayerPrepareError::NetworkMissionSelection { network })?
        } else {
            OFFLINE_MISSION_SELECTION
        }
    };
    request.mission.reset(MissionResetRequest {
        selection,
        elapsed_policy: MissionElapsedPolicy::Clear,
    }); // 004874B3 -> complete0048EBA0.
    *request.phase = PlayerPreparePhase::Complete;
    Ok(PlayerPrepareOutcome {
        constructed,
        selection,
    })
}

#[cfg(test)]
mod tests;
