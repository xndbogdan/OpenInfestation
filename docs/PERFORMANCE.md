# Performance of the traced (Vulkan) game

Measured 2026-10-26 on an RTX 5080, 1920x1080, the default settings (DLSS
Ray Reconstruction at Quality, so a 1280x720 trace), release build.

## How to measure

- Every Vulkan run prints, when it closes:
  - `Vulkan tracer:` and `Vulkan tracer stages`: the traced frame's GPU
    time by stage (textures, geometry uploads and acceleration-structure
    builds, the sky's clouds, the trace, the denoiser, the resolve, the
    anti-aliasing, the upscaler, the mist, the composite), mean and most;
  - `Vulkan frames`: per frame, its time, the CPU's share, the time the
    CPU waited for the GPU, the GPU's time over all of the frame's
    submissions, the swapchain's acquire and present, and the trace's CPU
    recording (`vk_device/perf.rs`).
  Both leave out the first 30 frames, which load the world.
- `INF_CPU_PROFILE=1` adds a table of named CPU parts per frame
  (`cpu_profile.rs`: the frame's passes, the world draw's phases, model
  execution, the terrain's furniture and water).
- `scripts/perf-scenes.ps1 -Settings <file> -Profiles <file>` times ten
  traced scenes (Pelagia in rain, dry, at night, in infrared and driving;
  Terra Nova, Stendahl Alpha, Artoria, Foldrak Noir at night, a lava world).
  With `-Snapshot` each run also saves its frame ten before the last, for a
  picture check against an earlier build's (a snapshot run now times like a
  plain one: below, item 9).
- A change meant to keep the game's look is checked by its snapshots of the
  same scenes against a baseline build's: identical, but for Pelagia in the
  rain's sky band, whose clouds differ by up to 2 levels from run to run on
  the same build.
- Sampling without administrator rights: build for MSVC with line tables
  (an import library for MinGW's `SDL2.dll` made with `dumpbin /exports` and
  `lib /def`), attach Visual Studio's standard collector
  (`VSDiagnostics.exe start 1 /attach:<pid> /loadConfig:...\CpuUsageHigh.json`),
  and read the `.etl` in the `.diagsession` with
  `xperf -i <etl> -symbols -a profile -detail` (hotspots) or
  `-a stack -butterfly` (callers and callees), with `_NT_SYMBOL_PATH` on the
  PDB's folder and Microsoft's symbol server.

## OpenGL and the Vulkan raster at 4K (2026-10-30)

Release build of `5c8cb53c8`, RTX 5080, 3840x2160 native (no upscaler),
`--hidden` (OpenGL presents without vertical sync there), the `t6` scenes'
starts and keys, the same settings with `--renderer opengl`, or Vulkan with
path tracing off and the upscaler off (SMAA). The machine was otherwise idle
(GPU 10%, CPU 7%).

OpenGL prints no frame timing, and the wall clock of short and long runs
differs too much in load time to subtract, so both renderers are timed by
`--session-log`'s `[status]` line over frames 600 to 1200 of a 1200-frame
run (the drive's keys held to frame 1250, not 400). The Vulkan split is
`Vulkan frames` over frames 30 to 1200 of the same runs; its frame mean agrees
with the status rate within 0.3 ms. OpenGL gives no CPU/GPU split.

| Scene | OpenGL ms | OpenGL fps | Vulkan ms | Vulkan fps | Vulkan CPU | Vulkan GPU |
| --- | --- | --- | --- | --- | --- | --- |
| Pelagia, rain | 12.9 | 78 | 12.4 | 81 | 12.7 | 4.2 |
| Pelagia, dry | 12.9 | 78 | 11.6 | 87 | 12.0 | 2.8 |
| Pelagia, night | 8.3 | 120 | 7.8 | 128 | 7.9 | 2.2 |
| Pelagia, driving | 9.8 | 102 | 10.4 | 96 | 11.0 | 4.8 |
| Terra Nova | 12.7 | 79 | 9.6 | 104 | 9.9 | 2.0 |
| Stendahl Alpha | 9.5 | 105 | 9.1 | 109 | 9.0 | 2.5 |
| Artoria | 5.7 | 174 | 5.2 | 192 | 5.3 | 2.3 |
| Foldrak Noir, night | 9.1 | 110 | 7.8 | 129 | 7.9 | 1.3 |
| Lava world (17) | 10.8 | 93 | 9.3 | 108 | 9.5 | 2.9 |

At 4K both renderers run 78 to 190 fps and are held by the CPU, not the GPU:
the Vulkan raster's GPU takes 1.3 to 4.8 ms a frame, its CPU 5 to 13 (the
world draw, `frame: linked present`, is nearly all of it). OpenGL is level
with Vulkan or up to 3 ms slower (Terra Nova). Runs in `.tmp/perf-4k`.

## Retail's CPU against the OpenGL port's (2026-10-30)

Terra Nova from its first start, standing, 16 ms steps (one world tick and
one draw a frame on both sides), Detail Level 1, retail camera.

- Retail: the scenario lab (`run.py retail --once`, no viewer) at 640x480.
  The stub drops every `DrawPrimitive`, so this is retail's tick, model
  programs, terrain and its own transform, without Direct3D's clipping, the
  driver or rasterising. Two runs quitting at pass 1800 and 3000 (world from
  pass 1708) took 0.89 s and 3.35 s: **2.05 ms a frame**, the lab's hooks and
  event log included.
- Port: the 04:51 release exe, `--renderer opengl --hidden`, the `t6`
  settings, `[status]` frames 600 to 1200. 3840x2160: **12.9 ms**; 640x480:
  **8.4 ms**; 3840x2160 with the Remastered camera: 11.4 ms. Sampled with a
  stack-walking sampler (`.tmp/perf-port/sampler.py`: suspends the thread,
  dbghelp `StackWalk64` over `.pdata`, names from `nm`; ~110 samples a
  second, 834 at 4K and 559 at 640, so about +-2 points), which left the
  frame time unchanged.

At 4K the widescreen scaling makes the view shown (1365x768) differ from
the game's (1024x768), so the world draws twice (`draw_world_views`): the
game's pass, which shows nothing, then the view shown under
`refusing_writes`. At 640x480 the two are one pass.

| Part (share of the main thread) | 4K % | 4K ms | 640 % | 640 ms | Whose |
| --- | --- | --- | --- | --- | --- |
| Tick (`first_pass`) | 2.0 | 0.3 | 2.5 | 0.2 | retail's |
| Game's pass (models, terrain, objects; no pixels) | 33 | 4.3 | - | - | retail's |
| One pass's models and terrain, without the pixel backend | - | - | 33 | 2.8 | retail's |
| View shown, second pass (all of it) | 65 | 8.3 | - | - | port-only |
| of which pixel backend (`SoftwarePixelBackend::draw`) | 28 | 3.6 | 59 | 5.0 | port (D3D's part in retail) |
| of which `clip_polygon` | 11 | 1.4 | 29 | 2.5 | port (D3D's part in retail) |
| Heap allocation and free (`ntdll` self) | 11 | 1.4 | 29 | 2.5 | port-only |
| `flash::record_primitive_memory` (both passes) | 4.4 | 0.6 | 4.1 | 0.3 | port-only (byte-exact record) |
| `TextureIdentities::resolve`, `resolve_cut` | 5.6 | 0.7 | 2.9 | 0.2 | port-only |
| `model_scratch` slot reads (`read_slot`, `slot_offset`) | 6.1 | 0.8 | 3.2 | 0.3 | port's form of retail's scratch |
| GL device (`GlFrame`, `nvoglv64`) | 3.2 | 0.4 | 6.4 | 0.5 | port's submission |
| `GameAsDrawn` copies, session log, hashing | <1.5 | <0.2 | <1 | <0.1 | port-only |

So retail's own work, reproduced, is 3 to 4.5 ms a frame against retail's
2.05 (its game pass at 4K, the 640 pass less its pixel backend); the rest is
the port's: at 4K the second pass of the view shown (8.3 ms, two thirds of
the frame), at 640 the pixel backend (5 ms), and in both heap churn, nearly
all of it `clip_polygon`'s two `Vec`s per polygon and the `Vec<Vec<_>>` of
clipped polygons its callers build. Particles, the sky, the object light pass
and present are under 1% each.

Suggested fixes, none made:

1. `clip_polygon`: accept a polygon inside every bound as it is, and clip in
   two stack arrays of 12 (`D3dViewportError::Capacity` already bounds them);
   the callers fill one reused flat buffer. Most of the 1.4 to 2.5 ms.
2. The view shown's second pass: share the game pass's model execution when
   only the viewport differs, or draw the game's pass at the shown aspect
   when the camera is retail's (needs the draw-time writes proved equal,
   `infestation-draw-feeds-simulation`). Up to 4 ms at 4K.
3. `record_primitive_memory`: skip it in the view shown's pass, whose state
   is put back anyway.
4. `TextureIdentities::resolve` and `resolve_cut`: resolve once per texture
   a frame, not per primitive.
5. `PrimitiveCommand::new`, `DrawQueue::allocate`, `from_iter`: reuse the
   frame's buffers instead of allocating per primitive.
6. `model_scratch` reads: precompute slot offsets per program run.

## The CPU trim after it (2026-10-30)

Items 1, 4 and part of 5 of the list above, on branch `infestation-cpu-trim`:

- `clip_polygon` clips in two stack lists of 12 (`ClippedPolygon`, a
  `tinyvec::ArrayVec`), and a polygon inside all four bounds comes back as
  it went in (every pass would copy it). The two pixel-backend callers
  (`fill_flat_submission`, `fill_textured_submission`) keep their clipped
  polygons in a stack array, so no heap at all. The `Vec` form stays for
  the tests.
- `TextureIdentities::resolve` and `CpuTextureStore::resolve_cut` find an
  entry through an Fx map by its source and descriptor addresses instead
  of scanning every entry; ordinals and allocation ids are given in the
  same first-use order.
- `DrawQueue::reset` keeps its groups' emptied entry lists for the next
  frame's groups.

Not done: item 3, `record_primitive_memory` in the view shown's pass: that
pass reads the arena back (`next_payload_word(0x20)`, the world flash's
allocation carry), so the record is not unread there. `PrimitiveCommand`'s
payload `Vec` moves into the queue's nodes and drops with them; reusing it
needs a pool threaded through the generic queue and every producer.

Same method as above (Terra Nova, `settings-gl`, `[status]` frames 600 to
1200), the `main` build (`cba0e004e`) against the branch's, alternating:

| Run | Before | After |
| --- | --- | --- |
| OpenGL 3840x2160 (three runs) | 12.3, 16.6, 11.0 | 10.1, 12.5, 9.7 |
| OpenGL 640x480 (two runs) | 7.3, 7.5 | 5.2, 5.9 |
| Vulkan raster 3840x2160 (two runs) | 10.1, 10.3 | 8.5, 8.7 |

About 1.3 to 2 ms a frame. Identity: Pelagia driving and firing
(`pelagia-drive`, `keys-drive`), 1920x1080 widescreen, 400 frames, on
OpenGL, software, Vulkan raster and Vulkan traced: the event logs are equal
line for line (except Vulkan's wall-clock ambience lines, which differ
between two runs of the same build too), and the snapshots at frames 100,
200, 300 and 400 are byte-identical; a Vulkan snapshot that differed once
also differed between two runs of the `main` build, and a rerun matched.

## Payload and scratch trim (2026-10-30)

Items 5 and 6, on branch `infestation-cpu-trim2`:

- `PrimitiveCommand`'s payload and exact-corner lists come from a
  per-thread pool and go back to it when the command drops (the queue's
  reset), so a frame's primitives reuse the last frame's memory without a
  pool threaded through the queue or its producers. Capped at 65536 lists.
- `model_scratch`: an allocation's custody is found from the top of the
  registrations (the newest, which nearly every read reaches); a model's
  view loads all its vertex slots under one check (`read_vertices`); a world
  write reads its slot once instead of once per axis.

Same method, `main` (`10c9e1c39`) against the branch, alternating:

| Run | Before | After |
| --- | --- | --- |
| OpenGL 3840x2160 (three runs) | 10.35, 10.74, 9.57 | 9.41, 9.43, 9.37 |
| OpenGL 640x480 (three runs) | 5.14, 13.1 (outlier), 5.23 | 5.02, 7.49 (outlier), 5.08 |
| Vulkan raster 3840x2160 (three runs) | 8.36, 8.61, 8.39 | 10.15 (outlier), 8.13, 7.97 |

About 0.1 to 0.9 ms a frame, most of it at 4K. Identity as above (Pelagia
drive and fire, 1920x1080 widescreen, 400 frames, OpenGL, software, Vulkan
raster and traced): event logs equal but for the command line and Vulkan's
ambience lines, snapshots at 100 to 400 byte-identical.

## Deep profile at 640x480 (2026-10-30)

`main` `fbad38a96`, OpenGL 640x480 (one pass), `--hidden`, `settings-gl`
(retail camera), 16 ms steps, 3600 frames; Terra Nova from its first start
(standing) and Pelagia dry (`pelagia-dry`'s start, standing). Kit and raw
output in `.tmp/perf-deep` (not committed).

- Frame time, `main`'s release exe, `[status]` frames 600 to 3600:
  **Terra Nova 4.9 ms** (two runs, 200 to 216 fps), **Pelagia 6.3 ms**.
- A profiling build (own target directory, `CARGO_PROFILE_RELEASE_DEBUG=1`,
  `-C force-frame-pointers=yes`, a counting global allocator, the
  `cpu_profile` table printed for OpenGL too, scopes on event pumping, swap,
  the crystal survey and the audio mix) runs 5.1 to 5.4 ms on Terra Nova.
- Stacks: `sampler2.py` suspends only threads busy in the last half second,
  walks with `StackWalk64`, names inf-game frames with `addr2line -i`
  (inlined frames expanded) and system DLLs from Microsoft's symbols: 4,447
  main-thread samples on Terra Nova, 4,121 on Pelagia. Sampling slows the
  frame (to about 7 ms), so shares are turned into ms at the unsampled
  frame time.
- Retail: the lab's `tn-long` with its quit moved to pass 7000, three runs,
  the main thread's EIP sampled (`retail_sampler.py`, `Wow64GetThreadContext`;
  retail keeps no frame pointers, so self time only), 5,339 samples after the
  world loads. Each function is named by the port files that cite its
  address, and its share taken from the lab's 2.05 ms.

### Where Terra Nova's 4.9 ms goes, against retail's 2.05

| Part | Port ms | Retail ms | Port / retail | Whose |
| --- | --- | --- | --- | --- |
| Draw queue close, sort, flush (`close_inner`, 004A44A0) | 0.31 | 0.35 | 0.9 | retail's |
| Receivers into D3D vertices and own clipping (`d3d_*`, `fill_*_submission`, `clip_polygon`; 004DDA30, 004DBA90, 004BCCE0, ...) | 1.45 | 0.92 | 1.6 | retail's |
| of which `memcpy` moving `ClippedPolygon`s by value | 0.45 | - | - | port's form |
| Model programs (vertices and projection 0.35, scratch reads 0.37, primitive building 0.33, lighting 0.13, interpreter 0.10, queue submit 0.05) | 1.35 | 0.22 | 6 | retail's |
| Terrain strips and tiles (projection 0.19, tiles and blend 0.53, queue submit 0.07) | 0.79 | 0.35 | 2.3 | retail's |
| Tick (`first_pass`: world contact, pairs, update list, player) | 0.18 | under 0.1 | about 2 | retail's |
| GL submission on the main thread (`GlFrame`, driver entry) | 0.30 | - | - | port-only (retail: D3D, stubbed) |
| `flash::record_primitive_memory` (`Option<u8>` byte record) | 0.23 | - | - | port-only |
| Rest of the world draw and present (swap 0.08, sky, light pass, payload drops) | 0.24 | - | - | mostly port |
| Lab stub and system DLLs | - | 0.12 | - | retail lab |

The reproduced work is about 4.1 ms against retail's 1.9 (2.1 times), the
port-only work about 0.8 ms. Pelagia (6.3 ms) splits the same way:
pixel backend 2.33 (GL 0.52, `memcpy` 0.50), model programs 1.59 (furniture
0.89), terrain 1.20 (with water strips 0.38), draw queue 0.30,
`record_primitive_memory` 0.26, tick 0.22, swap 0.21.

Why the reproduced work is slower: not the model interpreter (0.10 ms), but
what each opcode reaches.

- Emulated memory: `model_scratch` reads and writes (`slot_offset`,
  `require_mark`, `read_vertices`, `write_slot`, `decode_slot`) 0.37 ms; the
  byte payloads read back word by word as `Option`s (`PrimitiveCommand::read_u32`,
  0.08 ms in the receivers alone).
- x87 rounding: `fistp_low32` calls `round_ties_even`, which the baseline
  x86-64 target can't inline (no SSE4.1), so every rounding is a call into
  `ucrtbase!rint`: 0.15 ms in projection, lighting and shading.
- Error plumbing: `Result`/`Option` `branch` frames 0.2 ms, a
  `PrimitiveError` built and dropped in `vertex_xy` 0.01 ms, finite and
  1/z scans repeated three times per primitive.
- Data movement: `[ClippedPolygon; T]` (12 vertices of 32 bytes each)
  built with `from_fn`, then each polygon returned through `Result` and
  copied: 0.45 ms of `memcpy` and 0.14 ms of initialising.

### Frame-level checks

From the `cpu_profile` table (Terra Nova, profiling build): the frame's
named parts add up to its end-to-end time within 0.01 ms; outside the
draw: tick 0.20, swap 0.08 to 0.18 mean (12.5 ms once), crystal survey 0.01,
event pump 0.002, audio mix 0 (silent run), status line and snapshots
nothing. No sleep or pacing runs (fixed step, no vertical sync under
`--hidden`).

- Main thread on the CPU 94.7% of the wall (unsampled run); the rest is
  waiting in the swap (`NtWaitForSingleObject` under `Device::swap`) and
  preemption. NVIDIA's own thread presents (`DrvPresentBuffers`,
  `NtDxgkSubmitPresentToHwQueue`) and takes 38% of another core.
- Kernel time: 10 to 12% of the main thread's wall, but the same 10% under
  the software renderer at 21 fps, so it scales with time, not frames, and
  samples find under 1.5% in system calls (`NtWriteFile` of the session
  recording each frame 0.01 ms). Probably interrupt accounting; an ETW
  trace (administrator) would tell.
- Page faults 88 a second (0.4 a frame), working set flat at 321 MB.
- Locks: no contended lock on the main thread (SDL's audio lock 0.002 ms).
- Allocations: 1,726 a frame, 1.9 MB (Pelagia 2,073, 2.6 MB). Model runs
  make 1,389 of them and 1.5 MB (85 runs, 16 allocations and 17.6 KB each):
  `ModelExecutor::new` copies the 8 KB sine table into a new `Box` per run
  (`NativeBasisMath::new`), and `ModelVertices`/`ModelShades` make their
  `views` and `normals` `Vec`s each run. The allocator itself costs only
  0.07 to 0.09 ms (low-fragmentation heap).

### Opportunities, best gain for the risk first (Terra Nova ms)

1. `ClippedPolygon` by value: clip into the caller's buffer, pass a
   polygon inside the bounds as the slice it is, no `from_fn` array: 0.5
   to 0.6.
2. `fistp_low32` as one `cvtsd2si` (`_mm_cvtsd_si64`: rounds to nearest
   even, out of range gives 0x8000000000000000, whose low word is the 0 kept
   now), or SSE4.1 for the build: 0.15.
3. GL submission: the GL name cached on the `CpuTexture` instead of a
   SipHash map behind a `RefCell` per polygon (0.10), the vertex layout set
   once in a VAO and one streaming buffer instead of `VertexAttribPointer`,
   `EnableVertexAttribArray`, `BufferData` per flush (0.15): 0.25, and less
   work for the driver's thread.
4. `model_scratch`: resolve a run's slot bases once and read typed slices,
   without a custody check per read: up to 0.3.
5. `record_primitive_memory`: record words with a mask instead of
   `Option<u8>` bytes, or only what the world flash reads back: 0.15 to 0.2.
6. Terrain tiles: build the frame's tile commands into reused storage
   without a `PrimitiveCommand` and payload per tile (retail 0.35 against
   0.79): 0.2 to 0.3.
7. Primitive building and its readers: typed payload fields instead of
   byte payloads read back as `Option` words: 0.2.
8. One validation pass per primitive in `fill_textured_submission` (finite
   and 1/z checks once): 0.1.
9. Vertex projection (`project_with_depth_bias`, 0.35 against retail's
   0.13): a straight loop without a `Result` per vertex; needs the x87
   results kept exact: 0.15.
10. `NativeBasisMath` sharing one sine table, and the model run's `Vec`s
    reused: under 0.05 ms, but most of the frame's 1.9 MB of allocations.

Together about 2 ms of Terra Nova's 4.9. The draw queue's sort is level
with retail and the tick small, so neither is listed.

### First batch of the opportunities (2026-10-30)

Branch `infestation-cpu-batch1`, against its base `5c9229f98`. Done:

1. `clip_polygon_into` clips into the caller's `ClippedPolygon`, four
   passes ending in it; the receivers no longer move polygons through a
   `Result` or by value.
2. `fistp_low32` is one `cvtsd2si` (`_mm_cvtsd_si64`) on x86-64; the old
   form stays for other targets, and a test compares the two over ties,
   the 32- and 64-bit edges, infinities and NaN.
3. GL: a texture keeps its GL name for the presentation it was looked up in
   (`CpuTexture::device_name`, with the device's serial), so only its first
   use in a presentation reaches the map; the batch's attribute pointers
   are set once until the blit replaces them.
5. `record_primitive_memory`'s payload copy is branch-free (`value.or`);
   the record keeps its `Option<u8>` form, so it is unchanged. Words with a
   mask would change the queue's record type and its readers: not done.
8. `fill_textured_submission` checks finite and 1/z in one pass, and its
   clip skips the finite check it already made (`clip_finite_into`).
10. `NativeBasisMath::shared`: model runs share the sine table's `Arc`.

Not done: 9 (projection loop): each vertex's `Result` carries the emulated
scratch's partial commits, so a straight loop would change what an error
leaves; 10's buffer reuse: `ModelVertices` keeps per-run sets and residue
that would have to be cleared exactly, for under 0.05 ms.

Identity (`.tmp/cpu-trim-check/id.sh`, Pelagia drive-and-fire, 1920x1080,
400 frames): OpenGL, software and Vulkan raster snapshots at 100, 200, 300
and 400 byte-identical, event logs identical but for Vulkan's ambience
start line, which moves between two runs of one exe too. Vulkan traced:
a second run of the new exe equals the base at all four frames; the
tracer differs run to run in the base as well (9 pixels at frame 300 or
400).

Milliseconds a frame (`perf.sh`, Terra Nova, frames 600 to 1200, runs
alternated; other work shared the machine, so best and median):

| Scene | Base best | New best | Base median | New median | Runs each |
| --- | --- | --- | --- | --- | --- |
| OpenGL 640x480 | 4.96 | 4.48 | 5.20 | 4.87 | 9 |
| OpenGL 3840x2160 | 9.20 | 8.39 | 12.0 | 9.2 | 6 |
| Vulkan raster 3840x2160 | 8.04 | 7.25 | 9.6 | 9.2 | 6 |

### Second batch: arena record and scratch custody (2026-10-30, not kept)

Branch `infestation-cpu-batch2`, against `a8ac5e95d`. Tried, measured, and
dropped (the patch is kept locally in `.tmp/cpu-trim-check/batch2-A-B.patch`):

- B (item 6): `NativeDrawMemory` kept its `Option<u8>` record but flagged
  4 KiB pages as it stored, and the frame's renewal refilled only those.
- A (narrow): each `ModelScratchAllocation` cached the arena epoch of its
  last passed custody check; every custody-changing borrow bumped it, so a
  hit skipped only a check that would pass again.

Both were identical to the base (OpenGL, software, Vulkan raster
snapshots at frames 100 to 400 and event logs, but for Vulkan's ambience
timing; Vulkan traced equal to a second base run), and the lib tests
(4439) and Vulkan GPU tests (22) passed. But neither was faster
(Terra Nova, ms a frame, runs alternated):

| Scene | Base | B only | A and B |
| --- | --- | --- | --- |
| OpenGL 640x480, best / median | 4.27 / 4.34 | 4.44 / 4.45 | 4.46 / 4.48 |
| OpenGL 3840x2160, best / median | 8.19 / 8.26 | | 8.45 / 8.49 |

The full refill was not the arena's cost (its 1.6 MB fill is well under
0.1 ms; how much of it a frame dirties was not measured), and the custody check is cheap
next to the slot copies. A bitset in place of `Option<u8>` would change
the recorder's vectorised payload merge into per-bit work: not tried.

## After the fixes (2026-10-26)

Items 1 to 7 and 9 below are fixed; item 8 is kept as it is. Every scene's
snapshot is identical to the baseline build's (`ref1`), and the workspace
tests pass. Mean milliseconds, before (`t6`) and after (`t10`):

| Scene | Frame before | Frame after | CPU before | CPU after | GPU after |
| --- | --- | --- | --- | --- | --- |
| Pelagia, rain | 25.4 | 11.1 | 20.2 | 11.1 | 4.5 |
| Pelagia, dry | 26.3 | 11.0 | 21.3 | 11.0 | 4.3 |
| Pelagia, night | 18.0 | 7.7 | 13.1 | 7.7 | 4.2 |
| Pelagia, driving | 18.9 | 8.0 | 14.5 | 8.0 | 3.8 |
| Pelagia, infrared | 14.1 | 7.2 | 12.6 | 7.2 | 1.3 |
| Terra Nova | 18.4 | 9.1 | 13.4 | 9.1 | 4.3 |
| Stendahl Alpha | 17.3 | 7.2 | 12.0 | 7.0 | 4.6 |
| Artoria | 11.3 | 5.2 | 6.8 | 4.6 | 4.0 |
| Foldrak Noir, night | 13.7 | 6.5 | 9.6 | 6.5 | 3.7 |
| Lava world (17) | 16.9 | 7.8 | 12.0 | 7.8 | 4.3 |

Pelagia in the rain went from 39 to about 90 frames a second. The CPU no
longer waits for the GPU (0.0 ms, against 5.3), and its own work fell from
20.2 to 11.1 ms: the world draw 18.9 to 10.3, model execution 9.8 to 5.6
(about 29 us a model), the furniture 4.5 to 2.7, the ground, water and sky's
feed 2.2 to 0.6. The heap's share of the samples fell from 14% to under 4%.

What changed, by item:

1. **Frames in flight** (`vk_device.rs`, its module comment): two command
   buffers, fences and acquire semaphores. The swap submits without waiting,
   so the next frame records while the GPU finishes the last. A recording
   waits only for its own slot's last submission, and for the last frame
   (`State::settle`) where it rewrites what that frame's GPU work reads: the
   tracer's host-written buffers, descriptor sets and timestamps, an
   upscaler being rebuilt, the grade, the thermal camera and the raster
   anti-aliasing. Each submission keeps the staging buffers, images and
   descriptor sets it released until its fence signals, and each slot has
   its own vertex memory. Readbacks and a swapchain rebuild wait for
   everything (`submit_and_wait`). The GPU runs the same commands in the same
   order as before. The validation layer reports nothing, and in a build
   without the tracer's wait it reports the descriptor pool and acceleration
   structures still in use, so the check works.
2. **The capture**: `face_key` sorts on the stack and hashes with Fx; the
   faces seen over the same corners are chains in one flat list, not a
   `Vec` each. The feed's vertex and triangle lists are kept between frames
   (`SoftwareFrame::keep_spare_feed`). A face's corner lists are inline
   (`tinyvec`), and so are a round part's per-polygon lists and its split
   corners' blend weights. Round models and tread profiles are shared through
   `Rc`. The motion history keeps one slot list per key with a cursor, and
   the builder keeps the last (model, transform) key. Whether a model has
   exhausts is kept per model for the frame, and sprites are found through
   a map. `decode_shade` no longer collects into a `Vec` (it was 13% of the
   capture and a quarter of `prepare_d6`).
3. **Allocation**: removed with items 2 and 6. Also gone: the ground quads'
   double payload copy, the terrain's fog and clip bytes collected into
   `Vec`s, and `clip_polygon`'s list per clip plane.
4. **The water** (`scene_water::add_water`): the waves are taken only at the
   water tiles' corners and the neighbours their normals read. A test
   (`the_water_mesh_is_the_whole_grids`) builds the old whole-grid mesh and
   the new one and finds them equal.
5. **Model execution**: items 2 and 3 cut its cost per model from 48 to
   about 29 us. What remains is the draw programs' own work and the
   capture's per-corner work.
6. **The draw arena** is refilled, not remade
   (`DrawQueue::renew_with_native_memory`): a renewed arena equals a fresh
   one byte for byte (test `a_renewed_arena_is_a_new_one`).
7. **Fx hashing** (`fast_hash.rs`) in the hot maps: the feed's faces,
   sprites, motion, round parts and model names, the exact screen points,
   the raster's motion.
8. Kept as it is.
9. **Snapshots**: each present copies the frame on the GPU into one readback
   buffer, without waiting (`VkFrame::stage_rgba`). The CPU reads it only
   when `--snapshot-at` or `--snapshot` saves (`FrameSnapshot`). A frame that
   holds the last picture (a skipped world tick) presents nothing, so the
   saved snapshot is the last presented frame, as before. The CPU raster and
   OpenGL read as before.

## Findings (2026-10-26)

The GPU is not the limit: 4 to 5 ms a frame in every scene. The CPU is:
13 to 21 ms on the busy worlds (Pelagia 20, Terra Nova 13, Stendahl 12),
7 on Artoria. Pelagia in the rain runs at 25.4 ms a frame (39 fps): CPU
20.2, waiting for the GPU 5.3.

| Scene | Frame | CPU | GPU wait | GPU |
| --- | --- | --- | --- | --- |
| Pelagia, rain | 25.4 | 20.2 | 5.3 | 5.1 |
| Pelagia, dry | 26.3 | 21.3 | 5.0 | 4.9 |
| Pelagia, night | 18.0 | 13.1 | 4.9 | 4.7 |
| Pelagia, driving | 18.9 | 14.5 | 4.4 | 4.2 |
| Terra Nova | 18.4 | 13.4 | 4.9 | 4.8 |
| Stendahl Alpha | 17.3 | 12.0 | 5.3 | 5.2 |
| Artoria | 11.3 | 6.8 | 4.5 | 4.3 |
| Foldrak Noir, night | 13.7 | 9.6 | 4.1 | 4.0 |
| Lava world (17) | 16.9 | 12.0 | 4.9 | 4.8 |

Mean milliseconds; `perf-scenes.ps1`, tag `t6`.

What ran slower than it should before the fixes, largest first (Pelagia in
the rain; shares of the process's CPU samples from the sampled profile):

1. **CPU and GPU never overlap.** The device records one command buffer,
   submits it and waits on its fence before going on (`State::submit_and_wait`),
   so each frame costs its CPU time plus its GPU time: about 5 ms of every
   frame is the CPU waiting. With a frame in flight (per-frame copies of the
   host-written buffers and descriptor sets) the GPU's time would hide under
   the next frame's CPU work.
2. **The tracer's capture of model faces** (`scene_capture::capture_primitive`)
   is 22% of the samples, 4 to 5 ms a frame:
   - `SceneFeedBuilder::face`: its `face_key` allocates a `Vec`, sorts it and
     hashes it with SipHash for every face (46% of `face`); the feed's
     vertex and triangle `Vec`s start empty each frame (a new `SceneFeed`
     per frame) and grow push by push (45% of `face`);
   - `faces: HashMap<u64, Vec<FeedFace>>` allocates a `Vec` per face;
   - each primitive allocates about seven `Vec`s (`Face`'s slots, lighting,
     UVs and factors; the passes' polygons, corners and normals);
   - `vertex_motion` hashes a `MotionKey` with SipHash per vertex, after a
     linear search of the object's draws.
3. **Allocation**: the heap's own functions are 14% of all samples
   (`RtlpLowFragHeapAllocFromContext`, `RtlFreeHeap`, about 3 ms a frame),
   most from item 2.
4. **The water mesh** (`scene_water::add_water`) is 8% of the samples,
   about 1.7 ms a frame on water worlds: every frame it evaluates the game's
   waves at every grid point within the fog's reach (up to 204 by 204), land
   included, and builds the mesh through per-vertex closures into `Vec`s
   that grow from empty.
5. **Model execution** is 39% of the samples (item 2 is inside it):
   Pelagia's furniture alone takes 4.5 ms (195 models a frame, about
   48 us each).
6. **The draw arena is made anew every frame**: `replace_command_arena`
   allocates the 0xC8000-byte arena's `Option<u8>` byte record (1.6 MB) and
   frees the last (1.3% of the samples, with its page faults), and every
   queued command copies its bytes into it (`flash::record_primitive_memory`,
   2.5%). The record is retail's allocation carry, which the world flash
   reads; the allocation could be kept and refilled.
7. **SipHash** in hot maps (`HashMap<u32, _>`, `<MotionKey, _>`, `<i16, _>`,
   the face keys): about 4% of the samples.
8. **The ground's raster commands under path tracing**: the terrain
   traversal still projects, clips and queues every ground quad (each
   payload copied into a new `Vec`), which the flush then skips as fed to
   the tracer: about 1 ms a frame. Kept for now: the arena's fill decides
   retail's "arena full" stop, which also stops furniture and its particles.
9. **Probes only**: with `--snapshot-at`, every frame is read back from the
   GPU and copied on the CPU (`session_host.rs`'s present), not just the
   frames saved: 1 to 2 ms a frame in every probe.

GPU, for the record (Pelagia in the rain): the upscaler (DLSS Ray
Reconstruction, 720p to 1080p) 1.95 ms, the trace 1.47, the geometry 0.93
(every model's bottom level rebuilt each frame, furniture included), the
resolve 0.39, the rest under 0.15 each.
