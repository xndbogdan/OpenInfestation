# Infestation gameplay overlay

`gameplay_overlay` owns the world-22 branch `00441003..0044120F` inside
gameplay draw `00440F00`. It runs after the world/player submissions and their
flush, before `004395C0` and the remaining HUD. It is independent of the intro
controller's selector-88 draw callback. The [gameplay screen](GAMEPLAY_RUNTIME.md)
retains the screen lifetime and tick; the [world renderer](OBJECT_DRAW_RUNTIME.md)
retains the scene beneath this overlay.

## Masks and location caption

The gate reads live `G+250`, mission `+A0`, and accepts only world 22. It does
not read serialized mission selection. It then resolves H7 slot 32 through
`00405540`, retaining the backend-converted material DWORD, and submits two
immediate renderer `+84` packets in this order:

| Packet | Pixel bounds |
|---|---|
| Top mask | `(0,0)` to `(width,height/8)` |
| Bottom mask | `(0,height*7/8)` to `(width,height)` |

Both packets have four depth DWORDs `0x40`, descriptor `+20=0`, and the
resolved color at `+24`. Arithmetic is signed DWORD division toward zero,
with wrapping multiplication followed by narrowing to signed WORD corners.
At 1024×768 these formulas give 96-pixel masks. There is no crop, reduced
world viewport, projection change, or 640-pixel coordinate rescaling.
The preceding `00440FF9` renderer `+14(NULL)` clears the depth surface and
disables device Z without changing saved depth-policy words or projection.
The shared world-draw tail now executes this existing reset between its
final flush and `004A4400` batch reset; masks cannot inherit scene occlusion.

After both packets, the caller resolves H4 font 0, H6 style 0 and H0 string
223, in that order. English data supplies the MilCon location caption; the
runtime uses the resident string rather than a literal. Real `004A4240`
measurement is narrowed through signed AX before centering:

```text
x = (width - signed_WORD(measured_width)) / 2
y = (height * 15) / 16
reveal_age = max(signed_screen_age_ms / 2 - 1000, 0)
```

Font/style/string resolution and measurement precede the age gate. Only a
positive reveal age invokes raw text `004A3B20`, with full surface width and
descriptor `+1C=2`. The existing text owner performs its timed two-pass glyph
submissions through renderer `+A4`. Descriptor callback `00439DE0` is the H3
sprite resolver, not an additional draw effect. Glyph positions are already
absolute pixels; the font's coordinate divisor owns font-unit conversion.

## Screen time and production ownership

Base constructor `00437F90` clears screen `+C0`. Gameplay worker `00440D80`
forwards both scheduler arguments to `004392B0`; its first write is:

```text
screen_age_ms += signed_unclamped_elapsed_microseconds / 1000
```

The add wraps as a DWORD. The division happens on each admitted tick, so
sub-millisecond remainder is not accumulated. The clamped simulation delta
does not drive this caption. This clock is retained by `GameplayScreen` and
loaned to `session_platform::software` at each draw. No captured video time
or controller deadline value becomes runtime behavior.

`gameplay_overlay::software` resolves resources from the current residency
and sends masks through `SoftwareScene::draw_flat_quad_direct`. Caption
glyphs use `menu_text::draw_raw_text` and the same `software_menu::glyph_command`
consumer as other authored text. It neither allocates deferred commands nor
flushes unrelated work. Submission/resource failure stops the remainder of
that overlay invocation, logs the feature failure, and retains completed
pixels. The next ordinary frame draws its own overlay from live inputs.

## Verification and bounds

The tracked producer check_gameplay_overlay.py
runs unmodified retail instructions with FPCW `0x027F`. It authenticates the
whole executable before running the bounded overlay segment and real text
measurement. Its [LF fixture](../../crates/inf-game/tests/fixtures/gameplay_overlay_native.json)
contains 42 overlay cases and 10 real screen-clock prefixes. Cases cover all
five authored display sizes, odd/negative/overflow dimensions, signed AX
measurement, zero/positive/negative reveal time, world gates, resource and
submission failures, and a native font-division fault. Viewport getters are
stable supplied values; resource returns, renderer `+84`, and raw-text
`004A3B20` are supplied boundaries. This does not establish whole gameplay
draw or scene equivalence.

With Python 3 and Unicorn 2 installed, compare exact regenerated fixture bytes:

```powershell
python check_gameplay_overlay.py
```

`--write` explicitly regenerates the fixture. Rust consumers compare ordered
requests and stopped prefixes. A separate resident 1024×768 pixel test uses
the actual string, font, style, material and glyph sprites; it checks unchanged
projection and deferred queue, untouched world pixels, and real caption
pixels inside the bottom mask. It writes the diagnostic image to ignored
`.tmp/inf-gameplay-overlay-native/surface1024.png`.

Full uncropped retail/port video comparison remains the acceptance owner for
composition and timing. This overlay does not close unrelated HUD callbacks,
intro allocator carry, simulation, camera, terrain or audio differences.
