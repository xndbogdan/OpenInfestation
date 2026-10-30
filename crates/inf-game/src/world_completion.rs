//! Native offline completion 00487560 -> 0048E820 -> 0046AF70.
//!
//! Gameplay Escape on current world22 and selector88 state14 both pass
//! `(next selection18, start ordinal1, null actor)`. The second argument is
//! stored as WORD-minus-one at mission +A4, the next world's start-pair index;
//! it is not decoded as a success boolean. Completion requests phase8 after
//! player checkpoint reset, and phase8's world22 head then requests phase3.

use thiserror::Error;

use crate::object_runtime::{
    ObjectArena, ObjectError, ObjectId, PlayerActorLookupContext, PlayerActorLookupError,
    PlayerActorLookupRequest, PlayerAttachError,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::session::SessionPhase;
use crate::world_mission::{WorldMission, WorldMissionError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldCompletionRequest {
    pub next_selection: u32,
    /// Native low WORD; subtract one with WORD wrapping for owner +A4.
    pub start_ordinal: u16,
    /// Null on the intro skip/completion path. A supplied actor must belong to
    /// a player according to 0046B090, or native disposes it and returns false.
    pub actor: Option<ObjectId>,
}

pub const INTRO_COMPLETION: WorldCompletionRequest = WorldCompletionRequest {
    next_selection: 18,
    start_ordinal: 1,
    actor: None,
};

#[derive(Clone, Copy, Debug)]
pub struct WorldCompletionInputs<'a> {
    pub network_active: bool,
    /// Read only after successful linked cleanup and an actor-bearing request.
    pub process: Option<&'a ParticleFactoryProcessState>,
}

/// 004800E0 walks the current linked-record list, saving each next pointer,
/// and invokes 0047FFA0 for entries whose flag byte +14 contains bit2.
/// That owner is separate from actor disposal/creation-listener lists.
pub trait WorldCompletionHost {
    type Error;
    fn flush_link_records(&mut self) -> Result<(), Self::Error>;
}

impl WorldCompletionHost for crate::process_sound::ProcessSoundRuntime {
    type Error = crate::process_sound::ProcessSoundRecordError;
    fn flush_link_records(&mut self) -> Result<(), Self::Error> {
        // Only the actual flag+14 bit2 one-shot domain is admitted by this
        // process owner. Existing PCM voices survive 004800E0 queue disposal.
        self.cleanup_one_shots()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldCompletionOutcome {
    NetworkIgnored,
    ActorRejected,
    Accepted { request_phase: SessionPhase },
}

#[derive(Debug, Error)]
pub enum WorldCompletionError<E> {
    #[error("world completion linked-record cleanup failed")]
    Cleanup(E),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    PlayerLookup(#[from] PlayerActorLookupError),
    #[error(transparent)]
    Mission(#[from] WorldMissionError),
    #[error("0048E820 reached 0046AF70 with no first player")]
    MissingPlayer,
}

/// Execute the recovered writes and expose the required phase submission.
/// Host failures retain prior reached effects; no captured result or duration
/// can advance a mission. Network returns before cleanup and every mutation.
pub fn complete_world<H: WorldCompletionHost>(
    mission: &mut WorldMission,
    arena: &mut ObjectArena,
    request: WorldCompletionRequest,
    inputs: WorldCompletionInputs<'_>,
    host: &mut H,
) -> Result<WorldCompletionOutcome, WorldCompletionError<H::Error>> {
    if inputs.network_active {
        return Ok(WorldCompletionOutcome::NetworkIgnored);
    }
    host.flush_link_records()
        .map_err(WorldCompletionError::Cleanup)?;
    let player = if let Some(actor) = request.actor {
        let handle = arena.actor(actor)?.handle;
        let Some(player) = arena.find_player_for_actor_handle(
            PlayerActorLookupRequest { handle },
            PlayerActorLookupContext {
                process: inputs.process,
            },
        )?
        else {
            arena.dispose(actor)?;
            return Ok(WorldCompletionOutcome::ActorRejected);
        };
        Some(player)
    } else {
        None
    };
    let current = mission.current_world_a0;
    let status = mission
        .save
        .owner_3c
        .get_mut(current as usize)
        .ok_or(WorldMissionError::WorldOutOfRange(current))?;
    *status |= 1;
    mission.save.mission_selection = request.next_selection;
    mission.save.owner_a4 = request.start_ordinal.wrapping_sub(1);
    // The fallback lookup occurs after the mission writes, exactly as native.
    let player = player
        .or_else(|| arena.first_player())
        .ok_or(WorldCompletionError::MissingPlayer)?;
    arena.clear_player_completion_checkpoint(player)?;
    Ok(WorldCompletionOutcome::Accepted {
        request_phase: SessionPhase::Phase8,
    })
}

#[cfg(test)]
#[path = "world_completion_tests.rs"]
mod tests;
