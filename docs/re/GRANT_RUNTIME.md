# Infestation live grant mutation

This authority owns the shared Set `004750C0`, Merge `00475350` and Copy
`00475380` policies on live grant masks. The
[bounded difference](GRANT_DIFFERENCE_RUNTIME.md),
fixed formatter and
[player-message gate](PLAYER_MESSAGES.md) retain separate owners.
Complete intro and briefing acceptance belongs to
the Infestation objective.

## Original owners and units

Each native grant subobject has a header DWORD at `+0`, six mask DWORDs at
`+4..+1B`, and a notification recipient DWORD at `+1C`. Player grants belong
to player `+9C`; their mask is player `+A0..+B7`, and their normal recipient
is the actual message subobject at player `+88`. Selector-12 grants belong
to actor `+24C`, with six words at `+250..+267` and the current recipient at
`+268`. Those recipients are separate from first-player selection.

Query `004752E0` consumes the LOWWORD of the raw DWORD ID. ID zero returns
AL 1 before any owner or mask access. Other IDs read one DWORD and test one
bit. Six complete words represent IDs 0 through 191. LOWWORD 192 and above
reach adjacent native storage, whose raw truth values remain diagnostics.
Portable queries report unrepresented storage rather than clamping or importing
those values. An owner is required at the actual reached field before a storage
boundary.

Record calculation `00475320` performs address arithmetic without reading
record data. Below `0x95` it returns `0053ABD8 + 8*id`; otherwise it returns
`0053B088 + 16*(id-0x95)`. Here `0x95` is 149 decimal. The represented original
record domain is 0 through 174; the six-word mask has additional bits. Record
text at `+0` and dependencies at `+8/+A/+C` are WORDs; flags at `+4` are a full
DWORD. Only the H0
lookup interprets its index WORD as signed; dependency queries use LOWWORD
mask semantics.

Set, Merge and Copy return void with RET 4. Query truth belongs to AL, and
record calculation returns its full address DWORD with RET 0. Incidental EAX
values from the void bodies do not become changed-bit or grant-count results.

## Set and retained failure prefixes

Set first queries the target. LOWWORD zero and already-owned nonzero IDs
return before process, executable, recipient, string or formatter inputs.
For a new bit, it rereads the live word, ORs the bit and commits at
`0047510A`. Every later required-data or service failure retains that write.
The subsequent record calculation still consumes no record bytes.

The full network DWORD at `0055F448` is read at `00475113`. Any nonzero value,
including `00000100h`, skips notification and proceeds to dependency closure.
Missing process state is reached after the new bit; phase, RNG and process
NULL-handle state do not substitute for this DWORD.

Offline notification follows these phases in order:

1. Read the current recipient at `00475121`. NULL skips all text, flags,
   string, formatter and message phases, then proceeds to closure.
2. Read the record's text WORD at `0047512C`. Zero skips even the flags read,
   then proceeds to closure.
3. Read the flags DWORD at `00475138`. Bit 0 takes precedence over bit 3:
   bit 0 selects Single, otherwise bit 3 selects category `13Bh`, and its
   absence selects category `13Ch`. The record string is acquired before
   the category, while Pair formatting processes category first.
4. Execute the separate fixed formatter, with genuine NULL results and
   current CRT bindings kept distinct from nonNULL source backing.
5. Reread the current recipient at `00475270`, then call the message body at
   `00475273` with flag DWORD zero. Only this body reads suppression. A
   suppressed send returns successfully; enabled storage/UI remains required.

These are lazy phases. Suppression cannot skip acquisition or formatting.
A later failure stops before dependency closure without rolling back the
new bit or earlier completed phases. Record reads beyond the represented
domain require their own boundary when reached. A high mask ID can still
complete without adjacent record consumption on an online or NULL-recipient
path, provided the closure's reached records are available.

## Canonical H0 capture

The setter performs H0 lookup inline. A negative signed index returns NULL
before reading count or slots. For a nonnegative index, unsigned `index > count`
returns NULL. Equality reads one-past storage and is a distinct boundary;
it must not become an in-range hole. An in-range occupied slot captures its
actual registration and immutable allocation.

Canonical final H0 table `00527598`, published by `004740E4`, has virtual
`+0 = 004EFBF0`. Its complete `xor eax,eax; ret 4` body returns NULL without
reading its receiver or argument. An in-range hole therefore supplies genuine
NULL, rather than a lazy loader or an empty string. The
[shared private capture](../../crates/inf-game/src/object_runtime/h0_text.rs)
also serves [player commands](PLAYER_COMMAND_RUNTIME.md), whose inline source
has the same complete decision matrix. Other resource consumers retain their
own missing-residency decisions.

Record-call physical operands retain the recipient's upper WORD and text
index WORD; category calls use full `13Bh` or `13Ch`. The callback does not
consume those operands. Native argument values remain ABI evidence, separate
from portable player IDs, actor IDs and source slice addresses.

Use the resident registration's handler, region and local index together
with `StringEntry.record`, which includes the original NUL. Renderer string
data excludes NUL and is not complete formatter backing. The two captures
remain borrowed from their actual immutable allocations through formatting.
Canonical H0 and the admitted CRT path have no mutable external callback;
source replacement, temporary vtables and concurrent writers remain outside
this stable domain. Current CRT NULL/classification data has its separate
semantic owner; executable startup bytes do not establish global immutability.

## Ordered dependency closure

After notification completes or is skipped, Set visits candidates
`0x95..=0xAE` (149 through 174 decimal), ascending. It freshly queries each
candidate on the target; an owned candidate skips its dependency bytes.
For an unowned candidate, it reads dependency WORDs `+8`, `+A` and `+C` one
at a time. Zero skips the query. The first nonzero owned dependency causes
immediate recursive Set, which completes before any later dependency or
candidate is considered. An unused later WORD is not read eagerly.

The parent bit is already committed. A child also commits its bit before its
lower phases; a child failure retains both bits and stops the parent's suffix.
ID-zero query truth does not make a zero dependency trigger recursion, since
this consumer explicitly skips zero.

## Merge and Copy

Merge visits source IDs 0 through 174 ascending. For each ID it queries the
live source first, then performs the full target Set if owned, before querying
the next source ID. It does not snapshot all source words or stage a detached
union. The ID-zero source query and target Set require neither owner. Empty
source masks can leave an invalid target unused; invalid nonzero source
storage wins before target access. Equal identities still perform the reached
reads. Source bits 175 through 191 are excluded, while existing target high
bits are retained.

Copy visits word indices 0 through 5 ascending, reading each source DWORD at
`0047538F` and writing the target at `00475392` before the next source read.
It copies all six words, including high bits, and can clear target bits.
Equal identities still authenticate, read and write. Source failures precede
target failures. Header and notification-recipient fields remain untouched;
process, records, dependencies, H0, formatter and messages are unused.

The literal Copy caller at `0042F5B5` follows a reverse Merge at `0042F5AD`
and precedes event-22 broadcast at `0042F5C6`. Its receiver `+198` owner is
not represented by the player/selector-12 pair. It must not be aliased to a
similar-looking family word array or claimed as a production caller here.

## Implementation and verification boundary

The [shared live kernel](../../crates/inf-game/src/object_runtime/grants.rs)
names an actual Player9c or Selector12Actor24c owner and explicit Set, Merge
or Copy operation in one request. Reached
fields use actual live storage, with current process, executable, residency
and CRT data borrowed independently. No generic loader, successful message
host or duplicate raw/semantic recipient mirror is required.

The [player constructor](PLAYER_CONSTRUCTION.md#bound-initial-message-and-grant-phases)
borrows its actual unpublished mask/message through the same complete setter,
before FIFO publication. Public owner authentication remains in published
Set/Merge/Copy dispatch. The existing pure formats API, staged pickup
publication, save restore and bounded difference policy retain their distinct
contracts.
Sharing the stable owner identity does not establish equivalence between
those different consumers. Raw partial masks, native overlaps, arbitrary
recipient rebinding and allocation/arena reuse remain separate diagnostics.

This bounded kernel does not establish complete caller attachment,
enabled message storage/widgets, enclosing pickup/capture/screens, selector-13,
allocator timers, or matched retail scene acceptance. The separate
[player-command kernel](PLAYER_COMMAND_RUNTIME.md) owns Query/Set; captured
commands `0`, `1` and `48h` still require their enclosing caller composition.

## Focused verification

The [seventeen ordinary tests](../../crates/inf-game/src/object_runtime/grants/tests.rs)
pass with the locked release dependency graph. They distinguish LOWWORD-zero
and owned-bit bypasses, owner authentication before mask boundaries, full
network flags, NULL selector-12 bindings and text-zero flags bypasses. Actual
Boot registrations supply complete NUL records independently of their global
indices. Signed, greater-than-count, one-past and canonical hole policies stay
distinct.

The tests retain a new bit across missing process, source and formatter
inputs, and retain both parent and child bits when recursive closure fails.
They pin record-before-category acquisition, bit-0 format precedence,
formatting before suppression, actual second-player message identity,
source-first Merge and six-word Copy. Equal owners still authenticate;
unconsumed actor/player/message, arena, registry, list and resource custody
is compared. Explicit structural-EXE variants exercise field boundaries and
signed indices without changing any original file.

```text
cargo test --locked --release -p inf-game --lib object_runtime::grants::tests
```

## Original-byte execution

check_live_grants.py authenticates supported EXE
SHA256 `BF09F66E3685D4990D95DA13D4BE1CA7E293FB31CDDDCCC90BD575882921E4956`,
unchanged `.text` and FPCW `027F`. Each request executes authentic nested
Set/Merge/Copy, query, record calculation, canonical H0 miss, fixed CRT and
message bodies in one CPU. Thirteen complete executed bodies, two static H0
publication bodies, the grant constructor fragment, twelve data guards and
twenty instruction guards have separate byte/digest authentication. Static
construction/publication guards do not claim execution of those bodies.

The [fixture](../../crates/inf-game/tests/fixtures/live_grants_native.json)
contains 24 histories/28 requests: fifteen complete histories/nineteen calls,
six required-input prefixes and three named raw diagnostics. Across all cases
there are 2,654 body entries/2,641 authentic returns, 4,064 recorded nonstack
reads and 45 grant writes. Six message entries include five successful RET8
returns and one stop before enabled BYTE `+3D`; eight CRT entries include six
completed formatter returns. There are no faults, patched instructions,
imports, fabricated lower returns, prefabricated output or unlisted body
entries. Unrelated inherited harness preparation is unarmed and excluded.

Prefixes stop before missing process, record, H0, NULL-substitute and
classification inputs, or enabled message storage. Each retains the already
committed bit. Native formatter output is captured from the authentic caller
stack buffer before reuse. The complete initialized result includes NUL;
partial temporary output is native evidence only. Query AL, record addresses,
void returns, physical H0 arguments, native ABI and measured cadence remain
separate from portable identities and result/state parity.

The finite asset declaration authenticates PRELOAD and three original Boot
H0 banks: `0.ovl`, `0l0.ovl` and `2.ovl`. Their 692 complete NUL records produce
691 final occupied slots in capacity 950. Slot 355 is deliberately overwritten
by language local index 359 after local 346. The declaration retains both
records and registration order; it does not execute a Python residency owner
or original loader. Assets mapped but unbound in inactive/missing-pool cases
are unused native backing, without portable allocation-presence parity.

All known stack reads/writes and saved-register restoration are checked during
original execution. The fixture retains disjoint known spans, explicit unknown
omissions, entry/return frames, output writes and finite unknown/reset events;
it does not serialize a full known-stack access or saved-register timeline.
The only finite unknown consumers are the dead precision load, deterministic
precision reset and final NUL AND. Reused known stack bytes remain known;
requests do not reset or poison them to force a fixed event count.

Independent regeneration reproduced fixture SHA256
`9a30fbaa3f222d18484be3fab586d603c66baec7c5d96a10fe670651710bda31`.

The [independent native audit](../../crates/inf-game/src/object_runtime/grants/native_evidence.rs)
authenticates the producing script, fixture, original bytes and finite asset
declaration, then validates the nested read/write, return, formatting and
retained-state protocol for all 24 histories/28 calls. It never invokes the
portable kernel. Its stack-access counts and sparse masks supplement the
producer's online checks; they do not imply a serialized stack/register replay.

The [typed receipt test](../../crates/inf-game/src/object_runtime/grants/native_receipts.rs)
constructs actual Boot residency and actual player/selector-12 owners, then
invokes the public live API for 21 histories/25 calls, including all six
required-input prefixes. It independently verifies every one of the 692
original NUL records and all 950 final H0 slots, including the overwritten
registration. Expected masks are compared only after the API runs. Full live
player, actor, arena, registry, list, allocation and borrowed-resource custody
remain part of the comparisons; native pointers, cadence, stack temporaries
and incidental registers are not portable state. Both tests pass with the
locked release dependency graph.

Mask192/recipient alias, offline191/adjacent record and late unknown Copy-word
histories remain explicit raw diagnostics. They are excluded before portable
construction, without turning their physical outcomes into a mask or bounds
policy. Original constructor/loading, arbitrary pointer/binding/H0 mutation,
enabled UI, enclosing callers and retail acceptance remain outside this corpus.

```text
python check_live_grants.py --output .tmp/live-grants.json
cargo test --locked --release -p inf-game --lib object_runtime::grants::native
```
