# Shared projectile collision and impact

This authority owns the `00526EFC` collision lane, its live damage composition,
ground bursts and separate cosmetic impact birth `00489600`.
[PARTICLE_RUNTIME.md](PARTICLE_RUNTIME.md) owns the
physical pool walk, update/draw dispatch and particle retirement;
[PARTICLE_FACTORY_RUNTIME.md](PARTICLE_FACTORY_RUNTIME.md) owns the projectile
factories and normal deleting body. No mission, OVL or model identifier
selects these shared policies.

## Collision admission and service order

`00465FA0` reads the selected animation frame through `004657E0`. Its radius
is the wrapped signed DWORD product of frame signed WORD `+08`, animation
unsigned WORD `+0A` and particle DWORD `+2C`, arithmetically shifted by 17.
`00465570` then short-circuits these probes in order:

| Probe | Native owner | Contact response for `00526EFC` |
| --- | --- | --- |
| Actors | `00465390 → 004884F0 → 0047DA70 → 0047CEB0` | Virtual `+18 = 00466080` |
| Furniture | `004653B0 → 00435CB0 → 00435D50` | Virtual `+14 = 00466CF0` |
| Ground/water | `00465110` | Virtual `+10 = 00466530` |

Factory classes `00526F20` and `00526F60` share this lane with the same actor
and furniture responses. Their `+10` responses are `004661F0` and
`00466A00`; [PARTICLE_RUNTIME.md](PARTICLE_RUNTIME.md#factory-classes-00526f20-and-00526f60)
owns those.

Only a true completed collision reaches the `00465FA0` suffix. A nonnull
particle packet DWORD `+38`, followed by a clear flags bit 0, invokes
`004887D0` with the current particle position, current source handle and
notification flag 1. That service orders furniture/terrain radius work
`00436390`, actor radius work `00479870`, then conditional network notice
`00492500`. A failed earlier probe or response never reaches this suffix.

[collision.rs](../../crates/inf-game/src/process_particles/impact/collision.rs)
owns this order. `ParticleCollisionContext` loans the real pool, update
request, process state and RNG to reached callbacks. The live services may
mutate them; source/flags/secondary-packet reads remain at the corresponding
native sites. No callback can infer a successful hit from an unavailable
geometry owner.

## Actor and furniture swept tests

`004884F0` computes the movement's wrapped signed DWORD squared sum, calls
integer square root `0048EFC0`, then sign-extends the root's low WORD before
division by two. Each signed movement WORD is separately divided toward zero
by two. The first center is the current position minus those half movements,
with WORD wrapping. Actor scan radius is `max(half_length, particle_radius)`;
furniture scan radius is `half_length + particle_radius`.

The actor broadphase walks manager `+48` order. It requires actor flags
`+2C & 4`, nonzero unsigned radius `+28`, unsigned axis bounds and unsigned
wrapped squared-distance admission against the combined radius, then excludes
actor `+84 == particle +14`. Its coordinate differences are full signed
DWORD differences of separately sign-extended WORD positions. It retains
only candidate identities in the temporary list. Every subsequent precise
`0047CEB0` scan rereads the actor position, radius `+40`, model `+42` and
retained transform; the broadphase snapshot does not become precise geometry.

[world_projectile_collision.rs](../../crates/inf-game/src/world_projectile_collision.rs)
implements that precise query. It subtracts the current actor position with
low-WORD wrapping and sign extension, adds unsigned actor radius `+40` to the
full query DWORD radius, then applies the native unsigned inclusive cube and
squared-distance gates. An admitted query resolves current H8 model `+42`,
retains the actual basis and executes auxiliary `004F2C40`. Radius zero selects
the explicit point callback table `00544BF0`; other radii select sphere table
`00544C08`. Their sphere-pair callback and the sphere table's `8F` box callback
own normals and displacement through [shared geometry](CONTACT_GEOMETRY.md), and the
auxiliary interpreter owns conditional branches and best-contact publication.
The point table's box policy is its own [`004F1D20`](CONTACT_GEOMETRY.md#point-box-004f1d20);
sphere-box admission does not silently reuse it for a zero-radius point query.
It clears the output flag at entry, while a later rejecting sphere callback
preserves any previous result. Coarse model-radius admission is never a hit.
Missing models, unknown operations and unknown callbacks remain typed feature
boundaries. Required auxiliary failures use the existing
`RegisteredMarkUnwindApproximation` to contain stopped scratch ownership;
that recovery is neither native success nor retail acceptance.

Both scans first query the midpoint, then repeatedly halve the signed low-WORD
scan radius and clamp it to the original particle radius. Half movements are
also divided toward zero; the earlier center is tried first. A miss there
tries the later center by adding twice the half movement. Both misses end the
search. Actor geometry scans preserve temporary-list order at every step.
Furniture adds four to each query radius and reuses the wave clock captured
at `004653B0` entry.

The portable temporary candidate collection uses Rust allocation; it does not
claim the original `0047DAC0` allocator-null or allocator-stop behavior.
Native receipt cases for those external allocator boundaries are retained as
evidence and excluded from the admitted comparison domain. The real class
responses `00466080` and `00466CF0` return true after normal completion, so
receipt segments supplying a false virtual response are likewise outside this
class's comparison domain.

[world_projectile_furniture.rs](../../crates/inf-game/src/world_projectile_furniture.rs)
owns the actual `00435CB0/00435D50` resident query. The outer loop walks signed
low-WORD X offsets, then Z offsets, from `-512-radius` through
`sign_extend(low_word(radius))+512` with the current spacing. It returns early
only when the contact flag equals 1; empty lookup cells preserve the current
output. Each cell authenticates the prepared terrain's actual H12 registration
and allocation, chooses the flagged sparse record or retained process fallback,
then resolves its H11 placement to H8 model. A flagged cell without its sparse
record is a typed reached-pointer boundary, not a fallback lookup.

The selected state high nibble chooses the retained furniture basis.
Process pointer `00546E14` is published before the model-radius gate and kept
in `ProcessFurnitureQueryState`; startup reads its real file-backed value.
Zero model radius clears only the contact flag. Other models use aligned
cell X/Z, low-WORD signed scaled height and low-WORD relative coordinates.
The radius is sign-extended from its low WORD. The squared-distance broadphase
uses signed `JLE`, unlike the actor query's unsigned comparison. Admission
then executes actual model auxiliary `004F2C40` with explicit point `00544BF0`
or sphere `00544C08` policy. Coarse radius admission never substitutes for a
precise hit. Returned identity includes the qualified H12 record; the later
`00436020` consequence still reacquires its own cell from contact X/Z.

`PortableNonProgressingFurnitureScan` reports a repeated low-WORD offset cycle
when a large radius makes the native loop unable to pass its signed upper
bound. Completed queries and process publications remain retained. This is a
portable stop, not a no-contact result; the replacement owner is the shared
`00435CB0` scan if that authored radius domain is established.

`PortableNonProgressingProjectileSubdivision` is an explicit portable guard.
A negative radius can leave integer radius and half movement at zero forever
while geometry keeps reporting a hit. After a completed repeated query leaves
center, half movement and radius unchanged, the port reports this typed
boundary instead of inventing a miss. The replacement owner is this shared
subdivision kernel if such authored negative-radius input is established;
this guard is not retail acceptance for that domain.

## Contact consequences

Actor response `00466080` first gates on particle flags bit 0. When clear,
it passes packet address `+34` and source handle `+14` to target `0047C6E0`.
Only after damage returns does it capture live manager statistics `+1BC`.
If present, it rereads particle `+14`, invokes source classifier `0047F750`,
target classifier `0047F710`, then captured statistics owner `00481050` with
the target and source classifications. `0047F750` uses `ret 4`, `0047F710`
uses plain `ret`, and `00481050` uses `ret 8`: the source classification stays
on the stack across the target query. A pre-damage snapshot cannot substitute
for the source reread or statistics capture.

[The live combat host](../../crates/inf-game/src/session_platform/particles/combat.rs)
composes that response with
[WorldDamageOwners](../../crates/inf-game/src/session_platform/particles/damage.rs),
the real task/mission `0047C6E0` host, class reaction and lifecycle services.
Admitted `+B4` owners are family `00428040`, derived `004283E0` for base/11/77,
the vehicle's `004212F0` for selector 2 (see the
[vehicle notes](VEHICLE_DRIVE_RUNTIME.md#the-vehicles-damage-004212f0)), the
0040D800 family's `0040E030` for selectors 15/30/37 (see the
[family notes](FAMILY40D800_RUNTIME.md#family-prefix)), and shared `0047CBC0`
for shared/16/88/90/93, the [grenade](SELECTOR26_RUNTIME.md) (selector 26), the
[charge](SELECTOR27_RUNTIME.md) (27) and the [mine](SELECTOR38_RUNTIME.md) (38),
whose `+68` answers itself. Other overrides stop explicitly.
Source `+108` runs `00429C70` for family429E20; family base/34/35 use their real
`00466D40` return. Lethal state 1 invokes shared state change and supported
family `00428140`, selector-11 retirement or the grenade's and mine's `+AC`
(`00458F80`). Behavior selection borrows the
actual process/factory/RNG owners. Network-specific damage paths and other
class retirement overrides remain reached boundaries. Nested selector-11
retirement radius still has its separately bounded damage-service adapter;
the projectile adapter does not establish that nested class's complete chain.
That adapter admits only selector-11 `+B4`. Its source `+108` now goes through
the same final-table binding, `ObjectArena::source_component_108`, as the
projectile adapter. Before this change it stopped there. The 800x600 intro run
reached that stop when a projectile killed a turret, and the turret's radius
damaged slot 36 with family ship slot 78 as the source.

Furniture response `00466CF0` uses the same bit-0 gate, then invokes
`00436020` with the contact record's WORD `+18/+1C` cell coordinates and
packet `+34`. Both actor and furniture responses return true even when their
bit-0 gate suppresses consequences.

[The production radial adapter](../../crates/inf-game/src/session_platform/particles/radius.rs)
composes `004887D0` through the shared terrain `00436390` and actor `00479870`
kernels. Explicit origins retain the original executable packet address and
either particle allocation identity or the
[furniture program callback's borrowed stack context](TERRAIN_RUNTIME.md#furniture-world-radius-callback).
Every block callback rereads the ten authored bytes; every position callback
reads its current source, including particle changes caused by nested damage
or scratch allocation reuse. The source actor
handle is the original call argument. Ordinary actor hits invoke the live
`WorldDamageOwners` damage/reaction/lifecycle composition; no pool, process,
terrain, arena or mission snapshot is substituted.

Both radial and direct furniture damage resolve H11 inside the actual resource
callback, after `00436020`'s network and no-furniture gates. Terrain runs before
actors even when its accepted count is zero. Network event `13`, mode 1, remains
a required reached service only when the actor scan reports nonzero alternate
hits and replication is requested; ordinary hits do not enter it. Active-network
mass scaling likewise reaches the explicit `00463AB0` settings boundary rather
than assuming an offline role. Completed damage, terrain writes and shared RNG
consumption survive either failure.

Terrain `00465110` classifies the water volume and publishes flags `0C`
before ground tests. Reached ground contact publishes the floor cache and
selects the H15 material code; a transition from above-water to touching or
below-water uses the fifth H15 code. The same classifier is shared with the
existing debris callback, with class-specific response kept separate.

Projectile terrain response `00466530` treats selector 6 as water. It consumes
one RNG draw; one quarter of results reach sound selection 9/10 and its pitch
draw through `004655B0`. Only after successful audio does it publish particle
Y from the supplied height, then invoke splash birth `004893A0`. It returns
false, allowing the projectile to continue. Other selectors call
`00489390(selector, 0800)` and `00488E00` at the prior position
`current_position - movement`, then return true. The projectile's bit-0 flag
does not bypass this terrain response.

## Ground impact burst and material sprite

[ground.rs](../../crates/inf-game/src/process_particles/impact/ground.rs) owns
`00489390 → 00488E00 → 004894A0`. Material selection reads the actual DWORD at
`0053D7C8 + selector*4`. Burst count uses signed wrapped
`((strength >> 8) * load_factor) >> 16`, changing exactly zero to one; a
negative count emits nothing. Each attempt increments the shared vector cursor
before reading table `00539960` modulo 100, divides its signed components by
four arithmetically and makes Y nonnegative. Vector selection consumes no RNG.
A rejected sprite type still advances that cursor and does not end the burst.

Sprite selectors 0/1/2/3/4/6/7 resolve their original animation tables; other
full selectors return NULL before allocation. Admitted births allocate size
`34`, category 1, invoke common construction and publish final class
`005287F4`. One RNG draw sets scale to `(low16 >> 9) + 192`; shadow intensity
is 256 for this burst caller. A second draw lengthens the wrapped WORD lifetime
using its halved base value. Scratch birth follows the same writes and RNG.

The class updates through `0048B0E0 → 00465620`, applying full gravity when
prior water flags `0C` are nonzero and half gravity otherwise, then signed
wrapped motion. Its terrain callback `00488D10` places ground contact at
`height + radius`, sets signed Y velocity to `-(old_y / 8)` with truncation
toward zero, and applies X/Z drag from `max(80-new_y,0) << 7`. Ground contact
returns false. Water uses the ordered optional audio draws, then publishes
water Y, creates the real splash and returns true for retirement. Audio or
birth failure retains the earlier writes and RNG; it does not claim a
completed bounce or retirement. Sprite drawing and the separate projected
shadow branch belong to [PARTICLE_RUNTIME.md](PARTICLE_RUNTIME.md).

## Cosmetic impact birth

[impact.rs](../../crates/inf-game/src/process_particles/impact.rs) owns
`00489600`. Only selector 0 is admitted; all other full DWORD selectors return
NULL before allocation, process-null reads or RNG. Allocation requests size
`34` and final reclamation category 1. Pool exhaustion or phase gating can
return the real nonnull scratch record, which still receives every constructor
write and RNG draw.

The constructor reads actual process-null `0054E894` after allocation, invokes
common category-1 construction, then publishes animation `0053CA88` and
intermediate class `00526F40`. Lifetime uses the DWORD at `0053CA94`, minus
one, narrowed to a WORD. It publishes `005287D4`, consumes one shared RNG
draw, writes `+30=0`, final class `00528814`, and scale `+2C=(low16>>9)+64`.
Missing process-null leaves the completed allocation prefix and marker; it
does not invent a source or publish a completed particle.

## Evidence bounds

Independent original-executable receipts use unpatched x86 with FPCW `027F`:

- [Impact birth](../../crates/inf-game/tests/fixtures/particle_impact_birth_native.json):
  13 complete native births, including scratch, selector rejection, wrapped
  coordinates, flags, shared RNG and prior active-list custody.
- [Actor sweep](../../crates/inf-game/tests/fixtures/particle_actor_collision_native.json):
  19 native segments; 16 admitted Rust comparisons retain precise-query order,
  signed/unsigned arithmetic and reached geometry/response failure prefixes.
- [Furniture sweep](../../crates/inf-game/tests/fixtures/particle_furniture_collision_native.json):
  13 segments; 12 admitted comparisons cover earlier/later subdivision,
  captured clock, wrapped arithmetic and failures.
- [Furniture cell scan](../../crates/inf-game/tests/fixtures/projectile_furniture_scan_native.json):
  11 original `00435CB0` segments covering cell order, flag carry, signed bounds
  and explicit stopped-query prefixes.
- [Furniture model admission](../../crates/inf-game/tests/fixtures/projectile_furniture_cell_native.json):
  13 original `00435D50` segments with supplied resident/geometry boundaries;
  actual stack-basis setup, terrain-cell lookup and basis selection execute.
  Separate canonical composition uses original H11/H8 model data and precise
  auxiliary sphere contact with qualified terrain identity.
- [Actor model admission](../../crates/inf-game/tests/fixtures/combat_actor_direct_query_native.json):
  17 native segments establish wrapped admission and the precise query
  operands. Rust composition binds those operands to an actual retained model;
  this substitution does not claim the supplied native model body was run.
- [Direct point/sphere callbacks](../../crates/inf-game/tests/fixtures/combat_direct_sphere_native.json):
  13 native contact results preserve all five DWORDs, including rejection
  carry. [Auxiliary rotation](MODEL_AUXILIARY.md#saved-basis-rotation-30)
  has separate native instruction evidence and saved-context custody.
- [Ground birth](../../crates/inf-game/tests/fixtures/combat_ground_birth_native.json),
  [burst producer](../../crates/inf-game/tests/fixtures/combat_ground_producer_native.json),
  [update](../../crates/inf-game/tests/fixtures/combat_ground_update_native.json)
  and [contact](../../crates/inf-game/tests/fixtures/combat_ground_contact_native.json)
  compare selector rejection, cursor/count arithmetic, constructor bytes,
  gravity, motion, bounce and water failure prefixes. The burst comparison
  excludes a supplied third-factory stop unavailable in the concrete service;
  first-source failure still verifies the retained cursor/allocation prefix.
  Contact receipts stop at the splash boundary; Rust tests separately run the
  real splash birth and account for its additional RNG draw.
- [Collision wrapper](../../crates/inf-game/tests/fixtures/particle_collision_wrapper_native.json)
  and [actor callback](../../crates/inf-game/tests/fixtures/particle_hit_callback_native.json)
  independently establish short-circuit order, packet/flag gates and the
  damage/statistics capture sequence. They remain segmented evidence: the
  current concrete production adapter does not expose their supplied reentrant
  manager/source/classifier mutations as artificial callbacks.
  A separate loaded-world test invokes concrete `WorldProjectileHost::actor_impact`
  with an authored primary packet, actual family health, matching task-source
  clearing, damage totals and the full hit-count matrix. It preserves the
  distinction between raw resisted statistical damage and difficulty-scaled
  health damage; it does not substitute virtual consequences.

The sweep segments supply precise geometry verdicts at explicit boundaries;
they do not establish the geometry body, damage reaction, sound backend or
complete intro encounter. Those require their concrete production owners and
the integrated acceptance run. Temporary native allocator/free events in the
receipts are not claimed as Rust heap-address or allocation-failure fidelity.
