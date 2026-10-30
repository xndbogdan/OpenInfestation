//! Retained allocation policy for constructor-unwritten actor carries.
//!
//! The strict default preserves unknown storage. Production may select the
//! named approximation once before its first actual actor allocation. This
//! service models neither Windows heap reuse nor a native constructor write.

use thiserror::Error;

use crate::object_runtime::ObjectArena;

/// An arena-wide source for explicitly represented constructor-unwritten carries.
/// Native constructor stores and all other unknown fields remain independent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorAllocationPolicy {
    Unowned,
    /// The original four represented carries start at zero. This replaceable port
    /// approximation supplies family WORDs +220/+230 and Eye WORD +19C and
    /// DWORD +1A0 only; it is not acceptance of original allocator history.
    FreshHeapZeroCarryApproximation,
    /// The old four-carry domain plus derived DWORDs +14C and +150 and fresh
    /// +218 helper DWORD +00, plus selector66's previous-position WORDs
    /// +144/+146/+148, plus family DWORDs +18C (the descriptor period that
    /// Herding 00427AA0 reads first) and +19C (004294A0's descriptor
    /// reselection countdown). Infest01 shows both zero for two intro
    /// Selector35 allocations and heap string residue for a third, so zero
    /// here is not native parity. Its two spawned Selector12s read +150 zero
    /// at their first Attack update read 004298CE, before any tail store.
    /// Selector11's own +19C aim timer, Selector30's held handle +158 and
    /// Selector35's route distance +260 follow the same zero, as does the
    /// +08 point of Selector34's Attack entry clone, Selector74's zone
    /// timer +13C, the Pursuit Ship's bank direction +13C and the eyes'
    /// (selector 79) side WORD +1BC and aim timer +1C0, the spawner's
    /// (selector 81) member list +58, the mode-4 walker helper's stride
    /// phase +34 (Exxodus's TL717, the Nemian), the vehicle's free-look
    /// eye position +1CA..+1CE and the Siren's silence BYTE +16D (world 8's
    /// retail Siren read 0 there, with the lab's heap and a fresh one).
    /// Helper +04, selector66's saved flags +138 and all history rows remain unknown.
    /// This replaceable backing approximation does not model native heap reuse.
    FreshHeapZeroExtendedCarryApproximation,
}

/// Private addresses admitted by the retained DWORD backing policy. Calling a
/// constructor with one of these cannot silently seed another adjacent field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::object_runtime) enum ActorAllocationDwordCarry {
    Eye1a0,
    Derived14c,
    /// 00425E60 skips +150; Attack update 004298CE can read it before the
    /// class tail first stores the target status.
    Derived150,
    FamilyHelper00,
    /// 00428650 never writes +18C; Herding entry 00427A70 leaves it for its update.
    Family18c,
    /// 00428650 never writes +19C; 004294A0 first reads it as a signed timer.
    Family19c,
    /// 004014F0 never writes Selector11 +19C; 004019E0's aim adds the delta
    /// to it first. Infest01's eight intro Selector11s (refs 1Dh..24h) read
    /// zero there; three later in the trace read residue (0B8Ch, 21h,
    /// 003B003Ch).
    Selector11Aim19c,
    /// 0040D800 never writes Selector30 +158; 0048B3F0 resolves it as a
    /// handle every update. Infest01's three gates hold residue (3F4B0000h,
    /// 3ECA0000h, 12h) that never resolves; zero never resolves either.
    Selector30Handle158,
    /// Selector35 construction never writes +260. Tail 0043EC7E writes it
    /// only while clearance byte +250 is clear, so route update 0043F617 can
    /// read it first: the port's ref60 does at 33 ms steps, and a spawned
    /// ship too at 16 ms, while at 125 ms the tail writes first as in
    /// Infest01. There ref60's +260 stayed zero, never written, until its
    /// first tail write at clock 2331; ref61 held string residue 726F4D00.
    Selector35Route260,
    /// 004128C0 never writes the Pursuit Ship's BYTE +13C; 00412A10 first
    /// reads it to choose the bank's direction (its low byte is the carry).
    Selector61Bank13c,
    /// Case 0x4A never writes Selector74 +13C; 0042D570 first adds the
    /// delta to it and 0042D7D0 turns the draw's points by its low WORD.
    /// Infest03 held 0 there and Infest04 0x3B800000, whose low WORD is 0.
    Selector74Timer13c,
    /// 00408740 never writes the eye's +1C0; 00408AB0 resets it with +1BC
    /// when its target changes, and adds the delta to it while aimed.
    Selector79Aimed1c0,
    /// 00409360 allocates the spawner's member list (+44 DWORDs) and never
    /// writes it; 00409390 resolves each entry as a handle.
    Selector81Members58,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
pub enum ActorAllocationPolicyError {
    #[error("actor allocation policy was already selected: {policy:?}")]
    AlreadySelected { policy: ActorAllocationPolicy },
    #[error("actor allocation has already started with policy: {policy:?}")]
    AlreadyStarted { policy: ActorAllocationPolicy },
}

/// Retained service state, separate from current live object count. Destroying
/// every object cannot reopen selection or silently change later allocations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::object_runtime) enum ActorAllocationService {
    #[default]
    Unselected,
    Selected(ActorAllocationPolicy),
    Started(ActorAllocationPolicy),
}

impl ActorAllocationService {
    /// Additional named backing scope for selector66's untouched +144/146/148.
    /// It can change the first smoke branch and does not establish heap parity.
    pub(super) fn initial_selector66_position(self) -> [Option<u16>; 3] {
        match self.policy() {
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation => [Some(0); 3],
            _ => [None; 3],
        }
    }
    /// Selector34 Attack entry 00433FB0 stores a new type-2 clone in +148.
    /// Its explicit-point constructor 004828D0 never writes +08..+0D, and the
    /// same tick's update reads that Y at 00433F42 before the tail's refresh.
    /// Infest01's four entry clones held Y 0 three times and weapon-node
    /// residue BEB6 once; their X and Z were nonzero residue.
    pub(super) fn selector34_entry_clone_focus(self) -> Option<[u16; 3]> {
        match self.policy() {
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation => Some([0; 3]),
            _ => None,
        }
    }
    fn select(&mut self, policy: ActorAllocationPolicy) -> Result<(), ActorAllocationPolicyError> {
        match *self {
            Self::Unselected => {
                *self = Self::Selected(policy);
                Ok(())
            }
            Self::Selected(policy) => Err(ActorAllocationPolicyError::AlreadySelected { policy }),
            Self::Started(policy) => Err(ActorAllocationPolicyError::AlreadyStarted { policy }),
        }
    }

    /// Reached immediately before the common actor's actual object allocation,
    /// after registry and terrain preflights. A later failure keeps this state.
    pub(super) fn begin(&mut self) {
        *self = Self::Started(self.policy());
    }

    fn policy(self) -> ActorAllocationPolicy {
        match self {
            Self::Unselected => ActorAllocationPolicy::Unowned,
            Self::Selected(policy) | Self::Started(policy) => policy,
        }
    }

    /// The extended policy's WORD holes: the eyes' (selector 79) +1BC, the
    /// mode-4 walker helper's stride phase +34 and the vehicle's free-look
    /// eye position +1CA..+1CE; the Siren's BYTE +16D takes its low byte.
    pub(in crate::object_runtime) fn initial_extended_word(self) -> Option<u16> {
        match self.policy() {
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation => Some(0),
            ActorAllocationPolicy::Unowned
            | ActorAllocationPolicy::FreshHeapZeroCarryApproximation => None,
        }
    }

    /// Used only at the represented +220, +230 and Eye +19C construction holes.
    pub(super) fn initial_word(self) -> Option<u16> {
        match self.policy() {
            ActorAllocationPolicy::Unowned => None,
            ActorAllocationPolicy::FreshHeapZeroCarryApproximation
            | ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation => Some(0),
        }
    }

    /// Read only by the admitted construction/post-init sites. Never a read-site
    /// fallback or a substitute for a later native field writer.
    pub(in crate::object_runtime) fn initial_dword(
        self,
        carry: ActorAllocationDwordCarry,
    ) -> Option<u32> {
        match (self.policy(), carry) {
            (ActorAllocationPolicy::Unowned, _) => None,
            (
                ActorAllocationPolicy::FreshHeapZeroCarryApproximation,
                ActorAllocationDwordCarry::Eye1a0,
            )
            | (ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation, _) => Some(0),
            (ActorAllocationPolicy::FreshHeapZeroCarryApproximation, _) => None,
        }
    }
}

impl ObjectArena {
    /// Select once before this retained arena's first actual actor allocation.
    /// Non-actor allocations and failed actor preflights do not start the
    /// service. Leaving this unselected preserves the strict unknown policy.
    pub fn select_actor_allocation_policy(
        &mut self,
        policy: ActorAllocationPolicy,
    ) -> Result<(), ActorAllocationPolicyError> {
        self.actor_allocation.select(policy)
    }
}

#[cfg(test)]
mod tests;
