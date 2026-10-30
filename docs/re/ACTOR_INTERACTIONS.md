# Infestation canonical actor interactions

This authority owns canonical actor virtual `+50`/`+54`, the numeric component
`+CC` (selector 30's `0048BE30` and the staffed classes' `0040E090`), and void
interaction notification `+9C`, including
selector-35's `0043F860` deadline write. These shared callbacks use the actual
represented actor hierarchy. Mutable classification `+B4`, a scene, placement,
model or distance does not select a callback body.

[interactions.rs](../../crates/inf-game/src/object_runtime/actor/interactions.rs)
provides the actor-bearing requests. Component queries use one
`ActorComponentQueryRequest` with explicit `Self50`/`Interaction54` modes and
return an actual optional `ObjectId`. `ActorInteractionRelationRequest`
retains component/source identities and returns a full numeric DWORD.
`ActorInteractionNotificationRequest` retains candidate/source/distance-squared
arguments and returns unit. Its narrow context borrows the existing
`AnimationClock`; it installs no separate clock state.

## Messages (+98)

Actors send each other `+98(sender, distance², code, argument)`.
`0047DD80` sends one, from the sender to a target, skipping a target beyond
a nonzero range. `0047DD20` sends one to every actor near the sender
through `0047DAF0`'s radius arm, measured from the sender's live position.
`0047DBF0` sends one to every actor of one side (`+B4`), measured from the
position the sender had when it began; radius 0 reaches the whole side.
Both walks compare signed WORD deltas' wrapping squares unsigned with the
radius squared, read the next node after each receiver, and add up the
full EAX results.

[message.rs](../../crates/inf-game/src/object_runtime/actor/message.rs)
owns the receivers' arms by final table, and the world's
[delivery](../../crates/inf-game/src/session_platform/world_actor_update/message.rs)
applies their effects: the vehicle's text, damage and radar jam (the map's
jitter, which the world lends), a smoked factory's text, and retirement
through `0047CE90`.

| `+98` | Classes | Arms |
| --- | --- | --- |
| `004270B0` | derived and family bases, 10, 34, 35, 36, 77 | 0xD: `+17A` at least 8000 (signed) |
| `00402420` | 11 | 0xB: `+1B0` at least 8000; 0xC: `+1B0` 12000; 0xD: `+17A` at least 8000; 0x15: retire; 7, 8: lifecycle |
| `00420BE0` | 2 | [the vehicle's own](VEHICLE_DRIVE_RUNTIME.md) |
| `00468BE0` | 12, 13 | 0xD: retire unless flag `0x80`; 0x16: grants |
| `00480F40` | 60 | 7, 8: lifecycle |
| `004045F0` | shared, 16, 59, 88, 90, 93 | none |
| `00454AB0` | 53 | 0xF: an idle spawner arms and updates; an updating one answers its kind |
| `004546F0` | 52 | 7: a link; 8: one less |
| `0042D6B0` | 74 | 7: linked, and a link; 8: one less |
| `00405490` | 66 | 5: moves it by the argument's vector |
| `0048C470` | 30 | 0xD: none; 0x14, 0x15: `+164`; else `0040DE70` |
| `0042F380` | 15 | 0xD unless remote: [the smoke](#smoke); 6, 0x18: [the unit's broadcast](FAMILY40D800_RUNTIME.md); else `0040DE70` |
| `0040A180` | 37 | 2, 3; else `0040DE70` |
| `0043BDF0` | 39 | 0xB, 0xC: `+164` 0 or 1, then retire with `+160`; 0x14, 0x15: `+166`; else `0040DE70` |
| `0040DE70` | the 0040D800 family | 7, 8: lifecycle |
| `0047FE10` | 47 | 0xB, 0xC: silences it for 10 s; 0xE: the alarm, from the argument or `+16C` seconds; else `0040DE70` |
| `004864B0` | 57 | 5: retires it, then its sender when that is resource 0xE |
| `004557F0` | 26, 38 | 9, and 10 from the owner or a null sender: marks it to detonate |
| `00456AD0` | 25, 28, 68 | 0xB, 0xC: `+150`; then `00455740`: 5, 9 spend the fuse |
| `004860D0` | 32 | 1, 2: the repeat |

The arms listed as lifecycle, grants or moves are not ported; a message
that reaches one stops where it is. The other codes these tables do not
name return 0.

### Smoke

A smoke grenade (payload flag 1) fizzes once it is older than 1.5 s
(`004558B0`): its first alert is `0047DBF0(800, 0xD, 0, 0x12)` and each
later tick has a 1-in-16 chance of another within 1200, always to side
0x12, the mutants. A mutant scientist (`00468BE0`, selectors 12 and 13)
dies through `0047CE90` unless its flag `0x80` is set. A factory the
mutants hold is an enemy factory (command 22 = 1) with staff, which its
`0042E330` puts on side 0x12; its `0042F380` (`0042F3E7..0042F44B`), on a
local factory (`+2C` bit 31 clear):

1. with staff `+140`: `0040D8D0(-staff)` empties it (dirty `+39 |= 2`), and
   with a first player (`00488AA0`) that player's message reads H0 string
   0x30C, "Factory neutralised" (`004135D0(text, 0)`);
2. staffed or not, `+38 |= 2` and `+B4 = 0`;
3. then `0040DE70`, which answers 0 for 0xD.

Team 0 opens the factory to any vehicle (`0042F490` docks teams 0 and 1 and
the vehicle's own), where before it answered "Access denied" and, for side
0x12, command 0x1D, the hint "The factory is being controlled by the
mutants - you must get them out using smoke grenades" (H0 721). Worlds 5,
9, 13, 17 and 20 (OVLs 15, 19, 23, 27 and 30) place such factories.

The [port](../../crates/inf-game/src/object_runtime/actor/selector15/staff.rs)
runs the arm and the world delivers the text. No recording throws smoke:
`factory-smoke.js` finds no
`0047DBF0` walk and no 0xD to a factory or a scientist in Infest01..03
(Infest02's 32 factory `+98` calls carry 0xA and 0x16). The original
`0042F380` runs instead in
`check_world_interactions.py` over 48
supplied factories, staffed or empty, local or remote, with and without a
first player and with the string table short or long: the port's staff,
team, dirty bytes `+38`/`+39` and text match every one.
[A test](../../crates/inf-game/src/session_platform/world_actor_update/grenade26_tests.rs)
throws one beside world 5's mutant factory: on its fourteenth 125 ms tick it
alerts, the factory loses its staff and turns team 0, a mutant scientist 300
from the grenade dies and a team-2 one beside it lives. Live port runs agree.

An unmodified run from the Tabuchi save reaches the mutant factory on keys
alone. The hovercraft (3) crosses the lake to its north shore, climbs the
valley over the hills and settles in the ring east of the factory, about
2300 from it. Page Down three times selects the smoke, and X held 1.25 s
lobs it to 451 from the factory. 1.5 s after the throw the factory turns
team 0, its staff fall from 2 to 0, and "Factory neutralised" types out. A
build that only adds logging renders the same frames pixel for pixel.

The same alert raises the alert timer `+17A` of turrets near the grenade.
Their next update then wobbles the aim (`00401EA7`) from `+1AC`/`+1AA`.
The constructor never writes those two angles, so they are allocation
residue the port does not know. Infest04's 65 turret constructions show
what retail leaves there
(`turret-residue.js`). All
13 of its first world load, on a fresh heap, hold zero. The later loads
hold zero in only 24 of 52, and the rest hold leftovers such as
`(30308, 28257)` and `(3954, 0)`. No single start is retail's. The one
turret a stun reached in Infest04 held zero in both, and the port starts
them at zero, the fresh heap's value
([Selector11WobbleAngleApproximation](ACTOR_RUNTIME.md#selector-11-turret-update-and-retarget)).
Before that, a stunned turret stopped its update there for good and could
no longer die.

REA bounds that history. Among the turret's constructor
chain (`00425E60`, `004014F0`) and every method of its vtable `0051F080`,
only the update `004019E0` touches the two words, and only through
`ADD word ptr [ESI+1ACh]` at `00401EBD` and `[ESI+1AAh]` at `00401EDB`; the
constructor writes `+188..+198`, `+1A0..+1A8` and `+1AE..+1B2` around them.
`operator new` (`00511708`) reaches `__nh_malloc` and `005124A8`, which
serves every request up to `[00545C70]` = `0x3F8` bytes from the CRT's
small-block heap `00515BFC` under lock 9, and larger ones from `HeapAlloc`
on the CRT heap `[0065FAE0]`. A 0x1B4-byte turret is a small block, so its
leftovers depend on the order and contents of every earlier small
allocation and free in the process, from startup on. Modelling them means
replaying that whole history, not seeding one value.

On Terra Nova, smoke thrown beside two mutant scientists kills both, and the
four beyond 800 live. That save lacks smoke, so a temporary harness granted
it; the throw and the alert were the game's own.

The player's later save on Oolbryte (world 20, the sixth slot of the
retail `Infest.sav` of 2026-10-11) has both: eight smoke grenades and four
mutant scientists in the open east of the start, out of the turrets' reach.
An unmodified run from it shows the kill on the game's own keys. After the
menus (Return at 10 and 100, Down at 180, 200, 220, 240 and 260, Return at
300, Down at 420 and 430, Return at 440) it presses:

- Page Up at 462 (the smoke; row 9, with no grenades, is skipped);
- Left 467-470, 473-476, 479-483, 485-489, 491-495, 497-501, 503-507 and
  509-510, Up 467-473, 503-515, 527-533 and 539-545, Down 491-493 and
  521-523, Right 515-519, 521-525, 527-531 and 533-537: the buggy crosses
  the plain under fire from a mutant unit (model `0x12D`);
- X 541-545, releasing the smoke on the move toward mutant scientist 56.

The smoke arcs over the plain. 1.5 s after the throw (tick 102) it is
about 580 from the scientist, inside the first alert's 800. The scientist
dies there: health 300 to 0, lifecycle 1 and the dying model `0xF8`, with
no damage packet, while the buggy stands about 1150 away. The clean
build's frame 557 shows the scientist with the smoke overhead, 561 its
burst and 566 the smoke where it stood. A build that only adds logging
renders frame 561 pixel for pixel the same.

Infest03's chain grenade reached a gate, a factory, the vehicle, a selector
16, three scientists riding the vehicle and the grenade itself. Only the
vehicle acted: its radar jam.

The existing [pure `+58` and `+7C` policies](ACTOR_RUNTIME.md#pure-actor-derived-component-query)
retain their caller contracts. Damage's combined `+58` then pointer-valued
`+CC` has a separate owner in
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#shared-instantaneous-actor-damage).

## Original final-table bindings

These are exact file-backed PE DWORDs in the supported retail executable.
Every table but the factory's has `+50 = 00401300`, the null DWORD leaf;
the factory's answers itself. `+54` returns self for selector 30 and the
three staffed classes, the factory, the bridge and the course target. Their
reached `+CC` is numeric: `0048BE30` for selector 30 and `0040E090` for the
staffed classes. A null `+54` does not reach a component `+CC` in this
caller.

| Canonical class / final table | `+54` | Void `+9C` |
| --- | --- | --- |
| Central derived `00520E58` | `00401300` null | `004026D0` |
| Family `00520F34` | null | `004026D0` |
| Scalar base `0052104C` | null | `004026D0` |
| Probe base `00521164` | null | `004026D0` |
| Family 4236 `00520B84` | null | `004026D0` |
| Selector 10 `00520CC8` | null | `004026D0` |
| Selector 34 `00521EAC` | null | `004026D0` |
| Selector 35 `0052273C` | null | `0043F860` |
| Selector 36 `00527EE0` | null | `004026D0` |
| Selector 11 `0051F080` | null | `004026D0` |
| Selector 2 `00520860` | null | `004026D0` |
| Selector 60 `00527DE8` | null | `004026D0` |
| Selector 77 `0051FBB0` | null | `004026D0` |
| Selector 12 `00527058` | null | `004026D0` |
| Selector 13 `005269C4` | null | `004026D0` |
| Shared `00527C00` | null | `004026D0` |
| Selector 16 `005204D0` | null | `004026D0` |
| Selector 30 `005289C0` | `00404610` self | `004026D0` |
| Selector 15, the factory, `00521940` (`+50` self too) | self | `004026D0` |
| Selector 37, the bridge, `0051FF54` | self | `004026D0` |
| Selector 39, the course target, `00522398` | self | `004026D0` |
| Selector 88 `00524AC0` | null | `004026D0` |
| Selector 90 `00526B50` | null | `004026D0` |

Standalone scalar construction and selector-13 construction remain
unrepresented. Their static table rows supply binding evidence, not invented
portable classes. The probe-base constructor is represented independently of
the still unrepresented complete selector-21 dispatcher.

`00401300` is `xor EAX,EAX; ret`; `00404610` is `mov EAX,ECX; ret`.
Both leaves read no scalar payload. The portable caller first authenticates
the actual live actor/class, then selects the canonical binding. A direct
constant-leaf call with stale or unmapped ECX cannot establish live typed
dispatch custody.

`004026D0` consists of `C2 08 00` (`ret 8`). It performs no read or write
and promises no scalar return value. Its portable notification returns unit
after receiver authentication, preserving all state and bypassing the ignored
source, distance and clock inputs.

## Staffed relation `0040E090`

The factory, the bridge and the course target answer `+CC` with
`0040E090` and the source: their free staff, `+13C` less `+140`, but 0 for
a source of another team when the component has a team (`+B4` nonzero).
Selector 12's search divides a candidate's score by it, so a scientist
heads for the staffed actor with room on its team, or with no team. The
factory's [staffing](FAMILY40D800_RUNTIME.md#staffing-0042f170-0040dab0-beam-down-0042f670)
uses the same count.

## Selector-30 numeric relation

`0048BE30..0048BE66` consumes the real selector-30 component returned by
`+54`. Its order is:

1. Read BYTE component `+18E` at `0048BE30`. Nonzero returns full DWORD 1
   at `0048BE3A`, before source validation or either classification read.
2. Otherwise read the source argument at `0048BE42`. Null returns DWORD 0.
3. A nonnull source reads fresh source `+B4` at `0048BE4A`, then fresh
   component `+B4` at `0048BE50`. Different DWORDs return 1; equal DWORDs
   return 0. Each path uses `ret 4`.

`Selector30State.byte_18e` and `ActorState.word_b4` already own these scalars.
Source identity is admitted only at the reached source read. Nonzero `+18E`
therefore bypasses a retired/nonactor source, while a reached invalid source
stops before component classification. Unknown mandatory numeric spans remain
source diagnostics rather than fabricated typed values.

The numeric 0/1 is the original caller's divisor. A pointer-valued family
damage `+CC` result cannot substitute for it. The query mutates no actor,
registry, list, task, clock or process state.

## Selector-35 notification and live clock

`0043F860..0043F87C` reads BYTE candidate `+242` at `0043F860`. Known zero
returns immediately at `0043F87A`, preserving DWORD `+244` and bypassing the
clock. Known nonzero reads actual DWORD `005471C0` at `0043F86A`, adds `C8`
with DWORD wrap at `0043F86F`, then writes candidate `+244` at `0043F874`.
The two notification arguments remain unused.

`Selector35State.bytes_240[2]` retains optional byte custody. An unavailable
`+242` yields `ActorInteractionError::Uninitialized` before testing clock
availability. The nonzero arm requires the actual borrowed `AnimationClock`;
absence yields `MissingAnimationClock` before the destination write. Both
errors preserve the previous `+244`. Unused neighboring bytes do not become
preconditions.

The existing [animation clock](../../crates/inf-game/src/frontend_camera.rs)
owns `005471C0`. Its pure `value()` read happens only in the reached nonzero
arm, and immutable borrowing preserves that clock. The addition uses 50 Hz
animation ticks, not microseconds. Constructor zero, later code-24 writes,
repeat notifications and clock wrap retain their distinct field ownership.

## Selection caller and remaining boundaries

The distinct selector-12/13 `+104 = 00468350` caller invokes candidate
`+54` at `00468586`, retains the returned component identity, then invokes
candidate `+9C(self, distanceSquared)` at `004685A7`. Only afterward does it
query the captured nonnull component's `+CC(self)` at `004685B8`; a nonzero
numeric result reaches self `+50` at `004685C8`. Its prior `+58` read at
`0046847C` belongs to a different selection branch.

The interaction primitives implement these canonical boundaries independently
of the separately owned [selector-12 traversal and scoring](FAMILY_TARGET_SELECTION.md#selector-12-actor-list-selection-callback).
That policy borrows the live clock after earlier mutable gates and retains the
captured component identity through notification. Arbitrary virtual overrides,
component detachment, raw aliases, pointer reuse/ObjectId ABA, unrepresented
constructors, production update attachment and complete intro or briefing
acceptance remain separate boundaries in
the Infestation objective.

## Native evidence and verification

check_actor_interactions.py authenticates
the supported executable SHA-256, unchanged text, FPCW `027F`, all twenty
table bindings and five literal leaf bodies. It executes four unchanged
caller fragments from `00468350`, stopping immediately after their calls:
`004685C4..004685CB` (`+50`), `00468581..00468589` (`+54`),
`004685A0..004685AD` (`+9C`) and `004685B5..004685BE` (numeric `+CC`).
This evidence owns those call boundaries and leaves; it does not execute the
complete selection traversal or native actor constructors/destructors.

The [fixture](../../crates/inf-game/tests/fixtures/actor_interactions_native.json)
contains 36 histories/78 invocations, with 77 leaf entries, 70 native RETs,
77 caller-fragment entries, 69 completed fragments, 110 reads and three
deadline writes. Six completed fragments belong to the two static-only table
diagnostics, and the additional RET belongs to a raw constant-leaf diagnostic.
These totals do not establish portable acceptance of those diagnostic paths.
Fixture SHA-256:
`AA629B8ED195DBE28292FF10A6A020FFD2B19CF14F453047DDD6DB7E5E0B2834`.

The [native comparison](../../crates/inf-game/src/object_runtime/actor/interactions/native_receipts.rs)
passes 30 admitted histories/68 calls: 25 kernel histories/63 completed
fragments and five typed semantic prefixes. Prefixes retain retired actor or
source custody, unknown `+242` with known/unavailable clock, and missing clock
before consumption or destination mutation. Six exact source-only histories/
ten calls remain excluded: scalar-base/selector-13 tables, unknown mandatory
`+18E`/source `+B4`/component `+B4`, and a direct null leaf with retired ECX.
No prefix claims native bounds checks, raw faults or stop-IP parity.

The comparison uses real portable constructors for eighteen represented
classes, followed by explicit input fields. Selector-2's flat H12/origin
carrier is supplied construction setup, outside native constructor parity.
Known-byte masks retain unavailable spans; retired sources remain real retired
typed identities. Results, entire untouched typed receiver/source state, list
custody, native reached-read masks/order and declared writes are checked.
The actual animation clock is seeded/advanced through its public owner and
remains unchanged within each call. Only selector-35 `+244` may change. Void
`+9C` raw EAX observations have no portable scalar counterpart.

[Seven focused tests](../../crates/inf-game/src/object_runtime/actor/interactions/tests.rs)
also pass in release mode. They cover all represented classes, canonical
identity and purity, lazy retired/nonactor sources, full-DWORD classification,
receiver authentication, unknown-byte/clock error priority, repeat clocks and
wrapping deadlines. Independent regeneration matches the fixture hash.

```text
python check_actor_interactions.py --output .tmp/actor-interactions.json
cargo test --release -p inf-game --lib object_runtime::actor::interactions
```

Run Cargo from ``. The native checker uses the same
Python/Unicorn environment as the existing recovered instruction checkers.
These isolated checks do not establish production or retail cinematic acceptance.
