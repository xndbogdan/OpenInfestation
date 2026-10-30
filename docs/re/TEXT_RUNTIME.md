# Infestation fonts and menu text

Handler-4 font decoding belongs to
[`inf-formats::fonts`](../../crates/inf-formats/src/fonts.rs). Native label
layout belongs to [`menu_text`](../../crates/inf-game/src/menu_text.rs).
The surrounding authored rows, camera wrappers, clipping, and frame order
remain in [frontend runtime](FRONTEND_RUNTIME.md).

## Font allocation and intrinsic tables

Loader `0050F250` reads `count * 0x40` bytes of headers first, then each font's
metrics followed by sprite arguments. Header `+0x38/+0x3C` become runtime
pointers to those tables. The parser retains all sixteen original header
words and bounded source spans instead of rewriting those pointer placeholders.
Original OVL offsets and bytes remain fixed.

| Header offset | Recovered consumer meaning |
|---|---|
| `+0x04` | Mode 1: direct glyph table; mode 2: position-dependent grid |
| `+0x08` | Signed glyph count; negative counts rejected |
| `+0x10` | Signed coordinate divisor; zero rejected |
| `+0x14/+0x18` | Two signed terms added to Y on a wrapped line |
| `+0x28` | Signed maximum byte, compared against the unsigned source byte |
| `+0x2C` | Fallback byte (low byte) |
| `+0x30/+0x34` | Mode-2 signed grid columns/rows |
| `+0x38/+0x3C` | Metric/sprite table pointer placeholders on disk |

Each glyph metric has four signed 16-bit words: advance, spacing, X offset,
and Y offset. The text consumer multiplies each offset by 100. Mode 1 has one
32-bit sprite argument per glyph; mode 2 has `count * columns * rows`.
The high word of a sprite argument is retained even though its resource index
is the signed low word. Unknown header words are not assigned guessed names.

The complete retail H4 corpus contains one font in each of five difficulty
overlays, with no trailing payload. All use direct mode, line-step terms
`[6477,1216]`, and fallback byte 127:

| Overlay | Glyph count | Maximum byte | Coordinate divisor |
|---|---:|---:|---:|
| `0d0.ovl` | 128 | 127 | 400 |
| `0d1.ovl` | 256 | 255 | 500 |
| `0d2.ovl` | 256 | 255 | 400 |
| `0d3.ovl` | 256 | 255 | 400 |
| `0d4.ovl` | 256 | 255 | 400 |

Mode 2 is supported from its native consumer and synthetic fixtures; these
five retail fonts do not establish a rendered mode-2 example. The old opaque
export remains lossless evidence, but H4 is no longer semantically opaque.

## Label callbacks and layout

Root callback `004834F0` resolves its H0 string and passes it as the right text
to `00453810`. Camera wrapper `004835E0` reaches that callback after its camera
work. Generic `00453A80` passes left text instead. The concrete text entry point
accepts these two resolved label callbacks; other row families remain explicit
unsupported operations until their own consumers are recovered.

Quit callback `00483550` is a separate descriptor-copy wrapper. It copies all
seven descriptor DWORDs, obtains actual surface width through `00495900`, adds
`wrapping(-width * 2) / 100` to the copied signed-word X, and calls `00453810`
with right text. The copied descriptor is discarded, so the caller receives no
Y/advance mutation from it. `software_menu` executes this wrapper before the
shared right-label path; neither its offset nor root's 500 ms reveal delay is
applied to generic left labels.

`00453810` resolves H4 slot 0. Root labels are right-aligned; generic labels
retain the authored centering/reveal branches. `004A4240` measures original
bytes without fallback mapping and omits spacing after the final byte. Callers
narrow that width to a signed word before alignment. The drawing walker
`004A3E30` includes spacing after each consumed glyph, applies the maximum-byte
and missing-sprite fallback rules, and uses native word lookahead for wrapping.
Measuring and drawing are deliberately different operations.

`004A3AB0` is the single-line walker used by the name-entry draw. It copies
the ten-DWORD descriptor, scales X and Y by the font divisor, and runs
`004A3BA0` over the copy. It then writes the signed-word quotients back to the
caller's X and Y, X first. `004A3BA0` applies the same maximum-byte,
missing-sprite and mode-1/2 placement rules as `004A3E30`, and advances by
advance plus spacing after every glyph. It has no wrap, width, reveal or
NULL-text substitution: spaces and hyphens are ordinary glyphs, and Y never
changes. When flags bit 0 is clear it stores the shared base word `0055F8F8`
into its private copy only, so the caller's descriptor `+00` and `+20` survive.
[`draw_line_text`](../../crates/inf-game/src/menu_text.rs) implements it
over the shared glyph-pair submission. Unmodified `004A3AB0` runs with a
recording callback and `+A4` produced 48 retained cases in
[`line_text_native.json`](../../crates/inf-game/tests/fixtures/line_text_native.json),
covering both modes, negative and unit divisors, substitutions and signed-word
narrowing. Draws whose native table reads fall outside the synthetic font are
not retained; the port reports those references as explicit errors.

Coordinates use signed integer division and wrapping native-width arithmetic.
Mode 1 adds metric offsets before dividing. Mode 2 selects a sprite plane from
the position fractions and adds separately divided offsets to integer pixel
coordinates. Combining those divisions changes negative-coordinate results.
Invalid metric/sprite references and division faults are explicit errors.

The reveal policy uses the original 17-byte table at `005415C8` and 25 ms step
at `005415DC`. Root animation subtracts 500 ms. Flags independently control
emission, shadow, centering, and reveal. Shadow uses H6 slot 7 with a one-pixel
offset. Generic descriptor-Y feedback occurs at the native phase, including
before later backend failure on the animated measurement path. Completed
glyph submissions are not rolled back if a later operation fails.

## Glyph submission and resource custody

[`RenderResources`](../../crates/inf-game/src/render_resources.rs) resolves
H0/H3/H4/H6/H7/H8 through the resident registration's owning overlay and local
index, retaining source bytes and provenance together. A borrowed resource
cannot survive a mutable residency transition. Missing resources need their
real lazy-loading owner and return an error here.

Glyph dimensions read logical sprite entry `+0x14/+0x16`, which belongs to the
**first physical variant**. Width is `width >> 2`; height is `(height + 2) / 4`.
The renderer's selected variant is a later decision. The resulting four
corners retain signed 16-bit wrap and carry native vertex attribute `0x40`.
Caller context origin remains separate from those coordinates.

Each emitted glyph produces two required virtual `+0xA4` submissions, in order:
descriptor `+0x20 = 1` with base color `0x00808080` (initializer `004A3A70`),
then `+0x20 = 0` with the current text color. Descriptor `+0x24` uses the same
resolved sprite; `+0x28` carries the color word. The fourth color byte is not
an assumed alpha value: retain it when native code copies the whole word, and
leave it explicitly unknown when the selected-row animation initializes only RGB.

The [text tests](../../crates/inf-game/src/menu_text_tests.rs) cover native
arithmetic, alignment, reveal, wrapping, fallback, mode-2 indexing, partial
updates, and ordered backend failure. Corpus tests use actual root/options
labels, fonts, and sprite dimensions across all five font selections.

## Concrete CPU menu pixels

[`SoftwareMenu`](../../crates/inf-game/src/software_menu.rs) implements the
required `MenuDrawHost` and `MenuTextHost` effects over the persistent
[`SoftwareScene`](../../crates/inf-game/src/software_scene.rs). It resolves
every glyph through the borrowed resident Handler-3 owner, writes four signed
XY words with depth `0x40`, and submits both passes directly through
`EffectQuadNear` (`004D78A0`, renderer virtual `+A4`). It does not enqueue,
merge passes, select one physical descriptor early, or flush unrelated work.
The shared texture owner performs the native upload conversion and padded
allocation; the selected effect consumer applies blending and clipping.
Its portable allocation/raster precision limits remain in
[pixel runtime](PIXEL_RUNTIME.md).

Controller setup follows `0043B5D0`: directional light precedes ambient light,
so shared effect RGB uses the previous ambient value. The menu retains its own
context and all ten menu-basis words separately from the scene's model/view
basis. Before each row, `00452C3C` publishes exactly the six focal, viewport,
and origin DWORDs from context `+38..+4F`; it does not replace inherited global
fade/fog values with the local context's `+64/+68` values. Label corners are
already absolute pixels: context origin is retained separately and is never
added to their coordinates. Clip changes affect the actual pixel pipeline,
and the authored row walker restores its saved rectangle. Unknown callbacks,
missing resources, and unsupported pixel states remain errors.

The [pixel adapter tests](../../crates/inf-game/src/software_menu_tests.rs)
render actual root and Quit labels at all five authored display tiers, retain
returned generic-row height, exercise no-emission flags, and verify color
publication, lighting order, separate origin, clip restoration, and unknown
callback failure. Root, Options and Quit also have SDL visual smokes. These are
portable rendered results; matched retail-frame acceptance is still open.

Original-PE isolated execution of `004A3B20`/`004A3E30` produced 128 synthetic
single-glyph cases and 256 ordered `+A4` packets. Rust text layout plus packet
construction matches every written field, excluding only the native sprite
pointer replaced by borrowed resource custody. Eight cases are retained in
[`menu_glyph_native.json`](../../crates/inf-game/tests/fixtures/menu_glyph_native.json).
The Quit wrapper matches 256 original executions, including native signed
overflow and caller-descriptor preservation; fourteen cases are retained in
[`menu_quit_native.json`](../../crates/inf-game/tests/fixtures/menu_quit_native.json).
The installed frontend descriptors exercise both font passes; selector `FF`
suppression is established by separate synthetic effect-consumer tests, not
claimed as font-corpus coverage. Specialized row families beyond these labels
still require their own implementations.
