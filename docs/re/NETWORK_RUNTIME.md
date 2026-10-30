# Infestation network layer

Retail plays networked games through DirectPlay (IDirectPlay4: the calls
below are its `+38` GetCaps, `+64` Receive, `+68` Send and `+C8`
GetMessageQueue). The port has no network play yet, so the multiplayer
arenas [wait on it](CONSTRUCTOR_ASSETS.md#what-each-unbuilt-class-needs);
this note maps the transport layer, `00492060..00493F30`, from REA reads of
the retail executable, and the game's protocol above it: its messages,
scores, the Capture the Banner banner and the network menus. The arenas'
other classes are not covered.

## Session state

The session's DirectPlay object is `0055F3B0`, its group or player id
`0055F3B8`, and the network flag `0055F448` (which much of the game tests)
is set when a session starts. The player ids live in a table of sixteen at
`0055F400..0055F43C`; `0055F2F4` is the local player's slot (from 1) and
`0055F2F8` marks it assigned.

| Routine | Does |
| --- | --- |
| `00493150` | Initializes: slot 1 assigned, `CoInitialize`, and the events `0055F3DC`, `0055F3EC` and the send event `0055F3E8` created when missing; a failed `CoInitialize` returns an error |
| `00493300` | Starts the session: without a DirectPlay object it fails with "Session not created"; it clears the player table, puts the session's id in its first entry, reads GetCaps (`+38`) for its limits, allocates the message buffers (`004923D0`), clears the channel state at `0055F450` and the two send rings at `0055F480`, and creates the receive, send and heartbeat threads; then sets the network flag |
| `00493260` | Shuts down: `00493650`, then closes the three events |
| `004923D0` | Allocates the message buffers, their size clamped to `0x200..0x400` |

## Threads

| Thread | Does |
| --- | --- |
| `00493C00`, receive | Waits on its two events; on the first, loops Receive (`+64`) into a ring slot (`00492770`): a system message (from 0) goes to `00493A50`, a game message larger than three bytes to its channel's queue (`004926E0`), committed with `00492790` |
| `00493CC0`, send | Waits on its two events; on the first, drains both send rings (`00492720` peek, `00492740` pop) through Send (`+68`) to the session's id, counting failures at `0055F394` (with the error's text, `004928C0`) and successes at `0055F398` |
| `004934C0`, heartbeat | Every second until its event is set, while the network flag and the session stand: counts `0055F390`, and with `0055F3BC` set and no slot yet finds the local id in the table or takes the first free entry |

## Messages and rings

A ring (`004927B0` builds it, `00492800` frees it) holds a read index `+0`,
a write index `+4`, a count `+8`, a slot size `+C` and its storage `+10`.

| Routine | Does |
| --- | --- |
| `00492770` | The slot at the write index (and the slot size), for a message being built |
| `00492790` | Commits it: the write index moves on unless the ring is full |
| `00492720` | The slot at the read index, or none when empty |
| `00492740` | Pops it |
| `00492EC0` | Starts a message on a channel: a slot from the channel's ring, its header (two zero bytes, the channel, the local slot) |
| `00493790` | Finishes it: its length into the header, committed, and the send event set |
| `004926E0`, `00492630` | Queue a received game message on its channel (`0055F308` + channel × `0x14`), delivering it through the channel's callback |
| `00493A50` | Handles a system message by type (below `0x32`; `0x31` sets `0055F3A8`) |
| `00492FA0`, `00493100` | Write the player table into a message |
| `00492FF0` | Applies a received player table, counting `0055F38C` |
| `00493810` | Copies the player table out |
| `004937E0` | The session's GetMessageQueue (`+C8`) |
| `00493840`, `00493950` | DirectPlay calls over a cleared `0x50`-byte description (no DirectPlay object: `0x88770082`) |
| `00493F30` | Copies data into a new moveable global block (`GlobalAlloc(0x42)`) |
| `00492060` | Normalizes a vector after rounding each component to 1/4096 |

A frame is a send ring's slot: a WORD length, the channel and the sender's
slot (`00492EC0` writes these, then the protocol's header), then entries.
`00492500` (channel, id, argument) encodes a message into a 0x200-byte
buffer through its row's encoder; a zero length sends nothing. The entry is
the id byte, with bit 7 and a WORD length when `0054073C` is set (it is),
then the payload. An entry longer than the frame's room (a signed WORD
compare) sends the frame (`00493790`) and opens another, without checking
the fresh room. `00493790` opens a frame when none is, and sends it only
with entries; `00492E50` sends a frame with entries and closes it either way.
`00492630` reads entries while the WORD length lasts, stopping at an id past
the protocol's count or a sized entry its receiver reads otherwise.

Two protocols are built in. Channel 0 always takes `00540768`, the player
table: no header and one message, `00493100` out (the host's whole table of
17 DWORDs, a client's first entry) and `00492FF0` in. The host gives an
unknown sender the first free entry (a zero id is never found, so it fills
one too) and counts it at `0055F3D0` and `0055F384`; a client copies the
host's entries 1 to n and takes the index holding its own id as its slot,
except that a slot already set goes back to 0. Channel 1 takes the game's
protocol, or `00540780` by default: a sequence DWORD heads each frame
(`00492F50` out, counting `00540794`; `00492F70` in, into `0055F380`) and
its one message is a test pattern (`00492FA0`: `(7n & 0x7F) + 8` bytes, that
length, n, then bytes counting down to 1; `00492F90` reads the length back).
It does not receive (`+8` is 0), so `004926E0` returns 1 for its frames.
`004923D0` also raises every row's payload size below 4 to 4, in the
tables themselves, and the session start `00493300` builds each channel's
send ring with four slots of the frame size (`004928B0`).

The port's [network](../../crates/inf-game/src/network.rs) module holds
this transport: the ring, the framing, the route and dispatch, the buffers
and both built-in protocols, compared after every step with the original
instructions by `check_net_transport.py`.
Nothing drives it yet: DirectPlay, its threads and the sessions are not
ported.

## The port's transport

DirectPlay is not ported. In its place,
[`network::udp`](../../crates/inf-game/src/network/udp.rs) answers the calls
the game makes of it, over UDP:

| Retail | Port |
| --- | --- |
| `EnumConnections` (`00493F90`, callback `00493F30`): the providers Windows has (Serial, Modem and TCP/IP on Windows 11; IPX too in 2000) | one connection, "Internet TCP/IP Connection" |
| `EnumSessions` (`004940A0`, flags `0x11`: asynchronous, available sessions only), with the TCP/IP provider's dialog asking for the host's address (blank searches the LAN) | `SessionSearch`: each refresh queries the LAN's broadcast or the addresses typed in, and lists the sessions with room that answered in the last 3 s |
| `Open` with `DPOPEN_CREATE` (`00493840`): flags `0x2044` (DirectPlay protocol, keep-alive, host migration), application `{9FA88941-E07E-11D3-BD94-00105AAEB976}`, 16 players; the session named from `GetComputerNameA`, the player from `GetUserNameA` | `NetSession::host`, on UDP port 47630 |
| `Open` with `DPOPEN_JOIN` (`00493950`) and `CreatePlayer` | `NetSession::join`, which waits for the host's answer as `Open` did |
| `Send` with no flags, to all players (`00493CC0`) | `send_all`: one unguaranteed datagram to each other player |
| `Receive` (`00493C00`), and the system messages `00493A50` reads: 3, 5, `0x31`, `0x101` | `receive`: the game messages, and `PlayerCreated`, `PlayerDestroyed`, `SessionLost` and `BecameHost` |

The host answers searches and joins, and every second resends the player
list (ids, names and the addresses it sees), so a lost packet only delays it.
Every peer sends to every other, as DirectPlay's peer-to-peer sessions did,
and pings them each second. A guest silent for 10 s is dropped by the host,
and hears the list without it once (`SessionLost`). A host silent that long
is dropped by the guests that still hear someone: the lowest remaining id
becomes the host (`BecameHost`); a guest that hears no one loses the
session. The wire format is the port's own, so retail clients cannot join.
`a_searched_session_takes_a_player_and_carries_messages` and its neighbours
run real sessions over loopback.

Retail's three threads run beside the game; the port runs their bodies from
the frame through [`network::link`](../../crates/inf-game/src/network/link.rs)
and [`network::session`](../../crates/inf-game/src/network/session.rs):

- The session start `00493300` heads the player table with the session's id,
  rebuilds the buffers at `0x400` (`004923D0`'s top; DirectPlay's TCP/IP
  buffers are larger) and both send rings, and sets the network flag.
- The heartbeat `004934C0` runs once a second. The host takes a slot when it
  has none (the entry holding its own id, else the first free one, which it
  fills) and resends the player table after eleven quiet beats (`0055F4AC`),
  or at once when it added a player (`0055F3D0`); a guest sends its id until
  it has a slot. Then channel 0's open frame goes out. Over loopback a host
  takes slot 1 and its first guest slot 2.
- The receive thread `00493C00` drops messages of three bytes or fewer,
  writes the rest into the receive ring and routes them (`004926E0`),
  keeping the slot when the route asks; `00492830` later drains it to each
  channel's protocol. System messages go to `00493A50`: a destroyed player
  leaves the table, `0x31` sets `0055F3A8`, and `0x101` sets the host flags
  (`0055F3BC`, `0055F3A0`, `0055F3A4`).
- The send thread `00493CC0` takes one committed frame from each send ring in
  turn until both are empty. Its counter `0055F3C8` is never written, so
  `00493830` returns 0, and the port's sends leave at once, so
  `GetMessageQueue` (`004937E0`) has nothing queued.
- The shutdown `00493650` clears the network flag and the table, and puts the
  slot back to 1.

## Errors

`004928C0` turns a DirectPlay `HRESULT` (`0x8877xxxx`) into its text.
`004922C0` and `00492350` build the error objects the layer returns
(`00528CF8`, and `00528D00` with up to 255 characters of text);
`00492300` formats one as "Status 0x%08x (%s)".

## The game protocol

The game hands the layer a protocol object, `00538320`: `+0` reads a frame's
header (`0045C3D0`), `+4` writes one (`0045C3A0`), `+C` is the message count,
45, and `+10` the message table `00538050`. Hosting (`00494230`, called at
`0045F32C`) needs a DirectPlay object, names the player from
`GetComputerNameA` and `GetUserNameA` ("Session" and "unknown" when they
fail), creates the session through `00493840` and starts it (`00493300`);
joining goes through `00494190` (at `0045F8B6`).

A header is four DWORDs, copied to `0054E788..0054E794`: a packed status, a
session word that must match the sender's (`0054E338` by sender slot), a
sequence the sender's record must not have passed, and the sender's slot in
bits 16..23 with a host flag in the top byte. The status's low five bits are
the sender's state (`0x1F` reads as 1, others plus 2, in `0054E380`). From
the host it also carries the game settings into the settings object
(`00463AB0`): bits 5..9 to `+4C`, 10..14 to `+1C`, 16..22 to `+10` (bit 23 to
`0054E81C`), 24..25 to `+70` (a change rebuilds through `004502C0`) and bits
26..30 to the flags `+20`, `+24`, `+28`, `+2C` and `+34`.

In order, `0045C3D0` keeps the header, then needs the sender's session word
and a slot record (`00462460`: `0054E888`, `0x18` bytes a slot, count
`0054E88C`); without them the frame ends there. The frame goes on only when
the header's WORD 6 is the mission's current world (`+A0`) and its byte
`+12` is set, which also marks the slot present (`0054E400`). A sequence
below the record's `+4` returns without the status; otherwise the record
takes it, and the state and a host's settings follow. A changed detail
rebuilds only when H12's first slot is resident or loads, else `0054E84C`
counts the refusal. The writer `0045C3A0` copies `0054E838..0054E844`.
[`network::game`](../../crates/inf-game/src/network/game.rs) ports both,
the packers and the scores below; `check_net_game.py`
runs the originals over 120 packed vectors, 27 headers and 49 score cases,
and the port matches each. The DWORD packer picks its largest component by
signed compares, so a magnitude that wraps (`i32::MIN`) is passed over; the
WORD packer shifts at least once, and keeps its shift plus 3 modulo 4.

### The protocol object and each frame

[`network::protocol`](../../crates/inf-game/src/network/protocol.rs) is
`00538320` itself: the header in and out, no receive on the receive thread,
and the table's 45 payload sizes (raised to at least 4 when a session
starts, as `004923D0` raises every installed protocol's). A sender hands
`00492500` an argument its encoder reads, which the port names a
`GameSend`; a receiver's own sends wait until the drain ends, which keeps
their order in the frame. Messages 38 to 41 reach the
[territories](#the-territory).

[`network::game_tick`](../../crates/inf-game/src/network/game_tick.rs)
ports the work around it:

- `0045C320`, when the header's status or world changed: the sequence
  (`0054E840`) moves on, word 1 takes this machine's player id
  (`0055F39C`, the session's `0055F3B8`), the slot goes in byte `+E` and,
  as host, again in `+F`; the slot's state becomes the status's low five
  bits plus 2, or 1 from `0x12`.
- `0045CB70`, when a game starts: the session words, scores, kill words,
  this machine's kill word and the game state (`0054E820..0054E82B`)
  cleared; every slot named "Player N", absent and unheard; the line groups
  loaded once (kinds 0, 1, 2 and 4: `kill.txt`, `Die.txt`, `selfkill.txt`,
  `Host.txt`; `Names.txt`, kind 3, is never loaded, its count `0054E80C`
  only ever cleared); the message lines idle; the states and every task's
  five team words cleared (a missing task faults); team play `0054E828` set
  for modes 0 and 4.
- `0045C940`, each frame: once, this slot's name and initials from the
  identity (`00463D10`) and, as host, the winning score (settings `+30`) as
  the limit; then each other slot whose player id changed in the table: its
  scores cleared, only the low WORD of its kill word, its record's sequence
  reset, absent and to be asked its name, and when another player had it,
  every object its table holds disposed (the caller then drains them).
- `0045C5A0`, each frame with its delta: the delta gathers until more than
  `0053804C` (0.1 s). Then the host sends the game state (28), the world's
  values (36) and the territory list (38), and message 42 while a race is
  built (`0054ED70`); once the game has
  ended, every 150 clock ticks a line from `Host.txt` (one of those after
  its first, by the RNG) names the winner, unless team play. A guest sends
  its kill word (26) when its deaths moved. Each slot heard without details
  is asked its name (29). Each local object (its slot's table, from entry
  1) whose countdown `+DC` falls below the gathered time sends its record
  (1), counts `+E0` and restarts at `+D8` plus a jitter of the RNG's low
  WORD times a quarter of the delta, shifted down 13, unless `+38` bit 15
  holds it; the others count down. One other slot a time (`00538344`) has
  its objects whose `+3C` passed the clock expire (virtual `+C8`, which
  asks the owner again with message 7). Channel 1's frame goes out either
  way. The flow control (`0054E858`, `0054E85C`) never holds a send back:
  the port's sends leave at once, so `00493830` and `004937E0` read 0.

The tests drive both over a fake world, and a frame from a host to a guest
in the same world.

### Messages

Each row of `00538050` is a receive handler (ECX the payload, EDX the
header, the sender's slot at its `+E`), an encoder (ECX the buffer, EDX the
room; it writes the fields that fit and returns its length, or -1), the
message id and its payload size. Handles are the 32-bit object handles
`0047AD90` resolves; their top five bits are the owner's slot, and an actor
whose `+2C` bit 31 is set is another machine's copy.

| Id | Size | Receive | Encode | Does |
| --- | --- | --- | --- | --- |
| 0 | 0 | `0048C860` | `0045C140` | nothing |
| 1 | 8 | `0045DC00` (`0047E600`) | `0045DBD0` (`0047E330`) | an actor's state record. The encoder writes only when the room reaches the actor's `+E4` and keeps the length there. On receipt an unknown handle builds the actor (`0048E170` for ids below `0x4000`, else resource `00460310`) and a failure announces it (`00492500`); a local actor named by another machine is logged to `c:\bad_lode.txt` ("Strange error: Received update f...", with both ids) |
| 2 | 4 | `0045DE90` | `0045E0B0` | dispose of the actor (`00479FA0`) |
| 3..6 | 4 | `0045DDD0`, `0045DE00`, `0045DE30`, `0045DE60` | `0045E0B0` | set the actor's state 4, 3, 1 or 0 (`0047D670`), then dispose of it |
| 7 | 4 | `0045DEB0` | `0045E0B0` | a handle another slot gives up: when it is unknown here and the sender owns it, free its slot (`00462480`, `00462510`, `00462560`); then announce the handle, and a local actor's `+84` |
| 8 | 8 | `0045EB30` | `0045ED70` | A is taken by B: unless A is held, A's `+8C(1, B)` when local, then B's hold (`+BC`) flashes at A (its `+C`) |
| 9 | 4 | `0045EBA0` | `0045ED70` | A, held by B, is let go: B's hold sets it down (its `+4`), or the ground (`00479DD0`), and a local A's `+8C(0, NULL, +D4)`; the senders' pairs are [B, A], which `0045ED70` writes second first |
| 10, 11 | 8 | `0045ED30`, `0045EDA0` | `0045ED70` | a local banner (A's `+B8`) taken by B (`0045B9E0`) or dropped (`0045BA50`) |
| 12 | 12 | `0045BB90` | `0045BC60` | a local banner captured (`0045BA90`) by a carrier and a team |
| 13..17 | 4 | `0045BC10`..`0045BC50` | `0045BD70` | a banner event 0..4 (`0045BBD0`: `0046CC50` on the banner) |
| 18 | 8 | `0045DFB0` | `0045DF70` | damage to a local actor (`0047C6E0`) |
| 19 | 4 | `0045E050` | `0045DFF0` | a world effect at a point (`004887D0`) |
| 20 | 4 | `0045EDE0` | `0045E0B0` | the Sonic Shock pulse from the actor (`0048D980`) |
| 21 | 4 | `0045E0E0` | `0045E0B0` | nothing |
| 22 | 16 | `0045E620` | `0045E0F0` | furniture states, below |
| 23, 24 | 16, 20 | `0045E9C0`, `0045EAD0` | `0045E960`, `0045EA60` | an effect between two points (`0045EA20`: kind 0 `004665D0`, kind 1 `00466710`) |
| 25 | 4 | `0045EE00` | `0045E0B0` | `00431EF0` on the actor |
| 26 | 4 | `0045D300` | `0045D7A0` | on the host, a slot's kill word (`0054E414`) changes: a kill (`0045D350`) |
| 27 | 4 | `0045D4A0` | `0045D780` | on the host, a score transfer (`0045D4C0`) |
| 28 | 40 | `0045D1E0` | `0045D5D0` | the host's game state: the score limit `0054E820`, `0054E824`, the end flags `0054E828`, the players' scores (six bytes each from `0054E79E`, by a presence mask) and the teams' (to the team objects `0055F108..0055F148`) |
| 29 | 4 | `0045D050` | `0045ED10` | a request for this slot's name, answered with it |
| 30 | 32 | `0045D160` | `0045D100` | a player's details (24 bytes to `0054E71C`) and name (64 bytes to `0054E5B0`); the slot becomes present (`0054E3EC`) |
| 31 | 64 | `0045D1B0` | `0045D0B0` | a text of up to 200 characters, shown through `004135D0` |
| 32..35 | 4..16 | `0045DB10`, `0045DBA0`, `0045DBB0`, `0045DBC0` | `0045DA90` | a sound (`0045DB20(0..3)`: `00487EC0` with an optional source and position) |
| 36 | 32 | `0045EC20` | `0045ECA0` | sixteen bytes into the player object (`0055F160`, `0048EA30`; encoded from `0048EA10`) |
| 37 | 4 | `0045EC70` | `0045ED10` | a value for the player object (`0048E980`) |
| 38..41 | 12, 4, 4, 4 | `00475B50`, `00475BD0`, `00475C60`, `00475DF0` | `00475B00`, `00475BB0`, `00475C40`, `00475DB0` | the entries of list `0055EF10` (count `0055EF28`): their `+90` bytes, an entry's `+8D` (`00475750`), its `+8C` cleared, and `00475E10` |
| 42 | 8 | `0046F5E0` | `0046F410` | the race's records 1 to 16 ([below](#the-race)) |
| 43, 44 | 8, 10 | `00403DA0`, `00403E50` | `00403D60`, `00403DF0` | for Powerball's ball (`005468D0`) when its holder `+14C` and sequence `+174` match and its `+148` is 2 or 3: `00402A40(1, ...)`, or `00403940` with the throw in mode 2; they use 6 and 12 bytes |

The encoders fed by their argument or the game state are in
[`network::encoders`](../../crates/inf-game/src/network/encoders.rs), each
with its room checks; `check_net_encoders.py`
runs the originals over 221 cases, rooms that cut each one short included,
and the port matches each byte. Two leave bytes they do not report: message
30's details (`0045D100`) write 22 bytes but claim 32, so the last ten are
whatever `00492500`'s stack buffer held (the port's buffer is zeroed), and
the text (`0045D0B0`, up to 200 bytes with its NUL) writes a second NUL past
its length. The game state (`0045D5D0`) carries `0054E820..0054E82B` as
three DWORDs, a mask of the slots 1 to 16 whose details arrived (`0054E3EC`)
and of the teams they play for (states 2 to 17), each such slot's kills,
deaths and score, and each such team's five words from `+0C`.

The receivers but 1, 22 and 38 to 44 are in
[`network::receivers`](../../crates/inf-game/src/network/receivers.rs):
each decodes its payload, reads the fields retail reads, keeps the
protocol's own state (kill words, scores, names, the host's game state)
and names every game routine it calls to a host, which a live session
will carry out. `check_net_receivers.py`
runs the originals over 129 cases, with recording boundaries for those
routines and traps for the actor virtuals they reach (`+8C`, `+B8`, `+BC`,
and a component's `+4` and `+C`); the port's calls, arguments, answers'
uses and lengths match each. Details the table above leaves out: message
7's owner is the handle's top five bits; message 8's component takes B's
handle, message 9's drop B's resolved object; banner events 13 to 17 are
2, 0, 1, 3 and 4 (`0045BC10..0045BC50`); a point effect gets ECX the kind
word's low half, EDX the first point, then the second point, the extra
(message 24) or the process NULL (23), and 1, and kinds above 1 do
nothing; a sound's position is its DWORD's three high bytes, each shifted
up a byte, and its two values default to `0x10000`, bit 0 of the id's
offset from 32 reading the second argument first.

Furniture states (message 22) walk the terrain object's furniture buckets
(`+80014` counts and `+80414` lists, 512 buckets of four-byte entries): the
encoder `0045E0F0` resumes at bucket `0054E864`, entry `0054E860`, and packs
runs of each entry's state nibble (`0xFF` for a destroyed one) in four forms
(`0045E400`, `0045E490`, `0045E550`, `0045E5C0`) by each entry's code
(`0045E0C0`: 2 destroyed, 1 a nonzero state, else 0); the receiver `0045E620`
applies them (`0045E780`, `0045E810`, `0045E8B0`), and only on a client.
`0045DC10` and `0045DD00` pack a vector into a DWORD: three 10-bit fields and
a 2-bit exponent, or three 9-bit fields and a 5-bit one.

In detail ([`network::furniture`](../../crates/inf-game/src/network/furniture.rs)):
the encoder takes at most `0x80` bytes (none below 16), finds from the
resume bucket the first record whose value is at or above the resume
value (keeping that threshold for the buckets after it), and packs runs
until fewer than four bytes are left or the bucket ends, which also ends
the frame. A run's byte is its form `<< 6` and its count (up to 64, which
spills into the form's bits). Form 0 writes a byte a record (`0xFF` for a
destroyed one), form 1 a nibble a record (low first, stopping at a
destroyed one), forms 2 and 3 count zero states and destroyed records
without bytes. A run starts in the form its first record suggests (2, 1 or
3 by `0045E0C0`) and falls back to form 0 when one of the next three
records would break it. The frame's head is the run count `<< 18`, the
start value `<< 9` and the bucket (the value is not masked, so a large one
spills into the count), or `0xFFFFFFFF` after a full turn of the buckets
without a match. The receiver finds the record whose value is the head's
9-bit value, then applies each run from that same record (`0045E780`,
`0045E810`, `0045E8B0`, `0045E910`), so a frame's later runs overwrite its
earlier ones; a host applies nothing. The port's terrain answers through
its sparse buckets (a bucket is the one at that `y`). No machine sends
message 22: of the 46 calls to the send `00492500`, every id is a
constant but the heartbeat's 0 (`004935DE`), and none loads `0x16`; the
encoder's cursor `0054E860`/`0054E864` has no other reader. The codecs
run only from the protocol's table, so the port keeps them checked and
refuses a received 22.
`check_net_furniture.py` encodes 20
sequences of frames from supplied terrains and feeds them to others; the
port matches every frame, cursor and resulting record.

### The classes' sync slots

Message 1's record carries, after its shared fields, what the actor's
virtual `+C0` writes (room in EDX, its length or -1 back) and `+C4` reads
back. The port has the built classes' pairs
([`network/sync.rs`](../../crates/inf-game/src/network/sync.rs)):

- family `40D800` (selectors 15, 37 and 39), `0040DEF0`/`0040DF20`: `+140`
  and `+13C` as a byte's high and low nibbles. On receipt the low nibble is
  ORed with `0x0F`, not masked, before it replaces `+13C`, and `+140` takes
  the high nibble; each is clamped to `0..+13C`.
- the shots 31 and 32, `00486310`/`00486370`: the WORDs `+138..+13D`, the
  first's low bit replaced by `+144` (ORed whole) and the second's by the
  sign of `+146`. On receipt `+144` is the first byte's low bit, and `+146`
  becomes -1 when the third byte's is set.
- the vehicle, `00420DA0`/`00420E70` (with `0041F9B0` keeping `+208`'s bit
  4): a WORD packing the mode `+1E8`, `+208`, `+238`, `+20E` and `+58C`, the
  speed over 16 (`+244` in modes 1 and 2, else `+220`), the low WORDs of
  `+1EC`, `+210` and `+1FC`, then the hold's handles up to its first empty
  slot (`004127A0`; `00412810` reads them back and empties the rest,
  `004120B0` tests one). A payload under ten bytes is left alone.
- the turret (selector 11), `004025C0`/`00402600`: one DWORD, `+18C` in the
  high WORD and `+188`'s low WORD in the low. On receipt `+188` takes the
  low WORD and `+18C` the high, sign-extended, whatever the length.
- the charge (selector 27), `004560D0`/`00456130`: nothing while `+154` is
  the process null, else the handle of what it sticks to and its offset
  `+158..+15D`, 10 bytes. The room checks are unsigned and the handle goes
  in before the second, as retail's. An empty payload puts the null back at
  `+154`.
- the factory (selector 15), `00431AD0`/`00431C00`: the family's byte, then
  a flags byte: `+160`'s low byte, `+17E`'s code at bit 3 (0 for 0, 2 for
  0xFFFF, else 1 and the WORD follows), 0x20 when `+180` follows, 0x40 when
  `+182` does and 0x80 when the docked vehicle `+188` does. On receipt
  `+160` takes the low three bits, `+180` is 0 when absent, `+182` stays,
  and `+188` becomes the process null.

`check_net_sync.py` runs them natively over 461
cases on supplied objects, and the port matches every byte and field
([`actor_record`](../../crates/inf-game/src/object_runtime/actor_record.rs)
also carries the last three through a record to a remote copy). An REA read
of every class table's `+C0` (2026-10-21; the tables found through their
shared `+84`, `+8C`, `+90`, `+94` and `+C8`) finds 50 on the shared actor's
`00405580`, which syncs nothing. The rest hold these pairs, the
[banner's](#the-banner-and-its-base) and the [ball's](#powerball).
Family `40D800`'s serves selectors 30, 37, 39, 47 and 49, and the family's
own table `005203C0`, which only `004603B0`'s case 23 builds: a bare family
object the port has no class for, so a world naming it stops at
construction.

### Message 1, the actor record

[`object_runtime::actor_record`](../../crates/inf-game/src/object_runtime/actor_record.rs)
ports both ends. `0045DBD0` refuses a room below the actor's last length
`+E4` and keeps each new length there. `0047E330` refuses a room below 12;
gives the dirty word bit 0 unless `+2C` is exactly 1; drops a held actor
whose holder is gone (`00479DD0`, then `+8C(0, NULL, +D4)`); writes the
handle, the type (the placement WORD, or `0x4000` plus the resource for a
placement of -1) and the dirty word as the record's flags, less the holder
(bit 3) when it is NULL and the basis, velocity, spin and position bits
(`0xF0`) when it is not, with `0x800`/`0x1000` for a zero velocity or spin
instead of the vector. The fields follow in bit order: the count `+E0`;
the flags' low WORD, the state `+C4` and class `+B4` bytes (written without
a room check); the owner `+88`; the holder `+70`; the basis's angles
(`00491E80`); the velocity and spin packed (`0045DD00`, `0045DC10`); the
position; the model `+42`; health `+A0` and its maximum `+A2`; then the
class's sync behind its WORD length. The dirty word loses bit 10.

`0047E600` resolves the handle. An actor this machine has takes `+3C` = the
clock plus 200; a local one (`+2C` bit 31 clear) is logged to
`c:ad_lode.txt` ("Strange error: Received update for my own object!",
then the ids; the port writes those lines to its own log instead) and
skipped, as is a record whose count is below `+E0`. Otherwise the
count is kept; bit 10 sets or clears `+39` bit 2; the flags WORD moves the
list memberships its bits 0 and 1 change (`00479E90`, `00479F20`) and
replaces `+2C`'s low WORD; a changed class marks the dirty word, a changed
state goes through `0047D670(0, NULL, state)`; the owner and holder are
taken or set NULL; a held bit that changed with a local holder whose
component does not hold the actor sends message 9 (holder, actor), else
clears `+2C` bit 5; the basis comes from its angles (`004911F0`); the
velocity and spin are unpacked (`0045DD90`, `0045DCC0`) or zeroed; a
position within 0x80 (`0048EFC0`, a quarter of the square when it
overflows, doubled) eases in by twice the difference added to `+8E..+92`
(`0047B510`), a farther one is taken; the model goes through `0047A110`;
then health, maximum and the class's sync. An unknown handle builds the
actor: below `0x4000` the current world's placement of that index
(`0048E170`, mode 2 with the handle), else the resource less `0x4000` at no
position (`00460310(0, 0, -1, 2, handle)`); a failure sends message 7 with
the handle. The new actor takes nothing from this record: the next one
fills it. A remote actor built from its resource has no position before
that record, where retail compares with its allocation's residue; the port
takes the record's position (`RemotePositionResidueApproximation`, logged
once).

`check_net_record.py` runs both natively over
the shared actor's table: 140 actors encoded at rooms from 11 to 512, and
146 records received into remote and local actors, positions near and far
included; the port matches every byte, call and field.

### Scores and the message lines

Players' scores are six-byte records from `0054E798` (kills, deaths and
score), up to sixteen slots. `0045D350` records a death for the victim (ECX)
and, unless it was a self-kill, a kill for the killer (EDX), with their
teams' totals; the killer gains 200, 300 when the victim led, and nothing in
modes 2 to 4. A self-kill costs 200 outside modes 2 and 3. `0045D4C0` moves
a score between two players (doubled when the loser leads). `0045C150` ends the game when a player's
score reaches the limit `0054E820` (the winner in `0054E822`, the end flag
`0054E829`); `0045C190` does the same for a team. The team objects are
`0055F100` by team.

Up to four message lines (`0054E460`, `0x54` bytes each, `0054E848` the
first) show for 7.5 seconds (`0045C8D0`) and count down 100 ms a tick
(`0045C1D0`, drawn through `004A3AB0`). `0045D7C0` writes a line, replacing
`@1` and `@2` with two players' names (or "a stranger" for an absent slot)
or two H0 strings. `0045CCE0` loads the lines for a kind of message: those
`0045CE60` reads from its text file (up to n lines in the folder `00463800`
names, skipping blank lines, leading spaces and `#` comments; the protocol's
strings name Host.txt, Names.txt, selfkill.txt, Die.txt and kill.txt), or
else one default: H0 `0x5B`, `0x5A` or `0x5C` for kinds 0 to 2 ("@2 Killed
@1", "@1 Died." and "@1 Died stupidly"; `00538354`, "null", when H0 lacks
one), and for 3 and 4 the empty string `005468D4`, which nothing writes
(the window's title, and the stand-in for a null text elsewhere). Retail's
install has none of the five files, so each group holds its default. The
network module's release `0045C0B0` frees the groups (`0045C100` each), as
`00493260`, the transport's release, runs the shutdown `00493650` and closes
its Win32 events and COM.

In detail ([`network::lines`](../../crates/inf-game/src/network/lines.rs)):
`0045C8D0` takes the first idle line from the head, or the head's own line
(moving the head on) when all four are busy. `0045C1D0` takes 0.1 s off
each line (stopping at 0), lets the head pass idle lines (four looks), and
draws from the head in H4 font 0 and H6 style `0x2A` at (20, 60), 18 lower
each, while a line keeps more than 0.1 s. `0045D7C0` copies its format into
`room - 1` bytes: `@1` and `@2` become the slots' names (a name with no NUL
in its 16 bytes runs on into the next slot's) or "a stranger", or with its
flag the H0 strings `0x38` on; a NULL string leaves its digit to be copied,
and another byte after `@` drops the `@`.
`check_net_lines.py` runs the three over 5
allocation runs, 12 tick runs and 13 formats; the port matches each.

### The banner and its base

Capture the Banner's banner is selector 40 (`0045B4A0` over selector 41's
`0047A7A0`, table `005260A0`). It keeps its state at `+138` (away from
home), `+139` (carried), `+13A` (being captured), its carrier at `+144`
(the process NULL when none), a capturing team at `+13C` with the carrier
it began with at `+140`, its home at `+148`, the time left away at `+150`
and its spin at `+154`. Selector 41 (table `005267E0`) is a base. Both
tables are the shared actor's but for the payload (`0045B9A0`, `0045BEE0`:
`0047AD70`, then the ground `00479DD0`; the banner keeps that spot as its
home), the update `+24`, which `+3C` (`00461C50`) also runs, and for the
banner the draw, the shots' `+40` (`004863C0`), `+B8` (`00404610`, itself)
and the record sync `+C0`/`+C4`. World 33 has two of each, the banners on
teams 3 and 2.

| Routine | Does |
| --- | --- |
| `0045B5A0` | the banner's `+24`: it turns a sixteenth of the delta (a thirty-second while carried) about Y (`00491530`), starting again from the axes `0055F290/280/270` each whole turn; it stays in the draw list (`00479E90`) until the game ends. Unless a capture is under way: a free banner is taken by the first player's vehicle within 1.5 of its radius (twice that vertically), live and of another team, when the vehicle can take it (`004212B0`: its wait `+59C` over and nothing at `+598`); a carried one is dropped once its carrier is gone, not live, or (when the carrier is this machine's) no longer holds it. A change on this machine's banner is told (2 taken, 1 dropped), then `0045B4E0`. A carried banner rides with its carrier; this machine's banner away from home goes back when `+150` runs out (event 4) |
| `0045BDB0` | selector 41's `+24`: the first player's vehicle within 1.5 radii of the base (3 vertically), live and on team 0, 1 or the base's, carrying (`+598`) a banner marks that banner captured by the vehicle's team (`+13C`, `+140` = `+144`) and, when the banner is local, scores it (`0045BA90`), else asks its owner (message 12) |
| `0045BB50` | the banner's `+2C`: the carrier's position while carried, then `0047AFC0` |
| `0045BA90` | captured: no longer carried or away, event 3, `0045B4E0`, back home; the capturing team gains a capture and 1,000 points, the banner's team a loss and 500 points off, or to zero at 500 or less (then `0045C190` on the capturing team) |
| `0045B9E0` | message 10 on this machine's free banner: the carrier's live vehicle takes it if it can (`004212B0`), event 2 and `0045B4E0` |
| `0045BA50` | message 11: dropped by its carrier, `0045B4E0`, event 1 |
| `0045B4E0` | another machine's banner tells its owner what this machine's carrier did (10 taken, 11 dropped, `[carrier, banner]`); this machine's sends its record soon (`+DC`) and, when carried, stays away 60 seconds with its position left out of its record (dirty bit `0x80` cleared), else is grounded with its position sent and no carrier |
| `0045BCB0` | an event 0..4 on a local banner: `0046CC50` on the first player, then messages 14, 15, 13, 16 or 17 `[banner, carrier]` |
| `0046CC50` | the first player's line by event (taken, dropped, captured: H0 `0x64`/`0x67`/`0x65` when its own unit did it, else `0x6C`/`0x6F`/`0x6D` when the banner is its team's, else `0x68`/`0x6B`/`0x69` when the carrier is; event 0 `0x66`/`0x6E`/`0x6A`; home `0x70` or `0x71`); a capture by its own unit also gives it the pickup (4, 5000) |
| `0045B820`, `0045B860` | the banner's `+C0` (the carrier while carried) and, on another machine, `+C4`: a capture under way is asked again (12) while the owner names the same carrier; the owner naming this machine's vehicle as the carrier of a banner free here is told it was dropped (11), and calling free a banner this machine's vehicle carries is told it was taken (10); otherwise the owner's word stands |

The vehicle's side: its update prefix (`00422300`) keeps `+598` only while
it names a banner whose carrier is the vehicle. In a network game with a
banner held, Drop lets it go (`+59C` = 1 s, `+598` cleared) instead of
dropping an item, and Pickup only zeroes `+59C`. The map marks banners
([briefing](BRIEFING_RUNTIME.md)).
[`selector40`](../../crates/inf-game/src/object_runtime/actor/selector40.rs)
ports them.

Selector 45 is an arena's spawn marker (`0045BF00` over `0047A490`, table
`0052616C`): the constructor clears its offsets, makes its team `+84` 2 and
links it into the spawn list (`0054E2F8`, node `+6C`), which its destructor
`0045BF80` unlinks. Its payload (`0045BFC0`, after `+08`'s `0047A610`)
takes the flags (`00479D70`), the team (`+4`: 0 is 1, else one more) and two
offsets (`+8`, `+E`) turned by its basis into `+78` and `+7E` (`0047A6C0`);
its commands (`0045C010`) set the team (3) or an offset as it stands (4, 5),
the rest going to the rotating visual's `0047A1A0`. Its `+24` and `+3C` are
`00405140`, its draw the visual's `0047AFC0`.
[`selector45`](../../crates/inf-game/src/object_runtime/selector45.rs)
builds it; Capture the Banner's host lists four, two for each team.

### The territory

Buggy Madness and Buggy Platforms score territories, selector 46
(`00475610` over `0047A490`, table `005278A0`): the arches hanging over the
arena floor (world 27 has eight, 390 to 600 units up, so a buggy claims one
by jumping through it). The constructor makes the score `+6C` 16 and the
period `+70` 5 s, numbers it `+78` one past the count `0055EF28` and links
it into `0055EF10` (node `+7C`); the destructor `004756C0` unlinks it. Its
payload (`004756F0`: `0047A610`, then `00475710` reads the flags, the score
times four and the seconds) and commands (`00475CB0`: 3 the score, 4 the
seconds) set them; world 27's are 100 and 20 s.

| Routine | Does |
| --- | --- |
| `00475800` (`+24`, and `+3C` through `00461C50`) | with fewer than two players present it has no holder; else its charge `+88` rises by delta/16 while held (to `0xFFFF`) and falls otherwise; with fewer players or the game over (the winner `0054E822` then holds it) the charge settles towards `0x8000`. The first player's live vehicle (`0046B030`, lifecycle 3) within `+28` of its centre (in 3D, `0048EFC0`) marks the side it is on by its local Z (`0047A660`): `+91` at or below 0, `+92` above. When both name this slot, the host claims it (`00475750`) and a guest stores its claim `+8C` and sends 39; the sides then clear. Past its period the host scores the holder its score (halved when it was taken from another), divided by `(own + 1) * 5 / (best other + 1) + 1` capped at 10, tells it (`00475E70`) and frees it |
| `00475750` | a new holder takes half the score from no one, or a quarter from the old holder, then `00475E70` and the score limit (`0045C150`) |
| `00475E70` | the amount at `+94`, the line here and message 41 to every machine |
| `00475E10`, `0046D050` | the line for the slot it names, in the first player's message: a score names the scorer and the previous holder (H0 `0x75`, or `0x76` without one), a change the holder (`0x73`) or the previous holder (`0x74`, first), a first claim the holder (`0x72`) |
| `00475D10` (`+2C`) | its draw: the H6 style `0x26` + holder (`0x0E` when unheld) published as the tint (`0043A370`) around `0047AFC0`, then the saved tint |
| `00475EB0` (`+14`) | its model value: the charge |
| 38: `00475B00`, `00475B50` | the host's list of holders (count, then a byte each); a list as long as this machine's sets them, and a territory the host granted (`+8D`) answers with 39 |
| 39: `00475BB0`, `00475BD0` | a guest's claim `[number, +8C]`; on the host it claims the territory for that slot, keeps it at `+8D` and sends 40 |
| 40: `00475C40`, `00475C60` | the grant `[number, +8D]`; the claimer's `+8C` clears |
| 41: `00475DB0`, `00475DF0` | `[holder, previous, scorer, amount]`: the line on each machine |

[`selector46`](../../crates/inf-game/src/object_runtime/selector46.rs)
builds it.

### The arenas' other classes

| Selector | Table | Routine | Does |
| --- | --- | --- | --- |
| 91, the Powerball ball | `0051F1E0` (`004027E0` over `00405780`) | `00403300` (`+24`) | by state `+148`: free (0) or loose (1, reset after about 31 s) it runs down `+17E` and moves (`004057D0`); held (2, 3) it follows its holder `+14C`; when the terrain height at its cell reaches `+170` it resets (`00402A40(5)`) |
| | | `004028D0` | while `+17E` is 0, a vehicle (flags `0x924`) within 500 in front of the ball and within reach takes it, keeping the ball's bearing in the vehicle's frame (`+15C/+15E`, angles through `0048F540`) (`00402A40(2, vehicle)`) |
| | | `00403660` (`+A4`) | a contact: a free ball is taken the same way (its bearing from the contact); a held one is thrown (`00403940`) |
| | | `00402CE0`, `00403750` | while held, the holder's velocity carries it (`0047B510`) and it sits at the holder's reach, turned by its angles (state 2) or ahead along its basis (3) |
| | | `00403D30` (`+2C`), `00402DD0` (`+3C`) | its draw position (`00403750`, then `0047AFC0`), and its messages naming its holder (H0 `0x7F..0x81`, with a sound `00487F10`) |
| | | `00403630` (`+A0`) | whether an actor holds it |
| | | `00403AE0`, `00403BC0` (`+C0`, `+C4`) | its sync: state, holder and angles out and in (`0052E528`) |
| | | `00402880`, `004028A0` | its destructor: it clears the ball pointer `005468D0` (messages 43 and 44 test it) |
| 92, the goal | `0051F2B0` (`00403EC0`) | `00403F40` (`+24`) | on the host: the ball within 4,000 crossing the goal's mouth (radius `+28`) sets `+74` or `+75` by the side it came from, and with both set the goal scores ([Powerball](#powerball)); elsewhere `00479F20(0)` |
| | | `00403F10` | its payload: `00479D70`, and its side (1 or 2) at `+6C` |
| 94, the missile launcher | `00524E20` (`004458E0`) | `00445930` (`+24`) | every 1.5 s a missile (`00458A50`) at the first player's unit unless the last of its zones holding it is of another kind than 1 ([built](CONSTRUCTOR_ASSETS.md#placed-selectors-the-port-does-not-construct)) |

`0046D050`, the ring's lines (the territory's, H0 `0x72` to `0x76`), shows
one when it names this machine's slot. `0046E0F0`, the `+48` of selectors
14 and 89 (`00527208`, `00527254`), returns `00547590` for kinds
`0x0E..0x10` at `+44`, else `0054ED20`.

### The race

Jet Race 1 and 2 (worlds 28 and 29) and Buggy Race (28) are the races
(mode 3), three rotating visuals (`0047A490`):

| Selector | Table | Routine | Does |
| --- | --- | --- | --- |
| 70, the race | `00527390` (`0046E770`) | `0046E830` (the payload, after `0047A610`; no flags) | the laps to run `+338` (payload `+4`), each record's grid place `+00` its slot less one, and sixteen grid places `+2D6` from the payload's `+C` table (`0046F300`: 220 times each WORD pair at `[first, 0, second]`, turned by the race, `0047A6C0`, and placed at it); the first race built is `0054ED70` |
| | | `0046E880` (`+24`) | the race's clock `0054ED74` gathers the time and `+6C` the 50 ms steps. A guest only counts the start `+2D4` down (256 µs units). On the host the idle start (`0xFFFF`) clears the records but their grid places and vehicles; counting down, under `0x186` every other slot's record races (`+20` bit 0) from gate 1 with no laps, and at 0 this machine's. Each racing record's vehicle (`0047AD90`; gone, it stops racing) gains the steps on its lap and race times and meets each gate of the gate list: inside its next gate (local Z between 0 and the depth, local X under the half width) it passes it, and gate 1 with the laps run finishes it (it stops racing, 17 less the finishers before it added to its laps); its next is that gate's next, and through gate 1 a lap counts (the best lap kept from the second). Outside its next gate the distance keeps the nearest; inside another gate it is through the wrong gate (`+20` bit 1) unless that gate leads where it should. Each record's key is laps · 2^24 + gate · 2^16 − distance + 0xFFFF (record 0's −0x7FFFFFFF), sorted by `00512521` (MSVC's `qsort`) with `0046F2D0`; while the race runs each record sorted takes its place `+01`, each slot's score is 17 less its record's place, and each record's grid place 16 less the old place of the record sorted to its slot (0 to 15). Then this machine's slot: not racing, its live vehicle (`0046B030`) sits on its grid place swaying (three angles `+340..+348` the RNG drives), turned as the race, its velocity cancelled, healed by 5,000. Racing, through the wrong gate for 1.75 s (its player's `+198`, `0046D210`) it is put back 500 above its next gate, and asked (`+34C`) 250 above its last gate (or at the race); turned as that gate, moving at 128 along its heading (`0047DDF0`) |
| | | `0046F3F0`, `0046F390` | the host's restart key (F10, `+42`): a running race stops racing; at most once a second (`+354`) an idle race counts 3 s down (`0x2DC6`), and a race no one runs goes idle |
| | | `0046F760` | a live vehicle's record (from its update, `00417D67`, and its `+3C`, `0042129C`): below lifecycle 3 it stops racing; else the vehicle claims its slot's record, unless the race runs and another vehicle holds it |
| | | `0046E810` | its destructor: it clears `0054ED70` |
| 71 | `005273DC` (`0046F7F0`) | `0046F890`, `0046F810` | its half extents along X (`+70`: payload `+4` halved, or code 3) and Z (`+6C`: `+6`, or code 4), from `0x80` and `0x200` |
| | | `0046F8C0` (`+24`) | the first player's live vehicle inside it, facing against it, shows its player "Wrong Way" for a second (`+19C`, `0046D200`) |
| 72, a gate | `00527428` (`0046F9E0`) | `0046FB20`, `0046FA70` | in the gate list `0054ED58` (newest first; `0046FA40` unlinks it): its number `+6C` (code 3), the next `+70` (4), its half width `+74` (payload `+4` halved, or code 5; also its radius `+28`) and depth `+78` (payload `+6`, or code 6) |
| | | `0046FB50` (`+2C`) | after the visual's draw, four pairs of posts scrolling across it with `0054ED74` (`t = (+74 · ring / 4 + 0054ED74 / 2048) mod +74`, at `t` and `−t` along its X row, 100 above it), each a quad down to 50 under the ground, at most `0x96` wide, of H3 sprite 6 seen from in front, else 1, in mode `0053A7F0` (4) |

The vehicle in a race: its weapons wait for its slot's record to race, the
primary through the race's first 5 seconds (`004171E4`), the secondary
through the first 5 of each minute (`00417382`); damage leaves it one
health (`00421591`); the ground drive loses acceleration by its place ahead
of ninth and the catch-up scale (settings `+4C`, `00419CB8`); the
skimmer's boost speed falls the same way (`0042243D`), and its drive's
budget eases its share `+590` (held at `0x300000` while the race waits)
toward its place, scaled by it and by its health (`004225BB`). Online every
vehicle heals from `+5B0`, a point each 2^12 µs (2^14 in team play,
`00417CF4`). A mode request (`+34`) asks the race to put this machine's
racer back (`+34C`).

Message 42 (`0046F410`, `0046F5E0`) carries records 1 to 16: their grid
places and places as nibbles, their flags' low two bits, the start, a mask
of the records with a race time, and for each of those its three times,
its laps and, through the wrong gate, its next gate. The encoder wants four
bytes of room where it writes the laps' and the next gate's one.

[`race`](../../crates/inf-game/src/object_runtime/race.rs) ports them;
`check_net_race.py` runs the sort, both ends of
message 42, the restart key and the claim natively. The grid's sway
starts from the heap, which the port takes as zero
(`RaceSwayResidueApproximation`, logged once).

The gates' posts hold two logged stops, each leaving that gate undrawn for
the frame:

- A first post whose top is nearer than 64 projects no fog, and keeps the
  stack record's earlier byte, which the port cannot know.
- A post whose top is at or behind the view gets a quad with reciprocal
  depths of both signs. Retail hands that quad to the driver as it is,
  and only the driver decides what it draws. The port's raster refuses
  such a face, and the refusal stopped the whole frame's draw queue
  before the gate held it.

A network game's boost pickup (0x77, `0046B9C8`) gives a vehicle with a
clear `+5AC` two seconds of full power and steer (`004231C0`: `+5AC` and
`+224`, which the drive counts down), and sound `0x19` at the vehicle;
while a boost runs, or offline, nothing is needed. The port hands the
sound back with the pickup's outcome, and its caller plays it before the
rest.

### Network menus

| Routine | Does |
| --- | --- |
| `0045FB50` | adds a session to the join list (eight entries of `0x44` bytes: a name and an id at `+9C`) |
| `0045FDE0` | refreshes that list each second (`004940A0`) and rebuilds the rows |
| `0045F700` | a list row's text, or "No more" |
| `0045FF80` | waits for the slot (`0055F2F4`), then for the host's game state, before starting (`004391B0(9)`, `00438380`) |
| `0045F280` | a settings row's text: a number, divided by 1,000 in mode 4 |
| `0045F480` | steps a limit by 500 (or the mode's step), from 1,000 to its maximum |
| `0045F440` | a settings row's command: mode 4's through `0045F320`, others `00453B80` |
| `0045F670` | the rows a mode shows (modes 0 to 2, 3 with `+28` set, and 4) |
| `0045FBE0`, `0045FC50`, `0045FCB0`, `0045FF20` | the menus' destructors (tables `00526208`, `005261E0`, `0052621C`, `005261F4`): they close a session they opened (`00493650`, `00494130`) before the menu base `00452420` |
| `0045F9A0`, `0045FBC0`, `0045FD20` | their scalar deleting forms |

The flow, from the root's Network row (`0045FD40`):

1. `0045F9C0(false)` (vtable `0052621C`) lists the connections `00493F90`
   enumerated, under the header (`005386F8`: `0045F520`, H0 `0x32`
   "Network Options", camera 4). Each row (`00538718`) draws its name
   (`0045F700`) and runs `0045F740`, which initializes the connection
   (`00494160`), clears `+281` and replaces the menu with the sessions.
2. `0045F9C0(true)` (vtable `00526208`) lists the header, "Host session on
   this machine" (`005386B8`, H0 `0x21`, argument -1), then a row for each
   session (`005386D8`; at most eight, `0045FB50`). Its tick `0045FDE0`
   searches again once a second (`004940A0`) and rebuilds the rows when
   their count changed. The second layout record of `005384B0`, which the
   host menus share, takes the count (`005384C1`) and shows at most four
   rows (`005384C2`).
3. The host row restores that record to nine rows under ten visible, and
   replaces the list with `00538650` (vtable `005261E0`, `+5C = 1`): Arena
   name (`0045EE20`/`0045F560`, settings `+1C`; ten arenas, eleven with the
   cheat word's bit 7), Respawning power-ups (`+20`), Show Player Names
   (`+2C`), Landscape Damaging (`+34`), Recharging grenades (`+28`),
   Catchup Scale (`+4C`), Winning Score (`0045F280`/`0045F480`, `+30`, by
   500 or by 1000 in modes 3 and 4, from 1000 to 100,000), Start Session
   (`0045F320`) and Back. `0045F670` shows grenades and the score for modes 0
   to 2, catchup for 3 (setting grenades), the score for 4. Changing the
   arena replaces the menu with `00538668`, the same rows (`00538510`) under
   other layouts. Activate on any setting row, and every command on Start
   Session (its `0045F320` takes none), host: `00494230`, then settings
   `+10` takes the arena's mode, `0054694C` is set, `0045CB70` resets the
   game's state, and once the heartbeat gives the host its slot the session
   asks for phase 9.
4. A session row joins (`00494190`), sets `0054694C` and replaces the list
   with the wait `005386A0` ("Waiting for session data from host", vtable
   `005261F4`). Its tick `0045FF80` receives (`0045C940`, `00492830`) until
   the player table gives a slot, resets the game's state, then until a
   host frame sets `0054E814`, and asks for phase 9.

The menus' destructors close the session when a menu is dropped while its
flag is set (`+281` on the lists, `+5C` on the host menus, the wait's state
below 2); moving forward clears it first. In phase 9, the name entry
completes at once under the network flag (`0045AD30`: "Jameson" for a blank
name, no profile written; its read `00463550` numbers absent initials by the
slot the transport gave, the host's `1up`, which the scoreboard shows) and asks
for phase 3, where `00487410` takes the
world: the host's arena's (`00488A20`), or the host's world word
`0054E814`.

The arenas (`0053C570`, eleven of `0x18` bytes): Powerball and Buggyball
(world 25, mode 4), Capture The Banner and Aerial Banner (33, mode 0),
Fragtastic (32, mode 0), Buggy Madness (27, mode 2), Buggy Platforms (31,
mode 2), Four Play Heaven (26, mode 0), Jet Race 1 (28) and 2 (29, mode 3),
and Buggy Race (28, mode 3), which only the cheat word shows. Each has a
preview sprite (`+14`) that `0045EE20` draws in a ring.

[`network_menu`](../../crates/inf-game/src/network_menu.rs) ports the
lists, the host menus' commands and draws, and the wait. DirectPlay's TCP/IP
dialog, which the first search opened for the host's address, becomes a
prompt in the menu's own style: it takes typed keys (letters, digits, `.`,
`-`, and Shift+`;` for `:`), blank searches the local network.

The arena row (`0045EE20`) draws the previews before its label. With the
row's age above 0, or the menu's first layout (`+48`) without flag 2, it
opens the clip to the whole surface (renderer `+24`, not restored) and draws
each arena's sprite, from the draw's own copy of the table at `005383A8`
(the same records, `+14`), as a direct textured quad (`+9C`, packet word 4,
depth 0x40) on an arc about (W/2, 55% of H). Arena `i` sits at angle
`(0x50000 + 0x10000 i) / 40`, 45% of the surface out for even `i` and 35%
for odd, and spans W/20 each way; the selected one spans W/16, pulsing by
W/80 with the animation clock (`005471C0 << 11`), and is drawn twice. The
eleventh, Buggy Race, needs the cheat word's bit 7. The port draws them
([`arena_previews`](../../crates/inf-game/src/network_menu.rs)); its
corners match ones worked out from retail's arithmetic and sine table
(`the_arena_previews_follow_0045ee20`), and at 1280x1024 the Powerball,
Buggyball and Capture the Banner previews land where a retail
screenshot of 2026-10-21 shows them. Before, the port drew the label alone.

### The network game in the session

The link keeps the game protocol's state beside the transport, as retail's
globals outlive a game. Start Session and the wait's slot run `0045CB70`
over the settings' mode; the wait's receive runs `0045C940` and the drain
over the frontend, whose mission `+12` is 0 there (its constructor and the
leave's `0048E890` clear it), so a frame never passes its header: only the
states, the host's settings and its world word arrive. The frontend's
resident levels hold no terrain, so a changed detail is refused there
(`0045C506`). In a world H12's first slot is resident, so a host's changed
detail reselects the spacing and prepares the terrain (`004502C0`), as
Alt+F3 does, and the guest's settings `+70` take it. The line groups load
with `0045CB70` at Start Session or the wait's slot, as in retail
([`LineGroupSource`](../../crates/inf-game/src/network/game_tick.rs): the
port's data folder and its H0 defaults); before 2026-10-21 they never
loaded, so a death notice in a network game stopped at an empty group.

The session mirrors the process words the transport sets each poll and
before a transition: the network flag `0055F448`, the slot `0055F2F4` (1
offline, which new actors' handles carry as their owner) and the host flag
`0055F3A0`, with settings `+10`, the game's mode. Phase 9 then completes the
name entry, and `00487485` takes the network world: the host's arena's
(`00488A20` over settings `+1C`) or a guest's `0054E814`. The loader runs
its network branches: no `0047F6C0` counting, constructions with the network
flags (`+40`, `0047E2E0`, which disposes a guest's placed objects: the
host's copies arrive instead), and the player's branch: modes 0 and 4 hand
the player the camera (`0046AE90`), the rest respawn.

[`session_platform::network_game`](../../crates/inf-game/src/session_platform/network_game.rs)
is `004875A0`'s network branch after the particles: the header's status (this
slot's state, settings `+14` or `0x1F` without a unit; the arena, catchup,
mode with the host's team bit, detail and the five flags), republished with
`0045C320` when it or the world changes (or `0055F2F8` asks), `0045C940`,
`0045C5A0` with the tick's delta, the drain, a migrated host or a lost
session back to the frontend (`004391B0(2)`), and the recharging grenades:
every 4 seconds of `G+0` (2 s from `00487BE0`), item `0x1A` outside races,
`0x1D` with upgrade `0xA3` and `0x1C` with `0xA0`. Its world host resolves
handles, disposes, changes states (`0047D670`), takes the world values (`W+B0`
over the span an arena's index reaches), and encodes and applies message 1.
An expired remote actor's `+C8` (`0047ECF0`) moves its deadline on and asks
again with message 7.

`0046A260`'s network keys: `0054ED18` counts down unsigned, holding the
restart key `+42` off; a race (mode 3) clears a mode request `+34` and tells
the race's actor; Powerball (mode 4) clears `+28`, `+2C` and `+3F`, and the
team key `+3D` flips the host's team flag `0054E81C`; the host's restart
resets the race, the ball or the arena; the lockout `+19C` counts down and
`+42` ends cleared.

The arena's restart (`0046A587`, any mode but 3 and 4) clears every score,
the game's end `0054E829` and each team task's words `+0C..+14`. Then, over
the actor list, each banner (`+B8`) away from home (`+138`) is disposed
(`00479FA0`) and built again from its placement `+2A` (`0048E170` with the
process NULL, so in mode 0). The banners built come home, so the walk never
reaches one again. The port's player update disposes the banners and hands
their placements to the session, which builds them before the vehicle's
update, as `0046A260` does. The shared
[`placement_request`](../../crates/inf-game/src/session_platform/world_actor_factory/child.rs)
is also the one a remote actor's record builds from.

A placed pickup (selector 14) on the network host goes on the update list
(`0046D7D0`'s `00479F20(1)`), where `0046D950` runs: once the mission
records its serial as collected (`0048E930`), its clock `+50` becomes the
animation clock plus 30 (200 for any kind but `0x77`), it flashes
(`00489A90(3)`) and sounds twice (`00487EC0`, sound 2, the kind's pitch),
and without respawning power-ups (settings `+20`) it goes; when the clock
passes `+50`, the bit clears (`0048E9C0`), it flashes and sounds again and
`+50` returns to 0. Its words are an arena world's, past the 25 worlds'
pickup words (`W+B0..W+3CF`).

#### The player's vehicle in a network game

The respawn `0046A890` (`0046A9F3..0046AB45`) searches the spawn markers
(list `0054E2F8`) of the player's team (settings `+14` plus 2) or anyone's
(1): each draws a score (`0048F060`), and while that beats the best, every
player's vehicle (flag `0x10` on the actor list `G+48`) nearer than 1024
adds `0x100000` less its squared distance. The lowest wins, and the vehicle
spawns at its position plus its offset `+78`. Without one, a respawn
(`+EE` set) takes the camera's place and its heading (`00491E80`'s yaw
field shifted by 6). After the factory the vehicle takes the marker's whole
basis turned a quarter (`00491530(0x4000)`), its owner `+B4` is the slot
plus 1 in modes 2 and 3 or the team plus 2, and Powerball pushes it
(`0047B540` with `(0, 200, 500)`). The grants then end with the network's
loadout `004698F0` instead of `(0x32, 0)`: the player's rows, counters and
upgrades clear (rows 0 and 6 unlimited) and each mode grants its kit (mode
0 by the vehicle `+18`; mode 3 its grenades only with recharging grenades,
and `+28 = 6`), then `+142` takes settings `+48` and the unit its owner.
`0046B2B0` in a network game selects no weapon on a first pickup and says
nothing, and `004750C0` announces nothing; kind `0x77` with a clear `+5AC`
reaches `004231C0`, not ported.

The vehicle's constructor `004156E0` takes its form `+1F0` from the
settings: the player's vehicle `+18` plus one in mode 0, the arena's
(`00488A40` over `+1C`) in modes 2 to 4. Its guard `0041A620` scales the
maximum `+A2` before health takes it: in mode 0 the truck has half again,
the VTOL half and the skimmer a quarter; modes 2 and 4 give sixteen times,
kept in a WORD (the buggy's 5000 becomes 14464).

Its draw (`00420510`) blends the tint halfway to its owner's colour (the H6
style `+B4 + 0x25`, `0042088F..0042092D`) before the terrain's shade. With
Show Player Names (settings `+2C`), a vehicle that moved in the last two
seconds (`+5A0`) carries a name tag (`004205B2..004207C6`): its point from
the eye, 10 up and wrapped to the world's coordinate width, gives a depth
along the view and, past 64, a screen point (`00420AD0`, halved until it
fits a WORD); the owner slot's initials (`0054E71C`, "%5.5s",
right-aligned in five, written into the vehicle's `+E8`) go in a sorted
overlay item (`004A42B0` with `00423370`) keyed by the depth less `0x100`,
or by sprite `0x32`'s texture when the renderer sorts, and the renderer's
`+0C` (`004A3AB0`) draws them in font 0 in the owner's colour. The port
queues the text's glyphs at that key. In Powerball the draw swaps the
vehicle's model ([Powerball](#powerball)).

#### Remote actors

Message 1 for a handle this machine lacks builds the actor (`0047E600`):
below `0x4000` the type is a placement of the current world (`0048E170`:
its resource at its position raised to the ground, its angles and its
commands, mode 2 unless the handle is the process NULL; past the count it
builds nothing), else the resource less `0x4000` at no position
(`00460310(0, 0, -1, 2, handle)`, the position the fresh heap's). Mode 2
first asks `00462480` whether the handle's slot is free, and `004600D0`'s
`0047E2B0` gives the new actor that handle before its `+0C`, whose
`0047AC10` sets `+2C` bit 31 for a foreign owner. A refusal names the
handle in message 7; the record itself is skipped. The camera's `+0C`
(`00479DB0`) does not run `0047AC10`, so a remote camera stays local, and
each record for it logs "Strange error" as retail wrote to its file; its
`+40` (`00410F50`) adds dirty bit `0x8000`.

An actor whose bit 31 is set runs `+3C` in `00478F20`. The shared
`0047EBF0` moves it on its velocity and spin (`0047B1E0(delta, 0x18)`)
unless `+2C` bit 3 holds it, runs the status visual with dirty bit `0x400`,
and counts `+C8` down by the whole delta. The vehicle's `00420F40` first
checks its hold (`00422300`) and the moving timer `+5A0`, takes its engine
from the record's bits `+58C` (idle at 2000 without bits 0 and 1, else its
speed, 300 more with bit 1; bit 2 the throttle, bit 3 forward) into
`+22C..+236` as the local update does but with the velocity scaled by
`-(delta >> 6)`, then `0047EBF0`, the hold's carry (`004125C0`,
[below](#holds-between-machines)), the skimmer's wake ribbon (a new one,
`00486600`, from a quarter and an eighth of the velocity behind it, then
where it is, `004869B0`; in another mode let go, `00486B20`) and the
hovercraft's spray. A vehicle built from a record reads `+58C` before the
next sync writes it: the port takes the fresh heap's zero
(`RemoteVehicleBitsApproximation`, logged once). `004875A0` builds it
(`00492830`) after the actor walk (`00478F20`), so its first draw comes
before its first `00420F40`: a wheel's vertex (`00415D30`) reads the
intensity `+228` the constructor leaves to the allocator, where the port
takes the fresh heap's zero (`RemoteVehicleIntensityApproximation`, logged
once; no dust that frame). Only the ground drive writes `+58C`
(`0041A086`), so a vehicle that has been only a skimmer, VTOL or
hovercraft sends residue: the port sends zero
(`LocalVehicleBitsApproximation`, logged once). A short vehicle sync
(under ten bytes) leaves the vehicle alone; a full one writes every field
but the hold's slots. A turret (selector 11, `00402630`) first hurts the
first player's vehicle within 500 as its local update does (`00401920`),
then runs `0047EBF0`: Capture the Banner's 28 turrets are the host's. A
gate (selector 30, `0048C4E0`), a banner and a base (`00461C50`) run their
own `+24`; Four Play Heaven has eight gates, four banners and four bases.
A guest builds a remote gate with the world's starts at hand, as its
construction reads them. A grenade, a charge or a mine another machine
threw (`004559B0`) smokes as its local update does (`004558B0`), sets
`+138` and runs `0047EBF0`; its owner fuses and ends it. A missile
(`00457660`, selectors 25, 28 and 68) runs `0047EBF0`, then lays its
exhaust ribbon as its local update does (a new one, `00486600`, from a
quarter of the velocity behind in eighths, then where it is), and a Bomb
runs `0047EBF0` alone. A factory (`00431A90`) docks the first player's unit
on its pad (`0042EC60`, `0042F490`) with no cooldown, pull or production,
and a selector-10 or 21 actor (`00425CD0`) runs only its helper `+26C`'s
`+4`; no network world places either. The owner's end messages (3 to 6)
take another machine's copy through `0047D670`: a missile's `+AC`
(`00456B10`, the implant's `00458E40`) runs its state 0 (`00455680`) in the
missiles' lifecycle, as a local one's does (before 2026-10-21 that stopped
the network pass, dropping its later messages).

World code's own sends (`00492500`: a guest's pickup collection, message
37; the end of a local actor in `0047EC50`, messages 6, 5, 4 or 3 by its
state; damage to another machine's actor, message 18 with the packet's
DWORD, the target and the source) queue on the arena and go out first in
the next network pass.

#### Effects between machines

World code tells the other machines what it shows, each through
`00492500` on channel 1, and their receivers show it again without telling
anyone further:

| Message | Sent by | Received by |
| --- | --- | --- |
| 19 | `004887D0` when a blast reached another machine's actors and may repeat: the source, its position and the 10-byte block (`0045DFF0`) | `0045E050`: `004887D0` there, not repeated |
| 20 | Sonic Shock's pulse `0048D980` from this machine's owner (`0048D9E0`) | `0045EDE0`: the pulse on that owner there |
| 23, 24 | each particle a factory makes without flag 1 (`00466030`): 23 without a source, 24 with it; its position, its velocity (the position itself, or its own six bytes; a null velocity reads through) and its factory and selector (`0045E960`, `0045EA60`) | `0045EA20`: factory `004665D0` (0) or `00466710` (1) with flag 1 |
| 25 | the flamethrower `00431EF0` of this machine's owner, after the flame is built and before it is admitted (`00431F5F`) | `0045EE00`: the flamethrower on that owner there |
| 31 | the death lines ([below](#hits-and-deaths)) | `0045D1B0`: the first player's message |
| 32 to 35 | a sound (`0045DB10..0045DBC0`) | `0045DB20`: `00487EC0(id, &position, volume, pitch, 0)`, the id's low two bits telling whether each value came |
| 7 | a refused build, and `0045DEB0` | `0045DEB0`: its owner names the handle again (7, or 2); a machine that never built it frees the slot (`00462480`, `00462510`, `00462560`) |

The port keeps the world's sends on the arena and the particles' on their
pool, each stamped in the order made, and sends them together in that
order. A shot another machine fired runs its `+3C`: the cannon shot's
sparks (`00486100`, one draw in three) or the laser's turn (`004854C0`);
its owner ends it. A shot built from a record can throw before a record's
sync writes its aim `+138`: the port takes the fresh heap's zero
(`RemoteShotAimApproximation`, logged once).

#### Hits and deaths

The protocol's state (`GameNetState` and the frame's) is the world's while
it updates: the session moves it from the link to the arena before the
furniture's effects and back before the network branch, and a received
frame's world routine borrows it the same way. A live local vehicle in a
network game (`004212F0`, `0042134D..004213C2`) keeps its last marked
attacker `+5A4` and the clock `+5A8` of the hit; an unmarked hit within
200 ticks moves the clock on (before any marked hit the clock is the
allocator's: `VehicleAttackerApproximation`, the fresh heap's zero). A
player's vehicle killed by nothing marked (deep water, a fall) credits
that attacker: its `+64` answers itself, and within 200 ticks of `+5A8`
the death names `+5A6`, the attacker handle's high WORD (`0047CE52`,
`0047CE62`). In
Buggy Madness and Buggy Platforms (mode 2) vehicles then take no damage,
nor do Powerball's (mode 4), where a hard blow knocks the ball loose
([Powerball](#powerball)). A hit on a player's vehicle (`0047CBC0`)
outside mode 2 tells `0045DA50` the packet `[kind, victim, attacker]`: the
attacker (`0054E830`) is kept for 0.75 s (`0054E82C`), the host moves the
score (`0045D4C0`), a guest sends it (27). A death tells `0045D8F0`:
unless the game is over, a draw (`0048F060`) picks a line, an unknown
killer is the attacker still kept, the kill word `0054E804` counts the
death and names the killer, the line is formatted from the self-kill,
kill or death group (`0045D7C0`), the host records the kill (`0045D350`)
and a guest sends its kill word (26), and outside Powerball the line goes
to every machine (31) and the first player's message. The vehicle a death
names is the one whose last attacker hit within 200 ticks. A
departed player's objects disposed in `0045C940` are drained at once
(`004878D2`, `00479840`) before the frame's sends. Leaving a network world
closes the session (`0048721E`, `00493650`): the network flag clears and
the slot returns to 1. New actors' handles take the session's slot, the
loader's and the world factory's alike. Settings `+34` (Landscape
Damaging) gates a crash's damage (`0047D2F0`, `0047BFD0`), and the mode
`+10` scales a radius impulse (`00463AB0`: 800 in mode 2, 64 in mode 3).

A placed pickup's draw (`0046DAF0`) in a network game first settles a
collection in flight (`+54`): the host's bit ends it (`+50 = 1`, and the
pickup goes without respawning power-ups), `+50` passing clears the bit,
else it asks again with message 37. A guest hides a collected pickup and
flashes one that came back; the host hides one respawning. A collection
flashes, sounds and waits 200 ticks (30 for kind `0x77`); the host marks
it, a guest asks the host and waits.

#### Teams and vehicles

In modes 0 and 4 a player spectates from the camera until it picks a team.
The gameplay screen's T (`004435F0`, its `+5C` `00442CF0`) shows the team
menu: with the network flag, the screen's wait `+F0` passed (the animation
clock), the mode 0 or 4 and a first player, it sets the row count of
`005350D0` to the arena's teams (`00488A80`, arena `+0C`) plus the header,
and its second layout's counts (`005350B1`, `005350B2`) to the teams,
closes the player's newest menu (`004133E0`), shows the new one
(`004137D0(0, menu)`, header H0 `0x39` "Team choice", rows `0x3A` on) and
waits 250 ticks. Y (`00443620`, `+60` `00442EE0`, wait `+EC`) shows the
vehicle menu `005351D8` the same way in mode 0 when the arena has more than
one vehicle (`00488A40`, arena `+08`), without closing a menu first. The
screen's constructor `004408B0` zeroes both waits.

A team row (`00442C60`) sets settings `+14`. Unless the first player's unit
(`0046AE60`) already plays for the team (`+B4` = team + 2), it retires the
unit (`0047CE90`; the spectator's camera, whose `+68` and `+AC` are the
shared `00401300` and `00404600`, is disposed) and respawns the player
(`0046A890(1)`) at one of the team's markers; then it closes the menu, and
when nothing respawned, zeroes the wait. The vehicle's next mode switch
(`004168F9`) builds a new world camera (`00478E00`), as the skimmer's
switch `00422380` does. A vehicle row (`00442DF0`) sets `+18` for the next
respawn and tells the player "%s %s" (H0 `0x78`, then the vehicle's name
from `0x54`), then closes the menu and zeroes `+EC`. While a menu shows, the
camera's null `+64` view gives the player no brake (`0046A731`).
[`network_menus`](../../crates/inf-game/src/session_platform/network_menus.rs)
ports them.

Held, each a logged stop: other classes' remote updates (`+3C`), and a
remote grenade's, charge's or mine's state 0 (`004564B0`, from message 6),
whose burst visual and debris the receiver's host cannot yet build (the
network ACAM has grenades only with upgrades `0xA3`/`0xA0`).

#### Holds between machines

A hold (a vehicle's `+1A4`, the Prison Unit's `+144`) can take another
machine's actor. The take (`004122C0`) captures it into a slot, state 1 for
a remote actor, the capture's flags in the slot's `+8`. While no record
names its holder (`+70` the process NULL), each update (`00412380`)
captures it again (`00412468`) and message 8 tells its owner ([vehicle,
actor]); once a record names the vehicle the slot carries it (state 2). A
remote actor's drop (`00412880`) marks its slot 3: while it still names the
vehicle, message 9 tells its owner (`004123F4`) and it rides on, until a
record names no holder and the slot empties.

The owner receives message 8 (`0045EB30`): a local actor not yet held is
captured by the vehicle (`+8C(1, vehicle)` into a stack slot), and the
vehicle's hold flashes at it (`004122A0`, the hold's `+C`). Only two
classes have a hold (`+BC`): the vehicle (`0041ED30`) and the Prison Unit
(`0046E6E0`); every other table answers none (`00401300`). Message 9
(`0045EBA0`) sets the actor down before its holder (`004121D0`, the hold's
`+4`), or on the ground without one (`00479DD0`), and a local actor lets go
(`+8C(0, NULL, +D4)`). A record whose held actor's holder has gone grounds
it and lets it go (`0047E35B`). On another machine a held actor rides with
its vehicle (`004125C0` from `00420F40`) and, one this machine owns, runs
its `+90` with the vehicle.

#### Powerball

Powerball (world 25, mode 4) plays a ball (selector 91) into two goals
(selector 92), team 2 against team 3. The ball's goal bytes
`0052E528`/`0052E529` count each side's goals, bit 7 a side that has won;
`0052E52C`/`0052E52D` keep the counts a guest last announced.

- **The ball.** Its constructor `004027E0` (case `0x5B`, over the Boulders'
  `00405780`) sets its handles to the process NULL, makes it the ball
  `005468D0` when none is, and makes every goal byte idle (`0x80`). Its
  payload `004038E0` runs the Boulders' `00405DE0`, wakes it and keeps its
  home `+160`. Its commands and `+1C` are the Boulders'. In a network game
  its `+40` `00403CF0` runs the Boulders' `00413120`, then clears dirty bit
  `0x10`, sets `0x280`, clears `+D8` and scores it (`00402A40(4)`): with
  the goal bytes idle it waits at home, 500 up, for 500 ms. Its destructor
  `004028A0` clears `005468D0` when it names this ball's handle.
- **Its states** (`00402A40`, a local ball only): 0, free, after the
  kick-off's sound (`0x33` with both counts 0, else `0x36` or `0x37` for
  the side behind) and the strip's highest ground `+170` (64 cells of
  `0x400` in X); 1, loose, unlocked after 350 ms (`+17E`); 2 and 3, held by
  `+14C` (2 turned by the angles it was taken at, 3 ahead along the
  holder's basis); 4, scored: home and 500 up, waiting 500 ms while a side
  has won and otherwise 5,000 ms and up to 2,047 more (a `0048F060` draw);
  5, received, back at home rising `0x5DC`. Each change moves the sequence
  `+174` on (never 0).
- **Its updates.** On the machine that owns it, `00403300` (`+24`) lets a
  vehicle within 500 and just ahead take it (`004028D0`), resets it on
  ground at its strip's height or about 31 s loose (30,000 steps of
  1,024 µs), rolls it as a boulder while
  free, drags its holder's velocity and sits at its reach while held,
  counts a scored ball's wait while neither side has won, and models it
  0x271 or 0x2BE by the host's team flag `0054E81C`. Elsewhere `00402DD0`
  (`+3C`) announces each goal count that rose by one or two (H0 `0x80`,
  `0x81` with sounds `0x34`, `0x35`; an own goal `0x7F`; a win `0x82` with
  the team name), rolls it slowed near the ground, settles its angles and
  repeats a throw or drop (messages 44, 43) until the host's sync moves on.
  Its light `00404450` (`+28`, flag `08000000` kept) is radius 1,000 at its
  X and Z: grey and red, or blue for team 2, while held, else three phases
  cycling. Its draw `00403D30` places a held ball first. Its `+A0`
  (`00403630`) passes over its holder in the contact walk, and its `+A4`
  (`00403660`) lets a vehicle take a free ball or knocks a held one loose.
  Its `+14` (`004045C0`) answers 0xFFFF to query 1 but while held ahead.
- **Its sync** `00403AE0`/`00403BC0` carries the state and, with the ball
  pointer naming it, the goal bytes, the sequence and the holder (with the
  angles' high bytes held turned) or the scorer; a received return to free
  sounds the kick-off.
- **The goal** `00403F40`, on the host, marks the ball's side of its plane
  within its radius (or where the ball's velocity carries it to the
  plane). With both sides marked while neither has won, the side's count
  (and the count last said) rises by its points `+70` and wins at the host
  settings' `+30` over 1,000; the ball is scored; its scorer gains the
  points as kills (`0054E798`) for its own side's goal or, for an own goal,
  has its deaths set to them; and the first player hears it, the sounds at
  the goal.
- **The restart** (`0046A644`, the host's key `+42`): with both goal bytes
  idle play starts (both 0); otherwise the ball is let go and scored, every
  player's score clears and both bytes go idle. The key then waits 500 ms
  (`0054ED18`).
- **The vehicles** take no damage (`004213C8..0042153F`). A blow over 500
  to the ball's holder (unless the ball dealt it) or from the holder
  knocks the ball loose (`00403940` in mode 1): upward 300, or for a
  collision with a live actor half the holder's velocity less the other's
  and, upward, as much again as their facings agree, times 750. Their draw
  (`00420551..004205A2`) shows a buggy or hovercraft as model `0x10D`, 50
  lower, while the host's team flag is set, any other as `0x75` (dirty bit
  `0x100`), and afterwards sets model `0x9C`, flag 4 and the height back
  (`00420A31..00420A5A`).

[`selector91`](../../crates/inf-game/src/object_runtime/actor/selector91.rs)
ports the ball, the goal's update, the restart and the knock; the goal is
a [race part](../../crates/inf-game/src/object_runtime/race.rs).
`check_powerball.py` runs the original routines
over 1,928 cases, the goal's update over 240 (22 of them scoring) and the
knock over 120, and the port matches each. A goal without a side (`+6C` 0,
which no world places) would add to the bytes before `0052E528`; the port
stops there.

#### The network game on screen

Online (`0055F448`) three draws show the game.

**The player's texts.** After the player's message, `0046BE00`
(`0046C016..0046C5F1`) ticks and draws the message lines (`0045C1D0`, above),
then the race's status, each an H0 text in H6 style `0x28` centred on its
point:

- While the race runs (`+2D4` zero) "Wrong Way" (`0x83`, or `0x84` spread
  out, by the animation clock's bits `0x30`) shows at 30/128 of the height
  while the player's `+19C` is set. "Check Point Missed" (`0x87`/`0x88`)
  shows at 40/128 while its `+198` is.
- An idle race (`0xFFFF`) blinks (bits `0x30`) "Press F10 to start" (`0x89`)
  for the host and "Waiting" (`0x85`) for a guest at the middle.
- A countdown blinks (bits `0x0C`) "Starting" (`0x86`) at the middle and
  twice more. Those two slide in from the top and the bottom as it runs
  out: the countdown times half the height, divided by its whole span of
  11,718 (`IMUL` by `0xA6840AA9` and `0x597BF557`, `SAR 12`).

Powerball's status (`0046C43C`) shows at the middle once a goal byte has
bit 7: while one side has won, the host blinks "%s team wins" (H0 `0x82`
with the winner's name, on odd fifty-tick spans) with "Press F10 to start"
(`0x89`), which is all it shows while both bytes are idle; a guest sees
"Waiting" (`0x85`).

**The vehicle HUD.** `0041C320` online draws:

- the terrain radar, with or without the upgrade (`0041E343`);
- a score line at 20/256 of the layout and 88/100 of the height
  (`0041E7A3..0041E9DB`) instead of the counters (`0041E44E`), for a
  vehicle on teams 2 to 17 (`+B4`): this slot's score (`0054E79C`, "%02d")
  in modes 2 and 4; in mode 0 the team's `+0C` and `+0E` ("%d/%d"); and in
  a race the laps "%d/%d" (the racer's `+18` while racing, else the race's
  `+338`) with the place (17 less the score) 4% up and left;
- the turbo gauge after the score line.

Mode 3 with no race object draws an unwritten text buffer, which the port
leaves out with a logged omission. Powerball also leaves out the sight, the
health gauge, the carried units, the secondary's count and the primary
(`0041C940`, `0041D065`, `0041D3FF`, `0041DD88`, `0041DEDA`). Its secondary
is the ball (model `0x271`) on the vehicle that holds it (`0041DCA6`: the
ball's `+14C` is the vehicle's handle), else model 0.

**The scoreboard.** After the world draw (`00440F00`), `00442080`'s network
section (`004421E6..00442B2E`) draws on the surface, in this order:

1. The game's end (`0054E829`), blinking on odd fifty-tick spans of the
   clock: in a team game "%s team wins" (H0 `0x82`) with the team's name
   (H0 `0x38` + `0054E824`) at a third of the height, then "Press F10 to
   start" for the host or "Waiting" a tenth lower. Each is centred, in H6
   style 0.
2. While Backspace is held (`004435C0` to the screen's `+58`, `00440940`,
   which sets byte `+F8` while the key is down; `004408B0` zeroes it): four
   quarters of sprite `0x2A6` about the middle, with a radius of 45/100 of
   the width.
3. A bar of sprite `0x230` along the top, 4/100 of the height deep. Every
   quad has vertex DWORDs `0x40` and packet word 2.
4. The boards. A race (mode 3) adds a second bar under the first (to 8/100)
   and a deeper compact row. Outside team games, the compact board comes
   first, at (8/100, 3/100); with Backspace the full board follows at
   (60/100, 20/100), or at half the width in a race. In a team game, with
   Backspace, a full board for each team from 2 to 19 runs from (50/100,
   14/100), then one for team 1 (still choosing), then the compact board.

The board `00441510` sorts the present slots by score, highest first, with
ties in slot order. A team board shows only its team's slots (`0054E380`).

- **Compact board.** Each slot's details (`0054E71C`) are centred
  30/256 of the width after the last one. An entry that would pass the
  width less "MMMM" starts a new row at the board's left, under a new bar
  1/100 of the height below the last row (4/100 lower in a race). In a race
  the total time ("%02d:%02d" of `+06`, 20 ticks a second) goes 4/100 under
  the details.
- **Full board.** Each slot gets "name (details)" right-aligned on x, then
  its stats from x, a row each:
  - a race: "- %02d:%02d:%02d   %d/%d", the best lap `+02` as minutes,
    seconds and hundredths, then the laps;
  - mode 2: " : %5d", the score;
  - otherwise: " : %5d : %02d-%02d", score, kills and deaths.
- **Team games.** A full board's first slot draws its team's line at half
  a row, then skips two rows. A team from 2 to 17 shows "%s : %02d-%02d :
  Score %5d" from its task's `+0C`, `+0E` and `+10`; any other shows
  "Still choosing team :" (H0 `0x8A`).
- **Colours.** In a team game each slot takes its team's H6 style
  (`0x25` + state), white outside teams 2 to 17. In Capture the Banner a
  slot carrying a banner (the actor list's `+B8`, carried `+139` by `+144`)
  takes that banner's team colour on every eighth even five-tick span.
  Otherwise each slot takes its own style (`0x26` + slot), and in a race
  this machine's row is 0x40 brighter on even five-tick spans.

In Powerball (`0044274C..004428CC`) each side's compact board runs from a
fifth of the width, team 2's at 3/100 of the height, then a bar from 3/100
above to 1/100 below the next row, then team 3's; with Backspace each team's
full board follows from (50/100, 20/100), then team 1's. A compact board
opens with its side's "%s : %02d :" (the goal count) right-aligned on x,
then moves on by "MMM"; a team's line is "%s : %02d"; a slot's stats are
" +%2d  -%2d" (goals and own goals); and the ball's holder's row is 0x40
brighter on even five-tick spans. A side past team 3 would count the bytes
past the goal bytes; the port stops there. Names and details run on through
the next slots' fields until a NUL, as sprintf reads them.
[`network_hud`](../../crates/inf-game/src/network_hud.rs) plans all
three. `check_net_overlay.py` runs the
original section and `00441510` over 102 cases, and the port matches each:

- deathmatches and races, with and without the race object;
- team games with banners carried;
- the game's end, ties and wraps;
- long names, surfaces and widths.

A two-machine probe draws the race's and the team game's boards.
