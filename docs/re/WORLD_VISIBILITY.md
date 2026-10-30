# World visibility and timing feedback

`world_fade` owns the native world visibility chain independently from the
frontend template. It reuses `process_particles::timing::WorldTimingRing`:
the same process `0055F154` feedback drives both manager visibility and particle
admission. No duplicate timing ring or level-specific distance is needed.

## State and load custody

| Native field | Meaning and writer |
| --- | --- |
| `W = G+1B0`, `W+1C` | Visibility target F; `0048D9F0` initializes `0x2800` and `0048E2E0` updates it |
| `W+20` | Header target H; constructor initializes `0x400`, load `0048DC30` writes `min(signed H14 DWORD+8, trunc(F*3/4))` |
| `G+5C8/+5CC` | Retained predraw F/H; load wrapper `004874D0` calls `0048E7D0` to copy manager F/H after the header writes |
| `G+588` | Sum of eight elapsed-time samples |
| `G+58C..5A8` | Eight sample DWORDs |
| `G+5AC/+5B0` | Unsigned average and ring cursor |
| `0055F154` | Frame timing feedback, also consumed by particle allocation |
| `G+5C0` | Signed view/zoom scale shared with focal easing, rather than camera height or terrain height |

`00487BE0` initializes eight `125000` microsecond samples, sum `1000000`,
average `125000`, cursor zero and feedback zero. `0048DC30` does not reset F
or the timing history. Offline reload carries them through
`OfflineWorldLoadRequest.visibility`; it recalculates H from the new header
and repeats the load wrapper's F/H copy into the two slew fields. The fresh
manager constructor's F is source-proven, replacing its former inclusion in
`IntroCampaignReloadApproximation`.

## Tick order and arithmetic

`004388D0` forwards capped delta and uncapped elapsed microseconds to
`004875A0`. Timing feedback is written before animation `00439560`; manager
`0048E2E0` consumes it before actor and camera callbacks.

When signed delta is less than signed elapsed, all eight slots become elapsed,
sum becomes elapsed*8, average becomes elapsed and feedback becomes zero.
Otherwise the cursor's low three bits select the replaced slot, sum arithmetic
wraps as DWORDs, and average is unsigned sum shifted right three. Feedback is
`65536` below average `34464`, otherwise `100000-min(average,100000)`.
The shared ring also owns signed delta/1000 elapsed milliseconds and FPS
publication. A zero average fails only after the native reached ring writes;
the session propagates that failure before subsequent tick work.

`0048E6BF..0048E738` compares feedback B as signed, then writes manager F:

```text
B < 0xD000:
  decrease = signed_low_WORD(((trunc((0xD000-B)/2) << 8) >> 15))
  F = max(F-decrease, 0x1400, H+0x100)
B >= 0xD000:
  increase = signed_low_WORD((((B-0xD000) << 8) >> 15))
  F = min(F+increase, 0x3100)
```

The multiplications, additions and shifts preserve native wrapping and signed
arithmetic. H stays at its load-derived value during this feedback update.

## Predraw and terrain admission

`00488380` captures H then F through `0048E7D0`, whose accessor reads
`W+20` before `W+1C`, then freshly acquires the camera at `004883AA`.
It adds `signed_low_WORD((signed G+5C0*4000)>>15)` to H and clamps the target
to F-800. Each retained slew field changes by exactly 64 only when its target
lies outside the inclusive `[current-64,current+64]` deadband; a target inside
that band does not replace the current value. Context fade becomes
`[trunc((storedF-storedH)/4), storedF]`, followed by normal material publication,
selector-16 eye/basis publication and `0048FB00` fog-table preparation.

Production supplies the view zoom `G+5C0` to both focal and fade math. It is
zero from the session's gameplay object (`00487C52`) and written only by the
free look `00421DC0` (V): `0x3FFF` on entry, `0x1000` steps with the zoom
key and pitch, and either end on a weapon selection. It outlives the look:
nothing resets it when the look ends. This value is not inferred from the
camera basis.

The shared world Draw now freshly executes `00478E00` before slew and reads
the returned camera only after normal material publication. A NULL result
retains the reached fade/material changes and reports `RequiredRetainedWorldView`
without publishing a fabricated eye/basis or coordinate shift. Complete retained
native context custody remains separate from persistent frontend model globals.
The existing atomic Setup command remains the named
`WorldProjectionSetupApproximation`: unlike native scalar stores, it publishes
both fade bounds and projection/fog before material. The
[camera authority](CAMERA_RUNTIME.md#acceptance-and-external-boundaries) owns
that bounded prefix difference and its renderer follow-up.

The removed frontend fade `[1048576,1048576]` was unsuitable for `0044AA10`:
the furniture front width and the Far front width wrapped to zero as WORDs,
while the Near front depth wrapped negative. The shared terrain producer and
quad cull retain the original arithmetic. Replacing the world visibility
writer repairs that input rather than widening admission rules.

The focused fade tests cover feedback limits, inclusive slew thresholds,
signed division, zoom adjustment and reload custody. The world-draw regression
uses a passive port camera pose that exposed the bad fade and checks ground
submission plus the last draw's eye, basis, projection, height and outcome.
It is a port regression fixture, not native-camera or retail-frame acceptance.
The historical retail first predraw `[2224,10176]` is reproduced by the source
load/tick/slew order without baking the captured pair into behavior.

A release port replay at fixed `125000` microseconds, frame 380, published
native camera eye `[14073,1669,-22585]`, ground height `298` and fade
`[1088,5632]`; it submitted 477 terrain commands and visibly reached ground.
These commands include clipping-generated triangles, so the count is not a
quad census. The same camera previously supplied million-unit frontend fade
and submitted no ground. This controlled port comparison establishes the
reached visibility repair, not matching retail timing, camera or full-scene
acceptance.

The general terrain bridge and remaining feature boundaries belong to
[terrain runtime](TERRAIN_RUNTIME.md#shared-production-pixels); historical
camera receipts remain in [gameplay runtime](GAMEPLAY_RUNTIME.md#frame-1-camera-pose-evidence-ttd-this-turn).
