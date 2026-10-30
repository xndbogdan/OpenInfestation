//! The port's DirectPlay: a peer-to-peer UDP session in place of DirectPlay's
//! TCP/IP service provider. It answers the calls the game makes of DirectPlay
//! ([NETWORK_RUNTIME.md](../../../../docs/re/NETWORK_RUNTIME.md), "The
//! port's transport"):
//!
//! - one connection (`EnumConnections`, `00493F90`);
//! - session searches (`EnumSessions`, `004940A0`), asynchronous and listing
//!   only sessions with room;
//! - hosting and joining (`Open` and `CreatePlayer`, `00493840` and
//!   `00493950`), a session of at most 16 players;
//! - unguaranteed sends to every other player (`Send` with no flags,
//!   `00493CC0`);
//! - the system messages `00493A50` reads: a player created (3) or destroyed
//!   (5), the session lost (`0x31`) and becoming the host (`0x101`).
//!
//! Retail's sessions (flags `00540740`, `0x2044`) keep their players alive
//! and migrate the host. Here the host answers searches and joins and resends
//! the player list every second; every peer sends to every other; a peer not
//! heard from for [`TIMEOUT`] leaves; when the host leaves, the lowest
//! remaining id becomes the host. The wire format is the port's own, so
//! retail clients cannot join.

use std::collections::hash_map::RandomState;
use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, ToSocketAddrs, UdpSocket};
use std::time::{Duration, Instant};

use thiserror::Error;

/// The UDP port a host listens on, and the one a search broadcasts to.
pub const DEFAULT_PORT: u16 = 47630;
/// `00493840`'s `dwMaxPlayers`.
pub const MAX_PLAYERS: usize = 16;
/// The one connection the port offers (`00493F30`'s rows).
pub const CONNECTION_NAME: &str = "Internet TCP/IP Connection";
/// `{9FA88941-E07E-11D3-BD94-00105AAEB976}`, the application GUID
/// `00493840` and `004940A0` pass, as its bytes are stored.
pub const APPLICATION: [u8; 16] = [
    0x41, 0x89, 0xa8, 0x9f, 0x7e, 0xe0, 0xd3, 0x11, 0xbd, 0x94, 0x00, 0x10, 0x5a, 0xae, 0xb9, 0x76,
];
/// How often peers ping each other and the host resends the player list.
pub const KEEPALIVE: Duration = Duration::from_secs(1);
/// A peer silent this long has left the session.
pub const TIMEOUT: Duration = Duration::from_secs(10);
/// A search forgets a session that stopped answering for this long.
pub const SEARCH_EXPIRY: Duration = Duration::from_secs(3);
const JOIN_RETRY: Duration = Duration::from_millis(250);
const JOIN_TIMEOUT: Duration = Duration::from_secs(5);
const MAGIC: &[u8; 4] = b"INFN";
const VERSION: u8 = 1;
const MAX_DATAGRAM: usize = 2048;
const NAME_LIMIT: usize = 63;

#[derive(Debug, Error)]
pub enum NetSessionError {
    #[error("network socket: {0}")]
    Io(#[from] io::Error),
    #[error("no session answered the join")]
    JoinTimedOut,
    #[error("the session refused the join: {0}")]
    Refused(&'static str),
    #[error("the address {0:?} names no host")]
    Address(String),
}

/// A session a search found (`004940A0`'s callback `00494030`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionInfo {
    pub instance: u64,
    pub name: String,
    pub address: SocketAddr,
    pub players: u8,
    pub max_players: u8,
}

/// The system messages `00493A50` handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SystemMessage {
    /// `DPSYS_CREATEPLAYERORGROUP` (3).
    PlayerCreated { id: u32, name: String },
    /// `DPSYS_DESTROYPLAYERORGROUP` (5).
    PlayerDestroyed { id: u32, name: String },
    /// `DPSYS_SESSIONLOST` (`0x31`).
    SessionLost,
    /// `DPSYS_HOST` (`0x101`): the session migrated its host here.
    BecameHost,
}

/// What `Receive` returns: a player's message, or a system message (from 0).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Received {
    Game { from: u32, data: Vec<u8> },
    System(SystemMessage),
}

/// Where a search looks: the local network's broadcast, and addresses typed
/// in (DirectPlay's TCP/IP dialog asked for one; blank searched the LAN).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchTarget {
    pub broadcast: bool,
    pub addresses: Vec<SocketAddr>,
}

impl SearchTarget {
    /// The dialog's text: blank searches the local network, otherwise a host
    /// name or address, with `:port` when the host is not on [`DEFAULT_PORT`].
    pub fn parse(text: &str) -> Result<Self, NetSessionError> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self {
                broadcast: true,
                addresses: Vec::new(),
            });
        }
        let with_port = if text
            .rsplit_once(':')
            .is_some_and(|(_, port)| port.parse::<u16>().is_ok())
        {
            text.to_string()
        } else {
            format!("{text}:{DEFAULT_PORT}")
        };
        let addresses: Vec<SocketAddr> = with_port
            .to_socket_addrs()
            .map_err(|_| NetSessionError::Address(text.to_string()))?
            .filter(SocketAddr::is_ipv4)
            .collect();
        if addresses.is_empty() {
            return Err(NetSessionError::Address(text.to_string()));
        }
        Ok(Self {
            broadcast: false,
            addresses,
        })
    }
}

/// `EnumSessions` with `DPENUMSESSIONS_ASYNC | DPENUMSESSIONS_AVAILABLE`:
/// each refresh sends the query and lists the sessions that answered lately.
pub struct SessionSearch {
    socket: UdpSocket,
    target: SearchTarget,
    nonce: u32,
    found: Vec<(SessionInfo, Instant)>,
}

impl SessionSearch {
    pub fn new(target: SearchTarget) -> Result<Self, NetSessionError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_nonblocking(true)?;
        socket.set_broadcast(true)?;
        Ok(Self {
            socket,
            target,
            nonce: random_u32(),
            found: Vec::new(),
        })
    }

    pub fn target(&self) -> &SearchTarget {
        &self.target
    }

    /// Takes the answers received since the last call, forgets sessions that
    /// stopped answering, then asks again. Returns the sessions with room,
    /// oldest first.
    pub fn refresh(&mut self, now: Instant) -> Vec<SessionInfo> {
        let mut buffer = [0u8; MAX_DATAGRAM];
        while let Ok((length, from)) = self.socket.recv_from(&mut buffer) {
            if let Some(Packet::Session {
                nonce,
                instance,
                name,
                players,
                max_players,
            }) = Packet::decode(&buffer[..length])
            {
                if nonce != self.nonce {
                    continue;
                }
                let info = SessionInfo {
                    instance,
                    name,
                    address: from,
                    players,
                    max_players,
                };
                match self
                    .found
                    .iter_mut()
                    .find(|(seen, _)| seen.instance == instance)
                {
                    Some(entry) => *entry = (info, now),
                    None => self.found.push((info, now)),
                }
            }
        }
        self.found
            .retain(|(_, seen)| now.saturating_duration_since(*seen) < SEARCH_EXPIRY);
        let query = Packet::Query {
            application: APPLICATION,
            nonce: self.nonce,
        }
        .encode();
        if self.target.broadcast {
            let _ = self
                .socket
                .send_to(&query, SocketAddrV4::new(Ipv4Addr::BROADCAST, DEFAULT_PORT));
        }
        for address in &self.target.addresses {
            let _ = self.socket.send_to(&query, address);
        }
        self.found
            .iter()
            .map(|(info, _)| info.clone())
            .filter(|info| usize::from(info.players) < usize::from(info.max_players))
            .collect()
    }
}

#[derive(Clone, Debug)]
struct Peer {
    id: u32,
    name: String,
    address: SocketAddr,
    heard: Instant,
}

/// An open session: `Open` plus the player `CreatePlayer` made.
pub struct NetSession {
    socket: UdpSocket,
    instance: u64,
    name: String,
    local: u32,
    local_name: String,
    host: u32,
    peers: Vec<Peer>,
    inbox: VecDeque<Received>,
    keepalive: Instant,
    lost: bool,
}

impl NetSession {
    /// `00494230`: `Open` with `DPOPEN_CREATE`, named `session`, and the
    /// local player `player`. Port 0 takes any free port.
    pub fn host(
        session: &str,
        player: &str,
        port: u16,
        now: Instant,
    ) -> Result<Self, NetSessionError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port))?;
        socket.set_nonblocking(true)?;
        let local = random_id(&[]);
        Ok(Self {
            socket,
            instance: u64::from(random_u32()) << 32 | u64::from(random_u32()),
            name: limit(session),
            local,
            local_name: limit(player),
            host: local,
            peers: Vec::new(),
            inbox: VecDeque::new(),
            keepalive: now,
            lost: false,
        })
    }

    /// `00494190`: `Open` with `DPOPEN_JOIN` on a searched session, then the
    /// local player. Like DirectPlay's, the call waits for the host's answer.
    pub fn join(session: &SessionInfo, player: &str) -> Result<Self, NetSessionError> {
        let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_read_timeout(Some(JOIN_RETRY))?;
        let nonce = random_u32();
        let request = Packet::Join {
            instance: session.instance,
            nonce,
            name: limit(player),
        }
        .encode();
        let start = Instant::now();
        let mut buffer = [0u8; MAX_DATAGRAM];
        while start.elapsed() < JOIN_TIMEOUT {
            socket.send_to(&request, session.address)?;
            let Ok((length, from)) = socket.recv_from(&mut buffer) else {
                continue;
            };
            match Packet::decode(&buffer[..length]) {
                Some(Packet::Welcome {
                    instance,
                    nonce: answered,
                    local,
                    roster,
                }) if instance == session.instance && answered == nonce => {
                    socket.set_read_timeout(None)?;
                    socket.set_nonblocking(true)?;
                    let now = Instant::now();
                    let mut joined = Self {
                        socket,
                        instance,
                        name: session.name.clone(),
                        local,
                        local_name: limit(player),
                        host: roster.host,
                        peers: Vec::new(),
                        inbox: VecDeque::new(),
                        keepalive: now,
                        lost: false,
                    };
                    joined.apply_roster(roster, from, now, false);
                    return Ok(joined);
                }
                Some(Packet::Refused {
                    nonce: answered,
                    reason,
                }) if answered == nonce => {
                    return Err(NetSessionError::Refused(match reason {
                        REFUSED_FULL => "the session is full",
                        _ => "the session is gone",
                    }));
                }
                _ => {}
            }
        }
        Err(NetSessionError::JoinTimedOut)
    }

    /// The local player's id (`0055F3B8`).
    pub fn local_id(&self) -> u32 {
        self.local
    }

    /// Whether this machine hosts the session (`0055F3A0`).
    pub fn is_host(&self) -> bool {
        self.host == self.local
    }

    pub fn session_name(&self) -> &str {
        &self.name
    }

    /// The port the session's socket listens on.
    pub fn port(&self) -> Result<u16, NetSessionError> {
        Ok(self.socket.local_addr()?.port())
    }

    /// The players, the local one first.
    pub fn players(&self) -> Vec<(u32, String)> {
        std::iter::once((self.local, self.local_name.clone()))
            .chain(self.peers.iter().map(|peer| (peer.id, peer.name.clone())))
            .collect()
    }

    /// `Send` from the local player to every other, unguaranteed.
    pub fn send_all(&self, data: &[u8]) {
        let packet = Packet::Data {
            instance: self.instance,
            from: self.local,
            data: data.to_vec(),
        }
        .encode();
        for peer in &self.peers {
            let _ = self.socket.send_to(&packet, peer.address);
        }
    }

    /// `Receive`: the next message, after taking what arrived and running
    /// the keep-alive.
    pub fn receive(&mut self, now: Instant) -> Option<Received> {
        if self.inbox.is_empty() {
            self.poll(now);
        }
        self.inbox.pop_front()
    }

    /// Takes every datagram waiting, then pings, resends the player list from
    /// the host, and drops the silent.
    pub fn poll(&mut self, now: Instant) {
        if self.lost {
            return;
        }
        let mut buffer = [0u8; MAX_DATAGRAM];
        while let Ok((length, from)) = self.socket.recv_from(&mut buffer) {
            if let Some(packet) = Packet::decode(&buffer[..length]) {
                self.handle(packet, from, now);
            }
        }
        if now.saturating_duration_since(self.keepalive) >= KEEPALIVE {
            self.keepalive = now;
            let ping = Packet::Ping {
                instance: self.instance,
                from: self.local,
            }
            .encode();
            for peer in &self.peers {
                let _ = self.socket.send_to(&ping, peer.address);
            }
            if self.is_host() {
                self.send_roster();
            }
        }
        self.expire(now);
    }

    /// Leaves the session (`00493650`'s `DestroyPlayer` and `Close`).
    pub fn close(self) {
        let leave = Packet::Leave {
            instance: self.instance,
            from: self.local,
        }
        .encode();
        for peer in &self.peers {
            let _ = self.socket.send_to(&leave, peer.address);
        }
    }

    fn handle(&mut self, packet: Packet, from: SocketAddr, now: Instant) {
        match packet {
            Packet::Query { application, nonce } => {
                if self.is_host() && application == APPLICATION {
                    let answer = Packet::Session {
                        nonce,
                        instance: self.instance,
                        name: self.name.clone(),
                        players: (self.peers.len() + 1) as u8,
                        max_players: MAX_PLAYERS as u8,
                    };
                    let _ = self.socket.send_to(&answer.encode(), from);
                }
            }
            Packet::Join {
                instance,
                nonce,
                name,
            } => {
                if !self.is_host() || instance != self.instance {
                    let refusal = Packet::Refused {
                        nonce,
                        reason: REFUSED_GONE,
                    };
                    let _ = self.socket.send_to(&refusal.encode(), from);
                    return;
                }
                let local = match self.peers.iter().find(|peer| peer.address == from) {
                    Some(peer) => peer.id,
                    None if self.peers.len() + 1 >= MAX_PLAYERS => {
                        let refusal = Packet::Refused {
                            nonce,
                            reason: REFUSED_FULL,
                        };
                        let _ = self.socket.send_to(&refusal.encode(), from);
                        return;
                    }
                    None => {
                        let mut taken: Vec<u32> = self.peers.iter().map(|peer| peer.id).collect();
                        taken.push(self.local);
                        let id = random_id(&taken);
                        let name = limit(&name);
                        self.peers.push(Peer {
                            id,
                            name: name.clone(),
                            address: from,
                            heard: now,
                        });
                        self.inbox
                            .push_back(Received::System(SystemMessage::PlayerCreated { id, name }));
                        self.send_roster();
                        id
                    }
                };
                let welcome = Packet::Welcome {
                    instance: self.instance,
                    nonce,
                    local,
                    roster: self.roster(),
                };
                let _ = self.socket.send_to(&welcome.encode(), from);
            }
            Packet::Roster { instance, roster } if instance == self.instance => {
                if roster.host != self.local {
                    self.apply_roster(roster, from, now, true);
                }
            }
            Packet::Data {
                instance,
                from: id,
                data,
            } if instance == self.instance => {
                if let Some(peer) = self.peers.iter_mut().find(|peer| peer.id == id) {
                    peer.heard = now;
                    peer.address = from;
                    self.inbox.push_back(Received::Game { from: id, data });
                }
            }
            Packet::Ping { instance, from: id } if instance == self.instance => {
                if let Some(peer) = self.peers.iter_mut().find(|peer| peer.id == id) {
                    peer.heard = now;
                }
            }
            Packet::Leave { instance, from: id } if instance == self.instance => {
                self.remove(id, now);
            }
            _ => {}
        }
    }

    fn roster(&self) -> Roster {
        let mut entries = vec![RosterEntry {
            id: self.local,
            name: self.local_name.clone(),
            address: None,
        }];
        entries.extend(self.peers.iter().map(|peer| RosterEntry {
            id: peer.id,
            name: peer.name.clone(),
            address: Some(peer.address),
        }));
        Roster {
            host: self.host,
            entries,
        }
    }

    fn send_roster(&self) {
        let packet = Packet::Roster {
            instance: self.instance,
            roster: self.roster(),
        }
        .encode();
        for peer in &self.peers {
            let _ = self.socket.send_to(&packet, peer.address);
        }
    }

    /// The host's player list: players it names that are new here are
    /// created, players it no longer names are destroyed, and a list that
    /// leaves this player out loses the session. The host's own entry has no
    /// address: it is the sender's.
    fn apply_roster(&mut self, roster: Roster, sender: SocketAddr, now: Instant, announce: bool) {
        if !roster.entries.iter().any(|entry| entry.id == self.local) {
            self.lose();
            return;
        }
        self.host = roster.host;
        let gone: Vec<Peer> = self
            .peers
            .iter()
            .filter(|peer| !roster.entries.iter().any(|entry| entry.id == peer.id))
            .cloned()
            .collect();
        for peer in gone {
            self.peers.retain(|kept| kept.id != peer.id);
            if announce {
                self.inbox
                    .push_back(Received::System(SystemMessage::PlayerDestroyed {
                        id: peer.id,
                        name: peer.name,
                    }));
            }
        }
        for entry in roster.entries {
            if entry.id == self.local {
                continue;
            }
            let address = entry.address.unwrap_or(sender);
            match self.peers.iter_mut().find(|peer| peer.id == entry.id) {
                Some(peer) => {
                    peer.address = address;
                    if entry.id == roster.host {
                        peer.heard = now;
                    }
                }
                None => {
                    if announce {
                        self.inbox
                            .push_back(Received::System(SystemMessage::PlayerCreated {
                                id: entry.id,
                                name: entry.name.clone(),
                            }));
                    }
                    self.peers.push(Peer {
                        id: entry.id,
                        name: entry.name,
                        address,
                        heard: now,
                    });
                }
            }
        }
    }

    fn expire(&mut self, now: Instant) {
        let silent: Vec<u32> = self
            .peers
            .iter()
            .filter(|peer| now.saturating_duration_since(peer.heard) >= TIMEOUT)
            .map(|peer| peer.id)
            .collect();
        for id in silent {
            if self.is_host() {
                // The dropped player hears the list without it, once.
                if let Some(peer) = self.peers.iter().find(|peer| peer.id == id) {
                    let address = peer.address;
                    self.remove(id, now);
                    let goodbye = Packet::Roster {
                        instance: self.instance,
                        roster: self.roster(),
                    };
                    let _ = self.socket.send_to(&goodbye.encode(), address);
                }
            } else if id == self.host {
                // A client drops only the host, and only while someone else
                // is still heard; hearing no one, the session is lost.
                let heard = self.peers.iter().any(|peer| {
                    peer.id != id && now.saturating_duration_since(peer.heard) < TIMEOUT
                });
                if heard {
                    self.remove(id, now);
                } else {
                    self.lose();
                }
            }
        }
    }

    /// A player leaves: destroyed here, and when it was the host, the lowest
    /// remaining id takes over.
    fn remove(&mut self, id: u32, now: Instant) {
        let Some(index) = self.peers.iter().position(|peer| peer.id == id) else {
            return;
        };
        let peer = self.peers.remove(index);
        self.inbox
            .push_back(Received::System(SystemMessage::PlayerDestroyed {
                id: peer.id,
                name: peer.name,
            }));
        if id == self.host {
            let next = self
                .peers
                .iter()
                .map(|peer| peer.id)
                .chain(std::iter::once(self.local))
                .min()
                .unwrap_or(self.local);
            self.host = next;
            if next == self.local {
                self.inbox
                    .push_back(Received::System(SystemMessage::BecameHost));
                self.send_roster();
                self.keepalive = now;
            }
        }
    }

    fn lose(&mut self) {
        if !self.lost {
            self.lost = true;
            self.peers.clear();
            self.inbox
                .push_back(Received::System(SystemMessage::SessionLost));
        }
    }
}

const REFUSED_FULL: u8 = 1;
const REFUSED_GONE: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
struct RosterEntry {
    id: u32,
    name: String,
    address: Option<SocketAddr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Roster {
    host: u32,
    entries: Vec<RosterEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Packet {
    Query {
        application: [u8; 16],
        nonce: u32,
    },
    Session {
        nonce: u32,
        instance: u64,
        name: String,
        players: u8,
        max_players: u8,
    },
    Join {
        instance: u64,
        nonce: u32,
        name: String,
    },
    Welcome {
        instance: u64,
        nonce: u32,
        local: u32,
        roster: Roster,
    },
    Refused {
        nonce: u32,
        reason: u8,
    },
    Roster {
        instance: u64,
        roster: Roster,
    },
    Data {
        instance: u64,
        from: u32,
        data: Vec<u8>,
    },
    Ping {
        instance: u64,
        from: u32,
    },
    Leave {
        instance: u64,
        from: u32,
    },
}

impl Packet {
    fn encode(&self) -> Vec<u8> {
        let mut out = Writer(Vec::with_capacity(64));
        out.bytes(MAGIC);
        out.u8(VERSION);
        match self {
            Self::Query { application, nonce } => {
                out.u8(1);
                out.bytes(application);
                out.u32(*nonce);
            }
            Self::Session {
                nonce,
                instance,
                name,
                players,
                max_players,
            } => {
                out.u8(2);
                out.u32(*nonce);
                out.u64(*instance);
                out.string(name);
                out.u8(*players);
                out.u8(*max_players);
            }
            Self::Join {
                instance,
                nonce,
                name,
            } => {
                out.u8(3);
                out.u64(*instance);
                out.u32(*nonce);
                out.string(name);
            }
            Self::Welcome {
                instance,
                nonce,
                local,
                roster,
            } => {
                out.u8(4);
                out.u64(*instance);
                out.u32(*nonce);
                out.u32(*local);
                out.roster(roster);
            }
            Self::Refused { nonce, reason } => {
                out.u8(5);
                out.u32(*nonce);
                out.u8(*reason);
            }
            Self::Roster { instance, roster } => {
                out.u8(6);
                out.u64(*instance);
                out.roster(roster);
            }
            Self::Data {
                instance,
                from,
                data,
            } => {
                out.u8(7);
                out.u64(*instance);
                out.u32(*from);
                out.bytes(data);
            }
            Self::Ping { instance, from } => {
                out.u8(8);
                out.u64(*instance);
                out.u32(*from);
            }
            Self::Leave { instance, from } => {
                out.u8(9);
                out.u64(*instance);
                out.u32(*from);
            }
        }
        out.0
    }

    fn decode(bytes: &[u8]) -> Option<Self> {
        let mut input = Reader(bytes);
        if input.take(4)? != MAGIC || input.u8()? != VERSION {
            return None;
        }
        let packet = match input.u8()? {
            1 => Self::Query {
                application: input.take(16)?.try_into().ok()?,
                nonce: input.u32()?,
            },
            2 => Self::Session {
                nonce: input.u32()?,
                instance: input.u64()?,
                name: input.string()?,
                players: input.u8()?,
                max_players: input.u8()?,
            },
            3 => Self::Join {
                instance: input.u64()?,
                nonce: input.u32()?,
                name: input.string()?,
            },
            4 => Self::Welcome {
                instance: input.u64()?,
                nonce: input.u32()?,
                local: input.u32()?,
                roster: input.roster()?,
            },
            5 => Self::Refused {
                nonce: input.u32()?,
                reason: input.u8()?,
            },
            6 => Self::Roster {
                instance: input.u64()?,
                roster: input.roster()?,
            },
            7 => {
                let instance = input.u64()?;
                let from = input.u32()?;
                return Some(Self::Data {
                    instance,
                    from,
                    data: input.0.to_vec(),
                });
            }
            8 => Self::Ping {
                instance: input.u64()?,
                from: input.u32()?,
            },
            9 => Self::Leave {
                instance: input.u64()?,
                from: input.u32()?,
            },
            _ => return None,
        };
        input.0.is_empty().then_some(packet)
    }
}

struct Writer(Vec<u8>);

impl Writer {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn u8(&mut self, value: u8) {
        self.0.push(value);
    }
    fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }
    fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }
    fn string(&mut self, text: &str) {
        let bytes = &text.as_bytes()[..text.len().min(NAME_LIMIT)];
        self.u8(bytes.len() as u8);
        self.bytes(bytes);
    }
    fn roster(&mut self, roster: &Roster) {
        self.u32(roster.host);
        self.u8(roster.entries.len() as u8);
        for entry in &roster.entries {
            self.u32(entry.id);
            self.string(&entry.name);
            match entry.address {
                Some(SocketAddr::V4(address)) => {
                    self.u8(1);
                    self.bytes(&address.ip().octets());
                    self.bytes(&address.port().to_le_bytes());
                }
                _ => self.u8(0),
            }
        }
    }
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        if self.0.len() < count {
            return None;
        }
        let (head, tail) = self.0.split_at(count);
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }
    fn string(&mut self) -> Option<String> {
        let length = usize::from(self.u8()?);
        Some(String::from_utf8_lossy(self.take(length)?).into_owned())
    }
    fn roster(&mut self) -> Option<Roster> {
        let host = self.u32()?;
        let count = usize::from(self.u8()?);
        if count > MAX_PLAYERS {
            return None;
        }
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let id = self.u32()?;
            let name = self.string()?;
            let address = match self.u8()? {
                0 => None,
                1 => {
                    let octets: [u8; 4] = self.take(4)?.try_into().ok()?;
                    let port = u16::from_le_bytes(self.take(2)?.try_into().ok()?);
                    Some(SocketAddr::V4(SocketAddrV4::new(octets.into(), port)))
                }
                _ => return None,
            };
            entries.push(RosterEntry { id, name, address });
        }
        Some(Roster { host, entries })
    }
}

fn limit(text: &str) -> String {
    let mut end = text.len().min(NAME_LIMIT);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

/// A random word from the process's randomly keyed hasher; ids and nonces
/// need only be unlikely to repeat, not reproducible.
fn random_u32() -> u32 {
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u8(0);
    hasher.finish() as u32
}

/// A nonzero player id no other player holds (DirectPlay's ids are nonzero).
fn random_id(taken: &[u32]) -> u32 {
    loop {
        let id = random_u32();
        if id != 0 && !taken.contains(&id) {
            return id;
        }
    }
}

#[cfg(test)]
#[path = "udp_tests.rs"]
mod tests;
