//! Fresh-heap allocation-WORD policy for world loading (`0048DC30`).
//!
//! Retail constructors read three `u16` WORDs at allocation `+32/+34/+36`
//! before placement context initializes position. Those bytes are heap
//! leftovers from prior allocations (boot, menu, frontend, load), not
//! authored data. The port does not replicate the Windows heap history, so
//! exact leftover values are unowned.
//!
//! ## Census evidence (retail corpus only, no new capture)
//!
//! - `allocation_word_outcomes_across_the_triple_space` sweeps the classifier
//!   (`classify_actor_terrain`) and the native wave-height function over owned
//!   retail inputs (terrain, plane, flags, sine, clock 245):
//!   - At the recorded cell `(0,0)`, the joint reproducing band covers
//!     `32850/65536` y values; mismatches are exactly signed-negative heights
//!     below about `-82` (far above the plane, flipping to `Set20000000`).
//!   - At the recorded height `y=2`, every sampled map cell (`16384/16384`)
//!     reproduces the recorded outcomes (`Set40000000`, wave bit clear).
//!   - All 30 placement endpoints retain `flags & 0x60000000 == 0x40000000`,
//!     which is also the `H12/0` miss-branch outcome.
//!
//! ## Policy (explicit labeled approximation, not a gate)
//!
//! Use zero-fill `[0, 0, 0]` for every construction's pre-context WORDs when
//! real heap history is unavailable. Zero sits deep inside the proven
//! reproducing band for both outcome dimensions above, and matches the
//! observed fossil distribution (virgin-zero fresh subsegments, usually-zero
//! LIFO reuse, occasionally small-magnitude payloads).
//!
//! This is an explicit approximation: it does not claim to reproduce exact
//! retail heap leftovers, and it does not replace allocator-history ownership.
//! Allocator-history modeling (which freed block each construction receives,
//! replaying boot/menu/frontend/load order) remains a follow-up. Where a
//! subsystem is unknown, fail closed per-feature (skip the actor, log it),
//! not per-phase.
//!
//! ## Retained scalar allocation carries
//!
//! `ObjectArena::select_actor_allocation_policy` is a separate retained source
//! policy for constructor-unwritten carries, sampled at their actual allocation
//! sites. Strict/unselected arenas preserve unknown storage. The old
//! `FreshHeapZeroCarryApproximation` remains exactly the four-field mode: family
//! WORD +220, inherited water WORD +230, Eye WORD +19C and Eye DWORD +1A0.
//! `WorldLoadOwner::unprepared` instead selects the replaceable
//! `FreshHeapZeroExtendedCarryApproximation` once before its first actor: the
//! same four carries plus derived DWORD +14C and fresh family-helper DWORD +00
//! start at zero. Helper +00 is supplied only when native post-init reaches a
//! fresh +218 allocation; helper DWORD +04 and all history rows stay unknown.
//! Family DWORD +19C, 004294A0's descriptor reselection countdown, also starts
//! at zero (user decision 2026-10-06). The Infest01 timer receipt bounds this:
//! intro Selector35 refs 50h/60h read zero, but ref61 reads heap residue
//! 00746665 (the `CPU Left` string suffix), so retail delays ref61's first
//! selection by about 7.5 s while this port selects on its first tick after
//! the 1.5 s `+144` spawn delay. Spawned Selector35 08DD0001 also reads
//! residue 00020DED in the tick that constructs it, so retail picks its first
//! descriptor one tick later than this port. Family DWORD +18C, the descriptor
//! period that Herding update 00427AA0 reads when Herding is an actor's first
//! descriptor, follows the same labeled zero. Its Infest01 bounds match: refs
//! 50h/60h hold zero, ref61 holds string residue 50430070, and neither ref60
//! (whose Roam entry writes it first) nor ref61 reads that residue in the
//! intro. Inherited water WORD +230 is 00429D60's breath counter. In
//! Infest01, Selector36 placement 4Eh holds heap residue B70C there, still far
//! above the 4000 drowning threshold when it dives at clock 706. Retail
//! therefore drowns it at once, 3000 to 1536 health over 24 ticks, while zero
//! needs 33 submerged ticks and leaves it at 3000. At the sampled ticks, no
//! other intro actor's health depends on +230.
//! Derived DWORD +150, the target status that Attack update 004298CE can read
//! before the class tail first stores it, follows the same labeled zero. In
//! Infest01 both spawned Selector12s hold zero at that first read.
//! Selector11 DWORD +19C, the aim time 004019E0 accumulates and a targetless
//! update tests before any store, and Selector30 DWORD +158, the held handle
//! 0048B3F0 resolves every update, follow it too. All eight intro turrets
//! read +19C as zero first; the first world's three gates hold residue
//! (12, 3ECA0000, 3F4B0000) that never resolves, and zero never resolves.
//! Selector35 DWORD +260, the route distance that tail 0043EC7E writes only
//! while the ship is above its clearance, follows it as well. Route update
//! 0043F617 reads it before any tail write when the port runs at 33 or 16 ms
//! steps. Infest01's ref60 held zero there; ref61 held string residue
//! 726F4D00, which no native read reached. Selector34's Attack entry clone,
//! whose 004828D0 constructor leaves +08..+0D unwritten, starts zero there
//! too: the same tick's update can read its Y at 00433F42 before any
//! refresh, as the port does at 16 ms steps. Infest01's four entry clones
//! held Y 0 three times and residue BEB6 once. Selector74's zone timer
//! +13C, which case 0x4A never writes and 0042D570 first adds to, starts
//! zero as well: Infest03 held 0 there and Infest04 a float whose low WORD,
//! all the draw reads, is 0.
//! The eyes' (selector 79) side WORD +1BC and aim timer +1C0, which 00408740
//! never writes, start zero too; 00408AB0 resets both when the leader's
//! target changes, and no recording reaches world 19 to show their residue.
//! The spawner's (selector 81) member list, the +44 DWORDs 00409360
//! allocates and never writes, starts zero too: 00409390 resolves each as a
//! handle, and zero never resolves.
//! The mode-4 walker helper's stride phase WORD +34 and its legs' unwritten
//! bytes (00446BC0's fresh 0x74-byte blocks), which the walker tick 004476D0
//! and 00445CD0 read before any write, start zero too: held unknown,
//! Exxodus's TL717 and world 1's Nemian stopped at their first update.
//! The vehicle's free-look eye position +1CA..+1CE, which 00421DC0's first
//! tick compares with the vehicle's before any write, starts zero too: the
//! vehicle then lies more than five units off and its position is taken,
//! as for any residue but one within five units of it.
//!
//! Native425E60 skips14C and482FF0 writes only helper08. CRT small-block bins,
//! split/reuse/free metadata and HeapAlloc(flags=0) do not establish these initial
//! bytes. Accepted endpoints and the later14C movement writer establish only
//! their observed spans, not a generic initial distance or helper header. This
//! selected backing has no robustness proof from the XYZ census above and
//! supplies no captured values, native allocator fidelity or retail acceptance.
//!
//! The service retains Unselected/Selected/Started custody in the actual arena.
//! Registry and terrain failures before allocation leave selection available;
//! reached allocations lock it even if a later handle failure retires the new
//! object. Emptying the arena or moving it through a world reload cannot reopen
//! selection. Native constructor stores still win, including Selector34/35's
//! distinct DWORD +230 = 500. Other unwritten holes stay unknown; subsequent
//! callback writes remain retained and are never reset by drawing or allocation.
//!
//! The owning implementation is `object_runtime/actor/allocation.rs`; its
//! private typed DWORD carry request and constructor matrix follow native
//! inheritance, not mission/model/slot IDs. Both genuine post-init helper gates
//! preserve an existing helper. Attack's actual mismatch writer initializes04;
//! equality still requires its independent timer. Reached geometry writers
//! replace14C; neither later allocations nor reads restore modeled zero.
//! Faithful allocator backing/reuse, shared-bin/free-list history and
//! boot/menu/load composition remain the replaceable policy's follow-up owner.
//!
//! See `BRIEFING_RUNTIME.md` § Session LoadWorld execution boundary for the
//! full consumer audit. The vehicle wave bit (`byte_209`) is the one YES
//! outcome-dependent bit; zero-fill reproduces the recorded `0` there.

/// Fresh-heap substitute triple: virgin-zero pages, the common LIFO-reuse
/// value, and the center of the classifier's proven reproducing band.
pub const FRESH_ALLOCATION_WORDS: [u16; 3] = [0, 0, 0];

/// Returns the fresh-heap triple for one construction.
///
/// Currently always zero-fill. The function exists (rather than inlining the
/// constant) so future allocator-history work can replace the policy in one
/// place without touching every loader call site.
pub fn fresh_allocation_words() -> [u16; 3] {
    FRESH_ALLOCATION_WORDS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_triple_is_zero_fill() {
        assert_eq!(fresh_allocation_words(), [0, 0, 0]);
        assert_eq!(FRESH_ALLOCATION_WORDS, [0, 0, 0]);
    }

    #[test]
    fn fresh_triple_sits_inside_the_proven_y_band() {
        // The robustness probe pins the mismatch range to
        // `Some(32768)..=Some(65454)` (signed `-32768..=-82`). Zero (y=0)
        // is far from that range, inside the joint reproducing band.
        let y = FRESH_ALLOCATION_WORDS[1];
        assert!(
            y < 32768 || y > 65454,
            "fresh y={y} must sit outside the probe's mismatch range"
        );
    }
}
