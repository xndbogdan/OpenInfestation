# Infestation shared actor factory

This document owns the constructor6 path through `004603B0`, its shared
command-policy custody, and dynamic child creation through `0047AAF0`.
Shared actor fields,
registration and lifetime remain in [ACTOR_RUNTIME.md](ACTOR_RUNTIME.md).
Class-specific factories retain their own constructor and completion phases;
sharing the command walker does not make those classes interchangeable.

## Native class and phase order

The selector WORD is masked before the selector-minus-one jump table lookup.
Row `00461A18` selects `004604F5`, which requests `0x138` bytes, tests allocation
success and calls `0047A7A0`. The latter installs table `00527C00` at
`0047A847`. There is no `00425E60` call on this path. That derived constructor
adds `0048E120` world-mask selection and `0048F060` process RNG work after its
shared parent; constructor6 must not acquire that extra phase or derived state.
This call-chain distinction does not establish that every nested shared
resource callback is globally RNG-free.

The existing `ActorExtension::Shared` represents this actual final class.
The H13 payload remains the canonical shared payload; constructor6 has no
`00426C10` extension. No new constructor tag, model-dependent policy or parser
reinterpretation is needed. Its recovered table selects these existing phases:

| Phase | Native consumer | Portable responsibility |
| --- | --- | --- |
| Allocation | `004604F5 -> 0047A7A0` | Shared fields and handle registration; no derived inputs or extra derived RNG phase |
| Context/payload | `+0C=0047AD70` | Actual H13 model, context, shared payload and local owner |
| Guard | `+38=00401370` | True without a derived behavior guard |
| Commands | `+04=0047ADA0` | Shared command reads and effects in reached order |
| Postinitialization | `+1C=0047AAD0` | Unsigned `+28 >> 3` stored at `+74` |
| Completion | `00460120` | Dirty clear, mutable listener walk, conditional shared network callback, initialized flag |
| Network | `+40=0047E2E0` | Existing shared network policy and retained writes |
| Loader after-load | `+20=00401320`, `+34=00401310` | No-op class callback followed by after-load flag publication |
| Lifetime | Shared destructor chain | Existing unregistration and embedded-list cleanup |

Live model getters, lighting and ordinary draw routing follow the actual
shared class. They cannot acquire derived health, behavior or vehicle queries
merely because an authored model looks similar. Shared custom vertex `+10`
and ordinary update/disposal work retain their separate feature boundaries.

## Retained command image

`ObjectConstructionRequest.command_allocation` is independent from class
policy. When present, the shared command policy validates that retained owner
and rereads its current signed count, relocated pointers and consumed operands.
It does not substitute the immutable original H14 suffix after earlier writes.
Conditional grant operands remain lazy when no first player is present;
rejection does not read later commands. Typed-pointer aliases retain their
required-scalar errors at the actual consumed field.

The generic shared factories for6/90/93 forward the same optional allocation.
The direct `run_actor_commands` loan supplies its declared immutable suffix;
the fixed class93 factory supplies a literal NULL suffix. Those callers pass
no retained command allocation. No extra owner is fabricated for either domain.
Derived policies continue to run their actual admission guard. The former
constructor6 exception inside that policy has been removed because constructor6
now enters the shared policy directly.

## Dynamic child creation and selector36 model transition

The [shared virtual `+4C=0047AAF0`](../../crates/inf-game/src/object_runtime/actor/child.rs)
creates an object through `00460310`, then
separately queries that object's virtual `+18` actor component. The returned
object and the optional actor output are distinct: a nonactor object can be
returned normally with a NULL actor output. The mode is 1 only when the parent
has flag bit31 clear and handle bit26 set. The process NULL handle is an actual
input read before the factory, independent of the registry's lookup sentinel.

Construction initially borrows the parent's position and no rotation pointer;
the position is consumed during initialization after allocation.
The lower actor constructor separately copies current process NULL into `+88`
at `0047A885..0047A88A`. Creation listeners and a retained constructor stop
therefore observe that operand even when it differs from registry NULL; the
wrapper's source inheritance has not run yet.
After normal actor resolution, the wrapper publishes the actor output, marks
dirty bits, copies the current parent handle to child `+88`, and copies current
classification `+B4`. It then calls the parent's virtual `+48` before applying
the optional position override and copying a nonnull returned basis. These
are reached reads after construction, not fields from a frozen parent snapshot.
The optional override models the caller-owned stack value used by `004824A0`;
arbitrary native pointer aliases are outside that request's admitted domain.

The [selector36 `004824A0` owner](../../crates/inf-game/src/object_runtime/actor/derived/selector36_child.rs)
uses that shared path only with nonzero classification
and clear spawned bit `+264 & 1000h`. Its alternate stored position requires
both actor flag `02000000h` and `+264 & 0800h`; otherwise it captures the
parent's position and adds unsigned size `+28` to Y with WORD wrapping. H13
payload `+78` selects the child resource. Successful child resolution adds a
local velocity through `0047B540 → 0047A6C0 → 004909E0`: Y is half size or
twice size according to payload flag4, and Z is zero or twice size according
to flag8. This uses the child's basis and existing velocity, including signed
WORD interpretation and wrapped additions.

Only after normal child velocity return does the parent clear flag0800h, set
flag1000h, publish the model from payload `+76`, and call visual refresh
`0047A110`. The final writes clear `+74` and set timer `+27E` to `FFFFh`.
A refresh failure therefore retains the new model and flags but not those
last writes. NULL actor output bypasses this complete parent suffix.

Dynamic construction also distinguishes a native false admission result from
a port-side service boundary. Native rejection runs its existing cleanup;
a typed stop retains the allocated object's reached constructor state and
identifies that object in the error. It does not publish the wrapper's actor
output or run the parent suffix. This policy is explicit at the shared factory
call, separate from existing load-time construction cleanup.
An arena-owned stopped-construction marker contains that incomplete child
without rewriting its native fields or removing its list nodes. The update
visitor still transfers the node but skips the callback, including children
published during the same pass. Draw and lighting report explicit omissions;
live projectile, radius-effect and actor-selection scans exclude the stopped
object, as do actor broadcasts. Selector93 retains its original random draw
and list divisor, but reports a typed stop if that draw selects an incomplete
object; it never resamples. Destruction
releases the marker with the allocation, and a reused slot starts unmarked.

The resource operand has unusual provenance: `00482511` forms a stack-position
address in EAX, then `00482516` replaces AX with payload `+78`. `00460310`
preserves the complete DWORD for a lazy H13 supplier and `00460120`; it does
not clear the high half. The resident initialization route only consumes its
signed low WORD for lookup and low WORD for the stored resource identity.
Portable child requests retain this symbolic stack-address provenance rather
than inventing a captured native pointer. A reached lazy supplier remains a
typed boundary; callers supplying a complete DWORD keep that separate value.
The production child adapter currently admits offline construction only.
It does not equate network-session DWORD `0055F448` with the distinct
network-world DWORD `0055F3A0`.

### Selector 22 fading visual

The four resource58 children of the dying Selector36's stage 1 are selector
22. Constructor `0042DBE0` allocates 0x4C bytes over the visual base
`0047A030`, then sets final vtable `00521830`, `+44 = 0xFFFF` and
`+48 = 0`. That table differs from the visual base `00527B68` only in update
`+24` and model value `+14`. Its `+18` returns NULL, so `0047AAF0` returns
the object without an actor output and applies no position override. The
children therefore stay at the parent's position, and the four stage-1 Y
offsets move only the paired bursts. `00479DB0` runs the visual context
`0047A050` and then the payload flags. Resource58's flags put each child on
both the draw and the update lists.

Update `0042DC00` subtracts `((delta >> 12) * clamp(+44, 0x1000, 0xD000)) >> 6`
from `+44`, using a signed clamp, a wrapped product and logical shifts. A
result below `+48` (compared signed) disposes the object through `00479FA0`
and leaves `+44` unchanged. Model value 1 (`0046E050`, shared with the timed
pickup) returns the low WORD of `+44`, so the model fades with it; other
values return their own index. At 30 fps a child lasts 30 updates.

check_selector22_update.py runs the
unchanged update over 1056 combinations of fade, threshold and delta,
including the clamp edges, signed thresholds and wrapped products. It also
steps the constructor's 0xFFFF start at 30 fps until disposal. The
[Rust comparison](../../crates/inf-game/src/object_runtime/selector22/tests.rs)
matches every stored value and disposal. In the intro smoke, the dying
Selector36 creates four children with model `0x6E`. Each fades for seven
125 ms ticks, and the disposal pass then destroys it.

### Selector 65 water splash

The vehicle's water splash (H13 argument `0x3B`, H8 model 116, payload
flags 3) is selector 65. Constructor `0042DC60` is selector 22's with
`+48` = `00531FB0` (`0xA000`, never written), and its vtable `0052187C`
is selector 22's but for its update `0042DC90`
([selector65.rs](../../crates/inf-game/src/object_runtime/selector65.rs)):

- the fade steps by `((delta >> 12) * clamp(+44, 0x1000, 0x4000)) >> 7`
  and disposes below `+48`, so at 125 ms ticks the splash lasts seven;
- then a process RNG draw masked with `0x15` counts the tries. Each draws
  again and, when odd, a third draw gives an angle: a point
  `(0x8000 - (+44 >> 1)) >> 5` from the splash along it (the ring widens
  as the fade falls) throws a splash particle `00489600` (type 0, zero
  velocity) at the splash's height wherever that cell of H12 lies below
  the water plane.

check_selector65_update.py runs the
unchanged update over 36 cases (fresh, mid and expiring fades, signed
limits, zero, small and wrapped deltas, three terrain layouts) and the
[Rust comparison](../../crates/inf-game/src/object_runtime/selector65/tests.rs)
matches each fade, disposal, splash position and the RNG.

## Validation and boundaries

[Child receipt tests](../../crates/inf-game/src/object_runtime/actor/derived/selector36_child/native_receipts.rs)
compare the shared wrapper, selector36 caller and local velocity against 53
unmodified-executable cases at FPCW `027F`. They distinguish raw object return
from actor output, full resource DWORDs from symbolic stack operands, fresh
parent reads after callbacks, inherited basis, wrapped velocity and retained
failure prefixes. Lower allocation, virtual queries and model refresh are
explicit supplied boundaries in these receipts; they do not claim native
allocator or complete world execution.

[Factory tests](../../crates/inf-game/src/object_runtime/selector6_factory_tests.rs)
retain the original dispatch/body/table guards and exercise actual resident H13
classification, shared class identity, absent derived inputs, reached current
command bytes, rejection and completion custody. The supplied H12-miss domains
compare constructor6 with a genuine derived constructor to distinguish the
additional process RNG step. Existing shared/native actor and registry tests
own field, registration, network and lifetime checks.

These bounded tests do not establish heap payload initialization, native
allocation failure/SEH, every shared callback, or a matched world-load timeline.
Correcting the actual constructor is independent from the earlier selector30
custom8 capture; no model/placement/arena slot selects either class policy.
