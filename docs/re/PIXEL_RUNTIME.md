# Infestation renderer consumers

This owns the renderer-level interpretation after the primitive producers and
deferred queue described in [PRIMITIVE_RUNTIME.md](PRIMITIVE_RUNTIME.md).
The near flat-triangle adapter is
[`d3d_flat`](../../crates/inf-game/src/d3d_flat.rs); its CPU target is
[`software_frame`](../../crates/inf-game/src/software_frame.rs).
Shared transformed/lit vertices and native viewport clipping belong to
[`d3d_viewport`](../../crates/inf-game/src/d3d_viewport.rs).
Ordinary Near/Far textured quads use
[`d3d_textured`](../../crates/inf-game/src/d3d_textured.rs), with CPU texture
ownership in [`d3d_texture`](../../crates/inf-game/src/d3d_texture.rs).
Near/Far ordinary triangles, Near/Far shared-shade triangles, Near/Far effect quads
and solid quads have separate recovered
consumers in [`d3d_textured_triangle`](../../crates/inf-game/src/d3d_textured_triangle.rs),
[`d3d_effect`](../../crates/inf-game/src/d3d_effect.rs) and
[`d3d_solid_quad`](../../crates/inf-game/src/d3d_solid_quad.rs).
Running a CPU rasterizer does not select the original game's software renderer:
the original software and Direct3D consumers are different policies.
The [OpenGL device](#opengl-device) draws the same Direct3D submissions on the
GPU; the CPU raster stays the pixel reference.

## Concrete renderer identity

The executable contains these two concrete renderers, which share the base
slots `+08` and `+18..+28` (2D work on the locked back buffer: retail
always draws through the Direct3D one, below):

| Slot | Role (queued callback) | Software `00528FF0` | Direct3D `005290E8` |
| --- | --- | --- | --- |
| `+00` | device byte `+0E` test (software: 0) | `004A47C0` | `004EB480` |
| `+04` | material-sorting gate (software: 0) | `004A47C0` | `004EB490` |
| `+08` | scalar deleting destructor (`004BC9A0`, freed on flag 1) | `004A5C20` | `004A5C20` |
| `+0C` | its argument to `004A3AB0` | `004A4AE0` | `004A4AE0` |
| `+10` | device setter (software: none) | `004A4AF0` | `004BC9B0` |
| `+14` | reset (Direct3D: `0050DC60`) | `004A4B00` | `004BCA20` |
| `+18` | clipped 16-bit span fill (x, y, length, colour) | `004AF040` | `004AF040` |
| `+1C` | a 16-step ramp toward a colour, in the surface's channel layout (`00560A9C`) | `004AF810` | `004AF810` |
| `+20` | a rectangle halved toward a colour (a translucent fill) | `004AF670` | `004AF670` |
| `+24` | clip rectangle `+48..+4E`, clamped to the surface | `004AE870` | `004AE870` |
| `+28` | 16-bit image blit inside the clip rectangle | `004AE900` | `004AE900` |
| `+2C` | 2D sprite | `004AEA50` | `004BCA80` |
| `+30` | 2D line (Direct3D: two lit vertices) | `004AF0C0` | `004EB190` |
| `+34` | Direct3D: `+30` with its argument | `004AF360` | `004EB470` |
| `+38` | Direct3D: `0050D960` on the argument's `+4` | `004AF770` | `004BCA60` |
| `+3C` | near D8 solid triangle (`004A38E0`) | `004AF950` | `004BFAB0` |
| `+40` | far D8 solid triangle (`004A3920`) | `004AF9F0` | `004BFE80` |
| `+44` | near DC triangle (`004A3950`) | `004AFC00` | `004C03E0` |
| `+48` | far DC triangle (`004A3980`) | `004AFE90` | `004C0A30` |
| `+4C` | near flat triangle (`004A39B0`) | `004B0190` | `004C12F0` |
| `+50` | far flat triangle (`004A39E0`) | `004B0430` | `004C1940` |
| `+54` | near shared terrain triangle | `004B0730` | `004C2200` |
| `+58` | far shared terrain triangle | `004B0AE0` | `004C2DF0` |
| `+5C` | near shared-shade textured triangle (`004A38C0`) | `004B1110` | `004C3B60` |
| `+60` | far shared-shade textured triangle (`004A3900`) | `004B1660` | `004C5210` |
| `+64` | near D2 per-corner triangle (`004A3930`) | `004B1D20` | `004C6AB0` |
| `+68` | far D2 per-corner triangle (`004A3960`) | `004B2760` | `004C8160` |
| `+6C` | near UV-only triangle (`004A3990`) | `004B2FD0` | `004C9A00` |
| `+70` | far UV-only triangle (`004A39C0`) | `004B3380` | `004CA7B0` |
| `+74` | near shared-shade UV triangle (`004A3A10`) | `004B39B0` | `004CB6B0` |
| `+78` | far shared-shade UV triangle (`004A3A50`) | `004B3FA0` | `004CCEF0` |
| `+7C` | near textured triangle (`004A38F0`) | `004B4660` | `004CE930` |
| `+80` | far textured triangle (`0044EA50`) | `004B51E0` | `004D0170` |
| `+84` | near solid quad (`00466170`) | `004B5A60` | `004D1BB0` |
| `+88` | far solid quad | `004B5B20` | `004D2170` |
| `+8C` | near E4 shared-shade quad (`004A39A0`) | `004B5D90` | `004D2950` |
| `+90` | far E4 shared-shade quad (`004A39D0`) | `004B6030` | `004D3260` |
| `+94` | near coloured terrain quad (`0044EA80`) | `004B63F0` | `004D3EB0` |
| `+98` | far coloured terrain quad (`0044EA90`) | `004B6750` | `004D47C0` |
| `+9C` | near ordinary textured quad (`00466150`) | `004B6B10` | `004D5410` |
| `+A0` | far ordinary textured quad (`00466160`) | `004B6F20` | `004D6570` |
| `+A4` | near effect quad (`0040EF30`) | `004B75F0` | `004D78A0` |
| `+A8` | far effect quad (`00411D50`) | `004B7BB0` | `004D9840` |
| `+AC` | near D6 terrain quad (`0044EA60`) | `004B8330` | `004DBA90` |
| `+B0` | far D6 terrain quad (`0044EA70`) | `004B8FA0` | `004DDA30` |
| `+B4` | near UV-only quad (`004A3A20`) | `004B9910` | `004DFC80` |
| `+B8` | far UV-only quad (`004A3A60`) | `004B9D30` | `004E1000` |
| `+BC` | near shared-shade UV quad (`004A38D0`) | `004BA400` | `004E2550` |
| `+C0` | far shared-shade UV quad (`004A3910`) | `004BAA60` | `004E4710` |
| `+C4` | near per-corner shaded UV quad (`004A3940`) | `004BB1E0` | `004E6B70` |
| `+C8` | far per-corner shaded UV quad (`004A3970`) | `004BBFB0` | `004E8D30` |

The implemented Direct3D near textured-triangle thunk `004A38F0` uses
virtual `+7C` -> `004CE930`; Far `0044EA50` uses `+80` -> `004D0170`.
Solid quad `00466170` uses `+84` -> `004D1BB0`;
effect quads use `+A4` -> `004D78A0` and `+A8` -> `004D9840`. These are independently decoded consumers,
even where their final TL submission shares raster coverage.

The table words and bodies were checked with `objdump -s` and
`objdump -d -Mintel` against the local `Infest.exe`. A deferred thunk alone
cannot establish shading, sampling, clipping, or depth behavior. The diagnostic
viewer is not the authority for these runtime consumers.

## Renderer selection

Retail always draws through the Direct3D consumers. Settings `+38` is the
registry DWORD `Renderer` (value names at `00526C50`). No Options or Video row
edits it; its default is 1 (`00538D08`), Options Defaults copies 1, and
startup `004631E0` rewrites a stored 0 to 1 (`004632A7..004632B4`). The
display descriptor builder `00437CA0` copies it to descriptor `+0D`, beside
the bilinear (`+3C`) and fullscreen (`+40`) settings. The renderer factory
`004EB4B0` (called at `004956FF`) then switches on the descriptor kind:

- 1 builds the Direct3D renderer: base constructor `004A49B0`, then vtable
  `005290E8`;
- 0 builds the software renderer (vtable `00528FF0`), and only on a 16-bit
  display (`004EB546`, error `00528FE0` otherwise);
- any other kind raises error `00528FE8`.

Its caller frees a renderer through `004AE860`, which calls a non-null
renderer's `+08` destructor with flag 1 (at `00495706`, `0049573E` and
`004957AC`).

So the software column above is unreachable in a retail run. The Direct3D
renderer queries `IDirect3D2` (`004F87F9`) and enumerates devices
(`004F8977`, callback `004F8570`). The callback admits devices with the RGB
colour model and a fixed set of triangle capabilities, and prefers a hardware
(HAL) device over Direct3D's own software emulation; renderer `+18C` records
that a device was found and `+18D` that it is hardware (read at `0050B422`).
Without a hardware device, retail therefore runs the same Direct3D consumers
on Direct3D's emulation, never on its own software renderer. The HAL, RGB, MMX
and Ramp device GUIDs at `00523518..00523548` are not referenced.

The port leaves this dead layer out
(objective).
Its 145 functions are those only the software column reaches: starting from
that vtable's slots the Direct3D one doesn't share, every function reached
through direct calls, code references or the data tables that code names
(the span rows at `00543E00` and `00544280`, through the pointers `00544700`
and `00544704`), less any that live code or a live table also reaches
(`dead_software_renderer.py`). The
progress report marks them `not-needed`
([omitted](../progress/README.md#omitted)).

## OpenGL device

The port draws every recovered consumer through one of three devices, chosen
at startup. The launcher's Options save the choice in the settings sidecar,
and `--renderer` replaces it for one run:

| Choice | Device |
| --- | --- |
| `auto` (default) | OpenGL; the CPU raster when OpenGL cannot start |
| `opengl` | OpenGL only; startup fails without it |
| `vulkan` | Vulkan; OpenGL, then the CPU raster, when it cannot start |
| `software` | The CPU raster |

This mirrors retail: Direct3D with a fallback, where the port's fallback is
its CPU reproduction of the Direct3D rules. Both devices receive identical
work. The consumers in this document read, prepare and validate each packet,
apply the native viewport clipping (`004BCCE0`), validate the inherited
pipeline and sampling state, and resolve the same textures. Only coverage,
sampling and blending move to the device.

- The CPU raster ([`software_frame`](../../crates/inf-game/src/software_frame.rs))
  is the pixel reference: identical frames on every machine. Use
  `--renderer software` for snapshots, native receipts, retail comparisons
  and decompilation checks. Library tests always use it.
- The [OpenGL device](../../crates/inf-game/src/gl_device.rs) draws the
  clipped TL fans with OpenGL 2.1 into an offscreen framebuffer of the
  presented size, which is the native size for authored output, and
  presentation copies that frame into the window (see
  [rendering above native resolution](PRESENTATION_POLICY.md#rendering-above-native-resolution)).
  It keeps the CPU rules where
  GL allows: pixel centres at integer coordinates (a +0.5 shift), colour, fog
  and depth interpolated linearly in screen space, perspective UV divided by
  the interpolated reciprocal depth, LESSEQUAL depth from the fragment so no
  TL depth is clipped, the same four blends, and point or linear sampling
  with wrap or clamp over the same bordered 256x256 surfaces. Device textures
  are keyed by their uploaded pixels, so a later renderer epoch or loading
  loan reuses them without another upload.

The remaining differences belong to the device, not the game: modulation
rounding (one level), bilinear weights (up to three levels), coverage ties on
shared edges (driver-dependent), and texels outside an upload's border, which
stop the CPU raster but read zero padding on the device. The device does not
count fragments.

Checked on 2026-10-09 with an NVIDIA RTX 5080 (OpenGL 4.6, driver 617.14):

- Device tests draw identical flat and textured fans into both devices: flat
  coverage matched exactly, with no pixel more than 3 levels apart; point
  sampling differed by at most 1 level and bilinear by at most 3, across
  copy, additive, inverse-source-colour and source-alpha blends, vertex fog,
  and affine and perspective UV. Clears and surface replacements are exact.
- Whole frames rendered by the game with each device, at equal game state:
  the startup screen, the Options menu after a native Video reset to 800x600
  (720p output), the Level 1 briefing, and Level 1 driving (frame 2110 of
  `--mission 18`, at 640x480 and at 1024x768) each had at most 3 pixels more
  than 2 levels apart. The engine intro (frame 700 of New Game) had 32, three
  of them beyond 8 levels: pixels on shared edges that the GPU gave to the
  neighbouring polygon, and stacked additive fire.

```sh
cargo test --release -p inf-game --lib -- --ignored software_frame::device_tests frontend_app::display::tests::opengl_output
cargo test --release -p inf-game --test frontend_app_window opengl_renderer_follows -- --ignored
```

## Vulkan device

The [Vulkan device](../../crates/inf-game/src/vk_device.rs) takes the same
clipped TL fans and render states as the OpenGL one, through the shared
[frame device](../../crates/inf-game/src/frame_device.rs), which also builds
both devices' vertices. It draws with Vulkan 1.3 (dynamic rendering,
synchronization2) into an offscreen colour and depth target of the presented
size, with the OpenGL device's shader rules as GLSL compiled to SPIR-V
(`crates/inf-game/shaders`, rebuilt by `scripts/compile-shaders.ps1`; the
port embeds the SPIR-V, so building needs no Vulkan SDK). Vulkan's
framebuffer row 0 is Direct3D's top row, so neither readback nor
presentation flips. Presentation draws the target into an output image of
the window's size with black bars; the swap copies the output image to a
FIFO swapchain. A hidden window has no swapchain and keeps the output image
alone, which captures read either way. A command buffer records a frame
and is submitted at the swap, which does not wait for it: two frames are in
flight, the next recorded while the GPU finishes the last, and readbacks
wait for both ([performance](../PERFORMANCE.md#after-the-fixes-2026-10-26)). It exists
to carry the upgraded look ([the plan](../INFESTATION-RT-IMPLEMENTATION.md));
until then it draws what OpenGL draws.

Checked on 2026-10-22 with the same RTX 5080 (Vulkan 1.4.351, driver
617.14), the validation layer on: the OpenGL device tests' scenes match the
CPU reference within the same bounds on Vulkan, a swapchain on a hidden
window presents and reads back its output, and Terra Nova's start at
3840x2160 after 400 frames had no pixel more than 8 levels from OpenGL's.
Debug builds turn the validation layer on when the Vulkan SDK provides it;
`INF_VULKAN_VALIDATION=1` does in release builds.

```sh
cargo test -p inf-game --lib -- --ignored --test-threads=1 software_frame::device_tests::vulkan_
```

### Modern mode: the traced world

On a Vulkan device that traces rays, with the player's Ray Tracing on, the
world draw collects a [scene feed](../../crates/inf-game/src/scene_feed.rs)
beside its own draw and the device traces it
([the plan](../INFESTATION-RT-IMPLEMENTATION.md)). The game's draw
still runs in full: drawing consumes the world's random numbers, so skipping
any of it would change the game.

- **Positions.** Model slots in the projection range never get a world
  position (`model_vertices.rs`), so `ModelVertices` keeps a non-native side
  table of each slot's view-space point, filled where every projection
  passes (`project_with_depth_bias`) and carried through slot copies and
  stack residue like the projected tail. The view frame's transpose turns a
  point into an eye-relative world position, which the 16-bit world wrap
  never reaches.
- **Normals.** `ModelShades` keeps the world normal each shade slot was lit
  along (`004901B0`'s rotation), so traced models shade smoothly where
  retail's vertex shading did.
- **The tap** ([`scene_capture`](../../crates/inf-game/src/scene_capture.rs))
  runs once per primitive instruction, before preparation, sorting and
  clipping: triangles, quads and fans with their sprite or flat material,
  backfaces and off-screen parts included. The 1A/1B billboards, blended
  flats and sprites whose variant blends (flags `0x38`) stay on the
  raster. Unlit primitives (D8, DA, E0, E2) are
  emissive. The sky's backdrop (H8 `0x116`) stays on the raster, which draws
  it before the world; the composite leaves the pixels where the tracer met
  nothing to it, so retail's painted skies stay. The models' built-in
  ground shadows are taken by nobody, since the tracer casts its own. These
  are darkening sprites (H3 flag `0x20`) laid on vertices that opcode 2C (the
  vehicles', with its depth bias) or 2B (round blobs) dropped onto the
  ground, plus any primitive with a 2C corner. `ModelVertices` remembers
  which slots 2B projected, as it remembers 2C's biased ones.
- **Edges** (opcode 07, `model_primitives/edge.rs`) are rods drawn in
  screen space: retail widens the line between two points into a strip of
  its sprite, `parameter` world units to each side at their mean depth,
  the sprite's width along the line and its ends past the points by a cap
  of `height / (2 (width - height))` of the length. The markers' legs and
  cups, the vehicles' axles and parts of the warpgate are edges, and the
  raster drew them over the traced world, unlit by it (2026-10-22). The
  tap now stands the same strip in the world, facing the eye, its masked
  texels open, as two faces meeting along the line whose normals turn
  from one side through the eye to the other (`SceneFeedBuilder::ribbon`),
  so the tracer lights it as a rod and it casts shadows. A tube of that
  radius came out far too thick: the sprite's opaque part, tapering for
  the legs, is narrower than the strip.
- **Masked sprites** (H3 variant flag 1, whose index 0 or word 0 texels
  upload transparent) go into a bottom level of their own, which rays test
  rather than take: a texel of alpha below one half lets them through. The
  warpgate's light panels drew black where the tracer took them as solid
  (2026-10-22). The depth the raster's later draws meet is the ray's own
  hit, through keyed texels. The raster sorts masked sprites with its
  blended draws, back to front, so a glow behind one shows through its
  keyed texels: keeping the masked sprite's depth there cut the markers'
  flares into rectangles around their edge strips.
- **Faces drawn twice.** Some models draw a face twice over the same
  corners in different sprites (the hovercraft's `scatfan` faces 93 to 96
  in sprite 89 over 110 to 113 in sprite 78, `hoverjet2`, the silos). The
  raster's depth test keeps the one its queue draws last; a ray took either
  from frame to frame, and the pods flickered black. Each fed primitive
  takes its queue key and submission at `submit` (`model_commands.rs`), and
  before the trace the feed keeps, of the faces over the same corners, the
  one the queue draws last (`SceneFeedBuilder::settle_faces`): it replays a
  sorted group by descending signed key, equal keys in submission order.
  The others' triangles shrink to nothing.
- **Round parts** (2026-10-24,
  [`round_parts`](../../crates/inf-game/src/round_parts.rs)). The tuning's
  `round` names models that are round about one of their own axes: the
  ACAM's wheels, `huskywheel*` (7 sides, radius 32) and
  `bigballwheel*` (8 sides, radius 50), about X; the hovercraft's ducts
  and the skimmer's jet about Z. Each of their faces goes
  into the feed split onto the true circle round the axis its own draw
  brings, so their outlines and shadows are round.
  - **Rim edges.** An edge whose ends stand at one height along the axis
    (within 1.5) and one distance from it (within 10%), under 75° apart,
    is cut into pieces whose points lie on the circle. Every other edge
    stays straight. A face with two opposite rim edges (the tread, the
    side wall) becomes a strip, one with a corner off its rim edge (the
    hub's cone) a fan from that corner, and one whose every edge is a rim
    edge (the inner disc) a fan from its middle.
  - **How finely.** Each piece lies within half a pixel of the circle at
    the size the trace is shown: in the chase view at 4K the buggy's rim
    edges take 5 pieces and the truck's 6, and far wheels stay as they
    are. An edge is cut from its ends alone, by both faces on it alike,
    so no cracks open, during a form change's growing wheels too.
  - **Shading.** A new corner's normal keeps its ends' lean along the axis
    and their outward part. Its shade is retail's own formula at that
    normal (`shade_world`, `004901B0`'s second half) under the model's
    light; retail's corners keep theirs. The hubs' painted bolt rings
    turn into circles.
  - **Cost.** About 0.05 ms more a 4K frame. OpenGL and the software
    renderer never see the split.
  - **Lugs** (the same day, the tuning's `round.treads`). The tread faces
    (sprite 195) paint four lugs a facet: columns of means 27, 13, 16, 38,
    46, 74, 76, 76, 57, a groove rising to a lit top. On a wheel more than
    40 pixels in radius on screen, they become geometry:
    - **Which points.** Every corner on a ring the tread faces use (their
      slots, from the model's program, mirror twins included) sinks into
      its groove by `depth` (2) times one less the sprite's brightness at
      its texture u. The brightness is read between column middles,
      repeating, so u 0 and 1 sink alike.
    - **Cuts.** A rim edge between two such corners is cut 6 times a lug,
      24 a facet.
    - **Watertight.** Every face on such a ring (the side walls, the
      truck's shoulders and hub) runs u the same way along it, so all
      faces agree on every point (a test holds it for H8 133 to 136).
    - **Normals.** They tilt along the rim with the groove's slope, and
      the shades are retail's formula at them.
    - **Result.** The lug tops stay at retail's radius, where the game
      puts the ground. The tyres' outlines now have teeth, at no
      measurable cost.
  - **The ducts and the jet** (the same day). `scatfan` (7 sides, radius
    18 to 21), `mainjet` and the settled skimmer's own jet in `jetmode2`
    (5 sides, radius 10 to 18, round a point at y 9 and 13).
    - **Windows.** A part inside a larger model keeps its rims within a
      radius and a span along its axis (`jetmode2`'s: radius 12 to 18.5,
      -125 to -45 along Z).
    - **Flat-lit faces.** A corner whose normal leans round the axis
      carries its face's one normal, as retail lit the ducts' insides and
      the jet. On a split face it takes the round normal at its own angle.
    - **Cap pieces.** A face of rim edges and a chord (the duct's front
      cap, 3 rims and a chord), all at one height, fans from its own middle.
    - **Retail's one folded quad** (every model's quads read): the duct's
      grey underside, slots 20, 21, 29, 21 lit from 44, 45, 45, 52, repeats
      its second corner for its fourth. It covers half of the black face
      drawn over the same corners and slot 28 (28, 20, 21, 29); its three
      sibling grey faces each stand on their black face's corners, one turn
      round. In the trace the two speckled where they overlap, and once the
      black face rounded, the flat grey half stood inside the duct. The
      feed gives it slot 28, lit from 44 as the face beside it lights that
      corner, so it pairs with its black face as the others do
      (`scene_capture.rs`, `unfold_duct`); the raster draws retail's.
    - **The jet's mouth** (radius 10 to 11) stays a pentagon: its blue glow
      (sprite 599) is additive, so the raster draws it over the trace in
      retail's shape, and a round mouth left black slivers round it. Both
      jets round from radius 12.
    - **Cost.** About 0.04 ms more a 4K frame (the hovercraft turning).
  - **Spheres** (the same day, `"axis": "sphere"`, `split_sphere`): the
    menu's planets (`titleplanet*`), under the traced menu, where only a
    sphere rounds. A face whose corners stand at one distance from the
    centre is cut as a fan of triangles from its first corner. Each
    triangle halves an edge that turns wider round the centre than a piece
    may at its size on screen (`pieces`' rule), and recurses, at most
    `most` pieces an edge. Each new point is pushed out to the sphere at
    its edge's mean distance, and every corner faces out from the centre.
    An edge is halved from its ends alone, so the faces on it agree. About
    0.24 ms more a 4K menu frame.
- **The ground** ([`scene_terrain`](../../crates/inf-game/src/scene_terrain.rs))
  is the whole 512 by 512 grid as one mesh, built from the cells rather
  than the drawn strips, and rebuilt only when they change (bridges, the
  ending's flood). It copies the raster's tiles exactly: corners
  `(x0,z1), (x0,z0), (x1,z0), (x1,z1)` (the strips run towards -Z), the
  blend combo in that order, each corner's quad slot from the orientation
  word, and the split from the `(x0,z0)` cell's bit 11 against the word's
  bit 0. The tracer cuts it into 256 tiles of 4,096 units and places each
  at its copy across the wrap nearest the eye; while puddles stand, a
  finer patch takes the place of the 3 by 3 tiles round the eye
  ([ground detail](../rt-plans/ground-detail.md)). Under a traced world the terrain host marks every ground command
  `scene_fed`; water strips stay on the raster.
  - Each triangle keeps each corner's place in its tile's texture, U and V
    in quarters, six bits a corner (`texture_place`), which the shader
    turns to UVs (2026-10-24; before, a quad slot of two bits).
  - **Ultra ground detail**: on Ultra the mesh
    takes the game's fine grid ([`terrain_runtime::ultra`](../../crates/inf-game/src/terrain_runtime/ultra.rs)),
    a point every 64 units, so 2.1 million triangles. Each tile's fine
    cells split as the game's ground splits the tile (bit 11, `00450A30`'s
    split), and their corners take the tile's texture places blended
    between its corners'. The tiles' orientations are the square's turns
    and flips, so that blend is the affine map the raster's two triangles
    draw. Normals come from the fine grid; the baked shade blends between
    retail's points. About 0.15 ms more a 4K frame.
- **Textures.** The tracer samples each sprite from its own image: the
  page's interior without the one-pixel border, with a mip chain blitted at
  upload. Page mips would blend a sprite with the uninitialised rest of its
  256x256 page. Ray differentials (where the next pixel's rays meet the hit
  triangle's plane) give the sampler its gradients, so distant and grazing
  surfaces filter as a rasterizer's derivatives would. The Filtering setting
  (`+3C`) picks the sampler: Point and Bilinear read the top level only, as
  retail; Trilinear and Anisotropic 2x to 16x use the chain. Feed UVs stay
  over the sprite (0..1).
- **Bounce light.** Its brightness is kept but only half its colour (the
  tuning's `bounce.colour`, 0.5, in the resolve): at full colour the sunlit
  grass cast its green on world 1's shaded stores (2026-10-23). Each pixel
  sends one cosine-weighted ray from its
  surface. Sky it reaches brings the world's ambient (retail's tint shifted
  right by two), so an open surface keeps retail's brightness. A surface it
  meets sends on its albedo times its own light, sun shadow included, and
  glowing surfaces light what faces them. The bounce light replaces retail's
  flat ambient, so crevices, undersides and overhangs darken and neighbours
  tint each other. A temporal pass follows each point back to the last
  frame through its view and the world's wrap, keeping the mean where the
  surface is the same (distance and normal agree), up to 32 frames. Three
  a-trous passes (steps 1, 2 and 4) then filter it, stopping at changes of
  plane or normal. Only the bounce light is filtered; the composite brings
  albedo, direct light, bounce light, the consumers' cap and fog together.
- **GPU time.** Timestamps around the uploads and builds, the trace and the
  composite are read back a frame later; the device prints one line when it
  closes ("Vulkan tracer: N frames at WxH, GPU X ms a frame ...").
- **Water** ([`scene_water`](../../crates/inf-game/src/scene_water.rs)).
  Each frame the water within sight becomes a mesh at the game's own wave
  heights (`00410F60`), on the tiles where retail draws water (any corner's
  ground below the plane), with normals from the heights. It sits in its own
  bottom level and instance, which shadow rays leave out. The tracer mirrors
  the scene (Fresnel, index 1.33) over small travelling ripples, refracts
  to the bottom, which fades into the water's own colour with depth, and
  adds the sun's glint where it is not shadowed. The refracted ray starts
  half a unit back towards the eye (it skips the water itself): from under
  the surface it began inside the bank where the water thins to nothing at
  the shore, met nothing and took the water's deep colour, a dark outline
  round every shore until 2026-10-23. A grid point whose water would lie on
  its ground (ground at the plane, or a trough the game clamps to it) is
  drawn 2 units under it, the normals keeping the game's heights; the two
  had fought pixel by pixel, a speckle in tile-shaped patches on the worlds
  with ground at the plane (23, and 19's map). Retail's water takes no
  dynamic light (its colour is the tint times a shade from the waves'
  slope). The traced water glints with each deposited light within its
  reach, as it does with the sun, so the vehicle's lamps and an explosion
  by the water catch in the ripples. The water's colour is
  retail's: the all-water tile's texture (its last mip, the mean) times the
  tint at the shade of 6 retail gives flat water, so Altair's stays green.
  How far one sees into it and how much it mirrors at the least differ by
  world (2026-10-22, the tuning file's `worlds`). The bottom shows by
  `exp(-(t / 1.5 clarity)^2)` of the refracted ray's length `t` in the
  water (2026-10-25, `water_clear`; it went opaque
  too early): clear over the shallows, closing over the deep, gone by about
  three clarities. The plain `exp(-t / clarity)` before it had already
  hidden two fifths of the bottom half a clarity in; deeper than about
  2.3 clarities the new curve hides more, so world 18's deep river stays
  the mirror chosen. Worlds 2, 4, 7, 8, 10,
  11, 14, 16, 20 and 21 hold their plane below all their ground (no
  water); the others' water textures fall in six families, and each takes
  its liquid's: clear teal water (3, 13; the snow world 23 clearest),
  paler grey water (1, 9, 12, 15, 18), toxic yellow-green (5, 6, 22:
  murky, its surface mirroring a fifth of its surroundings), the red
  liquid of the brain's worlds (0, 19), and lava (17, 24).
- **The shore** (2026-10-23). Flat water cut
  the faceted ground in straight lines, and its reflection and glints ran
  at full strength to that line. Over its first `water.shore` units of
  depth (24) the water's reflection, ripples and glints now rise from
  nothing, the depth taken as the smaller of the bottom the refracted ray
  finds and the depth over the ground drawn bilinear between its grid
  points (whose contours curve within each tile, as retail's shore
  sprites do), plus a slow wander (`wobble`). A thin, broken foam line
  creeps between 1.5 and 3.5 units of depth as small waves lap (`lap`).
  Ground up to `wet` (96) above the water, near it, darkens to `damp`
  (0.75) at the waterline, as does the ground seen under the water, so
  the two meet; the ice, snow and lava worlds take none.
- **Water that varies** (2026-10-22: a large body of water
  looked the same everywhere, a visibly repeating pattern). The game's
  swell is three long sine waves (`00410F60`: about 970, 1,450 and 2,730
  units, up to about 150 high over deep water), and the ripples were four
  fixed ones, so a lake showed the same bands across. Slow value-noise
  fields over the water, wrapping with the world, now vary it. Beyond
  1,500 units from the eye, how much of the swell shows in the normals
  (a fifth to all; the heights stay the game's, which what floats rides).
  Everywhere, how choppy the ripples are, which of two ripple sets leads
  (the second longer and turned) and how far their crests drift. Patches
  are some 700 to 1,400 units across, glassy to choppy.
- **Caustics** (2026-10-22). Where a refracted ray meets a sunlit bottom
  it can see (the water's clarity leaving it at least a twentieth), the
  ripples focus the sunlight on it: the sunbeam enters the water up its
  refracted path, and the bottom takes `1 / (1 + depth x curvature)`
  of the ripples' height there, capped at 2.2 times, crests gathering
  light and troughs spreading it. That focusing is 2.4 times what water's
  index gives (the ripples are gentle for their depth, and the pattern
  hardly showed; at four times, capped at 3, a sixth of a sunlit bed sat
  at the cap, the golden cells that read as tiles, 2026-10-23), and it
  fades out between 250 and 800 units deep, as
  the sun's share of the bottom's light. The worlds' shores drop steeply,
  so the caustics show in the shallow bands along them (world 3's ice
  shores), not in broad lagoons, which none has.
- **Lava** (2026-10-22; rebuilt 2026-10-27). Where the world's water tile
  is lava (Shak'tahh, 17, the sea round its map and pits within; Caradion,
  24, a deep ring of trenches), the tuning's `water.glow` marks it. The melt
  is light, its colour a blackbody's run whose middle is the retail tile's
  own hue (`lava_ramp`), so it stays recognisably retail's red-orange. On it
  float basalt plates (Voronoi cells about 182 units across, ragged and
  bent, a third of their cracks sealed so plates join): welded against the
  banks, crowding the shallows, drifting rafts over the deep, as the game's
  plane less the ground under a point sets (`lava_cover`). Each plate
  decides at its own seed whether it floats; one drifting off its raft or
  over deeper melt melts in from its edges (`lava_inset`) rather than
  vanishing, and a sealed seam whose neighbour has melted opens. The plates are
  rock, lit as the ground is (sun with its shadow, sky, retail's lights,
  headlamps, lightning), their cracks glowing and breathing open and shut,
  their edges glowing into them. They stand five units proud of the melt,
  so a crack seen at a slant shows its far wall, not its floor. Open melt
  wears a thin dark red skin whose cells stretch along the flow; it cracks
  where the flow pulls it and is gone where it runs hottest. The melt
  crawls faster than the plates, slow swells shape a faint glassy sheen,
  and now and then an upwelling swells, bursts and spreads fresh melt,
  pushing the plates aside. Detail finer than a pixel gives way to its
  average (box-filtered cracks, plates fading to their share), so the far
  lake neither shimmers nor darkens. The lake lights its surroundings
  through the bounce rays that meet it (`lava_glow`, at `LAVA_LIGHT`, its
  colour kept whole rather than greyed as other bounce light is); the old
  separate area light counted it twice and cost two shadow rays a pixel
  (the trace stage at 4K fell from about 25 to 9 ms). Thermal vision shows
  the melt hotter than the crust; blasts in it throw up no water.
  Retail never lit the ground under its opaque water (a baked shade of 0),
  so those vertices take the dry ground's typical albedo. Under a traced
  world the raster leaves its water strips to the tracer. Past the mesh's
  reach (the fog's end) the water is its plane: a primary ray from above
  that meets ground under the water's level crossed the plane there. The
  traced water writes its surface's depth, so the raster's sprites below it
  (an explosion under the water) stay hidden, where retail's additive water
  shows them; deep water there hides its bottom too. The pick-ups' additive
  sprites are the exception (next). From below, the F12
  view under Altair's deep water shows a murky world, as retail's yellow
  fog does.
- **Sunk pick-ups** (2026-10-25). A token
  on a river's bed (world 18 has two, 72 and 83 under its plane; world
  15 one) vanished under the traced water: its additive sprites stayed
  the raster's, whose blend failed the water's depth. With the eye above
  the water, an additive face or billboard (1A, 1B) whose middle lies
  more than a unit under the plane is now the tracer's glow
  (`SceneSurface::Glow`, its own bottom level and mask 0x10, which no
  other ray looks for), and the raster leaves that draw. A billboard
  waits for its command's corners and takes the quad's effect colour
  (the token's pulse); one the tracer cannot place stays the raster's.
  Where the view crosses the water's surface, the water adds each glow
  texel the view meets behind it with nothing solid in front, as
  retail's blend adds it, at retail's place on the screen rather than
  where the surface bends the view (`sunk_glow`). A light under murky
  water lights the water over it, so each fades with how deep it lies,
  not with the view's slant through the water: by e every five
  clarities, taking the water's colour with depth, times what the
  surface lets through and the tuning's `water.sunk` (0.65). Retail
  sinks tokens only where the buggy and the truck reach them on the bed,
  so the water over one is shallow: in world 18's river
  (clarity 40) a token keeps two thirds of its light before `sunk`. At
  four clarities it was faint against the dark water; at eight, then
  six, and at all of its light it read too plainly for something under
  water (60-70% was wanted). Tried first, traced as an emissive
  the refracted ray meets: the black around an additive sprite's glow is
  solid texels (alpha 255), so it drew a dark card, and the refracted
  ray's slant through murky water hid the rest.
- **Splashes** ([`process_particles/splashes.rs`](../../crates/inf-game/src/process_particles/splashes.rs)).
  Retail's splashes (`004893A0`'s class `005287B4`, born where debris, a
  projectile or a falling drop meets the water, and by selector 65 as the
  vehicle leaves it fast) stay its sprites on the raster. The feed also reads each live one, pending ones included, without
  touching the pool: where it fell, its scale (`+2C`), and its age. The age
  is its life (`+10`) against the animation table's WORD `+0C` less one,
  which the birth set and the manager counts down by `delta >> 10` a tick.
  The water then shows the nearest of them as rings spreading from where
  each fell as it ages, a crest and a trough bending the reflections and
  fading out, and foam at the centre while it is young (values in the
  tuning file's `water`). Their tilts together are capped at 0.2 at any
  point: several overlapping stacked into a swirl of warped mirror round
  the craft (2026-10-23).
- **Blasts in the water** ([`scene_spray.rs`](../../crates/inf-game/src/scene_spray.rs),
  2026-10-22). Retail's water is translucent: a grenade that sank before
  going off (100 to 1,000 units down, `00455CCE`) shows through it with
  its fireball. The traced water hides what lies under it, so the water
  answers the blast instead. The feed reads the explosions' fireballs from
  the pool (`0048A0F0`'s class `00528914`, types 1 and 2 by their tables
  `0053D4F0` and `0053D270`), each with how far it lies under the water's
  plane where there is water. The tracer keeps an eruption for a blast up to
  1,500 under the surface or 150 above it, from the middle of its
  fireballs, for 4.5 seconds of the game's clock.
  - **Under the surface**: the blast glows up through the water for its
    first second, fading; a dome swells and sinks in its first half second;
    three rings run out one after another; a patch of foam spreads and
    breaks up as it fades. The dome's tilt and the rings fade out over the
    outer two fifths of the eruption's reach (since 2026-10-23); cut off at
    it they left a rim.
  - **The glow** (2026-10-23: hide retail's fireball under the
    water and show a glow). It replaced a flash added over the finished
    water, a flat disc clipped to a pale plateau at any depth. The water
    now scatters the blast's light towards the eye along the refracted
    ray: a ball of light the fireballs' size at the eruption's depth (the
    record carries it), by the closed form of 1 / distance squared along
    the ray, dimmed by the water on the way (twice its clarity), taking the
    water's colour with depth, its brightness soft-clipped below full light
    with its hue kept, and screened into what shows through, so the
    Fresnel term mirrors over it. Retail's own fireball lights cannot
    serve: a particle's positive light amount gives a radius of the amount
    over its height above the ground under it (at least 256), the bed of a
    deep lake, and retail deposits none below a radius of 256, so a blast
    in deep water lights nothing.
  - **Retail's fireballs and blast wave hidden under it** (2026-10-23).
    An explosion's fireball (`00528914`) whose centre lies under the
    water's plane over water is marked as the traced world's, which only a
    traced frame leaves undrawn: its flat sprite was cut by the water's
    surface into a block. Its blast wave, the fading visual's models
    (`exwave*`, blue streaks drawn as edges), hides under the plane too:
    against the white eruption they read as neon (compared side by side;
    chosen by looks). On land both stay retail's.
  - **Above it**: water traced as drops of its own. Twenty-four jets
    thrown up and out, each of 48 or 96 pieces, and 160 clear drops thrown
    highest and widest, all falling under gravity and drawn out along their
    motion. Since 2026-10-23 a jet is a column: its water leaves at speeds
    from the head's down to half of it, so the slower water trails the head
    back to the surface. Its pieces are large blobs at the base (up to 30
    units), smaller towards the head, thinning once past the top of their
    flight, a third of them fine spray (1 to 2.5 units) flung wider; the
    column widens as it flies. It was a ball of like drops round one moving
    point, so the jets flew as clumps of white beads ("rice"). The dome is
    white water on the surface (the foam, full while it swells), not 80
    drops. The primary ray meets them as ellipsoids, each never
    smaller than about a pixel (shown in as many frames as it fills), and
    shades them as spray: the sky mirrored, the sun glinting. The depth
    pass sees them too. Since 2026-10-23 (too big, thick,
    milky-white blobs, worst on the Brain's red liquid) the plumes rise
    520 instead of 800 with 70% of the drops, a clear drop lets what lies
    behind it show 60% of the time (white water 21%), dithered per pixel
    and frame, and spray takes 65% of its liquid's colour (white water
    39%), pulled a quarter of the way to grey where it was half; the water
    an eruption spread stays lighter for its 4.5 seconds (`churn`).
  - **Where it falls**: each jet, and every fourth clear drop, rings the
    water and leaves foam where it lands.
- **Foam** (2026-10-23: agitated water went white but
  felt wrong; the foam should be smaller and leave
  a small wake). It was a flat, unshadowed film mixed over each splash's
  whole ring, so the water round the hovercraft went milky. A splash's foam
  now lies where the water was hit, out to a fifth of its ring, strongest
  young. It shows as a thin, soft froth from two scales of smooth noise
  (9 and 3.5 units), broken into slow patches and never more than 0.6
  opaque. It has no cells: bubbles' walls, as lace, read as a spider web
  (replaced the same day). It lies on the water as a white surface with a quarter of
  the liquid's colour (pink on the Brain's red liquid), lit as the ground
  is: the sun at its angle with a shadow ray (the craft shadows it), the
  clouds' shadows, the sky's light and retail's lights, under the game's
  light cap. The glints stop where it covers the water. Retail's splashes
  keep coming behind a moving craft, so their foam trails it as a short
  wake. A blast's eruption takes the same froth.
  - Values in the tuning file's `water`: `plume` (the height, 800),
    `drops` and `flash`. A first version marched soft balls of droplets as
    a volume; it looked too gaseous, and the drops replaced it.
    Crowns of the same volume over retail's splashes went with it, which
    left the hovercraft's sparkling spray as it was.
- **Sky** (values in the tuning file's `sky`, per world). Retail's painted
  backdrop stays the default.
  - **The painting in reflections.** The raster still draws the backdrop
    (H8 `0x116`, a ring of unlit textured quads about 15,000 units out). The
    feed also keeps its quads as `SceneSurface::Sky`, in a bottom level of
    their own after the water's (instance mask `0x08`). So do the models
    some worlds hang on it, the whole H8 set with the force-near flag
    `0x20` (`stars01`, `spacegradient`, `mist`, `moonstar`, `moongradient`,
    `moon2star`, `commandmountains`, `cloud`), since 2026-10-23: traced as
    world geometry they stood as far walls, and Level 1's cumulus ring
    (`commandmountains`) hid its painted mountains once the rays' reach
    passed it; Tabuchi's and the Brain's skies differed from OpenGL's too.
    The traced menu keeps its own way.
  - **Who sees it.** Primary, diffuse and shadow rays leave the sky out
    (mask `0x07`). A reflection that meets nothing in the world looks for
    it next, and above the ring's open top it takes the clear colour, as
    retail does. Water and glossy metal now mirror the painted clouds
    where they mirrored one colour. A world whose recipe asks for the
  modern sky (`sky.style`, the [sky plan](../rt-plans/sky-per-world.md)'s
  base, 2026-10-23; no world does until its recipe is approved)
  gets the renderer's own (`pt_sky.glsl`):
  - the world's clear colour (H7 `0x72`) overhead gives way to its far
    terrain colour (`0x73`) at the horizon, where the haze ends, unless
    the recipe gives colours; a warm glow low toward the sun for the
    sunset worlds;
  - a forward-scattering glow (Henyey-Greenstein, g 0.76) and a disc the
    sun's radius across, along the world's light direction and in its
    colour; hashed stars where it is dark;
  - volumetric clouds (below).

  The composite draws the whole sky in the sky's pixels at the display's
  size, under the mist and the warpgates' fields, so the upscalers never
  see the drifting clouds; the trace writes the gradient alone where its
  rays meet nothing, for the edges they blend. Reflections look the sky
  up in a cube map with its clouds. The review of 2026-10-22 found the
  first procedural sky (a gradient with a flat noise layer) plainer than
  every painting; it is gone.
  - **The hybrid, 2026-10-22 to 2026-10-23** (first chosen, then rejected). The clouds filled the open sky
    above retail's painting, fading in over its top edge, and seamed
    against it: smooth blobs over painted cumulus, in another colour. They
    are gone. The painted sky shows retail's clear colour above the
    painting again, in the view and in reflections, as OpenGL does. Its
    clouds' drifting shadows stayed for the day; since 2026-10-23 the
    clouds and their shadows come only with a world's modern sky
    ([the sky plan](../rt-plans/sky-per-world.md), now Stage
    2).
  - **The clouds** (`sky.clouds`, per world; `vk_device/sky.rs`,
    `pt_sky_clouds.comp`): a layer between world heights `base` and `top`,
    its density a broad weather field (of which `cover` shows) times a
    profile over its height by `shape` (cumulus, stratus, storm, ash),
    shaped by tiling Perlin-Worley noise and eroded at its edges by finer
    Worley noise, the method of Horizon Zero Dawn's sky. The noise (128 and
    32 voxels a side) is made once and tiles with the world's wrap, the
    clouds `size` units across and drifting `speed` a clock tick. Light:
    Beer-Lambert toward the sun in six steps with a gentler second falloff
    for light scattered many times, two lobes for the silver lining, a
    powder term darkening thin edges facing the sun, and an ambient from
    the shaded colour to the zenith's. Without colours of their own they
    take the world's light near white (`lit`) and the zenith's darker
    (`shade`), as dim as the light is. Far clouds fade into the horizon.
    The view's clouds are marched at a quarter of the display's width and
    height in 64 jittered steps, a sixth of each frame taken into a
    history that follows the camera's turn; the cube map's (six faces of
    128) in 24.
  - **Cloud shadows.** A 512-texel map of the sunlight through the layer
    above each point of its base, 32,768 units across round the eye; a
    point's ray toward the sun reads it where it enters the layer, and
    `shadow` (0.5) of the darkening is taken: on the ground, models and the
    water's glints and caustics, at primary hits and bounces. They drift
    with the clouds. Under the painted sky only the shadows show (cover
    0.45 by default; worlds 14 and 16 have none).
- **Fog** (values in [`modern_tuning.rs`](../../crates/inf-game/src/modern_tuning.rs)).
  Retail fades models and the near ground into the fog colour (H7 `0x74`).
  Beyond the near ground it draws coarse terrain out to 1.5 times the fog's
  end (`0044AA10`'s far strips): fog-coloured, with the cells' relief
  showing again with distance, then fading into H7 `0x73`. The traced
  ground and water take one haze in place of both. It starts at the fog's
  start and thickens until it is complete where retail's far terrain ends,
  and past the fog's end its colour turns from `0x74` to `0x73`. Distant
  mountains therefore stay in sight, lit and shadowed, as retail's far
  terrain keeps them. Models keep retail's fog, complete at the fog's end,
  beyond which nothing is drawn. Since 2026-10-23 that holds only for
  thermal vision (its haze complete at the fog's end) and the briefing's
  pane: a world's view takes the distance fog below.
- **Distance fog** (2026-10-23). Two fogs with
  different curves and colours let what retail starts drawing at the fog's
  end F pop in: on Oolbryte two trilithons appeared on a far ridge 37%
  darker than the sky behind them. In a world's view the mist's grid now
  carries one fog for everything, counted by view depth as retail's is:
  none before retail's start, its optical depth growing as the way to F
  to the power 1.6 (`curve`), to hide 99% at F near the ground
  (`opacity`). Above the world's high ground (the height four fifths of
  its land lies below, so a world of sea keeps its hills in the fog: on
  Stendahl Alpha, 82% water, the grid's own fifth gave the sea's surface,
  2,165 units lower) plus `top` (900) it thins over `band` (1,200) to its
  `above` share (0), so the upper sky stays clear; an eye that climbs past
  that top takes it along half the band below it (`lag`, 2026-10-29), and
  the sky below the horizon, seen past the rays' reach from high up, takes
  the fog's own colour, as the ground past 1.6 F does. Its colour turns from `0x74` to
  `0x73` by F, lit by the sky and the sun as the mist is. The grid ends
  just past F. The traced ground, water, spray and models take no haze or
  vertex fog, except a model whose draw fogs it in a fog of its own (the
  actor flash `0047DEA0`: a hit vehicle's red, marked per triangle); the
  sky takes the whole volume as its rays meet it, so its lowest degrees
  fog over. Past F what still shows fades out by 1.6 F into the fog's own
  colour along the view (the composite), so the world's 65,536-unit wrap
  never shows its repeats. The raster's textured late draws over the
  traced world (sprites, particles, beams, the half-transparent crystals)
  read the same grid at their own depth (`vk_textured_fog.frag`, with the
  composite's set), so they sink into the fog as the world behind them
  does: an additive or darkening draw fades to nothing, the others into
  the fog's lit colour, and past F both fade out as the world does. A CPU
  copy of the curve without the sun's scattering, used until then, left
  Stendahl Alpha's crystals as dark shapes against a fog up to twice as
  bright toward the moon. Past `dissolve` (0.9 F) a
  model's pixel shows what lies behind it as often as it is near F, at
  random each frame, so what rises above the fog fades in. The tracer's
  rays reach a fixed 50,176, four times the largest F, so the skyline no
  longer moves with F; a scene whose own far end lies farther reaches four
  times that (the traced main menu, whose far planets the fixed reach
  dropped). Oolbryte's pop now changes none of its boxes, at
  the same cost. Each world takes its own distance (2026-10-23, the
  user): the land worlds from 1.5 times retail's start with a curve of
  1.9, the water worlds (Stendahl Alpha, world 1) from its start with 1.6,
  the airless ones later, Luna Azine latest and thinnest (98% at F);
  Altair Prime takes the densest fog, a smog. A world's dark profile
  (`worlds.N.fog_dark`, over its own fog) takes over while mission `+13`
  is set, rolling in over six seconds of its timer `+2C`
  (`darkened`, `world_scene.rs`): Artoria's load clears the byte and its
  four topside generators' deaths set it, as they start retail's dark
  tint, so once the city loses its power its fog closes in (2026-10-24,
  so that infrared has a use there: Artoria's fog starts at
  0.45 of retail's start, 99.5% at F, the mist twice as thick).
- **Mist** (`pt_fog_inject.comp`, `pt_fog_integrate.comp`). A grid of 160
  cells across the view and 64 slices deep, each slice a fixed ratio deeper,
  reaching the haze's end, lies over the traced world. Its density thins
  with height above the world's low ground: the height a fifth of the grid
  lies below, the water counting at its surface. Each cell takes the fog
  colour's light from the sky and, where a ray finds the sun, the sun's,
  scattered forward (Henyey-Greenstein), so terrain and buildings cast
  shafts in it. Retail's light grid glows in it near the ground.
  - Each frame lights a jittered point in every cell and blends it into the
    last frame's value where the cell's centre was seen then. It then
    gathers each column from the eye outwards.
  - The composite lays the mist over the traced world. By blending it also
    covers the raster's painted sky, which takes half of it, since retail's
    skies are mostly lighter than its fog colours.
  - There is no mist under infrared vision. It costs about 0.1 ms a frame
    at 4K.
  - **The thrusters' trails** (2026-10-24,
    [`scene_trails.rs`](../../crates/inf-game/src/scene_trails.rs)).
    While a vehicle thrusts (a boost running, `+224`; or Thrust, control
    `+1C`, on the hovercraft, the VTOL or the skimmer), the exhausts the
    tuning's `trails` names on its models (groups of vertex slots, or a
    model's origin) leave puffs a few units apart along their way. The
    puffs add density to the mist's cells, each a soft ball, lit as a grey
    surface is by the sky and the sun where a ray finds it; each thickens
    from clear, grows, rises and thins to nothing within a second. At most
    256 at once; only the cells within the box they fill look at them.
    Who thrusts is read without changing anything, not even the actor
    handles' lookup cache.
- **Metals** (values in the tuning file's `materials`). Each model has a
  metalness and roughness by name, packed beside its heat in the triangle
  words. A metal's bounce ray follows its reflection as often as it is
  metallic, the direction a GGX microfacet's mirror at its roughness. That
  reflection sees the sky's own colour, where diffuse bounces see retail's
  ambient. The rest of the metal's painted diffuse light remains, and the
  sun's highlight is added (GGX, Schlick, against the model's own light
  direction, shadowed). Retail's light cap still bounds the sum, so in the
  bright, mostly one-coloured worlds metals change little. Since 2026-10-24
 the warpgates, the factories, the griffin,
  the hoverbots' wings and the grenades take the ACAM hull's metal too.
- **Thermal vision** (values in `modern_tuning.rs`, camera in
  [`vk_device/thermal.rs`](../../crates/inf-game/src/vk_device/thermal.rs)).
  Under Vulkan, infrared vision is a thermal camera in place of retail's
  green.
  - **Heat.** The trace gives each surface a heat from 0 to 1 instead of
    bounce light: the sky coldest, then water and the ground; each model by
    its name (the feed records the H8 name from the overlay), the first of
    the tuning's patterns that matches. Engines, exhaust, weapons and lamps
    are hot, creatures and people warm, machines near ambient, rocks and
    plants cool. Unlit surfaces glow hot. Sunlight warms what it reaches,
    models half as much, and darker colours a little.
  - **Gain.** One workgroup takes a histogram of the frame's heat each
    frame, the sky left out (the traced depth's 1), puts black and white at
    its low and high shares (`gain`, 2% and 99.5%, 2026-10-23: before, the
    sky's 0 set the black and the ground sat at mid grey) and moves the
    camera's range a share of the way to that. The resolve then shows heat
    in grey, black to white over that range, and a sudden hot thing briefly
    dims the rest.
  - **Camera.** It traces at half the display's size, never through an
    upscaler (2026-10-23: the heat has no noise to reconstruct, and the
    sensor is coarser still); a switch into or out of infrared starts the
    histories afresh. After the world's last draw (`world_scene.rs`, before
    the HUD), the frame comes down to the camera's own 1,280-pixel width
    as grey (800 until 2026-10-23), then back over the view. On the way it
    takes sharpened edges, the image's fine contrast added back (`detail`),
    halos round hot things, the sensor's fixed column and row stripes,
    grain each frame (strongest where it is cold and flat) and a vignette.
  - **Palette** (2026-10-23). The grey shows white hot, black and white
    only. L's step after infrared (with the Headlights owned too,
    [`GAMEPLAY_RUNTIME.md`](GAMEPLAY_RUNTIME.md#infrared-vision)) turns it
    ironbow. An amber tint over body heat and hotter (an isotherm) came
    and went the same day: in infrared black and white mode the hovercraft's thrusters and some lights turned orange, which defeats the mode.
  - **Lights** (2026-10-23). The grid keeps each deposit's own colour
    from before infrared's green (non-native, `TerrainLightSource::own`)
    and marks infrared's lamp on the player cold. A particle's light
    (blasts, hits, fire, muzzle flashes) and a shot's warm what they light
    by that colour's brightest channel, the hottest one at a point counting
    (`lights`, 0.5); lamps warm nothing, so the ACAM does not heat itself.
  - **Heat memory** (2026-10-23, `vk_device/heat_memory.rs`). Those
    particles' lights also gather on the ground, by world cells of 64
    units over half each light's reach, at most 0.4, and halve every
    `memory` seconds (8): scorch marks and warm spots that fade. It
    gathers whether infrared is on or not; the tracer reads the 128 by 128
    cells round the eye behind the light grid's cells.
  - **Heat by object** (2026-10-23, `vk_device/object_heat.rs`). The
    object draw tells the feed which object each face comes from, where it
    stands and its root model, named for the tuning even when it draws only
    children (the ACAM's `acam`, model 117). A model no pattern lists
    takes its object's heat, so a soldier's limbs read as his body. Each
    object also warms by what it does, eased over a third of a second:
    - moving: up to `moving` (0.08) at 1,000 units a second, about the
      buggy's top speed;
    - firing: a bolt or a cannon spark at most 32 units of its life old
      (two ticks; retail's lifetime counts down from its table's) marks
      the nearest object within 500 units as its shooter, and its guns
      (`guns`: barrels and heads, not the towers, struts and mounts under
      them) warm by `fired` (0.3), halving every 6 seconds;
    - dying: an object whose root turns into a wreck or a corpse
      (`wrecks`: `dead*`, `*dead`, `damage*`, the vehicles' wreck
      `damagemedium` being model 0x46) keeps the heat it had over the
      wreck's own, halving every `cooling` seconds (15). A wreck first
      seen as one is only its own heat.
  - **The raster's effects.** Under thermal vision the raster's
    half-transparent and darkening sprites (smoke, dust) draw at half
    strength, as a thermal camera sees through smoke; additive fire and
    blasts stay hot by their brightness.
  - **The raster's other draws stay their own.** Additive effects and
    billboards (opcodes `1A`/`1B`, such as Terra Nova's start beacons) show
    by their brightness: explosions burn hot, and the beacons' tripods,
    sprites in retail too, come out dark.
  - **Without ray tracing** the camera greys retail's infrared image. There,
    brightness is retail's lamp on the player rather than heat, so the near
    ground shows bright; per-surface heat needs the tracer.
- **The frame.** The tracer runs just before the world batch's last flush,
  writing colour and the raster's own depth (`1 - 64/z`, with z taken a
  thousandth farther); that flush then
  replays only the commands the feed did not take (`scene_fed`), so blended
  effects, particles and billboards sort against the traced world. The HUD
  draws afterwards as ever. The slack exists because retail drew a glow,
  decal or particle lying on a surface with the same projection as the
  surface, so `LESSEQUAL` let the later draw win the tie. The traced depth
  only approximates the raster's rounded one, so such overlays flickered
  without it, the factories' blue lamp glows every other frame while the
  camera bobbed (reported as flickering, Tabuchi, 2026-10-22).
- **Models keep retail's light and fog.** Each corner carries the shade the
  producers read, from the shared arena (`scratch_shade`, the mirror pass's
  slot included). A slot the model never lit keeps an earlier model's
  colour, as in retail. The tracer lights with that shade and, where the
  model's sun is shadowed, takes the sun's share away (the light the shades
  were computed with, recorded beside them in `ModelShades`). Far-dispatch
  corners also carry their fog byte and the primitive's far colour: on
  Altair the buggy is about 40% fogged towards red at 600 units, so it
  draws red in retail and in the tracer alike.
- **Fan corners keep their colours** (2026-10-23). A fan's corner record
  carries three colour factors beside its shade slot (its lighting words),
  128 being one, and the raster lights the corner its shade times them
  (`model_primitives/fan.rs`). They paint most fans' colours: 16,004
  corners of 346 models are not neutral, among them the red cross
  (`healthsmall`, H8/168: red 128,0,0 and white 255,255,255). The tap had
  dropped them, so the tracer drew the cross the grey of its material. Now
  factors up to one tint the corner's colour, so the bounce light takes
  the tint too, and past one they brighten its shade; together that is the
  raster's colour times shade times factors. The starts of worlds 2, 9 and
  14 draw the same as before, world 3's differs in 105 pixels.
- **Pick-ups keep most of retail's light** (2026-10-23). Retail lights a
  pick-up (its draw `0046DAF0`, selectors 14 and 89) along the terrain's
  light in a pulsing colour of its own (`00490030` with `[p, 255 - p,
  255 - p]`, p swinging between 66 and 190 with the animation clock, each
  crystal pulsing its own channel), plus the ambient, unshadowed. The tracer had taken that colour
  for the model's sun and shadowed it, so the crosses sank into shadow.
  While a pick-up draws, the tap marks its light with the tuning file's
  `materials.pickups` (0.85), and the tracer keeps that share of the
  pick-up's retail shade as it stands, the rest traced (pick-ups should be
  far less affected by light).
- **Light.** Retail lights a model with the tint along N.L plus an ambient
  of a quarter, and applies a shade of 128 as one; the tracer uses the same
  scale. The ground is lit far more dimly in retail: each cell's baked shade
  (bits 12 to 15) gives the tint times shade/15, about half of N.L plus a
  little (Terra Nova fits 0.52 N.L + 0.07; on Altair most cells are 0). Each
  ground vertex therefore gets the albedo that turns the tracer's full light
  into that shade, so sunlit ground matches retail and shadows darken it.
- **Dynamic lights.** The light grid (`world_lighting.rs`) reaches the
  tracer as the frame's draws left it. Retail's ground vertex colours take
  twice the grid's bytes, in colours whose 255 is one
  (`terrain_draw/strip.rs`), so the traced ground adds about the grid's
  value, where 128 is one, read at the tile's grid points and blended
  across the tile as the vertex colours are, and clamped per channel as
  they are. It added twice that until 2026-10-22, which flooded the ground
  near the vehicle with its rear glow's red. Models add it once to
  the ambient and once to the sun's colour (`object_draw.rs`). Finally the
  consumers' cap applies: a shade above 128 scales the whole colour down by
  its largest channel (`normalized_shade`).
- **Soft shadows** (2026-10-22). A primary hit's shadow ray toward the
  sun (or toward the light its model was shaded with) aims at a random
  point of the sun's disc, `sky.sun.shadow` degrees across each way (2.25
  by default since 2026-10-23, when a degree's shadows read hard and very
  dark; the modern sky's drawn disc keeps `sky.sun.radius`) in the tuning
  file. The hit's light leaves the sun out; the sun
  before its shadow goes to its own image, and the one sample of whether
  it is seen to the bounce sample's alpha. The temporal pass keeps that
  visibility's own history, 12 frames at most and held within what the
  pixel's 3x3 neighbourhood sees this frame, so a moving shadow leaves no
  trail; the first two a-trous passes filter it (steps 1 and 2), and the
  resolve takes the sun times it. Ray Reconstruction takes the raw sample
  and denoises it with the rest. Shadows then soften with the distance
  from what casts them: palms' fronds blur out from their trunks. Bounce
  and reflection rays keep one ray toward the sun's centre.
  - **The terminator** (2026-10-23). Every shadow ray leaves from the
    curved surface the hit triangle's corner normals shade, not its flat
    facet: the point is projected onto each corner's tangent plane and the
    three blended by its barycentrics (Hanika's offset), kept only when it
    lies outward. A slope shaded smooth was shadowed by its own facets in
    hard dark blocks along the terrain's triangles (Pillier Stren's snow,
    Oolbryte); a facet whose corners share its normal moves nothing.
  - **The lights' shadows** (2026-10-23). The glows' and the shots' lights
    (kind 2) cast theirs through four rays to points across 40 units, a
    particle's deposited light through two, at a primary hit, the points
    turned per pixel and frame; a bounce's sampled light keeps one ray.
    A glow's points under the plane of the surface it lights count from
    as far above it (2026-10-24): the VTOL's jets at rest hang about 12
    units over the ground, and the rays toward their points under it
    stopped in the ground, so each jet lit only a hard disc round it.
- **Glows** (2026-10-22). Models the tuning's `materials.glows` names
  light what stands near them: the crystals (`token01` to `token03`, whose
  additive faces stay the raster's) and the yellow orbs (`token04`,
  billboards). The tap gathers each such model's blended faces, or its
  billboards' anchors, into one light where they cluster
  (`SceneFeedBuilder::glow`), with its sprite. The tracer gives it the
  sprite's mean colour, each texel counting by how coloured it is, so a
  crystal white at its core and green at its edges shines green; it rides
  in the light sources as kind 2. Only additive parts give light: a
  model's darkening and half-alpha ones (smoke, shades) are none
  (2026-10-22). A glow keeps the first four of its faces' sprites, with how
  many faces show each, and shines in their mean colours, each sprite's
  by how much light its faces hold, so its colour does not hang on which
  face is drawn first. Its strength follows the sprites drawn (the tokens
  should pulse rather than glow constantly): times how
  much light they hold (each texel's brightest channel times its alpha,
  on average) against a crystal's sprites (0.5). The orbs' core, cycling
  sprites 374 to 381 inside their steady bubble (507), takes their light
  from about a third to full and back about once a second; the crystals'
  sprites hold steady. The vehicles' jets glow too: the VTOL's blue jets
  (`vtol2`) and tail flame (`jettail`) and the skimmer's jets
  (`jetmode2`) light the ground under them, the VTOL's two jets each a
  light of its own, about as bright together as the turbo (2026-10-24). A glow's `gather` is how far apart its
  faces give lights of their own (a quarter of its reach unless the
  tuning says): the jets' flames, 92 units apart, had gathered into one
  light between the pods, which the hull shadowed. The buggy's and truck's
  turbo flames (the additive parts of `wheelmode1a` and `wheelmode2a`,
  drawn only while boosting, when retail's rear glow is out) light the
  ground behind them blue. A glow can name the sprites it comes from: the
  buggy's and truck's come only from the flames' face (sprite 321) and
  streaks (602). Their rear lamps' billboards (585 red, 282, 586 white,
  452) had glowed too, at full strength, and cut a hard-edged red block
  into the ground behind every buggy that stood, braked or coasted
  (2026-10-23, the lighting read as blocky). A primary hit takes each within its
  reach, fading as `(1 - d^2/r^2)^2` and softly with its facing
  (half-Lambert), shadowed by models and the ground short of the glow.
  A tuning entry may carry a colour of its own, used in place of the
  sprite's, and a glowing model's blended edges light too, at their
  middles: the warpgate's (`gatebod` H8/63) lightning, its edges with
  sprites 595 to 597 from the frame's corners to a point in its middle,
  drawn only while the gate is on, and its flares (1B, sprites 321 to
  323), light the gate and the ground round it blue-white. The power
  transformer (`util04` H8/55, terrain furniture of kind 22) lights its
  pylon and the ground round it orange from its core, one additive edge
  of sprite 342 from 108 to 282 units up its middle, in the tuning's own
  colour (2026-10-23).
- **The guns' shots as lights** (2026-10-22). Retail's shots light
  nothing: the bolts (`00466710`'s class `00526EFC`, the plasma's among
  them) draw animation tables whose frames all hold no light (`00539D28` to
  `00539D78`, and `00539DF8`'s light colour comes with a zero amount), and
  the laser (selector 31) draws its beam in screen space
  ([`SELECTOR31_RUNTIME.md`](SELECTOR31_RUNTIME.md#draw)). Under the tracer
  each bolt in flight, read from the pool with the sprite its frame shows,
  becomes a glow light of its own in that sprite's colour, and each laser
  beam four lights along its aim past the muzzle (the
  gun would shadow a light there), at the tuning's `materials.shots`
  strength and reach. The cannon's shot draws nothing (selector 32); its
  sparks, `0048A0F0`'s class at type 5 (table `0053D2B0`), light a little
  in their sprite's colour at `materials.sparks` (2026-10-23: not as much as
  plasmas, but a little), which glints in
  the water as the bolts' lights do. The flamethrower, the missiles and the grenades
  already light their surroundings through retail's own deposits
  ([`OBJECT_LIGHTING.md`](OBJECT_LIGHTING.md)). The bolts gathered into one
  light when near until 2026-10-23 (in the water only one
  central beam reflected, but the shots come from 2 opposing sides): Dual
  fire plasmas' two bolts fly side by side, 136 apart (the guns at -70 and
  +66), within a quarter of a plasma bolt's 700 reach, so each pair lit and
  glinted from its middle as one streak.
- **The warpgate's field** ([`scene_field.rs`](../../crates/inf-game/src/scene_field.rs),
  2026-10-22). While a model the tuning's `materials.fields` names draws
  blended edges, the tap keeps their ends. Where most of them meet is the
  field's middle; the far ends trace the opening's frame, and the field is
  the ellipse inscribed in their extent, in their plane, level across and
  up as near the world's up as the plane allows. The composite lays the
  nearest two over the world and the sky, in front of what the depth image
  holds there, under the raster's arcs: thin bright filaments swirling
  round a bright middle, faster nearer it, fading to its edge, in the
  tuning's colour. Off by default since 2026-10-23, with the gate's arc
  lights: it veiled the opening and pumped with the hub's jitter, so the
  gate kept retail's look until the redraw below. The tuning can turn both
  back on.
- **The warpgate's lightning, redrawn** ([`scene_gate.rs`](../../crates/inf-game/src/scene_gate.rs),
  2026-10-24, [the warpgate plan](../rt-plans/warpgate-effect.md)'s
  option b). For a model the tuning's `materials.gates` names (`gatebod`),
  the tap keeps each blended edge's ends, half-width and sprite (595 to
  597) as a bolt, and each flare's (1B: 321 to 323) quad, where its command
  lands, as a glint. Both are fed, so the raster skips retail's arcs and
  flares, but only while the world is traced; the gate's program still runs
  every instruction and draws the game's random numbers. Right after the
  traced world, the raster adds each bolt over it, depth-tested against
  it, so no upscaler smears them:
  - a jagged line between retail's ends, new each tick (midpoint
    displacement over 32 segments from a hash of the clock: the first step
    `jag` of its length, each finer one 0.7 of the last), flickering a
    little;
  - a core and a soft halo across it, untextured additive strips in the
    sprite's mean colour, the halo narrowing towards the ends, and now and
    then a short fork fading to its tip;
  - each glint a disc in its flare's colour (the hub keeps retail's blue,
    yellow and green), a small hot middle and four fine rays, turned each
    tick.
  One steady light stands at the opening's middle while the spokes show:
  the mean of the far ends of the bolts that meet at one hub (the ring's
  corners stand still while retail jitters the hub), at `light` (0.5 over
  600 units, about 0.35 ms of trace at 4K). OpenGL's frames and the game's
  event log stay the same, byte for byte.
- **Shadowed lights** (`TerrainLightSource`). The grid also keeps, as port
  bookkeeping, each deposit since its clear that reached its cells: where,
  how far, and its colour as the cells took it. Particles' deposits
  (explosions, projectiles' glows, through `WorldParticleLighting`) are
  marked as such. The feed places each a little above the ground under its
  X/Z, since the grid holds no height, with where it shines from: the hull
  for the vehicle's lamps (the headlamp lights the ground far ahead of it),
  the particle for a particle's, at least 64 above the ground there.
  - **On the ground.** A primary ray takes the ground's light from the
    nearest 64 sources instead of the grid, each as retail deposits it (its
    colour times `1 - d^2/r^2` in X/Z, the sum within the grid's signed
    bytes), with a shadow ray that stops 160 units short of the source. A
    faint contribution (below 0.08, a debris spark's or a light's far edge)
    goes without its ray, since explosions shed dozens.
  - **What may shadow.** A particle's light is shadowed by anything, on
    the way to the particle. Nothing shadows an actor's light (a vehicle's
    lamps, a beacon; 2026-10-22). Shadowed by models, the rear glow cast
    the whole vehicle's shadow across the ground ahead. Shadowed by the
    ground, the headlamp's pool broke into black facets on Foldrak Noir's
    hills and in Artoria's trenches, from the hull as from above its spot:
    retail's grid light has no direction, so every facet and bulge facing
    away from a low lamp shadowed itself at full strength.
  - **The headlamps** (2026-10-24). A traced
    frame leaves the headlamp's pool out of the grid: the light pass keeps
    each vehicle with its headlights on (`VehicleLamp`: its hull and basis)
    instead of depositing it, whether or not retail's ray found ground, so
    every reader of the grid in the trace is free of it. The feed sets two
    lamps on each nose (`place_lamps`, model (±13, -17, 104), the nearest
    four vehicles), aimed down the nose turned down 4°. A primary hit takes
    each lamp's beam (`lamp_beam`, `pt_frame.glsl`): full across the inner
    45% of its 50° spread and up to 8° above the axis, fading over 12°
    more. A slope or wall ahead takes it some way up, as real light climbs
    it. With the first beam's top edge level with the lamps (4° above,
    fading over 3°), the pool stopped at a hill's foot, where retail's grey
    disc lights the hill's face (2026-10-24; the beam was also narrower than retail's disc). Below the
    axis it thins over 8° as a low beam does, so the ground just ahead,
    near and so lit the most by the inverse-square falloff, does not
    outshine the ground 1,000 to 2,000 units out. The ground's facing wraps (0.6), as rough snow
    and soil send a low lamp's light back toward a viewer riding near it.
    One shadow ray a lamp; the ground's leaves 4 units up its normal, and
    where the ground's flat face is nearly edge-on to the lamp facing alone
    decides, so the black facets of the shadowed pool do not return.
    OpenGL, the software renderer and untraced Vulkan deposit the pool as
    retail.
    - **On the water** (the plan's step 2). The water's refracted and
      reflected rays take the lamps at what they meet, so the lit beach
      goes on under the shallows (the pool stopped in a hard line at the
      waterline) and the lit shore mirrors. The beam's light entering the
      water (`1 - Fresnel` at its angle, wrapped as on the ground) comes back
      in the water's own colour, `water_scatter` of it, as much as the water
      is deep against its clarity: murky water glows near its surface. Its
      ripples glint in the lamps' white, a GGX lobe of roughness
      `water_gloss` (0.15), shadowed by the same ray; foam and spray take
      the lamps as white surfaces.
    - **In the air** (step 3). The mist's gathering pass adds, slice by
      slice, the beam scattered toward the eye (`air`, 0.3; as much from
      behind as ahead, so the chase camera sees it), through the world's
      mist or, where that is thinner, a least air of the beam's own
      (`air_floor`, 0.03 a thousand units) that dims nothing behind it.
      The beam there is capped at 1.5 times its light 1,000 units out: the
      falloff made it a blot at the nose. It is drawn fresh each frame, so
      a turn leaves no smear; nothing shadows it in the air.
    - **Under the water** (item 35). A lamp under the water's surface
      lights the water itself: scattered back in the water's own colour
      (its texture's mean), absorbed by its clarity on its way from the
      lamp and on the way back, `water_glow` (3) strong. The camera stays
      above the water while the vehicle is under it, and the water's
      surface is the pixel's hit, so the glow is gathered along each
      refracted ray at eight points (`lamps_in_water`); with the eye under
      the water the mist's gathering takes it along the view instead.
    - **Bounces, lenses and glare** (step 4). A bounce ray's hit, and a
      metal's reflection, take one lamp picked by its share of their light
      there, shadowed with one ray, as they take one of retail's lights.
      Each lamp has a lens, a small unlit white disc (`lens`, 4 units) a
      unit behind where its light starts, so it never shadows it. Where a
      lamp faces the eye (within its spread), a small star in its colour
      glares at it over the traced world (`glare`, 0.5, as strong as the eye
      stands within the beam).
    - **The crystals** (step 5). The scenery crystals (`xshard*`,
      `xclump*`: terrain furniture, half-transparent, so the raster's)
      catch the light in a traced frame. The lamps are placed as soon as
      the light pass has run, before the furniture draws; for each crystal
      the CPU's twin of the beam (`lamp_beam` in `world_scene.rs`) is taken
      at its middle and near its foot (a low beam often reaches only a tall
      crystal's lower half), unless the ground stands between (seven
      points along the line). Retail's lights at its cell (blasts, muzzle
      flashes) and the shots in flight near it count too. It then glows by
      its ambient in its world's colour (Terra Nova white, Stendahl Alpha
      blue-ish; others the world's light), easing toward `glow` (0.6) as
      the light grows; the brightest lamp becomes its directional light over
      its draw, so the facets turned to it flash white (`flash`, 0.5); and a
      glow light in its colour tints what stands round it (`reach`, 500).
    - **The rear lamps** (2026-10-24). The rear glow leaves traced frames
      as the pool does. The light pass keeps which of the buggy's or
      truck's rear lamps retail's rear glow lights (`RearLamps`): the red
      pair in gear 1 (standing, braking or coasting) and the white pair in
      gear 0 (reversing). None are lit driving forward (gear 2) or in first
      person. The feed sets the lit pair at the billboards that draw them,
      2 units behind them: model (±17, -6, -81) for the red,
      (±10, -6, -81) for the white.
      - **Aim.** The lamps sit under the sloping back, which rises from
        (0..17, -16, -69) to (0..17, 4, -92), facing down and back. So
        they aim straight back turned down 45°.
      - **Beam.** It is the headlamps' with numbers of its own
        (`headlight.rear`): a 70° spread, its top 30° above the axis,
        thinning over 45° below it, eased over 300 units and ending at
        800, 0.025 strong.
      - **Shadows.** The rear pods stand 11 units behind the lamps and 8
        to 15 beside them. From a point, the light was cut into a
        hard-edged tongue, so the lamps cast their shadows across 12 units
        instead: two rays, turned per point and frame.
      - **Strength.** Brighter, the red pushed the ground's light past
        full. The brightest-channel cap (retail's `normalized_shade`) then
        dimmed the moonlit ground's teal into a brown blotch. 0.025 is the
        most that stays clear of it on Foldrak Noir.
      - **The rest.** They have no lenses or glare, since retail's
        billboards show. The crystals take them as they take the
        headlamps, the facets flashing in the brightest lamp's colour.
        They cost about 0.3 ms more a 4K frame with all four lamps lit.
  - **Glints.** The traced water's glint of a deposited light faces its
    emitter.
  - **On models** (2026-10-22: with path tracing the point
    is to replace retail's lights and trace everything). Retail's
    shade on a model holds the grid under it once in the ambient and once
    in the sun's colour (`object_draw.rs`). A primary hit on the grid
    takes that share out (its model light's ambient less the sky's, so the
    model's sun keeps only the tint) and adds the deposited lights
    instead, each as strong as on the ground under it, once all round and
    once more as the face turns toward its emitter, a particle's shadowed
    on the way to the particle as on the ground. A vehicle's side away
    from a blast no longer takes the blast's light; its near side takes
    more. About 0.1 ms more a 4K frame through a grenade's blast.
  - **In the bounce light** (2026-10-22). A bounce ray's hit on the grid
    takes one of the lights, picked by its share of their sum there and
    shadowed with one ray, standing in for them all; the denoiser gathers
    the samples. About 0.25 ms more a 4K frame through a grenade's blast.
    The picture barely changes, since the grid held the same lights,
    unshadowed.
  - **Where the grid stays.** The water's reflections and what shows
    through it, which nothing denoises, keep the grid, as do models beyond
    it (retail's colour there) and shades without a model light. An
    explosion behind a ridge no longer lights the ridge's far side.
  - **Cost.** On the ground, 0.2 to 0.4 ms during an explosion at 4K.
- **Render size and upscaling.** The tracer can render smaller than the
  display (`INF_RENDER_SCALE`, or an upscaler's quality mode). Its primary
  rays are then jittered (a Halton 2,3 sequence of `ceil(8 ratio^2)`
  phases), so frames sample between pixels. A resolve pass brings the
  G-buffer together into colour plus motion. The motion is in render pixels:
  where the hit point sat in the last frame, through the last view and the
  world's wrap, less where it sits now. A moving model's point sat where
  its model was (2026-10-23): the feed remembers each drawn object's model
  vertices from the last frame, by the object, the model, its place among
  the object's draws of that model and the vertex slot, so the wheels, the
  morphs and the spinning parts move too; each vertex carries where it was
  less where it is (zero on its first frame, past a jump of 2048 units, and
  for what no object draws), and the primary hit writes that per pixel for
  the resolve. Before, every pixel moved with the camera alone, and the
  upscalers drew trails behind the ACAM and grenades: under DLSS at
  Performance the turning ACAM now sits 5 to 7% nearer a frame drawn
  without history. The tracer and its upscaler are made on the frontend's
  frame before the title is built, under a line in the game's green font
  ("Preparing graphics...", `frontend_app/cold_start.rs`), and again after
  a settings change
  (`prepare_trace`), not on a world's first traced frame, which they had
  held for 940 ms with DLSS (the sound started while the
  picture waited). Made after the title's first frame, they held the Ubi
  Soft video's picture under its running sound: 0.7 s warm, 3.1 s when a
  new build's shaders compile cold (an unchanged build copied to a new
  path compiled cold too: the driver's cache follows the executable). A
  depth pass traces unjittered rays
  at the display's size for the composite, so the raster's later draws still
  sort against the traced world. FSR 3.1
  ([`vk_device/upscale.rs`](../../crates/inf-game/src/vk_device/upscale.rs);
  chosen by the Upscaler setting) runs through the FidelityFX
  API's library (`amd_fidelityfx_vk.dll`, MIT), loaded from beside the
  executable when present. It needs `VK_KHR_get_memory_requirements2`
  (it calls the KHR names without checking) and 16-bit shader types. FSR's
  jitter moves the image while the tracer's moves the sample, so FSR is
  given the negated offset: with the tracer's own sign, a still scene came
  out smeared and doubled. At 3840x2160 a frame takes 4.5 ms native and
  2.9 ms at FSR Performance (1920x1080).
- **DLSS** ([`vk_device/dlss.rs`](../../crates/inf-game/src/vk_device/dlss.rs)).
  NGX's Vulkan API ships only as an MSVC static library, so a bridge DLL
  (`port/ngx-bridge/inf_ngx.cpp`) links it and uses NVIDIA's own helpers.
  The instance and device enable the extensions NGX asks for, and NGX starts
  when DLSS is first wanted. Asking for the device's extensions takes 0.6 s,
  so a shown window keeps the answer in the per-user cache
  (`ngx-device-extensions.json`), keyed by the GPU, its driver and the DLSS
  libraries. The render size is DLSS's own optimal size for
  the mode, and the jitter takes the same sign as FSR's (the other sign
  doubled a still scene). Super Resolution matches native closely: the mean
  difference is 0.70 against FSR's 1.31, at 3.9 ms a frame.
- **Ray Reconstruction** replaces the port's bounce-light denoiser. The
  resolve hands it the unfiltered bounce light and its guides: the albedo the
  light was multiplied by, world normals with full roughness, and no
  specular albedo, with the sky at NVIDIA's defaults. It also gets the view
  and projection matrices for row vectors. It costs 4.8 ms a frame. RR
  accumulates long histories, so it smooths retail's pulsing lights: around
  Terra Nova's start beacons the ground's blue lags the pulse. Resetting RR
  every frame brings back the exact colours, so the history is the cause.
  NVIDIA's remedies change nothing on Vulkan. Its Vulkan helper does not
  pass the responsivity mask on, and setting the mask directly, or the
  bias-current-colour mask, left the output identical. Preset D, which
  needs the colour flagged HDR, lags about half as much as the default RR2
  model (preset F), so RR uses it. DLSS Super Resolution with the port's
  own denoiser keeps the pulse exact.
- **Settings** ([`modern_settings.rs`](../../crates/inf-game/src/modern_settings.rs)).
  Ray Tracing, Upscaler, Upscaling, Anti-aliasing and Lighting are the
  sidecar's `modern` (schema 11). They appear in the launcher's Vulkan
  group and, under Vulkan, in both Video menus: copies of the Filtering
  row on port cells past the native settings (`ModernRow`), added before
  Back.
  The frontend's (`00539088`) and the in-world Options' (`005390A0`, since
  2026-10-23) bind the same rows, and each lays them out on a chain of its
  own, so its visible rows (thirteen and ten) scroll. They reach the
  device from the next frame, without a reset.
  - **What the device runs** (`ModernSupport`, asked of the device once).
    It reports whether it traces rays and which upscalers run, as far as
    can be told without starting them: FSR where its library loads, XeSS
    where its library loaded for this device, DLSS and Ray Reconstruction
    where NGX's bridge loaded on an NVIDIA GPU. Starting NGX costs over a
    second, so that waits until DLSS is chosen. Without ray tracing the
    upscalers bring the raster's world up from their quality's size (the
    native size until 2026-10-23), and Ray Reconstruction runs nowhere.
  - **Effective settings.** The device and the rows both use
    `ModernSettings::effective`. A saved upscaler the device cannot run
    gives way to the next best (Ray Reconstruction to DLSS, DLSS and XeSS to
    FSR, FSR to none), and the row shows that one while the saved choice
    stays saved, as Filtering does. Without ray tracing the quality is
    Native AA and Ray Reconstruction is DLSS.
  - **What the rows offer.** The Upscaler row steps only through upscalers
    that run, and Ray Tracing reads "Unavailable" (in six languages) on a
    GPU without it and does not move. One that still fails to start falls
    back to the tracer's own scaling, logged once.
- **Lighting** (2026-10-23), Remaster or Eye candy, for a traced world
  only. The tuning's `lighting` numbers scale five things:
  - the water's least reflection;
  - each metal's share and sharpness (`reflect_gain`);
  - the mist's sun share;
  - the glows;
  - the mirrored effects (`effects`: 0 under Remaster, 0.7 under Eye
    candy). Every additive quad the raster draws (particles, the bolts'
    ten-sprite streaks, models' effect billboards and edges) is kept as
    it is submitted as an emitter, a disc from its native corners
    (`scene_capture.rs`'s `capture_emitter`), except what lies at the
    water's level, which mirrors into itself. The nearest 128 go to the
    tracer (binding 26; its texture array is binding 27), which adds
    those a ray passes, each its sprite facing the ray, to the water's
    reflection and refraction and to metal's glossy bounce.

  A preset whose exposure, saturation or S-curve differ from 1, 1 and 0
  grades the frame after the world's last draw and before the HUD
  (`vk_device/grade.rs`): the frame is copied aside and drawn back
  graded, the raster's effects over the world included. Thermal vision
  takes none of it.
- **Anti-aliasing** without an upscaler: FXAA (`pt_fxaa.comp`, after
  Lottes' FXAA 3.11), SMAA (`smaa_*.comp` over the reference
  `shaders/smaa/SMAA.hlsl`) or TAA (`pt_taa.comp`) over the resolved
  colour, before the composite, so the raster's overlays and the HUD stay
  sharp.
  TAA jitters the primary rays as an upscaler would and keeps its own
  history. That history follows each pixel's motion and is clamped to the
  colour's 3x3 neighbourhood (mean and 1.25 spreads in YCoCg), and the new
  frame takes a tenth. At 4K, FXAA adds 0.14 ms and TAA 0.19 ms.
  - **SMAA** runs SMAA.hlsl unmodified through its own language hooks
    (`smaa.glsl`): top-level sampling, as compute shaders need, and its
    edge pass's `discard` as no edge. Its area and search tables upload
    once, and its passes run with the High preset.
  - **Supersampling** (SSAA 2x, 4x) is no pass. The frame's target is
    drawn larger on each side (`VkFrame::presented` is the size shown),
    and a size change takes effect at the first draw after a present. The
    present averages it down (`vk_downsample.frag`, each frame pixel
    weighted by its cover), and `read_rgba` takes the same average on the
    CPU, so snapshots show what the screen does.
- **The main menu, traced** (2026-10-23, a limited test: the
  tuning's `menu.traced`, off). The menu's scene (`00483C60`) draws its
  far backdrop first, from its own eye, then eight models, each in a batch
  of its own. Armed only by the main menu's frame, the frontend host
  begins a feed at the second view (the backdrop stays the raster's; the
  settled camera shares its eye), under a black sky with no haze or
  clouds. The models' batches then wait as one, traced before the footer,
  and the frame's last flush draws what the trace left (the blended parts,
  the footer) over it. About 1 ms a 4K frame.
  - **Sun-lit planets** (2026-10-23: do what the
    tracer already has). The feed takes the models lit by the tracer
    alone (`SceneFeedBuilder::lit_by_trace`): their faces keep neither
    retail's shade nor its light. The scene's light, (255, 210, 170) at
    full brightness, shines from where the menu's sun (`planetsunspin`,
    model 757, at (120000, 35000, 30000), off the screen's right) stands
    as seen from the other models' middle; retail's own light came from
    the upper left, (-0.58, 0.58, -0.58). The planets show day and night
    sides, their days to the right.
  - **Smooth planets.** The planets are low-poly spheres; lit along their
    faces they showed flat facets and blocks of self-shadow. Each corner
    now faces out from its model's middle (`request.transform`'s view
    position), smooth however few its faces, and the shadow rays start
    600 units up their normals (`SceneFeed::shadow_lift`, 0 for the
    world), above the hull.
  - **Lights.** The logo (`titleinfestation`, strength 3, reach 60,000),
    the sun and the ring round the logo's O (`fadering`) glow as lights.
  - **Open.** The ringed planet (`titleplanetringed`) turns from retail's
    pink to tan, some colour the raster gives it that the trace misses.
- **The briefing's camera pane**, the live view of the world in the
  briefing's lower right, is traced as the world is
  (`briefing_renderer.rs`, `begin_pane_feed`). The world draw's feed setup
  is shared (`world_scene::feed_world`, `feed_light_grid`).
  - **Its camera.** The pane's camera covers the whole frame's native
    pixels. Its centre and clip are moved by the canvas origin, since the
    port's widescreen centres the briefing's 4:3 canvas in a wider frame;
    without that the traced view sat 64 native pixels off the raster's.
  - **Its trace.** The trace runs before the pane's flush, as the world
    draw's does. The composite fills only the pane's clip rectangle
    (`SceneFeed::clip`, a scissor), and the static, the text and the
    panel's frame draw over it.
  - **Its fog.** Retail's circular view there is its fog: the camera looks
    down, and nothing is drawn past the fog's end. The pane has no far
    terrain ring (`SceneFog::far_ring`), so the haze completes at the fog's
    end, in the fog's colour.
  - **Checked.** At 3840x2160 the traced pane matches the raster's circle,
    with the figures' shadows on the ground and the hills shaped by the
    sun, and runs clean under validation. A traced gameplay frame is
    unchanged by the shared setup (17 of 8.3 million pixels one level
    apart, the GPU's own noise).
- **Anti-aliasing without ray tracing**
  ([`vk_device/raster_aa.rs`](../../crates/inf-game/src/vk_device/raster_aa.rs)).
  An untraced world (the player's Ray Tracing off, or a GPU without it)
  takes the same FXAA, SMAA and TAA, supersampling, and DLSS, FSR or XeSS
  at the player's quality: at a smaller quality the world's draws fill
  the upscaler's render size at the frame's top left, and the upscaler
  brings it up before the HUD.
  - **The jitter.** The world draw tells the frame where its draws begin
    and end, with its camera, beside the scene feed's own calls. Under
    TAA or an upscaler, the raster's vertex shader moves those draws by the
    tracer's Halton jitter (a push constant, zero for every other draw).
  - **The inputs.** After the world's last flush, `vk_raster_prepare.comp`
    reads the frame's colour and its depth, which the target now allows
    sampling. It writes the colour, the depth and each pixel's motion, as
    the tracer's resolve does. The motion comes from the depth (the TL
    depth is `1 - 64/z`) through this view and the last one, with the eye's
    move taken across the world's wrap on the CPU. Undrawn pixels (depth 1)
    turn with the view alone.
  - **Moving models** (2026-10-23,
    [`raster_motion.rs`](../../crates/inf-game/src/raster_motion.rs)).
    Depth alone moves every pixel with the camera, so the upscalers drew
    trails behind the ACAM. Each object's models are remembered by their
    view transforms, keyed as the tracer's vertex history is (the object,
    the model, its place among the object's draws of that model), and a
    queued model command carries the map from this view to the last one
    (`PrimitiveCommand::motion`, non-native). The frame reprojects the
    corners of those fans that test and write depth through it, from their
    native position and `1/rhw`, and `vk_motion.vert`/`.frag` draw the
    triangles again after the world, depth-tested against its depth
    without writing it, into a motion image the prepare pass prefers.
    Blended draws keep what lies behind them.
  - **The rest.** FXAA and TAA are the tracer's own shaders, which now
    take TAA's reset as a push constant in place of the frame block, so
    they run without the tracer's features. The upscalers are the tracer's
    too. The result is blitted back into the frame before the HUD.
  - **Not under infrared vision.** There the thermal camera brings the
    frame down to its own resolution.
- **XeSS** ([`vk_device/xess.rs`](../../crates/inf-game/src/vk_device/xess.rs))
  loads Intel's `libxess.dll` directly. Its library asks for instance and
  device extensions, and it switches on the device features it needs in
  the feature chain, so the device's features ride under
  `VkPhysicalDeviceFeatures2`. It renders at its own optimal size (2.3 times
  smaller each way at Performance), takes the colour flagged LDR, and reads
  its inputs in the shader-read layout. The jitter takes the same sign as
  the other upscalers (the other sign doubled the difference from native).
  On the RTX 5080 it runs its DP4a path at 4.2 ms a frame (4K Performance),
  with a mean difference from native of 1.54.

## Near flat triangles and the CPU target

`004C12F0` consumes the original full material DWORD. Display fields
`+24/+28/+2C` determine its channel shifts: red and green use logical DWORD
right shifts by `(field+1)&31`; blue shifts the low byte left by `(field-1)&31`.
Each result is masked with `F8`. These are live display-format inputs, not a
universal RGB555 expansion or permission to discard the high material word.

Each base channel is multiplied by the corresponding corner shade byte and
divided by 128. If the largest result exceeds 255, all three channels scale
by `255/largest`, preserving their ratios. Red stays in the x87 register while
green and blue spill to float. `FISTP` uses nearest-even rounding. The CPU
adapter preserves those spills and the startup's 53-bit arithmetic. Unknown
fourth shade bytes and packet padding are not inputs to this conversion.

Signed packet depth becomes `rhw = float(1/signed_depth)`; the next step is
`sz = float(1 - unsigned(renderer+38)*rhw)`. Pixel depth therefore interpolates
the transformed `sz`, not the original view-depth DWORD. Zero depth keeps its
nonfinite native conversion ([depth zero](#depth-zero)). Near vertices have
specular `FF000000` and zero UV.

Mode bit `02` selects source/destination blend factors `ONE/ONE` and diffuse
alpha zero. Otherwise the factors are `ONE/ZERO` and alpha is 255. When display
byte `+0E` is set, the normal path forwards renderer `+3C/+40` to ZENABLE and
ZWRITEENABLE; the additive path disables depth writes but **inherits ZENABLE**.
When that display byte is clear, both depth states remain inherited. The
callback disables fog and unbinds a cached texture through `0050A6A0`, but does
not set alpha-blend enable, depth comparison, shade mode, or culling.

The CPU target requires those inherited values explicitly. Its admitted
device policy is LEQUAL, Gouraud, and no culling; unsupported native state is
an error. Additive drawing respects alpha-blend enable and does not silently
restore depth writes. Subsequent normal commands apply their own native
overrides. The target uses integer pixel centers and a top-left rule, so two
adjacent additive triangles do not draw their shared diagonal twice. This
coordinate convention is documented for Direct3D 9 and prior in Microsoft's
[rasterization rules](https://learn.microsoft.com/en-us/windows/win32/direct3d9/rasterization-rules)
and [Direct3D specification](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm).
Gouraud colors and transformed depth interpolate across the submitted triangle.
Hardware-specific subpixel/color/depth precision and matched retail images
remain acceptance work; this is not a byte-exact emulation of a GPU.

Read-only execution of `004C12F0` in an isolated PE emulator checked 1,024
synthetic cases for exact pre-clip TL vertex words, including signed depths,
unsigned near scales, display shifts, shade normalization and rounding. Four
additional cases checked every display-depth/additive state branch. External
COM/platform effects were intercepted; these checks do not execute GPU pixels
or establish a recorded frame. Small native-output fixtures are retained in
`d3d_flat_tests.rs`; the private emulator harness and output remain untracked.
The CPU regressions separately check interpolation, shared-edge coverage,
persistent state, malformed-packet containment and deterministic image output.
Resident `1.ovl` model169 also runs through model execution, primitive
preparation, queue replay and this pixel target: 18 emitted flat triangles
and 102 rejected fan triangles under an explicit synthetic camera. It retains
mode zero and stays within the native viewport identity path. Replay preserves
the image and depth buffer. All thirteen frontend models instead require
textured/effect pixels, so this model fixture is not menu acceptance.

### Far flat triangles

`004C1940` consumes the four-block far packet: the near fields, far color
DWORD `+2C` and corner fog bytes `+30..+32`; byte `+33` is unused. Its render
states, depth/rhw conversion, diffuse alpha and channel arithmetic (red kept in
x87, green/blue spilled to f32, joint normalization above 255) equal the near
consumer. Before FISTP, each unrounded channel blends toward the far color,
converted through the same display shifts:
`((256 - fog) * channel + fog * far) / 256`. FOGENABLE stays 0, so the fog is
already in the diffuse color; specular alpha carries `255 - fog` and has no
pixel effect. [`prepare_flat_triangle_far`](../../crates/inf-game/src/d3d_flat.rs)
reproduces that, and the CPU target renders it through the shared flat
submission exactly, with no unfogged fallback. The tracked
checker runs 256 original cases
with stubbed COM/display services and mode 20 (no clipping). The
[receipt](../../crates/inf-game/tests/fixtures/d3d_flat_triangle_far_native.json)
matches every TL DWORD and state write. Before this owner, a far flat triangle
in the intro's flush stopped the scene backend and ended the session.

### E4 shared-shade quads

`004D2950` (near) and `004D3260` (far) are the quad form of the flat
triangle consumers. They read E4's corners, mode `+20`, material `+24` and the
one shade DWORD `+28` for every corner; Far adds the far color `+2C` and corner
fog bytes `+30..+33`. Each draws two three-vertex fans from corner `k`, where
mode bit 14 clear selects `k = 1`, clipping each fan through `004BCCE0` unless
mode bit 20 is set. The color, depth, rhw and render-state arithmetic equals
the flat triangle's, and Far blends every corner toward the far color with the
same formula as `004C1940`. [`prepare_flat_quad`](../../crates/inf-game/src/d3d_flat.rs)
reproduces both. The checker runs 256
original cases of each with mode 20, and the
[receipt](../../crates/inf-game/tests/fixtures/d3d_flat_quad_native.json)
matches every fan's TL DWORDs and state writes. Infest01 draws far E4 quads 403
times in the intro (TTD count of `004D3260`); the port used to draw them
unfogged.

## Direct3D viewport clipping

`004BCCE0` takes transformed/lit vertices with eight DWORDs per vertex:
`sx, sy, sz, rhw, diffuse, specular, u, v`. The two color fields are packed
ARGB words despite the decompiler's propagated `float` type. The renderer's
signed WORD viewport is `left/top/right/bottom` at `+48/+4A/+4C/+4E`.

There is an exact identity path: when every vertex has
`left <= sx <= right` and `top <= sy <= bottom`, it returns the original vertex
count without changing vertices. For a triangle the function returns after
checking the first three vertices. Boundary coordinates are inside, including
right and bottom; this does not imply that a pixel outside the target exists.

Otherwise it clips against **top, left, bottom, right**, in that order:
`004BCCE0`, `004BDA00`, `004BE4E0`, and `004BEFD0`. Each pass preserves polygon
order and can introduce vertices. It interpolates `rhw`, recomputes
`sz = 1 - unsigned(renderer+38) * rhw`, and interpolates each diffuse and
specular byte separately. Color deltas pass through x87 `FISTP` before adding
the starting byte. These rounded new colors make unrestricted barycentric
sampling of the original triangle insufficient to reproduce clipped pixels.
The portable clipper preserves startup's 53-bit, nearest-even x87 policy.

For texture coordinates, clear mode bit `100` interpolates `u*rhw` and
`v*rhw`, then divides by the interpolated `rhw`. Set bit `100` interpolates
UV directly. The clipped axis is assigned the exact viewport boundary.
The near ordinary-quad consumer bypasses `004BCCE0` when mode bit `20` is set;
otherwise it clips each of its two triangles separately before submission.
The concrete CPU target now clips crossing flat triangles and submits the
result as the same ordered triangle fan. Bit `20` still bypasses the clipper.
Bounding the pixel loop alone would not preserve the rounded intersections.

The interpolation fraction and new reciprocal depth stay in retained
precision. Depth recomputation consumes that unrounded reciprocal depth;
the reciprocal used by perspective UV spills to float first. The starting
`u*rhw` product stays retained while `v*rhw` spills to float. Both color words
interpolate byte-by-byte, rounding the delta before adding the starting byte.
These asymmetric stores are observable and are preserved in the shared
`D3dTlVertex` adapter. Passes start at the original first vertex, suppress
duplicate boundary intersections, and preserve native polygon ordering.

An isolated execution of original `004BCCE0` and its three following passes
was compared with Rust for 2,048 synthetic triangle/quad cases. Every output
DWORD and vertex order matched, including affine/perspective UV, all four
viewport planes, unsigned near-depth scales and both color fields. Small
original-output fixtures remain in the Rust tests; the full oracle output
is local evidence. Integrated raster tests cover rounded intersection colors,
clipped additive shared edges, inherited depth state and rejection before
pixel writes. Finite attributes, a non-inverted signed viewport and the native
ten-vertex capacity are explicit admission limits. GPU precision and matched
retail frames remain separate acceptance work.

## Direct3D near ordinary textured quad

`004D5410` interprets the packet prepared for `00466150`:
four signed screen-WORD pairs and signed depth DWORDs at eight-byte strides,
mode at `+20`, and the logical Handler-3 entry pointer at `+24`.

It reads selector byte `entry[mode & 1]`; `FF` returns before renderer setup.
The physical descriptor is `entry + 4 + selector*1C`. It calls `0050E770`
for the live Direct3D scene/surface state and `00508FA0` with the selected
descriptor and its runtime texture slot at `+18`. That owner can allocate,
upload, evict, and bind a texture. An arbitrary address derived from a global
sprite ID is not a replacement for this allocation identity.

All four vertices use:

- `sx/sy` from signed screen WORDs;
- `rhw = 1 / signed_depth`, stored as a float;
- `sz = 1 - unsigned(renderer+38) * rhw`;
- constant white diffuse RGB, with alpha selected by the descriptor flags;
- specular `FF000000` in this near consumer.

In particular, this consumer does **not** read the producer's shared shade RGB
at packet `+28`. This is a consumer-specific fact, not permission to ignore
shade fields in other callback families.

UV corners come from the allocated texture object's stored bounds, in order
`(umin,vmin), (umax,vmin), (umax,vmax), (umin,vmax)`. The consumer gets the
allocation from descriptor `+18`, adds eight bytes, then reads these four
floats at `+14/+18/+1C/+20`. Texture allocator `00509D90` packs into 256-by-256
surfaces with border space; its normalized bounds use the executable's
`005291BC = 1/256` constant. These are allocation/presentation coordinates,
not intrinsic Handler-3 parser fields. Texture upload `0050A3D0` dispatches
conversion by encoding, mask/color-doubling flags, target pixel format, and
global device policy; a base-RGB555 preview does not prove all upload paths.

The ordinary path writes these Direct3D render states:

| Input | Native operation |
| --- | --- |
| mode bit `100` | `TEXTUREPERSPECTIVE = (~mode >> 8) & 1` |
| surface `+0E` nonzero | `ZENABLE = renderer+3C`; writes `ZWRITEENABLE` as below |
| flags contain `08` or `31` | disable depth writes when the preceding surface gate applies |
| otherwise | `ZWRITEENABLE = renderer+40` when that gate applies |
| flags `10` | source/destination blend `ONE/ONE`, white alpha `FF` |
| otherwise flags `20` | `ZERO/INVSRCCOLOR`, white alpha `FF` |
| otherwise flags `08` | `SRCALPHA/INVSRCALPHA`, white alpha `80` |
| otherwise | alpha `FF`; use `SRCALPHA/INVSRCALPHA` if global `0065EF2C` bit `08`, else `ONE/ZERO` |

Fog is disabled in these near paths. The consumer changes blend factors but
does not establish every persistent device state, including the global
alpha-blend-enable policy. The CPU target therefore requires those inherited
states explicitly alongside this submission routine.

The mode word also selects the exact diagonal. Let
`k = (~mode >> 14) & 1`; it submits triangles
`[k,(k+1)&3,(k+2)&3]` and `[k,(k+2)&3,(k+3)&3]`.
Thus clear bit `4000` selects diagonal `1-3`, and set bit `4000` selects `0-2`.
After optional screen clipping each result uses `DrawPrimitive` with
triangle-fan type `6`, transformed/lit vertex type `3`, and flags `1C`.
Descriptor flag `200` plus non-null renderer `+44` adds another texture pass;
its mapping and complete state transitions remain a separate boundary.

### Depth zero

No conversion tests its packet depth: `004D5603` loads it (`FILD`) and
divides one by it (`FDIVR`), so a depth of zero gives an infinite `rhw` and
`sz`, which go to `DrawPrimitive` with the finite screen corners. The
explosion debris (`0048A0F0` type 1, table `0053D4F0`) is drawn 250 nearer
than it lies (the table's signed WORD `+8`), so a piece exactly 250 in front
of the eye reaches it. On Stendahl Alpha (world 3), with the buggy parked at
its first start, one did on frame 964 in a zero-scale frame: four equal
corners at (752, 390), sort word `C0000000`.

The CPU target and the OpenGL device used to reject that submission. The
failed flush stopped the scene, the message box and HUD draws after the
world found it stopped, and the session ended ("Infestation bootstrap
failed: session frame 964 failed: ... scene backend stopped after an earlier
failure"). `SoftwareFrame` now applies D3D's rule first: a fan whose every
triangle has no screen area rasterizes nothing, so its depths are never
consumed, whatever they are. A textured primitive whose every corner sits at
depth zero and that covers screen area has a driver-defined result in
retail; the port draws nothing (`ReciprocalDepthPoleApproximation`, logged
once). Stendahl Alpha's run reaches both. Any other nonfinite vertex, such
as one corner at depth zero, still stops the queue. The
[test](../../crates/inf-game/src/software_frame_tests.rs) flushes the
debris packets through the backend.

### Depth of both signs

The world wraps at 16 bits: a point's offset from the eye is taken
sign-extended at the coordinate shift, so a point just past half a world
away lands behind the eye. A far sprite there, whose corners are projected
one by one (an opcode `D6` quad), can have two corners in front of the eye
and two behind. In a Kankutan session of 2026-10-21 (world 15),
sprite 669's corners had depths 32,745 and -32,694, four pixels across at
the horizon. Retail converts each with `FILD`/`FDIVR` (`004D5603`) as it
does depth zero, so D3D gets `rhw` of both signs and the driver decides.
The port refused the quad, and the failed flush stopped the frame's queue
there, for 72 frames of that session. It now draws nothing for such a
primitive and goes on (`ReciprocalDepthSignApproximation`, logged once;
one with no screen area draws nothing by D3D's rule above). A corner at
depth zero still stops the queue. Covered by
`perspective_depths_of_both_signs_draw_nothing`.

### Ordinary Far quad

Thunk `00466160` dispatches renderer `+A0` to Direct3D `004D6570`. The
ordinary Far packet occupies the same three blocks as Near: XY/depth pairs,
mode `+20`, and H3 reference `+24`. Its final eight bytes are converted far
color at `+28` and four fog bytes at `+2C..+2F`. This is a distinct packet
from the four-block Far effect quad. Selector `FF` suppresses the draw before
the geometry, color, fog, upload or device-state operations. Near ignores
the entire eight-byte tail; Far requires it without inventing unknown bytes.

`004D65C1..004D6613` converts the color using actual display channel shifts
and five-bit replication before setting `FOGCOLOR`. The primary branches are:

- Descriptor `10/20` selects additive/inverse-source-color blending, disables
  device fog, and uses literal diffuse RGB `255-fog` at each corner. Alpha is
  `FF`, specular is `FF000000`. At fog 127 this is RGB 128, distinct from
  the effect/shared-shade Far kernels' scaled and normalized RGB.
- Other descriptors use white diffuse RGB, alpha `80` for descriptor `08`
  or `FF` otherwise, and vertex fog toward the converted color. Specular
  alpha is `255-fog`, as stored at `004D6A2A..004D6BE9`.

The source proves the common depth-write mask, descriptor/global blend
precedence, perspective flag, allocation UV corners TL/TR/BR/BL and mode
`4000` diagonal. Each of the two triangles independently uses native viewport
clip `004BCCE0` unless mode `20` bypasses it. Descriptor `0200` with non-null
renderer `+44`, checked at `004D6C14..004D6C20`, remains an explicit required
secondary pass; the adapter does not substitute a primary-only draw.
[`TexturedQuadShading`](../../crates/inf-game/src/d3d_textured.rs) keeps
Near/Far packet and color policy explicit in the same data-bearing request.

The tracked read-only check
`check_ordinary_far_quad_consumer.py`
executes 1,024 synthetic original-PE primary cases under x87 CW `027F`, with
exact TL/state receipts, 1,536 bypass triangle fans and 512 original clipping
calls. Only external COM/display/upload/error services are intercepted;
consumer arithmetic and clipping remain unmodified. The supported executable
SHA256 is required. Run the script from the repository root with Unicorn 2;
`--samples` prints the bounded native outputs used by Rust bit-pin tests.
Focused packet, branch, pixel and original-preview fixtures exercise the CPU
adapter. GPU precision, the secondary pass and matched retail animation remain
separate acceptance work.

### Concrete upload and sampling

The CPU store holds borrowed source bytes and physical descriptors for the
entire residency of each texture. Native addresses are not fabricated from
sprite IDs. Each descriptor owns a separate 256x256 ARGB8888 surface, with a
stable CPU allocation identity. This portable allocation policy deliberately
does not reproduce native shared-atlas packing, eviction or repacking.
The original first allocation places source pixels at `(1,1)`, duplicates the
one-pixel border, and publishes `[1,1,width+1,height+1]/256` bounds. Native size
limits permit nonzero dimensions up to 254; unuploaded texels are not silently
available as black pixels.

`0050A3D0` selects eight alpha-capable conversion kernels from indexed/direct,
masked/unmasked and descriptor bit08. Descriptor bit02 selects **direct
RGB555 encoding**, not source alpha. Direct input uses signed WORD loads;
masked direct zero is transparent while `8000` is opaque black. Indexed
masking tests index zero before palette lookup. Bit08 doubles the source
color word and the ordinary quad separately selects diffuse alpha128.
Live RGB shifts `0065F118/11C/120` preserve the native packed-bit operations:
with shifts `[10,5,0]`, `7FFF` uploads as `[255,255,248,255]`, not a generic
five-bit expansion. Global `0065EF2C` bit08 must be set for this implemented
alpha-capable family; the alternate color-key family remains unsupported.

The CPU quad target applies native diagonal, depth-write and blend policies,
then clips each triangle through `004BCCE0` before rasterization. The supported
persistent texture states are matching point/linear MIN and MAG filters,
MODULATEALPHA (`4`), wrap/clamp addressing, disabled color key and disabled
alpha test. Native device setup selects matching filters and, for global bit08,
texture blend4 with color key disabled. Perspective UV uses interpolated
`uv*rhw / rhw`; mode100 chooses affine UV. Point sampling uses texel intervals;
bilinear sampling uses centers `(i+0.5)/256` and the uploaded border. See
Microsoft's [nearest-point sampling](https://learn.microsoft.com/en-us/windows/win32/direct3d9/nearest-point-sampling)
and [texture addressing/filtering conventions](https://learn.microsoft.com/en-us/windows/uwp/graphics-concepts/introduction-to-textures).
Integer-center coverage and blend arithmetic are deterministic portable
policies; exact GPU subtexel/channel precision and matched retail frames remain
separate acceptance work.

An isolated original-PE oracle and the public Rust store matched 1,024 upload
cases and 43,106 bordered pixels across all eight kernels. Eight allocation
cases checked bounds and the 254 limit. Small native kernel fixtures remain
in Rust tests; full oracle data remains local. Ordinary-quad submission is
checked against original `004D5410` TL words and native state branches, while
CPU tests cover both diagonals/windings, one-write shared edges, perspective
versus affine sampling, bilinear borders, blending, clipping and unsupported
state containment. Perspective polygons with zero or mixed-sign reciprocal
depths are outside the CPU sampling domain and fail before either triangle
writes pixels; native signed vertex conversion remains available for inspection.
These isolated checks intercept COM/platform effects;
they do not execute retail GPU pixels or prove a complete scene.

An end-to-end resident fixture selects Boot then frontend47 and executes original
model751. Its four ordinary textured quads reference original sprite1803
(level47/local22, 100x100, flags0014), share one cached upload, use additive
blending and leave depth writes disabled. An explicit synthetic camera produces
a complete orange ring with 15,044 changed pixels and 98 RGB colors at 256x256.
The test can export PPM through `INF_TEXTURED_CORPUS_PIXEL_SMOKE_PPM`; this is
original source through the full command/pixel chain, not an authored menu
camera or matched retail frame.

### The upload kernels

`0050A3D0` locks the texture surface (`+64`, a 0x6C-byte `DDSURFACEDESC`).
A palettized surface (pixel-format flag `0x20`) takes each sprite through
`00508E90`; any other runs one of the 48 kernels at `00544DF8`. The table
follows a reference count and the descriptor `005294F8` at `00544DF0`,
which `004F8C10` passes to `0048EE20`. The index adds:

- 1 for descriptor bit 01 (masked) and 2 for bit 08 (from the sprite's
  WORD `+4`, `(flags >> 2) & 2`);
- 4 for a 16-bit surface and 8 for a 32-bit one (the surface's
  `dwRGBBitCount`, `+54` of the descriptor); any other depth adds 0;
- 12 for descriptor bit 02 (direct RGB555 rather than indexed);
- 24 when global `0065EF2C` bit 08 is clear (the colour-key family).

Every kernel reads the live channel shifts `0065F118/11C/120` and calls
`0048EF80`. The port's [`d3d_texture`](../../crates/inf-game/src/d3d_texture.rs)
implements the eight it uploads through (alpha family, 32-bit): 8 to 11 and
20 to 23, matched against an original-PE oracle (above). The others are
omitted as replaced by the port's platform: which one runs depends on the
texture format and alpha support the Direct3D driver offers, and the port's
pages are always 32-bit with alpha. So are `004F8C10`, which acquires the
table's module, and the palettized path `00508E90`.

| Index | Kernel | Family | Source | Surface | Masked (01) | Bit 08 |
|---:|---|---|---|---|---|---|
| 0 | `004F8D80` | alpha | indexed | other | no | no |
| 1 | `004F9260` | alpha | indexed | other | yes | no |
| 2 | `004F97F0` | alpha | indexed | other | no | yes |
| 3 | `004F9CE0` | alpha | indexed | other | yes | yes |
| 4 | `004FA280` | alpha | indexed | 16-bit | no | no |
| 5 | `004FA770` | alpha | indexed | 16-bit | yes | no |
| 6 | `004FAD10` | alpha | indexed | 16-bit | no | yes |
| 7 | `004FB210` | alpha | indexed | 16-bit | yes | yes |
| 8 | `004FB7E0` | alpha | indexed | 32-bit | no | no |
| 9 | `004FBCD0` | alpha | indexed | 32-bit | yes | no |
| 10 | `004FC270` | alpha | indexed | 32-bit | no | yes |
| 11 | `004FC770` | alpha | indexed | 32-bit | yes | yes |
| 12 | `004FCD40` | alpha | direct RGB555 | other | no | no |
| 13 | `004FD1F0` | alpha | direct RGB555 | other | yes | no |
| 14 | `004FD7A0` | alpha | direct RGB555 | other | no | yes |
| 15 | `004FDC40` | alpha | direct RGB555 | other | yes | yes |
| 16 | `004FE1A0` | alpha | direct RGB555 | 16-bit | no | no |
| 17 | `004FE660` | alpha | direct RGB555 | 16-bit | yes | no |
| 18 | `004FEC20` | alpha | direct RGB555 | 16-bit | no | yes |
| 19 | `004FF0D0` | alpha | direct RGB555 | 16-bit | yes | yes |
| 20 | `004FF6A0` | alpha | direct RGB555 | 32-bit | no | no |
| 21 | `004FFB60` | alpha | direct RGB555 | 32-bit | yes | no |
| 22 | `00500110` | alpha | direct RGB555 | 32-bit | no | yes |
| 23 | `005005C0` | alpha | direct RGB555 | 32-bit | yes | yes |
| 24 | `00500B90` | colour key | indexed | other | no | no |
| 25 | `005010F0` | colour key | indexed | other | yes | no |
| 26 | `005016E0` | colour key | indexed | other | no | yes |
| 27 | `00501C50` | colour key | indexed | other | yes | yes |
| 28 | `00502260` | colour key | indexed | 16-bit | no | no |
| 29 | `005027C0` | colour key | indexed | 16-bit | yes | no |
| 30 | `00502DC0` | colour key | indexed | 16-bit | no | yes |
| 31 | `00503330` | colour key | indexed | 16-bit | yes | yes |
| 32 | `00503950` | colour key | indexed | 32-bit | no | no |
| 33 | `00503E90` | colour key | indexed | 32-bit | yes | no |
| 34 | `00504470` | colour key | indexed | 32-bit | no | yes |
| 35 | `005049C0` | colour key | indexed | 32-bit | yes | yes |
| 36 | `00504FC0` | colour key | direct RGB555 | other | no | no |
| 37 | `00505490` | colour key | direct RGB555 | other | yes | no |
| 38 | `005059F0` | colour key | direct RGB555 | other | no | yes |
| 39 | `00505EE0` | colour key | direct RGB555 | other | yes | yes |
| 40 | `00506490` | colour key | direct RGB555 | 16-bit | no | no |
| 41 | `00506990` | colour key | direct RGB555 | 16-bit | yes | no |
| 42 | `00506F20` | colour key | direct RGB555 | 16-bit | no | yes |
| 43 | `00507430` | colour key | direct RGB555 | 16-bit | yes | yes |
| 44 | `005079C0` | colour key | direct RGB555 | 32-bit | no | no |
| 45 | `00507E90` | colour key | direct RGB555 | 32-bit | yes | no |
| 46 | `00508410` | colour key | direct RGB555 | 32-bit | no | yes |
| 47 | `00508900` | colour key | direct RGB555 | 32-bit | yes | yes |

## Textured triangles, Near/Far effect quads and solid quads

`004CE930` reads five payload blocks: three XY/depth pairs, mode at `+18`,
Handler-3 reference at `+1C`, three RGB shade triples at `+20/+24/+28`, and
six signed UV DWORDs at `+2C..+40`. The unused fourth shade bytes are not inputs.
UV uses the allocation bounds and signed coordinates divided by 65536.
Unlike ordinary quads, this consumer uses corner shades: each channel is
multiplied by 255/128 and the entire RGB triple is normalized if its largest
channel exceeds 255, preserving native x87 spills and nearest-even rounding.
Selector `FF` skips before geometry/color reads. State, secondary-pass admission,
and viewport clipping retain their explicit native policies. The original PE
and Rust match 544 cases, including all low nine descriptor-flag bits and
clipped polygons. Small output fixtures are versioned; the full oracle is local.

### Per-corner Far textured triangles

`004D0170` consumes the same five-block signed XY/depth, RGB and signed
allocation-relative UV layout as Near `004CE930`. Far additionally requires
the color DWORD at `+44` and three fog bytes at `+48..+4A`; `+4B..+4F`
remain padding. Selector `FF` exits before these fields or renderer effects.
The Near consumer continues to ignore the entire Far tail.

Far converts color through live display channel shifts and five-bit replication,
then writes `FOGCOLOR`. Flags `10/20` disable device fog, choose additive or
inverse-source-color blending, and scale each corner RGB by
`(256-fog-(fog>>7))/128` before native peak normalization. Other flags retain
normalized `255/128` RGB and specular alpha `255-fog` for vertex fog. Descriptor
`08` gives diffuse alpha128; process policy08 chooses source-alpha blending
with alpha255 when descriptor08 is absent. Managed depth, perspective, signed
UV conversion and the required secondary0200 pass retain their explicit policies.
The shared shading enum carries stable Near/Far color and fog data without
merging the distinct per-corner and DE packet decoders.

`check_far_textured_triangle_consumer.py`
executes 1,024 supported original-PE primary cases under x87 CW027F. It checks
exact TL DWORDs/state receipts, 768 bypass fans and 256 unmodified native
viewport-clipping calls. Only external COM/display/upload/error services are
supplied. `--samples` reproduces the eight bounded
[Rust fixtures](../../crates/inf-game/tests/fixtures/d3d_far_textured_triangle_native.json);
`--output <path>` deliberately writes the full local oracle for the ignored
`full_local_far_native_oracle_matches_rust_submission` test through
`INF_FAR_TEXTURED_TRIANGLE_ORACLE`. Packet, fogged/faded pixel and shader tests
exercise the actual CPU callback route. This establishes the primary consumer,
not GPU-exact rasterization or a matched retail frame. Terrain's independent
[near-plane producer](TERRAIN_RUNTIME.md) submits this consumer; ordinary model
fans can use it through their existing callback.

### Per-corner triangle primary prefix

Descriptor0200 with non-null renderer+44 requires a secondary pass in both
ordinary per-corner consumers. Native Near `004CF3F3` and Far `004D0D61` decide
that continuation after conversion; primary draws `004CF460/004D0DD0` precede
secondary setup `004CF4BA/004D0E2A`. The
[triangle adapter](../../crates/inf-game/src/d3d_textured_triangle.rs) returns
the primary submission and an explicit continuation. The scene backend retains
its primary pixels, triangle count and fragments before reporting the required
secondary pass. Source validation, upload failures, nonfinite geometry and
selector-FF admission still precede the continuation. This policy applies to
ordinary model fans, clipped D6 outputs and terrain triangles across missions;
DE's separate implicit-UV consumer keeps its own policy.

check_textured_triangle_secondary_prefix.py
reproduces [48 native receipts](../../crates/inf-game/tests/fixtures/textured_triangle_secondary_prefix_native.json),
including toggle-identical primary TL/state/fans, four selector-FF returns and
four viewport rejects. Original conversion, clipping and draw ordering execute
unchanged; COM/upload/display/error helpers are supplied hosts. The
[prefix tests](../../crates/inf-game/src/d3d_textured_triangle_prefix_tests.rs)
also exercise Near/Far pixel and device-state equivalence, earlier/current
queue-prefix retention, later-command suppression and admission errors.
Secondary setup and GPU-exact pixels remain open.

### What the secondary pass draws

REA's decompilation of the Near per-corner consumer
`004CE930` (renderer vtable slot `00529164`) shows the continuation. After
the primary fan it copies the three TL vertices (0x18 DWORDs) to `005631C8`,
binds the renderer's `+44` texture through the shared cache bind `00508FA0`
(25 consumer callers), and rewrites each copy's UV from its own screen
position:

- `x = (sx - [0055F23C]) * w / ([0055F22C] * 512)` and
  `y = ([0055F240] - sy) * w / ([0055F230] * 512)`, with `w = 1/rhw`, each
  clamped to `[-1, 1]`;
- `tu = (x + 1) / 2 * (u1 - u0) + u0`, `tv = (1 - y) / 2 * (v1 - v0) + v0`
  over the bound texture's sub-rectangle (its record `+1C..+28`).

It then draws the same fan again (`DrawPrimitive(6, D3DVT_TLVERTEX, ..., 0x1C)`):
a second layer mapped by view-space position, like an environment map.
The DA consumers `004C2200`/`004C2DF0` take the same branch: `TEST AH, 2` on
the selected variant's flags word (`004C266F`/`004C3383`), then renderer
`+44` (`004C2696`/`004C338C`); the packet's mode word plays no part.

Whether retail ever runs it depends on renderer `+44`. The renderer is the
display's `+14` (`00495630` stores `004EB4B0`'s result; accessor `00495800`),
and its base constructor `004A49B0` clears `+44` at `004A49F0`. A sweep of
every `+44..+47` store through a non-`ESP` base in the functions that use the
renderer (callers of `00495800` or `004EB4B0`, and renderer vtable calls),
with `sweep_instructions.py`, finds only
viewport, HUD and menu structures (`0040BEB5`, `0041D840`, `0041D8B2`,
`004421B2`, `00444A49`, `004869CF`, `00486A85`). No static code sets
renderer `+44`. Computed offsets and block copies escape that sweep, so
`renderer-44.js` watched it at
run time: every write to `+44` of every renderer that `004A49B0` builds,
judged by the object's vtable at the write. In Infest01 (five constructions
over two objects) and Infest04 (three over two) the only writes while the
memory is a renderer are the constructor's zero at `004A49F0`; the others
land while the memory belongs to other allocations. The port's
`secondary_texture: false` matches retail in both
(ledger).

### Shared-shade textured triangles

Opcode DE's Near/Far thunks `004A38C0/004A3900` dispatch to separate
Direct3D consumers `004C3B60/004C5210`. Both packets occupy three blocks:
three signed XY/depth pairs, mode at `+18`, H3 reference at `+1C`, and shared
RGB bytes at `+20..+22`. Far adds converted color at `+24` and three fog bytes
at `+28..+2A`. Shade byte `+23`, byte `+2B` and final padding are not inputs.
Selector `FF` suppresses either draw before geometry, color, upload or state reads.
Both consumers read the allocation corners TL/TR/BR, with no authored UV
DWORDs. This replaces the previous DE Near adapter's labeled TL/TR/BL mapping;
intrinsic H3 data is unchanged.

Far writes FOGCOLOR using actual display shifts and five-bit replication at
`004C5261..004C52B3`. Descriptor `10/20` selects additive/inverse-source-color,
disables device fog, and scales the shared RGB per corner by
`(256-fog-(fog>>7))/128`. Other descriptors retain normalized `255/128` RGB,
alpha 128 for descriptor `08` or 255 otherwise, and vertex fog with specular
alpha `255-fog`. Primary depth, blend and perspective branches are source-backed;
descriptor `0200` with renderer `+44` still fails explicitly at the required secondary
texture pass. The existing shared textured raster consumes the resulting TL
geometry, clipping, fog and blend policy; Far never routes through Near shading.

The tracked read-only check
`check_shared_shade_triangle_consumer.py`
executes 2,048 synthetic original-PE primary cases under x87 CW `027F`: 1,024 for each
consumer, including exact TL/state receipts, 1,536 bypass fans and 512 original
viewport-clipping calls. Only external COM/display/upload services are
intercepted; consumer arithmetic and clipping remain unmodified. It checks the
supported executable SHA256 and writes no fixtures. Run
`python check_shared_shade_triangle_consumer.py`
from the repository root with Unicorn 2; `--samples` prints the bounded synthetic
cases used by Rust bit-pin tests. Packet containment, branch policy and Far
fogged/faded pixels have focused Rust checks. This establishes the primary
submission policy, not a GPU-exact raster or matched retail preview sequence.
An original Level1 H8 117 preview at an explicit synthetic screen age 7,500 ms
and model age 500 ms reaches this Far consumer together with the independently
owned [ordinary Far quad](#ordinary-far-quad). Differential pixel controls omit
one named consumer at a time; the actual draw retains strict complete queue
flush. Complete preview animation remains bounded by the other model/consumer
owners and does not follow from one covered pose.

### Effect quads and solid quads

`004D78A0` reads three blocks, mode at `+20`, Handler-3 reference at `+24`, and
effect RGB at `+28/+29/+2A`; byte `+2B` is unused. It uses allocation-bound quad
UVs, the native diagonal, and one normalized effect color at every vertex.
`004843D0` also calls this consumer directly for the footer: depth DWORDs are
`40`, mode is zero, and its resolved Handler-6 style word supplies the RGB.
The effect adapter matched 1,024 original-PE cases, including native clipped
polygons with three through six vertices. Sharing a textured submission type
does not erase the distinct ordinary, shaded-triangle and effect color rules.

`004D9840..004DBA81` consumes the four-block Far effect packet. It adds the
converted far-color DWORD at `+2C` and four fog bytes at `+30..+33`; effect
byte `+2B` and padding `+34..+3F` remain unused. Selector `FF` returns before
these inputs or any renderer-state writes. `004D9891..004D98E3` converts far
color through the actual display channel shifts and replicates each channel's
five high bits before setting `FOGCOLOR`. It does not interpret that DWORD
as an intrinsic RGB555 value independently of display state.

The primary blend, depth-write mask, perspective flag, UV bounds and mode4000
diagonal agree with Near. The Far color/fog branches remain distinct:

- Descriptor `10/20` selects additive/inverse-source-color blending, disables
  device fog and uses per-corner effect RGB multiplied by
  `(256 - fog - (fog >> 7)) / 128`. Peak RGB normalization and x87 spill/rounding
  follow the recovered kernel. Diffuse alpha is `FF`; specular is `FF000000`.
- Other descriptors use Near's `255/128` normalized RGB, retaining diffuse
  alpha `80` for flag08 or `FF` otherwise. Device vertex fog is enabled, and
  specular alpha is `255 - fog` at each corner.

[`D3dTextureFog`](../../crates/inf-game/src/d3d_textured.rs) names disabled
versus vertex fog explicitly. Existing disabled-fog callers retain their
policies. The CPU target interpolates the specular-alpha visibility in screen
space, blends textured RGB with the far color, preserves texture/diffuse alpha,
then applies destination blending. This follows Microsoft's
[pretransformed vertex fog contract](https://learn.microsoft.com/en-us/windows/win32/direct3d9/vertex-fog).
Native viewport clipping retains the fog alpha alongside the other TL fields.
Descriptor0200 with renderer+44 still requires an unimplemented second pass.
Native Near `004D8541` and Far `004DA671` decide whether to run that continuation
after TL conversion. Their reached primary fans precede secondary setup
`004D8748/004DA878`. The direct-effect adapter now returns a primary submission
and an explicit continuation, preserving primary pixels and effect statistics
before reporting the unavailable secondary pass. This shared path serves 1B,
direct E6, particles and footer draws; it does not depend on scene identity.

check_effect_secondary_prefix.py
reproduces [40 native prefix receipts](../../crates/inf-game/tests/fixtures/effect_secondary_prefix_native.json),
including real viewport fans, toggle-identical primary TL/state/draws and four
selector-FF returns. COM/upload/display/error helpers are supplied hosts.
The [pixel/queue tests](../../crates/inf-game/src/software_frame_effect_tests.rs)
retain earlier and current primary draws on the later secondary boundary and
preserve primary admission failures. Complete secondary setup and GPU parity
remain open.

The read-only original-PE check
`check_effect_far_consumer.py` executes
1,024 bounded synthetic primary cases, checking TL DWORDs and render-state
receipts across the low ten descriptor bits, fog endpoints, actual display
shifts, signed depths, global08, and both diagonals. It checks 768 bypass fans
and executes 256 original clipping calls with intercepted COM/upload effects.
It requires Python3 with Unicorn2 and runs without fixture writes:
`python check_effect_far_consumer.py` from the
repository root. These are submission checks, not GPU or retail-frame acceptance.
Focused Rust tests keep Near's existing oracle unchanged, check Far packet
admission and both shading branches, and exercise fog interpolation, clipping,
triangle winding and destination-alpha ordering. Original H8/98 decoration
feeds strict Far command/pixel fixtures in both 640x480 and 1024x768 tiers;
its native Z/extent dispatch is preserved.

`004D1BB0` consumes the converted material DWORD at `+24` of a three-block
solid-quad packet. It packs display channels and replicates their upper bits;
the shaded flat-triangle consumer does not perform that replication. Mode `02`
selects additive blending and inherits depth enable while disabling depth writes
when the display manages depth. It preserves quad diagonal and clipping rules.
The star producer writes the full signed depth DWORD `7FFF_FFFF`, not two
independent depth/attribute fields. With near scale64, TL depth rounds to exactly
`1.0f32`, and the stars pass LEQUAL against the cleared depth surface. All 512
original-PE oracle cases match Rust TL words and state receipts.

D8's solid triangles (`004A38E0`/`004A3920` -> `+3C`/`+40` ->
`004BFAB0`/`004BFE80`) are these consumers over three corners: the same
packed, replicated material color, Far's fog blend toward the far color with
`255 - fog` specular, mode `02` additive and the mode `20` clip bypass, one
three-vertex fan. The packet's mode, material and far tail start eight bytes
earlier (`18`, `1C`, `20`/`24`). check_solid_triangle_consumers.py
runs both over 512 cases; the
[Rust comparison](../../crates/inf-game/src/d3d_solid_quad_tests.rs) matches
every TL word and state.

### Direct D2 per-corner triangles

[`d3d_d2`](../../crates/inf-game/src/d3d_d2.rs) owns Near
`004A3930/+64 -> 004C6AB0` and Far `004A3960/+68 -> 004C8160`.
The direct packets retain three independent shade DWORDs and implicit allocation
UVs. Their consumers now follow native descriptor blend/depth policies,
hue-preserving normalization and Far vertex-fog versus diffuse-attenuation
branches. The former observed-frame blend/depth pins are superseded.
Selector `FF` returns before shade/geometry reads. Descriptor `0200` with
renderer `+44` reaches secondary setup at `004C756D/004C8D4D` after the primary
fan; the backend retains completed primary pixels/statistics before reporting
that unowned continuation.

check_d2_pixels.py reproduces
[48 native receipts](../../crates/inf-game/tests/fixtures/d2_pixels_native.json),
compared by [Rust](../../crates/inf-game/src/d3d_d2_prefix_tests.rs) for TL words,
ordered states and viewport fans. COM/upload/display/error services are supplied
hosts; secondary descriptor reads, GPU pixels and retail acceptance remain open.

### DA textured triangles

[`d3d_da`](../../crates/inf-game/src/d3d_da.rs) owns Near
`004A3A00/+54 -> 004C2200` and Far `004A3A40/+58 -> 004C2DF0`. Selector `FF`
returns first. The selected variant's flags low byte then picks the corners,
blend and fog:

| Variant flags | Corners | Blend | Fog | Depth writes |
| --- | --- | --- | --- | --- |
| no `30`, `08` | white, alpha `80` | source alpha | Far: device fog | off |
| no `30`, no `08` | white, alpha `FF` | copy, or source alpha with global `08` | Far: device fog | off with `01` |
| `10` | Near white, Far `255 - fog` grey | additive | off | off |
| `20` (without `10`) | Near white, Far `255 - fog` grey | inverse source color | off | off |

The alpha comes from `004C2349`/`004C238D` (Far `004C2FA6`/`004C2FEA`) and is
ORed with `00FFFFFF` at `004C2554`. Far device fog takes the far color at
packet `+20` through the display shifts as `FOGCOLOR`, and each corner's
specular alpha is `255 - fog`. check_da_consumers.py
runs both consumers over 336 cases (every low-byte flag combination, both
global policies, signed depths and fog), and the
[Rust comparison](../../crates/inf-game/src/d3d_da_tests.rs) matches every
TL word and state. Before it, the port drew every DA corner opaque, skipped the
additive and inverse variants, left Far DA unfogged and kept depth writes: the
translucent tips of Terra Nova's crystal clumps (sprite 1444, flags `800C`)
drew opaque.

### UV-only triangles and quads

[`d3d_textured_uv`](../../crates/inf-game/src/d3d_textured_uv.rs) implements
Near triangles `004A3990/+6C -> 004C9A00`, Far triangles
`004A39C0/+70 -> 004CA7B0`, Near quads `004A3A20/+B4 -> 004DFC80` and Far quads
`004A3A60/+B8 -> 004E1000`. The mixed near-plane DA/E2 producers emit these
distinct four/five-block packets. Triangle mode,
texture and UV begin at `+18/+1C/+20`; quad fields begin at `+20/+24/+28`.
There are no shade fields. The final eight bytes of each Near packet are unused;
Far color/fog begins at `+38/+3C` for triangles and `+48/+4C` for quads.
Signed UV DWORDs divide by 65536 before mapping through the selected allocation
bounds. Near diffuse RGB is white. Far flags `10/20` attenuate that white by
`255-fog`; ordinary Far paths retain white with vertex fog. Alpha, blend factors,
depth-write masks and secondary-pass admission follow each consumer. The quad
uses the original mode4000 diagonal, with each triangle clipped separately
unless mode20 bypasses clipping. Both shapes use the same bounded CPU coverage
and upload owner after their distinct native packet conversion.

The original PE and Rust agree on 2,112 cases across both Near consumers: low descriptor
flags, signed UV/depth, state branches, diagonals and actual clipped ordered fans.
Twenty-four literal output fixtures are versioned. This does not authorize
inventing the E2 scratch colors: their mapped-but-unknown values remain separate
from these RGB-free packets, as documented in
[model scratch ownership](MODEL_RUNTIME.md).

check_textured_uv_consumers.py adds
[112 Near/Far receipts](../../crates/inf-game/tests/fixtures/textured_uv_consumers_native.json),
compared by [Rust](../../crates/inf-game/src/d3d_textured_uv_prefix_tests.rs).
Selector `FF` precedes geometry/UV/Far reads. Enabled secondary descriptor reads
at `004CA04B/004CAED2/004E0521/004E19B7` follow any primary fans; the backend
retains those pixels and statistics before the explicit secondary-pass error.
Original consumers and viewport clipping execute with supplied COM/upload/
display/error hosts. Neither these receipts nor CPU prefix tests establish
secondary execution, GPU precision or matched retail scenes.

Reproduce both consumer fixtures with Python 3 and Unicorn 2:

```powershell
python check_d2_pixels.py --output .tmp/d2-pixels-native.json
python check_textured_uv_consumers.py --output .tmp/textured-uv-consumers-native.json
```

### Shared-shade UV primitives

[`d3d_shared_shade_uv`](../../crates/inf-game/src/d3d_shared_shade_uv.rs)
implements the four consumers emitted by the
[clipped E6 producer](PRIMITIVE_RUNTIME.md#opcode-e6-shared-shade-quads-and-clipped-uv).

| Shape/pass | Queued callback | Renderer slot / Direct3D consumer | Blocks | Shared RGB / UV | Far color / fog |
|---|---|---|---:|---|---|
| Triangle Near | `004A3A10` | `+74` / `004CB6B0` | 4 | `+20` / `+24` | — |
| Triangle Far | `004A3A50` | `+78` / `004CCEF0` | 5 | `+20` / `+24` | `+3C` / `+40` |
| Quad Near | `004A38D0` | `+BC` / `004E2550` | 5 | `+28` / `+2C` | — |
| Quad Far | `004A3910` | `+C0` / `004E4710` | 6 | `+28` / `+2C` | `+4C` / `+50` |

These packets retain explicit signed 16.16 UV and shared RGB. They share the
bounded TL/raster submission shape with other textured families while retaining
their native layout, color, fog and state policies. Selector `FF` returns before
reading RGB, UV or Far fields. The shade DWORD's high byte, Near packet tails
and the fourth fog byte in a Far triangle are unread by the primary consumer.
Mapped but unknown RGB remains an error only when the reached consumer needs it.

Near RGB uses the native `255/128` conversion. Far opaque/source-alpha paths
retain vertex fog; additive and inverse-source-color paths scale RGB by
`(256 - fog - (fog >> 7))/128`. Allocation bounds map signed UV after division
by 65536. Native reciprocal-depth rounding, mode4000 diagonals, viewport fans,
blend and depth-write policies feed the shared CPU target.

Descriptor `0200` with renderer `+44` reaches secondary texture setup **after**
the primary draw. The scene backend commits primary pixels and statistics before
reporting the unowned secondary pass. This prefix rule belongs to these consumers;
each callback family keeps its own recovered continuation boundary. Direct
effects, ordinary per-corner triangles and [direct D6 quads](TERRAIN_RUNTIME.md#shared-production-pixels)
retain their separately proven primary prefixes, as do direct D2 and UV-only
consumers above. DE's implicit-UV consumer retains its existing policy.
Exact legacy software spans and the secondary texture pass remain open.

check_shared_shade_uv_pixels.py
executes all four original Direct3D consumers and their real viewport clipper.
Its [136 receipts](../../crates/inf-game/tests/fixtures/shared_shade_uv_pixels_native.json)
match [Rust TL words, ordered state changes and primary fans](../../crates/inf-game/src/d3d_shared_shade_uv_tests.rs),
including eight selector-FF early returns and reached secondary boundaries.
COM, upload, display and error helpers are supplied terminal hosts; secondary
setup and GPU pixels are outside this oracle. Separate CPU pixel tests establish
nonwhite output for all four callbacks and primary pixels/statistics retained
after a secondary-pass error. These tests do not close matched scene acceptance.

Reproduce from the repository root with Python 3 and Unicorn 2:

```powershell
python check_shared_shade_uv_pixels.py --output .tmp/shared-shade-uv-pixels-native.json
```

The checker authenticates the original executable. Compare its transient result
with the tracked fixture; original assets and generated output remain local.

### Per-corner shaded UV quads

[`d3d_textured_shade_uv`](../../crates/inf-game/src/d3d_textured_shade_uv.rs)
implements Near `004A3940/+C4 -> 004E6B70` and Far
`004A3970/+C8 -> 004E8D30`. The
[D6 clipping producer](PRIMITIVE_RUNTIME.md#de-and-d6-source-polygons) emits
these six-block quads alongside the existing per-corner UV triangles.
Mode is at `+20`, H3 relocation at `+24`, four shade DWORDs at `+28..37`,
and signed UV pairs at `+38..57`. Far adds color at `+58` and fog at `+5C..5F`.
Near ignores `+58..5F`; both primary consumers ignore shade high bytes.
SelectorFF exits before shade, UV, geometry and Far reads.

The primary conversion retains per-corner hue-preserving `255/128` normalization,
Far30 diffuse attenuation versus vertex fog, native reciprocal-depth rounding,
signed 16.16 allocation UVs, mode4000 diagonal, viewport fans, blend and depth
policies. This data feeds the same proven final raster target. Direct D6's
four-block implicit-UV packet remains a separate policy.

Enabled secondary setup begins at `004E7B27/004E9E70`, after both primary fans.
The backend retains those pixels and counts the submitted primary consumer
before its explicit secondary-pass error. A completed-primary count can therefore
be one while the failed queue command's completion count is zero.

check_textured_shade_uv_pixels.py
executes both original consumers and their actual viewport clipper.
Its [68 receipts](../../crates/inf-game/tests/fixtures/textured_shade_uv_pixels_native.json)
match [Rust TL words, state and fans](../../crates/inf-game/src/d3d_textured_shade_uv_tests.rs),
including four selector-FF gates and reached primary-before-secondary paths.
COM/upload/display/error calls are supplied terminal hosts; GPU pixels and the
secondary implementation are outside the oracle. Separate CPU tests establish
both primary consumers and retained pixels/statistics after a secondary error.
Their constant corner-color fixture does not independently prove varied-corner
interpolation; that boundary is covered by native TL/fan comparisons and the
shared raster owner's tests. Matched complete scene acceptance remains open.

Reproduce the new pixel-prefix receipts with Python 3 and Unicorn 2:

```powershell
python check_effect_secondary_prefix.py --output .tmp/effect-secondary-prefix-native.json
python check_textured_shade_uv_pixels.py --output .tmp/textured-shade-uv-pixels-native.json
python check_textured_triangle_secondary_prefix.py --output .tmp/textured-triangle-secondary-prefix-native.json
python check_d6_secondary_prefix.py --output .tmp/d6-secondary-prefix-native.json
```

These tools authenticate the supported original executable before execution.
Compare their transient results with the tracked fixtures.

## Resident scene backend

[`software_scene`](../../crates/inf-game/src/software_scene.rs) joins resident
resources, live model state, actual Handler-8 execution, primitive queues and
CPU pixels. All nine authored frontend roots, stars and six footer sprites
reach this backend. It retains lighting, view, texture residency and renderer
state across frames. Unsupported callbacks and unknown fields remain errors.

The light setter `00490030` computes effect RGB from half the directional RGB
plus the **previous** ambient word. The following scene ambient setter changes
ambient only; it does not recompute effect RGB. Both preserve their packed high
byte. Renderer reset `004BCA20(0)` clears depth and sets device ZENABLE=0,
ZWRITEENABLE=1 when the display manages depth, without modifying renderer
near/depth-enable/depth-write fields or the color surface. Direct3D virtual `+04`
(`004EB490`) only returns the material-sorting gate and emits no commands.

Opaque sort identity is explicit. Native replay keeps the actual entry address,
physical descriptor offset and signed shift. The portable backend assigns each
first-used resident sprite entry an ordinal, shared by aliases of the same
source entry. Its key is `40000100 + (ordinal << 8) + physical_selector`, keeping
flat opaque key `40000000` below textures. Separate source allocations remain
distinct even when their bytes match; retained borrows protect identity lifetime.
FF and translucent descriptor paths retain geometric keys. This portable
allocation policy is not a reconstruction of process pointers: cross-allocation
coplanar overlap still needs retail comparison.

Model746 instead uses sprite712, whose 4,191 palette WORDs are authored zero
in all six installed copies (`1d0..1d4` and `47`). Literal `47.ovl` palette
`8989E..8B95C` and index bytes `13020C..13291C` also produce only opaque black
through unchanged native `0050E690` conversion and `004FB7E0` upload, checked
with four source-shift settings. Its additive output cannot serve as a visible
smoke fixture. A later game-specific palette mutation is unproven; do not
repair the source offset or invent a palette to make that ring visible.

## Original software textured-quad policy

`004B6B10` selects the same Handler-3 physical descriptor but constructs UV in
16.16 texel coordinates with endpoints `(W<<16)-1` and `(H<<16)-1`. It copies
the four screen positions to a native polygon span converter, `004A51E0`,
using 16.16 X and integer Y. The primary pass reads neither corner depth nor
the packet's shared shade RGB. Descriptor encoding and blend/mask flags select
the scan tables at `00543E00` (indexed) or `00544280` (direct), through globals
`00544700/00544704`. The optional descriptor-`200` second pass does use depth.

### The software renderer's spans

The software renderer (unreachable in a retail run, above) fills polygons in
`004A51E0`. It takes the polygon's vertices (records of nine DWORDs from
`+64`: 16.16 X at `+10`, Y at `+14`, then the interpolated values), finds the
top and bottom, and builds the left and right edge chains. `004A4C60` clips
a chain at the clip rectangle's top (`+4A`) with new vertices from the pool
at `+1C`; `004A4E70` classifies each edge against the left and right sides
(`+48`/`+4C`, 1 left, 2 inside, 4 right) and inserts the crossing vertices.
A polygon whose chain turns back in Y is cut to a triangle by `004A5090`,
which re-enters `004A51E0` with three vertices. Scanlines go to the selected
row's span.

Each triangle routine picks the row (`004B0730` shows it): descriptor flag
`0x02` selects the table, `00544704` = `00544280` for 16-bit texels and
`00544700` = `00543E00` for 8-bit texels through a palette (renderer
`+1058`); the blend flags select a group of twelve rows (`0x10` 24, `0x20`
36, `0x08` 12, else 0); a texture adds 4, or 8 with flag `0x01` (colour key);
the triangle kind adds its shading column. The row (renderer `+60`) holds a
span and five edge steppers. Spans by row (the 8-bit table's / the 16-bit
table's, where they differ):

| Rows | Flags | Kind | +0 | +1 | +2 | +3 |
| --- | --- | --- | --- | --- | --- | --- |
| 0..3 | none | flat | `004A5FF0` | `004A5FF0` | `004A65E0` | `004A6D70` |
| 4..7 | none | texture | `004ABA40` / `004A73D0` | `004ABB80` / `004A74D0` | `004ABCB0` / `004A7C70` | `004ABE40` / `004A85A0` |
| 8..11 | none | keyed texture | `004AC080` / `004A8850` | `004AC1D0` / `004A8960` | `004AC310` / `004A8A70` | `004AC4B0` / `004A8CC0` |
| 12..15 | `08` | flat | `004A8F80` | `004A8F80` | `004A9010` | `004A9180` |
| 16..19 | `08` | texture | `004AC710` / `004A9320` | `004AC880` / `004A9450` | `004AC9E0` / `004A9580` | `004ACC50` / `004A97E0` |
| 20..23 | `08` | keyed texture | `004ACEA0` / `004A9AC0` | `004AD020` / `004A9C00` | `004AD190` / `004A9D40` | `004AD370` / `004A9FB0` |
| 24..27 | `10` | flat | `004AA2A0` | `004AA2A0` | `004AA350` | `004AA350` |
| 28..31 | `10` | texture | `004AD5E0` / `004AA4C0` | `004AD740` / `004AA5F0` | `004AD890` / `004AA720` | `004AD890` / `004AA720` |
| 32..35 | `10` | keyed texture | `004ADA50` / `004AA980` | `004ADBC0` / `004AAAC0` | `004ADD20` / `004AAC00` | `004ADD20` / `004AAC00` |
| 36..39 | `20` | flat | `004AAE70` | `004AAE70` | `004AAF20` | `004AAF20` |
| 40..43 | `20` | texture | `004ADEF0` / `004AB090` | `004AE060` / `004AB1C0` | `004AE1C0` / `004AB2F0` | `004AE1C0` / `004AB2F0` |
| 44..47 | `20` | keyed texture | `004AE380` / `004AB550` | `004AE500` / `004AB690` | `004AE670` / `004AB7D0` | `004AE670` / `004AB7D0` |

The shading columns: `+0` plain; `+1` a constant shade (8-bit texels: the
palette's shade row, 32 shades of `0x40` bytes; 16-bit texels and flat fills
run the same code as `+0`); `+2` the vertices' colour (flat: per-channel
values shifted into the surface layout from renderer `+2C`'s `+24/+28/+2C`;
16-bit texels: that colour added to the texel, unsaturated; 8-bit texels: an
interpolated shade row); `+3` the colour plus fog, a ramp
toward the fog colour (`00560A98`/`00560A9C`, the value's top bits) added and
saturated (`00560C20`, `00563308`). The `0x10` and `0x20` groups have no fog
column. 16-bit texels come from rows of 1,024, 8-bit ones from rows of 2,048
bytes (those spans fill right to left); keyed spans skip texel 0.

| Flags | Flat | 16-bit texels | 8-bit texels |
| --- | --- | --- | --- |
| none | written | written | written through the palette |
| `08` | averaged with the screen | halved, the screen not read | the palette entry plus half the screen |
| `10` | added to the screen, saturated | halved, as `08` | added, saturated |
| `20` | as `10` (byte-identical code) | as `10` (byte-identical) | subtracted from the screen, floored at 0 (`005601C8` guards the borrow) |

The five steppers of each row, by the values they carry:

| Used by | Edge setup | Clip at X | Clip fraction | Skip rows | Load |
| --- | --- | --- | --- | --- | --- |
| flat `+0/+1` | `004A5C40` | `004A5CB0` | `004A5E50` | `004A5FD0` | `004A5FE0` |
| flat `+2` | `004A6050` | `004A6130` | `004A6340` | `004A6560` | `004A65B0` |
| flat `+3` | `004A6750` | `004A6860` | `004A6A90` | `004A6CD0` | `004A6D30` |
| texture `+0/+1` | `004A6F10` | `004A6FD0` | `004A71B0` | `004A7380` | `004A73B0` |
| texture `+2` | `004A75D0` | `004A7700` | `004A7950` | `004A7BC0` | `004A7C30` |
| texture `+3` | `004A7EA0` | `004A7FF0` | `004A8250` | `004A84E0` | `004A8560` |

Edge setup takes each value's step per scanline from the reciprocal table
`005415F8` (heights up to `0xA00`; taller edges divide `0x7FFFFFFF`). Clip at
X moves an edge's start to a given X and its Y by the cut fraction; clip
fraction returns that fraction (Q30, saturated); skip rows advances every
value by n steps; load copies a vertex's start. The 16-bit spans read the
surface's channel shifts from renderer `+2C`. The triangle routines turn a
vertex colour into a shade with `004A4B20` (luma `0.299 R + 0.587 G + 0.114 B`
times `0.21875`, at most 31) and `004A4B90` (the shade and the colour packed
into the surface's layout).

This affine native polygon path must not be silently combined with Direct3D
triangle diagonals, reciprocal depth, texture-atlas UV, or device blending.
The Direct3D near ordinary quad now has submission, upload and CPU sampling
owners. Far effect primary fog is owned; other Far families' fog, the color-key
upload family, the extra texture pass, other
primitive families, and exact legacy software spans remain separate
acceptance boundaries.
