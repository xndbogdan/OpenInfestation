# Infestation weapon pickups

This document owns how the ACAM's weapons reach the player: the furniture
callback `00437880` that drops a pickup where destroyed furniture stood, the
weapon cases of the pickup grant `0046B2B0`, and the full-row notice
`004693C0`. The pickup objects themselves (`0046D870`, `0046DAF0`) and the
grant's shared parts (`004750C0`'s notice, `0046CBB0`'s refresh, the `+100`
bit, `0041A600` and the message) belong to
[ACTOR_RUNTIME.md](ACTOR_RUNTIME.md#respawn-pickups-and-the-world-loader);
cycling between owned weapons belongs to
[VEHICLE_DRIVE_RUNTIME.md](VEHICLE_DRIVE_RUNTIME.md#weapon-cycling-and-the-panels).

The code lives in
[pickup_callback.rs](../../crates/inf-game/src/world_furniture_effect/pickup_callback.rs)
(with the world's services in
[furniture.rs](../../crates/inf-game/src/session_platform/furniture.rs)),
`grant_player_pickup` in
[player.rs](../../crates/inf-game/src/object_runtime/player.rs), and the
delivery in
[message.rs](../../crates/inf-game/src/object_runtime/player/message.rs).

## Rows

The vehicle's sixteen weapon rows (`00530158`, copied into `+26C`) are gated
twice. The weapon cycle skips a row whose upgrade ID (the WORD table
`005305B0`) the player's `+100` lacks, and a row fires only while its
behavior entry's count (`+00`) is nonzero. `0041A600` clamps each count to
the limit table `0053A310`: rows 0 to 3, 6 and 7 have limit -1, so their
counts stay -1 and their IDs alone gate them; the others hold up to 8.

| Row | Weapon (registry kind) | Upgrade ID | Pickups that stock it | Limit |
| --- | --- | --- | --- | --- |
| 0 | Cannon (1) | `0x32`, granted at every respawn | | -1 |
| 1 | Plasma (5) | `0x9B` | `0x9B`, `0x9C` | -1 |
| 2 | [Lasers](SELECTOR31_RUNTIME.md) (8, selector 31) | `0x98` | `0x98` | -1 |
| 3 | Flamethrower (10) | `0xA1` | `0xA1` | -1 |
| 4 | [Rocket](SELECTOR25_RUNTIME.md) (11, selector 28) | `0xA0` | `0xA0`, `0x1E`, `0x1C` | 8 |
| 5 | [Homing missile](SELECTOR25_RUNTIME.md) (15, selector 25) | `0xA3` | `0xA3`, `0x1D` | 8 |
| 6 | Detonator (23) | 0, always owned | `0x26` (Sonic Shock) | -1 |
| 7 | Kind 28, selector 68 | `0x33`, settings `+84` bit `0x40` | | -1 |
| 8 | Standard grenade (14) | 0, always owned | `0x25`, `0x1A` | 8 |
| 9 | Power grenade (17) | `0xA9` | `0xA9`, `0x1A` | 8 |
| 10 | Smoke grenade (18) | `0xA8` | `0xA8`, `0x1A` | 8 |
| 11 | Mine (19) | `0xAA` | `0xAA`, `0x1B` | 8 |
| 12 | Kind 20, selector 27 | `0xAB` | `0xAB`, `0x1B` | 8 |
| 13 | Mine (21) | `0xAC` | `0xAC`, `0x1B` | 8 |
| 14 | Chain grenade (22) | `0xAD` | `0xAD`, `0x1A` | 8 |
| 15 | Napalm grenade (24) | `0xAE` | `0xAE`, `0x1A` | 8 |

No world places a `0x26` or `0x33` pickup; see
[where they come from](PROFILE_RUNTIME.md#the-completion-bonus).

The port fires every row; row 12 throws the
[charge](SELECTOR27_RUNTIME.md) and row 7 the
[Cranial Implanter](SELECTOR68_RUNTIME.md).

## Furniture pickups

Destroyed furniture runs its effect program
([TERRAIN_RUNTIME.md](TERRAIN_RUNTIME.md)); its callback `00437880` drops
the furniture's pickup. EDX holds the cached H11 entry (two WORDs: the model,
then a parameter); a null entry drops nothing. Byte `+2`, the parameter's low
byte, less `0x20` indexes the table at `00437A48`:

| Byte | Pickup | Value |
| --- | --- | --- |
| `0x20`, `0x21`, `0x22` | `0x0E`, `0x0F`, `0x10` | 1 |
| `0x23` | `0x25`, or `0xA9` | 4 |
| `0x24` | `0xAA` | 4 |
| `0x25` | `0xA8`, or `0xAD` | 4 |
| `0x26`, `0x27`, `0x28` | `0x1C`, `0x1D`, `0xAB` | 4 |
| `0x29` | 4 | 1000 |

Level 1 places two crates, both byte `0x23`, at `(3500h, 9200h)` and
`(3600h, 9200h)`: the two Infest02 broke. Across the worlds the port's
loader opens, byte `0x23` crates are common, byte `0x25` ones sit in worlds
12, 20 and 21, byte `0x26` (row 4's kit) in worlds 0 (18 of them), 5 and
22, and one byte `0x27` (row 5's) in world 3; none place `0x24`, `0x28` or
`0x29`.

For bytes `0x23` and `0x25`, the first player (`00488AA0`) that owns the
upgrade in its grant mask (`004752E0` on `+9C`) gets the upgrade when one
draw of `0048F060` has bit 8 set; without a player or the upgrade nothing is
drawn. Any other byte drops a pickup whose kind is the stack XYZ pointer
itself (`00437A29`); the port stops there. `0046D870` takes that stack XYZ,
so the burst's later callbacks see whatever it moved, and the callback
returns AL 1.

## Weapon cases

The grant's cases for the weapon IDs (`0046B315..0046B9E4`), offline:

- First, `0046B315` sets a first-pickup flag for `0x25`, `0x98`, `0x9B`,
  `0xA0`, `0xA1`, `0xA3` and `0xA8`..`0xAD` when `0046BDC0` finds the ID
  not yet in `+100`. A first pickup selects its weapon: the primary `+10`
  becomes the row modulo the signed primary count `+0E` (IDIV), the
  secondary `+11` the row.
- `0x9B`, `0x98`, `0xA1` set rows 1, 2, 3 to -1 in the player's `+150`;
  `0x26` sets row 6.
- `0xA0`, `0xA3`, `0x25` and `0xA8`..`0xAE` stock their row: when the
  behavior entry's count (`0046D310`) equals the row's limit, nothing is
  taken (AL 0) and `004693C0` runs; otherwise the row gains the signed
  amount. `0xAE` selects nothing.
- `0x1E` grants `0xA0` first (a whole nested `0046B2B0`), then stocks row 4
  as `0xA0` does, without selecting. `0x1C` and `0x1D` grant `0xA0` or
  `0xA3`, then stock row 4 or 5 with prefix `0x13A`. `0x9C` grants `0x9B`.
- `0x1A` adds the amount to rows 8, 9, 14, 10 and 15, and `0x1B` to rows 11,
  13 and 12, with no limit test.
- `0x11` sets player command `0x38`, silent; `0x77` takes nothing offline.

A repeat pickup normally says nothing, but `0x25`, `0xA9`..`0xAD`, `0x1C`
and `0x1D` clear the grant's repeat silence (`[esp+12h]`): their repeats say
"%s %s" with prefix `0x13A` and the record text, without the detail. Then
the tail sets the `+100` bit and `0041A600` clamps the rows into the
behavior.

`004693C0`, offline, compares the animation clock `005471C0` with the gate
`0054ED14` (signed). Past it, the gate moves to the clock plus 150 and the
H0 string `0x304` goes to the player's message (`004135D0`). Nothing else
reads the gate. Natively it lives for the process; the port keeps it with
the world players in the retained arena, so a new campaign starts it at 0.

The port returns the nested grants and the full row with the outcome, and
`deliver_pickup_outcome` sends them in native order: the notice, each nested
grant whole, the full row's notice, the command, then the message. The world
pickup and the factory's grants both deliver through it. No weapon pickup
reaches the dependent-record notice: the high records (`0x95..0xAE`) depend
only on IDs `0x43..0x81`.

## Verification

The [grant receipts](../../crates/inf-game/tests/fixtures/pickup_grants_weapons_native.json)
hold every `0046B2B0` call of Infest02 and Infest03 with the player's mask
and `+100..+19F`, the hull, the behavior's bytes `+0C..+13` and its sixteen
counts, in and out, and the nested `004693C0` and `004135D0` calls
(ledger).
`pickup_grants_match_the_recorded_weapon_rows` replays all 47: the
respawns, crystals and counters, and Infest02's two grenade pickups (`0x25`,
four each), the first announced in full and the second as a repeat, both
stocking row 8 from 0 to 4 and 8. The result, the mask, `+100`, the rows,
the selections, the counts and each call's notice and message match.

`check_weapon_grants.py` runs the unmodified
`0046B2B0` in Unicorn for every weapon ID in seven states (a first pickup, a
repeat, a full row, a full row whose notice the gate holds back, three
primaries, a negative amount and rows past their limit) and writes
[weapon_grants_oracle.json](../../crates/inf-game/tests/fixtures/weapon_grants_oracle.json).
`0046BDC0`, `0046D310`, `00475320`, `004693C0` and the nested calls run
natively; the lookups, `004750C0`, `0046CBB0`, `0041A600`, the command, the
text and the formatter are recording stubs.
`weapon_pickups_match_the_native_grant` replays the 161 cases: the result,
`+100`, the rows as `0041A600` then clamps them, the selections, the order
of nested grants, full rows, commands and messages (with or without the
detail) and the gate match.

The three recorded furniture drops are byte `0x23`: Infest02's two crates
gave `0x25` to a player without `0xA9`, drawing nothing; Infest03's player
owned `0xA9` and the draw from `634D6F0A` set bit 8.
`the_grenade_crate_upgrades_on_a_draw_when_the_player_owns_the_power_grenade`
and `each_furniture_byte_names_its_pickup` cover the table.

A Level 1 probe broke the crate at `(3600h, 9200h)` with a strong cannon-channel
hit: its effect program burst and dropped `0x25` with value 4 at
`(3600h, FFE0h, 9200h)`, where Infest02's landed, and the vehicle driven onto
it took four grenades, announced, the HUD's secondary showing 4.
