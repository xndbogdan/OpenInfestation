//! The session around the transport's rings: its start (`00493300`), the
//! heartbeat thread (`004934C0`), the shutdown (`00493650`), and the receive
//! and send threads (`00493C00`, `00493CC0`) with the system messages
//! `00493A50` handles. Retail runs the three threads beside the game; the
//! port runs their bodies from the frame, over [`super::udp::NetSession`].

use super::transport::{NetProtocol, NetTransport, TransportError, PLAYER_TABLE_ENTRIES};
use super::udp::SystemMessage;

/// `004923D0`'s clamp on `GetCaps`' buffer size less the frame's header:
/// DirectPlay's TCP/IP buffers are larger than its top, `0x400`.
pub const FRAME_SIZE: i32 = 0x400;
/// `004934C0`'s period.
pub const HEARTBEAT_MICROS: u64 = 1_000_000;
/// `0049359C`: the host resends the player table after this many quiet
/// heartbeats.
const TABLE_PERIOD: i32 = 10;

impl NetTransport {
    /// `00493840`/`00493950` then `00493300`: the session's id heads the
    /// player table, the host flag is the session's `+C` (`0055F3BC`), the
    /// buffers and both send rings are rebuilt, and the network flag set.
    pub fn start_session(
        &mut self,
        session_id: u32,
        host: bool,
        protocols: [&mut dyn NetProtocol; 2],
    ) -> Result<(), TransportError> {
        self.state.players = [0; PLAYER_TABLE_ENTRIES];
        self.state.players[0] = session_id;
        self.state.session_id = session_id;
        self.state.host = host;
        self.state.slot_assigned = 1;
        self.setup_buffers(FRAME_SIZE, protocols)?;
        self.start_channels()?;
        self.state.network_active = true;
        Ok(())
    }

    /// `004934C0`'s body, once a second while a session runs. The host takes
    /// a slot when it has none: the entry holding its own id, else the first
    /// free one, which it fills. It resends the player table after eleven
    /// quiet heartbeats, or at once when it added a player; a guest sends its
    /// own until it has a slot, and when it was asked to. Then channel 0's
    /// open frame goes out.
    pub fn heartbeat(&mut self, table: &mut dyn NetProtocol) -> Result<(), TransportError> {
        if !self.state.network_active {
            return Ok(());
        }
        let state = &mut self.state;
        state.heartbeats = state.heartbeats.wrapping_add(1);
        let send = if state.host {
            if state.local_slot == 0 {
                let own = state.players[0];
                let slot = (1..PLAYER_TABLE_ENTRIES)
                    .find(|&index| state.players[index] == own)
                    .or_else(|| {
                        let free =
                            (1..PLAYER_TABLE_ENTRIES).find(|&index| state.players[index] == 0)?;
                        state.players[free] = own;
                        Some(free)
                    });
                if let Some(slot) = slot {
                    state.local_slot = slot as u8;
                    state.slot_assigned = 1;
                }
            }
            if state.players_added[0] == 0 && state.table_countdown <= TABLE_PERIOD {
                state.table_countdown += 1;
                false
            } else {
                state.tables_sent = state.tables_sent.wrapping_add(1);
                true
            }
        } else {
            state.local_slot == 0 || state.players_added[0] != 0
        };
        if send {
            self.state.table_countdown = 0;
            self.state.players_added[0] = 0;
            self.send(0, table, 0, 0)?;
        }
        self.finish_frame(0, table);
        Ok(())
    }

    /// `00493C00`'s body for one received message. A game message longer
    /// than three bytes is written into the receive ring's next slot and
    /// routed (`004926E0`); the slot is kept when the route asks.
    pub fn receive_message(
        &mut self,
        protocols: [&mut dyn NetProtocol; 2],
        data: &[u8],
    ) -> Result<(), TransportError> {
        if data.len() <= 3 {
            return Ok(());
        }
        let Some(slot) = self.receive_ring.write_slot() else {
            return Ok(());
        };
        if data.len() > slot.len() {
            return Ok(());
        }
        slot[..data.len()].copy_from_slice(data);
        let frame = slot[..data.len()].to_vec();
        if self.route(protocols, &frame)? != 0 {
            self.receive_ring.commit();
        }
        Ok(())
    }

    /// `00493A50`: a player destroyed leaves the table; a lost session and a
    /// host that migrated here set their flags. The joined and left texts it
    /// builds are freed unshown.
    pub fn system_message(&mut self, message: &SystemMessage) {
        match message {
            SystemMessage::PlayerCreated { .. } => {}
            SystemMessage::PlayerDestroyed { id, .. } => {
                if let Some(index) =
                    (1..PLAYER_TABLE_ENTRIES).find(|&index| self.state.players[index] == *id)
                {
                    self.state.players[index] = 0;
                }
            }
            SystemMessage::SessionLost => self.state.session_lost = true,
            SystemMessage::BecameHost => {
                self.state.host = true;
                self.state.host_changed = true;
                self.state.slot_assigned = 1;
            }
        }
    }

    /// `00492830`: the received frames queued on the receive ring, each on
    /// channel 0 or 1 to that channel's protocol (`00492630`).
    pub fn drain_received(
        &mut self,
        protocols: [&mut dyn NetProtocol; 2],
    ) -> Result<(), TransportError> {
        while let Some(slot) = self.receive_ring.read_slot() {
            let length = usize::from(u16::from_le_bytes([slot[0], slot[1]])).min(slot.len());
            let frame = slot[..length.max(4)].to_vec();
            let channel = usize::from(frame[2]);
            if channel < 2 {
                let protocol: &mut dyn NetProtocol = &mut *protocols[channel];
                self.dispatch(protocol, &frame)?;
            }
            self.receive_ring.pop();
        }
        Ok(())
    }

    /// `00493CC0`'s body: one committed frame from each send ring in turn,
    /// until both are empty, each its WORD length long.
    pub fn take_outgoing(&mut self) -> Vec<Vec<u8>> {
        let mut frames = Vec::new();
        loop {
            let mut sent = false;
            for channel in &mut self.channels {
                if let Some(slot) = channel.ring.read_slot() {
                    let length =
                        usize::from(u16::from_le_bytes([slot[0], slot[1]])).min(slot.len());
                    frames.push(slot[..length].to_vec());
                    channel.ring.pop();
                    sent = true;
                }
            }
            if !sent {
                return frames;
            }
        }
    }

    /// `00493650`: the network flag cleared, the slot back to 1 and assigned,
    /// and the player table cleared.
    pub fn close_session(&mut self) {
        self.state.network_active = false;
        self.state.local_slot = 1;
        self.state.slot_assigned = 1;
        self.state.players = [0; PLAYER_TABLE_ENTRIES];
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
