# Future ideas

Ideas noted for later. Nothing here is researched or planned
yet; each entry records the idea as given, for a future session to pick up.

## Crystals as instances of one Crystal (2026-10-26)

Make the Terra Nova crystals and the Stendahl Alpha ones (the
models themselves) instances of a single Crystal, so that other instances
could be made later too, for example a pink crystal.

Not researched or implemented.

## Lava (2026-10-27)

The lava should be the best-looking version of itself, not merely a
fix. It must read as molten rock rather than a glowing decal, it must throw
visible orange light on the walls, ground and vehicles round it, it must be
far less transparent and less reflective than water, and it must very slowly
move and mix.

The ideal, in the order each part carries the look:

**Liquid first.** A lava lake is mostly open melt, not a crust with cracks.
The deep channel runs molten end to end, with slag only as drifting islands;
the banks are where plates crowd, thicken and weld into a black shore. The
depth of lava under a point — the game's plane less the ground beneath — is
what chooses the mix, and nothing else: shore is crust, channel is melt.

**Motion is slow advection, not a scrolling texture.** Two or three
low-frequency warp fields bend the sample point into eddies and stretched
filaments. Each octave of the crust is carried at its own speed, the melt
faster than the plates, so plates stretch, part, turn and close again. A tide
turns the whole warp over minutes so the eddies never repeat, and the fine
melt slips over the coarse. The retail tile is detail carried by the flow,
never the base colour.

**The crust is plates, not contours.** Noise contours read as worms. The
crust wants cellular plates — Voronoi or ridged cells — with near-black
basalt interiors carrying the tile's grain, and thin, ragged, bright seams
where they have parted. Plates should have thickness: a parallax or normal
offset at the seam so it reads as a crack between slabs with melt below, not
a painted line. Plate edges are finer than their bulk.

**The surface has shape.** Slow swells breathe across the lake, fine ripples
ride the flow, and the crust sits a little proud of the melt. The shading
normal comes from that height, not from the texture, so the sheen moves with
the lava. Lava's highlight is rough, streaky and viscous: dimmer and much
broader than water's, never a mirror, never a reflection of the sky.

**The shore is an interaction, not a boundary.** Where melt meets a wall, a
bright meniscus climbs a few units, the wall above is baked dark, and the
lowest stretch takes a warm bounce. Plates pile against the bank and thin into
the channel. If a world ever puts lava against water, that contact should
steam and crust over.

**Events sell the heat.** Rare upwellings part the crust in place, each
stretch of the lake breathing on its own slow turn; a blister swells and pops
into a short spray of hot droplets and a ring of fresh melt; a blast throws
molten bombs and a fireball, not water spray.

**Colour is a blackbody, not a gradient.** Cooled basalt, dull red, deep
crimson, molten orange, and a pale yellow-white core at a fresh upwelling.
The trace's albedo image is UNORM, so the ramp stays inside [0, 1];
brightness comes from the light the lake sheds, not from values above one.
The retail tile's hue modulates the detail, never replaces the ramp.

**The light is the lake as an area light.** The lake fills the lower half of
the sky: a wall takes half of it, an overhang all, the ground at its own
level a smaller share along its horizon. That share, a soft shadow ray across
the lake, a falloff with height and a slight flicker from the churn give the
warm fill that makes the lava feel hot. It must reach walls, ground, vehicles
and the player's own model, and it should appear in reflections as a warm
smear rather than a mirror image. It stays in the trace shader: no synthetic
lights in retail's own light grid.

Rebuilt from scratch on 2026-10-27 (scrapped fully); what is
built is in [the RT plan](INFESTATION-RT-IMPLEMENTATION.md) and
`PIXEL_RUNTIME.md`: the retail-hued blackbody melt under a cracked skin,
basalt plates lit as rock with glowing cracks seen at depth, upwellings that
push them aside, sub-pixel detail filtered to its average, and the lake's
light carried whole by the bounce rays that meet it.

Still open:

- A bright meniscus where the melt meets the bank, and the bank's rock
  glowing at the waterline (the resolve caps a lit surface at its albedo, so
  a glowing bank needs its own path).
- Blasts in the lava throwing molten bombs and a fireball, not water spray.
- The lake's light flickering with its churn.
- Heat shimmer over the lake (primary rays bent where they pass low over
  it), and the ash and haze above it lit from below.

## Spoken briefings (2026-10-28)

Narrate every world's briefing with ElevenLabs voices, so the
briefing speaks its text instead of only showing it.

Where the text lives: each world's briefing records name H0 text IDs, and the
strings are exported per OVL and language in
`extracted/strings/sources/<ovl>l<language>.bin` (NUL-separated,
first local slot the world's name; `manifest.json` maps each local slot to
its global ID). The Command Centre's is OVL 28, IDs 875-886 spoken plus 887
"Briefing ends."; 888-891 are the Mission Summary's objectives. `l0` is
English, `l1`-`l5` the other five languages. Pelagia (world 1) has the
port's own briefing texts.

The plan so far (2026-10-28): one clip per text ID, since
ElevenLabs bills by characters, not clips, and a bad line is then re-rolled
alone; generated by an agent through the ElevenLabs MCP; the clips shipped in
`Remastered.pak`. The briefer is a serious voice; the pick is
ElevenLabs' "Adam".

Not researched or implemented: how the lines are timed against the briefing
program's records, and how a spoken line is played.
