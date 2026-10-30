# Infestation 00428650 actor family and selectors 34/35

This document owns the family above the central `00425E60` actor and the
implemented selector-34/35 branches through `004293D0 -> 00428650`.
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md) owns the shared and central prefixes;
[CONSTRUCTOR_ASSETS.md](CONSTRUCTOR_ASSETS.md) owns authored input views.
Code lives in [family428650.rs](../../crates/inf-game/src/object_runtime/actor/derived/family428650.rs)
and the concrete [selector34.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector34.rs)
and [selector35.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35.rs) owners.

## Family construction and admission

`00428650` consumes no additional RNG beyond the central constructor's draw.
It initializes behavior index `+184=-1`, count `+1A0=0`, four twelve-byte
handle entries beginning at `+1A4`, six pointer/scalar pairs at `+1D4..+200`,
placement reference `+20C` to the explicit global null value, `+210=0`,
`+214=256`, pointer `+218=null`, `+21C=0`, `+228=0`, byte `+22C=0` and
WORD `+22E=0`. Unwritten fields remain unknown.

Guard `+38=004270E0` first invokes central admission `00426C80`. After it
succeeds, it copies current actor XYZ to the three home-position WORDs at
`+222`. This happens before checking the suffix count, even for no commands.
It can overwrite the home-position adjustment made by a derived payload.
Every family table binds this guard: selectors 9 (the 004236D0 family's own
class, `00520B84`), 10, 12, 21, 34, 35 and 36. The port used to admit
selectors 9 and 21 through the plain derived admission, so Oolbryte's supply
truck (selector 9, model 297) and the Nemians (selector 21) never had a home,
and the home-distance score `00425DB0` stopped their behavior at `+224`. A
[test](../../crates/inf-game/src/object_runtime/construction_tests.rs)
loads Oolbryte and world 15 and finds every family actor homed, the truck
where it stands.

The six owned subobjects stay null throughout the implemented methods.
Common allocation `+218` stays null for selectors 34/35, while the
[004236D0 family and selector 36](FAMILY4236D0_RUNTIME.md) acquire its
concrete `00482FF0` owner during postinitialization. Nondeleting destructor
`00428750` releases that separate allocation before the derived/shared
destructors; later creation/deletion of the six subobjects remains open.

## Family commands and after-load

Virtual `+04=004276B0` delegates signed codes at most 18 to `00426030`.
It adds:

| Code | Read and effect |
| --- | --- |
| 19 | no operand read, success |
| 20 | two unsigned WORDs X/Z; sample `00450A30`, store home XYZ |
| 21 | three WORDs X/Y/Z; add interpolated terrain height to Y with WORD wrapping, store home XYZ |
| 22 | raw DWORD placement reference to `+20C` |

Codes above 22 reject. Terrain is required only for reached codes 20/21;
the command decoder retains their raw operands and does not sample terrain.

After-load `+34=004277E0` resolves a **nonzero** raw `+20C` through the
shared one-based placement lookup and writes the returned handle into that
same DWORD. Exact zero skips lookup. Repeating the callback reinterprets a
previous handle as a placement reference, as retail does. No sentinel or
host phase flag changes this behavior. Enclosing `00401320` then sets
`00800000`.

## Selector 34

Constructor `00433B20` installs vtable `00521EAC` after intermediate
`004293D0` initializes DWORDs `+230=500`, `+234=512` and bytes `+238=1`,
`+239=0`. It clears retained pointer `+23C` and DWORD `+25C`, and sets
`+240=750`. Unwritten fields remain unknown; construction adds no RNG draw
beyond the central constructor's one draw.

Payload wrapper `00433C70` runs virtual context initialization before
`00433C00`. The latter calls the central payload owner, retains payload `+58`
at `+23C` without reading it, and installs static table identities
`+24C=005329F0`, `+250=005329E8`, `+254=005329E0`, `+258=005329D0`.
It also copies current XYZ into home `+222` with Y increased by `0400`.
Family admission subsequently replaces that intermediate home position with
the unadjusted current position before any commands execute.

Command `00433C90` delegates signed codes at most 22 through `00429440` to
the family owner. Code 23 retains the H14 operand address at `+23C` without
reading it; higher codes reject. The retained H13/H14 runtime allocation
preserves shared alias identity and remains alive independently of residency.

Postinitializer `00434FA0` has a distinct ordered policy:

1. Run behavior selection `00428390` against the existing `+140` owner.
2. Read unsigned WORD `retained+0`. If nonzero, read unsigned WORD
   `retained+0E`, write `(numerator << 10) / denominator` at `+214`, and OR
   flag `00001000`. A zero denominator preserves both the ratio and flag
   and skips the numerator read. Selector 35 instead uses numerator `+10`.
3. Run shared `0047AAD0`, including `+28 >> 3` feedback into `+74`.
4. Only if flag `00200000` is set, sample `00450A30` at current X/Z and
   compare **signed** current Y with the sign-extended terrain result plus
   **unsigned** model radius WORD `+40 >> 2`. If lower, store the floor's
   low WORD directly into Y. This does not dirty the object or update home Y.

The terrain surface is required only in that final branch. Missing portable
inputs fail explicitly after earlier effects; no terrain or retained operand
is invented. After-load `+34=004277E0`, admission `+38=004270E0` and network
`+40=0047E2E0` use the established owners.

Update `+24=00434210` has Selector 35's prefix shape: no `00429D60` water
tick, the signed `+144` delay with physics mask `1F`, status `0047C990`,
weapon tick `0048D030` with `+140`, then `+D8=00435050`. The port runs that
prefix through the shared family owner and the
[behavior profile](ACTOR_BEHAVIOR_RUNTIME.md#concrete-weighted-target-ownership)
over tables `005329F8/00532A20`. Candidate bias `+110=004350E0` is the
family consumer of retained WORD `+0C`. The class tail follows in native order:
controls `0043429C..004349D3` (target refresh, the terrain-clearance scan over
the retained sampling policy, predictive firing and signed control words),
motion `+118=00434C10` (force response, shared physics mask `19`, then the
`+25C` ribbon) and the camera-gated engine sound `004349E7..00434B0F`. The
motion force phase is pinned by
[17 native cases](../../crates/inf-game/tests/fixtures/selector34_motion_native.json).

Motion stores `00486600`'s result directly in `+25C` and calls `004869B0`
only while it is nonnull. The ribbon is the [selector 67 trail](SELECTOR67_RUNTIME.md);
ship forces and physics never read `+25C`.
Destructor `00433B90` exits any selected descriptor with argument false, then
delegates to `00428750`; it never releases `+25C`. Every selector 34 exit
target (`00428630`, `00434130`, `00433FB0`, `00434080`, `00428AB0`,
`00427990`, `00427A00`, `00427A70`) is entry-only, so the exit writes only
`+184=-1`.

[Attack](../../crates/inf-game/src/object_runtime/actor/derived/selector34/attack.rs)
is class-specific. Entry `+F8=00433FB0` captures `+1F4`, writes Attack
status, and with no source sets `+22E` bit 0. Otherwise it rearms `+18C` as
2,000,000 plus `(draw & FFFF) << 4`, clones the unsigned-score winner of
`+1D4/+1F4` into `+148`, and marks `+22C` when the clone resolves an actor.
It has none of common `00429B00`'s `+208`, helper or attack-sound effects.
Earlier production routed this entry through that common owner.

Update `+F4=00433CC0` returns while byte `+239` is set. Otherwise it writes
`+230=200` and `+234 = 350` (`+14C <= 1000`) or 600. The period then either
decays (signed `+18C` above delta with status `+150 >= 3`) or rearms. A
rearm with a source rearms again and may replace `+148` with a refreshed
clone that is more than a quarter nearer. Without secondary `+154`, a WORD
`+248` below `+240` and `+14C` below twice `+240` publish a velocity-led
kind-8 point: `+234/+230 = 750/500` and `+15C` from a fresh draw plus
`2DC6Ch`. With `+154`, a distance beyond half `+164` sets `+15C=1`. The
[38 native receipts](../../crates/inf-game/tests/fixtures/selector34_attack_native.json)
run both bodies with only allocator/free supplied, including the native
NULL `+148` faults.

The entry's clone is the one `+148` read that can see unwritten bytes. A
type-2 clone goes through explicit-point constructor `004828D0`, which writes
`+14` but never `+08..+0D`. Native runs the update right after the entry in
the same tick, before the class tail refreshes `+148`. If that update takes
the velocity-led branch, `00433F42` reads the clone's unwritten Y at `+0A`.
Infest01 never takes that branch on an entry tick. Its four `00433F42` reads
all see targets that a tail refresh at `004826C0` rewrote frames earlier. The
port's 60 fps intro does take it: slot 77 enters Attack and reads the new
clone in the same update. The
clone histories
bound the value. Infest01's four entry clones held Y `0` three times and
`BEB6`, weapon-node residue, once. Their X and Z were nonzero residue.
Production therefore zero-fills a type-2 entry clone's `+08..+0D` under
the extended allocation approximation. Strict arenas still stop at
`00433F42`.

Clones made in the update itself differ. Before cloning, the update releases
the old `+148`, and the native free writes only that block's `+00/+04` links.
In Infest01 the clone reused the same block (`03849B60`), so its `+08` kept
the released target's last refreshed point. The port has not reached a
same-update read of such a clone, and it does not model this reuse.

[Dying](../../crates/inf-game/src/object_runtime/actor/derived/selector34/dying.rs)
is class-specific too: descriptor 1's wrappers `0043FE40/00435240` reach
`+120=00434130` and `+11C=004341A0`. The entry tests only its argument's low
byte. It writes Dying status `+228=00530D3C`, allocates a kind-8 point at the
actor's position with the process NULL, read only after a successful
allocation, and stores it in `+148` without destroying the previous target.
It then clears byte `+238` and `+230`. Unlike Selector35's `0043DEB0`, it
draws no velocity RNG. The update leaves state 1 through
`0047D670(0, process NULL, 0)` once the descriptor's `+188` exceeds 15 s.
Until then it runs the shared status visual `0047DFD0` and points a kind-8
`+148` at `(X, EC78h, Z)`, a dive toward Y = -5000; other target kinds keep
their focus. The dive ends at the terrain: contact `+94=00434ED0` treats
every `+C4` but 3 as a crash, running a burst, the radius effect, a kind
`0x83` pickup worth 500 and `0047D670(0, process NULL, 0)` before the shared
response ([WORLD_CONTACT.md](WORLD_CONTACT.md#terrain-responses-virtual-94)).
State 0 runs final `+B0=004026E0`, which queues disposal like Selector12's.
The [24 native receipts](../../crates/inf-game/tests/fixtures/selector34_dying_native.json)
run both bodies with the status visual and RNG unchanged, covering the
exit edge, a NULL allocation and the visual's skip, timer and size branches.
SHA-256: `1E6F0D1618B1D4B164092E66D626FDD2A968FDABF73B59127FC12F933C2AA7B7`.

## Selector 35

Intermediate `004293D0` adds DWORDs `+230=500`, `+234=512`, byte `+238=1`
and byte `+239=0`. Selector 35 installs final vtable `0052273C` and adds
its known scalar fields and null handle `+268`; unwritten fields stay unknown.

Payload `0043D1C0` first executes the central payload initializer. It then:

- Reads payload DWORD `+70` as a boolean into byte `+251` and selects static
  data identity `00533D68` or `00533D70` at `+270`.
- Retains the address of payload `+58` at object `+23C`, without reading it.
- Copies current XYZ into home `+222` with Y increased by `0400`.
- Computes unsigned `(payload_dword_6C * 0x3D0) >> 8` with DWORD wrapping
  into `+258`, then its wrapping square into `+25C`.

The subsequent family admission guard copies unadjusted current XYZ over
that home position before any command runs. The `+400` value is a real
intermediate state, not the final default home Y.

Command `0043D270` delegates codes at most 22 to the family. Code 23 stores
the H14 operand address at `+23C` without reading it, replacing the H13
address. Code 24 reads a DWORD into `+240`, clears byte `+243`, and ORs
flag `10000000`. Larger codes reject. The retained value keeps the actual
shared H13 or H14 runtime allocation alive; aliases use one allocation.

Postinitializer `0043F760` reads unsigned WORD `retained+0`. If it is zero,
it leaves `+214` unchanged and does not read `retained+10`. Otherwise it
sets `+214 = (unsigned_word_at_10 << 10) / denominator`. It then invokes
shared `0047AAD0`, including model-selector feedback. This path does not
allocate another object or select behavior.

Network `+40` is shared `0047E2E0`; after-load is the family callback.
Destructor `0043D0F0` exits a nonnegative selected descriptor with argument
false before delegating to `00428750`. The
[Selector35 runtime](SELECTOR35_RUNTIME.md#destruction-exit) owns that exit
together with the class update, descriptors and lifecycle.

## Probe collision callback

[Shared contact geometry](CONTACT_GEOMETRY.md) owns the supplied sphere-pair
and group-direction primitive results used below the auxiliary dispatcher.
The recursive interpreter and the full `0047C1D0/0047BFD0` path remain separate
owners; primitive comparisons do not attach the actor callback to production.

Virtual `+A8=0042A760` belongs to the probe-owning `00429E20` family,
`004236D0` and selectors 10/12/36. The complete literal pointer audit also
finds selectors 13/21, whose final constructors are not admitted by the
current runtime. Central `00425E60`, base `00428650` and selectors 34/35
instead use `0047BFD0` directly. `00455880` is another class callback with
its own `+14C/+14E` policy before that continuation; renderer thunk
`00411D50` uses a separate renderer `+A8` far-effect-quad method.

[collision_guard.rs](../../crates/inf-game/src/object_runtime/actor/derived/collision_guard.rs)
owns the bounded wrapper. If WORD `+22E` contains `8000`, it clears only
that bit and returns `AL=1` without reading any of the four operands.
Otherwise it requires mutable `0047BFD0`, forwarding complete ordered
operand custody and preserving its low-byte return and reached mutations.
At actor call `0047C67F`, those operands are the 20-byte collision result
(success DWORD, three float vector components, signed low-WORD displacement),
H11 four-byte selector record, terrain-cell XYZ WORDs and saved actor XYZ
WORDs. The API retains them as an opaque borrowed owner; it does not decode
or synthesize the packet. Portable class admission precedes these native
mutation-prefix claims. Geometry `0047C1D0`, the complete `0047BFD0`
velocity/damage/material continuation and enclosing updates remain required.

Reproduce the [native fixture](../../crates/inf-game/tests/fixtures/family_collision_guard_native.json)
with `python check_family_collision_guard.py`
(Unicorn 2 required). The tracked checker
executes unchanged `0042A760` in 78 cases and pins all seven guard pointer
registrations. Every guard case has an unmapped four-operand stack; clear
cases supply the continuation explicitly, including upper-EAX/low-AL
differences, actor mutations and stopped failure prefixes. The
[comparison tests](../../crates/inf-game/src/object_runtime/actor/derived/collision_guard/tests.rs)
also preserve non-Clone operand custody and distinguish the represented
guard classes from direct-continuation classes. Supplied continuation
results are wrapper-boundary coverage, not evidence of its internal policy.

## Native checks and production coverage

The [selector-34 receipt](../../crates/inf-game/tests/fixtures/selector34_native.json)
contains 12 cases and 228 snapshots from unchanged constructor, payload,
nonallocating admission, commands, postinitializer, after-load, network and
nondeleting destructor routines. Its
[comparison](../../crates/inf-game/src/object_runtime/actor/derived/selector34_native_tests.rs)
checks all initialized shared/family/class fields and retained identities.
Inputs explicitly supply allocation bytes, model metadata and flat terrain.
The zero-denominator case leaves the numerator unmapped; flag-clear cases
leave the native terrain table unmapped. Cases also distinguish numerator
`+0E` from `+10`, unsigned WORDs, signed Y, direct-write dirty preservation,
repeated postinitialization and home/actual-position divergence.
These routines have no instruction patches or substitute callbacks. The
behavior pointer stays null in this receipt; existing behavior tests own its
nonnull selection policy. Registry initialization uses the separately
identified original tails. This is supplied-input native execution, not a
recorded complete factory or world load.

First-world placements 6 and 21 use selector 34 (`pership`). Production
factory tests exercise their resident payloads and ordered H14 commands,
shared retained custody, creation-listener timing and failure cleanup.
Combined mode-0 construction coverage, including the separately owned
[selector-10/12/36 family](FAMILY4236D0_RUNTIME.md), is all 118 first-world placements;
later loader passes and complete world admission remain open.

The [family receipt](../../crates/inf-game/tests/fixtures/family428650_native.json)
has four cases and 88 snapshots. The
[selector-35 receipt](../../crates/inf-game/tests/fixtures/selector35_native.json)
has eight cases and 152 snapshots. The comparison executes production code
against unchanged retail constructor, payload, nonallocating admission,
commands, postinitializer, after-load, network and destructor results.
Registry setup uses separately identified initialization tails with explicit
buffers. There are no substituted callbacks or instruction patches in the
tested actor routines. Nonnull owned resources and behavior index at least
zero are outside these receipts, as are enclosing factory calls.

The zero-denominator case retains the last mapped WORD in its allocation,
with numerator `+10` unmapped: the real native postinitializer returns
without reading that numerator. Selector-35 receipts also execute actual
`0043D270 -> 004276B0 -> 00450A30` for codes 20/21 with uniform signed
heights -128, -1, 0 and 127 and WORD-wrapped coordinates. Broader interpolation
coverage belongs to the existing terrain owner. The family-only receipt
exercises codes 19/22, not its terrain commands.

Fixture SHA-256 values:

- Family: `3E3E2B8B90083B6E431A83B2488921F37F5CFB1101087921DBA0803A47397437`.
- Selector 34: `73352843604B941CCD9B128A280048B05B9855E57E06323A14B03D02FFC961D6`.
- Selector 35: `3B53C273D2B55E949764C10C35A979CB5EC4D4CBE7238AF5FAD50B1D849BD931`.

First-world placements 80, 96 and 97 use selector 35. Placement 80 uses
H13 slot 144, `demont3`; its denominator is 250 and numerator is zero, so
postinitialization sets `+214=0`. Placements 96/97 use slot 145, `commship`,
with zero denominator, preserving constructor `+214=256`. These inputs and
their ordered commands run through the production factory with explicit
terrain and shared allocation custody.
