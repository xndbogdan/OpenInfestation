# Shared actor angular response

This document owns `0047D440` and the current process-up input used by its
callers. The supported `Infest.exe` SHA-256 is
`bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956`.
The complete leaf `[0047D440,0047D66F)` is 559 bytes, SHA-256
`317bfbc44cd3ddea881725cbbc1c001f3903ec06c262fde011424a08196c9838`.
Static disassembly supplies its authority; the recovered C index omits callers.

## Caller inputs and mutation

[`ObjectArena::apply_actor_angular_response`](../../crates/inf-game/src/object_runtime/actor/angular_response.rs)
receives one `ActorAngularResponseRequest`: current float axis, desired float
axis and full signed DWORD gain. These are actual caller inputs. The owner
does not select basis-Y, world-up, elapsed units, normalization or a class.

Eight original direct call sites are `0042674A`, `0043F1D0`, `0043F21A`,
`0043F2C8`, `00440429`, `00455C13`, `00457BDC` and `004823DA`.
The `00426730` and `00482160` callers bypass zero power and derive gain from
wrapping power times delta followed by logical shift 14. The `0043Fxxx`
calls use copied/synthesized vectors and distinct saved-gain or delta-shift
policies. Contact `004559E0` supplies a terrain/contact-derived axis after its
linear writes. `00457BDC` follows a local impulse and precedes angular clamps.
Constructor93 supplies fresh basis-Y and current process up with delta shifted
by three. Its near return bypasses this leaf. Similar appearance does not make
these caller policies interchangeable with camera orientation or control motion.

Only world-angular DWORDs `+94/+98/+9C` change. Position, linear velocity,
basis/counter, dirty WORD, lists, registry, players and process data are untouched.
The upper-dot return occurs before any actor-storage read, so a typed invalid
receiver can be bypassed there. Reached actor errors preserve state. Complete
typed arrays do not claim raw alias, dangling pointer or partial fault parity.

## Dot, cross and recovery

`0043B2B0` accumulates `(desiredZ*currentZ + desiredY*currentY) +
desiredX*currentX`, then spills to float. The supported CRT selects 53-bit x87
arithmetic; the existing basis math policy uses retained `f64` operations and
explicit `f32` spills. The source thresholds are float bits `3F7FF000`
(0.999755859375) and `BEFA0000` (-0.48828125).

- Dot at or above the upper threshold returns without mutation.
- Dot at or above the lower threshold computes current cross desired. X/Z
  stay retained; only Y spills to float at `0047D50D`. Each axis independently
  reads signed current angular velocity: matching signs, including zero,
  select signed truncation of gain/3; opposing signs select the original gain.
  All three CRT truncation/low-DWORD conversions precede ordered wrapping
  world-angular additions. Gain zero is not an entry shortcut.
- Lower or unordered dot takes recovery. `0047A680/00490870` projects full
  signed angular DWORDs into the actual basis. Only local Z strictly inside
  `(-4800h,+4800h)` responds: positive selects `+2000h`, zero/negative selects
  `-2000h`. Gain is ignored. `0047B4B0/0047A6E0/004907E0` transforms that local
  impulse back to world using nearest-even QWORD/low-DWORD conversions, then
  performs the same additions. General point-transform order and WORD transforms
  are different policies. Unordered cross follows the nonpositive sign branch;
  masked invalid CRT conversion yields low DWORD zero.

The [focused tests](../../crates/inf-game/src/object_runtime/actor/angular_response/tests.rs)
cover exact thresholds, independent signed gains, retained X/Z versus spilled
Y, strict recovery bounds, zero-gain recovery, full DWORD/nonidentity transforms,
wrapping angular-only custody and lazy errors. They are source-backed ordinary
tests; no new original-CPU or complete enclosing-callback receipt is implied.

## Current process up

CRT initializer array `[0052E000,0052E190)` registers `0048F630` at `0052E154`.
The `0051237E` loop invoked at `00512297` calls nonzero entries. Initializer
`0048F67F/684` selects `0055F280` and calls `00402690`, which copies three
float DWORDs in order, producing bits `[00000000,3F800000,00000000]`.
This startup value is `[0,1,0]`; the loader's preceding BSS zero is different.

The full supported file has 65 exact references to these three component
addresses. Its 17 address-taking uses consist of this initializer and 16
reviewed input paths whose callees neither modify nor retain the up pointer.
No later literal store/reset is present. This is bounded literal/known-callee
evidence, not proof against arbitrary computed writers or global immutability.

[`PortSession`](../../crates/inf-game/src/session_platform.rs) seeds one current
`process_up` owner once and retains it across screen, world and display
replacement. Camera, intro helpers and constructor93 forward that same current
data. No per-call startup fallback is supplied. The explicit constructor93
request can omit up and reaches its error only at `00440429`, after terrain/Y
and steering prefixes. Native future writer custody remains a separate domain.
