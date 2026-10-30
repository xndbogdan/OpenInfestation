# Infestation output presentation policy

This document owns the port's display modes, resolutions, widescreen frame
and output scaling. Native display construction and the Video rows' retail
behavior remain in [frontend runtime](FRONTEND_RUNTIME.md#video-menu), and
gameplay/briefing rendering remain in their subsystem documents. These
choices are port conveniences: retail had no widescreen. An original tier's
own frame on a 4:3 output draws and plays exactly as before. On software
and OpenGL a wider frame (or the Remastered camera) changes only the
picture: the game's draw keeps retail's view and the view shown draws
apart from it; in Vulkan single player the wider view plays the game
([below](#the-games-draw-and-the-shown-draw)).

## Display modes and resolutions

The Video menu's **Display** row offers **In a Window**, **Full Screen** and
**Borderless**; **Resolution** offers the original 4:3 sizes plus every mode
the monitor chosen in the Device row reports, up to its largest. V2000's port
(OpenV2K) offers the same choices.

- **In a Window**: a centred window whose client area is the resolution.
  Its list holds the originals and the reported modes that fit the desktop.
- **Full Screen**: a real mode change to the resolution through SDL's
  exclusive full screen, as retail's DirectDraw exclusive mode was. Only
  reported modes are listed, and the port refuses SDL's closest substitute.
  Leaving Full Screen, quitting or alt-tab restores the desktop mode: SDL
  minimizes the window when it loses focus and restores the mode on return.
- **Borderless**: a borderless window over the whole desktop, without a mode
  change. Its resolution is the desktop's: the row shows that size (such as
  `3840x2160`) as its only entry, so it can't be changed, and the game draws
  the largest original inside it, 1024x768 on most monitors. Leaving
  Borderless restores the saved resolution. V2000's Borderless does the
  same.

[`display_selection`](../../crates/inf-game/src/display_selection.rs) owns
the lists and rules; [`frontend_app::display`](../../crates/inf-game/src/frontend_app/display.rs)
applies them to the SDL window.

- **Physical pixels.** The port asks SDL for per-monitor DPI awareness
  before it starts video, so sizes, modes and windows are physical pixels. On
  a 3840x2160 monitor at 125% scaling the list tops out at 3840x2160, never
  the scaled 3072x1728.
- **Tier.** Every size draws the largest original that fits it, which is the
  largest that fits its 4:3 area: 1280x720 draws the 800x600 tier, 1920x1080
  and above the 1024x768 tier. An original size draws its own tier exactly as
  before. Smaller sizes than 320x240 are not offered.
- **Row values.** The rows keep `00462F40`'s contract over the port's lists:
  `00453B80` arithmetic (previous stops at the first entry, next at the last,
  activate wraps), an unchanged choice returns, a changed one resets the
  display, and a failed reset restores the previous choice. A size between
  entries sits between them, as tier 0 sits before tier 1. The menu lists no
  320x240, because `00462F40` never keeps tier 0: a change made at tier 0
  stores tier 1, and a Display change there stores it without a reset, as
  retail does.
- **Row text.** Original sizes and the two retail Display values draw their
  authored H0 strings (H0 7..11 and 14/15). Other sizes draw `1920x1080` in
  the authored strings' form, and Borderless draws `Borderless`; neither has
  an authored string, in any language.
- **Device row.** A new monitor keeps the choice when it lists the size, and
  otherwise takes its largest listed size below, as the startup search does.
- **Restore Defaults** returns the authored default words (Full Screen at
  640x480) and clears Borderless.

### Settings

Retail words keep retail values: `+40` is 1 for Full Screen and Borderless
and 0 for a window, and `+0C` is the tier the display draws with, the
desktop's in Borderless. Borderless and the exact size are port settings in
the sidecar's schema 6 `display` object (`width`, `height`, `borderless`),
beside completed-frame `scaling`; Borderless keeps the saved size for the
next mode. Loading derives `+0C` from the saved size until startup reads the
desktop.

Older sidecars load straight into this model. Schema 5's `output_presentation`
folds into the resolution: `authored` keeps the `+0C` tier's own size,
`720p` becomes 1280x720 and `1080p` 1920x1080, each keeping its saved Display
and scaling. Schemas 1 to 4 load as the `+0C` tier's own size. The next save
writes schema 6. Partial native writes (Current Display, identity) keep the
last saved display, like every other preference.

### Startup

`00438AD0` searches for a display before the frontend exists. From the saved
choice it tries each tier from `min(+0C, 4)` down to 0, then restores `+0C`,
increments `+40` with wrap and tries again; the renderer and Bilinear values
are outer loops. The port follows the same order over its lists
(`MonitorModes::startup_order`): the saved size and each smaller listed size
of the saved mode, largest first, then the same from the saved size for each
following Display value (Window, Full Screen, Borderless, wrapping).
Borderless has one candidate, the desktop, and keeps the saved size. Its renderer
falls back before its display starts, and Bilinear never changes the
display, so those loops have nothing to vary.

The search stores the chosen words without saving them; the frontend
factory's settings save records them, as in retail. Retail shows a one-time
notice (H0 44, `Infestation cannot run in the desktop. Trying full
screen...`, or H0 45) for its DirectDraw error `0xA08`, which SDL never
raises, so the port's search reports failed candidates only in its log.

The command line chooses from the same lists. `--width`/`--height` replace the
saved resolution with an exact size (at least 320x240) and keep the saved
Display; `--difficulty` selects a tier's original size, and must name the tier
of explicit dimensions. `--scaling four-three|stretched|integer` replaces the
saved scaling. Former `--output-resolution` values are exact sizes now:
`--width 1280 --height 720` or `--width 1920 --height 1080`.

## Native dimensions and output

Native `00437CA0` selects an H5 display record, using settings `+0C`, and
`00437C60` selects its corresponding resources. The supported authored tiers
are 320x240, 512x384, 640x480, 800x600 and 1024x768 (OVL selectors 0..4).
Resident H2 words supply focal inputs for that tier. The native menu, name
entry and briefing use those dimensions for their geometry and clipping.

The window's physical size is the output: the resolution in a window or full
screen, the desktop in Borderless. When it equals the tier's size, frames
draw exactly as on the original display. Otherwise the shared SDL
presentation callback copies the completed native frame into one output
rectangle for every screen, including title, frontend, name entry, intro,
briefing and gameplay. Black bars belong to the output canvas; they do not
replace the native H7/32 post-presentation clear.
The explicit `SoftwareSurface::ClassicFramebuffer` policy lets frontend and
name-entry draw/layout read the CPU surface size while independently validating
the physical window size. At the native size the `Window` policy retains its
surface/framebuffer equality check; a larger output never bypasses window
validation by pretending to be a headless pixel target.

[`ClassicScaling`](../../crates/inf-game/src/presentation.rs) chooses how
the native frame fills the output:

- `widescreen`, the default, draws a wider native frame at the output's
  aspect and fills the output with it; see [widescreen](#widescreen).
- `four-three` uniformly fits the complete 4:3 frame, centered with black bars.
- `stretched` fills the output rectangle, changing the displayed aspect ratio.
- `integer` fits the largest whole-number enlargement; when no positive integer
  factor fits, it uses the same fractional fit as `four-three`.

Final output sampling is nearest-neighbor, independently of native Video's
point/linear model texture policy. Native Video resets replace the OVL tier,
framebuffer and projection together with the window. Display, monitor and
resolution changes are transactional: the physical size is validated, and a
failed candidate restores the prior window, mode and border.

PNG export retains the last native frame, then copies/reads the selected output
once at export so the image includes output bars. Ordinary rendering never
reads a larger output back to CPU for snapshots.

## Widescreen

Under `widescreen` the native frame keeps its tier's height and takes the
output's aspect, rounded to the nearest pixel and never narrower than the
tier ([`NativeFrame`](../../crates/inf-game/src/presentation.rs)). A
1920x1080 or 3840x2160 output draws a 1365x768 frame with the 1024x768
tier's resources, 1280x720 draws 1067x600, and a 3440x1440 ultrawide
1835x768. Presentation fills the output with it; the CPU raster draws it
natively and enlarges it, and OpenGL draws it at the presented size. A 4:3
output keeps the tier's own frame, and an output narrower than 4:3 fits it
like `four-three`. V2000's port does the same with its default Native
scaling.

The tier's H2 words still give the focal pair, while the viewport, its
centre and the startup clip rectangle (`frontend_startup`) are the drawn
frame. The same lens therefore keeps its vertical field of view and sees
further to each side, in gameplay, the engine intro and the frontend's 3D
scenes. What it sees further is the picture's alone wherever the session
plays retail's game (software, OpenGL and network play,
[below](#the-games-draw-and-the-shown-draw)). Most 2D layout follows the surface as retail's does: menu layouts
are authored in a 1024-unit space scaled by the surface's width and height
(`menu_draw`), so rows and boxes spread with the frame while glyphs keep
their size. Retail itself draws that way at its hidden 1280x1024 H5 record.

The vehicle HUD (`0041C4B9..0041EAAC`) places and sizes its gauges, weapon
icons, radar and compass at fractions of the surface width, which at 16:9
pulled each cluster apart and widened its icons. It lays out on the
surface's 4:3 width instead ([`hud_x`](../../crates/inf-game/src/vehicle_hud.rs)):
an element in the left third keeps its 4:3 position, one in the right third
moves by the surface's extra width and the clock, in the middle third, by
half of it. So the primary weapon, the counters and the crystal tally stay
against the left edge, and the health and mode icon, the secondary weapon,
the turbo and the radar with its compass against the right, each cluster
exactly as retail draws it at 4:3.

The guided missile's static (`00456B50`, a disrupted missile's view) draws
three of the process's random numbers for each sprite along every row of
the surface, so a wider frame drew more of them and moved the game on. It
draws retail's across the 4:3 width at the frame's height and the wider
sides from a copy of the random state
([`surface_static_quads`](../../crates/inf-game/src/missile_view.rs)),
so a widescreen frame leaves the random state as retail's surface does.

The player message's two boxes (`00413820`) size their panel by the focal
but place and wrap their text by the surface, so at 16:9 the top-left box's
text ran out of its panel. Each box draws on a 4:3 canvas: the lines box on
the one against the left edge (`SoftwareFrame::left_four_three_canvas`), the
hints box on the centred one.

Two screens lay out at fixed 4:3 positions instead, and draw on the frame's
centred 4:3 canvas ([`FrameCanvas`](../../crates/inf-game/src/software_frame.rs)):
the language carousel, whose flags circle (500, 280) in tier pixels
(`00433494`), and the briefing, whose panes, vignette and globe mix fixed and
scaled placements. While a canvas is set, layout reads its size, clipping
keeps primitives inside it and the flat and textured fills move clipped
fans by its origin, so both devices draw the same frame as a 4:3 display
would, centred; clears still cover the whole frame. The canvas keeps its own
clip rectangle across its draws (`SoftwareScene::enter_canvas`): the
briefing leaves `[0, 0, 1023, 767]`, its whole display, which must not
narrow the next gameplay frame. The briefing's backdrop (`004737A0`'s rotating disc, centred an eighth of
the width left of the display) keeps its 4:3 layout but draws in frame
coordinates with the frame's clip, so it runs on into the left strip
instead of stopping at the canvas edge; the panes, the globe and the text
stay on the canvas. The 16:9 briefing was accepted against a four-three
render of the same Level 1 frame (`--scaling four-three`, replay frame 1700)
on four criteria, all met:

1. No hard edge: nothing stops at the canvas boundary; the backdrop runs on
   to the frame's left edge.
2. Retail's composition: the 16:9 frame's centre repeats the 4:3 frame,
   every pane, the globe and the text at their 4:3 positions.
3. No distortion: glyphs, models and sprites keep their 4:3 size.
4. Consistent sides: the right strip is as dark as the map's own
   surroundings in the 4:3 frame.

The panes therefore stay at their fixed 4:3 positions: spreading them
apart would separate them from their panel frames.

The title video fits at its own aspect with black sides
(`SoftwareFrame::fit_surface`), as a fullscreen video renderer would show
it, instead of stretching.

The object manager `00479570`'s near-callback cone (squared distance below
twice the square of the depth plus extent) is 45 degrees, which matches every
original tier's horizontal field of view because each tier's focal is half
its width. An object's near callback (`+2C`) is game state: it writes the
admission flag `02000000` the updates read, poses, a pickup's collection,
shots and custom vertices, and steps the RNG. So the game's draw keeps the
cone of the game's view: retail's in retail's game, with its DWORD wrapping.
The view shown, and the remaster's game, take the cone of the view drawn
out to its corners ([`ObjectDrawCone::for_projection`](../../crates/inf-game/src/object_runtime/object_draw.rs)):
`1 + (hx/fx)^2 + (hy/fy)^2` times the squared depth, `hx`/`hy` the farther
half-view from the projection's centre across and down and `fx`/`fy` its
focals. A view reaching no further than retail's on either axis (half its
width across at a focal of as much, 3/4 of that down: the Original camera
at 4:3) keeps retail's cone, so what its corners show beyond the cone
stays undrawn, as in retail. The briefing's pane is retail's view on the
4:3 canvas on any frame, and keeps retail's cone.

### The game's draw and the shown draw

Retail runs game logic inside its world draw, so what its view sees is
part of the game: the particle draw flags a particle off the view or past
the fade (bit `0x40`), which the next update destroys; the manager's near
callbacks run on what its cone admits; a model's frustum test and far
cut-off decide whether its opcode 22 jitter draws random numbers; the
sound service starts voices by the eye's range; the terrain's furniture
walk follows its footprint. Over a Tabuchi replay
`run-20261028-181209` (2026-10-28, every random draw logged by call site)
each presentation input parted the game from the Original camera at 4:3:
the widescreen cone at frame 2939, the Remastered focal at 3031 (the
factory's chimney passing the frustum test at focal 430, not 512), the
Remastered eye at 4904 (a particle one unit nearer the view's edge), the
tracer's longer draw distance at 6017, and Vulkan's underwater look at 7147.

Which game a session plays (`presentation::GameView`) is fixed when its
window opens and its replay records it, with what that game reads of the
machine (`presentation::SessionGame`, the replay's `g` line, as
`g remaster-traced 1365x768 rows 1111111 far 1,1.15 18:1.5,1.15`): the
frame the window opened with, what the device ran of the Vulkan rows (the
Options cursor skips the rows it locks) and, traced, each world's fog
`far` and `past`. A replay plays the recorded game on any renderer: the
Vulkan rows show for the remaster's game and lock as recorded, the traced
game sees by the recorded reach, and the remaster's game draws at the
recorded frame (said on stderr) while this run's display gives the frame
its window opened with; another tier's height, or a display change to
another frame, is said too, as the game may part from the recording there.
A replay older than the line plays the game of the renderer replaying it.
Vulkan single player always plays the remaster's game, whatever the
camera style (objects at a widescreen picture's edges were confirmed
fully alive there, 2026-10-29); on the software raster and OpenGL the
Remastered camera's sound comes from retail's camera (below; judged
fine as is, 2026-10-29); and Ray Tracing switched in game changes the game
only at the next session (current behaviour).

- **Retail's**: the software raster, OpenGL, a Vulkan choice that fell
  back to OpenGL, and every network game (the process's network flag set)
  on any renderer. The game's view is retail's: the camera's own eye, the
  template's focal slewed with no speed kick (`world.view_focals`), a 4:3
  viewport the frame's height (`[min(w, h * 4/3), h]`), the header's fade,
  retail's cone and retail's `0xC8000` arena.
- **The remaster's**: Vulkan single player, plain or traced. The game's view
  is the view shown as any renderer would draw it: the Remastered eye and
  focal (its speed kick in it), the frame's viewport and cone, the widened
  arena, the header's fade under the water (no renderer's underwater
  look), and the tracer's longer draw distance only where the session
  started traced (Ray Tracing switched in game changes the picture at once
  and the game at the next session; until then the world draws twice).

Each world frame builds both views (`world_scene::world_views`): the
game's, and the frame's own with its own focal and fade slews
(`world.shown_focals`, the fade's second pair), so the scope and the dock
ease on screen as before.

- **One pass** where the view shown plays the game's (`ViewSpec::plays_as`:
  the same eye, focals, viewport, fade, cone and arena; Vulkan's murk counts
  as the view above the water): most frames of the Original camera at 4:3
  on any renderer, and the remaster's game drawn as it plays. A few frames
  there draw two passes all the same: one whose focal's decay overshoots
  on leaving the scope or the dock (its view sees past retail's cone), and
  a traced network frame (its view sees by the tracer's reach). On those
  only the game, not the picture, is the single-pass build's byte for byte.
- **Otherwise two.** The game's draw runs on the live game and shows
  nothing (`SoftwareScene::set_game_draw`: its own command queue, flushes
  that replay without drawing, no depth clear, no device feed or motion,
  the game's viewport as the clip, 4:3 in retail's game). Its lines are the
  game's own; the view shown's begin `World view shown` and keep their own
  once-a-world keys, so a stop grep counts the game's alone. Then the view shown draws apart from the game, on copies
  of all it could write (`GameAsDrawn`): the RNG, arena, tint, mission
  save and statistics, world start, creation listeners, flash, light delta,
  the ground's furniture buckets and plane, the object process state,
  particles, light grid, furniture bases, sounds, weapon tables, map
  jitter, the scene's process globals with the textures' sort ordinals,
  and every byte of the model scratch. Its runtime allocations refuse
  writes (an object whose draw meets one is left out of the picture, logged
  once per distinct error as `World view shown ... omitted, its writes put
  back`), it makes no particles, writes no event line and serves no sound.
  The originals are then put back and the copies dropped, so a copied
  network menu never closes the session.
- **What the view shown draws.** The game as the game's draw left it, one
  draw ahead: the shots that draw fired, the pickups it collected and the
  particles it made are the frame's. Its cone is its own view's, so objects
  at its edges draw whole, their models, the particles they have and their
  lights. A particle retail's view turned away dies at the next update, as
  in retail, though the view shown still sees it. Objects only a wider
  view sees (widescreen's side bands) draw from those copies, so their
  own per-draw animation, particles and sounds stand still there: the
  user's choice on software and OpenGL (2026-10-29: "if user picks
  widescreen resolution then have them see frozen side objects, it's
  their problem. Retail never handled widescreen so this is how we cope
  with it on OpenGL and software").
- **What reads the game's view.** The receipt (`world.last_draw`) keeps the
  view shown, which the HUD's reticle projects through, and carries the
  game's eye, projection and outcome: the carried actor's draw (a near
  callback on the live arena) and the status line's eye read those. As
  built, sound pans and fades from the game's camera, so under the
  Remastered camera on software and OpenGL it is heard from retail's.

An Altair deep-water drive (3,000 frames at 16 ms) wrote byte-identical
event logs at Camera Style 0 and 2 at 1024x768 four-three on the software
raster, style 0 at 1920x1080 and style 2 at 3840x2160 widescreen on OpenGL,
and style 0's matched the single-pass build's byte for byte; a
replay on OpenGL at styles 0 and 2, four-three and widescreen, gave the
same `[status]` RNG and eye lines in all four. Four Original-camera 4:3
start states (two at Tabuchi, Pelagia, Stendahl) draw pixel-identical to
the single-pass build.
[`the_view_shown_draws_apart_and_leaves_retails_game_as_one_pass_leaves_it`](../../crates/inf-game/src/world_scene_tests/shown_draw.rs)
draws two Tabuchi loads with their objects' lights (the terrain's dynamic
lighting off) and a sound host (a one-shot
queued each frame), one Original at 4:3 and one retail's game with a 16:9
Remastered view shown from an eye moving ahead over ten frames (a widened
arena, a longer fade with the wet look, Vulkan's murk, infrared vision, a
network game's initials and team), then an arena that fills at the objects
and one that cannot be made, and compares their games after every frame:
the world's and the lent owners' state, the one-shots played and every
scene global the shown pass puts back. The tracer and a live network
session need a device and a peer the test has not. The cone's and the refused
writes' unit tests are in
[`object_draw/tests/cone.rs`](../../crates/inf-game/src/object_runtime/object_draw/tests/cone.rs).

It replaced the *view band* (2026-10-28), which kept the game's draw on
the frame's own view and widescreen cone and drew only the objects the
Remastered view showed beyond that cone apart from the game: the game
still moved with the frame's focal, eye, width and fade.

Model opcode 07's edge expanders (`00495CA0` near, `004961A0` far) reject an
edge unless all four corner X words lie inside `-0x3FF..0x3FF`
(`00496089..004960AE`, `004965B4..004965D9`). The bound is the right edge of
retail's widest frame, 1024x768, so a 1365x768 frame lost every edge with a
corner in its right quarter while the same edge drew on the left: a Level 1
drive dropped thousands of edge quads a run (sprite materials 275, 276, 517,
1391 and 1393 among them). A frame wider than 4:3 moves the right bound out by
its width beyond the 4:3 centre, `0x3FF + width - height * 4 / 3`, 0x554 at
1365x768 ([`edge_right_bound`](../../crates/inf-game/src/model_primitives/edge.rs));
a 4:3 frame keeps retail's window.

The world frame's command arena grows with the frame too. Retail's arena
preamble `00439270` gives each frame `0xC8000` bytes, sized for its 4:3
view, and an overflow raises error `0x301` (`004A42B0`, which commits
nothing on that path). The terrain draw `0044AA10` runs each 14-row strip
in its own error frame, which disposes any error (`004923C0`) and goes on to
the next strip; elsewhere in the world draw `00488020` the error reaches
`00440F00`'s frame, which takes only `0x301` (any other code is raised
again, to the session's fatal handler) and skips the rest of the world
draw. Either way `00440F00` then flushes (`004A4460`) what the frame had
queued, so a full arena loses the strips that did not fit, mostly far
terrain, and not the frame. At a fast machine's full far
distance (`0x3100`) with fine terrain (Detail Level 1), the port's 4:3 frame
on a drive from Tabuchi's start toward the water peaks at 699,168 bytes, 85%
of the arena. The same frames at 16:9 keep more terrain tiles on screen
(7,370 terrain quads at the peak against 5,877) and overflowed. A widescreen frame
therefore draws its view shown in `0xC8000 * width / (height * 4 / 3)`
bytes ([`world_arena_bytes`](../../crates/inf-game/src/world_scene.rs),
which also grows with the Remastered camera's narrower focal), 1,092,000 at
1365x768, where the same drive peaks at 79%. An overflow outside the
terrain skips the rest of the world draw (particles, flash, sound,
terrain, furniture), so the arena is game state: retail's game draws its
4:3 view in retail's arena, and with it retail's overflow, whatever the
frame shows; the remaster's game draws in the widened one.
[`tabuchi_water_view_fits_the_widescreen_arena_and_overflows_retails`](../../crates/inf-game/src/world_scene_tests/arena.rs)
draws the heaviest frame of that drive at 4:3 in retail's arena, at 16:9 in
retail's (which overflows) and in the widened one, and in retail's game at
16:9, whose game's draw keeps the 4:3 view in retail's arena while its view
shown gets the widened one.

The port used to lose the frame: the draw queue and the scene stopped at the
overflow and refused the flush, so only the sky, flushed before the world,
showed. Altair Prime at Detail Level 1 did it near its surface warpgates
(155 of 1,500 frames of a drive at 4:3, 18 at 16:9). The queue now resumes
after a caught overflow ([`DrawQueue::recover_overflow`](../../crates/inf-game/src/draw_queue.rs)),
a terrain strip's omission disposes it as `0044AA10`'s frame does
(`SoftwareScene::recover_native_301`), and the world draw keeps `00440F00`'s
catch: an overflow outside the terrain skips the rest of the world draw, and
the batch is flushed after it.

The sidecar's schema 8 makes `widescreen` the default (on software and
OpenGL a picture's choice; under Vulkan single player the frame the
remaster's game plays). Before schema 8,
`four-three` was the default fit, so an older sidecar's `four-three` loads
as `widescreen`; `stretched` and `integer` stay as chosen, and a schema 8
`four-three` is a choice that stays.

The screens were checked in a 1920x1080 window with `--renderer software`:
the title, the language carousel, the root, Options, Video, Sounds and
Controls menus, the profile menu, name entry, the loading screen, the engine
intro, the Level 1 briefing, Level 1 gameplay before and after a pause, and
Tabuchi's start turning toward the water with fine terrain at a 31 ms step.

[`widescreen_frames_fill_the_window_on_both_devices`](../../crates/inf-game/tests/opengl_above_native.rs)
runs the startup carousel, Options after a Video reset, the engine intro,
the Level 1 briefing and Level 1 driving in hidden 1280x720 and 1920x1080
windows with both devices: each draws the widescreen frame (1067x600 and
1365x768), gameplay and the intro reach both edges of the window, the
briefing's sides stay clear, and under each native pixel's centre OpenGL
differs from the CPU raster by more than 24 levels at no more than 1.4% of
pixels (Level 1 driving), as at native size. Canvas, clip and cone policies
have unit tests (`software_frame`, `software_scene`, `object_draw`).

## Renderer

`--renderer auto|opengl|software` chooses the device that draws the native
frame; see [the OpenGL device](PIXEL_RUNTIME.md#opengl-device). Retail has no
setting for it: it always draws through Direct3D. The choice is a port option:
the [launcher](../../README.md#pre-game-launcher)'s Options save it in the
settings sidecar (schema 9), and `--renderer` replaces it for one run.

- With the CPU raster (`software`, or `auto` when OpenGL cannot start), the
  window is an SDL software canvas and the completed CPU frame is copied into
  the output rectangle, as above.
- With OpenGL (the `auto` default), the device draws the native frame into an
  offscreen framebuffer of its presented size: the output rectangle the
  scaling chooses, which is the native size at an original resolution.
  Presentation copies it one to one with black bars, and a shown window
  presents on vertical blank, as retail's page flip did. A hidden window
  never waits for a display.
- A snapshot remembers the frame each presentation drew: native CPU pixels,
  or the OpenGL frame at its drawn size. Export to a larger output places it
  with bars; SDL's 16.16 nearest steps enlarge CPU pixels, and the OpenGL
  frame is copied one to one.

### The tracer's models outside the view

Under path tracing (Vulkan, any game) the objects the view does not show,
and in first person the player's own craft, are drawn for the tracer
alone, so that their shadows, reflections and bounce light reach the view
(the [full path tracing plan](../rt-plans/full-path-tracing.md)'s
stage 8).
No renderer shows them: the software raster, OpenGL, Vulkan's plain raster
and the tracer's own view draw only what retail's draw admits, and retail
draws no hull in first person.

- **Which objects.** The view shown's object pass (the frame's one pass,
  or the view shown apart from the game's) lists those inside its radius
  (their extent plus the far distance, and the ground's excess) whose
  `+2C` can draw but whose centre its cone turns away: behind the eye,
  beside the view
  ([`Flag0ObjectDrawOutcome::culled`](../../crates/inf-game/src/object_runtime/object_draw.rs));
  beyond the radius the manager considers nothing, and neither does this.
  Of those the tracer takes the ones that can reach its view
  (`world_scene::reaching_view`): within 1,024 units of the eye past their
  extent (their bounce light, a metal's reflection of them, a lamp's
  shadow), mirrored into the view by the water's plane, or whose bounds
  (a sphere of their extent) swept along the sun's light until wholly
  under the ground under that line meet the view's frustum. A shadow
  thrown down past a cliff's foot well beyond its extent is missed.
- **The craft in first person.** Where the first player's craft's view
  mode `+D0` is above 0, `00420510` draws no hull (its late model and its
  fire only). The tracer takes it too, its mode cleared on the copy, so its
  draw lays the hull as from behind
  (`ObjectArena::show_first_person_hull`).
- **How they draw.** Last of the world draw, after the terrain, on copies
  of all they could write, as the view shown is drawn apart from the game
  (`GameAsDrawn`: runtime allocations refusing writes, no particles, no
  event line, no sound, everything put back): each one's `+2C` admitted as
  the manager admits one (`ObjectArena::draw_near_callbacks`), inside
  [`SoftwareScene::feed_only`](../../crates/inf-game/src/software_scene.rs).
  Their models pass no outer frustum gate (`0049FEF0`'s), and choose their
  near or far table and meet the far cut-off (`004A00E0`'s) by their
  distance from the eye, as the eye turned toward them would, not by their
  depth along the view, which behind the eye is negative and would take
  the near table past every cut-off (`ModelTransform::at_distance`). The
  feed marks their faces; what they queue for the raster goes to an arena
  of its own, dropped with any stop it latched, and the gates' lightning,
  flares and fields they lay are dropped too, so the raster draws nothing
  of them; so are their exhausts and wheels, with which the hull would
  lay the craft's trails in the mist and a wake by wheel in first person,
  where retail's draw lays neither. A failed set-aside is said once
  (`World outside the view stopped, its writes put back: ...`), as is each
  object left out (`World outside the view ... omitted, its writes put
  back`); neither is a game stop.
- **In the tracer.** Their faces sit in bottom levels of their own (opaque,
  and masked sprites) under the instance mask `MASK_OUTSIDE`
  ([`vk_device/trace.rs`](../../crates/inf-game/src/vk_device/trace.rs),
  `pt_scene.glsl`), their records after the view's of their kind, the
  instance's record offset holding the view's count (as the ground's tiles
  hold their first triangle). Shadow, mirror, bounce and light rays, the
  mist's sun shadows and the lit translucents' shadows meet them; the
  eye's rays, the depth pass and the eye's view on through the water
  (refraction from above and, with the eye under it, up through the
  surface, and the sunk glows) do not, so the view shows the models
  retail's draw admits and nothing more. The radiance cache leaves them
  out: its paths' rays pass them and no path feeds a cell from a hit on
  one (their instances' custom index carries a bit beside their kind), so
  they reach the view through its own hits' rays alone. Caught in the
  cells round the eye, the hull, which moves with it, showed its shade
  as a darker square a cell wide on the ground of a daylight turn. An
  object the cone turns away can stand round the eye (an earlier try,
  `infestation-pt-culled-models`, found the eye's rays meeting the walls
  of lava's factory dock round a camera behind its centre, and the frame
  black), as the craft does in first person. Where one of their faces
  lies on the view's copy of the same corners (an actor that fired, its
  late model drawn in view), the view's stays
  (`SceneFeedBuilder::settle_faces`). Their sunk glows are left out.
- **What it holds back.** No particles, sounds or late model of theirs.
  Terrain furniture outside the view (the terrain draw's strips,
  `0044AA10`: the fuel silos and the rest of the scenery) is not the object
  pass's, is not drawn and stays out of the tracer.

[`the_tracer_draws_outside_the_view_and_leaves_the_game_as_it_found_it`](../../crates/inf-game/src/world_scene_tests/shown_draw.rs)
draws Tabuchi twice, both collecting their feed as a traced frame does,
one taking nothing from outside its view: from an eye with an object
behind it, then from behind the first player's craft (given the buggy's
form: a loaded world's craft has none yet and draws nothing), then with
the craft in first person and the eye in it. After every frame both
games (every owner `GameAsDrawn` keeps, the one-shots played and the
scene's globals) and both pictures are the same, and the second's feed,
as its frame ends, holds faces of the object and then of the hull, more
of them in first person than from behind the craft, and the first's
wheels and exhausts alone (a draw that stalls, or the hull's wheels
kept, fails it).
[`the_tracer_keeps_the_models_outside_the_view_that_reach_it`](../../crates/inf-game/src/world_scene_tests/shown_draw.rs)
pins the selection, and
[`vulkan_models_outside_the_view_light_it_unseen`](../../crates/inf-game/src/software_device_tests.rs)
traces them behind the view's own triangles of their kind: a masked panel
keyed half transparent shadows only under its solid half, a green panel
behind the eye tints a shaded wall, a panel before the eye leaves the eye
its view.
Against main's build (`7c345b333`), a Tabuchi replay traced (the
remaster's game) gives the same `[status]` RNG and eye lines in all 12,
and a traced 600-frame drive in first person, firing, writes the same
event log (every random draw, spawn, damage and sound; the ambience's
beds follow the audio clock and are left out of the comparison), as
three more runs did before the rebase. The pass costs at most 0.06 ms
of CPU a frame for the models outside the view, and 0.75 to 0.86 ms for
the hull in first person, about what the craft's draw costs in third
person; at 4K the GPU frame is 0.5 to 3% dearer than main's in the path
tracing plan's scenes (about 0.2 ms of the tracer's, from its larger
shaders, where nothing lies outside the view) and 4 to 7% in first
person, still cheaper than third person (the
[plan's stage 8](../rt-plans/full-path-tracing.md) has the
table).
What shows (4K against main's build, both giving the same frames run to
run): in first person at night with the lamps on, the bonnet and the
wheel arches shadow the headlamp pool's edges, as third person shows
them (308,000 pixels by more than 2 levels, 96,000 by more than 8, at
most 45); in a daylight first-person turn, 109 pixels, a strip the
craft's own glow lights. By day the hull's shadow falls within about 70
units of it while the view's ground starts about 160 ahead, and a level
mirror shows only what is above the view, so its reflection in level
water is out of view. The models outside the view show little in third
person (1 and 162 pixels, the same turn and the night): the suns stand
38 to 74 degrees high and the models behind the eye are mostly far.

### Rendering above native resolution

Native tiers end at 1024x768. When the output is larger than the tier, the
OpenGL device draws the same frame at the output's own pixel density (user
decisions, 2026-10-09), so geometry, textures and text cover the presented
rectangle's pixels. Projection, layout, clipping and the native framebuffer
size stay authored:
this is not widescreen and changes no camera. The CPU raster stays native,
enlarged by presentation, and remains the reference.

- The vertex shader maps a native TL position with `u_scale = 2 / native
  size` and a +0.5 pixel-centre shift into whatever viewport it draws, so
  native coordinate `x` lands at target coordinate `(x + 0.5) * k` and native
  pixel `i` covers target pixels `i*k .. (i+1)*k`. [`GlFrame`](../../crates/inf-game/src/gl_device.rs)
  keeps the native size for that and for surface uploads; its colour texture,
  depth buffer and viewport take the target size. `stretched` gives different
  X and Y factors.
- `SoftwareFrame::width` and `height` stay native for layout, clipping and
  `SoftwareSurface` validation; `read_size` reports the drawn image's size.
- `Output::frame` chooses the target at startup and at each native Video reset
  from the requested output, not the window being replaced, and the scaling:
  `ClassicScaling::destination(native, output)`. An original size therefore
  draws exactly as before.
- Surface replacement (the title video) uploads native pixels and the device
  enlarges them with nearest sampling.

The internal resolution is not a separate option: it follows the presented
size, so an original resolution keeps native drawing.

Accepted with [`opengl_above_native`](../../crates/inf-game/tests/opengl_above_native.rs)
on an RTX 5080 (OpenGL 4.6, driver 617.14): one game state through
`--renderer software` at 800x600 and 1024x768 and through OpenGL in 1280x720
and 1920x1080 windows, which draw those layouts, in all three scalings, on
the startup screen, Options after a Video (Colour Depth) reset, the engine
intro, the Level 1 briefing and Level 1 driving.

- Sampled under the native pixel centres, each OpenGL frame differs from the
  software frame by more than 24 levels at no more than 1.6% of native
  pixels (Level 1 driving at 1280x720, four-three): polygon and texel edges
  that the finer coverage moves. Integer output whose target is the native
  size matches as at native.
- Inside regions the software frame paints uniformly (3x3), drawn pixels
  differ only where geometry narrower than a native pixel now draws: the
  intro's antennas and the briefing globe's grid lines become continuous.
  Text, menus, the HUD and briefing panes show no such pixel, so no seams.

```sh
SDL_AUDIODRIVER=dummy cargo test --release -p inf-game --test opengl_above_native -- --ignored --nocapture --test-threads=1
cargo test --release -p inf-game --lib -- --ignored software_frame::device_tests frontend_app::display::tests::opengl_output
```

## V2000 comparison and limits

The V2000 port offers the same Display and Resolution choices, with its own
retail words and startup search (see cross-game parallels).
Its `ScalingMode` distinguishes Native, 4:3 and Stretched, as `ClassicScaling`
does here. Its authored tier selection and live display code also relayout
HUD/menu submissions, retain tier resources and adapt world projection. The
following source was inspected read-only; it was not copied as Infestation RE
authority, and Infestation's own rollback design was only a reference for
it:

- V2000 config
- V2000 renderer contract
- V2000 OpenGL backend

The local V2000 UI mapping, live-display and system-layout changes were also
inspected. Their native relayout behavior is a design reference, not proof that
Infestation's coordinate and clipping consumers are equivalent.

Infestation's renderer is [`SoftwareScene`](../../crates/inf-game/src/software_scene.rs)
over a [`SoftwareFrame`](../../crates/inf-game/src/software_frame.rs),
with recovered Direct3D pixel, clipping, texture and depth policies. The
frame's pixels come from the CPU raster or from the
[OpenGL device](PIXEL_RUNTIME.md#opengl-device), which takes V2000's pattern
(OpenGL 2.1 through SDL, software when it cannot start) but not its code: it
draws Infestation's own clipped Direct3D submissions. With the CPU raster,
increasing only output dimensions preserves raster cost, although SDL must
copy more output pixels. The CPU raster stays selectable beside OpenGL; see
renderer backends.

[Widescreen](#widescreen) widens the frame and the world lens and centres the
two fixed-layout screens on a 4:3 canvas; it adds no OVL tier. V2000's port
also scales its HUD up above 1024x768 for readability; Infestation's HUD
keeps its tier's pixel size.

## Validation

Pure policy tests cover the lists of each Display value (sizes that fit the
desktop, reported modes only, layouts), the tier rule, `00453B80` stepping
over a row including sizes between entries and tier 0, the port's row text,
Borderless drawing the desktop's tier while keeping the saved size, and the
`00438AD0` order. Settings tests
cover schema 5 folding (`authored`, `720p`, `1080p`), schema 1..4 loads,
retail words, partial writes, Restore Defaults and invalid fields. Startup
tests cover explicit sizes and tiers; scaling tests cover every authored
tier, fitting, stretching, integer enlargement and smaller-output fallback.

The explicit SDL tests (release, `--ignored`) passed on a 3840x2160 monitor at
125% scaling:

- 1280x720 and 1920x1080 windows in every scaling, against colored-quadrant
  pixels and exact opaque bars, keeping the 640x480 texture; the OpenGL
  output draws the same frames at their presented size.
- A visible Borderless window covers the physical 3840x2160 desktop without a
  mode change, and the monitor's largest reported mode is that desktop.
- A visible Full Screen 800x600 switches the monitor to 800x600; an
  unreported 801x600 fails before touching the window; an invalid window
  leaves exclusive mode and restores it; a window restores the desktop.
- The hidden Video replay walks Resolution from 640x480 to the next listed
  window size and back over six resets, persisting the size and its tier;
  relaunches restore it, or the explicit 640x480.
- A 1280x720 window steps to the next listed size, survives a saved restart,
  draws the 800x600 tier natively again with `--difficulty 3`, and name entry
  renders in a 1920x1080 window with the 1024x768 tier.
- OpenGL follows that reset like the CPU raster, within 2% of native pixels.
- Borderless over the 3840x2160 desktop draws the 1024x768 tier; at integer
  scaling it matches every RGB pixel and opaque bar of a 1024x768 window in
  the engine intro and the Level 1 briefing, with unchanged native draw
  counts and camera.
- Full Screen startup at 800x600 presents through the exclusive surface, and
  the Display row returns it to a window and saves `+40 = 0`.

Each mode also ran on that monitor through the real menu, with OpenGL:

| Display | Resolution | Result |
|---|---|---|
| In a Window | 800x600, 1920x1080, 3840x2160 | Centred client of that size; the 3840x2160 client covers the desktop with its caption above it. The monitor stays 3840x2160@240Hz. Alt-tab leaves it in place. |
| Borderless | 800x600, 1920x1080, 3840x2160 saved | A popup over the 3840x2160 desktop, no mode change. Alt-tab leaves it in place. These runs predate the desktop-only Resolution row; since then it draws the 1024x768 tier whatever the saved size, and the Video menu reads `3840x2160` and `Borderless` (checked through the game's own snapshot). |
| Full Screen | 800x600, 1920x1080, 3840x2160 | The monitor switches to that mode at 240 Hz. Alt-tab restores 3840x2160 and minimizes the game; returning switches back. Quitting, or the Display row's In a Window (a centred window), restores the desktop. |

From Borderless, the Display row also switched live into Full Screen 800x600
and then into a window. Screen captures of a borderless OpenGL window can be
stale under flip presentation, so those checks used the game's own snapshot.

Three window smokes had kept expectations from before later gameplay work, and
failed the same way without these changes; they now follow it. Level 1's
first tutorial hint pauses the game in phase 6 and returns to phase 4
([selector 50 hint triggers](PLAYER_COMMAND_RUNTIME.md#selector-50-hint-triggers)),
loading presentations clear frames like any other presentation, and the
post-intro loading model draws whole since its register 3 starts from
Infest01's stack
([LoadingModel117StackHistoryApproximation](MODEL_RUNTIME.md#loadingmodel117stackhistoryapproximation)).
Two display tests also count the frames a present title video holds before
Return skips it.

```sh
cargo test --release -p inf-game --lib -- --ignored frontend_app::display::tests
SDL_AUDIODRIVER=dummy cargo test --release -p inf-game --test frontend_app_window -- --ignored --test-threads=1
SDL_AUDIODRIVER=dummy cargo test --release -p inf-game --test opengl_above_native -- --ignored --nocapture --test-threads=1
```

The Full Screen cases change the monitor's mode; run them only when that is
acceptable. These checks validate output adaptation, not retail presentation
acceptance.
