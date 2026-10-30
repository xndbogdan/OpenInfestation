# Infestation shared object lighting

This document owns the Flag0 light pass `00479500` and its admitted virtual
`+28` callbacks. The same
[`ObjectArena::run_flag0_light_pass`](../../crates/inf-game/src/object_runtime/light_pass.rs)
owner runs in the briefing pane and gameplay world, over the actual arena and
process [`WorldLightGrid`](../../crates/inf-game/src/world_lighting.rs).
Construction and embedded-node lifetime remain in
[object runtime](OBJECT_RUNTIME.md); grid arithmetic and terrain reads remain
in [terrain runtime](TERRAIN_RUNTIME.md#shared-production-pixels).
No campaign selection or placement record selects this pass's behavior.

## Native authority and transfer order

The complete native body is `00479500..0047956B`. Its receiver is manager
`M = G + 8`, where `G = [0055F160]`. Manager `+08/+0C`, therefore game
`+10/+14`, hold the current and alternate Flag0 headers. Both native callers
establish this receiver: briefing at `0040C1DD`, gameplay at `00488144`.
The original pass and complete callback bodies are authenticated by
[source guards](../../crates/inf-game/src/object_runtime/light_pass/tests.rs).
These are static original-byte checks, not evidence that a retail frame
executed every callback.

The pass reads actual settings DWORD `+80` at entry (`00479508`). Zero returns
before reading or moving any node, without exchanging headers. When enabled:

1. Read the fresh first node of `Flag0Primary`.
2. Transfer that object's same embedded `Flag0` node to the tail of
   `Flag0Secondary`, before reading its current flags or callback.
3. Test current flag `08000000`; only a set bit invokes actual virtual `+28`.
4. Restart at the fresh primary head. Only normal exhaustion exchanges the
   two header owners (`0047955C..00479565`).

The transfer preserves bit 0, dirty state and all other embedded memberships.
`set_flag0` is a different owner: it changes the flag and can move the node to
Parking. This pass must not call it in place of a transfer. Ordinary order is
preserved by tail insertion. An enabled empty pass still exchanges headers;
a disabled pass does not.

Callbacks can publish additional primary nodes. An initial copied work vector,
predecoded position list or placement-order walk would hide those mutations.
The admitted callbacks currently do not allocate, but the shared owner retains
the fresh-head loop. Its typed live-node domain authenticates object, node kind
and physical-list membership before transfer. Raw NULL/replaced payloads,
native pointer faults and arbitrary vtable replacement remain outside that
domain; logical custody is not a claim of heap/address equivalence.

## Current callback ownership

Dispatch follows the object's actual `ObjectClass` and `ActorExtension`.
Constructor selectors, resource arguments, mutable resource words and actor
`+B4` classification are separate fields; none substitutes for the vtable.
Unrepresented constructors are not mapped to a common callback merely because
their storage resembles a represented class.

| Actual class or reached branch | Native owner and admitted behavior |
| --- | --- |
| Base, visual, rotating visual, route node, map annotation and selector 86; shared/16/88/90/93 actors and admitted derived actors other than selector 2 | `00401350..0040136B` clears `08000000` through the common changed-low-nine dirty predicate. A high-only clear leaves the existing dirty byte unchanged. No position, lighting source or deposit policy is read. |
| Selector 76, the [flamethrower](SELECTOR76_RUNTIME.md#light) | `00432490` reaches `0045B1A0` at its X/Z with red `+70 * 127 >> 16`, green and blue `+70 * 80 >> 16`, and radius `(+70 >> 6) + 256`. |
| Actor selectors 25 and 28, the [missiles](SELECTOR25_RUNTIME.md#light-draw-messages-and-contacts) | `00457EE0` reads the raw H12 cell height and, within 1500 above it, reaches `0045B1A0` with red `(1500 - height) * 20 / 1500`, green and blue `(5 * height - 7500) / 1500` (wrapping below zero) and radius `height * 500 / 750`, at least 256. |
| Actor selectors 26, 27 and 38, the [grenade](SELECTOR26_RUNTIME.md#light), the [charge](SELECTOR27_RUNTIME.md) and the mine | `00456560` reads the raw H12 cell height under its X/Z and, within 1500 above it, reaches `0045B1A0` with grey `(1500 - height) * 20 / 1500` and radius `height * 500 / 750`, at least 256. The pass lends it the world's terrain; without it the callback stops at `GrenadeTerrain`. |
| TerrainLight 19 | `004481A0..004481B9` consumes packed color at `+44`, unsigned WORD radius `+48`, and signed WORD X/Z at `+32/+36`, then reaches `0045B1A0`. Only the low three packed color bytes are signed RGB; padding is not alpha. It does not clear `08000000`. |
| The [rotating terrain light](OBJECT_RUNTIME.md#selector-54-rotating-terrain-light) 54 | `004649C0` is the same code reading `+6C` and `+70`. |
| Actor selector 30 | `0048BD40..0048BD82` treats DWORD `+178` as unsigned, stores 256 when greater than 256, then rereads the zero and byte `+18E` gates. A reached deposit uses radius `+178 << 3`, the low three signed bytes of DWORD `+174`, and signed WORD X/Z. No high-flag clear belongs to this callback. |
| Actor selector 2, [the vehicle](VEHICLE_DRIVE_RUNTIME.md#the-vehicles-lights-00420080) | `00420080` lights up to three deposits in order: the headlamp ray's hit with the headlights on (`+208`), the rear glow in the buggy or the truck unless the camera is in first person, and a pulse under flag `00040000`. With none of the three it returns without lower services (no position, basis, terrain, camera, delta or timer storage). |

The selector-30 clamp precedes both disabled gates and later required reads.
It survives a missing position or deposit error, along with the already moved
node and earlier grid writes. TerrainLight 19 likewise reads its represented
light storage before requiring position. `position_words: Option<[u16; 3]>`
supplies a complete known-vector domain; it does not reproduce native per-axis
uninitialized-memory faults. No origin or missing light values are invented.

The pass deposits the vehicle's lights it reaches, in native order, before
any of its boundaries:

| Arm | Port boundary |
| --- | --- |
| Byte `+208` nonzero | `Position` without the hull's position, `HeadlampTerrain` without a lent H12. |
| DWORD `+1E8` equals 1 or 2 | `VehicleCamera` when no M+74 actor resolves, where `00478E00` would build the camera before reading its `+188`. A camera handle naming an actor with no camera (its `+60` NULL) lights nothing, as natively. |
| Flag `00040000` set | None: the ECM pulse deposits after `+588` gains the world's delta (`0055F0E0`); `Position` when the hull has none, after that store. |

## Policy and process-grid custody

`Flag0LightPassRequest` carries the actual mutable grid, a narrowly scoped
`ObjectLightPolicySource` and an explicit failure policy. Its
`dynamic_lighting()` getter serves the manager entry read. Its
`terrain_light_policy()` getter is reached only after the callback fields and
gates have produced an actual `0045B1A0` request. The deposit reads settings
again even when that read disables the deposit.

The production
[`SettingsObjectLightPolicySource`](../../crates/inf-game/src/world_lighting.rs)
borrows actual immutable `FrontendSettings`, reads `words()[80/4]` at each
reached phase, and supplies current terrain spacing and the admitted tint
policy from their distinct owners. A captured entry Boolean cannot substitute
for the later getter. The immutable loan bounds intervening alias mutation;
it does not establish arbitrary synchronous mutation parity. A future mutable
callback domain must revisit that bound. Optional world `+5C3` tinting retains
its separate source gate; it is not selected by object class.

The pass neither clears cells nor publishes anchors, selects a camera, resets
renderer state or consumes RNG. Each admitted deposit uses the existing
`0045B1A0` grid arithmetic, signed RGB saturation and actual settings gate.
`Flag0LightPassOutcome` reports visited nodes, reached callbacks, reached
deposit calls, native grid sample/store counts and classified omissions. Those
port diagnostics are not original callback/allocation counters; a disabled
deposit call can count as reached without writing cells.

## UnownedObjectLightCallbackApproximation

`Flag0LightFailurePolicy::Stop` preserves strict supplied-owner completion.
Missing represented light storage or XYZ and the three active vehicle phases
return `UnownedCallback { object, reason }` at their reached boundary. Any
earlier transfer, flag clear, selector-30 clamp or grid deposit remains. The
pass stops without exchanging header owners; it neither rolls back nor retries.

Interactive briefing and gameplay explicitly select
`UnownedObjectLightCallbackApproximation`. It records the current `ObjectId`
and `ObjectLightCallbackBoundary`, skips only that callback's unowned remainder,
and continues from the fresh primary head. The reasons include
`TerrainLightStorage`, `Position`, `HeadlampTerrain` and `VehicleCamera`. It
supplies no fake values or successful active service. Header exchange still
requires the enabled pass to drain; omissions are logged by the actual
consumer.

Object lookup, embedded-node custody and native grid IDIV failures are hard
errors under both policies. They preserve their earlier prefix and stop before
header exchange. The approximation therefore cannot silently turn corruption
or invalid native arithmetic into a completed pass.

This replaceable policy is not native callback execution or retail acceptance.
Follow-up ownership belongs here: supply actual retained light/position storage
where it is unwritten, then own selector-2 beam, camera and timer continuations
over their real world/process services. A passing inactive branch or a rendered
terrain patch does not close an active branch.

## Shared consumers and remaining draw boundary

| Consumer | Native order retained by the light insertion |
| --- | --- |
| Briefing terrain pane `0040BB40` | Ground `0044AA10` at `0040C1D2`, object light `00479500` at `0040C1E0`, shared object draw `00479570` at `0040C1EE`, flush `0040C1FC`, reset `0040C20A`, separate depth clear `0040C21C`, then camera static `0040C23B`. |
| Gameplay world `00488020` | Active particles `00464C80` at `00488121`, object light `00479500` at `00488147`, objects `00479570` at `0048816E`, pending particles at `00488196`, transfer `0048819D`, then terrain `0044AA10` at `00488359`. |

Briefing clears process cells at `0040BE6F` before its pane-resource lookups and
ground traversal, while retaining anchors. Its object lights run after ground,
so their new deposits are not retroactive light for that already submitted
ground. The pass stays there even when moving it earlier would change the
image. Closed or camera-disabled panes retain their existing cells and anchors
and do not invoke this world branch. Gameplay clears and publishes its anchor
before particles and this pass; its later particles and terrain share the
resulting grid. These distinct caller policies generalize over loaded worlds.

The scoped briefing draw borrows the actual mutable arena; gameplay retains its
actual `WorldSession` arena. Both use current settings loans from the platform
draw owner. Both now invoke the
[shared object draw manager](OBJECT_DRAW_RUNTIME.md) at the native position
after lighting. Fresh terrain/culling, actual-class callbacks, common packet
lighting and distinct late execution use the current arena and process clock.
The light pass itself does not substitute for those independent phases; its
active callback omissions and matched lighting acceptance remain open.

[Focused pass tests](../../crates/inf-game/src/object_runtime/light_pass/tests.rs)
cover actual allocated classes and embedded memberships, disabled entry,
common high-bit/dirty custody, signed light operands, selector-30 clamp order,
vehicle gate order, separate entry/deposit reads, callback omissions and strict
stopped prefixes. Caller verification must keep briefing ground-before-light and
gameplay active-particles-before-light distinct. These supplied-state and
original-byte checks establish bounded owners; complete intro/briefing
composition and matched retail lighting remain open.
