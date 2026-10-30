# Selector67 ribbon

Selector 67 (vtable `005286D8`) is the engine trail behind each selector 34
`pership` ship. It is the base object class with three overrides: deleting
destructor `00486570` (body `00486590` frees the node list, then base
`00479D00`), update `+24 = 004866A0` and draw `+2C = 00486B30`. Every other
slot, including `+18 = 00401300`, `+28 = 00401350`, `+30 = 00401310` and
`+3C = 00405140`, is the base table's.
[selector67.rs](../../crates/inf-game/src/object_runtime/selector67.rs)
owns the class and [draw.rs](../../crates/inf-game/src/object_runtime/selector67/draw.rs)
its draw.

## Creation

Selector 34 motion (`00434C10`) calls `00486600(ship handle)` while `+25C` is
NULL and stores the result there; once it is set, motion calls `004869B0`
instead. `00486600` allocates `0x5C` bytes and runs constructor `00486530`:
base constructor `00479C60`, then `+40 = 150`, `+42 = 3500`, `+44 = 0`,
`+46 = 2`, `+48 = 250`, `+4C = NULL`, `+54 = 0` and `+50` = the process NULL
`[0054E894]`. `004600F0` mode 0 then admits it through `00460120` with H13
resource 4, a NULL position and placement -1. Resource 4 is selector 67's own
H13 entry: its flags payload reaches `00479DB0` (base context `00479D40`,
then payload flags `00479D70`) and its commands the base handler
`00479E70`. In the first world the admitted ribbon has flags `08100003`,
resource word 4 and placement `FFFF`, in both the flag-0 and flag-1 lists.
A rejected admission deletes the ribbon and returns NULL. On success `+50`
takes the ship's handle. The position `+32` stays unwritten until the first
ship update.

## Ship update 004869B0

Each frame the ship's motion calls `004869B0(delta, &ship+32)`:

1. `00479040` moves a flag-1 ribbon to the end of the alternate update list,
   so the current `00478F20` pass no longer reaches it; the ribbon's update
   runs once per frame through the ship.
2. `+46 = 2`. An empty list first gets `004868F0`: two `0xC`-byte nodes at
   the ship (next, point, length 0) and `+32` set to the ship.
3. The distance from `+32` to the ship is the integer root `0048EFC0` of the
   wrapped sum of squared signed WORD differences; a sum with bit 31 set is
   quartered first and the root doubled.
4. Within `[0053C558]` (200) the head node moves to the ship with that
   length. Beyond it, `+32` takes the head's point and a new head is pushed
   at the ship with the length (measured from the old `+32`).
5. Beyond `[0053C55C]` (750) with `+44` clear, a split happens first:
   `00486600(process NULL)` creates a new ribbon that takes `+4C`, `+44`,
   `+48` and `+40`, and this ribbon restarts with `004868F0` (which frees the
   old nodes when the split ribbon is NULL).
6. The ribbon's own `+24` runs.

Both thresholds are `.data` DWORDs read only by these functions.

## Update 004866A0

`+24` clears `+54`. An empty list writes `+28 = 0`; fewer than two nodes go
straight to the owner check. Otherwise it sums the node lengths into `+54`
and takes the largest squared distance from `+32` to a node, at least 64,
with an unsigned comparison. Then exactly one of: `+46` counts down; the
one-shot `+42` clears and raises `+40` to at least 200; or a total below 400
queues disposal `00479FA0` and the update continues.

`+48` eases by `(total - (signed WORD) +42) * (delta >> 5) >> 15`, truncated
to a signed WORD, and is clamped first to at least `+40`, then to at most the
total. `+28 = isqrt(largest) + 16`. The kept length is the total minus
`(delta >> 5) * +48 >> 15` as a signed WORD. Walking from the head, the node
whose segment crosses the kept length moves its successor to the cut along
the segment (each axis `offset * keep / length`, truncated) and keeps
`keep` as its length; `00486500` then zeroes the stopping node's length and
frees every later node, and the update returns. Only a walk that runs off the
end (a negative recession) reaches the owner check: `0047AD90(+50)` failing
queues disposal.

While its ship drives it, `+46` never reaches zero, so the ribbon only grows
at the head and recedes at the tail. After the ship dies, the world pass
runs `+24` alone: two countdown frames, the one-shot, then disposal once the
trail is shorter than 400.

## Split ribbons

A split ribbon's `+32` is never written, yet its own `+24` reads it for
`+28` and the world draw's distance test reads it for visibility. Infest01
and Infest02 never split; Infest03 and Infest04 split 23 times
(`ribbon-split.js`,
ledger). Each split
ribbon's `+32` held allocator residue: halves of heap pointers from the
block's earlier use, such as `(0, F030, 37D)` and `(392, 1474, 392)`, and
zero in three. The port starts it at zero, a fresh heap's value
(`SELECTOR67_POSITION_APPROXIMATION`, logged once), so a split ribbon
recedes and is disposed like any other. Before, its update stopped there
(`Selector67Error::UnwrittenPosition`) and its trail stayed in the world, as
turret fights showed. A new ship's ribbon is the same case until its first
point (`004869B0` sets `+32` to the head): `00486600` never writes it, and
the world draw's distance test reads it from the first frame, which left
Level 1's two new ribbons out of the draw ("Position") in every sweep.
Since 2026-10-18 the port starts every ribbon `00486600` creates at that
zero.

## Draw 00486B30

The draw returns at once without a second node. Otherwise it first takes
the texture: H3 sprite `029D` for `+44` 0, `038B` for 1, else sprite 1.
`0048FE60` sets an identity object basis at the context position (its
sign-extended low WORDs), and each node is projected through context
`+50` at its WORD offset from that position. A `00511C90` frame on
`0055F4E8` guards the walk; `00494A00` pops it.

The walk keeps a running length. It starts with the head's length, and
the first iteration adds the head's length again before it advances, so
every segment counts the head twice. For the segment ending at node `k+1`
with running length `L`:

- the end's grey level is `max(128 - L * 130 / +54, 0)`; the head's is 128;
- its half width is `min(focal_x * f / depth + 1, 200)` (200 when the depth
  is not positive), with `f = ((+54 - L) << 7) / +54 + 1` while `L` is
  below `+54`, else 1; `focal_x` is context `+38`;
- the screen direction turned a quarter, `(previous Y - current Y,
  current X - previous X)`, scaled to the half width (left raw when it is
  zero), offsets the end. The previous end keeps the previous segment's
  offset; the head's is zero.

Both ends must lie beyond depth 64 (the clip-byte and fog tests that also
guard it can never fail with this projection). The quad's sort word is the
ends' average depth; with material sorting and a texture, selector `FF` or
variant flags `39` give `(depth >> 1) + C0000000`, otherwise the
descriptor's opaque key, with the selector read at the entry plus
`([0053C568] & 1)`. `004A42B0` allocates four blocks for either kind: the
far D6 quad `0044EA70` when either end has fog, else the near effect quad
`0040EF30`, whose consumer reads only three. The corners are previous
minus offset, current minus offset, current plus offset, previous plus
offset, each with the sign-extended low WORD of its depth. Mode
`[0053C568]` sits at `+20`, the texture at `+24`, the grey levels at
`+28/+2C/+30/+34` (R, G and B alike), and a far quad adds the context's
far colour at `+38` and the fog bytes at `+3C..+3F`. A near quad's
consumer therefore shades the whole quad with its first corner's level.

The manager's first world draw after `00486600` reaches a ribbon before its
first ship update has written `+32`: `00479570` reads the position for its
distance test before the ribbon's own `+2C`. That `+2C` would return at once
without nodes, so only the manager's per-frame flag bit depends on the
unwritten bytes. The port omits that one frame's ribbon (logged as a
`Position` omission at clock 6 in the intro).

## Evidence

check_selector67_ribbon.py runs
`004869B0`, `004868F0`, `004866A0`, `00486500`, `0048EFC0` and `00479040`
unchanged over 44 cases and 120 steps: first frames, both threshold edges,
pushes, splits with and without a new ribbon, the `+44` guard, wrapped and
negative distances, three flights, and every `+24` arm. Allocation, the
split's creation, handle resolution and disposal are recorded boundaries.
The [Rust comparison](../../crates/inf-game/src/object_runtime/selector67/tests.rs)
matches every field, node, list position and service call after each step. A
loaded-world test admits the ribbon through resource 4.

check_selector67_draw.py runs `00486B30`
unchanged over 27 cases with the particle draw harness's view: lateral,
head-on, receding, behind-the-eye, depth-64 edges, width-capped,
degenerate, sorted and yawed deep trails and all three textures. Packet
allocation is a recorded boundary. The
[Rust comparison](../../crates/inf-game/src/object_runtime/selector67/draw/tests.rs)
matches all 41 packets' kind, sort word, texture slot and every written
byte, and all 17 draw mutants fail it.

Sprite `029D` is a grey band with flags `8024`: its inverse-source-colour
blend darkens the scene, so the trails are dark smoke. In the retail video
(MilCon flyover, about 38 s) the ships trail faint dark streaks; the
port's window shows the same darkening, fading out with fog at range.
Port PNG snapshots keep the framebuffer alpha, which this blend zeroes, so
an image viewer shows those pixels white.

Infest01 creates two ribbons, both admitted; it runs 687 ship updates (675
pushes, 12 follows, no split) and 735 `+24` updates, and both ribbons are
disposed by the short check after their ships die
(counts).

```text
python check_selector67_ribbon.py
python check_selector67_draw.py
cargo test --release -p inf-game --lib selector67
```
