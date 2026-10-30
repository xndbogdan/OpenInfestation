//! `00538320`, the game protocol a session installs on channel 1 before it
//! hosts or joins (`0045F327`, `0045F8A5`): `+0` reads a frame's header
//! (`0045C3D0`), `+4` writes one (`0045C3A0`), `+8` is 0, so the receive
//! thread leaves its frames for the frame's drain (`00492830`), and `+C`/`+10`
//! are the 45 messages of `00538050`.
//!
//! A sender hands `00492500` an argument its encoder reads; the port names it
//! a [`GameSend`]. A receiver's own sends (`00492500` inside the drain) wait
//! until the drain ends, which keeps their order in the frame.

use thiserror::Error;

use super::encoders;
use super::game::{GameHeaderHost, GameNetError, GameNetErrorOr, GameNetState};
use super::receivers::{receive_message, GameCall, GameMessageHost, ReceiveError, SendArgument};
use super::transport::{NetProtocol, NetTransport, ProtocolRow, TransportError, TransportState};

/// The channel the game protocol takes.
pub const GAME_CHANNEL: usize = 1;

/// `00538050`'s payload sizes, by message id.
const SIZES: [u32; 45] = [
    0, 8, 4, 4, 4, 4, 4, 4, 8, 4, 8, 8, 12, 4, 4, 4, 4, 4, 8, 4, 4, 4, 16, 16, 20, 4, 4, 4, 40, 4,
    32, 64, 4, 8, 12, 16, 32, 4, 12, 4, 4, 4, 8, 8, 10,
];

/// What a message's encoder reads: `00492500`'s argument and the state
/// behind it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameSend {
    /// Encoders that read only the protocol's state: 0 (nothing), 26 (this
    /// machine's kill word) and 28 (the host's game state).
    State,
    /// A DWORD: the handle of 2 to 7, 20, 21 and 25, a slot (29) or a value
    /// (27, 37).
    Dword(u32),
    /// 1: the actor a handle names (`0045DBD0`, `0047E330`).
    Actor(u32),
    /// 8 to 11: two handles (`0045ED70`).
    Pair([u32; 2]),
    /// 18: damage to another machine's actor (`0045DF70`): the packet's
    /// DWORD, the target and the source.
    Damage([u32; 3]),
    /// 30: a slot's details and name (`0045D100`).
    Details(u8),
    /// 31: a text (`0045D0B0`).
    Text(Vec<u8>),
    /// 36: the current world's four values (`0048EA10`).
    WorldValues,
    /// 38: the territory list's bytes (`00475B00`).
    Territory,
    /// 39 and 40: a territory's number and a slot (`00475BB0`, `00475C40`).
    Territory2([u8; 2]),
    /// 12: a banner's capture (`0045BC60`): its handle, `+140` and `+13C`.
    Capture([u32; 3]),
    /// 23 and 24: a particle's creation (`0045E960`, `0045EA60`): its
    /// position and velocity, factory and selector, and for 24 its source.
    Points {
        points: [[u8; 6]; 2],
        kind: u32,
        extra: Option<u32>,
    },
    /// 19: a blast that reached another machine's actors (`0045DFF0`): its
    /// source, position and block.
    Effect {
        value: u32,
        point: [u8; 6],
        record: [u8; 10],
    },
    /// 13 to 17: a banner's event (`0045BD70`): its handle and `+144`.
    Banner([u32; 2]),
    /// 41: a territory's holder, previous holder, scorer and amount
    /// (`00475DB0`).
    Territory4([u8; 4]),
    /// 42: the race's records (`0046F410`).
    Race,
    /// 43 and 44: Powerball's ball let go (`00403D60`, `00403DF0`): its
    /// holder `+14C`, the throw's sequence `+17C` and, for 44, the throw
    /// `+176..+17A`.
    Ball {
        holder: u32,
        sequence: u16,
        throw: Option<[i16; 3]>,
    },
}

#[derive(Debug, Error)]
pub enum GameProtocolError<H> {
    #[error("message {id} cannot carry {send:?}")]
    Argument { id: u32, send: GameSend },
    #[error("message {0}'s receiver is not ported")]
    Unported(u32),
    #[error(transparent)]
    Game(#[from] GameNetError),
    #[error("message {id}: {error}")]
    Receive { id: u32, error: String },
    #[error("game protocol host: {0:?}")]
    Host(H),
    #[error(transparent)]
    Transport(#[from] TransportError),
}

/// The world behind the protocol: the header's settings and mission, the
/// receivers' routines, and the messages that read live objects.
pub trait GameWorldHost:
    GameMessageHost + GameHeaderHost<Error = <Self as GameMessageHost>::Error>
{
    /// `0054E894`, the process NULL handle.
    fn null_handle(&self) -> u32;
    /// `0047E330` for the actor `handle` names: its record into `out`,
    /// whose room is `out.len()`; its length, or -1 when it does not fit.
    fn encode_actor(
        &mut self,
        handle: u32,
        out: &mut [u8],
    ) -> Result<i32, <Self as GameMessageHost>::Error>;
    /// `0047E600`: a received record, the bytes it used; `sends` takes the
    /// messages it answers with.
    fn receive_actor(
        &mut self,
        payload: &[u8],
        sends: &mut Vec<(u32, GameSend)>,
    ) -> Result<i32, <Self as GameMessageHost>::Error>;
    /// `0048EA10`: the current world's four DWORDs of the mission's `+B0`.
    fn world_values(&mut self) -> Result<[u32; 4], <Self as GameMessageHost>::Error>;
    /// `0055EF10`: each territory entry's byte `+90`, in list order.
    fn territory(&mut self) -> Result<Vec<u8>, <Self as GameMessageHost>::Error>;
    /// `0046F410`: the race's records into `out`, whose room is
    /// `out.len()`; their length, or -1.
    fn encode_race(&mut self, out: &mut [u8]) -> Result<i32, <Self as GameMessageHost>::Error>;
    /// `0046F5E0`: the host's race records, the bytes they used.
    fn receive_race(&mut self, payload: &[u8]) -> Result<i32, <Self as GameMessageHost>::Error>;
    /// Messages 43 and 44 (`00403DA0`, `00403E50`) over their whole
    /// `payload`: Powerball's ball dropped or thrown by its holder.
    fn receive_ball(
        &mut self,
        id: u32,
        payload: &[u8],
    ) -> Result<(), <Self as GameMessageHost>::Error>;
    /// The messages world code sent (`00492500`) while a receiver ran,
    /// which wait for the drain's end with the receiver's own.
    fn take_world_sends(&mut self) -> Vec<(u32, GameSend)> {
        Vec::new()
    }
    /// Messages 38 to 41 (`00475B50`, `00475BD0`, `00475C60`, `00475DF0`)
    /// over their whole `payload`: the holders, a guest's claim, the host's
    /// grant and a score's line. `sends` takes the messages they answer
    /// with.
    fn receive_territory(
        &mut self,
        id: u32,
        payload: &[u8],
        state: &mut GameNetState,
        sends: &mut Vec<(u32, GameSend)>,
    ) -> Result<(), <Self as GameMessageHost>::Error>;
}

/// The protocol object over the session's game state and its world.
pub struct GameProtocol<'a, H: GameWorldHost> {
    rows: [ProtocolRow; 45],
    pub state: &'a mut GameNetState,
    pub host: &'a mut H,
    pending: Option<GameSend>,
    /// Sends asked for inside a drain, made once it ends.
    deferred: Vec<(u32, GameSend)>,
    error: Option<GameProtocolError<<H as GameMessageHost>::Error>>,
}

/// A receiver's host: sends wait in `deferred`, the rest reaches the world.
struct Deferring<'h, H> {
    host: &'h mut H,
    deferred: &'h mut Vec<(u32, GameSend)>,
}

impl<H: GameMessageHost> GameMessageHost for Deferring<'_, H> {
    type Error = H::Error;
    fn call(&mut self, call: GameCall, state: &mut GameNetState) -> Result<u32, Self::Error> {
        match call {
            GameCall::Send { id, argument } => {
                let send = match argument {
                    SendArgument::Value(value) => GameSend::Dword(value),
                    SendArgument::Name(slot) => GameSend::Details(slot),
                };
                self.deferred.push((id, send));
                Ok(0)
            }
            other => self.host.call(other, state),
        }
    }
    fn flags(&mut self, object: u32) -> Result<u32, Self::Error> {
        self.host.flags(object)
    }
    fn word_70(&mut self, object: u32) -> Result<u32, Self::Error> {
        self.host.word_70(object)
    }
    fn handle_84(&mut self, object: u32) -> Result<u32, Self::Error> {
        self.host.handle_84(object)
    }
}

impl<'a, H: GameWorldHost> GameProtocol<'a, H>
where
    <H as GameMessageHost>::Error: std::fmt::Debug,
{
    /// `004923D0` raises every payload size below 4 to 4 when the session
    /// starts with this protocol installed.
    pub fn new(state: &'a mut GameNetState, host: &'a mut H) -> Self {
        Self {
            rows: std::array::from_fn(|id| ProtocolRow {
                id: id as u32,
                size: SIZES[id].max(4),
            }),
            state,
            host,
            pending: None,
            deferred: Vec::new(),
            error: None,
        }
    }

    /// `00492500(1, id, argument)`.
    pub fn send(
        &mut self,
        transport: &mut NetTransport,
        id: u32,
        send: GameSend,
    ) -> Result<(), GameProtocolError<<H as GameMessageHost>::Error>> {
        self.pending = Some(send);
        let result = transport.send(GAME_CHANNEL, self, id, 0);
        self.pending = None;
        self.take_error()?;
        Ok(result?)
    }

    /// `00492830`'s game frames: each queued frame to its receivers, then
    /// the sends they asked for.
    pub fn drain(
        &mut self,
        transport: &mut NetTransport,
        table: &mut dyn NetProtocol,
    ) -> Result<(), GameProtocolError<<H as GameMessageHost>::Error>> {
        let result = transport.drain_received([table, self]);
        self.take_error()?;
        result?;
        for (id, send) in std::mem::take(&mut self.deferred) {
            self.send(transport, id, send)?;
        }
        Ok(())
    }

    fn take_error(&mut self) -> Result<(), GameProtocolError<<H as GameMessageHost>::Error>> {
        self.error.take().map_or(Ok(()), Err)
    }

    fn fail(&mut self, error: GameProtocolError<<H as GameMessageHost>::Error>) -> TransportError {
        let text = error.to_string();
        self.error = Some(error);
        TransportError::Protocol(text)
    }

    fn encode_send(
        &mut self,
        id: u32,
        send: &GameSend,
        out: &mut [u8],
    ) -> Result<i32, GameProtocolError<<H as GameMessageHost>::Error>> {
        let room = out.len() as u32;
        let host = |error| GameProtocolError::Host(error);
        Ok(match (id, send) {
            (0, GameSend::State) => encoders::encode_nothing(),
            (26, GameSend::State) => encoders::encode_dword(out, room, self.state.kill_word),
            (28, GameSend::State) => encoders::encode_game_state(out, room, self.state)?,
            (2..=7 | 20 | 21 | 25, GameSend::Dword(handle)) => {
                encoders::encode_handle(out, *handle)
            }
            (27 | 29 | 37, GameSend::Dword(value)) => encoders::encode_dword(out, room, *value),
            (30, GameSend::Details(slot)) => encoders::encode_details(out, room, self.state, *slot),
            (31, GameSend::Text(text)) => encoders::encode_text(out, room, text),
            (1, GameSend::Actor(handle)) => self.host.encode_actor(*handle, out).map_err(host)?,
            (8..=11, GameSend::Pair(pair)) => encoders::encode_pair(out, room, *pair),
            (18, GameSend::Damage(damage)) => encoders::encode_damage(out, room, *damage),
            (36, GameSend::WorldValues) => {
                let values = self.host.world_values().map_err(host)?;
                encoders::encode_world_values(out, room, values)
            }
            // 00475B00: the count, then each entry's byte.
            (38, GameSend::Territory) => {
                let bytes = self.host.territory().map_err(host)?;
                if bytes.len() + 1 > out.len() {
                    -1
                } else {
                    out[0] = bytes.len() as u8;
                    out[1..=bytes.len()].copy_from_slice(&bytes);
                    bytes.len() as i32 + 1
                }
            }
            (39 | 40, GameSend::Territory2(bytes)) => encoders::encode_bytes(out, bytes),
            (12, GameSend::Capture(words)) => encoders::encode_capture(out, room, *words),
            (
                23 | 24,
                GameSend::Points {
                    points,
                    kind,
                    extra,
                },
            ) => encoders::encode_points(out, room, *points, *kind, *extra),
            (
                19,
                GameSend::Effect {
                    value,
                    point,
                    record,
                },
            ) => encoders::encode_effect(out, room, *value, *point, *record),
            (13..=17, GameSend::Banner(words)) => encoders::encode_banner(out, room, *words),
            (41, GameSend::Territory4(bytes)) => encoders::encode_bytes(out, bytes),
            (42, GameSend::Race) => self.host.encode_race(out).map_err(host)?,
            (
                43 | 44,
                GameSend::Ball {
                    holder,
                    sequence,
                    throw,
                },
            ) if throw.is_some() == (id == 44) => {
                encoders::encode_ball(out, room, *holder, *sequence, *throw)
            }
            _ => {
                return Err(GameProtocolError::Argument {
                    id,
                    send: send.clone(),
                })
            }
        })
    }
}

impl<H: GameWorldHost> NetProtocol for GameProtocol<'_, H>
where
    <H as GameMessageHost>::Error: std::fmt::Debug,
{
    fn receives(&self) -> bool {
        false
    }
    fn message_count(&self) -> u32 {
        SIZES.len() as u32
    }
    fn rows_mut(&mut self) -> &mut [ProtocolRow] {
        &mut self.rows
    }
    fn write_header(&mut self, _: &mut TransportState, out: &mut [u8]) -> Option<usize> {
        Some(self.state.write_header(out))
    }
    fn read_header(
        &mut self,
        _: &mut TransportState,
        header: &[u8],
    ) -> Result<Option<(usize, bool)>, TransportError> {
        match self.state.read_header(header, self.host) {
            Ok(proceed) => Ok(Some((16, proceed))),
            Err(GameNetErrorOr::Game(error)) => Err(self.fail(error.into())),
            Err(GameNetErrorOr::Host(error)) => Err(self.fail(GameProtocolError::Host(error))),
        }
    }
    fn encode(&mut self, _: &TransportState, id: u32, _: u32, out: &mut [u8]) -> i32 {
        let Some(send) = self.pending.take() else {
            self.error = Some(GameProtocolError::Argument {
                id,
                send: GameSend::State,
            });
            return 0;
        };
        let result = self.encode_send(id, &send, out);
        self.pending = Some(send);
        match result {
            Ok(length) => length,
            Err(error) => {
                self.error = Some(error);
                0
            }
        }
    }
    fn receive(
        &mut self,
        state: &mut TransportState,
        id: u32,
        payload: &[u8],
    ) -> Result<i32, TransportError> {
        match id {
            // 0045DC00, message 1's receiver, jumps to 0047E600.
            1 => {
                let mut sends = Vec::new();
                let used = self.host.receive_actor(payload, &mut sends);
                self.deferred.append(&mut sends);
                used.map_err(|error| self.fail(GameProtocolError::Host(error)))
            }
            // 38 uses its count and the bytes after it, 39 and 40 two bytes,
            // 41 four.
            38..=41 => {
                let length = match id {
                    38 => payload.first().map_or(1, |&count| usize::from(count) + 1),
                    41 => 4,
                    _ => 2,
                };
                let payload = payload.get(..length).ok_or(TransportError::FrameOverrun {
                    offset: 0,
                    length: payload.len(),
                })?;
                let mut sends = Vec::new();
                let result = self
                    .host
                    .receive_territory(id, payload, self.state, &mut sends);
                self.deferred.append(&mut sends);
                match result {
                    Ok(()) => Ok(length as i32),
                    Err(error) => Err(self.fail(GameProtocolError::Host(error))),
                }
            }
            42 => {
                let used = self.host.receive_race(payload);
                used.map_err(|error| self.fail(GameProtocolError::Host(error)))
            }
            // No machine sends 22 (no send names 0x16), so its receiver
            // 0045E620 is left out.
            22 => Err(self.fail(GameProtocolError::Unported(id))),
            // Powerball's ball let go on another machine (00403DA0,
            // 00403E50): the holder, the sequence and, for 44, the throw.
            43 | 44 => {
                let length = if id == 43 { 6 } else { 12 };
                let payload = payload.get(..length).ok_or(TransportError::FrameOverrun {
                    offset: 0,
                    length: payload.len(),
                })?;
                let result = self.host.receive_ball(id, payload);
                let mut world = self.host.take_world_sends();
                self.deferred.append(&mut world);
                match result {
                    Ok(()) => Ok(length as i32),
                    Err(error) => Err(self.fail(GameProtocolError::Host(error))),
                }
            }
            _ => {
                let sender = (self.state.header_in[3] >> 16) as u8;
                let null = self.host.null_handle();
                let mut host = Deferring {
                    host: &mut *self.host,
                    deferred: &mut self.deferred,
                };
                let received = receive_message(
                    id,
                    payload,
                    sender,
                    state.local_slot,
                    null,
                    self.state,
                    &mut host,
                );
                let mut world = self.host.take_world_sends();
                self.deferred.append(&mut world);
                match received {
                    Ok(used) => Ok(used),
                    Err(ReceiveError::Host(error)) => {
                        Err(self.fail(GameProtocolError::Host(error)))
                    }
                    Err(ReceiveError::Unported(id)) => {
                        Err(self.fail(GameProtocolError::Unported(id)))
                    }
                    Err(error) => {
                        let error = GameProtocolError::Receive {
                            id,
                            error: error.to_string(),
                        };
                        Err(self.fail(error))
                    }
                }
            }
        }
    }
}
