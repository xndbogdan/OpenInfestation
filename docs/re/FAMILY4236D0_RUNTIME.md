# Infestation 004236D0 actor family and selectors 9/10/36

This document owns the probe-bearing `00429E20` prefix, `004236D0`
construction/payload/commands/postinitialization, and selectors 9/10/36. The
[common family](FAMILY428650_RUNTIME.md) owns home-position admission,
terrain commands and placement-reference resolution. The
[shared actor](ACTOR_RUNTIME.md) owns handles, central payload and behavior
storage. Phase-3 acceptance remains in the
Infestation objective.

## Distinct allocation owners

`00429E20` calls `00428650`, installs vtable `00521164`, sets pointer
`+234=null` and WORD `+238=0`. It does not initialize `+230`, `+232` or
`+23A`. This is a sibling of the `004293D0` prefix used by selectors 34/35;
their scalar `+234=512` must never be interpreted as this owned pointer.

`004236D0` installs vtable `00520B84` and initializes retained pointer
`+23C=null`, DWORDs `+240=0`, `+248=0`, byte `+260=1`, WORD `+262=FFFF`
and WORDs `+264/+266=0`. Executable data identities are `+244=00520A80`,
`+250=0052FE40`, `+254=0052FE48`, `+258=0052FE38`, `+25C=0052FE58`.
Unwritten fields remain unknown. It acquires a 12-byte `00454B30(8)`
helper only when `+234` is null. No additional RNG draw occurs here.

[`TerrainProbeCache`](../../crates/inf-game/src/object_runtime/actor/derived/terrain_probe.rs)
owns that helper's separate packed sample allocation. Its BYTE argument
accepts 8 or 16; other values normalize to 8. Header WORDs `+4/+6` become
zero, byte `+9` becomes twice the argument and byte `+A` is the row stride
in DWORDs, dimension divided by eight. The two sizes are 16 by 16 samples
in 32 DWORDs (128 bytes) and 32 by 32 in 128 DWORDs (512 bytes). Constructor
`00454C20(1)` clears every DWORD. Bytes `+8/+B` remain unknown. Private
storage and read-only accessors retain that constructor contract; the shared
[cache access owner](#shared-packed-probe-cache) implements its later updates.

The common family's `+218` is another allocation, unrelated to `+234` or
behavior `+140`. Postinitialization can allocate `0x5C` bytes and invoke
`00482FF0`, which writes only DWORD `+8=0`. `FamilyHelper482ff0` retains
that initialized field and all other DWORDs as unknown.

Nondeleting destruction runs `004237D0 -> 00429E70 -> 00428750 -> 00425F70`
on the supported behavior-index `+184=-1` domain. It frees the probe's
inner buffer (`00454B90`), the probe header, common helper `+218`, then
the central behavior allocations before releasing the actor handle and
nodes. The arena explicitly releases those resources before returning its
retired object state. A freed native pointer is not a live portable owner.
Nonnegative behavior-state exit callbacks and six later owned common-family
subobjects are still outside the supported construction path.

## Payload and instance commands

`004239C0` invokes virtual context initialization before payload `00423850`.
The payload calls central `00426C10` and retains the H13 address at payload
`+58` in object `+23C`. Its DWORD `+68` selects `+25C`:

| Input | Executable table |
| --- | --- |
| 1, 2, 7 | `0052FE68` |
| 3, 8 | `0052FE88` |
| 4 | `0052FE98` |
| 5 | `0052FE78` |
| 6 | `0052FEB8` |

WORD `+6C` copies to `+264`; its low bit becomes central byte `+160`.
WORD `+6E` copies to `+262`. DWORD `+70` values 1..9 select `+254` from
`0052FF38, 0052FEE8, 0052FEF8, 0052FF48, 0052FED8, 0052FF08, 0052FF18,
0052FF28, 0052FF88` respectively. Unmatched inputs preserve the current
table, including after prior initialization. These are full DWORD switches,
not truncated WORD selections. Canonical `Family4236d0Payload` preserves
source values and spans without running these policies.

Command `004239E0` forwards signed codes at most 22 to `004276B0`.
Code 23 succeeds without reading any operand. Code 24 retains the operand
address at `+23C` without reading it; larger codes reject. Both H13 and H14
retained addresses use the actual shared mutable runtime allocation,
independent of later residency changes. Aliases observe each other's writes.

## Postinitialization

`004250C0` reads unsigned WORD `retained+0`. A nonzero denominator causes
an unsigned WORD read at `retained+0E`, then writes `(numerator << 10) /
denominator` to common `+214`. Zero preserves the prior ratio and skips
the numerator read. Unlike selector 34, this does not set flag `1000`.

If actor `+B4` is zero, it ORs flag `10000000`. `00425AB0` selects mode
`+266`: zero for `+B4=0`, one for `+B4=18`, otherwise one without behavior
`+140` and two with it. `00425A40` only changes a differing mode; on the
supported `+184=-1` path it ORs low byte `+22E` with `0C` before storing
the mode. A later active-state change requires unimplemented exit callbacks
and fails explicitly rather than skipping them.

After shared `0047AAD0`, a null common `+218` triggers virtual `+7C`.
For the represented family and selectors10/36 this is `004027A0`, which returns
behavior `+140`. A nonnull result acquires the `00482FF0` helper. Repeated
postinitialization preserves an existing helper even if the behavior owner
later becomes null. These resource owners have independent lifetimes.

## Selector 9: the family's own class

The trucks and assault craft (29 placements in leaves 12, 13, 16, 19,
21..24, 30 and 34) are the family's own class. Case 9 of `004603B0`
allocates `0x268` bytes and runs `004236D0` alone: vtable `00520B84`, no
further fields. Its `+0C` is `004239C0` itself (the virtual context, then
`00423850`), its commands are `004239E0` (code 24 replaces `+23C`, as on
selector 36), and its `+1C` is `004250C0` with no behavior selection before
it, so the behavior state stays fresh.

Against selectors 10 (`00520CC8`) and 36 (`00527EE0`), the vtable differs
from both only at `+00`, `+0C`, `+10`, `+1C`, `+118` and `+138`. Its update
`+24` is `00424830`, selector 10's: the prefix, the behavior dispatch `+D8`,
the target, avoidance and fire tail `004248DD..00424BBF`, a call through
`+118`, and the persistent-audio suffix `00424BC5..00424C32`, which only
resources 33, 34 and 106 sound. The dying entry `+120` is the shared
`00424280` and the stage `+138` the shared `00423EE0` that selector 36's
wrapper calls (three kind-`0x83` pickups about the `+X` axis). Its pair
reaction `+A0` is `00425B90`, as on both: a person (a `+5C` view) touching
a team-0 unit is taken aboard
([world contact](WORLD_CONTACT.md)). Artoria's four neutral tanks
(placements 114, 115, 258 and 259, team 0) sleep until a wake trigger
fires, and a sleeping unit is in no contact pass: in the lab
(`artoria-tank-board.json`) a scientist beamed down onto one beside the
column was never offered to it, and none of the tanks ran its update. The
user's session of 2026-10-21 reached a tank awake, after driving through
the world, and the port stopped there at the boarding (`pair virtual
00425BE0`) until the boarding was built.

Its motion `+118 = 00424CF0` is selector 10's `00425400` without the mode
`+268` and the helper `+26C`, which its allocation does not have, and with
two differences of its own:

- It reads `+C4` once. Above 1 the actor takes contact at `delta >> 5` and
  drives; otherwise it takes contact at step zero and damps its spin while
  flag `0x80000` is set (thresholds 64, 24 and 64, the logical shift by 14).
  Selector 10 rereads `+C4` after contact.
- A live actor without contact zeroes its engine scales `+170` and `+16C`
  before the shared physics.

Otherwise the arms are the same: the contact's alignment turns the Y basis
toward the normal (`00426730`); with the normal's local Y above 0.0
(`0051F198`) and a nonzero power, `+22E` bit 5 boosts power by half (capped
at `0xFFFF`) and the step by half, then the forward `00426860` (`+254`,
control 1), angular `00426AE0` (`+258`, control 0, local Y) and lateral
`00426760` (`+250`) responses; physics `0047B1E0(delta, 0x1F)` closes every
path. Water contact at a nonzero step keeps selector 10's native-stack
boundary.

The [port](../../crates/inf-game/src/object_runtime/actor/derived/family4236d0/motion.rs)
builds the class through the family's existing owners and admits it to the
shared update (`FamilyUpdateClass::Family4236d0`), the tail, the audio, the
pair reaction and the dying paths. Infest04 calls `00424CF0` 2,673 times
(Infest03 1,090; Infest02's short Terra Nova stretch never). Sixteen of
them, every 167th, on Oolbryte, Tabuchi and Helniazine, one dying, replay
from their captured entries on the loaded worlds and match every field
(`family-motion.js`); a
sensitivity pass kills mutants of the controls, the alignment, the damping
thresholds and the boost.

Oolbryte's supply truck (placement 98) dies in Infest04, and the port's
truck dies the same way
(ledger).
The dying entry runs the camera effect `0xF00`, the blast `005307D8`, death
sound `0x28`, a burst, the blast `00531250` and the engine loop's stop. Stage 1 comes
7.6 s later with three kind-`0x83` pickups worth 500. Stage 2 comes another
7.6 s later and does nothing.

### The assault craft and the trucks

Selector 9 places two kinds of unit, with the update, motion and death
above.

- The **assault craft** (resource 106, model 403 `assault`; H0 `0x2B2`
  "Assault craft": "Allied attack craft, armed with plasma cannons") are
  Terra Nova's placements 83 and 84 and Stendahl Alpha's 83. Terra Nova's
  are the units its briefing says the aliens have pinned down ("we need you
  to try to rescue if you can before they are all wiped out"). Command 8
  gives them 1000 and 2500 health; the resource's own is 5000.
- The **trucks** (resources 33 `truck1` and 34 `truck2`; H0 `0x2AC`
  "Trucks": "Used to carry cargo between locations. Often extremely
  explosive due to their fuel payload") are the other 27 placements, on
  Stendahl Alpha, Helniazine, Tabuchi, Artoria, Shalapreth, Grach'to,
  Foldrak Noir, Oolbryte and Caradion.

Pillier Stren (world 10, leaf 20) has neither: its spawner (placement 102)
makes scientists and its selector 86 is a particle emitter.

The two differ only in their payloads:

| | Assault craft | Trucks |
| --- | --- | --- |
| Team `+B4` (payload `+0C`) | 2 (3) | 18 (2) |
| Mode (`00425AB0`) | 2: nine descriptors (`0053076C`) | 1: eight (`0053074C`) |
| Channel 1 threshold (payload `+10`) | 800 | 4000 |
| Weights (`+23C` = payload `+58`) | Attack and Follow Player 500 (WORD `+0`), Run Away 0 | Attack 0, Run Away 10 |
| Trail weight (WORD `+0A`) | 100 | 1000 |

Both have radius `+164` 2500, Going Home, Herding and Wander weights 10,
candidate bias 1, thresholds 100 on channels 6 to 8, the `+25C` table
`0052FE68` (`+68` = 7), the `+254` table `0052FF08` (`+70` = 6) and the
death sound 40 (`0x28`) at `+182`. While they live (`+C4` above 1), the
update's audio suffix keeps their engine running: sound 20 at volume and
pitch `0xAAAA`, created into `+168` (`00487F60`) and then kept at their
position (`00487FC0`).

Command 18 makes a sleeper: `+144` becomes the negated operand and the
flags take `04200000` ([ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#selector-42-wake-triggers)).
`00424830` returns at once while `+144` is negative, so Terra Nova's craft
and 11 of the trucks (Stendahl Alpha's three, Artoria's three, Grach'to's
four and one of Foldrak Noir's) stand until a selector 42 trigger near them
wakes them. Stendahl Alpha's craft and the other trucks are awake from the
load.

Lifecycle 0 and 1 force descriptors 0 and 1; otherwise each reselection
scores the rest and the highest wins ([dispatcher](ACTOR_BEHAVIOR_RUNTIME.md#dispatcher-policy-and-custody)):

| Index | Status `+228` | Score source (weight WORD) | Change, update |
| --- | --- | --- | --- |
| 0 | "Dead" | none | dormant `00424790`, `004247C0` |
| 1 | (dying) | none | the dying entry and stages above |
| 2 | "Attack" | target scores `+1D8`/`+1F8` (`+0`) | `00429B00`, `004298A0` |
| 3 | "Run Away" | target scores `+1E0`/`+1F0` (`+2`) | `00423E50`, `00423BD0` |
| 4 | "Wander" | half of `+164` (`+8`) | `00427990`, `00427800` |
| 5 | "Going Home" | the distance from home (`+4`) | `00427A00`, none |
| 6 | (herding) | target score `+1E8` (`+6`) | `00427A70`, `00427AA0` |
| 7 | "Following a trail..." | `+FC` (`+0A`) | `004288E0`, `00428830` |
| 8 | "Follow Player" (mode 2) | `00423B40` (`+0`) | `00423A80`, `00423A20` |

So a craft attacks whatever its targets hold and, once they are gone,
follows a nearby player; otherwise it follows a trail when it has one,
wanders or goes home. A truck never attacks: it runs from threats, follows
a trail when it has one and otherwise wanders or goes home.

**Targets and fire.** The periodic table `005307B0` (`00425D00`,
`00425CF0`, `00427510`) runs one producer, in turn, every 0.25 to 0.51 s
(`+21C = (draw & 0xFFFF) * 4 + 250000` microseconds), and the
[target selection](FAMILY_TARGET_SELECTION.md) fills the six slots
`+1D4..+1FC` and their scores. Attack's entry clones the better of slots 0
and 4 into `+148` and its update retargets every 2 to 3 s
([Attack](ACTOR_BEHAVIOR_RUNTIME.md#common-attack-descriptor-entry)). The
update's tail fires (`00424AAF..00424B4A`): with a weapon behavior
(`+140`), a target nearer than `+164` (signed), an actor target (`+22C`
set), that target alive (its refresh `+150` = 3), the selected entry ready
(its cooldown `+14` zero, its `+0` nonzero and its `+1C` below 6,000,000,
unsigned) and the target in front of the nose (local `z > 0`, `|x| < 2z`,
`|y| < z + 50`), it calls `0048D340` at `+148` with the distance, the world
offset and the delta, publishing its lead, or `0048D5A0` while the WORD
`+17A` counts down. check_family_fire_gate.py
runs the unchanged gate over 33 cases, each condition at its edge and the
signed and unsigned ones past bit 31; the port's decision and arguments
match every one
([test](../../crates/inf-game/src/object_runtime/actor/derived/selector10_tail/tests.rs)).
Hits come back through `0047C6E0` and the payload's response table.

**Death.** At zero health the lifecycle drops to 1 and descriptor 1 runs
the shared dying entry `00424280` with status "Dying" (`00530D3C`); every
7.6 s the update moves `+240` on and the stage `00423EE0` runs. Descriptor
0, "Dead" (`00530D44`), is the end. The payload's dying kind `+174`
(payload `+4C`) and flags `+264` (payload `+6C`) set the two deaths apart:

| | Assault craft | Trucks |
| --- | --- | --- |
| Kind `+174`, flags `+264` | 0, `0xA0` | 2, 0 (Infest04's trucks) |
| Entry | the death sound `0x28`, a burst of sixteen type-1 debris and the blast `00531250` | the camera effect `0xF00` and the fuel blast `005307D8` before the same three |
| Ejected (`+264 & 0x380`, shifted right 7) | one: resource 2, a scientist (84 on team 18), on the craft's team, its size `+28` (240) from the craft toward the player's unit, with the [selector 66 attachment](ACTOR_EFFECT_ATTACHMENT.md) | none |
| Stage 1 | three kind-`0x83` pickups worth 500, then (bit `0x20`) a second burst and blast | the three pickups |

Both entries end by stopping the engine sound `+168`.
check_family_dying.py runs the unchanged
entry and stage over every branch these fields choose: kind 2 or not, one
or three units, the team-18 resource, the player's direction and its
absence, and stage 1 with and without bit `0x20`. The
[death tests](../../crates/inf-game/src/object_runtime/actor/derived/family_dying/selector9_tests.rs)
kill Oolbryte's truck and Terra Nova's craft on their loaded worlds and get
the order above, the truck's as Infest04 recorded it.

### Follow Player

Descriptor 8 (`00546A5C`) is the rescue. Its score `00425D30` reads the
WORD at `+23C`: zero scores nothing; otherwise it calls `+12C` (`00423B40`)
with the current flag, rereads the WORD and returns `(weight + bias) *
source`, wrapping. The source is zero while target slot 4 (`+1F4`) or 0
(`+1D4`) is held, without a first player's unit (`0046B030`) or while that
unit's `+C4` is at most 1. Otherwise the squared distance (signed WORD
differences, DWORD squares) must be above `0x40000` (512) and below the
square of `+164`, both unsigned, and `00427620` returns `+164` less its root
(`0048EFC0`). A craft between 512 and 2500 from the player scores up to
about a million, far above Wander's 12,500.

The change `00423A80` publishes "Follow Player" (`00530D20`), looks up the
unit again and, with neither target slot held, puts a 0x20 follow target on
the unit's handle at `+148` (`004825D0`; null if the allocation fails) and
the distance `+208` at the unit's size `+28` plus 0x100. Without the unit
or with a target held it sets `+22E` bit 2 instead. The update `00423A20`
is the shared timed retry. Selectors 9, 10, 21 and 36 share all three.

The port ran neither. Stendahl Alpha's awake craft stopped its behavior at
its first reselection ("score 00425D30 requires actor virtual +12C"), and
Terra Nova's would have stopped once woken.
check_follow_player.py runs the unchanged
`00425D30` (through selector 9's own table), `00423B40` and `00423A80` over
27 scores and 14 changes: the 512 and radius edges (a negative radius at the
edge itself), WORD wrap, a bit-31 radius, coefficient and bias wrap, each
gate, the allocation failure and the low byte of `+22E`. The [port](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_follow_player.rs)
matches all 41 ([tests](../../crates/inf-game/src/object_runtime/actor/derived/descriptor_follow_player_tests.rs)).

### Under the player's hand (`+44`, `00424C40`)

The Cranial Implanter's take (`0046AE90`) accepts selectors 9, 10 and 21
through `00405750`, which only returns 1, so a sleeper stays asleep. The
player's update then calls their `+44`, `00424C40`: selector 36's
`00481A50` without the counters and the engine sound. In order: the water
(`00429D60`), the control block's intents (`00428490`), the weapon (its tick
`0048D030`; with Fire held and the selected entry ready, `0048D0D0` along
the nose), the class's own `+118` on the intents (selector 9's `00424CF0`,
selector 10's `00425400`, selector 21's `00405600` with its swings) and the
status (`0047C990`). check_family_drive.py
records that order and the intents over 44 drives, over every `+254`
table, the craft's and trucks' `0052FF08` among them; the port's intents
and fire decisions match
([test](../../crates/inf-game/src/object_runtime/actor/derived/player_drive_tests.rs)).
Selector 12's `+44` is still not run.

### Terra Nova's fight, in retail and in the port

None of Infest01..04 runs a craft
(census),
so one was recorded, Infest05
(ledger).
Its player loads a Terra Nova profile and drives toward the craft; both
wake together (clock 919) as the buggy comes within the beacon's trigger
radius, 4,500 from it. Their slots fill, and both fight the spirax
(resource 86, placement 16) and a quixle (resource 87), alien selector 10s
on team 18. The recording and every port run below agree on what the fight
is made of: a craft's shot (strength 250) takes 250 from the spirax, the
spirax's (550) 450 from a craft and the quixle's (350) 250; craft 83 dies
to three spirax hits (to 550, 100 and 0) and craft 84 to seven; the ejected
pilots, scientists (selector 12), fight on (strength 200), and their shots
at the wreck do nothing.

The timing depends on two things a recording fixes and a port run has to
match. TTD runs the game at 125 ms an update (the delta `0055F0E0` is
125,000 throughout), and the update loop moves each actor to the front of
the other list (`00478FB7`), so the order reverses every pass; in the
recording's wake frame both craft update and fire before the spirax. The
port's start harness wakes the craft at the load, with the buggy at
(24500, 22500) or near the recording's (21148, 21680):

| Ticks (48 a second) from the craft's first hit on the spirax | Craft 83 dead | Craft 84 dead | The spirax's hits on craft 83 |
| --- | --- | --- | --- |
| Retail (Infest05) | 69 | 188 | +19, +50, +69 |
| Port, 16 ms updates | 34 | 154 | +0, +16, +34 |
| Port, 125 ms updates | 44 | 194 to 219 | +7, +19, +44 |
| Port, 125 ms, the recording's update order | 62 | 181 to 231 | +6, +31, +62 |

The 125 ms rows are ten runs each from starts within 120 of the
recording's position, the last with a temporary probe that reverses the
port's update list once at the load; craft 83 comes out the same in all
ten. With both matched, the craft's own shots land when retail's do (+0,
+25, +37 against +0, +25, +38), craft 83 dies within an update of the
recording and craft 84's range holds its 188. One difference is left, the
spirax's first volley: retail's, fired in the wake frame, missed craft 83
as it began to move, and its second (fired at clock 944) landed at +19;
the port's first lands at +6. That volley turns on where the wake falls
in a pass and on the random state, which a wake at the load does not
reproduce.

Each craft runs the dying entry, sinks (lifecycle 0) and leaves; in the
recording the second entries come at clocks 2294 and 2413. In the port the
buggy dies there too, and Stendahl Alpha's craft attacks from the load and
falls from 5000 to 1570 in its first 12 s.

On Oolbryte, taking the supply truck through `0046AE90` and holding Thrust
drives it about 5,700 units under `00424C40`. Meanwhile the vehicle's own
`+24` (`00416090`) runs its controlled update braked, as
[a parked vehicle](VEHICLE_DRIVE_RUNTIME.md#a-parked-vehicle-00416090-alive)
does in retail.

## Selector 10

`00425190` invokes `004236D0`, installs vtable `00520CC8` and clears owned
pointer `+26C`. DWORD `+268` is untouched. Payload wrapper `00425270` runs
the virtual context method followed by `00425290`: common `00423850`, then
the full DWORD at payload `+74` copies to `+268`. Selectors 10 and 21 share
this proven payload/command chain; selector 21 construction remains separate.

Command `00425240` forwards signed codes at most 24 to `004239E0`. Code 25
succeeds without an operand read; larger codes reject. In particular, code 24
still replaces retained `+23C` independently of the helper selector.

Postinitializer `004252B0` selects these owned helper allocations before
running `00428390` behavior selection and common `004250C0`:

| Full DWORD +268 | Descriptor | Header bytes | Child count | Child bytes |
| --- | --- | --- | --- | --- |
| 0 | `005307F8` | `34` | 6 | `50` |
| 1 | `00530900` | `34` | 4 | `50` |
| 2 | `00530A08` | `34` | 4 | `50` |
| 3 | `00530B10` | `34` | 2 | `50` |
| 4 | `00530C18` | `38` | 2 | `74` |

Byte sizes are hexadecimal. The child counts are unsigned WORDs in immutable
executable descriptors. Other full DWORD values preserve the current pointer.
For 0..3, helper `00445AE0` has final vtable `00524EA8`; mode 4 uses
`00446BC0` and vtable `00524EC8`. Common `00445A90` clears DWORD `+08`,
sets bytes `+0C=1/+0D=0`, retains descriptor `+10` and clears all eight child
slots at `+14..+30`. DWORD `+04`, WORD `+0E`, and the mode-4 extra DWORD
`+34` remain unknown. Each child runs `00445A70`, writing only vtable
`00524E88`, DWORD `+14=0` and WORD `+36=0`. Mode-4 children then replace
the vtable with `00524EB8`; all other bytes, including their extension,
remain unwritten.

Both helpers delete through `00446C60 -> 00445B90`: restore common vtable
`00524E98`, directly free nonnull children in ascending slot order, then free
the header. They do not invoke child virtual destructors. Selector destructor
`004251D0` releases this whole helper and nulls `+26C` before the inherited
probe/common/behavior destruction chain. The
[portable helper owner](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper.rs)
keeps these allocation classes and unknown fields explicit.

The normal factory finishes once. A recognized selector on native re-entry
would overwrite `+26C` without freeing the previous helper. The portable API
reports `Selector10HelperReplacementBoundary` for that separate path; it does
not silently free or reuse the old allocation. Default-selector re-entry
preserves an existing helper, as in the original. Later helper updates and
selector-10 movement/rendering callbacks remain open.

Two first-world `quixle` placements now use this factory owner.

### Mydons: neutral until shot

Terra Nova's two mydons (resource 96, model `mydon`, placements 23 and 24)
are selector 10 on team 1, with 4000 health and radius `+164` 2500. Their
retaliation bytes `+7C/+7D` are 0, so the nearby scan
[`0047C840`](ACTOR_RUNTIME.md#nearby-retaliation-and-family-target-reaction)
never runs for them. Bumps (channel 1) cost them nothing, and they leave
the player alone until hit. In the bridge recording the player's cannon
(strength 200, channel 6) takes one from 4000 to 3800 at `1D313B:26F7`:

1. Its `+B4` reaction `00428040` sets `+22E` bit 0, records the vehicle in
   its own target table (`00482E30`: `+1A0` count 1, `+08` = `07800000`),
   runs its target selection (`0043F3D0`) and broadcasts the vehicle
   (`004281B0`) to units of its team and resource within a quarter of
   `+164`, 625.
2. At its next behavior choice (`004294A0`, 0.15 s later) Attack
   (`00429B00`, status `00531244`) wins and puts the vehicle at `+148`.
3. 0.4 s after the shot it fires: channel 2, strength 1500, which the
   vehicle's shielding turns into 140 to 182 a hit. It fires again every
   1.3 to 1.7 s.

The other mydon stood 701 from it (667 across, 215 along) and heard
nothing; it never attacked. In the port, where the two graze about 375
apart, the broadcast reaches both and both attack, as the original does at
that distance ([test](../../crates/inf-game/src/object_runtime/actor/damage_family_tests/mydon_retail.rs)).
Whether mydons fight the aliens is open: no alien came within their reach
in the recording, and a team-1 unit's choice of enemies goes through the
process tasks' relations ([target selection](FAMILY_TARGET_SELECTION.md)).

### Calls and the frame rate

A creature's call comes from its behavior choice `004294A0`: each choice
draws one in four (`00429626`), then `+17E` or `+17C` (`00429642`) plays
through `00487F10(0x10000, 0x10000)` at the actor (`004296F9`). The slots:
quixle 33/37, spirax and mantomorph 31/33, mydon and mutant mydon 43/43,
morphix 35/74, deamont4 33/0, and selector 36's torq, deamont2, tankbot and
nmehovercraft 37/0. A choice comes when the behavior's time `+19C` runs out
(0.5 to 7.2 s) or while `+22E` bit 0 is set: by the periodic scan's hit
(every 0.25 to 0.51 s) or a full scan, by every packet through `00428040`,
a contact's channel-1 bump included, and by another team's pair
`00427C20`. The updates (`00424830`, selector 36's `00481710`) choose once
a frame (`+D8`, `00425AF0`), so a creature in contact chooses every frame
and calls at a quarter of the frame rate.

Retail does that at any rate. From Terra Nova's start moved 650 south of
the mydons, 60 s at each step
(tampered-run: `runtime_re/lab/scenarios/mydons-calls-33ms.json`
and `-16ms.json` (a constructed save, world 2's first start moved)), retail
gave 0.33 and 0.28 calls a bump at 33 and 16 ms; the two mydons in contact
called 26.5 times a second at 16 ms. At 60 fps they also walk slower
(median speed 12 to 28 against 80 to 96 at 30 fps, from the legs' tick
`00445CD0`), so they stay in contact longer: 13 s of the minute against
5.5 s, and 6.4 calls a second against 1.2. The port, which ticks at most
about 61 times a second, did the same at 16 and 8 ms. `paced_actors` updates
selectors 10 (21 with it) and 36 at retail's 30 fps step
([high frame rates](GAMEPLAY_RUNTIME.md#high-frame-rates)); its
[test](../../crates/inf-game/src/session_platform/intro/runtime/tests/family_prefix_tests/creature_pacing.rs)
bumps a mydon every tick and finds its choices and calls only at the paced
updates.

### Legs: custom vertex 00425870

Selector 10's table `00520CC8/+10` is `00425870`. Low-byte selectors 0, 1
and 5 are its own; every other low byte goes unchanged to `004260C0`. With a
helper, it first calls helper `+0C`, which is `004463E0` for both helper
classes. Unless lifecycle `+C4` is 1, that is all. Otherwise it fetches the
hip row (extra1's low byte) through `00439E10`. Without a helper the row is
the output. A clear process byte `0053B890` then lets a trail gate run: a
`0048F060` draw below `0x400`, then a second draw against
`+188 / (table_244[+240] >> 16)`. The divisor is read before the index range
check. Index 0 births type 11 or 8, index 1 type 6 inside a window, 2 type
6, and 3 type 5 above the quotient. Three more draws set the velocity, and
`00489D90` gets the hip row with flags `0x800`.

`004463E0` solves the leg named by the selector's high bits once per frame,
while child `+14` bit 1 is clear:

- Rows: the rest foot is extra0's high byte, the hip extra1's low byte and
  the rest knee extra1's high byte. The model's records list them for every
  leg.
- Setup: a child without bit 0 runs `00445F70`. It stores the hip at
  `+04`, the squared thigh `+44` and shin `+48`, the rest span `+38`
  (`0048EFC0` root), the reach `+3A` and its square `+4C`, half the span
  `+3C`, the stride `+3E = 3 * half >> 2` and its square `+40`. `+0C/+10`
  then take the span and half as DWORDs.
- Foot: with helper byte `+0C` clear, or on the first frame (bit 4 clear,
  which it then sets), the foot `+18` is the rest foot. A standing leg
  (`+36` zero) grounds the foot through `00450A30`, or the higher of terrain
  and `00410F60` water when helper byte `+0D` is set. `0043A380` probes the
  rest foot along the actor's up row (basis `+54`) for twice `+3C`, and
  `+24` takes the foot minus that point. Bit 3 set only clears itself.
  Otherwise a step longer than `+40` sets bit 2 and shortens to
  `(3 * L) >> 2`, where L comes from the root of `(u32)(-d²)`: `0044671B`
  negates before an unsigned QWORD load. A foot beyond reach `+4C` returns
  onto the hip-foot line at `+3A`, divided by the f32-rounded length.
- Stepping (`+36` nonzero): the target `+2A` (rest foot plus step) is
  probed the same way in place. The foot moves from the start `+30` by
  `+36/32768`, lifted by `+38/4` times the quarter-sine `0053DF00` arc.
- Knee: base child `+8` is `004460B0`, with the descriptor WORD at
  `table + 16 * (index + 1)` as its mode. The hip-foot unit (double length,
  f32 components) crosses basis row 1, 2 or 0. Modes 0..2 use unit x row,
  3..5 row x unit, and both cross the unit again. The two-bone height comes
  from `+44/+48` and the knee lands at `+1E`.

Selector 0 returns the foot, 1 the knee, and 5 the child's `+0`: NULL for a
base child, which leaves the output unwritten, and the child itself for an
extended one, whose ankle `+56..+5A` is the output. `00425870` sends every
other low byte to `004260C0`. The helper sets bits 0 and 1 afterwards; the
`445CD0` tick clears bit 1 and runs the steps. The helper saves and restores
the published transform block `0055F190..0055F26F` around the solve, and
nothing in between writes it.

Extended children (helper mode 4) have three-segment legs:

- Setup `00446EE0` also reads the ankle row (extra0's low byte). It stores
  each segment's squared length and root: `+44/+6C` hip to knee, `+48/+70`
  knee to ankle and `+60/+64` ankle to rest foot. The reach `+3A` is their
  sum, `+68` the lower two's sum squared, `+38` the hip-foot span, `+5C` a
  quarter of it and `+4C` the reach squared. The first two legs take half
  the thigh as `+3C` and `+3E`; the others take the reach's excess over the
  span as `+3C` and three quarters of it as `+3E`. `+40` is `+3E` squared.
  It keeps no copy of the hip.
- Knee `004470C0` also takes the descriptor WORD at `table + 16 * index +
  0x12` (1 for both of mode 4's legs). A foot at or beyond the reach is
  pulled onto the hip-foot line at the reach, with the knee and the ankle on
  that line. Otherwise the bend is `004460B0`'s. The knee's distance along
  the line comes from the law of cosines against the foot (WORD zero) or
  against a point the toe short of it. Where the thigh exceeds it, the lift
  `(2 * rest - toe / 2) / 2`, held between 0 and the toe, shortens that
  distance and raises the knee along the bend by the remaining height. The
  ankle then lies on the knee-foot line by the same law between shin and toe,
  bent along the first cross product, or against it for WORD zero.

The [leg owner](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper/legs.rs)
keeps child bytes unknown until a writer reaches them. Its
receipt runs the original code over 75
`0043A380`, 30 `00445F70`, 90 `004460B0` and 78 `00425870` cases (518
calls, 25 trails), and over 30 `00446EE0`, 166 `004470C0` (16 of them with
the knee-foot distance on shin + toe or one below) and 20 mode-4 `00425870`
cases (168 calls). Targeted cases put the second trail draw exactly on its
quotient, `q/4` and `q/2`, and drive both clamps where f32 and f64 truncate
differently. Terrain, water and births are recording stubs; every
other callee runs unchanged.

### Mode 4: the walker tick 004476D0

The nemian in world 1 (leaf 11, placement 10) and the rhino in world 4
(leaf 14, placement 12) are selector 21 in mode 4. They walk on two extended
legs, and their helper's `+04` is `004476D0` instead of `00445CD0`. No
recording builds one: `00446BC0` has no calls in Infest01..05.

- The forward speed is the velocity WORDs `+8E..+92` dotted with basis row 2
  (`+60..+68`), truncated, made positive, shifted up 12 and divided by the
  actor's `+28` (zero faults). A two-child descriptor selects gait table
  `00536A78` (two legs; thresholds 409 and 4096), any other `005368E0` (four
  legs, three gaits). The gait, stored in helper `+04`, is the first whose
  threshold is at least the speed, or the gait count past the last.
- The stride phase is the helper's WORD `+34`, which `00446BC0` never
  writes; nor does it write most of the legs' fresh `0x74`-byte blocks, whose
  foot targets `+24..+28` `00445CD0` reads before a swing writes them. The
  port starts them at zero, as fresh heap blocks hold them
  (`FreshHeapZeroExtendedCarryApproximation`, `initial_extended_word`); held
  unknown, the walker stopped at its first update and Exxodus's TL717 never
  moved, fought or died. A step is
  `(value * (microseconds << 10)) >> 31` against the gait's period: equal is a
  whole period, `0x7FFF`, otherwise `(x << 15) / period`. Gait 0 steps a
  nonzero phase by the mean of its threshold and the speed and clamps it to 0
  once it turns negative, so a standing walker finishes its stride and rests.
  The other gaits add the speed's step within `0x7FFF`. Between two gaits the
  period and each leg's four WORDs blend by `(speed - low) / (high - low)`:
  `00447AE0` lerps, and `00447B00` turns the swing angles the short way round
  `0x8000`.
- Gait 0 then runs the common `00445CD0`. The others run `00446C80`: the
  velocity less its part along basis row 1, made a Q31 unit by `0048F080`
  (halved under `0x6882`, a bitwise root plus one, saturated by sign) and
  scaled by `+28` times the period `>> 12`. A leg whose phase lies between its
  first two WORDs swings: its start `+30..+34` is copied on the first step, its
  timer `+36` takes the fraction and its target `+24..+28` the unit times the
  third WORD `>> 8`. A swinging leg out of its window lands: its timer clears,
  `+14` gains bit 3, and the first two legs sound resource `0x18` at the foot
  (volume `0x4000`, pitch `0x10000`). Every leg clears bit 1, and the return
  counts the landings. `00450A30`'s sample under each foot and `00447B30`'s
  keyframe reads are discarded; no key pair in either table makes `00447B30`
  divide by zero.
- Each child's `+5C` becomes its rest span `+38` times the gait's fourth WORD
  `>> 15`.

Neither `00446BC0` nor `00445A90` writes `+34`, so the first call reads heap
residue. The port stops there (`RequiredHelperField` `+34`) and holds the
walker until a value is decided. The
[walker](../../crates/inf-game/src/object_runtime/actor/derived/selector10_helper/tick/walker.rs)
matches the original over its receipt's
103 cases and 226 ticks: every return, helper field, child byte, RNG state
and footstep, across both tables, every gait, the phase's edges, each step's
`0x7FFF` arm and twelve-tick strides. The same receipt calls `00447AE0`,
`00447B00` and `0048F080` directly at their edges. A mutation pass killed 16
of 20 changes to the walker and the extended legs; of the four left, one is
equivalent and these direct and boundary cases now catch the other three.

### Selector 21: the Nemian

Selector 21 ("Nemian; TL717", eight placements in leaves 11, 14, 25 and
31) is selector 10 with two swings. Case 0x15 of `004603B0`
(`0046085A..0046087D`) allocates `0x280` bytes and runs `00405590`:
selector 10's `00425190`, vtable `0051F7A8`, swings `+270 = +274 = 0x2001`
and speeds `+278 = +27C = 0`. Its table is selector 10's but for `+00`
(`004055D0`, the same teardown), the model values `+14` (`00405700`: the
low WORDs of `+270` for selector 1 and `+274` for 2, otherwise the derived
`004260A0`) and the motion `+118` (`00405600`). It initializes, takes
commands and finishes as selector 10 (`00425270`, `00425240`, `004252B0`).

Its motion runs selector 10's `00425400` and then, in state 3, the swings.
A swing beyond `0x1000` takes a new speed toward the other side from the
process RNG `0048F060`: for `+270`, `0xFE000000 - ((draw & 0xFFFC) << 9)`
above and `((draw & 0xFFFC) + 0x10000) << 9` below; for `+274`,
`0xFF000000 - ((draw & 0xFFF8) << 8)` and `((draw & 0xFFF8) + 0x21000) <<
8`. Each swing then moves by the signed product of the delta and its speed,
shifted right 31 (`IMUL`, then `SHL`/`RCL` of the pair). The persistent
audio follows as for selector 10.

No recording reaches its worlds. The port builds it as selector 10 with
the swings (`Selector21Swing`), runs them after selector 10's motion, and
answers the model's selectors 1 and 2 from them. Its tests match four
hundred updates against an independent model of the disassembly at five
checkpoints, and all eight placements build.

## Selector 12

Selector 12 (`00467070`, vtable `00527058`) extends the `00429E20` terrain-probe
prefix with an embedded `00475020` grant owner at `+24C` whose notification
owner `+1C` stays null, so its grants only change bits. Allocation merges the
first player's mask through `00475350` (ascending IDs `0..AE`, each granted with
recursive dependency closure; higher source bits are not copied). Initializer
`00468310`/`00468330` retains payload `+58`; command 23 (`004672C0`) replaces
that retained address without reading it. Postinitializer `00468040` clamps the
retained WORDs `+10,+0,+2,..,+C` to 1000 (leaving `+0E`), stores terrain Y from
`00479DD0`, grants actor `+7A`, and sets `+29D` when a present first player does
not own that ID. An absent first player leaves `+29D` zero. The
[owner](../../crates/inf-game/src/object_runtime/actor/derived/selector12.rs)
requires the executable grant table and terrain before publication.
Its six placements complete mode-0 construction for all 118 first-world
placements, before later loader passes.

The later [event-22 grant-difference notification](GRANT_DIFFERENCE_RUNTIME.md)
reads this actor's live mask against the actual first player's mask and
overwrites `+29D` only when that player exists. It retains its separate
source direction and callback result from construction/postinit above.

## Selector 36

`00481110` installs final vtable `00527EE0`. It clears DWORDs
`+268/+26C/+270/+274/+280/+284` and WORD `+27E`; WORDs `+278/+27A/+27C`
remain unknown. `00482480` invokes the final context method then
`00482460`, which runs `00423850` and retains the whole H13 payload at
`+274`. A later code-24 override replaces `+23C` only.

Postinitializer `00481E80` runs `00428390` behavior selection before
`004250C0`. Commands, admission `004270E0`, after-load `004277E0` and
network `0047E2E0` use the concrete inherited owners. Three first-world
`tankbot` placements run through the production factory.

### The treads

Every tracked tank model (torq, deamont2, tankbot, torqescape, goodtorq)
draws two tread children that read model selectors 5 and 6, the low and
high WORDs of `+270`: `F5` takes the phase, `F3` divides it by 21824
(`0x5540`), and three opcode-16 branches draw one of three tread frames
(sprites 400, 399, 398) by the quotient. The update `00481710` steps the
phases in `00481180` (also reached from the player-driven `00481A50`): the
forward speed (velocity `+8E/+90/+92` through basis `+60/+64/+68`, `__ftol`)
times the authored constant `0053BDB4` (16384, with no writer anywhere in
the executable), divided by 300 toward zero, less (left) or plus (right)
the yaw rate `+98` times the same constant, shifted right 15. The step is
per update, not per second: retail at 60 fps steps the treads every frame,
about a quarter turn of the three frames a frame and half at full speed
(the lab's `exxodus-tank-treads-16ms`: Exxodus's six tanks, speed up to 607). Under the
port's `paced_actors` a tank updates every other world tick, so stepped at
its updates alone the treads turned at half that rate and flipped between
two frames (2026-10-27: the tracks should move as the enemy moves,
but did not in the port). The step reads only the tank's own velocity,
basis and yaw rate, and only the model reads the phases, so the held ticks
step it too (`tank_treads.rs`): the treads turn as at 60 fps, and the rest
of the update keeps its paced step.

The shared packed cache below owns `00454ED0`; its actual terrain/collision
miss callback is composed through the [shared entry/scan](FURNITURE_COLLISION.md).
The first active-query boundary, remaining class callbacks and complete mission
behavior stay open. Construction does not make phase 3 or a complete mission
playable.

## Shared packed probe cache

[`TerrainProbeCache::probe`](../../crates/inf-game/src/object_runtime/actor/derived/terrain_probe/access.rs)
owns `00454ED0` for both constructor dimensions 16 and 32. X/Z inputs consume
unsigned low WORDs; `SHR6` selects 64-unit cells. Relative coordinates and
origin updates wrap as WORDs. Nearby forward/backward changes slide X through
`00454D70`/`00454E30` and Z through `00454C60`/`00454CF0`; a far change clears
all packed words and replaces both origins. Header bytes +08/+0B remain unknown.
There is no null-cache fallback: entry `00454EE6` immediately reads cache
WORD +04, and scan `0042A0D0..0042A0F2` loads actor +234 and calls directly.
A missing cache cannot be substituted by an uncached `00429EE0` call.

X movement operates on adjacent DWORDs with x86 counts masked modulo 32.
An eight-cell shift retains that original policy; it must not become a
whole-DWORD move or a generic nibble-array copy. Z movement copies rows in
the direction that preserves overlap, then zeros exposed rows. Reading
`00454BA0` returns the low four-bit encoded sample; nonzero returns sample-1
without invoking the host.

On a miss, the required synchronous `00429EE0` service receives the **same
mutable cache**, after all window changes. A service error retains those
changes and its own mutations. Completion retains the pre-callback relative
coordinates while rereading the current stride/storage, as `00454BE0` does.
It consumes the service's low BYTE, adds one, and ORs the complete DWORD at
the captured nibble shift. There is no guessed four-bit mask or destination
clear: large supplied results can spill into another nibble or wrap out of
the DWORD. If a callback shortens storage below the captured write address,
the port reports the reached allocation-range error after preserving callback
mutations. The owner does not temporarily remove an actor's cache from the
arena, fabricate a collider, or initialize allocation WORDs +220/+230.

The unchanged-byte
`check_terrain_probe_access.py` checker
and [sequence fixture](../../crates/inf-game/tests/fixtures/terrain_probe_access_native.json)
cover 137 supplied sequences and 350 accesses: both dimensions, 64-unit cell
edges, WORD origin/relative wrap, 8/16-cell shift crossings, hits/misses,
encoded spills, post-callback stride changes and failure prefixes. The constructor's
allocator storage and mutable miss service are explicit supplied boundaries;
the positive allocation is checked before a post-callback cache write.
This cache receipt supplies the miss boundary. Separate
[terrain and miss owners](TERRAIN_RUNTIME.md#shared-actor-terrain-probes)
implement `00450BD0` and `00429EE0`. The production
[collision prefix](FURNITURE_COLLISION.md) now executes the reached entry and
skip-only scan through the real mutable host. An ACTIVE candidate retains its
typed context/query boundary; complete actor +24 and cinematic parity remain
open. Unknown header poison remains native custody evidence.

### Captured actor-cache allocation

The shared [allocation owner](../../crates/inf-game/src/object_runtime/actor/derived/terrain_probe/allocation.rs)
keeps actual actor `+234` installed while an arena miss service runs. A short
prepare phase applies the existing window/read algorithm; a hit bypasses the
service. A miss captures native relative cells and an opaque allocation identity,
then releases the cache borrow before passing the actual mutable arena and actor
to the callback. The token retains no decoded cache or storage.

Only normal callback return reacquires current `+234`, verifies the original
allocation, and encodes the low BYTE plus one using captured cells and fresh
stride/storage. In-place header or buffer changes retain identity; a decoded-equal
clone creates a different allocation. Removal, replacement, actor deletion or
slot reuse fails at the reached owner check without restoring old state or
writing a sample. Callback errors preserve their cache, arena and service prefixes.
The existing supplied mutable-cache API shares the same prepare/finish algorithm.

Selector12 and Selector36 avoidance consume this installed-cache service with
one explicit terrain/miss host. The production
[429EE0-to-47C1D0 composition](FURNITURE_COLLISION.md) retains the same actual
actor/header while the callback runs. Normal skip-only return restores actor
XYZ/guard before captured-cache completion; a reached ACTIVE boundary preserves
temporary snapped XYZ, guard and uncompleted sample. Auxiliary/contact execution
and complete enclosing callbacks remain separate owners.

## Shared obstacle scan

[`scan_family_obstacles`](../../crates/inf-game/src/object_runtime/actor/derived/avoidance/scan.rs)
owns `00429FF0` independently of a scene. It clears +22E above bit 0010,
quantizes persistent basis +60/+68 by CRT truncation after scaling by 4096,
and computes the unsigned wrapped DWORD squared length. Normalization uses
signed low-WORD numerators, a float-spilled denominator and normalized Z;
normalized X remains double precision until its final 224-unit multiplication.
A zero length uses horizontal X=1/Z=0. Reverse changes the signed distance.

The captured initial X/Z feeds six ordered probes: center, both perpendicular
sides, forward center and both forward sides. A nonzero DWORD return sets its
bit only after the callback, preserving callback writes. All six bits set
adds a farther center probe; current flags then decide the two rear probes.
Rear coordinates reread current actor position, subtract the captured forward
offset and add/subtract signed WORD arithmetic halves of the perpendicular.
Every coordinate wraps as a WORD. Each call reloads actual +234; a missing
owner fails at that reached probe without substituting direct terrain sampling.
Errors retain flag resets, completed probes and callback mutations. No virtual
basis query, timer initialization or scene policy is inserted here.

check_family_obstacle_scan.py executes
unchanged scan/CRT instructions against explicit cache-probe boundaries.
Its [208 supplied receipts](../../crates/inf-game/tests/fixtures/family_obstacle_scan_native.json)
cover all six-bit outcomes in both directions, every extra-probe outcome,
quantization/float spills, zero/nonfinite/extreme vectors, coordinate wrap,
changed flags/position/cache and missing service/position prefixes. These
establish geometry and dispatch, not collision results or full +24 execution.

## Native evidence and tests

The [selector-10 fixture](../../crates/inf-game/tests/fixtures/selector10_native.json)
contains eight cases, 161 snapshots and 118 heap boundaries. It covers all
five helper choices, full-width unmatched values, poisoned unknown storage,
null/nonnull behavior, inherited payload/commands/postinitialization and the
complete supported allocation-release order. Default-selector re-entry retains
the helper. As with selector 36 below, original code runs unchanged between
explicit heap boundaries, with separately supplied positive storage and
postfree continuations; allocator/deallocator implementations do not run.
These receipts do not establish complete factory execution or later movement.

The recorded selector-10 endpoints
independently correlate first-world placements 67/68 with this owner. Both
factory-return states have full selector `+268=1`, a `00524EA8` helper using
descriptor `00530900`, four `00524E88` children with the constructor's zero
fields, and four null child slots. Their probe dimensions/stride are 16/2
with 32 zero DWORDs, and `+218/+140` are nonnull. Unwritten helper bytes
differ between the two actors, supporting unknown state rather than assumed
zeroes. These two read-only endpoints do not prove later helper use or the
complete allocator/destructor timeline.

The [selector-36 fixture](../../crates/inf-game/tests/fixtures/selector36_native.json)
contains 8 cases, 148 snapshots and 72 explicit heap call boundaries. Original
constructor, guard, postinit and destructor code runs unchanged between those
boundaries; positive poisoned storage is supplied as an input to each
postallocation tail. Free-call arguments and order are observed, followed by
separately supplied postfree continuations. The allocator and deallocator
implementations do not execute. No instructions, game callbacks or functions
are patched or replaced, and these are not complete heap/factory receipts.

Cases cover real guard segments with null and nonnull behavior, helper
allocation/reuse, payload table switches, shared command behavior, lazy
zero-denominator reads, inherited after-load/network policy and release order.
The [probe fixture](../../crates/inf-game/tests/fixtures/terrain_probe_native.json)
separately covers BYTE inputs 0/8/16/255, exact extents, cleared contents,
untouched header bytes and matching free-call custody. Production tests use
current resident H13/H14 data and explicit construction inputs; they do not
establish the later complete loader timeline.

SHA-256 values:

- Selector 10: `E2A5963CB7A38910F34CD0F7B77EB57A5332FDA70B02E800D350E4D40A497E7B`.
- Selector 36: `A8F86DD915687D4BA5D492DF02C2F1715957006F42D3AEE7FE82283327F05673`.
- Probe: `E91371915792FCBDE11B05816BBF73D0188E113E71B1A8F76FA053CFC1635CC2`.
