# Ultra ground detail: plan

Status (2026-10-24): built. The setting, the surface for play and the
tracer's mesh (about 0.15 ms a 4K frame), and the buildings' footings
(step 5).

A third terrain detail, Ultra (wanted 2026-10-23), past High as High is
past Low: more detailed ground, rough edges smoothed. It is a setting of the traced world (Vulkan
with path tracing), off by default.

## What retail does

- The ground is a 512 by 512 grid of cells, one height byte each, 128
  units apart; the world wraps every 65,536 units. High draws every cell,
  Low every other one (settings `+70`; the world's H14 flag `0x2000`
  forces Low).
- The game's ground height (`TerrainRuntime::height_at`, `00450A30`)
  interpolates on the cell's two triangles, on the detail's grid. Wheels,
  contacts, the camera and placed things all stand on it.
- Each cell's diagonal comes from its texture blend (`prepare_diagonals`,
  `004506A0`: the blend table's `uses_alternate_diagonal` for the cell's
  four materials), not from its heights. The triangles carry no shape of
  their own.
- A network game takes the host's detail: the host's status word carries
  it (bits 24 and 25) and each client rebuilds its terrain to match
  (`network/game.rs`).

## The surface

Each cell becomes a bicubic Hermite patch through retail's four corner
heights, with no twist. At each grid point and along each axis, the slope
is the mean of the two neighbouring segments, unless they turn by more
than the crease angle: then each side keeps its own segment's slope. Hills
and slopes round off, smooth across cells; ridges, cliff edges and the
foot of a cliff stay sharp, with no overshoot. A plane stays a plane.

- Along a shared edge both cells use the same corner heights and slopes,
  so no crack opens, and away from creases the slope across it matches.
- The surface is sampled every 64 units (each cell 2 by 2) into a fine
  height grid, 1,024 a side. The fine cells split along their parent's
  diagonal.
- The game's ground on Ultra is that fine grid, interpolated as retail
  interpolates its own: what is drawn is what the wheels touch.
- A changed cell (a bridge, the ending's flood) redoes the fine points
  within two cells of it.

## Where it applies

- Offline worlds on High, traced. The worlds that force Low keep it, and
  network games keep the host's Low or High, as retail.
- The game reads the saved settings: Ultra Ground and path tracing on, with
  Vulkan chosen. A session log's replay keeps it however it runs. A
  Vulkan that falls back to OpenGL at startup would stand the game on
  Ultra's ground under retail's drawn one; the Video menu offers the row
  only while the world is traced.
- The slope probe (`00450BD0`) keeps retail's cells: it snaps to cell
  centres for a placement decision.
- Ultra changes play, as Low and High do: the lab compares at High; a
  session log keeps the detail it was recorded at.

## The traced mesh

- The ground mesh takes the fine grid: four times the triangles (about two
  million). Each corner's texture place moves from a quad slot (2 bits) to
  quarters of the tile on each axis (3 bits each), so a fine triangle maps
  its part of the tile. The tiles' orientations are the square's turns and
  flips, so the map is the same affine one the raster's two triangles draw.
- Normals come from the fine grid; the baked shade blends between retail's
  points.
- 4 by 4 near the camera is left for later, if the 2 by 2 ground still
  shows its facets close up.

## Buildings on the ground

Retail stands furniture at its cell's height and a placed model at its
origin's, whatever the ground does across it. A census of every world's
buildings (furniture and placed models standing on their base) found gaps
of up to 146 units on Level 1 (its factory over the riverbank) and more
elsewhere; Low and High alike.

Built (`ultra_footings.rs`, `ultra::Footing`): when a world loads, each
building's footing (the hull of its model's lowest corners, turned as it
is drawn) goes to the terrain. On Ultra the ground rises to the base within
64 units of the hull, eases back to its slope over 192, and never falls.
Furniture counts when it is at least 96 across; placed models when the
tuning names them and they stand on the ground. A rise past 256 is left
undone: those are buildings on world 16's walls and cliff edges. Skirts are
not built, since those are the only gaps left.

## Steps

1. The surface and the fine grid (`terrain_runtime/ultra.rs`), with tests:
   retail's heights at the grid points, no cracks, a plane kept, creases
   kept, local updates equal to a rebuild.
2. The setting: the Video menu and the launcher (Vulkan, traced),
   `--ultra-ground`, the session's hook, the session log.
3. The traced mesh and the shader's texture places.
4. Snapshots: hills, ridges, shores and bases before and after, a drive;
   the frame time; OpenGL frames and High's traced frames unchanged.
5. Buildings on the ground.

## Tuning (`modern_tuning.json`)

`"ultra": {"crease": 40.0}`: the crease angle in degrees.
