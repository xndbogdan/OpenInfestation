# Infestation native model execution

This document owns runtime Handler-8 execution boundaries. Intrinsic entry and
command layouts remain in [format documentation](FORMAT_DOCUMENTATION.md);
frontend transforms and submission order belong to
[frontend runtime](FRONTEND_RUNTIME.md). The diagnostic viewer's partial mesh
projection is not a production interpreter.
The [retained graphics and ACTIVE auxiliary owner](MODEL_PROCESS_GRAPHICS.md)
consumes shared renderer publications, the process byte arena and selected-table
history through the same projector. Auxiliary entry retains its separate setup
and grammar; it does not borrow invented camera or surface defaults.

## Packed values and register ownership

[`ModelValues`](../../crates/inf-game/src/model_values.rs) implements packed
operands (`004ED4B0`), register instructions F0..FF (`004A0260..004A0770`), and
the value comparison of branch opcode 16 (`004A0C90`). It consumes canonical
decoded instructions and the original 4096-word sine table at `0053DF00`.
It supplies the value state to the program executor described below; it does not
emit geometry by itself.

Packed literals rotate `(raw & 0xFF03)` left by `(raw >> 2) & 15` in 16 bits,
then complement when bit 6 is set. With bit 7 set, bit 6 selects a register;
otherwise the callback receives `raw & 0xFF3F`. Callbacks receive the live
invocation. F0..FF evaluate left before right; branch opcode16 (`004A0C90`)
evaluates right before left. A callback's writes and failure prefix are visible
at that actual order, including when the other operand reads a register.
A taken branch continues at the first argument WORD plus the jump in WORDs
(`004A0CC4`) and a fall-through at that WORD plus three (`004A0CD1`), so a
jump of 3 continues at the next instruction either way: the comparison
decides nothing. Such a comparison still reads both operands, but an
unwritten register there reads as nothing instead of stopping the model
(`ModelValues::branch_operand`); see the factory model's
[register 17](FAMILY40D800_RUNTIME.md#the-models-dock-point-and-chimney).
FA passes the full unsigned sum of
the two evaluated 16-bit operands, without truncating or remasking that sum.
The [live object getter owner](OBJECT_MODEL_CALLBACKS.md) now supplies
`004789B0` clock/captured-source values and admitted actual-class `+14` fields
to both common and late consumers of the [shared object manager](OBJECT_DRAW_RUNTIME.md).
Packet identity, required custom backing and remaining simulation writers retain
explicit feature boundaries.

The model lookup at `0049FEA0` uses the signed low-word resource index.
`0049FEF0` performs the outer model work and `004A00E0` constructs the execution
context. The independent value words at context `+0x70..+0x9B` are **not
initialized** by top-level construction. The Rust owner represents them as
unknown and rejects a read before a proven write; zero is not a substitute.

Although packed indices have six bits, only indices 0..21 are independent
registers. Indices 22/23 alias the parent-context pointer at `+0x9C`; indices
24 onward overlap the saved basis beginning at `+0xA0` and other context
storage. These accesses require the full context owner and fail explicitly
in the isolated value engine. Destination validation before callbacks is a
Rust containment rule, not emulation of unchecked native memory writes.

External child opcode 0E (`004A0ED0`, copies at `004A10B9..004A10FD`) inherits
only words 0..3. It does not copy them back on return. Inline nested programs
05/09 reuse the current owner. Unknown inherited values remain unknown.
Completed writes survive an execution failure, but that invocation stops;
inspection remains available for diagnostics.

### Feature-level unowned value containment

[`SceneModelSubmission`](../../crates/inf-game/src/frontend_scene.rs) selects
an explicit `ModelFailurePolicy`. Existing frontend, name-entry, briefing,
world and furniture callers retain `StopScene`. Ordinary loading selects
**UninitializedValuePrefixApproximation**: only the exact independent-register
read-before-write fault can stop the current model remainder while admitting
its completed packet/publication/RNG prefix. The actual reset-owned implicit
root must be the sole running open group both before and after execution.
Nested, closed or stopped queues, context-alias reads, unsupported operations,
resource/backend errors and execution limits still propagate. This does not
reset the queue, unwind native groups, restore an earlier view or fabricate a
register value. The failed transient invocation remains stopped; a later
invocation starts with unknown registers again. The caller must execute its
actual common flush successfully before any scoped resource batch can retire.
`model_value_prefix_skips` counts these omissions separately from complete
models and the first omission is logged.

The canonical parent1 H8/117 loading model exposes this boundary with selector2
and pulse other than `FFFF`: root command `+42` enters child119; its `+144`
computes register17 from pulse and `+14C` can enter child131 at `+154` before
parent register3 writes at `+19C` or `+42A`. Child131 queues four opcode07
edges at `+18/+22/+2C/+36`, then its saved-basis opcode03 at `+40` reads packed
register3 (`00C3`). Child0E inherited the still-unknown parent word. Neither
program opens a depth group before this read. Selector1 exercises a different
path and cannot validate this one. Native root construction leaves independent
registers as stack history; neither zero nor a process-persistent register bank
is justified. The follow-up owner is native model-context stack/input custody.
This approximation preserves a bounded rendering prefix and loading continuity;
it does not count as complete model or retail visual acceptance. The loading
draw now seeds that register under
[LoadingModel117StackHistoryApproximation](#loadingmodel117stackhistoryapproximation)
and keeps this prefix only for any other unknown register. The source
model bytes and reached boundary are covered by the
[loading model tests](../../crates/inf-game/src/loading_model/tests.rs).

### WingModel457StackHistoryApproximation

`004A0ED0` builds each child context in its own `0x1F0`-byte stack frame and
copies only registers 0..3 (`+70..+76`) from the parent at
`004A10B9..004A10F6`. Every other register is whatever that stack held. Both
griffin roots, the selector-35 flyers `demont3` (H8 448, the intro's) and
`gryfin` (556), set register 9 to `clock << 10`, then draw wing child 456
once per side; 456 draws grandchild 457 (`gryfwing2`, the wing's middle).
457's `F4 [10, C9, 0]` copies register 9 into register 10 before anything in
457 writes register 9, and register 10 then turns the wing tip 458's saved
basis (opcode `1C` on axes 2 and 1). In the port that read stopped the
model in its first wing's middle, so the griffin lost that wing's tip and
its whole other wing (and `demont3` its last two root primitives). Every
world that loads 457 has this read: levels 13, 15..17, 21, 27, 29, 30, 32,
33, 35..39 and 43. Each root carries its own copies of 449..458, whose
wings differ only in their normals (`gryfin`'s point up, `demont3`'s are
zero); levels 35..39 and 43 load both, and the later registration owns the
global slots.

Retail reads whatever last ran at that stack depth. In Infest01 the record
is resident at `047E1E84`, and a TTD script seeking to every read of its
operand word found 830, all in the intro, all with child context `1AF4E0`.
Register 9 held `56` in 796 of them, 0 in 27 and `472` in 7. The `56` is the
high word of `00565008`, a pointer into the process scratch arena that its
allocator `004EBE60` stores there at `004EBEB8`; the first read's 0 came
from a DWORD that `004F2FA4` in the auxiliary interpreter stored there.
Infest04's gameplay reads, again in context `1AF4E0`, find heap addresses
instead (ledger):
`004F1420` pushes the auxiliary interpreter's record pointer there at
`004F142B`. Oolbryte's 347 reads hold `C9F` 209 times, `C92` 36, `C7` 69
and 0 33; Helniazine's 68 hold `C92` 40, `F73` 22 and `C98` 6. In the
sampled reads, the `C7` came from an ntdll pointer and the 0 from the
publication's `FISTP` at `0048FE45`. Both wings of a frame read the same
word. None of these is a game value: they are this machine's and session's
addresses. At `56` register 10 bends the tip about 0.4 degrees; the heap
words bend it 14 to 17.

The object draw's `ModelStackRegisterPolicy::ObjectStackHistoryApproximations`
(alone, `WingModel457StackHistoryApproximation`) starts register 9 of global
H8 457 at the intro's `56` in every world, so every griffin draws both wings
with one fixed tip. Any other uncopied register, and the same model outside
object draws, stays unknown. The follow-up owner is native model-context
stack custody. The policy is logged once per process and covered by
`griffin_wing_model_457_register_9_needs_the_stack_history_approximation`.

### LoadingModel117StackHistoryApproximation

The loading model H8/117 (level 1) enters child 119, and for selector 2 with a
pulse other than `FFFF`, child 131. Child 131's opcode 03 at `+40` reads packed
register 3 (`00C3`) before anything in that chain writes it. The root context
is a stack local of `0049FEA0` that writes no register, so the value is stack
history, copied down with registers 0..3.

A TTD script seeking to every execution of opcode 03's angle read (`004A0AFA`)
in Infest01's post-intro loading screen (`736C1F:64F..73C25E:594`, 41 loading
draws) found 68 such reads, all in child context `1AF394`. The program there
(`0474E0F2`) holds the four opcode-07 edges at `+18..+36` and then
`0003 0000 0000 00C3` at `+40`. Register 3 held `6EBB` in 64 reads and 0 in the
four reads of one draw, whose register 0 was also 0. The 20 loading draws
before the intro never reached this read.

The loading draw selects `ModelFailurePolicy::LoadingModel117StackHistoryApproximation`.
Through `ModelStackRegisterPolicy` it starts only that root resource's
register 3 at `6EBB`; any other unknown register still keeps the prefix as
under `UninitializedValuePrefixApproximation`. The value records one session's
stack, not a native initializer. The policy is logged once per process and
covered by `retail_selector_two_starts_register_three_from_infest01_stack_history`.

### VehicleModel117StackHistoryApproximation

The vehicle draws the same H8 117. While a mode change is in progress (its
getter 2, the transition progress `+1EC`, is not `FFFF`) and the mode table
gives it 2 (the truck), child 131 runs and reads the root's register 3 just
as on the loading screen. Before this policy the read stopped the vehicle's
model, so the vehicle vanished for the morph's frames.

In gameplay the copy is not stable. A query over Infest01's 682 transition
updates found 2813 such reads by program `0474E080`, with about 940 distinct
values, mostly in pairs that sum to `10000` within a frame (`A442`/`5BBE` 156
times each, `D924`/`26DC` 34, `A652`/`59AE` 22, ...): per-frame residue that
looks like an angle and its negation. No recorded value is the native one.
The object draw selects `ModelStackRegisterPolicy::ObjectStackHistoryApproximations`,
which applies the wing seed above and starts this root's register 3 at the
loading screen's `6EBB`, so the morph draws with one fixed turn. It is
logged once per process and covered by
`vehicle_model_117_register_3_needs_the_object_stack_history_approximation`.

### SkimmerModel127StackHistoryApproximation

The settled skimmer (the mode table's 5, progress `FFFF`) draws the root's
child 127. Its `0116 [21, reg15, 0]` at `+464` (source byte `18104`)
reads register 15 before the program's only write to it, `FB [15, reg2,
A000]` 63 instructions later, and skips three sprite-599 triangles over
black ones when the register is 0. Before this policy the read stopped the skimmer's
model after its hull, so its side wings never drew.

Retail reads stack residue there, but not the previous frame's value
(`FFFE` once settled): all 16 reads in Infest03's skimmer frames
(`10C4F8..12DB26`, child context `1AF690`) found 0 and skipped, while the
weapon switch moved registers 6 to 10 between them
(ledger).
`ObjectStackHistoryApproximations` starts that child's register 15 at 0. It
is logged with the other object seeds and covered by
`skimmer_model_127_register_15_needs_the_object_stack_history_approximation`.

### ShopPreviewModel127StackHistoryApproximation

The factory shop's Facilities title (`0042FD70`) turns the selected
product's model above the list. For Skimmer mode that is the vehicle's
H8 117 in form 5, settled, so child 127 makes the same unwritten read of
register 15 as in gameplay, from child context `1AF488` instead of
`1AF690`. The preview drew with no register seeds, so the read stopped the
model, and with it the whole shown menu: on Artoria the shop
vanished while Skimmer mode was selected.

There retail's register 15 is residue that differs even between twin
runs. Three lab runs of Artoria's shop
(`artoria-skimmer-register.json`)
read `A195`, `05F1` and `D39A`, 104 reads, never 0, and every branch fell
through to the three triangles gameplay skips. The lab's hooks write the
stack at that depth too, so these exact values may be theirs. Without
hooks
(`artoria-skimmer-shop.json`),
retail's preview frames over a full turn match the port's within three
pixels of 5,580 at 640x480. The port's frames are the same whether the
seed is 0 or `A195`, so the pixels cannot tell them apart there. The
preview's policy (`ModelFailurePolicy::ShopPreviewStackHistoryApproximation`)
starts register 15 at `A195`, which falls through as every retail read
did. Any other unwritten register keeps the model's drawn prefix rather
than stopping the menu. Covered by
`skimmer_model_127_register_15_needs_the_object_stack_history_approximation`
and `the_shop_previews_the_skimmer_whole`.

### SkimmerMorphModel126StackHistoryApproximation

While the vehicle turns into the skimmer (the mode table's 5, progress not
`FFFF`), the root runs child 126 instead of 127. Like 127, it branches on
its register 15 before its only write to it (`FB [15, reg2, A000]`):
`0016 [39, reg15, 0]` and `0116 [27, reg15, 0]` choose whether its two
child-137 fins draw inside a depth group. Its child 141 then reaches
`0116 [21, reg15, 0]` with a register 15 it never writes. Before this
policy the first read stopped the vehicle's model mid-morph, so everything
126 draws after it (its child-137 fins, its other children 139 to 153 and
their faces) vanished until the skimmer settled.

Infest03's two morph frames into the skimmer (`12F250` and `131142`, before
the second entry `13299B`) read 0 in all six: 126 in child context `1AF690`,
the settled 127's, where the first branch fell through and the second
jumped; and 141 in `1AF460`, which jumped
(ledger).
`ObjectStackHistoryApproximations` starts register 15 at 0 in both. It is
logged with the other object seeds and covered by
`skimmer_morph_model_126_register_15_needs_the_object_stack_history_approximation`.

### LaserMineModel85StackHistoryApproximation

The laser mine ([selector 38](SELECTOR38_RUNTIME.md), resource 118) draws
H8 85 (level 1). Its model value 1 is the mine's armed byte `+138`
(`004556D0`). While that is 0, from the throw until the fuse first rearms
a second later, the program's `0116 [40, 81, 0]` jumps over `F5 [10, 80,
0B]` and `F3 [3, CA, 8000]`, the only writes to registers 10 and 3, and
over the first `0016 [24, C3, 1]`. The second, ahead of the third fin and
the base (`05E6`), then reads register 3 unwritten. Before this policy that
read stopped the mine's model, so a thrown mine lost its third fin and its
base until it armed.

Retail reads stack residue there. All 17 such reads in Infest03 (root
context `1AF940`, both throws) found a value other than 1, `CF2` in 13 and
0 in 4, and jumped: the fin's three red lights (`01E2/03E2`, sprite 282)
stayed dark and the rest drew
(ledger).
`ObjectStackHistoryApproximations` starts the root's register 3 at `CF2`.
An armed mine writes the register before reading it, and so does the HUD's
icon, H8 184, whose value 1 is the process word `00533760` (1 to 5 in
gameplay). It is logged with the other object seeds and covered by
`laser_mine_model_85_register_3_needs_the_object_stack_history_approximation`.

### ProximityMineModel84StackHistoryApproximation

The proximity mine (selector 38, resource 117) draws H8 84 (level 1), with
the same unarmed path: its `0116 [40, 81, 0]` jumps over `F5 [10, 80, 0B]`
and `F3 [3, CA, 8000]`, and its `0016 [8, C3, 1]`, ahead of the lights and
the body, then reads register 3 unwritten. Before this policy the read
stopped the model after its first ten primitives, so a mine in flight
showed only a few thin edges until it armed (reported on
2026-10-18).

Retail reads stack residue there too. The lab's
`proximity-mine-throw.json` throws one on Level 1 and logs every opcode-16
branch (`004A0C90`, the lab's `model_branch` site: the command words and
the context's registers 3 and 10): all 9 reads in flight (root context
`1AF940`, passes 245 to 253) found `2861` and jumped, past the lights to
the body. `ObjectStackHistoryApproximations` starts the root's register 3
at `2861`; the flying mine then draws one face the armed path skips and
the armed body's twelve quads. Retail's and the port's frames of the
flight match, where the port before showed the edges alone
(`proximity_mine_model_84_register_3_needs_the_object_stack_history_approximation`).
Of the other grenades' models (80 to 83, 86, 87 and 64), none reads a
register before writing it, armed or not.

### Briefing preview after the map frame

Thumper H8 46 (level 1), previewed in the briefings of leaves 12, 20 and 25,
opens with three `0A26` lerps whose factor is register 1 (`0A26 [48, C, 6,
C1]` and its two neighbours); `F5 [1, 80, 400A]` writes it only after them.
In the briefing that read is not residue to approximate. Before `004A00E0`,
`0049FEF0` constructs five identity bases (`0040D6A0`) inside its context, at
`+14`, `+A0`, `+D4`, `+108` and `+13C`, none over its own registers
(`+70..+9B`). `0040B0F0` draws the map panel's frame (`0040C390` →
`00455080` → `0049FEA0`) just before the model preview (`0040B840` →
`0049FEA0`), and the frame's `0049FEF0` runs `0x50` bytes lower on the stack.
Its second base's last row (0, 0, 1.0f) becomes the preview root's registers
0..3 (0, 0, 0, `3F80`). Its third base's zero DWORD and first two rows become
registers 10..21 (0, 0, 0, `3F80`, 0, 0, 0, 0, 0, 0, 0, `3F80`).

In Infest02's Terra Nova briefing (`4F1C16..82E01D`), the thumper's program
was resident at `0CDC9A30` (the first lerp at `0CDC9BCC`). All 131 lerp
reads at the preview's root context `1AFA84` found exactly these 16 values.
Before the first and the last of them, the last write to register 1's slot
was the map frame's base construction (`00402690`, from `0040D6A0` at
`0049FF12`). Registers 4..9, the second base's unwritten tail, varied from
read to read and stay unknown. Ten more reads, at root context `1AF9B8`,
were the terrain pane's draws of the world's thumpers (`0040C1F3` →
`0047955A` → `0047B1D2` → `0049FEA0`). There register 1 held 0 in eight and
`1A` in two, residue of the objects drawn before
(ledger).

The briefing renderer passes `map_frame_drawn` when the map panel is
visible, and the preview then takes
`ModelFailurePolicy::BriefingPreviewMapFrameBases`, which seeds those
registers (`BRIEFING_PREVIEW_MAP_FRAME_REGISTERS`). It is not logged as an
approximation. The thumper preview now draws in all three briefings; with
the map panel closed its registers stay unknown and the preview is omitted.
Covered by `thumper_model_46_register_1_comes_from_the_map_frames_bases`,
`every_world_briefing_preview_draws` and
`authored_map_model_bracket_matches_one_native_flush_including_omitted_preview`.
The world's thumpers and the database's Generator draw through other stacks
(below).

### ThumperModel46StackHistoryApproximation

Outside the briefing the thumper's register 1 is residue. Terra Nova's
generator (resource 18, the world's only selector-39 course target, at
23423, 0, 30206) draws H8 46 at root context `1AF940` in the lab's
scene beside it. All
21 gameplay draws there read registers 0..1 as the DWORD `001AF558`, a
stack address, so register 1 held its high word `1A`. The five slot-59
draws just before, at a context `0x10` higher, held `001AF568` in theirs.
`ObjectStackHistoryApproximations` starts the root's register 1 at `1A`
(`THUMPER_MODEL_46_REGISTER_1`), which also covers the database's Generator.
It is logged with the other object seeds and covered by
`thumper_model_46_register_1_comes_from_the_map_frames_bases`.

The generator also needs its source `+10`: selector 39's table `00522398`
keeps the shared actor table's `0047A240`
([callbacks](OBJECT_MODEL_CALLBACKS.md#shared-actor-0047a240)), and the model
asks it for selector 10. With both, the port's frame beside the generator
(tick 17) differs from retail's in 439 of 307,200 pixels, thin edges and two
moving creatures, against 43,140 before.

### ExplosionWaveModel112StackHistoryApproximation

The blast wave, resource `0x38` (selector 22, the
[fading visual](SHARED_ACTOR_FACTORY.md#selector-22-fading-visual)), draws
H8 112 (`exwave4`), which only runs its child 115 (`exwave7`) at its one
vertex (`0E [0, 73, 8, 0]`). The child lerps twelve points by model
value 1, the fade (`0F26 [1A, 2, 0, 81]`, three more like it and eight
`0A26`), and eight more between those by register 1 (`0A26 [32, A, 1A,
C1]` and seven like it); neither model writes register 1, and the child's
registers 0..3 are the root's. All 22 draws of model 112 in the lab's Foldrak Noir run
(scene), the Alien
Brain attack's blasts beside the arrival gate, read registers 0..1 as the
DWORD `001AF570` at both the root and the child, so register 1 held the
high word `1A`, as the thumper's does. With it unknown the port omitted
every wave in view; `ObjectStackHistoryApproximations` now starts the
root's register 1 at `1A` (`EXPLOSION_WAVE_MODEL_112_REGISTER_1`), and the
port's frames match retail's to the frame's one-pixel right and bottom edge
(1,122 pixels at the wave's pass, against 31,494 without). Covered by
`explosion_wave_model_112_register_1_needs_the_object_stack_history_approximation`.

### Terrain furniture's shared root block

Helniazine also places the thumper as terrain furniture (H12 record 40/0,
H11 lookup 37, at about 27700, 5180). Every furniture item draws through
the same chain, `0044AA10` -> `0044CD00` -> the walk `0044CEF0` -> the
item `004359A0` -> `0049FEA0` (with the static packet `00546B48`) ->
`0049FEF0` -> `004A00E0`, so every furniture root's context lies at one
depth (root context `1AF0B8` in the lab), and its registers are what the
previous furniture root left there. `0049FEF0` writes no register before
`004A00E0`, and it skips `004A00E0` for a model outside the view, which
then leaves the block as it was.

The lab's scene beside it
logs each program's register 1 at entry and return. The thumper's root
read, in all 21 ticks, the value model 380's root (the furniture drawn just
before it) left on return. The same scene from 600 east, 3000 east and
1000 north agrees: 126 of 126 draws, with the twin runs. The values cycle
through 0..6 from tick to tick, so no constant would stand in for them.
Within a frame the block carries; across frames it does not: each tick's
first furniture root found `0297`, which other code wrote meanwhile.

The port's furniture draw runs its roots under
`ModelStackRegisterPolicy::FurnitureRootCarry`: each root starts over the
block the previous one left (a culled root leaves it, a failed one leaves
nothing known), and the block clears at each walk and at an item's trail
birth (`00489D90` runs there before the model draw, and its stack use is
unproven). A furniture root that reads a register no earlier root of its
walk wrote still stops. Covered by
`thumper_model_46_register_1_comes_from_the_map_frames_bases`.

A hook on `0049FEA0` or `004359A0` runs its own frames over the dead stack
where the root's registers will lie, so it changes them: such runs read
other values. Only `004A00E0`, which runs below them, can watch them.

### Database models

The computer's database (`00414C80`) draws the world objects' models, the
ACAM's forms among them, outside the world. Its submissions take
`ModelFailurePolicy::ObjectStackHistoryApproximations`: the object draw's
seeds above, and any other unknown register keeps the model's prefix
as `UninitializedValuePrefixApproximation` does. No recording opened the
database (`00414C80` and `00414830` have no calls in Infest01..04), so
these are the object draw's evidence, not the database's
([database](PLAYER_MESSAGES.md)).

Arithmetic preserves the original low-word stores, unsigned comparisons and
division, the distinct signed-right-shift instruction, and 32-bit x86 shift
counts masked by 31. F3/F8 retain the left operand for a zero divisor.
FD/FE use the original unsigned sine-table representation and native signed
product-bit extraction rather than a reconstructed floating-point curve.
The branch result retains its signed word displacement relative to the first
argument pointer. The program owner follows it only to a decoded instruction
boundary or the terminator.

## Context, admission, and child execution

[`ModelContext`](../../crates/inf-game/src/model_context.rs) owns world position,
the ten-word current basis, and the independently initialized saved basis at
context `+A0`. Opcode 03 (`004A0AE0`) first copies an oriented current basis into
the saved basis through `00490520`, preserving its counter, then evaluates and
rotates only when the authored packed-angle word is nonzero. An evaluated zero
angle still performs that rotation call. Signed axis 1 chooses `00491530`, axis
2 chooses `004917D0`, and other values choose `00491290`, using the original
sine table and the concrete [basis owner](../../crates/inf-game/src/menu_basis.rs).

Opcode 0F (`004A0B60`) copies the nine fixed axis DWORD bit patterns from
`0055F290/280/270` into the saved basis, preserving its counter at `+A0`.
The CRT initializers `0048F5D0/630/690` establish these read-only X/Y/Z unit
vectors. Opcode 10 (`004A0BE0`) restores all ten saved DWORDs to the current
basis, then republishes the transform through `0048FB40`. It neither
reinitializes invocation vertices nor changes the selected projection table.

`00490520` uses orientation bits 0..2 for axis permutation and bits `20/10/08`
to negate source X/Y/Z. Higher bits, including `400`, do not mirror this basis.
For external child 0E, low-three-bit value 6 instead copies all ten saved-basis
words; other orientations start with a zero counter. The child origin is the
first three DWORDs of the signed-indexed parent vertex slot: it is already in
world coordinates and must not receive the parent transform again. The child's
own saved basis starts at identity; only value registers 0..3 are inherited.

Publication `0048FB40` preserves world state and computes a separate view basis.
It reads published camera basis `0055F19C`, eye `0055F190/194/198` and
coordinate shift `0055F250`. Both object roots consume this retained scene
state; a borrowed caller context is not another view publication.
Each model-axis vector is dotted against camera basis rows in `(Z + Y) + X`
product order, with a final f32 store. Its counter is the source counter plus
one; unsigned values above 100 trigger the native normalization. Translation
uses wrapping `position-eye`, then x86 `SHL/SAR` with the masked coordinate
shift. Its dot product uses `(X + Y) + Z` order. The supported CRT sets 53-bit
x87 precision; direct `FISTP QWORD` rounds nearest-even and the caller keeps the
low DWORD, including zero from a masked invalid conversion's indefinite value.
This is distinct from the camera's truncating `__ftol` conversion.

The outer gates in `0049FEF0`/`004A0ED0` run after publication. Header extent is
consumed as signed i32; `z+extent` wraps and must be at least 64. Screen lower
edges are inclusive and upper edges exclusive. The X radius multiplies the
retained integer extent before its f32 store; the Y radius uses the separately
rounded f32 extent. `004A00E0` then applies its separate depth gate using
published global near/far `0055F248/4C`: flag `20` forces the near table,
otherwise `z-extent >= far` rejects and `z+extent >= near` selects the far
table. Force-near never bypasses the outer gate. Caller context distances
can differ after terrain restoration; only an actual fade publication changes
these model inputs. See [object projection custody](OBJECT_DRAW_RUNTIME.md#published-model-state-and-caller-context).

Top invocation publishes once. Child entry publishes twice; after an admitted
child returns, the shared projection selection is restored to the parent's,
then the parent transform is republished. Outer rejection skips projection
restoration, while the child's later far-depth rejection does not. Resource
lookup precedes child-origin access. The opcode wrapper `004A0E70` reads model
header byte `+7` even on its null-result path, before `004A0ED0` can null-check;
the Rust executor rejects an invalid child instead of inventing a successful
null-child transform sequence. Negative signed root indices remain no-ops.

Depth-group opcodes EB/EC (`004A0840/004A0860`) consume no argument words
and call `004A4740` with sorting disabled. EB uses current published model
origin Z at `0055F200`; EC supplies literal signed depth -1. Neither reads
a vertex slot. EF (`004A0920`) closes the group through `004A44A0`. Both
projection tables select these same handlers. Focused tests cover origin
depth with no vertex slots, parent republication after a child, and a failed
group submission stopping later close/open effects.

## Runtime vertices, projection, and shades

[`ModelVertices`](../../crates/inf-game/src/model_vertices.rs) owns the actual
24-byte invocation slots. Process views retain startup loader-zero bytes or
reached prior writes; isolated fixtures begin unknown until a writer
establishes the required bytes. Allocation does not initialize each slot;
see [retained model scratch](MODEL_SCRATCH.md).
Initialization preserves `004A00E0` order: seed shades 0/1 with the full ambient
word, project raw vertices through `004EBFE0`, populate world ranges through
`004EC0A0`, then shade normals through `004EBD30`. Projection and world ranges
are distinct header fields; projecting a vertex does not initialize its world
coordinates. Mirrored raw vertices negate sign-extended X in i32. World mapping
`00490900` evaluates `(Z + Y) + X`, converts with direct `FISTP`, and adds the
world translation with i32 wrapping.

Opcode 23 (`004EC410`) averages wrapping signed sums with division truncating
toward zero. Opcode 26 (`004EC950`) evaluates its packed weight even when no
output is requested. For each component it computes signed
`product = (right-left) * (u16_weight << 14)` and stores
`left + 2 * low32(product >> 31)`, preserving native wrapping and even steps.
The two command-mode bit pairs independently request world and projection
writes; the second pair XORs source indices with 1. Live aliases observe writes
from the earlier pair. Unknown required world coordinates fail explicitly.

Opcode 25 (`004EC810`) shares that mode loop. Each active pair turns the zero
vector through `00490900` (always zero) and adds the origin published at
`0055F1C4..CC`, then stores the wrapping `source1 + source2 - origin`. The
interpreters pass the context position they last published; the late world
stream already masked its projection bits. The
receipt runs `004EC810` unchanged over 56
cases (every mode nibble, sign-extended commands, aliased sources, DWORD
edges and random bases). The common path matches every world write,
projector input and depth; the late stream matches the 15 cases without
projection bits.

Opcode 2A (`004ED350`) without a parent context (`+9C` NULL, a root) does not
copy. World bits 8/10 store the published origin in the destination, and bit
10 then copies those 12 bytes to the next slot. Projection bits 9/11 project
the local zero point through context `+3C -> +1C` into the destination, and
bit 11 then copies its whole 12-byte tail to the next slot. Mirror-only bits
therefore write the destination too. With a parent, the 24-byte copies
described in [model asset coverage](MODEL_ASSET_COVERAGE.md) apply. The
receipt covers both branches over 66 cases;
every slot byte and projector call matches. The mirror copy reads the passed
slot's partner even when the parent passed an odd slot, which turns the
ACAM's right gun mounts to the left side in the hover morph; the port's
default `vehicle_gun_mounts` fix reads the odd slot itself there (see
[the black band over the roof](VEHICLE_DRIVE_RUNTIME.md#the-black-band-over-the-roof)).

Custom-vertex opcode 2D (`004EC670`) invokes object virtual `+04` separately
for each nonzero command-mode pair, using the same arguments on both passes.
Bit 1 writes world XYZ; bit 2 projects those world coordinates and stores
the returned depth. The second destination is consecutive; source arguments
are not XORed. A clear pair reads no source. Reached world writes survive a
later projection failure.

Frontend/name-entry vtables `00528518/00522360` bind that callback to
`0043A2D0`. Its selector low byte chooses a runtime row: 0 uses the high
byte of argument 0, 1 the high byte of argument 1, 2/3 the full unsigned
WORD argument 0, and 5 its low byte. `00439E10` reads the low WORD at row
offsets 0/4/8 and sign-extends each output. Selector 4 and the default arm
skip `00439E10` and return the callback's own uninitialized stack locals
(see the residue section below).
This generic callback performs no actor normalization tail. World actors
and briefing-panel vtable `00520378` retain their separate callback owners.
Captured object packets require the actual source callback; admitted family
selectors2/10 and selector36 forwarding are bounded by
[the object callback authority](OBJECT_MODEL_CALLBACKS.md#family-custom-vertex-selector-10).
Selector36's [exact11 capture](OBJECT_MODEL_CALLBACKS.md#selector36-exact11-capture)
retains its own valid-bit/captured-position prefix through the same source loan.
[Selector60 cached9](OBJECT_MODEL_CALLBACKS.md#selector60-cached9-capture)
uses a separate full-selector first-row cache on the actual retained source.
Sky, frontend and briefing-panel policies remain distinct.

[`ModelProjectionBackend`](../../crates/inf-game/src/model_projection.rs)
executes near local/world `004EB5A0/004EB730` and far local/world
`004EB8E0/004EBAA0`. Raw-local input uses the composed model/view basis and view
translation; world input subtracts the camera position and uses camera rows.
Below depth 64 it stores uncentered view X/Y and clip byte `C0`, preserving fog.
Otherwise perspective coordinates use direct nearest-even `FISTP`, then center
addition/subtraction. Signed absolute-value OR and arithmetic halving constrain
X/Y before their low-word stores; viewport comparisons themselves are unsigned.
Only far projection writes fog, using the live `0055F244` multiplier and signed
comparisons against global `0055F248/4C`. Keep bounds and factor from the
same reached renderer publication. Near fog and the final two slot bytes retain their previous
bytes: startup loader zeros or reached prior writes in the production process
arena, unknown bytes in isolated fixtures until a writer establishes them.
Per-allocation synthesized zeroing would change this contract; see
[retained model scratch](MODEL_SCRATCH.md).

[`ModelShades`](../../crates/inf-game/src/model_lighting.rs) retains RGB and
the fourth byte separately. Normal ranges read signed i16 triples from
`normal_pool + (slot/2)*6 - 6`, including the proven prefix record. The paired
normal negates X as i16, so `-32768` remains `-32768`. `004901B0` transforms and
rounds the normal before its lighting dot product, clamps negative intensity,
multiplies unsigned directional color bytes, adds signed ambient components,
and scales all channels together when their signed maximum exceeds 255.
Products and additions wrap as native i32 operations. Initial ambient seeding
writes the whole DWORD for slots 0/1. Later normal writes touch RGB only,
retaining each shade's current high byte.

### Exact twins under Vulkan (non-native)

Retail keeps every slot in whole units: `00490900` rounds a raw vertex,
`004EC950`'s blend moves in steps of two units (`IMUL; SHL EAX,1; RCL
EDX,1; LEA [left + EDX*2]`, up to two units low on each axis), and a
child's view position rounds at `0048FB40`. The ACAM's gun pods are
children at an opcode 26 slot (`0F26 [114, 17, 18, reg 10]`, 37.5%
deployed), so at 4K they hopped 3 to 4.5 pixels against the hull, single
hops 7 to 11. Under Vulkan (`ModelExecutor::set_exact_twins`: a traced
frame, or the raster's untraced world) the store keeps beside each slot
the same formula's point without rounding (`model_vertices/exact.rs`):
raw vertices, 21/23/24/25/26/27/28/29, 2A's copies (from the parent too),
2B and 2C (their contact height whole, following the source's twin
across), 22's jitter (whole, on the source's twin); a child's origin is
its parent slot's twin. Each projection keeps the exact view point (the
model's published rows over the exact origin's view point) and its exact
native screen point, by the whole-pixel XY it rounds to. The tracer draws
from the views; a raster corner whose payload XY a twin rounds to takes
the exact screen point ([`PrimitiveCommand::corner`]), so a pod's blended
charge panels stay on the traced pod. A writer with no twin (2D's
callbacks, the late table's world stream, the auxiliary owners) leaves
whole units, as does a corner the clipper makes. The slots, the behaviour
groups the bolts start from, every native byte, OpenGL and the software
renderer stay retail's (OpenGL's frames and the event log bit for bit).

### CustomVertexStackResidueApproximation

`0043A2D0` (frontend, name-entry and briefing-preview objects) handles opcode
2D selectors 0..3 and 5 through `00439E10`. Selector 4 and the default arm
skip that call and copy the callback's own uninitialized stack locals
(`[esp]`, `[esp+2]`, `[esp+4]` after `sub esp, 8`) into the output. The
value is residue of whatever earlier call used that stack depth. Briefing
previews reach this arm in several models. Each feeds a material-633 beam
triangle, a depth-group key or a child anchor:

| Model | Selector | Destination use |
|---|---|---|
| factory1 H8/157 | 6, 4, 7 | three beam triangles |
| scientst H8/223 | 10 | one beam triangle |
| gatebod H8/63 (gate 62) | 8 | EE group key (slot 118) |
| hottorso2 H8/497 (tankbot 492) | 11 | child H8/294's anchor (slots 46/47) |

Infest01 records the Level 1 previews. The factory's residue was
`(-195, -1, -1)` for selector 6, the stack words of the context pointer
`001AFA84` and opcode `00DA` for selector 4, and `(-204, -1, -1)` for
selector 7. All three changed from frame to frame. One selector-6 vertex
projected behind the camera (clip `C0`, rejected); a selector-4 vertex
projected to `(-2897, 2636)` at depth 218 and its triangle passed the clip
table. The retail video still shows the factory clean, with at most small
flares at the light bar. The gate's selector-8 residue was the same pointer
halves every frame, world `(-17204, 1139, 22340)`, so its EE group always
keyed at 22140. That is behind all preview geometry.

The port cannot reproduce stack residue, so it invents no value:

- The destination's written fields become unknown: world XYZ for mode bit
  1, the projected tail for bit 2. Primitives skip a vertex whose clip byte
  is unknown, as they do any unprojected dynamic slot.
- EE, EA or 15 keyed on a residue projection opens its group at the
  farthest signed key, which keeps the recorded order.
- A child instance anchored on a residue world vertex is skipped.

Any other unknown read still fails as before. `ModelVertices` tracks residue
slots until a later write, including 2A parent copies. The policy is logged
once per process and covered by
`custom_2d_stack_residue_selectors_forget_only_the_written_fields`, the
Level 1 preview matrix and
`every_world_briefing_preview_draws`.

### Terrain-projected vertices: renderer 2C and auxiliary 96

`004EC5C0` is shared by renderer opcode 2C and auxiliary opcode 96. The
renderer sign-extends the command WORD at `004A0212`; the auxiliary interpreter
zero-extends its opcode BYTE and shifts it left eight bits. Thus renderer
`962C` passes `FFFFFF962C`, while auxiliary 96 passes positive `00009600`.
[`ModelSurfaceVertexRequest`](../../crates/inf-game/src/model_vertices.rs) preserves
that distinction with an explicit command origin and signed source/destination
slot WORDs. Near and Far dispatch tables `00540DC0/005411C0` both select
`004EC5C0` directly. Separate wrapper `004EDD80` masks AH with `F5` before its
call; that wrapper is not the renderer's normal Near/Far dispatch.

The handler arithmetic-shifts the full command right eight bits, then consumes
two-bit chunks with **logical** shifts. Every chunk advances the destination
by 24 bytes, including zero chunks. Source XOR is zero for the first chunk
and stays one for every later chunk; it does not alternate. For example,
renderer `102C` skips destination slots D/D+1 and writes world XYZ to D+2.
Auxiliary `9600` reaches four consecutive slots: projection, XYZ, XYZ,
projection. Negative renderer command WORDs can reach sixteen chunks.

Each active chunk first reaches context `+58` -> object `+50` -> callback
`+14`, receiving the source-slot pointer and one reused stack XYZ temporary.
Only after a successful callback does the handler reach the destination:
bit 0 copies world XYZ; bit 1 invokes the selected world projector at table
`+20`. At `004EC642/649`, successful projection stores its returned EAX
**minus 20**, wrapping as a DWORD, at slot `+10`. Screen coordinates, clip
and fog were already calculated using the original projector depth. Other
vertex handlers retain their unbiased projector result. A failed projection
retains reached world and partial projection writes without applying this bias.
Exact zero mode reaches no terrain service, source or destination access.

The required mutable
[`ModelSurfaceVertexCallbacks`](../../crates/inf-game/src/model_vertices.rs)
owns the selected Ground/Project callback phase for both 2B and 2C. The
terrain adapter selects residency before reading source XYZ. The typed service does not
claim native arbitrary context/table mutation, buffer rebinding or invalid
pointer faults. Temporary writes from a failed terrain callback stay with the
service and are not published to the destination. This domain admits complete
owned XYZ vectors; native default Ground does not read source Y, while raw
partial-vector and alias cases retain separate diagnostic receipts.

The unchanged-instruction
native checker and
[receipt fixture](../../crates/inf-game/tests/fixtures/model_terrain_project_command_native.json)
cover 32 cases: signed renderer transport, positive auxiliary transport,
skipped destinations, depth wrap, callback ordering and reached partial writes.
Thirty are compared by the
[portable tests](../../crates/inf-game/src/model_vertices/project_2c_tests.rs).
Two invalid-source/buffer-rebinding cases remain native diagnostic receipts.
Supplied flat H12 and the actual `0044FFA0`/`00450A30` copy path establish this
handler contract; the substituted world projector establishes neither full
projection, renderer packets nor matched retail pixels. Auxiliary setup and
other shared vertex handlers retain their separate
[decision matrix](MODEL_AUXILIARY.md#state-and-execution-boundaries).

### Renderer surface callbacks and table custody

`ModelSurfaceBinding` explicitly selects the renderer's default or terrain
pair, independently of packed-value callbacks, projection Near/Far selection,
and terrain availability. Default table `00544740` binds Ground `+10` to
`004EBCD0` and Project `+14` to `004EBCF0`. Ground copies X/Z and writes
Y=-300. Project adds signed truncating `wrap(Y+300)/2` to X/Z with DWORD
wrapping, then writes Y=-300. It rereads source Y after writing output X;
raw alias cases can therefore differ from a supplied nonalias XYZ value.
Neither default callback reads terrain, renderer XYZ, a clock or a sine table.

The constructor copies all nine default DWORDs into selected table `00546E60`
at `004386FC..00438709`, publishes renderer `00546E88` at G `+648`
(`00438780`), and binds that renderer `+50` to `00546E60` at `004389E4`.
Template construction `0043994B` instead publishes `00544740`; `0043B4A0`
copies that alias. `004501A0` replaces only the selected table's `+10/+14`
with `0044FF50/0044FFA0` and copies the default `+00` entry into it.
The three installer call sites are world `004881B3/0048838D` and briefing
pane `0040BE6A`. They do not modify the template's Ground/Project pair.

| Caller class | Surface table and reached policy |
|---|---|
| Frontend/name-entry model, rotating briefing preview, panel frame | Template alias; default callbacks even when a world terrain is resident |
| World sky, actor and terrain furniture | Selected renderer alias, installed by world setup before model drawing |
| Briefing terrain pane and its backdrop/entities/furniture | Inherited selected alias; visible, camera-enabled pane setup reaches the installer |
| Briefing decoration | Inherits the selected alias; sees earlier pane/world installation, or retains Default when no installer has run |

`ModelSurfaceTableOrigin` records those two alias origins on scene submissions;
it is separate from `ModelCallbackPolicy`. `SoftwareScene` retains selected
mode through command/depth resets and residency rebinds. Closed or disabled
panes leave that mode unchanged. Current H14 world flags and process clock
are refreshed separately; retaining the alias must not freeze those inputs.
Each `ModelDrawRequest.surface` passes the selected binding to its children
unchanged. Missing terrain on the selected terrain binding reports
`MissingSurfaceTerrain { callback }` before source/destination access, never
an implicit switch to Default or an unsupported-opcode diagnosis. Inactive
command modes still bypass the callback and need no terrain. This is portable
required-backing containment; it does not claim native invalid-pointer faults.

The interactive renderer refreshes this backing after session update through
`set_terrain(session.world().map(...))` in
[`frontend_app.rs`](../../crates/inf-game/src/frontend_app.rs). The linked
headless scheduler uses the same handoff before presentation, including clearing
backing when no world is retained. Availability and selected-table installation
remain separate. Missing-backing warnings from an incompletely initialized
headless epoch do not establish an unowned 2B/2C handler or a production gap.

Root `0049FEF0` stores the original renderer at context `+58` (`004A0024`),
separately from the model callback object at `+44` (`004A0020`). Child 0E copies
the parent `+58` verbatim at `004A10AC..004A10B7`. Camera publication
`00410D90`, preview `0040BA94..0040BAA7`, panel `00455296..004552B4`, and
briefing background `0040AC85..0040AC97` publish the same renderer
`+04/+08/+0C` fields to the view eye before draw. The covered copy-path input
is therefore the renderer eye, independently of model position.
The terrain Project kernel's actual H14/clock/water inputs belong to
[terrain projection](TERRAIN_RUNTIME.md#terrain-project-callback-0044ffa0).

The default callback checker
and [fixture](../../crates/inf-game/tests/fixtures/model_default_terrain_project_native.json)
cover 58 complete nonalias Ground/Project inputs, 23 separate pointer/alias
diagnostics and eight bounded native 2B calls. Those calls retain 2B's `0F`
mode mask, zero-mode service bypass and unbiased projector return. The
selected-table audit executes unchanged `004501A0` with distinct supplied
tables; actual lifecycle alias custody above comes from constructor, copy
and writer source, not an invented table-pointer observation. These checks
do not establish arbitrary mutable callbacks or matched retail pixels.

### Native scratch allocation

`004EBE60` allocates from the fixed `0xFA000`-byte arena at `00564728`, using
cursor `0065E728`. At `004A0126..004A0135`, model header `+08` and `+0C` are
zero-extended vertex and shade counts. The invocation receives `24*vertices`
bytes followed by `4*shades` bytes. An end exactly equal to the arena size is
accepted. The cursor advances through both ranges before nested execution;
children allocate after their parent's complete vertex/shade allocation.

The allocator returns the old cursor and registers `004EBEE0` plus that saved
value with the native cleanup owner `004948C0`. Normal return `004A022F`
unregisters the cleanup and restores the old cursor. Neither allocation nor
restore clears scratch bytes. A whole-executable reference audit found cursor
writes only in these allocation/restore routines. The cursor is in PE
loader-zero storage beyond initialized `.data`'s `00547000` end; balanced root
draws therefore begin at zero in the fresh background runner. Other native
model/query interpreters use the same allocator and paired restore, rather
than a separate arena.

The shared [retained model scratch owner](MODEL_SCRATCH.md) now holds actual
process bytes and registered old marks across common and late invocations.
Normal child return restores the cursor without clearing memory; a typed stop
retains its cursor, byte prefix and marks until an explicitly named containment
policy recovers them. Renderer recreation, loading and resource/display epochs
retain the same owner. Startup loader-zero bytes and isolated unknown fixtures
are distinct evidence domains; model value registers are not initialized by
this storage proof.

E2/DE/E6 mixed-plane fallbacks read signed shade references from the original
instruction and following bytes. Such a DWORD can lie outside the invocation's
declared shade range while remaining inside the fixed arena. Its narrow scratch
read checks **all four bytes** against the retained process arena, including
actual prior vertex/shade bytes outside local slots. E2 UV packets omit RGB; DE/E6 shaded UV packets
serialize a separate nominal shared shade. These packet families do not consume
the copied/interpolated scratch colors. D6 instead serializes those corner RGB
values and preserves known source high bytes while intersections leave high
unwritten. Native DE/E6 direct and clipped producers
also read their nominal shared shade through this signed DWORD address contract;
unknown RGB/high bytes stay unknown in the queued packet. Its reached pixel
consumer requires the low three RGB bytes. Ordinary fan shading keeps strict
declared-slot reads and rejects unknown required RGB.
Direct D6 also retains its four signed arena shade DWORDs after its second H3
lookup. An isolated shade fixture without proven arena custody retains local bounds.
The concrete first-entry case and native color-independence check belong to
[Primitive runtime](PRIMITIVE_RUNTIME.md#opcode-e2-textured-quads-and-the-near-plane-fallback).
Shared and per-corner source policies belong to
[the polygon contracts](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons).

## Frontend program closure

The nine authored roots are 744, 749, 754, 756, 745, 748, 747, 755, and 757.
Their external-child closure adds 746, 751, 752, and 753. This is a concrete
13-model frontend scope, not a claim about every model in the game.

- Roots 747/749/755/756 write register 1 from the clock callback shifted left
  by **4**. The encoded literal 6 decodes to 4; its raw encoding is not a shift
  count.
- Root 745 uses saved-basis opcode 03 before child 746. That child computes
  clock-derived registers, selects one of sixteen face groups through opcode
  16, and uses nine opcode-26 interpolation records.
- Model 751 contains midpoint opcode 23; 752/753 and root 757 use opcode 1B.
- The closure does not contain inline 05/09 programs or require a random or
  custom callback in place of the recovered frontend clock policy.

The [value tests](../../crates/inf-game/src/model_values_tests.rs) exercise
the register-bearing roots 747/749/755/756/757 and child 746's value/branch path
at clocks 0, 1, 3, 4, 60, 63, 64, 65535, and 65536. Synthetic tests cover
arithmetic, callback mutation, unknown reads, context aliases, and child
ownership. The [execution corpus tests](../../crates/inf-game/src/model_execution_corpus_tests.rs)
run all 13 models, including their recursive children, at seven clocks in both
near and far modes. Each pass reaches 3,226 required primitive requests with
concrete projected vertices and shades; child 746's phase and interpolated
geometry change and wrap together. The fixture records primitive requests and
checks their defined bytes and source provenance; it does not rasterize them.
An authored-scene composition test uses the original model positions, bases,
and returning camera preset with explicit renderer settings. It preserves the
first model's special eye, near/far selection, and actual admission/culling.

## Program execution and draw-command handoff

[`ModelExecutor`](../../crates/inf-game/src/model_execution.rs) borrows the
resident resource catalog and walks the canonical instruction stream. Required
`ModelRootMode::{CommonVisual,ManagerLate}` names the actual entry rather than
encoding late execution through Far projection. Common visual behavior below
retains its previous decision matrix. It joins
the concrete owners above for F0..FF, branch 16, saved-basis 03/0F/10,
external-child 0E, midpoint 23, interpolation 26, and the owned frontend 2D
callback. The selected near/far dispatch, original
command, live values, vertex/shade state and child state words reach a required
primitive host for 01/E2/1B. The concrete
[`ModelCommandBackend`](../../crates/inf-game/src/model_commands.rs) now owns
that host: resident material lookup, fan/quad rejection, E2 near-plane clipping,
1B effect geometry and native callback payloads feed the shared deferred queue.
See [primitive and queue ownership](PRIMITIVE_RUNTIME.md) for source boundaries,
packet layouts and remaining pixel-backend work.

EE (`004A08B0`) reads the selected vertex's projected depth DWORD and adds its
authored low/high-word displacement with wrapping i32 arithmetic, then calls
the depth-group owner `004A4740(depth, false)`. EF (`004A0920`) calls
close/sort/pop `004A44A0`. The concrete
[`DrawQueue`](../../crates/inf-game/src/draw_queue.rs) preserves native arena
accounting, nested groups, signed sort words, equal-key allocation order and
repeat-flush behavior. A required renderer service runs before group allocation,
just as it does before admitted primitive allocation. EE/EF do not bias projection.

E8 (`004A0770`) and E9 (`004A07C0`) open the same unsorted group at the
farthest or nearest projected depth (slot `+10`) of a `0xFFFF`-ended slot
list, the slot WORD zero-extended, starting from `-0x7FFFFFFF` and
`0x7FFFFFFF`; an empty list keeps the start. The late table skips both
(`004ED5F0`). 13 (`004A0C50`, shared by all three tables) jumps when
"origin Z (`0055F200`) >= the signed literal high:low" differs from the
command's `SAR 8`, else continues three WORDs on: a distance switch. The port
runs E8, E9 and 13 in the common path as well as the late one.

The Near and Far tables hold other handlers the port's common path does not
run (each stops as an unsupported instruction if a drawn model reaches it).
No authored model reaches them: a census of every OVL's H8 bank, nested
programs included, finds no command 02, 09, 18, 19 or 29 (2026-10-19).

| Command | Near | Far | Does |
| --- | --- | --- | --- |
| 02 | `00495A60` | `00495B70` | `[material][slot][slot]`: a line. Both slots' outcodes (`+14`) must pass the clip table `00528BD0`; the material comes from the context's `+4C` pool (its loader `+0` runs on a vacant slot); the depth is the first slot's, or `0x40000000` (`(depth >> 1) + 0xC0000000` with mode flags `0x16`) when the renderer's `+4` sorting gate holds; a kind-2 record through `004A42B0` with `004A39F0` (Near) or `004A3A30` (Far) |
| 09 | `004A12E0` | `004A12E0` | the [two-slot basis sub-object](FORMAT_DOCUMENTATION.md) (the late table's `004EDB10`): it saves the origin and basis, takes the origin from the first slot and a basis from the vectors to the other two (`00417DA0`, `00490370`, command bit 8 mirrors it), publishes it (`004EBFE0`, `004EC0A0`, `004EBD30`), runs the nested stream through the Near or Far table by context `+60`, then restores |
| 18 | `004A1530` | `004A26A0` | a rotated sprite: the sprite (`00405540`), a packed size and angle, four corners turned by the sine table and tested against the screen (`0055F234`/`0055F238`) and the clip table; a kind-3 record with callback `00466170` (Far: `00466180`) |
| 19 | `004A1940` | `004A2AD0` | the same with a kind-3 (Near, `004A39A0`) or kind-4 (Far, `004A39D0`) record, then `00490350` |
| 29 | `004ECFE0` | `004ECFE0` | the two-weight cubic spline (the late table's `004EDD50`, which the port runs) |

The late table's 01, `004ED5B0`, only skips a fan: entries of four WORDs (six
with flag 8) until one whose high byte is nonzero.

Unknown required register/world-coordinate reads, unsupported instructions,
malformed branch targets and unavailable resources remain explicit failures.
Shade and projection byte validity stays explicit in the primitive request;
the renderer must not synthesize missing bytes. Instruction/depth
limits and stopping after failure are Rust containment policy; completed host
effects are retained, with no implied native recovery or rollback. Common
visual inline09 and aliased register words remain unsupported. Common inline05
uses the shared owner below. The distinct manager-late owner retains its own
complete inline block policy. Context
tests cover publication, orientation, admission, numeric boundaries and original
03/child records at several clock values. Concrete command generation and queue
replay still do not establish rasterization or matched frontend pixels.

Do not feed the diagnostic viewer's neutral callback values, approximate
sine helper, or partial static mesh to a production model submission.
Recover additional static consumers first; use the existing TTD recording for
executed or transient state that static source cannot establish. No new retail
capture was needed for the static execution owners described here.

## Common inline transform

Common Near table `00540DC0/+05` and Far table `005411C0/+05` both select
`004A1150`. Its complete 391-byte body owns inline05 on the current invocation;
Late table `00544764/+05=004ED9C0` remains a distinct world/control policy.

The handler saves exactly thirteen DWORDs: position3 and basis10. It reads the
signed origin slot from argument0, then evaluates and rotates arguments1/2/3
in Xz/Yz/Xy order. All three calls and rotations execute even for literal zero.
After `0048FB40` publication, signed argument ranges execute in native order:
raw projection4/6/8 (`004EBFE0`), world5/6/7 (`004EC0A0`), then normals9/10/11
(`004EBD30`). These are independent phases; a later invalid world range cannot
erase completed projection writes. The existing root-header prevalidation
matrix and phase loops remain unchanged.

The nested program retains the same resource, Near/Far table, active scratch
views, context, register file, state words, parent vertices and surface. It
performs no allocation, culling, resource lookup, ambient reseed or child-value
copy. On normal return only, position and basis restore and republish. Saved
basis+A0 and inner register/scratch writes persist. Callback, projector,
primitive or publication failure retains the reached context and earlier
writes; there is no error-path restore or implied native rollback.

[`common.rs`](../../crates/inf-game/src/model_execution/common.rs) names this
shared phase. [Focused tests](../../crates/inf-game/src/model_execution/common_tests.rs)
pin the complete native body and both table entries, current invocation in both
modes, zero rotations, independent projection/world/normal ranges, persistent
saved basis/registers, failure prefixes and allocation-free recursion. The
existing portable depth/instruction bounds also contain nested inline programs.
Malformed negative/odd/out-of-view ranges retain checked portable failures;
arbitrary native pointer-wrap and alias-fault timing remain outside that domain.
Synthetic instruction/geometry cases establish these contracts. The separate
[loaded-eye test](../../crates/inf-game/src/world_scene_tests/eye.rs) completes
original698→699→700→701 through real software commands and final flush for two
explicit poses in eight supplied tier/mode views. Its controlled middle-span
fixture and source-owned length control belong to
[eye capture](OBJECT_MODEL_CALLBACKS.md#selector77-eye-capture). This resource
closure remains distinct from eye simulation, the production camera timeline
and matched retail acceptance.

## Manager late world/control root

`ModelRootMode::ManagerLate` selects `004ED610` and table `00544764` through
the private [late walker](../../crates/inf-game/src/model_execution/late.rs).
It publishes the actual transform before signed-WORD H8 lookup and requires
root backing at the first header dereference. Common negative-root success
does not apply. Scratch allocation is shared, but world-only `004EC0A0` uses
header +12/+14/+16; it does not initialize projected slots, shade values,
normal lighting, common culling/fade or a Near/Far projector.

The direct table retains genuine shared basis/value handlers and distinct
world/control branches, inline05/09, child0E, masked20..2C and forced2D. Native
primitive/effect slots advance their complete canonical records without queue,
projection, shade or RNG callbacks. Reserved infinite-spin slots remain typed
unsupported boundaries. Full signed SAR8 modes for20/2C retain any reached
upper projection requirement as unknown state; no fake projector is selected.

Child `004ED7E0` bypasses invalid/genuine NULL lookup before state/publication;
admitted children preserve source and registers, publish twice, initialize world
slots and use the same direct table. Only normal return restores scratch and
republishes parent. Inline05/09 likewise restore only after a normal nested
terminator. Root2A's genuine NULL parent writes transformed local zero (model
origin); parent-present2A reads its actual state reference before mode gating.
These branches do not reuse common child or common NULL-parent rejection.

Late2D rewrites `(command & F1FF) | 0100` and always reaches the captured
object's required custom vertex callback before any later stride boundary.
The current Object backend reports source virtual+10 explicitly. Its required
common policy preserves the native mode0 bypass and reaches the callback in
mode2; the distinct late forced callback also cannot be suppressed. Reached values,
world slots, publication and RNG survive the failure. It is not a successful
late return that admits manager pending-clear or behavior suffixes.

[World-stream tests](../../crates/inf-game/src/model_vertices/world_stream/tests.rs)
and [late tests](../../crates/inf-game/src/model_execution/late/tests.rs)
cover bounded initialization, skips, source inheritance, NULL/root/child,
branch/block/restore and required-callback prefixes. Original byte/table checks
authenticate those static entries, not native execution or retail pixels.
The consumed manager and remaining feature owners live in
[object drawing](OBJECT_DRAW_RUNTIME.md).
