# Infestation platform layer

The FGDK's Windows layer under the game: the input devices and their
threads, CD audio, and the DirectDraw/Direct3D screen device (its errors
name the source `C:\Coding\Fgdk2\Code\Windows\DDC...`). This note maps the
routines no other note owns, from REA reads of the retail executable. The
port replaces this layer with SDL and its own renderers
([PIXEL_RUNTIME.md](PIXEL_RUNTIME.md)); the bindings and analog consumers
above it are in [INPUT_RUNTIME.md](INPUT_RUNTIME.md), the scene tree in
[RUNTIME_FOUNDATION.md](RUNTIME_FOUNDATION.md#scene-tree-and-resource-lifetime).

## Input devices

Devices share a base (`004F50F0`, given a kind, button and axis counts and
an index) and keep their buttons as a mask at `+2C` (bit 0 unused) and axes
from `+4C`; a change goes to the consumer at `+1C`. The device list is
`0065EDF8`.

| Routine | Does |
| --- | --- |
| `004F4880`, `004F4900` | create (`0x6C` bytes, kind `0xB`, `0x1A` buttons, table `005293C8`) and delete the button device `0065ECC8` |
| `004F4840` | a press (EDX 2) or release (1) of the buttons in ECX on it, then the consumer |
| `004F5300`, `004F53B0` | create the mouse `0065EE10` (table `00529470`) after starting its DirectInput thread (`0050FE30`, then `0050FFD0`), and stop and delete it |
| `004F5290` | a mouse packet: four buttons to `+2C`, the X, Y and Z movement added to `+4C..+54` |
| `004F55A0`, `004F5730` | create and delete up to two joysticks (`0065EE14`, `0065EE18`: `joyGetPosEx` and `joyGetDevCapsA`, `0x70` bytes, table `005294B0`, polled by a 20 ms timer through `004A48C0`) |
| `004F54A0` | poll a joystick: X, Y, Z, R, U and V to `+4C..+60` and its buttons to `+2C`, or bit 31 when the read fails |
| `004F53F0`, `004F5410` | the joystick's destructor: its timer (`004A4920`), then the base `004F51B0` |
| `004F4820` | the base's scalar deleting destructor (tables `00529318`, `005293C8`, `00529470`) |
| `004F5170` | table `00529438`'s: it unlinks the entry first |
| `004F5080`, `004F50D0`, `004F51D0`, `004F5790` | delete every device; the first device; a device's first list entry (`+20`); a list's length |
| `004F51F0` | switch a device on or off (`+0`, `+4`) and hand its state to its consumer (from `004F0F17`, `004F0F3B`) |
| `004F0EE0` | sets a device's table to `005292F0`, its eight axis triples (as `004F0F80` reads them) to bias 0, multiplier `0x40000000` and scale 2, and switches it on |
| `004F1160` | that table's `+C`: stops the first input thread (`0050FF10`) and deletes device `0065EC9C` |
| `004F0EA0` | links an analog binding set into an input context's list `+3C` (from `00439D00`); `004F0AD0` destroys one (from `00439D50`) |

Which devices reach bindings, and how the port reads its pad, is in
[the input notes](INPUT_RUNTIME.md#the-joystick-the-mouse-and-the-input-mode).

Two DirectInput threads run `0050FA40`, started with `_beginthreadex` and
a manual-reset event: the first by `0050FD90` (from `004F10E6`), the
mouse's by `0050FE30`. `0050FF10` and `0050FED0` stop them (`SetEvent`, then
wait), `0050FF50` and `0050FF90` pause and resume the first, and
`0050FFD0` reads the mouse's device (`005454E8 +C`; a failure is error
`0xB00`). `0050F950` turns a DirectInput result into its
text ("DirectInput failure", "Aggregation not supported", ...).

Above the devices ([INPUT_RUNTIME.md](INPUT_RUNTIME.md#analog-consumers-and-device-notifications)):
`004EFE00` emits an analog
consumer's aggregate (`004F0150`), closing its gate around the timer
(`004A4990`, `004A4940`); `004F0AD0` releases a consumer: it unlinks it from
every device list, drops its references and kills its timer.

## CD audio, messages and the command line

`004F5820` drives CD audio through MCI (`mciSendStringA`, "status cdaudio
mode", `mciGetErrorStringA` into an error object, table `005294C8`).
`004F4F70` shows a message box, turning the engine's flags into
`MessageBoxA`'s icon (`0x10`, `0x20`, `0x30`) and buttons. `004F4D20` reads
the working directory into `0065ECCC` (without a trailing backslash) and
the command line's "systemmemory" and "emulation" options. `004F4060`
checks a CRT call's result (from `00449ED7..00449F2E`) and raises the CRT's
error text when it differs.

## The screen device

`004F8B30` builds the DirectDraw device `0065EE48` (400 bytes) through
`004F8670`, which calls `DirectDrawCreate` and queries `IDirectDraw2`
(`00523488`), and starts four lists (`0065EE58`, `0065EE68`, `0065EE78`,
`0065EE88`). `004F8510` releases its COM objects (back to the normal
cooperative level first, `+50(0, 8)`) and `004F8BE0` frees it. `004F8C50`
sets the flags `0065F128`/`0065F12C`. Errors are objects (table `005294E8`,
`0xA00` or `0xA04` for `DDERR_SURFACELOST`, the HRESULT, the source path and
a line); `004F7EF0` turns a Direct3D result into its text.

| Routine | Does |
| --- | --- |
| `0050B900` | sets a screen up from its descriptor (window, parent, buffers; `0050A770`) |
| `0050B6D0` | creates a screen buffer (`IDirectDraw +18`, CreateSurface) |
| `0050BCD0` | the first screen creates the Direct3D device `0065EE4C` (`IDirect3D2 +20`, CreateDevice); later ones count it (`0065EE84`) |
| `005094B0` | creates a texture page: 256 palette entries, a free list of 255 slots, its surface |
| `0050E0B0` | adds a texture record to list `0065EE80` |
| `005093D0` | renews every texture's handle for the device (`+C`, GetHandle) |
| `0050E700` | converts pixels to 5:5:5 by the source's channel shifts |
| `0050DED0` | a fast blit (`+1C`, BltFast), ignoring `DDERR_WASSTILLDRAWING` (`0x8876021C`) |
| `0050EC60` | ends an open scene (`0065EE4C +2C`, EndScene; `0065EE94`) |
| `0050EC90` | flips to the GDI surface (`IDirectDraw +28`) |
| `0050E630` | removes a matching record from list `0065EE68` |
| `004F8CE0` | frees a texture slot: unlinks it, takes its area off `+92C` and drops its palette references |
| `0050D080` | frees every texture page (each slot, then its five COM objects) |
| `0050D1D0` | closes a screen: restores the window (`0050D330`), releases its buffers, and the last one ends the scene, frees the pages (`0050D080`), and deletes the viewport `0065EE98` and the device |
| `0050B640` | when the last user lets go (`0065F114`), runs list `0065EE68`'s callbacks, releases list `0065EE78`'s objects, calls `0065F108 +70(0)` and releases `0065F10C` and `0065F108` |
| `004F7EB0` | closes every screen in `0065F0F0` (`0050D1D0`) |
| `0050F060` | walks a list of byte pairs (`0xFF` for none) into `0x1C`-byte records |

## What the port replaces

The port's SDL window, OpenGL and software renderers, audio device and file
handling stand where these did, so the progress report omits them
([status.csv](../progress/status.csv)) after a check that none
decides anything a player sees or hears beyond its own platform:

- **The screen device**, replaced by the port's SDL/OpenGL platform: the
  DirectDraw device and its lists (`004F8670`, `004F8B30`, `004F8BE0`,
  `004F8C50`, `004F8510`, `0050B640`), the screens and their buffers
  (`0050B900`, `0050A770`, `0050B6D0`, `0050BCD0`, `0050D1D0`, `004F7EB0`,
  `0050EC60`, `0050EC90`, `0050DED0`), the texture pages (`005094B0`,
  `004F8CE0`, `005093D0`, `0050D080`, `0050E0B0`, `0050E630`, `0050E700`)
  and the Direct3D result texts (`004F7EF0`). What the renderer draws is
  ported from its Direct3D slots and the pixel kernels
  ([PIXEL_RUNTIME.md](PIXEL_RUNTIME.md)).
- **The shell**, replaced by the port's SDL platform: MCI CD audio
  (`004F5820`; the port plays the same tracks, [AUDIO_RUNTIME.md](AUDIO_RUNTIME.md#shared-music-selection)),
  the message box (`004F4F70`), the working directory and the
  "systemmemory"/"emulation" options (`004F4D20`, the launcher's settings
  instead), and the CRT and DirectSound error texts (`004F4060`,
  `00510040`), which only a failing platform call reaches.

The joystick, the mouse and the DirectInput devices stay counted: retail
plays with a joystick and a mouse, which the port does not read yet
([INPUT_RUNTIME.md](INPUT_RUNTIME.md#analog-consumers-and-device-notifications)).

## Sound errors

`00510040` turns a DirectSound result into its text ("Allocated to another
client", "Already initialised", "Buffer memory lost", ...). `00510290` makes
a lost buffer (`0x8878000A`) an error object (table `00529868`, `0x2202`),
and `00510370` restores a buffer and plays it again (`+24` status, `+50`
restore, `+2C` play) when the status says it was lost.
