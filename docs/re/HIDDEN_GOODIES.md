# Infestation's hidden goodies

What the game hides, and how to get each. It is all read from retail
`Infest.exe` and the world data: REA reads of the code, and a census of
every world's placements, factories and terrain furniture, loaded through
the port's world loader. Coordinates are world X and Z. Heights are world
Y, which grows upward (gravity `0047AD10` lowers the vertical speed).

## Scientists, scans and the research tree

This is the trick behind the early goodies: "scan everything with
scientists".

- **What a scientist learns.** A scientist on your side studies what it
  does not know yet. That can be an actor: a creature, vehicle or building,
  whose grant word `+7A` it reads through the
  [family target selection](FAMILY_TARGET_SELECTION.md#grants-and-canonical-interactions).
  It can also be terrain furniture within 1,200 of it on either axis
  ([the grant grid](FAMILY_GRANT_GRID.md) `00468E30`; the furniture kind's
  grant WORD is at `00532F88 + kind * 16 + 0xC`). It walks over, studies the
  thing for a while, and keeps it in its own grants (`+24C`).
- **How it reaches you.** Beam the scientist up (C), or let it walk into
  your craft. Either way its grants merge into yours
  ([carried units](ACTOR_RUNTIME.md#carried-units): `00475350`, which grants
  each ID through `004750C0`). The notice reads "New blueprint: …" for an
  upgrade and "New info: …" for a creature or building.
- **The research tree.** Learning a creature or building also teaches the
  upgrades that depend on it. Granting any ID rescans the sixteen-byte
  records `0x95..0xAE` at `0053B088`, and each record names up to three IDs
  that teach it:

| Blueprint | Scan any of |
| --- | --- |
| Rapid-fire cannons, Dual fire cannons, Cannon Guidance System | Ichneu |
| Lasers | Homing tower |
| Laser power-up, Limpet Mines | the generator-fed Force Field (info `0x68`) |
| Laser guidance system | Electrified fencing |
| Plasma cannons | Quixle, Small gun turret, Assault craft |
| Plasma power-up | Spirax |
| Rapid-fire plasmas | Pursuit Ship |
| Dual fire plasmas | Tank |
| Plasma Guidance System | Heli gunship |
| Rockets | Gun Turret |
| Flamethrower | Jatom, Mydon |
| Dual fire flamethrower | Podwa |
| Guided Missiles | SAM Launcher |
| Pilot guided rockets | Mantron |
| Helicopter mode | Transporter, Heli gunship |
| Skimmer mode | Jet Fighter |
| Hovercraft mode | Hovertank |
| Proximity Mines | Assault craft, Hovertank |
| Smoke grenades | Fuel Storage Silo |
| Frag grenades | Trucks |
| Laser Mines | TL717 |
| Stun grenades | Powerball, Power Coupling |
| Napalm grenades | Demons |

- **Buying it.** Docking at a factory teaches you its stock and copies all
  your blueprints to it (`0042F490`: `00475350`, then `00475380`). Its
  Facilities then list every product you know and do not own yet
  ([the factory](FAMILY40D800_RUNTIME.md#docking-the-shop-and-production)).
  The products come in three lists of `0042F250`:
  - every factory has the 18 shared ones at `005320D8`: the Hovercraft,
    Helicopter and Skimmer modes, the Tractor Beam, Radar, Headlights,
    Infrared vision, Bomb, Portable mining rig, Proximity Mines, every
    grenade, Rockets, Guided Missiles and Pilot guided rockets;
  - a friendly factory adds the list at `00532240`: Turbo boost, Truck
    mode, Scanner, Resource scanner, Sonic Shock, the three cannon
    upgrades, the Flamethrower, ECM Unit, Remote Beamer and Prison Unit;
  - an enemy-type factory (placement command 22, `+15C = 1`) adds the list
    at `00532348`: the plasma and laser weapons and their upgrades, the
    shield codes, and the Limpet and Laser Mines.
- No factory in any world stocks the Flamethrower, Rapid-fire cannons, the
  Cannon or Plasma Guidance System, or the Laser guidance system. A factory
  offers them once you know them, and a scan teaches them.

## Terra Nova: the Radar from the dish on the hill

A player's account of this holds, and the data agree.

- Six scientists stand by Terra Nova's factory at (6584, 16679), on the
  plain at about -1,800. They already know the Scanner (grant `0x11`), so
  the first one you beam up teaches it to you.
- A radar dish stands on the upland above the plain, at (12544, 20480). It
  is terrain furniture of kind 5, whose grant is the Radar (`0x12`,
  "Enables mapping of the terrain"). Its ground is at -288, some 1,500
  above the plain and about 7,000 from the scientists, too far for them to
  study it on their own.
- Carry a scientist up there and beam it down within about 1,200 of the
  dish. It studies the dish, and when it is back aboard you get "New
  blueprint: Radar". The Radar is in every factory's shared list, so
  Terra Nova's own factory then builds it.
- Without the scan, Altair Prime has the first factories that stock the
  Radar (Helniazine, Sanrel-Dek and Tabuchi stock it too).
- It is the furniture the port drew (lookup 31 in bucket 160) when the
  user's own scientist brought the blueprint
  ([model scratch](MODEL_SCRATCH.md)).
- A second dish stands by the hidden base, at (36096, 56320), 3,000 from
  its gate. Sanrel-Dek (39168, 29184, by the start), Tabuchi (33024, 40448)
  and Shalapreth (41216, 28160) have one each.

Terra Nova has more worth scanning. These follow from the world data and
the tree; only the dish has been seen in play:

- the two Gun Turrets on your side by the start, at (10502, 15083) and
  (10453, 17100), teach Rockets, which no factory stocks before Tabuchi;
- its seven Fuel Storage Silos teach Smoke grenades;
- its Quixles, small gun turrets and two Assault craft teach Plasma cannons
  (which only an enemy-type factory builds), and the Assault craft also
  teach Proximity Mines;
- its Spirax teaches the Plasma power-up;
- the two Mydons at (32000, 22144) and (32000, 22656) teach the
  Flamethrower, which Terra Nova's own factory then builds.

## Sanrel-Dek: Lasers from the factory in the crater

- Sanrel-Dek's start lies in a basin at about -2,100, by the start gate at
  (36601, 28607). About 5,000 from it a walled crater rises out of the
  basin. Its walls climb some 2,600 within 500 to 800, to a rim at about
  +300 to +700. Its floor lies at about -900, 1,200 above the basin. To
  one side a narrow trench at the basin's depth parts it from the high
  ground (about +1,900) where the exit gate stands.
- On the crater floor stand a factory at (40006, 32429) and a warpgate at
  (39063, 33408), 1,400 apart. The way in is over the wall: one way
  is a wall-ride and a jump.
- The factory is an enemy-type factory (command 22) on no side (team 0),
  so you can dock. It stocks Lasers, Dual fire plasmas and the Portable
  mining rig. Docking teaches all three, and since only enemy-type
  factories build Lasers, this one builds them at once.
- Without it, Vernimab has the next factories that stock Lasers, two
  worlds on. Caspadril, Grach'to and the Defence Satellite stock them
  later. A scan of a Homing tower (on Vernimab or Grach'to) teaches them
  too.
- The crater's gate links to the start gate (selector 30's `+180`), so
  crossing it puts you out of the start gate. The start gate itself leads
  off the world (to Helniazine's third start) and links back to nothing,
  so the crater's gate is one-way: the quick way down, never a way up
  ([gates](SELECTOR30_RUNTIME.md#pass)).
- Sanrel-Dek's other two factories are enemy-type too. The one by the
  start, at (37857, 28162), stocks the Radar, Plasma cannons, Proximity
  Mines and Stun grenades. The one on the high ground at (32707, 31719)
  stocks the Portable mining rig and Rapid-fire plasmas.

## Terra Nova's hidden base

- Oolbryte's gate 4 hangs over the slope of a deep pit, with its ring
  about 1,000 above the pit floor. No other gate in any world leads to
  Terra Nova's third start, by gate 97: the hidden base
  ([the starts](SELECTOR30_RUNTIME.md#the-starts-and-their-gates)).
- The base's gate and factory stand only once you have finished Terra Nova
  (the completing gate grants `0x84`, which has no text). Before that,
  placement command 13 keeps them away.
- The factory is a friendly one. It builds Dual fire cannons, which no
  other factory stocks, and Smoke grenades. Docking also teaches the
  Plasma cannons blueprint, which an enemy-type factory then builds.

## BUGGY RACING and the hidden network card

- Naming a profile `BUGGY RACING` sets bit `0x80` of the settings' cheat
  word (registry value `Cheats`). So does finishing Exxodus by its gate to
  Helniazine (the gate's completion `+167 = 1`, `0048C40C`). Nothing clears
  the bit, so it stays on that installation.
- With the bit set, the network game's arena carousel (`0045EE20`) draws
  an eleventh card, and its row (`0045F560`) steps onto it. The card is
  sprite 1821 (`47.ovl`): a buggy racing between marker cones on a snowy
  canyon track. It is the one sprite missing from the ten other cards' run,
  1817 to 1827.
- The draw reads each card's sprite from ten 24-byte records at
  `005383BC`. The eleventh card's sprite is the word after them, and the
  next table starts right behind it. The records' other fields (a world
  number from 25 to 33, and for three cards a name: "Buggyball", "Aerial
  Banner", "Buggy Race") are read by no code. What choosing the eleventh
  card starts is untested.

## Sonic Shock

- What it does. It is the Detonator's upgrade (`0x26`, "This will detonate
  all explosives within an area"). With it, the Detonator sends its pulse
  (`0048D980`) instead of the plain detonation. Every explosive within
  4,000 goes off, whoever owns it: grenades, mines, missiles in flight and
  Bombs. The radar shakes for about 1.6 s, and the screen flashes white
  ([the detonator](SELECTOR26_RUNTIME.md#messages)).
- How to get it. No world gives it. Name a new profile `SEMPER UBI SUB UBI`
  (cheat bit 4), then open the computer's Cheats Menu (Tab, row 6):
  - Get Full Ship Upgrades fits it;
  - Get Full Research Tree (`00414AE0`, every ID into the player's grants
    `+9C`) teaches its blueprint, and any friendly factory then builds it
    (it is in their list).
  A network game's loadout gives it too
  ([where upgrades come from](PROFILE_RUNTIME.md#the-completion-bonus)).
- The port runs the Cheats Menu ([below](#the-cheats-menu)), and its start
  harness's `upgrades` gives Sonic Shock too.

## The name cheats

A new profile's name can be a cheat. It sets one bit of the settings'
cheat word, which the registry keeps
([cheats](PROFILE_RUNTIME.md#cheats)):

| Name | It opens |
| --- | --- |
| `INVINCE ME` | the Cheats Menu's Invulnerability |
| `GIVE IT TO ME BABY` | Skip to, Complete Level and Scavange Level |
| `SEMPER UBI SUB UBI` | Show Full Database, Get Full Research Tree and Get Full Ship Upgrades |
| `I AM HUNGRY` | Fill resource crystal tanks |
| `DR FRANKENSTEIN` | the Cranial Implanter at every respawn ("Cranial Implanter cheat now activated") |
| `BUGGY RACING` | the network game's eleventh card |

## The Cheats Menu

Tab opens the computer; its sixth row, Cheats Menu, shows once any of the
first four names has been entered, and inside it only the rows those
names open ([the Cheats Menu](PLAYER_MESSAGES.md#the-cheats-menu)):

- Skip to: Left and Right pick a world from Terra Nova to Caradion (2 to
  24, Powerball excluded); Select gives what the campaign would have
  given by then (the upgrades of every stage up to it, the factory's
  token levels, a hold full of scientists and full crystal tanks) and ends
  this world into that one.
- Show Full Database: the database lists every entry, known or not, and
  Select on an entry grants it.
- Get Full Research Tree: every blueprint and fact.
- Get Full Ship Upgrades: every upgrade but Invulnerability, with 8 of
  each ammunition and the research tree, so Sonic Shock too.
- Invulnerability: the vehicle takes no damage.
- Complete Level: every pickup the vehicle can take is taken, every
  factory gives its products, every unit on the aliens' side but the
  factories is killed, and the scientists board the hold while it has
  room.
- Scavange Level: Complete Level, and every creature's and building's
  scan grant on top.
- Fill resource crystal tanks: the three crystal tanks to 30.

The port's launcher can open every row without the names (Options, "The
computer's Cheats Menu shows every cheat", or `--all-cheats true`); the
saved cheat word stays as it is.

With all eight rows open, by that option or by the four names, the port
adds a ninth row of its own, **Teleport to world** (2026-10-23, added to see
world 1). Return on it lists every world, 0 to 24, by
number and name (world 0, whose name slot reads "Cheat Accepted", as the
Alien Brain map; world 1, "Completed", as the unused prototype), and
Return on one loads it at its first start, its briefing first, as the
frontend's Load does. The world left ends as any world's end ends it, so
what it keeps (a fence knocked down, for one) is kept. It is no
completion: the world left is not marked
done, no results page shows (retail's selection 1 is the campaign's end,
its credits) and none of Skip to's grants are given. Retail has no such
row; without all eight rows open the menu is retail's.

## Finishing the campaign

The first completion sets the cheat word's bit `0x40`, as `DR FRANKENSTEIN`
does. From then on, on that installation, every respawn fits the Cranial
Implanter (Possession missiles, `0x33`), the first spawn of every world
included. The finished profile cannot be loaded again. Nothing counts
playthroughs, so a second completion adds nothing. The bonus belongs to
the installation's settings, not to a save: reset settings, or a save
copied to another installation, start without it.

The port's `completion_bonus` fix, on by default, gives the bonus whenever
the save list holds a finished campaign (`--completion-bonus false`
restores retail's rule). The port cannot finish the campaign yet. The full
account is [the completion bonus](PROFILE_RUNTIME.md#the-completion-bonus).

## Where this comes from

- Grant names and the research tree: the eight-byte records at `0053ABD8`
  (IDs below `0x95`) and the sixteen-byte ones at `0053B088` (three
  dependency WORDs at `+8`), with their H0 texts.
- Furniture grants: the H12 record's lookup selects an H11 row, whose byte
  `+2` is the furniture kind (`00436BC0`). Kind 5 is the Radar.
- The census: every world through `offline_world_retained` (both terrain
  spacings), and Terra Nova's third start with `0x84` known.
- The card table: the only references into `005383BC..005384AC` are the
  carousel draw's.
