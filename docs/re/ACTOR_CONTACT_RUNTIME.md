# Shared actor terrain contact

This owner covers complete `0047A2F0` terrain-normal construction and
`00426270` contact against terrain/water. The
[class-specific movement tails](ACTOR_MOVEMENT_TAILS.md) consume these services.
[Furniture collision](FURNITURE_COLLISION.md) owns the separate avoidance query
and its auxiliary model-contact response; a terrain contact return supplies
no furniture collision result. Level or placement IDs select neither path.

## Resource and process custody

[contact.rs](../../crates/inf-game/src/object_runtime/actor/derived/contact.rs)
uses one `ActorContactRequest` carrying actual table, native step, executable,
environment and explicit caller-address policy. Independent terrain and raw
H14 hosts retain their distinct resource loans. The production
[H14 host](../../crates/inf-game/src/world_actor_contact.rs) requires current
handler14/slot0, validates the existing canonical world header and borrows its
source span. It never substitutes a copied flags byte or an absent resource.

The native animation clock is read before initial H12 acquisition. That captured
H12 survives the subsequent actor BYTE160/XZ/ground reads and lazy H14
acquisition. On the water arm, plane base comes from captured H12 DWORD0;
the shift comes from the existing paired prepared-terrain policy. Fresh X/Z
follows H14 acquisition, then actual header BYTE4. Flag40 and plane above ground
select the original three sine waves. Their wrapped arithmetic, signed WORD
truncations and ground clamp remain explicit. A water surface above ground
copies the actual retained process-up vector. Otherwise `0047A2F0` acquires
its own current H12. Missing services preserve preceding writes.

`0047A2F0` captures H12 before actor coordinates. Four current height samples
use WORD-wrapped X/Z plus/minus64. Native normalization retains f32 spills and
the cross-product stores Z, X, Y in that order. Intrinsic normal decoding is
independent of presentation basis, mirroring and renderer state.

## Scalar and packet order

Size WORD28 is **unsigned** at the native IDIV. Actor Y and table WORDs are
signed. The initial separation projected onto normalY is truncated through
the existing CRT arithmetic owner, shifted by12 and divided by size.
`DWORD138` then receives wrapped `ratio * 100 SAR12`. This scalar is unrelated
to descriptor virtual-slot138; constructor `00425E60` leaves the field unwritten,
so the portable constructor retains `None` until the actual store.

Normal axes, impulse WORD0, alignment DWORD4 and motor DWORD8 each retain their
reached publication. Native packet padding WORD2 is untouched. Contact thresholds
and impulse use the actual signed WORD table at actor+25C, with fresh repeated
reads in native order. Required backing/trap failures preserve scalar and packet
prefixes. A zero native step still executes contact/scalar/motor work while
skipping the source-guarded positional correction and impulse addition.

The lower contact arm may correct Y and bounce a negative vertical velocity
through signed division by4. Water can reacquire H12 and recompute ground contact
without rewriting the original scalar138. Positive projected velocity uses the
native denominator `(dot SAR8) + 0x1000`; negative projected velocity has its
separate size-scaled impulse branch. The final motor ratio uses table WORD8/10.

## Native stack comparison domain

The water/no-recompute arm at `004265A6` compares the retained native stack-buffer
address against a signed WORD threshold. `ActorContactStackPolicy` represents
the recovered caller domain explicitly; it never supplies a numeric Rust
address, zero or an assumed scalar ratio.

`Bf09Class36Windows8PlusStack` admits only original BF09, its non-LAA PE32 image,
and actual class36 `00481C33/00481D1D` stack-buffer caller under Windows8+ address
bounds. Such a valid address lies at least at10000h and below80000000h, therefore
the native signed comparison takes JGE for every signed WORD threshold.
Microsoft's [earlier explanation](https://devblogs.microsoft.com/oldnewthing/20130925-00/?p=3123)
names Windows7 enforcement; its [later explanation](https://devblogs.microsoft.com/oldnewthing/20170420-00/?p=96005)
states that pre-Windows8 applications could unreserve the low64KB. The portable
policy conservatively admits Windows8+ only. Windows7, older systems, custom
low-address domains and other callers retain `RequiredNativeAddress` if reached.
This policy proves one branch, not arbitrary native pointer values or all callers.

## Verification bounds

The [contact tests](../../crates/inf-game/src/object_runtime/actor/derived/contact/tests.rs)
exercise required services, captured versus fresh H12, normal signs, unsigned
size, scalar/packet error prefixes, real water inputs and lazy stack policy.
[Original-byte guards](../../crates/inf-game/src/object_runtime/actor/derived/contact/native_guards.rs)
pin complete consumers, original tables, final virtual slots and non-LAA status.
These supplied intrinsic cases do not establish a loaded movement sequence,
active furniture contact, persistent audio or complete retail intro acceptance.
