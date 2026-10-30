//! The port's DirectPlay objects (`0055F444`, the connection's, and
//! `0055F3B0`, the session's) with the transport they feed: the menus
//! enumerate, connect, search, host and join through it, and the frame runs
//! the receive, send and heartbeat threads' bodies ([`NetworkLink::pump`]).

use std::time::Instant;

use super::game::{GameNetState, TeamScore};
use super::game_tick::GameFrameState;
use super::session::HEARTBEAT_MICROS;
use super::transport::{
    NetProtocol, NetTransport, PlayerTableProtocol, SequenceProtocol, TransportError,
};
use super::udp::{
    NetSession, NetSessionError, Received, SearchTarget, SessionInfo, SessionSearch,
    CONNECTION_NAME, DEFAULT_PORT,
};

/// `00494230`/`00494190`'s fallbacks when Windows names no computer or user.
const SESSION_FALLBACK: &str = "Session";
const PLAYER_FALLBACK: &str = "unknown";

pub struct NetworkLink {
    /// `00494160`'s `InitializeConnection`, with the address the provider's
    /// dialog took.
    connection: Option<SearchTarget>,
    search: Option<SessionSearch>,
    session: Option<NetSession>,
    pub transport: NetTransport,
    pub table: PlayerTableProtocol,
    /// Channel 1's protocol until the game installs its own: retail's
    /// default, `00540780`.
    pub sequence: SequenceProtocol,
    /// The frame clock of the next heartbeat.
    heartbeat_due: Option<u64>,
    /// Set by a network menu dropped without moving forward.
    close_requested: crate::network_menu::CloseToken,
    /// The game protocol's state (`0054E3xx..0054E8xx`), which outlives the
    /// session's games.
    pub game: GameNetState,
    pub frame: GameFrameState,
}

/// The network module's release `00493260`: the shutdown `00493650`, then
/// its Win32 events and COM, which the port's transport does not have.
impl Drop for NetworkLink {
    fn drop(&mut self) {
        self.close();
    }
}

impl Default for NetworkLink {
    fn default() -> Self {
        let mut game = GameNetState::with_slot_records();
        // 0055F100's nineteen tasks, each its own team (`+4`).
        game.teams = std::array::from_fn(|team| {
            Some(TeamScore {
                index: team as u32,
                ..TeamScore::default()
            })
        });
        Self {
            connection: None,
            search: None,
            session: None,
            transport: NetTransport::default(),
            table: PlayerTableProtocol::default(),
            sequence: SequenceProtocol::default(),
            heartbeat_due: None,
            close_requested: Default::default(),
            game,
            frame: GameFrameState::default(),
        }
    }
}

impl NetworkLink {
    /// `00493F90`: the connections, as `00493F30` lists them.
    pub fn connections() -> [&'static str; 1] {
        [CONNECTION_NAME]
    }

    /// `00494160`: the chosen connection, searching where `target` says.
    pub fn initialize_connection(&mut self, target: SearchTarget) {
        self.search = None;
        self.connection = Some(target);
    }

    pub fn connected(&self) -> bool {
        self.connection.is_some()
    }

    /// `004940A0`: the sessions with room that answered lately; the query
    /// goes out again for the next call.
    pub fn enumerate_sessions(
        &mut self,
        now: Instant,
    ) -> Result<Vec<SessionInfo>, NetSessionError> {
        let Some(target) = self.connection.clone() else {
            return Ok(Vec::new());
        };
        if self.search.is_none() {
            self.search = Some(SessionSearch::new(target)?);
        }
        Ok(self.search.as_mut().expect("opened search").refresh(now))
    }

    /// `00494230`: a session named for this computer, with this user's
    /// player, then `00493300`'s start.
    pub fn host(&mut self, now: Instant) -> Result<(), NetLinkError> {
        let session = NetSession::host(&computer_name(), &user_name(), DEFAULT_PORT, now)
            .or_else(|_| NetSession::host(&computer_name(), &user_name(), 0, now))?;
        self.start(session)
    }

    /// `00494190`: this user's player in a searched session, then the start.
    pub fn join(&mut self, info: &SessionInfo) -> Result<(), NetLinkError> {
        let session = NetSession::join(info, &user_name())?;
        self.start(session)
    }

    fn start(&mut self, session: NetSession) -> Result<(), NetLinkError> {
        self.search = None;
        self.transport.start_session(
            session.local_id(),
            session.is_host(),
            [&mut self.table, &mut self.sequence],
        )?;
        self.session = Some(session);
        self.heartbeat_due = None;
        Ok(())
    }

    pub fn session(&self) -> Option<&NetSession> {
        self.session.as_ref()
    }

    /// The close request the network menus hold.
    pub fn close_token(&self) -> crate::network_menu::CloseToken {
        self.close_requested.clone()
    }

    /// [`Self::pump`] with channel 1 on its default protocol.
    pub fn pump_default(&mut self, now_micros: u64, now: Instant) -> Result<(), TransportError> {
        let mut sequence = std::mem::take(&mut self.sequence);
        let result = self.pump(now_micros, now, &mut sequence);
        self.sequence = sequence;
        result
    }

    /// The host's first heartbeat, which gives it its slot (`0045F320`
    /// waits up to 3 s for the heartbeat thread's); whether it has one.
    pub fn first_heartbeat(&mut self) -> Result<bool, TransportError> {
        self.transport.heartbeat(&mut self.table)?;
        Ok(self.transport.state.local_slot != 0)
    }

    /// `00492830` with channel 1 on its default protocol.
    pub fn drain_default(&mut self) -> Result<(), TransportError> {
        self.transport
            .drain_received([&mut self.table, &mut self.sequence])
    }

    /// The threads' bodies for one frame: every message received goes to
    /// the transport (`00493C00`), a heartbeat runs each second
    /// (`004934C0`), and the committed frames go out (`00493CC0`).
    pub fn pump(
        &mut self,
        now_micros: u64,
        now: Instant,
        game: &mut dyn NetProtocol,
    ) -> Result<(), TransportError> {
        if self
            .close_requested
            .swap(false, std::sync::atomic::Ordering::Relaxed)
        {
            self.close();
        }
        let Some(session) = self.session.as_mut() else {
            return Ok(());
        };
        while let Some(received) = session.receive(now) {
            match received {
                Received::Game { data, .. } => self
                    .transport
                    .receive_message([&mut self.table, &mut *game], &data)?,
                Received::System(message) => self.transport.system_message(&message),
            }
        }
        let due = *self
            .heartbeat_due
            .get_or_insert(now_micros + HEARTBEAT_MICROS);
        if now_micros >= due {
            self.heartbeat_due = Some(now_micros + HEARTBEAT_MICROS);
            self.transport.heartbeat(&mut self.table)?;
        }
        for frame in self.transport.take_outgoing() {
            session.send_all(&frame);
        }
        Ok(())
    }

    /// `00493650`: the session left, and the connection released.
    pub fn close(&mut self) {
        if let Some(session) = self.session.take() {
            session.close();
        }
        self.search = None;
        self.connection = None;
        self.heartbeat_due = None;
        self.transport.close_session();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum NetLinkError {
    #[error(transparent)]
    Session(#[from] NetSessionError),
    #[error(transparent)]
    Transport(#[from] TransportError),
}

/// `GetComputerNameA`, else "Session".
pub fn computer_name() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| SESSION_FALLBACK.to_string())
}

/// `GetUserNameA`, else "unknown".
pub fn user_name() -> String {
    std::env::var("USERNAME")
        .or_else(|_| std::env::var("USER"))
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| PLAYER_FALLBACK.to_string())
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
