# Infestation shared weapon firing

This authority owns the shared firing boundary reached by the
[actor movement tails](ACTOR_MOVEMENT_TAILS.md). Weapon behavior storage and
timer decay remain with [actor behavior](ACTOR_BEHAVIOR_RUNTIME.md#retained-weapon-timers).
The recovered producers are shared actor services; mission IDs, placement
references and visual model names do not select a firing implementation.

## Direction, queue and later emission are separate phases

The original bodies are retained in
game_logic.c. Their shortened decompiler signatures
omit some register/stack operands. The register/stack contract below records
the original x86 consumer boundaries, independently of those signatures.

| Native owner | Recovered responsibility |
| --- | --- |
| `0048D340` | Start with the supplied signed-WORD world displacement; optionally lead the target, compensate the gravity-enabled weapon and update behavior `+20`; apply spread with three shared RNG draws when behavior `+04` is nonzero; normalize the resulting vector and call `0048D0D0`. |
| `0048D5A0` | Apply the separate spread policy, including the distance quarter term and three shared RNG draws; normalize the result and call `0048D0D0`. |
| `0048D0D0` | Admit ordered fire requests into behavior `+00`, and update the actual selected ammunition, group cursor, cooldown and accumulated firing age. It does not emit a projectile or particle. |
| `00479570` | Flag0 object-draw manager. Its late firing lane can request an actual component draw before clearing the original actor's flag `01000000`, querying the component's virtual `+7C`, and calling `0048CDB0`. |
| `0048CDB0` | Walk the retained `+00` fire chain, resolve the current weapon position, apply the queued time compensation and dispatch `0048C890`; unlink/free each node only after that callback returns. |
| `0048CD60` | Publish each queued successor before freeing its node; release entry/group/timer arrays before stopping retained audio and releasing behavior storage. |
| `0048C890` | Select the executable weapon factory/resource, construct the actual projectile or particle, apply inherited actor velocity and branch-specific effects/statistics, then update the firing sound lifetime and enqueue or retain audio. |

`0048D0D0` prepends each allocation to the chain. The later walk therefore
consumes the actual linked order rather than FIFO order. A node is 32 bytes:

| Node byte offset | Source |
| --- | --- |
| `+00/+04/+08` | Three normalized direction floats |
| `+0C` | Signed firing-time compensation carried by the producer's loop |
| `+10` | Captured weapon group index |
| `+14` | Selected behavior-entry index, with bit `10000` when selection equals behavior byte `+11` |
| `+18` | Caller-supplied speed scale |
| `+1C` | Previous chain head |

Queue admission first captures the selected `+18` entry and its `+14`
cooldown. Each admitted round decrements finite ammunition (`FFFFFFFF` remains
unlimited). Each reached group marks its `+1C` flags with bit 4 before
allocation, then publishes the chain head, sets actor flag `01000000`, writes
that group's timer and advances the selected entry's group cursor. Accumulated
firing age and cooldown use the selected `+14` descriptor's current values;
group wrap adds the separate delay and may consume one shared RNG draw.
The native allocation result has two distinct outcomes. An unavailable or
failed allocator service stops after the ammunition decrement and group-bit
write. An actual returned NULL continues: it overwrites `+00` with NULL,
sets the actor flag, and performs timer/cursor/age and round suffix writes.
Older nodes detached by that NULL retain their separate heap allocation;
neither a subsequent drain nor `0048CD60` invents a free for the lost chain.

The manager's actual draw boundary matters because weapon positions can be
captured by model callbacks into behavior groups. Those live coordinates and
group flags already share the retained
[weapon-position owner](ACTOR_BEHAVIOR_RUNTIME.md#shared-selected-weapon-position).
Draining fire requests directly at the end of an actor's `+24` would bypass the
native manager, visibility/draw ordering and these position writers.

## Native call contracts

Stack operands below are in increasing argument order after the return address.
All five behavior methods receive the behavior in ECX. `0048D5A0` retains the
six-slot producer ABI even though it does not consume the target or final flag.

| Owner | Stack operands | Confirming consumer/return |
| --- | --- | --- |
| `0048D340` | Actor, target, distance, signed-WORD displacement pointer, interval, update-`+20` flag | Queue call `0048D58F`; `0048D59A ret 18h` |
| `0048D5A0` | Actor, unused target, distance, signed-WORD displacement pointer, interval, unused flag | Queue call `0048D6B4`; `0048D6BC ret 18h` |
| `0048D0D0` | Actor, unsigned interval, direction-float pointer, speed scale | `0048D2CA ret 10h` |
| `0048CDB0` | Actor | Emitter call `0048CF39`; `0048CF61 ret 4` |
| `0048C890` | Fire node, position pointer, actor | Factory call `0048C9AC`; `0048CBFD ret Ch` |

The indirect factory call `0048C9AC` supplies ECX from registry `+04`, EDX
as the position pointer, and stack operands velocity pointer, source
ActorHandle, flags zero. It reaches the existing
[particle factory request contract](PARTICLE_FACTORY_RUNTIME.md#captured-requests-and-live-storage).
At `0048CF3E..0048CF48`, the drain publishes the next head and frees the old
node only after the emitter returns. A failed emission must retain its earlier
direction/position/effect writes without inventing that normal-return suffix.

## Canonical initial intro weapon data

These are constructor inputs from the canonical Rust resident inspector for
OVLs `47,48,32`, H14 world slot 0. They establish initial table selection, not
later runtime selection or permission to dispatch by placement/mission ID.
[Behavior construction](../../crates/inf-game/src/object_runtime/actor/behavior.rs)
starts at entry 0; both rows remain mutable shared executable data owned by
[ProcessActorBehaviorTables](../../crates/inf-game/src/object_runtime/actor/behavior/damage_cache/storage.rs).

| Constructor and placement indices | H13 / mode / table | Entry 0, entry 1 | Native factory selectors |
| --- | --- | --- | --- |
| Selector10: 67,68 | 87 / 1 / `00530E60` | Kinds 2,5; raw speed field 3000; shot-count field 2; raw cooldown field 150000 | `00466710`: 5,2 |
| Selector36: 51,78,79 | 108 / 5 / `00530FF0` | Kinds 3,6; raw speed field 4000; shot-count field 1; raw cooldown field 500000 | `00466710`: 6,3 |
| Selector12: 47,48,49,50,82,83 | 3 / 0 / no behavior | No constructor weapon entries | None initially |

The four firing registry entries have flags zero and resource zero. Both
tables use one-shot H9 slot 57 (`+20` byte zero); they do not initially require
projectile actor constructors, gravity, the registry extra-effect branch or
retained loop audio. Their reached particle-statistic branch still matters:
`0048C890` captures G `+1BC` before the factory, then `0048C9BC` classifies the
firing source actor's fresh flags and classification through `0047F710`;
`00481030` increments DWORD `+14 + category*4`
on that captured owner through the existing
[mission statistics view](../../crates/inf-game/src/world_statistics.rs). Sound enqueue `0048CBEA` uses volume/pitch `10000h`; the
following `004923C0` disposes a nullable returned error, not the sound record.

Reproduce the bounded constructor inspection from ``:

```powershell
$world = cargo run --release -q -p inf-game -- --data-root retail --mode resident --activate 47,48,32 --inspect-world 0 --difficulty 2 --language 0 --json | ConvertFrom-Json
$world.world_assets.placement_links |
    Where-Object { $_.constructor.data.resource.constructor_selector -in 10,12,36 } |
    ForEach-Object {
        $d = $_.constructor.data
        [pscustomobject]@{ placement=$_.placement_index; selector=$d.resource.constructor_selector; H13=$d.source.registration.global_index; mode=$d.asset.payload.data.derived.byte_44.value }
    }
```

## Retained implementation and failure custody

The [shared producer and queue](../../crates/inf-game/src/object_runtime/actor/behavior/firing.rs)
now execute from the native Selector10, Selector12 and Selector36 firing gates.
Predictive `0048D340` and scattered `0048D5A0` remain explicit modes. Predictive
velocity comes from the actual retained target's virtual `+08`; target resource
identity and ActorHandle are distinct. Process `0055F0F8` is required only when
the target class reads it. Shared executable descriptors and selector2's inline
`+26C` copy use the existing table owner, including signed current-entry selection.

Queue `0048D0D0` returns early for unsigned interval less than or equal to
cooldown, or zero ammunition. Zero/negative shot counts and exhausted group
ranges still reach the enclosing round's ammunition, timer, compensation and
possible group-wrap RNG writes. Finite ammunition, unlimited `FFFFFFFF`, the
separate wrap delay and the unsigned `6000000` age threshold retain their native
arithmetic and store order. No constructor/mission-specific firing bypass exists.

`ActorBehaviorState.fire_head_00` retains real 32-byte allocations in the
[actor-service heap](../../crates/inf-game/src/object_runtime/targets/heap.rs).
Its existing `PortableFallibleActorHeapApproximation` provides fallible backing
and stable allocation identity; Windows heap address/reuse and new-handler
cadence remain outside that owner. The linked order is independent of allocation
order. `0048CD60` teardown runs both on behavior replacement and object destruction:
each successor is published before free, then the `+18`, `+1C` and `+24` arrays
are released, and a retained `+28` sound requires the actual sound teardown loan.
A failed release preserves the reached head and earlier releases. Behavior storage
is dropped only after normal teardown return.

The [shared drain and emitter](../../crates/inf-game/src/object_runtime/actor/behavior/emission.rs)
are reached from both existing consumers:
[run_flag0_object_draw_pass](../../crates/inf-game/src/object_runtime/object_draw.rs)
retains late component draw, original actor flag clear, component query and drain
order; [ordinary selector2 draw](../../crates/inf-game/src/object_runtime/object_draw/vehicle.rs)
retains its direct `0042086D` call only for a nonnull head. Both operate on the
same behavior allocation. Queued group lookup remains independent of the current
selected group, and model callbacks can supply its current captured position.
The separate `004791F0` collision/terrain pass is not the firing manager.

Both [world drawing](../../crates/inf-game/src/world_scene.rs) and
[briefing drawing](../../crates/inf-game/src/briefing_renderer.rs) loan the
current process RNG to `SoftwareScene` across their object pass. The emitter
continues that loan after [model callbacks](../../crates/inf-game/src/software_scene/object_models.rs)
and passes it directly to the reached particle factory. It never reads the
caller's not-yet-restored `world.rng`. The initial intro registry entries reach
the owned particle construction, captured-owner firing statistic and H9 slot57
one-shot enqueue. Persistent sound record construction and lifetime switching
also use `ProcessSoundRuntime`; a reached nonnull voice still requires the
actual voice teardown service. The actor-resource arm builds the shots of
the ported classes ([selector 32](SELECTOR32_RUNTIME.md), the cannon, the
[grenades](SELECTOR26_RUNTIME.md) of selector 26, the
[mines](SELECTOR38_RUNTIME.md) of selector 38 and the
[missiles](SELECTOR25_RUNTIME.md) of selectors 25 and 28) through the
owner's `+4C`, or repeats a group's live one through its `+98`; a grenade,
mine or missile in the group answers that `+98` with 0. Registry flag 8
(kind 15) launches along the owner's `+48` basis instead: its row at `+10`
times -128.0, plus the owner's velocity. Resource 0 is the
[detonator](SELECTOR26_RUNTIME.md#messages) `0048CB00`: event 10 within 4000
of the owner, then hint 0x30. Resource `0x82` is the
[flamethrower](SELECTOR76_RUNTIME.md#emission) `00431EF0`, built through the
world factory's registry construction. The
registry's extra particle `00489D90(2)` is born at the emission point with the
owner's velocity. A shot with flag `0x200` from a player's unit (the
implant, a pilot guided rocket) goes to that player (`0048CAF9`, through the
child host's `take_shot`). Named typed limits bound unknown factories, the
other resource classes and their live shots, and the network notices,
among them the flamethrower's and the detonator pulse's online. The
[weapon pickups](WEAPON_PICKUPS.md) can give the player row 2, whose
firing reaches these limits and stops the vehicle's chain (below). The
detonator's pulse `0048D980` (upgrade `0x26`) runs: its event 9 through
the receivers' `+98`, its map jitter and flash through the draw's pulse
host ([the detonator](SELECTOR26_RUNTIME.md#messages)). Factory input state is read from the live process; admitted
factory kernels do not write that state, while unowned mutation callbacks stop
explicitly.

Two explicit portable guards prevent repeated unsupported work. They do not
count as retail behavior acceptance:

- `PortableNonProgressingWeaponQueueGuard` stops an unlimited round only after
  its native writes when both cooldown and group cursor repeat and no wrap RNG
  was reached. This bounds malformed mutable rows whose native loop cannot
  progress; finite rounds and random wrap loops remain admitted. The firing
  owner must replace or narrow this guard if a proven retail row requires it.
- `firing_stop_00` records the reached node when a nonempty drain fails,
  including direction/position, factory/effect and release prefixes.
  `PreviouslyStopped` blocks that actor's whole chain, including newly prepended
  nodes, so no later frame repeats completed writes or particle creation.
  Linked allocations remain for teardown; earlier writes/effects and normal
  head advances are retained. This is a fail-closed boundary, not a successful
  emission. The future owner of the stopped service must explicitly define
  resumption before removing this guard.

The initial intro factories construct particle class `00526EFC`. Its shared
[particle owner](PARTICLE_RUNTIME.md) now executes `00466250` movement and
`004663E0` ten-segment drawing. The separate
[collision authority](PROJECTILE_COLLISION_RUNTIME.md) owns ordered actor,
furniture and terrain queries, `00489600` impact birth, material bursts,
live damage/statistics and supported class retirement. The same retained
arena, task, terrain, resource, process and RNG owners cross these boundaries.
Collision auxiliary traversal consumes actor pose and packed values through
its own native context; it does not change accepted eye/scaffold drawing policy.

The [1,100-frame production smoke](INTRO_SEQUENCE.md#verification-and-acceptance-boundary)
reports successful drains as `World weapon firing: N shot(s) emitted by M
nonempty drain(s)`. Projectile contact and live health changes have separate
diagnostics, so construction alone cannot count as combat acceptance. Required
update/draw/retirement services stop only the reached particle allocation and
preserve its earlier writes; another allocation continues. Newly reached drain
errors still use the actor's retained no-replay guard. The production route's
current verification and remaining cinematic limits belong to the intro authority.

## Native verification

The executable-byte Unicorn fixtures run unmodified code with `FPCW=027F`
(53-bit arithmetic). The producer preserves the unspilled `00417DA0` length
and the final producer's distinct float32 length spill before division; the
fixture checks exact direction bits, live RNG and behavior `+20`. The fixtures
record explicit allocator, target, free, emitter, factory and audio boundaries.
Supplied lower-service returns authenticate the resumed native suffix; they do
not establish that the lower service itself succeeded in retail.

- [Producer receipts](../../crates/inf-game/tests/fixtures/firing_producers_native.json):
  24 predictive/scattered cases, lead saturation, gravity, signed-WORD wrapping,
  zero direction and shared RNG consumption.
- [Queue receipts](../../crates/inf-game/tests/fixtures/firing_queue_native.json):
  17 cases covering guards, finite/unlimited ammunition, wrap RNG, alternate
  selection, age acceleration, no-node rounds, actual NULL results and an
  allocator-stop prefix.
- [Teardown receipts](../../crates/inf-game/tests/fixtures/firing_teardown_native.json):
  successor publication, node-before-array frees and the reached audio boundary.
  Portable tests compare node-free order and failure head custody; Rust-owned
  array drops have no external deallocator failure service.
- [Drain receipts](../../crates/inf-game/tests/fixtures/firing_drain_native.json)
  and [emitter receipts](../../crates/inf-game/tests/fixtures/firing_emitter_native.json)
  authenticate the reached direction/position, factory and audio prefixes.

The producing scripts remain local scratch as requested; committed fixtures
are LF JSON bytes. No new retail capture was needed. Full shot lifecycle and
matched complete-intro visual acceptance remain separate from this shared
firing-chain milestone.
