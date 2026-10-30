//! 00469D60's successful-allocation construction flow, before and after FIFO
//! publication. This is independent of saved-player import and session phase.
//!
//! The actual input references and unpublished message/mask stay in one carrier.
//! Stopped source reads retain their prefix; they are not C++ exceptions. The
//! complete known, immutable process loan excludes an unavailable/mutating NULL
//! after publication. The public WorldPlayer is a projection of represented
//! fields, not an emulation of all 0x1A0 bytes or native allocator backing.
//! Published teardown owns FIFO unlink, constructor-known message destruction
//! and reverse input releases; stopped retirement keeps the same components.

use crate::input::InputRuntime;
use crate::object_runtime::grants::{
    set_live_grant, GrantSetTarget, LiveGrantContext, LiveGrantError,
};
use inf_formats::{GRANT_MASK_WORDS, PLAYER_INITIAL_GRANTS};
use thiserror::Error;

use super::input::{
    PlayerInputDescriptorSource, PlayerInputError, PlayerInputReferences, PlayerInputReleaseError,
};
use super::{
    ActorHandle, ObjectArena, PlayerControlBlock, PlayerId, PlayerMessageState, WorldPlayer,
};

mod destruction;
pub use destruction::{PlayerDestructionError, PlayerDestructionPhase};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerConstructionPhase {
    Allocated,
    Constructing,
    Stopped,
    Published,
}

/// Semantic allocation identity and actual owned constructor components. Keep
/// this carrier alive with the published player: its input references are not
/// duplicated in the list projection and are never automatically released.
#[must_use]
#[derive(Debug)]
pub struct WorldPlayerConstruction {
    arena: u64,
    player: PlayerId,
    phase: PlayerConstructionPhase,
    inputs: PlayerInputReferences,
    message: Option<PlayerMessageState>,
    grants: Option<[u32; GRANT_MASK_WORDS]>,
    // Fields needed to describe reached pre-input and post-grant custody,
    // which the existing published save/attachment projection does not retain.
    dword_48: Option<u32>,
    dword_4c: Option<u32>,
    dword_50: Option<u32>,
    byte_58: Option<u8>,
    handle_d8: Option<ActorHandle>,
    handle_dc: Option<ActorHandle>,
    input_release_attempted: bool,
    destruction_phase: Option<PlayerDestructionPhase>,
    retiring: Option<WorldPlayer>,
}

impl WorldPlayerConstruction {
    pub fn player(&self) -> PlayerId {
        self.player
    }

    pub fn phase(&self) -> PlayerConstructionPhase {
        self.phase
    }

    pub fn callback_context(&self) -> u32 {
        self.inputs.callback_context()
    }

    /// The player's digital input references +68..+84, when all are held.
    pub fn input_consumers(&self) -> Option<super::PlayerInputConsumers> {
        self.inputs.input_consumers()
    }

    /// The actual mask before publication, including writes before a failure.
    /// On publication the same value moves into the real list owner.
    pub fn unpublished_grants(&self) -> Option<&[u32; GRANT_MASK_WORDS]> {
        self.grants.as_ref()
    }

    pub fn unpublished_message(&self) -> Option<&PlayerMessageState> {
        self.message.as_ref()
    }

    /// The last completed deleting-destructor phase. Some also records that
    /// whole destruction was attempted and cannot silently restart.
    pub fn destruction_phase(&self) -> Option<PlayerDestructionPhase> {
        self.destruction_phase
    }

    /// Same unlinked record retained through a stopped or completed teardown.
    /// This readonly snapshot is not a published or live allocation pointer.
    pub fn retiring_player(&self) -> Option<&WorldPlayer> {
        self.retiring.as_ref()
    }

    /// Only the proved component release phase: reverse completed acquisitions.
    /// The caller owns any preceding player unlink/message destruction. This
    /// neither destroys a published player nor claims native SEH unwinding.
    pub fn release_input_references(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<(), PlayerInputReleaseError> {
        self.input_release_attempted = true;
        self.inputs.release(input)
    }
}

#[derive(Debug, Error)]
pub enum PlayerConstructionError<E> {
    #[error("player construction belongs to another object arena")]
    ForeignArena,
    #[error("player construction already attempted in phase {0:?}")]
    AlreadyAttempted(PlayerConstructionPhase),
    #[error(transparent)]
    Input(#[from] PlayerInputError<E>),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
    #[error("00469E80 requires the current process NULL 0054E894 after initial grants")]
    MissingProcessNull,
}

impl ObjectArena {
    /// Reserve one arena-qualified semantic identity for an explicitly supplied
    /// successful player allocation. This does not execute the allocator or
    /// publish a list node. callback_context is the caller's stable actual
    /// logical payload identity, unique for distinct simultaneously live players
    /// in this InputRuntime, never derived from this arena-local PlayerId.
    pub fn reserve_world_player_construction(
        &mut self,
        callback_context: u32,
    ) -> WorldPlayerConstruction {
        let player = PlayerId(self.players.next_id);
        self.players.next_id = self
            .players
            .next_id
            .checked_add(1)
            .expect("portable player identity exhausted");
        WorldPlayerConstruction {
            arena: self.identity,
            player,
            phase: PlayerConstructionPhase::Allocated,
            inputs: PlayerInputReferences::new(callback_context),
            message: None,
            grants: None,
            dword_48: None,
            dword_4c: None,
            dword_50: None,
            byte_58: None,
            handle_d8: None,
            handle_dc: None,
            input_release_attempted: false,
            destruction_phase: None,
            retiring: None,
        }
    }

    /// Execute 00469D60's represented construction against actual shared owners.
    /// Existing saved/imported add_world_player callers keep their own policy.
    /// Descriptor metadata must stay known/stable for the input lifetime; this
    /// inherits InputRuntime's untimed, successful-consumer domain. Current
    /// process/EXE/H0/CRT loans cannot be mutated by the reached lower phases.
    /// No process-NULL admission occurs before input or grant initialization.
    pub fn construct_world_player<S: PlayerInputDescriptorSource>(
        &mut self,
        construction: &mut WorldPlayerConstruction,
        input: &mut InputRuntime,
        descriptors: &mut S,
        context: &LiveGrantContext<'_>,
    ) -> Result<PlayerId, PlayerConstructionError<S::Error>> {
        if construction.arena != self.identity {
            return Err(PlayerConstructionError::ForeignArena);
        }
        if construction.phase != PlayerConstructionPhase::Allocated {
            return Err(PlayerConstructionError::AlreadyAttempted(
                construction.phase,
            ));
        }
        construction.phase = PlayerConstructionPhase::Constructing;
        let result = self.construct_player_components(construction, input, descriptors, context);
        if result.is_err() {
            construction.phase = PlayerConstructionPhase::Stopped;
        }
        result
    }

    fn construct_player_components<S: PlayerInputDescriptorSource>(
        &mut self,
        construction: &mut WorldPlayerConstruction,
        input: &mut InputRuntime,
        descriptors: &mut S,
        context: &LiveGrantContext<'_>,
    ) -> Result<PlayerId, PlayerConstructionError<S::Error>> {
        // 00469D84..00469D9D precede the first input wrapper.
        construction.dword_48 = Some((-1000_i32) as u32);
        construction.dword_4c = Some(0);
        construction.dword_50 = Some(0);
        construction.byte_58 = Some(1);
        construction.inputs.acquire(input, descriptors)?;

        // 00469E75 -> 00413140. The actual message owns suppression=1 and
        // 00475020 binds it before the six-word clear and twelve real Set calls.
        construction.message = Some(PlayerMessageState::new(self.identity, construction.player));
        construction.grants = Some([0; GRANT_MASK_WORDS]);
        for id in PLAYER_INITIAL_GRANTS {
            set_live_grant(
                &mut GrantSetTarget::ConstructingPlayer {
                    player: construction.player,
                    mask: construction
                        .grants
                        .as_mut()
                        .expect("constructed grant mask"),
                    message: construction.message.as_mut().expect("constructed message"),
                },
                id,
                context,
            )?;
        }

        // First reached NULL read is 00469E80, after complete initial grants.
        // Once known, this same immutable actual process loan proves the two
        // later reads cannot become unavailable. There are no intervening
        // external callbacks or mutable descriptor/process aliases in this domain.
        let process = context
            .process
            .filter(|process| process.process_null_handle.is_some())
            .ok_or(PlayerConstructionError::MissingProcessNull)?;
        construction.handle_d8 = process.process_null_handle;
        construction.handle_dc = process.process_null_handle; // 00469E8B

        let player = construction.player;
        // WorldPlayer projects completed typed fields. Its mandatory D4 and
        // command values can be materialized here only because their post-FIFO
        // suffix is known, infallible and callback-free. Actual suffix writes
        // below still occur after the real append; partial native byte/fault
        // observations at that boundary are not admitted by this projection.
        let live = WorldPlayer {
            id: player,
            message_88: construction.message.take().expect("constructed message"),
            grants_9c: construction.grants.take().expect("constructed grant mask"),
            handle_d4: process.process_null_handle.expect("immutable known NULL"),
            handle_d8: construction.handle_d8.take().expect("00469E80 completed"),
            // The carrier keeps its +DC copy after publication.
            handle_dc: construction.handle_dc.expect("00469E8B completed"),
            word_11c: 0,
            byte_58: construction.byte_58.take(),
            bits_100: [0; 6],
            dword_118: 0,
            word_11e: 0,
            dwords_120_128: [0; 3],
            dword_12c: 0,
            command_mask_130: [0; 3],
            bytes_13c_143: [0; 8],
            dword_144: 1,
            dword_148: 0,
            dword_14c: 8,
            byte_ed: 0,
            byte_ee: 0,
            dword_f0: 0,
            saved_e0: None,
            byte_ec: None,
            dword_e8: 0,
            controls: PlayerControlBlock::default(),
            dword_48: construction.dword_48.take(),
            dword_4c: construction.dword_4c.take(),
            dword_50: construction.dword_50.take(),
            dword_54: None,
            dword_f4: 0,
            dword_f8: 0,
            byte_fc: 0,
            dword_198: 0,
            dword_19c: 0,
            values_150: {
                let mut values = [0; 16];
                values[0] = -1;
                values
            },
            items_190: [0; 3],
        };
        // 00469F67..00469F7C: append the same identity/components after all
        // existing players, never making them temporarily public during grants.
        self.players.list.push(live);
        let live = self.players.list.last_mut().expect("appended player");
        live.handle_d4 = process.process_null_handle.expect("immutable known NULL"); // 00469F7E
                                                                                     // 00469F8E zeroes +04..+47; retain its named projection fields.
        live.controls = PlayerControlBlock::default();
        live.command_mask_130 = [0; 3]; // 00469F97..00469FA4
        construction.phase = PlayerConstructionPhase::Published;
        Ok(player)
    }
}

#[cfg(test)]
mod tests;
