# Process particles, effect birth and lifetime

This document owns the process particle pool, native birth/update/draw/teardown
consumers and the world timing-ring head feeding them. [INTRO_SEQUENCE.md](INTRO_SEQUENCE.md)
owns the selector-11 retirement caller. These services are reusable across
worlds and all six authored type selectors; no mission/OVL number selects
their behavior. [PARTICLE_FACTORY_RUNTIME.md](PARTICLE_FACTORY_RUNTIME.md)
owns the additional shared `004665D0/00466710/00466850` factories, live
creation notifications, resident packet sources and normal deletion of
`00526EFC/00526F20/00526F40/00526F60`.

## Pool and list custody

[process_particles.rs](../../crates/inf-game/src/process_particles.rs) owns
`00464A60`'s 400 allocation cells. Each cell is 72 bytes: an eight-byte header
and 64-byte particle payload. Native constructor `00464A60` allocates `7080`
bytes, initializes six active and six pending intrusive lists, and pushes
cells 0 through 399 through `00464E30`. First allocation therefore uses cell
399. Constructor-unwritten payload bytes remain unknown, and destruction
retains their last written values for the next use of that cell.

Production startup applies **FreshParticlePoolZeroApproximation**: every
payload byte starts as a known zero, except cell 32's `+00..+17`. In
Infest01 the block comes from `00495940 → 0051246A` (returned at
`2FD49:4CC`) and is zero apart from leftover heap words in cell 0's header
and cell 32's first 24 payload bytes
(evidence).
Only a cell's first occupant sees these zeros; the heap history before the
allocation is not replayed, so another session's leftovers could differ.
Native receipts keep the unknown pool of `ProcessParticleRuntime::default`.

`00466100` passes ECX size and EDX signed reclamation-category bound to
`00464ED0`. The native allocator receives but does not inspect size. Nonzero
process `0055F448` bypasses the phase query; otherwise `004391B0` supplies the
current manager and its phase `+64C` must equal 4. Gate failure returns the
nonnull scratch record `0054EC60`, without popping a free cell or clearing
an existing allocation marker. Scratch is in the original virtual zero-fill
region; constructors execute against it even though it is not admitted to a
pool/list when the marker is null.

On exhaustion, only the last active particle in each category is eligible:
`00464ED0` reads the active tail sentinel's previous-node pointer at `+14`.
The scan starts at category 0 and includes the signed bound. Earlier
categories skip the flags read; the limiting category tests only BYTE `+12`
and rejects a tail with bit `80`. Unknown BYTE `+13` does not prevent that
decision. Pending births are not candidates. A chosen particle's scalar-deleting
virtual is called before the allocator pops the returned free cell. An
unknown destructor is an explicit feature error. Owned base, central, debris,
trail and splash destructors unlink through `00465050`, write the base vtable and
return the physical cell through `00466110 → 00464EC0 → 00464E30`.
The additional factory classes use their proven normal wrappers; `00526EFC`
can require impact work before base unlink, while intermediate `00526F40`
uses the same normal base deletion as `00526F20/00526F60`. The
[factory authority](PARTICLE_FACTORY_RUNTIME.md#per-class-normal-deletion-and-failure-prefixes)
owns these branches and flags 0/2 allocation retention.

A successful allocation records its header at manager `+128`; the cell
occupancy pointer is still null. Base constructor `00464FB0` writes XYZ,
WORD lifetime `+10=2000`, WORD flags `+12=(low_word(flags)|009C)`, source
ActorHandle `+14`, self/list userdata `+20` and its base vtable. The source is
the supplied ActorHandle; existing intrinsic birth callers use process-null
`0054E894`, while the shared factories retain their actual source argument.
It is not a terrain pointer, and the owned intrinsic collision consumers do
not read it. The constructor then
calls **`00464E50`**, which publishes the particle only when the marker is
nonnull and clears that marker. The native list/self addresses are represented
by portable custody rather than invented pointers in the payload bytes.
Numeric reads of native `+18/+1C/+20` pointers reject explicitly and require
that custody, including scratch's original zero-filled storage.
Flag `0100` then adds flag 1. `ParticleConstruction` uses six-byte
`ParticleVectorBytes` with independent known masks: the native four-byte then
two-byte XYZ/velocity copies propagate available unknown data without
inventing zeroes. A nonnull velocity pointer copies all three WORDs at
`+0A/+0C/+0E`; a null pointer retains reached allocation bytes. Numeric
consumers require only their used spans. Captured same-position and separate
source modes, late unavailable-source prefixes and mutable notification
custody belong to the [factory authority](PARTICLE_FACTORY_RUNTIME.md#captured-requests-and-live-storage).

`00464E50` prepends to the pending category. `00464DA0` visits categories in
order, removes each pending head and appends it to the active category.
Its active `+14` anchor is the tail sentinel's previous node, retaining older
active particles first and the transferred group's reverse birth order.
`00464F60` clearing visits occupied physical cells in ascending index order,
directly unlinks their list nodes and returns each physical header through
`00464E30`. It does **not** invoke virtual destructors or overwrite payload
vtables. Even an unknown class is cleared, retaining every payload byte.
The released header belongs to the physical cell, including the unusual
case where a retained allocation marker published scratch as its occupant.
Expiration and exhaustion instead execute the scalar-deleting virtual.
Process pool custody must survive world replacement.

Native Session constructor `00438640` contains G at `+64`, with this manager
at G `+80`; it is not a phase-4 screen constructor. Pool constructor
`00464A60` temporarily registers `0054ECB4` through `00464B10`, initializes
the free stack, then unregisters through `00464B20`. Later Session publication
`00487180` installs G and its particle manager. `PortSession` retains one
pool across screen changes and world replacement. Its world-clear path calls
the owned direct clear and removes per-allocation failure markers; it does
not reconstruct the pool or invent a screen-switch unregister.

## Birth consumers

[births.rs](../../crates/inf-game/src/process_particles/births.rs) owns
`0048A3A0`, `0048A0F0`, `0048A720` and `0048A8B0` with explicit current
phase/network, a borrowed live process owner, original executable data and shared RNG.
Requests represent actual producer arguments; table choice is intrinsic to
the type selector, independent of scene presentation.

### Implicit process source and late failures

`ParticleBirthInputs` borrows the retained
[process state](PROCESS_ACTOR_STATE.md). Its `process_null_handle` supplies
the source DWORD at each reached producer read, independently of the actor
registry. Known zero is a valid value; unavailable storage is a typed
`ParticleBirthSourceError` naming the producer and original read address.
It is a portable required-input stop, not an observation of a native fault.

| Producer | Reached `0054E894` read | Earlier allocation |
| --- | --- | --- |
| Central `0048A0F0` | `0048A215`, once per instance | `34h` bytes, category 1 |
| Debris `0048A3A0` | `0048A435` | `3Ch` bytes, category 1 |
| Trail `00489D90`, types 0..11 | `00489EB2` | `34h` bytes, category 1 |
| Trail type 12 | `00489F3A` | `34h` bytes, category 1 |
| Trail type 13 | `00489FBE` | `34h` bytes, category 1 |
| Splash `004893A0` | `00489409` | `34h` bytes, category 1 |

All these reads follow allocation and precede their common/special constructor.
Unavailable source therefore retains the popped free cell, allocation marker,
reclamation and earlier sound/RNG effects. Constructor payload/list writes and
later producer RNG have not run. Phase failure occurs earlier at allocation.
Phase-gated scratch is still nonnull, so it also reaches the source read and
retains its previous payload when that input is unavailable. Invalid unsigned
selectors return before allocation/source; trail 2/3 consume their table-choice
RNG before allocation. Central 1/2 enqueue sound before their first allocation
and read source separately for all eight instances. Burst wrappers retain their
earlier vector cursor/RNG work before calling these lower producers.

A native zero allocation result bypasses the source read. The represented
allocator admits Pool or Scratch; it has no invented NULL-failure mode.

Intro retirement, manager tick, briefing drawing and software world drawing
lend this same owner. Their former registry-source preflights are removed.
Nonemitting ticks, expiry, pending transfer and drawing can proceed without
source storage; only a reached producer needs it. A later furniture/terrain
failure retains earlier model/draw/audio work and omits the reached feature.
Intrinsic birth consumers retain actual current session phase as a genuine
request operand, independently of process network/NULL storage. Shared factories
have a different read contract: after successful synchronous notification and
the fresh offline gate, their mandatory host getter reads the actual manager.
The [factory authority](PARTICLE_FACTORY_RUNTIME.md#selection-notification-and-construction-order)
owns that lazy call and its normal non-4 scratch suffix; no process phase copy
or temporary assignment supplies it.

Intrinsic birth calls borrow process state only for their reached constructor.
The update manager's `ParticleUpdateBirthInputs` retains a mutable loan of that
same process owner, plus the genuine phase/network request operands. Its
`borrowed()` method creates a short immutable birth loan at the actual producer
call. Collision/deletion hosts therefore can mutate the retained process between
births without cloning it or retaining stale NULL-handle snapshots. Audio-only
intrinsic birth callbacks still cannot mutate process storage during that birth.
`ParticleConstruction.source_handle` and shared factory source arguments remain
explicit genuine inputs, with their existing independent policies.

### Authored producer programs

`0048A3A0` accepts types 0 through 5, returning native null for any other
unsigned selector before allocating. Its animation tables are `0053D328`
(type 0), `0053D348` (type 3), and `0053D338` (types 1/2/4/5). The actual
table WORD `+0C` supplies initial lifetime minus one. Three reached RNG draws
set scale `+2C`, extended lifetime `+10`, and fade `+38`; `+30=256`. The
type-specific `+34` counter is 2, 2, 3, 2, 6 or 12; fade `+3A=150` only
for type 5 and zero otherwise. Final vtable is `00528934`. Scratch births
are still nonnull and consume those same draws.

`0048A0F0` accepts types 0 through 5, using original tables `0053D4E0`,
`0053D4F0`, `0053D270`, `0053D5E0`, `0053D5E0`, `0053D2B0`. Types 1/2
first enqueue required positional sound through `00487F10` with selector 6,
original XYZ, gain/pitch `0x10000` and flag zero. `0047FF60` copies these into
the process sound list. The following `004923C0` destroys a nullable returned
error object; it is not playback. Their eight instances each consume five RNG draws.
Other types produce one instance with one draw. Null source velocity explicitly
copies zeroes here, unlike the base constructor's null policy. Final vtable
is `00528914`, `+30=0`, and `+2C` retains the native type scale and random
offset. Type-1/2 lifetime multiplication wraps as a DWORD before signed
division and arithmetic shift.

Central XYZ is copied before sound. Each birth uses the previous iteration's
jittered local XYZ, then rereads original XYZ to produce the next jittered
copy. The eighth iteration still consumes its three final jitter draws, even
though no ninth instance uses that copy. A missing sound service fails after
earlier debris admission rather than fabricating the central effects.

`0048A720` uses signed arithmetic shift of the wrapped count/load-factor
product. A zero result falls back to the authored count; negative results do
not enter the loop. It clamps placement radius to at least 16, alternates the
two debris selectors, and increments retained process cursor `0054EC5C` as
a **wrapping DWORD**. Only lookup uses `cursor % 100` into original six-byte
vector table `00539960`; the cursor itself is not normalized modulo 100.
Velocity uses low-WORD products and signed WORD shifts, including upward Y
reflection. Position offsets use the low signed WORD of the vector's summed
absolute components. Invalid debris type ends the debris loop on native null;
the following central producer still runs.

`0048A8B0` snapshots XYZ before debris and then submits the central type from
record `+0C` with null velocity and flags `+1C`. Neither producer reads its
unwritten record WORD `+0A`; no value is synthesized for that field.

### Oriented pickup effects

[oriented.rs](../../crates/inf-game/src/process_particles/oriented.rs) owns
`00488E90`, called by the [timed pickup draw](PICKUP_RUNTIME.md#draw-and-collection).
Its above-plane branch uses signed strength/load products and the native
count clamp1..8, transforms a signed WORD speed through the supplied basis,
and calls `00489920`. The below/equal-plane branch retains its distinct signed
count and vertical-direction policy, uses the original scalar `00520A64`,
and consumes four RNG draws per bubble before real `00489600` birth. The
basis trigonometric terms follow the actual X/Z component expressions rather
than an inferred symmetric rotation. Both paths continue the loaned process RNG.

[oriented/births.rs](../../crates/inf-game/src/process_particles/oriented/births.rs)
owns three additional animation constructors:

| Birth / final class | Ordered producer policy | Lifecycle |
| --- | --- | --- |
| `00489920 / 00528834` falling drop | Three velocity-jitter draws before allocation; category1, table53D7B8; scale and extended-lifetime draws after construction | Gravity/motion48B0E0, shared terrain collision, contact488DC0; draw466D40 is actual RET4 |
| `00489CB0 / 00528894` material dust | Table53D840 selector lookup before allocation; category0, random scale and shortened lifetime | Motion48B110, false collision, common466CD0 draw |
| `00489A90 / 00528854` pickup flash | Authored selector/table/base-scale matrix; invalid selectors return NULL before allocation; NULL velocity preserves allocation bytes | Stationary4027B0, false collision, common466CD0 draw |

All three use the real pool/scratch and current process-NULL source at its
native construction phase. Their scalar deletion uses `00466D50`. Falling
drops are intentionally invisible; no approximate sprite replaces RET4.

[update/drops.rs](../../crates/inf-game/src/process_particles/update/drops.rs)
owns contact `00488DC0`: non6 calls `00489160` using the absolute truncating
half of signed Y velocity; code6 calls `0048A8F0`. Ground dust samples the
live terrain/material and emits real `00489CB0` particles. Water emission
retains its signed count, per-axis RNG-before-source-read order, native wave
formula and second ground query before splash1 birth. Its rare audio branch
reads the newly born splash XYZ after the subsequent RNG draws. Scratch births
may overwrite the contact source; later reads use that current record, not a
copied position or velocity.

The [31 birth cases](../../crates/inf-game/tests/fixtures/pickup_particle_birth_native.json)
execute full native constructors/pool/RNG/list work. The
[24 contact cases](../../crates/inf-game/tests/fixtures/pickup_drop_contact_native.json)
execute the actual contact, height, dust/splash births and scratch reuse on
supplied flat H12/H15 data; only the successful audio enqueue is supplied.
The [orientation receipt](../../crates/inf-game/tests/fixtures/pickup_lifecycle_native.json)
adds 13 producer cases with explicitly supplied terrain/birth returns.
Their tracked birth, contact and lifecycle generators are listed in
NATIVE_RECEIPTS.md.
Rust consumers compare known payload bytes, unknown-byte retention, pending
lists, cursor, source, audio requests and final RNG at FPCW027F. These bounds
do not claim arbitrary terrain scenes or complete visual acceptance.

### Attached short smoke

`00489BC0` admits selector0/table53D830 and selector1/table53D820, rejecting
other selectors before allocation. It requests category1/size38, copies no
velocity, captures actual process NULL at489C29 and initializes shadow30=0,
counter34=1 and random scale `(draw_low16 >>9) + 3C0` before final class528874.
Its update `0048B0F0` is the complete18-byte counter decrement: low32 subtraction
then a signed-negative return. It ignores delta and does not loop an animation
or read unwritten velocity. Thus one ordinary update survives and the next
retires it, unless the manager expires it earlier. Its actual table selects
false collision402720, common466CD0 draw and normal466D50 deletion.

The [short-smoke fixture](../../crates/inf-game/tests/fixtures/short_smoke_native.json)
contains nine complete native births, seven complete counter leaves and original
vtable words. The tracked generator compares these
bytes by default. Its [consumer](../../crates/inf-game/src/process_particles/update/tests/short_smoke.rs)
compares pool/list/source/RNG, preserves NULL-velocity carry, and separately
runs two real portable manager updates through ordinary release. This is the
particle service used by the separately owned selector66 attachment lifecycle.

### Furniture terrain flashes

Furniture callback `00437760` calls `0048AD70` with the live XYZ, authored
selector and zero flags. Selectors 0/1 choose executable tables `0053D610` /
`0053D620`; other values return before allocation. The category1 producer
allocates `34h` bytes, reads process NULL at `0048ADD9`, constructs the shared
animation prefix, consumes one RNG draw for scale `(low16 >> 9) + 448`, and
installs final class `00528974`. It leaves velocity unwritten. Ordinary update
and collision callbacks return false; shared lifetime/deletion remain active.

Its `0048AE60` draw emits terrain light without a sprite or projection test.
Table `0053D620` fades each signed frame RGB byte by unsigned lifetime/total
and grows radius from 256 through the native signed division. This arm calls
`0045B1A0` directly, including when flag40 is set. Table `0053D610` instead
computes its radius operand through the original mirrored sine lookup, whose
WORD is duplicated into both halves of a DWORD before the signed shift. It
passes the signed low WORD amount and frame RGB through `00465720`, retaining
that helper's flag40 and cached/fresh terrain-floor policy. Zero divisors stop
the reached effect explicitly. The source is
[light_effect.rs](../../crates/inf-game/src/process_particles/draw/light_effect.rs).

check_terrain_flash.py reproduces the 16 birth
and 36 draw records in `terrain_flash_native.json`. With Python 3 and Unicorn 2
installed, run `python check_terrain_flash.py`
from the repository root to compare the existing fixture; `--output <path>`
writes new LF JSON explicitly. Original pool/list/RNG, frame selection and
floor/light-argument bodies execute; only the final terrain-light sink is
supplied. Mutable RGB/total and constant terrain inputs are recorded. These
receipts cover failure prefixes and no-replay containment, not rendered light
grid pixels or complete intro parity.

### Impact sparks

The actor-pair kernel `0047B610` calls `0048AB10` with the radius-weighted
midpoint and strength `energy / 13333` (see
[world contact](WORLD_CONTACT.md#actor-pair-kernel-0047b610)). It first
enqueues sound `0x47` at the midpoint through `00487EC0`, with gain `0x8000`,
pitch `0x10000` and flag 0, then runs `004923C0` on the returned error. A
strength of zero or less (signed `JLE`) stops there. Otherwise, for each of
`strength` sparks, it:

1. draws three offsets, so each axis of the spark is the midpoint's plus
   `(rng & 0x1F) - 0x10`, as wrapped WORDs;
2. draws three more for a velocity that `0048ABD0` never reads;
3. calls `0048ABD0` with type 0 and flags 0.

`0048ABD0` returns NULL for any nonzero type, which no caller passes. It
draws three jitter WORDs `(rng & 0x3F) - 0x20` before allocating a category-1,
34h-byte particle. It then reads process NULL at `0048AC59` and constructs the
particle with the jitter as its velocity. Table `0053D2B0`, lifetime
`+0C - 1`, `+30 = 0` and the scale `(low16 >> 9) + 0x1C0` from one more draw
all match central type 5. The final class `00528954` differs from `00528914`
only in its update slot, so it shares that class's draw, collision and
deletion.

The update `0048ACE0` treats the lifetime WORD shifted left by 7 as an angle.
It reads sine and cosine from the mirrored quarter table `0053DF00`, negates
them in the upper half and shifts them right by 10, which keeps each within
±32. The sine is added to velocity X, the cosine to Z and their difference
(sine minus cosine) to Y, all as WORDs. `00465660` then integrates the motion
and returns 0. The port's owners are
[births.rs](../../crates/inf-game/src/process_particles/births.rs) and
[update.rs](../../crates/inf-game/src/process_particles/update.rs).

check_impact_sparks.py runs the unchanged
`0048AB10` over 11 cases: strengths from negative to 25, a wrapped midpoint,
both phase gates, edge seeds, and a 410-spark pool exhaustion into scratch.
It also runs the real `00464B30` walk over three sparks for five delta
histories through expiry and release. It calls `0048ACE0` directly over all
512 distinct angles and five high lifetimes. The
[Rust comparison](../../crates/inf-game/src/process_particles/update/tests/impact_sparks.rs)
matches every record, list, free count, RNG state and enqueue argument.
Each of 14 single-point source mutations fails it: the jitter mask, the
position offset, the scale, the unread draws, the strength gate, the gain,
the final class, both folds, the cosine phase, the axis order, the Y
difference, the shift and the angle.

## Update, collision and teardown consumers

[update.rs](../../crates/inf-game/src/process_particles/update.rs) owns
`00464B30`, whose caller runs pending transfer separately. It visits fresh
occupancy pointers for physical cells 0 through 399, including admitted
particles still pending. Birth or reclamation during a callback can replace
the current payload or populate a later cell, which receives its update in
this same walk. A birth into an already visited cell waits until next tick.
No active-list snapshot substitutes for this lookup.

Before that walk, signed `delta*300000 >>31`, retained as signed WORD,
publishes gravity `0054ECB0`. Motion operand `00539958` is 10,000 below signed
delta 10,000; otherwise it uses wrapped `((7FFF-load_factor/2)<<16)` and the
signed high-product shift with `(delta-10000)`, adds 10,000 and falls back to
delta if negative. Its retained initial value is the executable's DWORD
10,000; gravity starts in original virtual zero-fill. These globals do not
replace the actual delta argument passed to the class update.

Lifetime `+10` is unsigned WORD, but `lifetime-(delta >>10)` is wrapped DWORD
tested as signed positive. Expired or flag-`0040` particles skip lifetime
publication and update/collision. Otherwise the manager clears `0080`, writes
low-WORD remaining lifetime and snapshots XYZ before class `+4`. On zero
return it computes wrapped movement WORDs, clears height-cache flag `0020`
and calls the current payload's `+0C`, rereading its vtable after possible
reclamation. Nonzero update/collision or expiration invokes `+1C` and the
scalar-deleting virtual. All owned classes except `00526F20` have actual
return-only `+1C=00401310`, not a substitute host callback; `00526F20`'s
retirement `004661B0` is described [below](#factory-classes-00526f20-and-00526f60).

| Class vtable | Update owner | Collision owner |
|---|---|---|
| `00528914` central | `0048A390 → 00465660`: signed velocity times signed `delta >>5`, wrapped DWORD product, arithmetic `>>15`, low-WORD XYZ addition | Actual constant false `00402720` |
| `00528954` impact spark | `0048ACE0`: sine and cosine of the lifetime angle, `>> 10`, added to the velocity WORDs, then `00465660` | Actual `00402720` |
| `00528934` debris | `0048A550`: nonfading motion/trails; fading gravity, distance counter and trails | `00488CC0 → 00465550 → 00465110`, terrain/water only |
| `005288F4` trail 0..11 | `0048A060`: three velocity RNG draws, optional flag-`0800` scale growth, then integration | Actual `00402720` |
| `005288D4` trail 12 | `0048AF70`: cached floor, gravity-scaled Y settling/clamp | Actual `00402720` |
| `005288B4` trail 13 | `0048AFD0`: per-axis deceleration, Y clamp before X/Z motion | Actual `00402720` |
| `005287B4` splash | Actual constant zero `004027B0`; null-constructor velocity remains unread | Actual `00402720` |
| `00526EFC` projectile | `00466250`: additional signed `delta >>5` lifetime decay when flags `000C` are zero, then `00465660` | `00465FA0`, through required live collision host |
| `00528814` impact bubble | `004896E0`: water crossing/splash or buoyancy, drag and `00465660` | Actual `00402720` |
| `005287F4` ground material sprite | `0048B0E0 → 00465620`: full/half gravity then `00465660` | Shared terrain classifier followed by actual `00488D10` ground response |
| `00526F20` factory `004665D0` | The same `0048B0E0` gravity update | `00465FA0`, terrain response `004661F0` |
| `00526F60` factory `00466850` | `00466980`: selector 8 lengthens lifetime and leaves a trail, then `00465660` | `00465FA0`, terrain response `00466A00` |

[projectile.rs](../../crates/inf-game/src/process_particles/update/projectile.rs)
owns both added update kernels. Projectile extra decay clamps its wrapped signed
DWORD result to zero before storing low WORD. The manager's earlier lifetime
write remains separate, and collision still follows even if extra decay reaches
zero. Impact bubbles measure water with fixed radius 10 but never write the
classification flags used by collision. A crossing writes Y and births a real
`004893A0` splash before returning true. Below water, Y velocity adds low WORD
of signed `delta >>10`; X/Z each subtract low WORD of the signed velocity times
`delta >>2`, wrapped as DWORD then shifted right 15, before ordinary integration.
The normal `00466D50` bubble destructor reaches base unlink/release.

The projectile radius consumer uses the current animation frame and scale before
lending movement, radius, mutable terrain/process, pool and RNG to
`ParticleUpdateHost::collide_projectile`. Retirement similarly lends the live
owners to `delete_projectile`. Their default implementations retain typed
required-collision/impact boundaries; a sound-only host cannot report a hit or
impact as successful. The full shared collision/effect authority is
[PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md).

### Factory classes 00526F20 and 00526F60

The [factory authority](PARTICLE_FACTORY_RUNTIME.md) creates both classes.
They share the projectile's collision `00465FA0`, its radius and its actor and
furniture responses `00466080`/`00466CF0`. Only virtual `+10`, which
`00465110` calls as `(selector, height, movement, radius)`, differs:

- `004661F0` (`00526F20`): a contact other than water (6) stores the height
  in Y and negates Y velocity `+0C`, then returns false. Water takes the
  projectile's own sound, height and splash arm.
- `00466A00` (`00526F60`): water and selector 7 return true, which ends the
  particle. Any other contact stores height plus radius in Y; a negative
  `+0C` becomes `1 - vy/4`, with the division truncated toward zero.

`00466980`, the `00526F60` update, tests selector `+3C` against 8 first. At
8 it adds signed `delta >>12` to lifetime WORD `+10` and births trail 13 at
the current position, with each velocity WORD times 3 divided by 4
(truncated). Selector 9 skips both. Integration `00465660` follows either
way. `00526F20` uses the material sprite's gravity update.

No constructor writes `00526F20`'s shadow `+30`, which `00466CD0` reads.
Infest01's only drawn instance (cell 304) reads 0 there; the fresh-pool
approximation gives a never-used cell the same zero.

`00526F20`'s `+1C` is `004661B0`. It runs after the manager's expiry or a
true collision and before the scalar deletion. Selector 1 spawns object
`0x36` through `00460310` at the particle; the port stops there with
`ParticleUpdateError::RetirementObject`. Any other selector births central
particle 4 there, with no velocity and flags 0 (`0048A0F0`).
`ProcessParticleRuntime::retire_factory_particle` owns that arm; a host lends
it the positional audio for the birth.

Infest01 creates 15 `00526F60` and 2 `00526F20` particles. All 55
`00466980` updates take the selector-8 arm, `00466A00` answers 5 contacts and
`004661F0` none, and the one `004661B0` retirement takes the central arm
(counts).

### Per-allocation failure containment

Before this containment change, the native manager decision matrix was:
expired or flag-40 cells go directly to deletion; update true skips collision
and deletes; update false calls collision, whose true result deletes. Earlier
writes remain committed, pending cells are visited, births into later physical
cells can tick immediately, and ordinary unadmitted scratch has no cell to visit.
Those decisions and physical visitation order are unchanged on successful paths.

`ParticleUpdateReport` records each newly reached error with particle identity
and `Update`, `Collision` or `Retirement` phase, then continues other physical
cells. `ParticleStopPhase` is a named portable containment policy: a failed
allocation retains its exact prefix and skips later updates/draws until destruction,
reuse or world clear removes the marker. Draw errors use the same owner with
phase `Draw`; later manager draws count the stopped allocation and skip it
without repeating omissions or resubmitting work. A direct draw request retains
the typed `PreviouslyStopped` boundary.
This is replaceable containment for missing services, not native resumable
exception behavior. In particular, a failed streak retains its shifted XYZ;
silently restoring or integrating that partially drawn particle would falsify
the reached prefix. New allocations clear old failure custody.

Nonfading debris emits a trail when the wrapped signed sum of velocity squares
exceeds 40,000, using `+34` selector, current XYZ and velocity `(0,64,0)`.
Fading debris subtracts full gravity when flags `000C` are nonzero, half
gravity otherwise. It subtracts absolute signed low-WORD movement from
`+38` after each axis; a negative signed counter emits a trail and consumes
one later draw to add `(rng_low * signed_WORD(+3A)) >>16`. Trail birth can
reclaim the same cell, so that later counter read uses its current payload.
Type-5 debris retains its selector 12 policy; all fourteen `00489D90` trail
selectors remain independent from levels.

[collision.rs](../../crates/inf-game/src/process_particles/update/collision.rs)
owns current animation-frame radius `004657E0`/`00488CC0` from actual tables,
unsigned lifetime, signed frame size and wrapped scale products. `00465110`
compares signed H12 plane, its scale-dependent margin and particle top/bottom
to classify water. Near the plane it uses aligned raw H12 height, live H14
wave flag and clock with the shared `00410F60` owner. The first coarse
comparison retains the full signed shifted DWORD; its second read supplies
signed low AX to the water formula.

Classification writes flags `000C` before ground queries. Ground proximity
uses signed low-WORD raw height and interpolated `00450A30`. A contact caches
`+24` through `004656A0`, setting flag `0020` before sampling; flag `0010`
selects interpolation, otherwise the aligned cell. Ground impact material
is sampled separately at wrapped `(X+64,Z+64)`, indexing the four actual H15
DWORDs in H15 slot 0, aliased by `00548394`. A transition from old class 2 to
a class below 2 reaches the fifth H15 DWORD and water height. `ParticleTerrainContext`
carries resident H12, actual H15 selectors, H14 flags, live clock and original
sine table; no mission-number rule selects these operands.

Debris response `0048A6B0` first writes Y. Non-6 codes reflect negative Y
velocity as truncating `-3*velocity/4`. Code 6 consumes the selection draw,
conditionally reaches positional sound 9/10 at volume 65,536 and random pitch,
then produces real `004893A0` splash type 0. Required enqueue `00487EC0` and
nullable error cleanup `004923C0` are `ParticleUpdateHost` effects; missing audio
retains earlier contact writes/RNG and fails before the splash. Both response
branches return native false. This debris class never calls actor/furniture
collision `00465570`; those remain separate class owners.

Birth and impact hosts share `ParticleSoundRequest`, retaining the copied
world WORDs independently of particle lifetime. `00480110` services the sound
list later in world draw `00488020` with its current listener, applying signed
wrapped relative coordinates, native attenuation and pan before PCM start.
`004800E0` then removes one-shot queue records; it does not stop their emitted
PCM voices. The producer checker supplies a successful enqueue and executes
the actual nullable `004923C0` path; its XYZ/selector/gain/pitch/flag pins do
not establish that queue executor or sound playback.

[producers.rs](../../crates/inf-game/src/process_particles/update/producers.rs)
implements actual trail/splash allocation and constructors. Trail 2/3 consume
the table-choice draw before allocation, then scale RNG after publication.
Types 12/13 retain distinct tables/vtables. Splash's null velocity preserves
allocation bytes, and its proven stationary update leaves them unread.
Invalid unsigned selectors return null before allocation/RNG.

## Draw and terrain-light consumers

[draw.rs](../../crates/inf-game/src/process_particles/draw.rs) owns
`00464C80` and the shared central/debris/trail/splash draw virtual
`00466CD0 → 00465810`. `ParticleDrawRequest` explicitly selects active or
pending banks and supplies the renderer context, published view, projection,
fog/far color and resident terrain. Categories 0 through 3 use an identity
object basis at the renderer position; category 5 uses its full basis.
Category 4 is skipped. Native camera publication `00410D90` supplies the
same position/basis to the renderer context and view globals; the request
keeps these operands distinct for other callers.

Projectile `00526EFC` has a distinct draw virtual `004663E0`, implemented by
[draw/projectile.rs](../../crates/inf-game/src/process_particles/draw/projectile.rs).
It normalizes signed WORD velocity with native CW `027F` precision: length
spills to float32, X remains double precision while Y/Z normalize through
float32 spills. Zero velocity uses executable constant `00520A70=1` for X.
Each normalized component times `00526F1C=8` passes through original low-DWORD
truncation and low-WORD storage. The wrapper calls shared `00465810` ten times,
advancing Y/Z/X before calls 2..10, with fresh table/scale reads between writes.
It supplies shadow zero explicitly because the projectile constructor leaves
`+30` unwritten. Only ten successful returns restore original XYZ; callback
mutation of velocity does not change the already captured step. Impact bubble
`00528814`, ground material sprite `005287F4` and factory class `00526F20` use
the ordinary `00466CD0` animation path. Factory class `00526F60` draws through
`00466960`, the same `00465810` call with a literal zero shadow instead of
`+30`. Their birth/contact ownership is in
[PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md).

[animation.rs](../../crates/inf-game/src/process_particles/draw/animation.rs)
shares original `004657E0` frame addressing with collision:
`base - 10 * ((unsigned_rate * unsigned_lifetime >> 14) % unsigned_count)`.
The 16-byte animation table retains unsigned count/rate/size/lifetime,
signed sort bias and flags. Its ten-byte frame supplies signed H3 sprite slot,
signed light amount, three signed RGB bytes and signed size scale. Quad size
uses wrapped DWORD products, the sprite's unsigned dimensions, arithmetic
shift and signed division by `depth << 9`; division faults remain errors.
Table flags retain minimum-size, horizontal/vertical flip and center-versus-
bottom anchoring policies. Particle `+30` enables the shadow branch; it is
not a vertex-depth bias.

Destructive projection rejection publishes flag `0040`, unless protected by
flag 2; null animation tables also publish `0040`. Later quad rejection does
not. Reached frame lighting precedes the final far rejection, sprite lookup,
flag `0010` and queue allocation. `00465720` uses cached/interpolated floor
for positive light amounts, or lifetime-scaled wrapped arithmetic for negative
amounts. Its signed world X/Z, radius and RGB reach the required
[world lighting writer](../../crates/inf-game/src/world_lighting.rs), which
retains the actual settings `+80` gate, terrain spacing and signed saturation.
The world clears the grid and publishes the camera-derived anchor before
the active pass; terrain consumes the resulting shared grid afterward.

The [SoftwareScene bridge](../../crates/inf-game/src/software_scene/particles.rs)
borrows actual resident H3 textures and submits three-block deferred packets
to the existing queue and Near `00466150`/Far `00466160` quad consumers.
Vertex depths use projected depth plus the signed table bias. Far packets
also carry converted far color and four repeated fog bytes. Material sorting
retains the transparent/depth versus opaque/physical-entry-address policies.
Unknown vtables, unavailable sprites, lighting and queue failures omit only
the reached particle, preserving earlier mutations and submitted commands.

[draw/shadow.rs](../../crates/inf-game/src/process_particles/draw/shadow.rs)
owns the `00465C96` suffix reached when signed particle `+30` is nonzero and
animation flag 8 is set. It follows the main textured packet, measures or reuses
cached terrain floor, computes signed `half_width * shadow_amount *2 / scale`
with wrapped products and a one-unit zero-width fallback, then resolves H7/32.
The second sorting query uses `depth + signed_table_bias +1`; sorted shadows
use the native transparent/depth key. Two endpoints use the retained renderer
X basis, original main depth and delta, with Y replaced by relative floor.
Each endpoint is projected separately, retaining its own depth; signed screen
comparisons select the one-pixel quad thickness. Near `00466170` and far
`00466180` packets retain converted material color, while far packets also
carry original main-projection fog and converted far color.

[d3d_solid_quad.rs](../../crates/inf-game/src/d3d_solid_quad.rs) shares the
packet reader and Direct3D state/triangle policy while keeping native near
`004D1BB0` and far `004D2170` color arithmetic distinct. Far CPU fog expands
each fog byte to the range 0..256 before mixing five-bit material channels;
it does not perform the near path's low-three-bit replication. The production
host converts the actual resident H7 material through the renderer color layout.
Missing H7, terrain, division or submission services retain the main packet and
any reached cache writes, then stop only this allocation.

## Timing and production attachment

[timing.rs](../../crates/inf-game/src/process_particles/timing.rs) owns the
timing head of `004875A0`. Constructor `00487BE0` writes eight 125,000-us
samples, sum 1,000,000, average 125,000, elapsed milliseconds/cursor zero and
process load factor zero. Current/alternate delta comparison is signed;
ring sum is wrapped DWORD arithmetic and average uses logical shift by 3.
Below unsigned average 34,464, load factor is 65,536; otherwise it is
`100000-min(average,100000)`. An alternate delta greater than current delta
seeds all eight samples and publishes factor zero. Elapsed milliseconds use
signed division by 1000. Cursor increments before FPS division; zero average
retains all reached ring/elapsed/load-factor writes and fails before FPS
publication. Process FPS before that first publication is not initialized
by this world constructor and remains explicit.

The timing ring is production-attached through `WorldFadeState.performance`
and `Session::tick(delta,elapsed)`; [WORLD_VISIBILITY.md](WORLD_VISIBILITY.md)
owns that connection and retained reload custody. The process load-factor
operand required by particle producers therefore has an existing live owner.

[session_platform/particles.rs](../../crates/inf-game/src/session_platform/particles.rs)
attaches pending transfer and physical-cell update after actor/collision/
camera work, using the live delta, timing load factor, current phase/network,
process-null ActorHandle, shared world RNG, animation clock and actual
H12/H14/H15 operands. Reached update failure retains its writes and stops
only that allocation, logs the newly reached omission and continues the manager's
physical walk. Subsequent frames skip its committed prefix while other particles
keep updating.
[world_scene.rs](../../crates/inf-game/src/world_scene.rs) draws active
particles, entity models and pending particles, transfers pending, services
and clears positional one-shots, then draws terrain. These submissions share
one deferred world queue and flush; the sky has its earlier separate pass.
`004395A0/004395C0` are timing markers, not queue resets or flushes.
Impact updates enqueue copied source XYZ, selector, gain, pitch and flags;
the [one-shot audio owner](AUDIO_RUNTIME.md#process-positional-one-shots)
attaches the later listener-dependent service to actual portable PCM output.
Birth producers share this request contract. Family dying, weapon and pickup
callers now loan these same owners through their dedicated production adapters.
Persistent playback and modulation belong to [AUDIO_RUNTIME.md](AUDIO_RUNTIME.md);
arbitrary cross-allocation aliases remain separate evidence boundaries.

This attachment does not establish cinematic acceptance. The projectile collision,
damage and impact adapters lend the same retained session/world owners described
in [PROJECTILE_COLLISION_RUNTIME.md](PROJECTILE_COLLISION_RUNTIME.md), including
their remaining actor lifecycle and network limits. Other particle vtables retain
explicit update/collision/destructor/draw errors. Required services fail per
feature rather than fabricating inert particles or successful empty callbacks.
Producer requests currently cover stable argument snapshots such as retirement's
stack record. Arbitrary callbacks mutating the original XYZ/flags allocation
or phase/network globals during an audio call require live source-query
custody before that alias domain can be attached.

## Validation

The projectile [update](../../crates/inf-game/tests/fixtures/particle_projectile_update_native.json)
and [draw](../../crates/inf-game/tests/fixtures/particle_projectile_draw_native.json)
receipts cover 19 and 16 cases respectively. Update executes unchanged native
`00466250/00465660` to normal return, including signed intervals, WORD wraps and
underwater lifetime clamping. Draw authenticates the ten-call wrapper with explicit
stopped/supplied-return boundaries at `00465810`; it does not substitute or claim
the lower renderer. Callback mutations and stops at calls 1, 3 and 10 pin fresh
reads and the normal-return-only restoration. Existing shared packet receipts
remain the lower renderer authority. A focused real draw test additionally checks
packet-prefix retention, shifted XYZ, no replay and marker clearing on reuse.

The 19-case [impact update receipt](../../crates/inf-game/tests/fixtures/particle_impact_update_native.json)
covers buoyancy/drag arithmetic, water-plane and wave geometry, both terrain scales
and stopped splash prefixes. Rust comparisons call the actual splash birth for
normal crossings; the supplied stopped boundary is checked using a required-source
failure with the same retained Y prefix. These synthetic inputs authenticate
individual kernels and service order; they do not establish full cinematic
acceptance. Manager tests separately verify that a collision stop leaves the next
physical particle running and never repeats the failed particle's mutations.

The [12-case shadow receipt](../../crates/inf-game/tests/fixtures/combat_particle_shadow_native.json)
contains complete native main/shadow packets for near/far depth, both sorting
policies, fresh/cached terrain floor, negative scale/shadow values, rotated basis
and converted H7 colors. Supplied services are H3/H7 residency and renderer queue
allocation/sorting; it does not establish H7 decoding or raster output. The
[16-case far solid-quad receipt](../../crates/inf-game/tests/fixtures/d3d_solid_quad_far_native.json)
executes unmodified `004D2170` with supplied successful display/COM returns and
viewport clipping bypassed. It records actual TL vertices, two triangle calls and
render-state calls across fog endpoints/midpoints, arbitrary material shifts,
signed depth extremes and both diagonal/additive modes. These fixtures bound the
packet/backend arithmetic; visual acceptance remains a separate production check.

check_particle_birth_null_source.py
executes unchanged producer, allocator, constructor, RNG and reached normal
deletion bytecode. Its
[separate fixture](../../crates/inf-game/tests/fixtures/particle_birth_null_source_native.json)
contains 27 histories and 29 producer requests: thirteen complete original-body
histories, thirteen histories ending at supplied required-input/service boundaries,
and one source-only foreign-pointer diagnostic. Recorded requests enter native
bodies 253 times and reach 238 actual returns, excluding separately recorded
preparation/free-stack initialization. Six source sites account for 29 reached
read events, including eleven unavailable-source stops; recorded requests enter
the RNG body 59 times.
Audio enqueue is a supplied success/error service; successful enqueue executes
the actual nullable `004923C0` cleanup. No audio playback is established here.

Trail's real two-argument velocity/flags ABI, all three arms' reached stack
reads, constructor velocity bytes and original `RET 8` cleanup are asserted.
Splash takes flags alone and forces null constructor velocity. Independent
regeneration reproduced SHA-256
`A3A22BB0D0F32761C99515F6D6148653F844487C41670EEBFC33FF542DEDD060`.
Original `.text`, producer tables and x87 control word remain unchanged.

The foreign free header at supplied `FFFFFFF8` lets original allocator `+8`
arithmetic wrap its return to zero, preserving its actual marker/header writes
and bypassing the source read. This noncanonical raw-pointer diagnostic stays
outside portable Pool/Scratch parity; it is not a retail allocation-failure
observation. Unavailable global/phase reads are deliberate semantic prefix
stops, and current callbacks do not mutate process source within a request.
Complete session/particle-manager initialization, arbitrary aliases, full game execution and retail
acceptance are outside these supplied histories.

The [Rust comparison](../../crates/inf-game/src/process_particles/births/null_source_native_receipts.rs)
compares the admitted 26 histories/28 requests against actual pool/list/marker
custody, cumulative known payload bytes, RNG, process operands and audio requests.
The earlier known-source producer/update/draw fixtures below remain unchanged.

Focused tests also exercise missing-source prefixes through the actual terrain
emission host, furniture/custom-vertex producers and intro retirement/update.
The world-frame test preserves earlier view/model submission, pending-particle
transfer and positional audio service; its supplied view does not establish
that the authored furniture walk reaches an emission. Requests borrow the
retained process owner while intrinsic allocator phase remains independently
supplied; shared factories use the separate lazy manager loan described above.
Release menu, intro and briefing snapshots match the preceding checkpoint
byte for byte. The explicit hidden-SDL pixel-preservation test (then
`classic_hd_output_preserves_intro_and_authored_briefing_pixels`) also
preserved authored pixels at native, 720p and 1080p output. These are portable
regression checks, not matched retail cinematic acceptance.

check_particle_births.py authenticates the
retail executable and executes original bytecode against a supplied 400-cell
pool, original sentinel/free initialization, live phase data and terrain
operands. It does not patch instructions. The audio enqueue boundary supplies
a null success error; native `004923C0` executes. The receipt asserts copied
XYZ, selector, gain, pitch and flag arguments, but does not
establish actual sound output. All birth, allocation, list publication,
vector, shared RNG and pending-transfer routines execute normally.

The 20-case
[native receipt](../../crates/inf-game/tests/fixtures/particle_births_native.json)
includes all six direct debris and central selectors, the actual retirement
burst, zero and negative load factors, phase-gated scratch, DWORD cursor
wrap, alternating selectors, low-WORD speed, radius clamp, existing active
tail insertion and an invalid
selector. [Rust comparisons](../../crates/inf-game/src/process_particles/tests.rs)
check consumed fields, list ordering, pool admission, final shared RNG,
cursor, positional enqueue requests and nullable error cleanup count. Additional focused tests cover reclamation,
constructor-unwritten velocity, byte retention, direct clear ordering,
timing thresholds and a required-audio failure after 16 admitted debris.

check_particle_updates.py executes original
manager/class/terrain/update/trail/splash/expiration/clear bytecode, with no
collision hooks or instruction patches. Its
[62-case receipt](../../crates/inf-game/tests/fixtures/particle_updates_native.json)
supplies flat resident H12 grids, H15/H14 operands, pool custody and clocks.
It covers all owned selectors, signed deltas/load factor, expiry and forced
death, pending updates, a valid free-stack permutation whose trail is born
into a later cell and updated immediately, rounded material lookup, ground
bounce, water entry, spatial waves/coarse sampling, both impact sound branches
and clearing an unknown vtable. Three additional original-bytecode receipts
fill all 400 cells, distinguish active-tail reclamation from an unprotected
head, and cover protected tails in earlier categories. Audio hooks assert
positional enqueue arguments and execute native null error cleanup, without
claiming actual sound output. The separate [one-shot audio owner](AUDIO_RUNTIME.md#process-positional-one-shots)
owns queue service and actual portable PCM output.

[Rust comparisons](../../crates/inf-game/src/process_particles/update/tests.rs)
check retained known payload fields, unknown heap-pattern preservation,
occupancy/free cells, active/pending order, process globals, final RNG and
positional audio. Focused failures check prefix writes before missing terrain
or impact audio, scalar deletion on expiry, and direct scratch-occupant clear.
Supplied flat grids do not establish arbitrary sloped-terrain scenes,
runtime callback mutation of terrain/global operands, or visual acceptance.

check_factory_particles.py reuses that
harness for the factory classes. A central birth is given the fields
`004665D0`/`00466850` write, then the original pool update runs the classes'
update, `00465FA0` with both terrain responses, the trail, splash and central
births, the RNG and `004661B0`. Its
[24-case receipt](../../crates/inf-game/tests/fixtures/factory_particle_updates_native.json)
covers both `00526F60` selectors and `00526F20` falling, resting on ground,
entering water with and without the water flag, lava contact, expiry and
rising. Two `00526F60` cases use negative velocities whose triples are not
multiples of 4, so truncation and flooring differ in the trail velocity and
the rise. The actor sweep `004884F0` (no actors), the furniture query
`004653B0` (no furniture), the radius effect `004887D0` and the object
factory `00460310` are recorded boundaries. The Rust comparison also checks
each actor sweep and all 18 radius effects. Furniture query counts are not
compared, because the port subdivides the step into several inner queries
where the native hook skipped the whole query.

check_particle_draws.py authenticates and
executes original list traversal, transform/projection, frame selection,
lighting and quad bytecode without instruction patches. Its
[101-case receipt](../../crates/inf-game/tests/fixtures/particle_draws_native.json)
supplies resident H3 descriptors, view/context, flat terrain, queue allocation
and a light-writer boundary. Cases cover every owned authored selector,
both banks, category bases/order, animation endpoints, clipping, material
sort modes and physical selectors, fresh aligned/interpolated floor, signed
light arithmetic, light-before-far-rejection and a mixed-particle required-
light prefix. Supplied same-layout animation bytes expose negative/overflow
light and the unsupported shadow branch; its receipt stops after the actual
main-quad prefix. [Rust comparisons](../../crates/inf-game/src/process_particles/draw/tests.rs)
check exact packet bytes, sort/callback selection, flags, light requests and
ordering. These bounded packets do not establish complete-scene visuals;
the shared lighting and audio owners retain their own native comparisons.

```text
cargo test -p inf-game process_particles --lib
cargo test --release -p inf-game --lib terrain_emission_retains_native_row_rng_and_allocation_before_missing_source
cargo test --release -p inf-game --lib missing_birth_source_preserves_world_view_models_particle_transfer_and_audio
cargo test --release -p inf-game --test frontend_app_window classic_hd_output_preserves_intro_and_authored_briefing_pixels -- --ignored
python check_particle_births.py
python check_particle_updates.py
python check_factory_particles.py
python check_particle_draws.py
python check_particle_birth_null_source.py --output .tmp/birth-null-source.json
```

Each checker optionally accepts `--output <receipt>`; regeneration is a
deliberate validation action, not part of normal runtime startup.
