# Infestation shared object draw manager

This document owns manager `00479570`, common visual `0047AFC0`, actor
`0047D910`, flash `0047DEA0`/`0047DFA0`, offline vehicle `00420510`,
and indexed process profiling `004395A0`/`004395C0`. The consumed
[object draw owner](../../crates/inf-game/src/object_runtime/object_draw.rs)
uses the actual arena, embedded Flag0 lists, current terrain, caller context,
process state, and backend. [Packed getters](OBJECT_MODEL_CALLBACKS.md) and
[model execution](MODEL_RUNTIME.md) retain their separate owners. Complete
intro, briefing, and gameplay retail presentation acceptance remains open.

## Shared caller order

| Caller | Reached order |
| --- | --- |
| Briefing terrain pane `0040BB40` | Ground `0044AA10` at `0040C1D2`, light `00479500` at `0040C1E0`, draw `00479570` at `0040C1EE`, flush `0040C1FC`, reset `0040C20A`, separate depth clear `0040C21C`, camera static. |
| Gameplay world `00488020` | Active particles at `00488121`, light at `00488147`, draw at `0048816E`, pending particles at `00488196`, transfer at `0048819D`, terrain at `00488359`. |

Both callers consume the same manager without campaign or level checks.
Gameplay clears and anchors the process light grid before particles; briefing
clears cells while retaining anchors before ground. Neither manager insertion
moves light ahead of its original caller position. The placement/respawn redraw
approximation is removed from production.

Briefing supplies its actual pane camera and retained `[1800,1A00]` caller
distances. Gameplay supplies its freshly acquired camera and predraw
distances. These caller fields remain distinct from the published renderer
view and projection state consumed by models. Actual H12/0 acquisition
authenticates the retained prepared terrain's registration and allocation at
each reach; it neither decodes a replacement per object nor invents a missing
terrain. The renderer retains the same command queue and process RNG. Gameplay
commits the reached RNG before returning from this complete manager phase;
briefing's enclosing draw commits it on every return.

## Traversal, geometry, and component custody

Complete `00479570..00479835` moves each freshly selected primary embedded
Flag0 node to the alternate tail before fields, terrain, or callbacks. It
captures entry far once and caller eye LOWWORDs. Clear `02000000`, saved
signed-WORD X/Z differences, unsigned extent `+28`, and fresh H12 sampling
precede horizontal admission. Positive signed height excess above
signed LOWWORD `max(raw_height,plane)+256` expands the radius by six times the
excess, with native DWORD wrapping. Horizontal squared distance must be
strictly smaller than the wrapped radius square.

The cone then rereads Y, accumulates the retained dot in Y,Z,X order and uses
CRT truncated QWORD LOWDWORD. Fresh extent is added before the signed positive
test and unsigned strict `distance_squared < 2*dot_squared`. Passing writes
`02000000` before the actual-class virtual `+2C`. Geometry uses signed WORD
coordinates and wrapping arithmetic; widened distances and convenient
absolute values do not substitute.

After the near callback, fresh original-source `01000000` selects virtual
`+18`. Admitted actors return Self; admitted nonactors return genuine NULL.
NULL retains the source flag and skips all component/model/behavior reads.
Otherwise fresh component `02000000` bypasses only the late packet. The late
packet captures component model `+42`, signed XYZ and actual `+48` basis;
packet `+3C` is that component, distinct from the manager source and H8 model.
Only normal late return reaches original-source `01000000` clear and component
`+7C`. The same retained behavior now owns its actual `+00` fire nodes.
The [shared drain](WEAPON_FIRING_RUNTIME.md) consumes nonempty chains with the
current renderer RNG loan; its empty branch has no descriptor or geometry
reads. Table entries and timer values do not stand in for that chain.

Only normal drain exchanges the physical list-header owners and writes process
BYTE `0053B890=1` at `00479827`. No finally block swaps headers or clears
pending flags after failure. Startup reads the actual authored BYTE from the
executable, not BSS zero. `PortSession` retains this state across world and
screen replacement. Each world update clears it at `00478F44`, before the
disposal drain, so draw-time readers (selector 10's trails at `00425906`,
table `00520860`'s `00415D30` at `00415DF6`) see zero. In Infest01 those are the only
readers and writers.

Static visual `+48=00401390` reads retained process basis `00547590`; rotating
visual/actor `004026B0` uses its own `+44` basis. Startup `00439630` establishes
the shared initial identity once. This does not establish every later native
writer or arbitrary alias mutation; no per-object identity reset is added.

## Near class policies and common model lighting

| Actual admitted owner | Virtual `+2C` policy |
| --- | --- |
| Base / map annotation | `00409990` parks through the real Flag0 setter. |
| Visual / rotating visual / route node | Common visual `0047AFC0`. |
| Terrain light | `00461C30` reads actual `+4A0` gate before common visual. |
| [Rotating terrain light](OBJECT_RUNTIME.md#selector-54-rotating-terrain-light) | `00461C60` reads gate `+72` before common visual, whose `+48` basis is its own. |
| Selector 86 | `0042DF20` stores wrapped square of signed current far/2. |
| Selector 76, the [flamethrower](SELECTOR76_RUNTIME.md#draw) | `004323B0` turns the basis a quarter (`0047A720(0x4000)`, from the host's sine table), draws `0047AFC0` at the thrower and again at the root, then restores the position and basis. |
| Selectors 25 and 28, the [missiles](SELECTOR25_RUNTIME.md#light-draw-messages-and-contacts) | `00457620` is the common `0047D910`, except a live missile flagged `0x10` that a player holds (`0046B090`), which draws nothing: its player looks out of it through the [guided view](VEHICLE_DRIVE_RUNTIME.md). |
| Common actors, the [grenade](SELECTOR26_RUNTIME.md#draw), the mine and same-this wrappers | `0047D910` terrain expansion, flash, common visual, normal-only restore. |
| Selector 27, the [charge](SELECTOR27_RUNTIME.md#draw) | `00456180` puts a stuck charge back on its target (position through `004909E0`, basis through `00417DA0` and `00490370`), then the common `0047D910`. The host lends the live process null; without it the draw stops at `Charge27`. |
| Selector-2 vehicle | `00420510` parks on attract; offline ordinary drawing preserves signed view-mode, terrain tint, distinct pending model and normal-return stores. Network drawing remains required. |

`0047AFC0` model WORD `FFFF` parks before packet XYZ, basis or light reads.
Other models capture the actual packet, then fresh source flag `80` gates only
lighting. Settings DWORD `+80` belongs to grid accumulation/terrain readback;
it does not replace this object flag.

Fresh low-WORD X/Z, inherited grid anchors, spacing alignment, and masked
indices select signed RGB bytes in the actual X-major 90x90 grid. Outside
indices read authored DWORD `00526068`; inbounds grid reads bypass that
fallback load. Ambient channels are unsigned tint/4 plus signed grid bytes;
the directional channels clamp unsigned tint plus signed grid bytes to 0..255.
Ambient publishes before directional light using the actual normalized H12
direction. These renderer writes persist; furniture's tint/16 policy is a
separate consumer.

`ObjectModelPacket` carries source, model slot, signed XYZ and basis.
`ObjectModelDrawSubmission` adds explicit `ModelRootMode` and current context.
The backend lends fresh packed callbacks and process clock to either genuine
common `0049FEA0` or distinct manager late `004ED610`. Child selection retains
the captured source without confusing it with another H8 resource.

### Published model state and caller context

Common `0049FEF0` and manager late `004ED610` publish packet transforms
through `0048FB40`, which reads the published view basis, eye and coordinate
shift. The scoped object adapter retains that scene state; borrowing an
`ObjectDrawContext` does not publish its camera. Caller view still owns
manager admission, flash depth and successful custom-vertex eye rebasing.
Caller far color and primitive mode remain explicit invocation inputs.

Common `004A00E0` additionally reads global near/far `0055F248/4C`; Far
projection reads those bounds and factor `0055F244`. The adapter retains
this reached publication together. It never substitutes caller distances
while retaining a factor computed for a different interval. Manager late
retains its distinct world/control interpreter and acquires no common
Near/Far selection from this rule.

Shared terrain restores caller near/far at `0044B2A9/AC` without a final
`0048FB00`; its last reached global fade survives. Actor terrain expansion
and its normal restoration at `0047DA17` do publish fade, as do flash enter
and restore. Errors preserve the last reached publication and caller stores;
the model adapter adds no restoration.

## Ordinary vehicle and retained process profiling

The shared near callback selects the actual selector-2 actor, without model,
placement, campaign or level admission. Current attract BYTE nonzero parks
before tint/network reads. Ordinary entry saves the full current `005474E0`;
raw nonzero network DWORD `0055F448` reaches the explicit `VehicleNetwork`
boundary before the offline `+1C0` gate. Offline nonzero `+1C0` returns without
view-mode, terrain, model or `+228` reads/writes. Production world and briefing
borrow the same actual process network DWORD and mutable retained world tint.
The backdrop may copy presentation for its own reads; the manager cannot copy
its mutable tint custody.

Signed BYTE `+D0 > 0` uses an independent pending-model lane. Pending clear
skips packet construction but still reads direct `+140`; pending set submits
the actual source's current model, signed XYZ and virtual `+48=004026B0` basis
to ManagerLate `004ED610` first. Direct NULL `+140` or a NULL behavior `+00`
head skips `0048CDB0` itself. A nonempty head invokes the same retained
[drain/emitter](WEAPON_FIRING_RUNTIME.md) before the vehicle suffix. Neither
array emptiness nor virtual `+7C` presence proves an active chain. The enclosing
manager may subsequently invoke the now-empty behavior; that distinct call
alone increments its empty-behavior diagnostic. Emission failure preserves
the current node and earlier writes and prevents replay of that stopped chain.

Signed `+D0 <= 0`, including unsigned `80..FF`, captures unsigned Z and X WORDs
before lazy current H12 acquisition. Its host may mutate pose; the sample uses
the captured coordinates and Common later captures fresh ones. Existing
`TerrainRuntime::cell_at` reproduces fixed-shift7/mask alignment for spacing 128
and 256. The raw cell's high nibble, capped at 8, multiplies each unsigned tint
RGB byte and shifts right 3; the fourth byte survives. `0043A370` publishes only
that full tint word. Profiling begin `(13,1)` precedes the genuine Common actor
`0047D910`; profiling end and full saved-tint restoration occur only after its
normal return. Actual common ambient/directional reads therefore see this
published tint, independently of renderer ambient storage.

Both completed lanes store DWORD `+228=0` at `00420A13`, then reach the fresh
network tail gate. Constructor `004156E0` leaves `+228` unwritten: the retained
Option begins unknown and becomes `Some(0)` only at this actual writer. Reset
and update writers remain separate owners. Model/terrain/publication failure
preserves prior `+228`, reached tint, profile start, caller far/flash, source,
RNG, scratch and queue prefixes. Named per-object omission does not execute a
failed vehicle's normal-return suffix, with one exception: the saved tint
returns. Native always completes this draw, so an omission (the port's
unowned model value or callback) stands in for a normal return. Left shaded,
the world tint would compound on every later frame while the vehicle stands
on a cell with a shade below 8. The vehicle's model reads the track phase
`+1FC` (getter selector 3) every frame; until the
[track phase approximation](VEHICLE_DRIVE_RUNTIME.md#vehicletrackphaseapproximation)
gave it a start, that read stopped the model after its body, so the vehicle
drew without wheels, the truck barely at all, and the world darkened.

The process profiler uses native `index + 27*bank`, distinct from the scheduler's
`004875A0` world timing ring. Vehicle `(13,1)` uses slot 40 (start `00547308`,
elapsed `00547078`). Gameplay `00488020` surrounds its manager with `(19,1)`:
its immediate `13` is hexadecimal, so this separate slot 46 starts at `00547338`
and stores elapsed at `005470A8`. Briefing has no outer pair. Begin overwrites
one shared start QWORD; end subtracts that retained start modulo 2^64 and retains
it. Repeated same-slot begins never create a stopwatch stack. End is a normal
manager-return suffix before pending particles, not a finally block.

The supported portable storage domain is indices 0..26, banks 0/1 (54 slots),
covering the observed native constant callers. Native helpers themselves do
not check those bounds; arbitrary indices/aliases and bank 2 remain unsupported.
Elapsed's first 200 bytes at `00546F38` are original file data; the remaining 232
and all 432 start bytes have independent loader-zero proof. Startup initializes
once; `PortSession` retains the owner through world/screen/display replacement.
This does not claim every native profiler scope is connected.

Complete `004A4890` samples `WINMM!timeGetTime`, CDQ-sign-extends its DWORD
milliseconds, multiplies by 1000 modulo 2^64, and returns EAX=0. Its composed
`004923C0` therefore takes the NULL return without a pump callback. Windows
uses that same clock API. Other platforms use the explicitly replaceable
`ProcessProfilingClockEpochApproximation`: a process-local monotonic epoch;
the platform clock owner must close native epoch parity there. Both paths
retain signed conversion and wrapping subtraction. Clock cadence, all native
profiler consumers and complete retail scene/timeline acceptance remain open.

The [vehicle phase tests](../../crates/inf-game/src/object_runtime/object_draw/tests/vehicle.rs)
and [profiling tests](../../crates/inf-game/src/object_runtime/profiling/tests.rs)
pin complete native bodies, startup ranges, signed/coordinate branches and
normal/error state. Supplied service fixtures establish model-boundary order,
not a successful renderer or complete original frame.

The [loaded-world vehicle tests](../../crates/inf-game/src/world_scene_tests/vehicle.rs)
retain the original current H8 binding in tiers 2/4, supply explicit view and
D0 modes, and execute the shared software backend. Constructor `+1E8=0`
reads callback 1 as zero from `00530578`: original model 117's 27-instruction
wrapper reaches nine control/register instructions and skips all child models
118..127 before its terminator. Its complete 336-byte body is hash-guarded.
Common is admitted as Far in this fixture; ManagerLate intentionally submits
no visible packets. Normal return completes the actual `+228` and manager
suffixes. A supplied already-active process scratch scope produces the exact
hard `DrawState { expected: Ready, actual: Running }` backend failure and retains
tint, profiler, list, scratch, queue and RNG custody. This does not prove visible
vehicle geometry, a native camera/mode writer, or a complete retail frame.
The existing independent real-model manager queue-fault test owns queue-stop
coverage; no vehicle queue submission is invented to reach that failure.

## Actor terrain and flash nesting

`0047D910` captures actor XYZ before its second fresh terrain acquisition.
Positive excess expands the actual context far by four times the excess and
publishes fade using current near. Fresh C8 zero skips CA and flash. Nonzero C8
requires actual CA, computes full `(C8*32767)/31250`, and passes that quotient
to flash's signed LOWWORD consumer. It does not clamp the quotient.

Flash computes wrapped WORD eye differences in Z,Y,X dot order, then saves
actor `BC=near`, `C0=far`, `B8=color`, before its gate or H7 lookup. Depth at
least signed truncated `(3*far-near)/4` returns normally with these saves intact.
The other arm uses unsigned extent diameter, then actual H7 color. Signed
intensity LOWWORD, wrapping product/left shift, arithmetic shift15, and signed
result LOWWORD produce the correction. Store caller color, far=`diameter+depth`,
near=`diameter-correction+depth`, then publish fade. No persistent renderer
FarColor command belongs to this phase.

The registered H7 lookup consumes signed LOWWORD. Native index==count is an
unchecked backing boundary. Missing slots call actual vtable `005276B0/+00`
leaf `004EFBF0` (XOR EAX,EAX; RET4), which ignores the full argument and returns
NULL before the native dereference. The port requires current resident backing
and converts both RGB555 halfwords with the actual display layout. Missing,
negative and one-past resources remain typed per-feature omissions; they do not
become successful zero color or a same-numbered overlay fallback.

The common-model return rereads C8 and conditionally restores flash.
Restore reads/stores B8 color, BC near, C0 far individually before publishing
fade. A missing later save preserves earlier context stores. All three saves
remain actor-owned Option fields, constructor-unwritten until an actual writer
initializes them. The outer terrain far restoration is last. Native always
returns from the model, so a port omission of the model (an unowned model
value or callback) stands in for that return: the flash and far restorations
still run before the omission is reported. Host, terrain and object faults
keep their reached prefix. Left narrowed, a flashing actor's fog applied to
everything drawn after it: a drowning ACAM, flashing as it takes damage,
turned the terrain behind it into flat fog-coloured polygons.

## Omission policy and remaining acceptance

Strict `Flag0ObjectDrawFailurePolicy::Stop` retains every reached list, field,
context, callback and model prefix and returns the required boundary.
`UnownedObjectDrawCallbackApproximation` omits only the current object's
remaining callback phases, logs its actual source/reason, then reads the fresh
head. The world draw logs each new object/reason pair once, with its clock,
for the whole world, not just its first frame. It never claims normal late completion merely to clear pending or reach
behavior. Required packed values, custom vertices, model/resource backing and
flash material/storage remain named. Backend queue, publication, custody and
pixel/presentation corruption stay hard.

Only the typed command `PrimitiveError::Unsupported` host boundary enters
model-feature omission. Every other command host error remains hard. This type
also contains explicit native-fault and required-source bounds; it does not
prove that every reason is an absent native handler. The failed primitive can
leave earlier mirror/model payloads, actor writes and RNG intact. Local command
backend stop is distinct from queue stop; recovery still requires the actual
healthy queue and authenticated implicit root below.

`ObjectDrawGroupClosureApproximation` additionally closes the incomplete
object's healthy new queue groups through native `004A44A0` before admitting
the next independent object. Entry must be the actual implicit root; preexisting
caller groups, a closed root, stopped queues or aborted presentation stay hard.
Earlier payloads, offsets, RNG, caller context and list writes survive. It neither
replays/resets the queue nor claims a normal model return or clears pending.
The software statistic counts these closes and logs the first use. Strict mode
retains the exact open-group prefix. This early close replaces no native EF
execution and remains port containment until the missing callback is owned.

World pending particles (`00488193`) and terrain (`0044AA53/56`) read the same
caller context after the manager; reached near/far/color stores therefore feed
the pending request, and reached near/far feed terrain. Terrain's actual H7/74
color publication remains its own later phase.

The approximations are replaceable and do not count as retail acceptance.
The [family custom selectors2/10](OBJECT_MODEL_CALLBACKS.md#family-custom-vertex-selector-2)
write real retained group storage and actor height through the manager's mutable
arena loan. [Selector36 exact11](OBJECT_MODEL_CALLBACKS.md#selector36-exact11-capture)
sets its retained valid bit and captured position through the same source loan.
[Selector60 cached9](OBJECT_MODEL_CALLBACKS.md#selector60-cached9-capture)
retains its first reached invocation row in actual actor storage. These
successful arms rebase through the actual enclosing caller eye; later model
or simulation boundaries preserve the reached source writes.
Network selector-2 drawing, other captured custom vertex branches,
remaining [weapon factory and shot lifecycle limits](WEAPON_FIRING_RUNTIME.md),
unowned class/simulation writers and native aliases remain follow-ups. Packed
getters and current process-clock reads alone do not close those phases.
The [manager tests](../../crates/inf-game/src/object_runtime/object_draw/tests.rs),
[flash tests](../../crates/inf-game/src/object_runtime/object_draw/flash/tests.rs),
[late root tests](../../crates/inf-game/src/model_execution/late/tests.rs), and
[consumed world tests](../../crates/inf-game/src/world_scene_tests.rs) establish
bounded source/setup behavior. Release intro, briefing and gameplay smoke tests
check the consumed port. Complete matched retail scene/timeline comparison is
still required.
