//! The transport's framing (`00492EC0`, `00492500`, `00493790`, `00492E50`),
//! its receive dispatch (`004926E0`, `00492630`), its buffers (`004923D0`)
//! and its two built-in protocols: the player table on channel 0
//! (`00540768`) and the sequence protocol channel 1 takes by default
//! (`00540780`).
//!
//! A frame is a slot of a channel's send ring: a WORD length, two bytes (the
//! channel, then the sender's slot `0055F2F4`), the protocol's header, then
//! entries. An entry is its message id (bit 7 set when a WORD payload length
//! follows, `0054073C`) and the payload its encoder wrote.

use thiserror::Error;

use super::ring::{MessageRing, RingError};

/// `0055F400..0055F440`: the session's id, then sixteen players.
pub const PLAYER_TABLE_ENTRIES: usize = 17;
/// `00492500`'s encoder buffer.
const ENCODE_ROOM: usize = 0x200;
/// `004923D0`'s receive ring: 64 slots of 0x400 bytes.
const RECEIVE_SLOTS: i32 = 0x40;
const RECEIVE_SLOT_SIZE: i32 = 0x400;
/// `0049341B`: each send ring holds four slots.
const SEND_SLOTS: i32 = 4;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TransportError {
    #[error(transparent)]
    Ring(#[from] RingError),
    #[error("message {id}'s encoder returned length {length}, outside its 0x200-byte buffer")]
    EncoderLength { id: u32, length: i32 },
    #[error("channel {channel}'s entry of {entry} bytes overruns a fresh {slot_size}-byte frame")]
    EntryOverflow {
        channel: usize,
        entry: usize,
        slot_size: usize,
    },
    #[error("a received player table of {count} entries overruns 0055F400's 17")]
    PlayerTableOverflow { count: i32 },
    #[error("a received frame reads past its {length} bytes at offset {offset}")]
    FrameOverrun { offset: usize, length: usize },
    /// A protocol's own failure, which it keeps whole for its caller.
    #[error("{0}")]
    Protocol(String),
}

/// One 16-byte row of a protocol's message table: receive `+0`, encode `+4`,
/// the id `+8` and the payload size `+C`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolRow {
    pub id: u32,
    pub size: u32,
}

/// The transport's globals the protocols read and write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportState {
    /// `0055F448`, set while a session runs.
    pub network_active: bool,
    /// `0055F3BC`: this machine hosts.
    pub host: bool,
    /// `0055F3B8`: this machine's DirectPlay id.
    pub session_id: u32,
    /// `0055F400..0055F440`.
    pub players: [u32; PLAYER_TABLE_ENTRIES],
    /// `0055F2F4`, from 1; `0055F2F8` marks it assigned.
    pub local_slot: u8,
    pub slot_assigned: u32,
    /// `0055F38C`: player tables received; `0055F3D0` and `0055F384`: players
    /// the host added.
    pub tables_received: u32,
    pub players_added: [u32; 2],
    /// `00540794`, the next sequence sent; `0055F380`, the last received.
    pub next_sequence: u32,
    pub last_sequence: u32,
    /// `00540734`, the frame size `004923D0` clamps to `0x200..0x400`.
    pub buffer_size: i32,
    /// `0054073C`: each entry carries its payload length.
    pub size_prefix: bool,
    /// `0055F390`, heartbeats; `0055F388`, player tables the host's
    /// heartbeat sent; `0055F4AC`, its countdown to the next.
    pub heartbeats: u32,
    pub tables_sent: u32,
    pub table_countdown: i32,
    /// `0055F3A4`, the host migrated here; `0055F3A8`, the session lost.
    pub host_changed: bool,
    pub session_lost: bool,
}

impl Default for TransportState {
    fn default() -> Self {
        Self {
            network_active: false,
            host: false,
            session_id: 0,
            players: [0; PLAYER_TABLE_ENTRIES],
            local_slot: 0,
            slot_assigned: 0,
            tables_received: 0,
            players_added: [0; 2],
            next_sequence: 0,
            last_sequence: 0,
            buffer_size: 0x200,
            size_prefix: true,
            heartbeats: 0,
            tables_sent: 0,
            table_countdown: 0,
            host_changed: false,
            session_lost: false,
        }
    }
}

/// A protocol object: header reader `+0`, writer `+4`, receive gate `+8`,
/// message count `+C` and table `+10`.
pub trait NetProtocol {
    fn receives(&self) -> bool;
    fn message_count(&self) -> u32;
    fn rows_mut(&mut self) -> &mut [ProtocolRow];
    /// `+4`: the header after the frame's four bytes, its length; None
    /// without a writer.
    fn write_header(&mut self, state: &mut TransportState, out: &mut [u8]) -> Option<usize>;
    /// `+0`: a received frame's header, its length and whether to go on;
    /// None without a reader.
    fn read_header(
        &mut self,
        state: &mut TransportState,
        header: &[u8],
    ) -> Result<Option<(usize, bool)>, TransportError>;
    /// Row `+4`: the payload into `out`, its length (0 sends nothing, -1 when
    /// it does not fit).
    fn encode(&mut self, state: &TransportState, id: u32, argument: u32, out: &mut [u8]) -> i32;
    /// Row `+0`: a received payload, the bytes it used.
    fn receive(
        &mut self,
        state: &mut TransportState,
        id: u32,
        payload: &[u8],
    ) -> Result<i32, TransportError>;
}

/// `0055F450 + channel * 0x18`: the frame being built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenFrame {
    pub slot_size: usize,
    pub used: usize,
    pub room: i32,
    pub cursor: usize,
    pub entries: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Channel {
    /// `0055F480 + channel * 0x14`.
    pub ring: MessageRing,
    pub open: Option<OpenFrame>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetTransport {
    pub state: TransportState,
    pub channels: [Channel; 2],
    /// `0055F2E0`.
    pub receive_ring: MessageRing,
    /// `SetEvent(0055F3E8)` calls: frames waiting for the send thread.
    pub send_signals: u32,
}

impl NetTransport {
    /// `004923D0`: the frame size, the local slot, the receive ring and the
    /// protocols, whose payload sizes below 4 become 4.
    pub fn setup_buffers(
        &mut self,
        size: i32,
        protocols: [&mut dyn NetProtocol; 2],
    ) -> Result<(), TransportError> {
        self.state.buffer_size = size.clamp(0x200, 0x400);
        self.state.local_slot = 0;
        self.state.slot_assigned = 1;
        self.receive_ring = MessageRing::build(RECEIVE_SLOTS, RECEIVE_SLOT_SIZE)?;
        for protocol in protocols {
            for row in protocol.rows_mut() {
                row.size = row.size.max(4);
            }
        }
        Ok(())
    }

    /// `00493300`'s channel tail: no frame open, and each send ring rebuilt
    /// with four slots of the frame size (`004928B0`).
    pub fn start_channels(&mut self) -> Result<(), TransportError> {
        for channel in &mut self.channels {
            channel.open = None;
            channel.ring.free();
        }
        for channel in &mut self.channels {
            channel.ring = MessageRing::build(SEND_SLOTS, self.state.buffer_size)?;
        }
        Ok(())
    }

    /// `00492EC0`: the channel's open frame, else a new one in the send
    /// ring's write slot with its prefix and the protocol's header.
    pub fn open_frame(
        &mut self,
        channel: usize,
        protocol: &mut dyn NetProtocol,
    ) -> Option<OpenFrame> {
        if let Some(open) = self.channels[channel].open {
            return Some(open);
        }
        let local_slot = self.state.local_slot;
        let state = &mut self.state;
        let ring = &mut self.channels[channel].ring;
        let slot_size = ring.slot_size();
        let slot = ring.write_slot()?;
        slot[..4].copy_from_slice(&[0, 0, channel as u8, local_slot]);
        let used = protocol
            .write_header(state, &mut slot[4..])
            .map_or(4, |length| length + 4);
        let open = OpenFrame {
            slot_size,
            used,
            room: slot_size as i32 - used as i32,
            cursor: used,
            entries: 0,
        };
        self.channels[channel].open = Some(open);
        Some(open)
    }

    /// `00493790`: an open frame with entries gets its length and is
    /// committed for the send thread.
    pub fn finish_frame(&mut self, channel: usize, protocol: &mut dyn NetProtocol) {
        let Some(open) = self.open_frame(channel, protocol) else {
            return;
        };
        if open.entries == 0 || open.used == 0 {
            return;
        }
        let ring = &mut self.channels[channel].ring;
        if let Some(slot) = ring.write_slot() {
            slot[..2].copy_from_slice(&(open.used as u16).to_le_bytes());
        }
        ring.commit();
        self.send_signals += 1;
        self.channels[channel].open = None;
    }

    /// `00492E50`: sends an open frame that has entries, then closes it.
    pub fn flush_frame(&mut self, channel: usize, protocol: &mut dyn NetProtocol) {
        if self
            .open_frame(channel, protocol)
            .is_some_and(|open| open.entries != 0)
        {
            self.finish_frame(channel, protocol);
        }
        self.channels[channel].open = None;
    }

    /// `00492500`: message `id` of the channel's protocol, encoded with
    /// `argument`, appended to the open frame (sent first when full).
    pub fn send(
        &mut self,
        channel: usize,
        protocol: &mut dyn NetProtocol,
        id: u32,
        argument: u32,
    ) -> Result<(), TransportError> {
        if !self.state.network_active || channel >= 2 || id >= protocol.message_count() {
            return Ok(());
        }
        let mut payload = [0u8; ENCODE_ROOM];
        let length = protocol.encode(&self.state, id, argument, &mut payload);
        if length == 0 {
            return Ok(());
        }
        if !(1..=ENCODE_ROOM as i32).contains(&length) {
            return Err(TransportError::EncoderLength { id, length });
        }
        let length = length as usize;
        let sized = self.state.size_prefix;
        let entry = length + 1 + if sized { 2 } else { 0 };
        let Some(mut open) = self.open_frame(channel, protocol) else {
            return Ok(());
        };
        if entry as i16 > open.room as i16 {
            self.finish_frame(channel, protocol);
            let Some(fresh) = self.open_frame(channel, protocol) else {
                return Ok(());
            };
            open = fresh;
        }
        if open.cursor + entry > open.slot_size {
            return Err(TransportError::EntryOverflow {
                channel,
                entry,
                slot_size: open.slot_size,
            });
        }
        let Some(slot) = self.channels[channel].ring.write_slot() else {
            return Ok(());
        };
        let mut cursor = open.cursor;
        slot[cursor] = id as u8 | if sized { 0x80 } else { 0 };
        cursor += 1;
        if sized {
            slot[cursor..cursor + 2].copy_from_slice(&(length as u16).to_le_bytes());
            cursor += 2;
        }
        slot[cursor..cursor + length].copy_from_slice(&payload[..length]);
        self.channels[channel].open = Some(OpenFrame {
            used: open.used + entry,
            room: open.room - entry as i32,
            cursor: cursor + length,
            entries: open.entries + 1,
            ..open
        });
        Ok(())
    }

    /// `004926E0`: a received frame to its channel's protocol. Returns 1 when
    /// that protocol does not receive.
    pub fn route(
        &mut self,
        protocols: [&mut dyn NetProtocol; 2],
        frame: &[u8],
    ) -> Result<u32, TransportError> {
        let channel = usize::from(*frame.get(2).ok_or(TransportError::FrameOverrun {
            offset: 2,
            length: frame.len(),
        })?);
        let Some(protocol) = protocols.into_iter().nth(channel) else {
            return Ok(0);
        };
        if !protocol.receives() {
            return Ok(1);
        }
        self.dispatch(protocol, frame)?;
        Ok(0)
    }

    /// `00492630`: the frame's header, then each entry to its message's
    /// receiver, while the frame's length lasts. An unknown id, or a sized
    /// entry its receiver reads otherwise, ends the frame.
    pub fn dispatch(
        &mut self,
        protocol: &mut dyn NetProtocol,
        frame: &[u8],
    ) -> Result<(), TransportError> {
        let overrun = |offset| TransportError::FrameOverrun {
            offset,
            length: frame.len(),
        };
        let bytes =
            |offset: usize, count: usize| frame.get(offset..offset + count).ok_or(overrun(offset));
        let total = i32::from(i16::from_le_bytes(bytes(0, 2)?.try_into().unwrap()));
        let mut consumed = 4usize;
        if let Some((length, proceed)) =
            protocol.read_header(&mut self.state, frame.get(4..).ok_or(overrun(4))?)?
        {
            consumed = length + 4;
            if !proceed {
                return Ok(());
            }
        }
        let mut position = consumed;
        while total - consumed as i32 > 0 {
            let byte = bytes(position, 1)?[0];
            let (id, sized) = (u32::from(byte & 0x7f), byte & 0x80 != 0);
            position += 1;
            consumed += 1;
            let mut size = 0;
            if sized {
                size = i32::from(i16::from_le_bytes(bytes(position, 2)?.try_into().unwrap()));
                position += 2;
                consumed += 2;
            }
            if id >= protocol.message_count() {
                return Ok(());
            }
            let used = protocol.receive(
                &mut self.state,
                id,
                frame.get(position..).ok_or(overrun(position))?,
            )?;
            if sized && used != size {
                return Ok(());
            }
            consumed = consumed.wrapping_add(used as usize);
            position = position.wrapping_add(used as usize);
        }
        Ok(())
    }

    /// `00493810`: the player table.
    pub fn player_table(&self) -> [u32; PLAYER_TABLE_ENTRIES] {
        self.state.players
    }
}

fn dword(bytes: &[u8], index: usize) -> Result<u32, TransportError> {
    bytes
        .get(index * 4..index * 4 + 4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .ok_or(TransportError::FrameOverrun {
            offset: index * 4,
            length: bytes.len(),
        })
}

/// `00540768`, channel 0's protocol: no header, and one message, the player
/// table (`00493100` out, `00492FF0` in).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerTableProtocol {
    pub rows: [ProtocolRow; 1],
}

impl Default for PlayerTableProtocol {
    fn default() -> Self {
        Self {
            rows: [ProtocolRow { id: 0, size: 8 }],
        }
    }
}

impl NetProtocol for PlayerTableProtocol {
    fn receives(&self) -> bool {
        true
    }
    fn message_count(&self) -> u32 {
        1
    }
    fn rows_mut(&mut self) -> &mut [ProtocolRow] {
        &mut self.rows
    }
    fn write_header(&mut self, _: &mut TransportState, _: &mut [u8]) -> Option<usize> {
        None
    }
    fn read_header(
        &mut self,
        _: &mut TransportState,
        _: &[u8],
    ) -> Result<Option<(usize, bool)>, TransportError> {
        Ok(None)
    }
    /// `00493100`: the host sends the whole table, a client its first entry.
    fn encode(&mut self, state: &TransportState, _: u32, _: u32, out: &mut [u8]) -> i32 {
        let count = if state.host { PLAYER_TABLE_ENTRIES } else { 1 };
        let length = count * 4 + 4;
        if out.len() < length {
            return -1;
        }
        out[..4].copy_from_slice(&(count as u32).to_le_bytes());
        for (index, player) in state.players[..count].iter().enumerate() {
            out[4 + index * 4..8 + index * 4].copy_from_slice(&player.to_le_bytes());
        }
        length as i32
    }
    /// `00492FF0`: the host adds a sender it does not know to the first free
    /// entry; a client takes the host's table and finds its own slot in it.
    fn receive(
        &mut self,
        state: &mut TransportState,
        _: u32,
        payload: &[u8],
    ) -> Result<i32, TransportError> {
        let count = dword(payload, 0)? as i32;
        let used = count.wrapping_mul(4).wrapping_add(4);
        state.tables_received = state.tables_received.wrapping_add(1);
        if state.host {
            // A zero id is never found, so it fills the first free entry.
            let id = dword(payload, 1)?;
            if id != 0 && state.players[1..].contains(&id) {
                return Ok(used);
            }
            if let Some(free) = state.players[1..].iter().position(|&player| player == 0) {
                state.players[free + 1] = id;
                state.players_added[0] = state.players_added[0].wrapping_add(1);
                state.players_added[1] = state.players_added[1].wrapping_add(1);
            }
            return Ok(used);
        }
        if count > 1 {
            for index in 0..count as usize {
                let id = dword(payload, 1 + index)?;
                if index == 0 {
                    continue;
                }
                // Past 17 entries retail writes on into 0055F444 and on.
                *state
                    .players
                    .get_mut(index)
                    .ok_or(TransportError::PlayerTableOverflow { count })? = id;
                if id == state.session_id && index != usize::from(state.local_slot) {
                    let previous = state.local_slot;
                    state.local_slot = if previous != 0 { 0 } else { index as u8 };
                    state.slot_assigned = 1;
                }
            }
        }
        Ok(used)
    }
}

/// `00540780`, channel 1's default protocol: a sequence DWORD heads each
/// frame (`00492F50` out, `00492F70` in), and its one message is a test
/// pattern (`00492FA0` out, `00492F90` in). It does not receive (`+8` 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SequenceProtocol {
    pub rows: [ProtocolRow; 1],
}

impl Default for SequenceProtocol {
    fn default() -> Self {
        Self {
            rows: [ProtocolRow { id: 0, size: 0x10 }],
        }
    }
}

impl NetProtocol for SequenceProtocol {
    fn receives(&self) -> bool {
        false
    }
    fn message_count(&self) -> u32 {
        1
    }
    fn rows_mut(&mut self) -> &mut [ProtocolRow] {
        &mut self.rows
    }
    /// `00492F50`: the next sequence number, then counts on.
    fn write_header(&mut self, state: &mut TransportState, out: &mut [u8]) -> Option<usize> {
        out[..4].copy_from_slice(&state.next_sequence.to_le_bytes());
        state.next_sequence = state.next_sequence.wrapping_add(1);
        Some(4)
    }
    /// `00492F70`: keeps the sender's sequence (`0055F380`).
    fn read_header(
        &mut self,
        state: &mut TransportState,
        header: &[u8],
    ) -> Result<Option<(usize, bool)>, TransportError> {
        state.last_sequence = dword(header, 0)?;
        Ok(Some((4, true)))
    }
    /// `00492FA0`: `(7n & 0x7F) + 8` bytes: that length, n, then the bytes
    /// left counting down to 1.
    fn encode(&mut self, _: &TransportState, _: u32, argument: u32, out: &mut [u8]) -> i32 {
        let length = (argument.wrapping_mul(7) & 0x7f) as usize + 8;
        if (out.len() as i32) < length as i32 {
            return -1;
        }
        out[..4].copy_from_slice(&(length as u32).to_le_bytes());
        out[4..8].copy_from_slice(&argument.to_le_bytes());
        for (index, byte) in out[8..length].iter_mut().enumerate() {
            *byte = (length - 8 - index) as u8;
        }
        length as i32
    }
    /// `00492F90`: the pattern's own length.
    fn receive(
        &mut self,
        _: &mut TransportState,
        _: u32,
        payload: &[u8],
    ) -> Result<i32, TransportError> {
        Ok(dword(payload, 0)? as i32)
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
