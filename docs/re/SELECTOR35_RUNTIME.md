# Infestation Selector35 runtime

This authority owns the distinct flying-actor class selected by factory
selector 35 and final vtable `0052273C`. The shared
[family dispatcher](ACTOR_BEHAVIOR_RUNTIME.md),
[target heap](TARGET_RUNTIME.md), and
[weapon queue](WEAPON_FIRING_RUNTIME.md) keep their own custody. Similar
Selector10/12/36 callbacks do not substitute for these virtuals.

## Construction and descriptor selection

The [retained class owner](../../crates/inf-game/src/object_runtime/actor/derived/selector35.rs)
implements `0043D070`, payload initialization `0043D1C0`, commands `0043D270`
and postinitialization `0043F760`. The constructor leaves active descriptor
`+184 = -1`; later `+D8` can change it. Neither construction nor the owned
descriptor callbacks initialize the timer at `+19C`. Production allocation
supplies it as zero under the labeled
[`FreshHeapZeroExtendedCarryApproximation`](../../crates/inf-game/src/fresh_heap.rs);
[Remaining boundaries](#remaining-boundaries) states its evidence bounds.

`Selector35RouteProcessState` retains process byte `00547638` and DWORD
`0054762C` across actors and worlds. Their initial zero bytes come from the
PE loader-zero portion of `.data`, not an actor constructor. Route update
sets the byte's low bit before publishing its lazily derived threshold.
These fields are not reset when the display or world is replaced.

The [descriptor kernel](../../crates/inf-game/src/object_runtime/actor/derived/selector35/descriptor.rs)
authenticates the actual class and uses an explicit request selected by the
native wrapper. Production borrows the same process state, target heap,
terrain, settings and RNG as the enclosing family dispatch.

| Wrapper / virtual | Native body | Distinct behavior |
| --- | --- | --- |
| `0043FA80` / `+E8` | `0043F700` | Object-target entry calls the route setter only on true entry |
| `0043FAA0` / `+EC` | `0043F590` | Route refresh, segment advancement, clearance mode and shared process threshold |
| `+12C` | `0043F4A0` | Target publication, terrain-relative altitude and initial distance |
| `0043FB00` / `+F8` | `0043D620` | Attack entry/exit, weighted target clone and ordered RNG timers |
| `00435130` / `+F4` | `0043D2E0` | Attack drift, current-target replacement and nullable secondary point |
| `00435170` / `+124` | `0043DB90` | Evade point generation with unsigned division and class-specific altitude |
| `0043FCA0` | `0043E660` | Landing entry: a fresh kind-8 point at the ship, unreleased old `+148`, `+18C/+190/+22C/+208` cleared |
| `0043FCC0` | `0043E390` | Landing stages over `+190`, with the site search `0043E270` |
| `0043FD80` | `0043D780` | Rescue entry: settles over the ground, with a fresh kind-8 point at the ship |
| `0043FDA0` | `0043D870` | Rescue update: `+194` comes down to `+190` and the point's height follows |
| `0043FE00` | `0043D9A0` | Go to space entry: the departure point and speed, and the first departure's grants |
| `0043FE20` | `0043DAC0` | Go to space update: the climb, the speed's square and the disposal above 0x7D00 |

Route publication captures source X/Z before the H12 getter and rereads
source Y afterward. It clamps the signed height difference to 100..2000,
writes `+190/+194`, calculates the distance and then replaces the target.
Changed target identity remains committed if a later read fails. Route update
zeros `+208` before dereferencing its target and writes `+238` only after the
actual target callback. The far branch rereads the current segment length;
its process threshold uses signed division by two with truncation toward zero.

Attack false entry clears `+264`. True entry preserves the two initial RNG
draws and the later conditional timer draw before its unsigned weighted
choice. It does not call the common helper/size/sound entry. Attack update
releases the current target before cloning; a failure retains the released
raw reference. A secondary allocation reads process NULL only after allocation,
and a genuine NULL allocation is a normal result with its subsequent RNG
timer. Evade uses unsigned division for its distance adjustment and a fixed
point Y of `0xEC78`; it does not acquire Selector12's different signed policy.

### Landing (descriptor 7)

Descriptor 7 is how a supply ship lands. Its score `0043FC60` multiplies the
coefficient (`+23C` WORD `+0C`) plus the bias by `0043E360`: zero while byte
`+26C` is set, else `+164`, halved toward zero unless the descriptor is the
current one. Its entry `0043E660` makes the current target `+148` a fresh
kind-8 point at the ship's position (the old target is overwritten, not
released) and clears `+18C`, `+190`, `+22C` and `+208`. Its update
`0043E390` gives up (`+22E |= 1`) while `+26C` is set or `+148` is NULL, and
otherwise runs the stage `+190`:

- **0, or any other value.** The site search `0043E270` sets `+26C`, walks
  the actor list `G+48`, and every actor with flags `0x28` but not `0x80`
  whose `+54` answers with byte `+138` set clears `+26C`. The 0040D800
  family answers `+54` with itself and its constructor sets `+138`, so
  gates, factories, bridges, sirens and stabilisers are sites, and course
  targets, which clear it, are not. A site farther than 1000 (the wrapped
  sum of squares, unsigned) draws the process RNG, and the highest low WORD,
  compared signed with `0x80000001`, writes its handle to `+268`; the search
  answers whether that draw is above zero. A resolved site gives the landing
  point behind its face: its X and Z less `__ftol` of its forward X and Z
  (basis words 7 and 9, its `+48` being its own basis) times the ship's size
  times 5/4 plus the site's size, at `00450A30`'s ground height there. A
  kind-8 current target takes that point, and the stage becomes 1. Without
  a site the stage keeps its value and `+22E` bit 0 is set.
- **1, "Traveling" (`00533DB8`).** Under a quarter of the size, `+260`, a
  ship slower than 50 (the integer root of its X and Z velocity squared,
  compared as a signed WORD) starts landing: stage 2, `+194 = +234 << 12`,
  `+230 = 0`. At or above it, `+230 = 0xFA` and `+234` is the size plus 500.
  A helper target `+154` returns the stage to 0.
- **2, "Landing" (`00533DC4`).** Above half the size it travels again. Else
  `+194` counts the delta down (signed), with `+234 = +194 >> 12`; when it
  runs out, an RNG draw gives the wait, `(draw & 0xFFFF) + 0x2625A << 5`
  microseconds, and stage 3.
- **3, "Waiting" (`00533DCC`).** `+234 = 0`, then `+194` counts down with
  `+234 = +194 >> 20`; when it runs out the stage returns to 0 and `+22E`
  bit 0 lets the ship choose again.

Oolbryte's supply ship reached this descriptor and stopped, first at the
score's source and then at the entry, so it never left its spawn. It now
travels from site to site, lands behind a gate or factory, waits and takes
off.
`check_selector35_landing.py` runs the
original bodies for 111 landing and 18 score cases, with only the allocator,
`0047AD90` and `00450A30` supplied; the port matches all but the one whose
supplied resolver refuses a listed site, which a live handle cannot do.

### Flying Escort (descriptor 9)

The small alien ships of Helniazine (resource 81) wake through it. When a
teammate is hit, the nearby retaliation (`0047C840`) runs the family target
reaction `00429340`, which names the victim at `+20C` with an escort timer
`+210`; the escort's score `0043FCE0` (source `00428990`) is then the
distance to it plus 0x100. The change `00435220` runs the actor's `+E4`,
`00428AB0` here and on selector 34 (its descriptor 7): entering, the status
"Flying Escort" (`00531210`), a follow target on the `+20C` actor
(`004825D0(+20C, 0)`) at `+148`, and with it resolved `+234` its radius WORD
`+28` (at least `0xA4`) and `+230` that plus twice the ship's own (at least
`0xFA`). The update `0043FD20` runs `00428B80`: past half a second (`+188`)
within 500 of the target (`+14C`), both signed, `+22E |= 1` asks for a new
choice. [descriptor_escort.rs](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_escort.rs)
ports both. Before them the port stopped at the change and the ships sat
idle until the vehicle touched them (a Helniazine play,
2026-10-18). The lab's `helniazine-ships.json` shoots the alien beside them:
the port matches retail in every compared event over 481 ticks, where the
port before stopped at `00435220` for all three ships and parted at tick 43.

### What wakes them

In the lab (`helniazine-ships.json` and variants run from scratch,
2026-10-18) retail's small ships wake only through that hit on a nearby
alien. Firing the cannon 1,500 units from them without hitting anything
left all three asleep (descriptor -1) for 98 s. The weapon's noise does
reach them (`0048CB33` sends 0xA, and `0048D980` 9, to every actor within
4000), but their `+98` (`004270B0`) answers only 0xD, the smoke. Driving up
to them woke them at tick 200, when the big ship (resource 95, the same
class, awake from tick 4 near the player) destroyed the ACAM and its death
blast (`00531250`) hit an alien beside them. With the lab's ship log, each
selector-35 update `0043E6D0` (resource, position, descriptor, state,
velocity, `+20C`; the port's `event_log::small_ship`), the port matches all
3,724 of retail's over 931 ticks at 125 ms, and every compared event in
the shooting, the noise, the drive and at 16 ms and 33 ms steps.

The griffin (placement 148, resource 95, H8 556) does not wake them either
(`helniazine-griffin.json`): fought 700 west of the small ship at 146 with
the plasma, it dies at tick 387 and all three stay asleep for the run's 981
ticks, in retail and in the port, which matches every compared event.
From a start 1500 east of the griffin, eight cannon hits on it left them
asleep, and so did its death there with its health `+A0` written to 1
(tampered-run: a scratch variant of `helniazine-griffin.json` with that
start and the cannon, the write at pass 225). In a longer fight with row 2
retail's shots hit the alien beside them (resource 33) at tick 211 and they
woke at 214, the hit that wakes them above.

Vernimab's (world 21) ships wake the other way, through its seven
[wake triggers](ACTOR_RUNTIME.md#selector-42-wake-triggers) (selector 42).
Each fires when the player's unit comes within 4000 of the trigger (its
`+44`), and wakes every sleeper within its own wake radius of the trigger
(`+48`): 3000, or 6000 for the one at (11425, 53496). Its two small ships
(resource 0x53) sleep by the triggers at (27983, 48678) and (22248, 60555),
the third is awake from the load and lands, and two larger ships (resource
0x69) set off by the first trigger once it fires. A ship therefore stands until the player
nears its trigger, at a distance set per trigger, not per ship. The lab
(`vernimab-wake.json`, 2026-10-19, after a ship was seen standing until
the player came near) matches retail with the port: from the start all 2,043
ship updates, and from a start beside the first trigger all 2,344,
including the tick each ship wakes.

### Frame rate

How they fly once awake depends on the step, in retail as in the port.
Descriptor 2 is the attack (`0043FB00`, `00435130`), 8 the route travel
(`0043FA80`, `0043FAA0`) and 9 the escort; the three small ships after
the hit, retail:

| Step | Velocity (median) | Attack (2) | 5 and 6 | Route (8) | Escort (9) |
| --- | ---: | ---: | ---: | ---: | ---: |
| 125 ms | 605..664 | 14..19% | 52..70% | 0 | 8..10% |
| 33 ms | 327..438 | 42..72% | 7..42% | 6..13% | 5..6% |
| 16 ms | 259..335 | 61..65% | 8..13% | 15..18% | 6..8% |

The port's `paced_actors` fix updates them at retail's 30 fps step, as it
does the [scientists](ACTOR_MOVEMENT_TAILS.md#the-scientists-gait-and-the-frame-rate).
Run at 60 frames a second from the 16 ms case, the paced ships fly at a
median of 404 and escort 7% of the time, as retail's at 33 ms do; unpaced
they fly at 277 and escort 23%.

### Rescue and Go to space (descriptors 10 and 11)

These two carry a rescue. A transport with a count `+242` (command 24's
byte) lands where it is and takes its passengers aboard. Once all are aboard
and the player's unit comes near, it leaves.

- **Rescue's score** `0043FD40` multiplies the coefficient (`+23C` WORD
  `+12`) plus the bias by `0043D730`. That is nothing without a count. It is
  also nothing while another descriptor runs (`+184` not -1), unless Rescue
  is the current one. Otherwise it is 0x7FFF until the clock `005471C0`
  passes `+244`, and 1 after. `+244` is a deadline: the interaction
  notification `+9C` (`0043F860`,
  [interactions](ACTOR_INTERACTIONS.md)) writes the clock plus 200 whenever
  the count is nonzero.
- **Its entry** `0043D780` settles over the ground below the transport.
  `+14C` is its height above the ground (`00450A30` at its X and Z). `+190`
  is the ground and `+194` its own height, both shifted up by 14. `+260` and
  `+238` clear, "Rescue" (`00533DA4`) goes to `+228`, and a fresh kind-8
  point at the transport becomes `+148`.
- **Its update** `0043D870` lowers `+194` by the delta toward `+190` while a
  count remains, and a kind-8 `+148` takes the new height (`+194 >> 14`):
  about 61 units a second.
- **The pickup** is the transport's `+A0` `0043F790` in the pair pass. A live
  transport (`+C4` at least 3) with a count takes a partner whose `+5C`
  answers, which only a scientist's (Selector12's `00404610`) does. It
  disposes the scientist (`00479FA0`) and counts it in `+243`, a wrapping
  byte. With all aboard, the mission records the placement
  (`0048EAD0(+78, 0)`), and `00427BE0`'s settle follows inline: a negative
  `+144` times -1000, and flags `0x04600000` cleared. The slot flash
  `00489A90(2)` marks the scientist's spot.
- **Go to space's score** `0043FDC0` over `0043D8F0` is nothing while fewer
  than the count are aboard, and 0xFFFF once the transport has gone (`+243`
  0xFF). Otherwise, after the deadline `+244`, it is 0xFFFF when the first
  player's unit (`0046B030`) is nearer than WORD `+240`. The distance is the
  wrapped sum of squares' root (`isqrt(sum >> 2) << 1` past bit 31).
- **Its entry** `0043D9A0` aims along the basis X row: `+18C`/`+190` are its
  x and z times 15000 (`__ftol`, wrapping) and `+194` is the speed `+258`
  shifted up by 14. `+230` and `+208` clear, "Go to space" (`00533DAC`) goes
  to `+228`, `+238 = 1`, `+234 = 750`, and a fresh kind-8 point at the
  transport becomes `+148`. The first departure (`+243` not 0xFF) sets `+22E`
  bit 1, marks both counts 0xFF and grants `+7A` to every player in `G+60`'s
  list (`004750C0`).
- **Its update** `0043DAC0` sets `+22E` bit 1 and points a kind-8 `+148` at
  the transport's X and Z plus `+18C`/`+190`, 1500 above it. Below 10000 the
  speed grows with `+194` (by the delta; its `>> 14`, capped at 10000
  unsigned, is `+258`, and `+25C` its square). From 1500 below the top the
  point stays at 0x7FFF, and above 0x7D00 the transport disposes itself.

Pillier Stren's (world 10) transport stopped at Rescue's score; it now runs
the whole rescue.

So the rescue's grant (Pillier Stren's `+7A` = 0x24, its message) waits for
the departure: after the last boarding, the player must be within `+240`
(2500 there; the count is 8) once the deadline has passed. A player who
drives off hears nothing until coming back, as in retail. The port fix
`PortFixes::mission_completion` (on by default, every renderer, off in a
network game; `--mission-completion false`) grants `+7A` at the last
boarding as well
.
`check_selector35_rescue.py` runs the
original bodies over 155 cases: both scores, both entries and updates, and
the pickup. Only the allocator, `0047AD90` and `00450A30` are supplied, and
the grants, disposals, records and flashes are recorded. The port matches
every case.

## Target, clearance and firing continuation

[tail.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35/tail.rs)
owns `0043E766..0043EE61`, after a normal family prefix and `+D8` return.
It returns controls to the separate motion virtual; a genuine NULL `+148`
returns zero controls and still reaches that motion call.

The captured primary target refresh commits before the status write to
`+150`. Live kind-2 prediction uses two target-velocity units and twice the
actor's size in Y, retaining separate fresh target reads. Secondary refresh,
distance, timer subtraction and release remain ordered. If the secondary
expires, native code keeps its already calculated world direction/distance
for this tick while altitude again uses the captured primary target.

The clearance scan is enabled by byte `+238`; otherwise it uses `+230`.
Its horizontal velocity radius is at least three grid steps. X is the outer
loop and Z the inner loop. Each cell consumes the actual H12 samples, signed
height WORD and sparse-cell flag. Flag `4` selects the sparse record;
otherwise the retained process fallback placement is used. Only a reached
nonzero placement requests H11's signed model resource and optional H8 radius.
The production host borrows one current terrain owner and its immutable
sampling policy; it does not copy or replace terrain during these reads.

Clearance adds 64 to the highest sampled obstruction. The low-altitude arm
sets `+250` and clears flag `04000000`; sufficient separation sets that flag.
Tilt uses basis-Y Y multiplied by the original `-1024.0` constant and native
truncation. Direction dead zones and the signed low-WORD square-root result
retain the original wrap behavior.

Firing requires the live behavior owner, range, descriptor state and refreshed
target status. The direction is transformed through the actual inverse basis;
signed-BYTE selected entry, entry timer/age and the front-cone tests precede
the shared `0048D340` producer with lead enabled. Its queue and any failure
prefix use the enclosing pass's borrowed RNG. Alternate yaw runs afterward
only when its native zero-turn and speed gates allow it.

## Motion and persistent audio

[motion.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35/motion.rs)
owns virtual `+118 = 0043EF60`. It preserves the live state gate, thrust
budget, wrapped WORD controls, class drift and alternate-axis policy. Vector
normalization follows FPCW `027F`: Z squared plus Y squared, then X squared,
square root, and the original float spill. The desired axis controls alignment;
acceleration acts along the captured basis-Y axis. These are distinct vectors.

The actual angular response `0047D440`, force limiter `00426AE0` and physics
`0047B1E0` are shared owners. Alternate ground decay uses arithmetic right
shift. Byte `+250` is cleared only after normal physics return.

[audio.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35/audio.rs)
owns `0043EE75..0043EF55`, after normal motion. The actual camera getter runs
first; a genuine NULL camera returns normally without sound reads. Fresh
actor/camera WORD coordinates produce wrapped signed differences and an
unsigned squared-distance comparison against `0x03840000`. A far actor
deletes its retained record before clearing `+168`.

The near branch tests **actor WORD `+8C`**, not size WORD `+28`. Above 400 it
uses H9 slot 64, volume 65536 and pitch 8192; otherwise slot 16, volume 16384
and pitch 65536. An existing record updates without another H9 lookup.
Creation retains the sound owner's allocation-before-position-read order.
Playback and voice teardown remain with the shared [audio owner](AUDIO_RUNTIME.md).

The [production adapter](../../crates/inf-game/src/session_platform/world_actor_update/selector35.rs)
orders target/clearance/firing, motion and persistent audio. It borrows current
H12/H11/H8, process furniture state, sound records, camera inputs and the same
live RNG. A reached error retains earlier writes and stops this actor's feature;
later passes do not replay the incomplete continuation.

## Dying and dormant lifecycle

[lifecycle.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35/lifecycle.rs)
owns `+120 = 0043DEB0`, `+11C = 0043DFA0`, `+E0 = 0043E120` and
`+DC = 0043E1C0`. Their production wrappers borrow the existing live
[dying-service context](../../crates/inf-game/src/session_platform/world_actor_behavior/family_dying.rs)
through an explicit class request; no family10/36 stage behavior is substituted.

Dying false entry returns before receiver reads. True entry publishes the
Dying status, allocates a kind8 copied point, captures process NULL after
allocation and before actor XYZ, then publishes `+148`, clears `+238/+230`
and adds six ordered low-byte RNG impulses to linear/angular velocity.
An actual NULL allocation skips source/XYZ reads and still reaches the suffix;
a source failure preserves the earlier allocation/status prefix.

Update treats elapsed `+188` as signed. Only values strictly greater than
15,000,000 request state0 with current process NULL. Otherwise a current
kind8 target receives actor X/Z and Y `-5000`; other target kinds are preserved.
The first RNG draw conditionally writes `+C8/+CA`; two further draws spread a
captured position using unsigned size, then three draws perturb captured
velocity. Size above 200 chooses trail10; otherwise trail9. Zero size traps
after the reached color and RNG prefix. Normal `00489D90` return precedes
`0047E190` on fresh actor coordinates. That shared terrain owner performs its
height gate before the possible additional RNG draw and clears the selected
cell's high flag nibble with count-minus-one zero.

The dive ends at terrain contact `+94 = 0043F3E0`. Only `+C4 == 1` crashes:
the radius effect (unless bit31), a burst over the position copied before it,
and `0047D670(0, process NULL, 0)`, then the shared response
([WORLD_CONTACT.md](WORLD_CONTACT.md#terrain-responses-virtual-94)).

State0 reaches actual `+B0 = 0043E250`: byte `+251 == 0` queues disposal;
nonzero leaves the object resident. Unknown byte custody is a typed error
after the shared state transition, never an assumed zero. The shared next-frame
[disposal pass](WORLD_CLEANUP.md) owns later destruction and handle release.

Dormant change always runs the central2/debris1 burst, including false entry.
After that callback it freshly checks bit31; a clear bit runs radius block
`00531250` on current actor XYZ, with freshly captured actor source `+84` and
replication enabled. Only normal effect return reaches shared `00428630`, which
writes Dormant status on true entry and leaves it intact on false entry.
Dormant update captures position, consumes one RNG draw for vertical velocity,
then reads live load factor `0055F154`. Its signed divisor may require another
draw before emitting trail6. Effects use the same process RNG and retain every
earlier write when a reached service fails.

## Destruction exit

Destructor `0043D0F0` reinstalls `0052273C` and, for a nonnegative `+184`,
calls the selected `00533D38` descriptor's change with argument false before
storing `-1` and delegating to shared `00428750`.
[destruction.rs](../../crates/inf-game/src/object_runtime/actor/derived/selector35/destruction.rs)
runs that exit inside `ObjectArena::destroy`. Ten exits are entry-only under
this vtable and write nothing: `+120 0043DEB0`, `+128 0043DE00`,
`+E8 0043F700`, `+E4 00428AB0` (the [Flying Escort](#flying-escort-descriptor-9)), `0043E660`, `0043D780`, `0043D9A0` and
shared `00427990/00427A00/00427A70`. Attack `+F8 0043D620` clears `+264`.
Dormant `+E0 0043E120` runs its burst and unflagged radius effect even on
false: `0048A8B0`, then, unless `+2C` bit 31 is set,
`004887D0(00531250, +32, +84, 1)`, then `00428630(false)`, which leaves
the status. `ObjectArena::destroy` has no particle or radius services, so
there that exit and any out-of-table index stay a typed destruction
boundary that retains every field. The world's own walks lend them: the
cleanup `00478CD0` (Results and LeaveWorld) and the disposal drain
`00479840` pause at a Dormant ship after its `+30`
(`WorldCleanupError::DestructionExitPending`, the drain's `exit_pending`).
The session runs the exit over the world's live owners (the burst through
the process particles, the radius through the world's damage owners, the
world's RNG word), `exit_selector35_dormant_for_destruction` stores `-1`,
and the walk resumes at that ship's destructor without a second `+30`.
LeaveWorld clears the particles right after its walk (`004871FB`), so
there the exit's lasting trace is the RNG word its burst drew from.
Negative indices skip the table and keep their value.

## Native receipts and evidence limits

The tracked native generators execute the authenticated
EXE with FPCW `027F`; default execution compares the tracked LF JSON bytes.

| Generator | Fixture and scope |
| --- | --- |
| check_selector35_descriptors.py | [65 descriptor cases](../../crates/inf-game/tests/fixtures/selector35_descriptors_native.json): six actual bodies, real target refresh, RNG and integer square root; allocator/free and terrain returns supplied |
| check_selector35_tail.py | [21 continuation cases](../../crates/inf-game/tests/fixtures/selector35_tail_native.json): actual target refresh, clearance, arithmetic and release order before the motion call; preceding status/D8 and reached free returns supplied |
| check_selector35_motion.py | [19 force cases](../../crates/inf-game/tests/fixtures/selector35_motion_native.json): actual response/force callees through the boundary before shared physics |
| check_selector35_audio.py | [16 audio cases](../../crates/inf-game/tests/fixtures/selector35_audio_native.json): actual suffix, unsigned size/range choices and camera mutation prefixes; preceding status/D8/motion and lower camera/record services supplied |
| check_selector35_lifecycle.py | [45 lifecycle cases](../../crates/inf-game/tests/fixtures/selector35_lifecycle_native.json): four complete callback bodies and actual RNG/target writes; explicit heap, trail, burst, radius, scatter and state service boundaries with stopped/mutated prefixes |
| check_selector35_crash.py | [13 crash cases](../../crates/inf-game/tests/fixtures/selector35_crash_native.json): terrain contact `0043F3E0` over every `+C4` and bit31, with its services recorded in order |

Module tests compare the relevant retained fields, controls, targets, RNG,
process words and lower-call order. Separate production tests compose real
world resources and the complete tail. Copied targets retain whole XYZ and
constructed-field ownership; a stopped constructor is not a claim of raw
partial-byte target parity. These receipts do not establish
allocation history, complete retail callback cadence or cinematic parity.

## Remaining boundaries

The accepted `+19C` custody evidence
proves that the first descriptor timer can retain prior H0 string bytes
through native small-block allocation. Its first `004295F4` read changes
descriptor admission. Universal zero, a per-placement captured value or a
constant-delay substitute is therefore not equivalent. On the
2026-10-06 decision, production allocation nevertheless zero-fills `+19C` as
a labeled approximation (`ActorAllocationDwordCarry::Family19c`). Ref50/ref60
match. Ref61's first selection comes about 7.5 s early: right after its
1.5 s `+144` spawn delay instead of after the residue countdown. The shared
allocation/resource reuse owner still has to replace the zero. Strict arenas
keep reporting the missing field.

With dispatch running, the port follows native ref60 (Roam `4`, then
follow-trail `8`, whose entry frame takes the near-node advance) and stops
on the next route update at `0043F617`, which reads unwritten `+260`. The
`+260`/`+184` histories
show that native never reads that residue. For both ref60 and ref61 the first
post-construction access is the tail write at `0043EC7E`, and the route
update's `0043F612` read starts frames later. That write runs only while
clearance byte `+250` is clear. Native refs are constructed at Y `72`/`82`
and are held up near Y 150. On ref60's first Roam frame its clearance read
(`0043EB8E` at `683524:1EA8`) sees Y `0x98`, and physics `0047B40D` moves
ref61 from Y `0x97` to `0x8F`. The port's `0047B1E0` is pure kinematics like
native's. Without a world actor contact pass, slot 97 (ref61) fell through
the terrain during its 1.5 s `+144` delay physics. Slot 96 (ref60) sank to
Y≈40, under its clearance of 172: terrain 32, plus the 76-unit radius of
furniture placement 47, plus 64. Both therefore kept `+250` set, never wrote
`+260`, and stopped at the read. A zero `+260` would have run them in that
physically wrong state. The fall window then named the holder: terrain
response `0047D33D` restores ref61 to Y `151/152` after every physics drop.
The port now runs that [world contact pass](WORLD_CONTACT.md). The ships stay
on the terrain, and at 125 ms steps the `+260` stop is no longer reached.

Faster steps still reach it. At 33 ms the route update for slot 96 (ref60)
runs before any tail write. At 16 ms the same happens for slot 96 and for a
spawned ship. Production allocation therefore zero-fills `+260` as another
labeled carry, `ActorAllocationDwordCarry::Selector35Route260`, and strict
arenas still report the missing field. The
write history
bounds the zero. Ref60's `+260` is never written from the start of the
trace until its first tail write at clock 2331, so it holds zero. Ref61
holds string residue `726F4D00`, written before the intro, until its first tail
write at clock 2443. Infest01 ran near 125 ms per tick, so no native read of
either value is recorded.

The remaining allocation input and any newly reached class callbacks must be
closed before matching the complete visible sequence. That remains part of the
full intro objective.
