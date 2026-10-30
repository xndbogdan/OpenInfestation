//! The game protocol's receivers (rows `+0` of `00538050`), but for the
//! actor state (1), the furniture (22) and the arena classes (38 to 44).
//! Each decodes its payload, reads the fields retail reads, keeps what
//! belongs to the protocol's state, and reports every game routine it calls
//! to the host as a [`GameCall`]; the host answers resolutions and queries.

use thiserror::Error;

use super::game::{GameNetError, GameNetState};

/// Where a hold call's third argument points: a stack slot (`0045EB30`) or
/// the actor's `+D4` (`0045EBA0`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoldOut {
    Stack,
    FieldD4,
}

/// The argument of a message a receiver sends back (`00492500` on channel 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendArgument {
    Value(u32),
    /// `0054E5B0 + slot * 16`, the slot's name.
    Name(u8),
}

/// The routines the receivers call; objects are the host's ids (0 none).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameCall {
    /// `0047AD90`: the object a handle names.
    Resolve {
        handle: u32,
    },
    /// `00479FA0`.
    Dispose {
        object: u32,
    },
    /// `0047D670(0, process NULL, state)`.
    SetState {
        object: u32,
        null: u32,
        state: u32,
    },
    /// `00492500(1, id, argument)`.
    Send {
        id: u32,
        argument: SendArgument,
    },
    /// `00462480`, `00462510`, `00462560`: the slot tables' handle.
    SlotUnknown {
        handle: u32,
    },
    SlotObject {
        handle: u32,
    },
    SlotFree {
        handle: u32,
    },
    /// Virtual `+8C(take, other, out)`.
    Hold {
        object: u32,
        take: bool,
        other: u32,
        out: HoldOut,
    },
    /// Virtual `+BC`, the object's holding component.
    Component {
        object: u32,
    },
    /// The component's `+C(handle, held)` and `+4(holder, held)`.
    ComponentTake {
        component: u32,
        handle: u32,
        held: u32,
    },
    ComponentDrop {
        component: u32,
        holder: u32,
        held: u32,
    },
    /// `00479DD0`.
    Ground {
        object: u32,
    },
    /// Virtual `+B8`, the banner an object names.
    BannerOf {
        object: u32,
    },
    /// `0045B9E0`, `0045BA50` and `0045BA90` on a banner.
    BannerTaken {
        banner: u32,
        by: u32,
    },
    BannerDropped {
        banner: u32,
        by: u32,
    },
    BannerCaptured {
        banner: u32,
        values: [u32; 2],
    },
    /// `00488AA0`: whether a first player exists.
    FirstPlayer,
    /// `0046CC50(banner, event, value)` on the first player.
    BannerEvent {
        banner: u32,
        event: u32,
        value: u32,
    },
    /// `0047C6E0(&record, amount)`.
    Damage {
        object: u32,
        record: u32,
        amount: u32,
    },
    /// `004887D0(&record, &point, value, 0)` on G.
    WorldEffect {
        record: [u8; 10],
        point: [u8; 6],
        value: u32,
    },
    /// `0048D980` and `00431EF0` on an object.
    SonicPulse {
        object: u32,
    },
    Effect431ef0 {
        object: u32,
    },
    /// `004665D0` (kind 0) or `00466710` (kind 1): ECX the argument, EDX the
    /// first point; the second point, the extra (or the process NULL), 1.
    PointEffect {
        kind: u8,
        argument: u32,
        points: [[u8; 6]; 2],
        extra: Option<u32>,
    },
    /// `004135D0(text, 0)` on the first player's `+88`.
    PlayerText {
        text: Vec<u8>,
    },
    /// `00487EC0(id, &position; b, a, 0)`; its answer goes to `004923C0`.
    Sound {
        id: u32,
        position: [u16; 3],
        values: [u32; 2],
    },
    ErrorDispose {
        error: u32,
    },
    /// `0048EA30(&words, 16)` and `0048E980(bit)` on the mission.
    WorldWords {
        words: [u32; 4],
    },
    WorldBit {
        bit: u32,
    },
}

pub trait GameMessageHost {
    type Error;
    /// The world routine `call` names. The protocol's state is lent: a
    /// routine can reach a notice that keeps it (`0045D8F0`, `0045DA50`).
    fn call(&mut self, call: GameCall, state: &mut GameNetState) -> Result<u32, Self::Error>;
    /// An object's `+2C` flags, `+70` and `+84`.
    fn flags(&mut self, object: u32) -> Result<u32, Self::Error>;
    fn word_70(&mut self, object: u32) -> Result<u32, Self::Error>;
    fn handle_84(&mut self, object: u32) -> Result<u32, Self::Error>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ReceiveError<H> {
    #[error("message {id}'s payload ends before offset {offset}")]
    Short { id: u32, offset: usize },
    #[error("message {0} is not one of these receivers")]
    Unported(u32),
    #[error(transparent)]
    Game(#[from] GameNetError),
    #[error("game message host: {0:?}")]
    Host(H),
}

/// `+2C` bit 31: another machine's copy.
const REMOTE: u32 = 0x8000_0000;
/// `+2C` bit 7: held.
const HELD: u32 = 0x80;

struct Payload<'a> {
    id: u32,
    bytes: &'a [u8],
}

impl Payload<'_> {
    fn take<const N: usize, H>(&self, offset: usize) -> Result<[u8; N], ReceiveError<H>> {
        self.bytes
            .get(offset..offset + N)
            .map(|b| b.try_into().unwrap())
            .ok_or(ReceiveError::Short {
                id: self.id,
                offset: offset + N,
            })
    }
    fn dword<H>(&self, index: usize) -> Result<u32, ReceiveError<H>> {
        Ok(u32::from_le_bytes(self.take(index * 4)?))
    }
}

/// The receiver of message `id`: the bytes it used. `sender` is the frame
/// header's byte `+E`.
#[allow(clippy::too_many_arguments)]
pub fn receive_message<H: GameMessageHost>(
    id: u32,
    payload: &[u8],
    sender: u8,
    local_slot: u8,
    null: u32,
    state: &mut GameNetState,
    host: &mut H,
) -> Result<i32, ReceiveError<H::Error>> {
    let p = Payload { id, bytes: payload };
    let call = |host: &mut H, state: &mut GameNetState, call| {
        host.call(call, state).map_err(ReceiveError::Host)
    };
    let resolve = |host: &mut H, state: &mut GameNetState, handle| {
        host.call(GameCall::Resolve { handle }, state)
            .map_err(ReceiveError::Host)
    };
    let local = |host: &mut H, object| {
        host.flags(object)
            .map(|flags| flags & REMOTE == 0)
            .map_err(ReceiveError::Host)
    };
    let used = match id {
        // 0048C860: nothing, and nothing used.
        0 => 0,
        // 0045DE90: dispose of it.
        2 => {
            let object = resolve(host, state, p.dword(0)?)?;
            if object != 0 {
                call(host, state, GameCall::Dispose { object })?;
            }
            4
        }
        // 0045DDD0, 0045DE00, 0045DE30, 0045DE60: a state, then disposal.
        3..=6 => {
            let object = resolve(host, state, p.dword(0)?)?;
            if object != 0 {
                let lifecycle = [4, 3, 1, 0][id as usize - 3];
                call(
                    host,
                    state,
                    GameCall::SetState {
                        object,
                        null,
                        state: lifecycle,
                    },
                )?;
                call(host, state, GameCall::Dispose { object })?;
            }
            4
        }
        // 0045DEB0: a handle another slot gives up. Its owner (the top five
        // bits) announces it: a local actor's +84 as message 7, else
        // message 2. Elsewhere an unknown handle the sender owns frees its
        // slot entry.
        7 => {
            let handle = p.dword(0)?;
            let object = resolve(host, state, handle)?;
            let owner = (handle >> 27) as u8;
            if local_slot == owner {
                if object != 0 && local(host, object)? {
                    let argument = host.handle_84(object).map_err(ReceiveError::Host)?;
                    call(
                        host,
                        state,
                        GameCall::Send {
                            id: 7,
                            argument: SendArgument::Value(argument),
                        },
                    )?;
                } else {
                    call(
                        host,
                        state,
                        GameCall::Send {
                            id: 2,
                            argument: SendArgument::Value(handle),
                        },
                    )?;
                }
            } else if object == 0
                && sender != local_slot
                && sender == owner
                && call(host, state, GameCall::SlotUnknown { handle })? & 0xff == 0
            {
                let entry = call(host, state, GameCall::SlotObject { handle })?;
                if entry != 0 {
                    call(host, state, GameCall::Dispose { object: entry })?;
                }
                call(host, state, GameCall::SlotFree { handle })?;
            }
            4
        }
        // 0045EB30: A takes B.
        8 => {
            let (a, b) = (p.dword(0)?, p.dword(1)?);
            let holder = resolve(host, state, a)?;
            if holder != 0 && resolve(host, state, b)? != 0 {
                let flags = host.flags(holder).map_err(ReceiveError::Host)?;
                if flags & HELD == 0 {
                    if flags & REMOTE == 0 {
                        call(
                            host,
                            state,
                            GameCall::Hold {
                                object: holder,
                                take: true,
                                other: b,
                                out: HoldOut::Stack,
                            },
                        )?;
                    }
                    let held = resolve(host, state, b)?;
                    if held != 0 {
                        let component = call(host, state, GameCall::Component { object: held })?;
                        if component != 0 {
                            call(
                                host,
                                state,
                                GameCall::ComponentTake {
                                    component,
                                    handle: b,
                                    held: holder,
                                },
                            )?;
                        }
                    }
                }
            }
            8
        }
        // 0045EBA0: A, held by B, is let go.
        9 => {
            let (a, b) = (p.dword(0)?, p.dword(1)?);
            let held = resolve(host, state, a)?;
            if held != 0
                && host.flags(held).map_err(ReceiveError::Host)? & HELD != 0
                && host.word_70(held).map_err(ReceiveError::Host)? == b
            {
                let holder = resolve(host, state, b)?;
                let component = if holder != 0 {
                    call(host, state, GameCall::Component { object: holder })?
                } else {
                    0
                };
                if component != 0 {
                    call(
                        host,
                        state,
                        GameCall::ComponentDrop {
                            component,
                            holder,
                            held,
                        },
                    )?;
                } else {
                    call(host, state, GameCall::Ground { object: held })?;
                }
                if local(host, held)? {
                    call(
                        host,
                        state,
                        GameCall::Hold {
                            object: held,
                            take: false,
                            other: null,
                            out: HoldOut::FieldD4,
                        },
                    )?;
                }
            }
            8
        }
        // 0045ED30, 0045EDA0, 0045BB90: a local banner taken, dropped or
        // captured.
        10..=12 => {
            let object = resolve(host, state, p.dword(0)?)?;
            if object != 0 {
                let banner = call(host, state, GameCall::BannerOf { object })?;
                if banner != 0 && local(host, banner)? {
                    let by = p.dword(1)?;
                    call(
                        host,
                        state,
                        match id {
                            10 => GameCall::BannerTaken { banner, by },
                            11 => GameCall::BannerDropped { banner, by },
                            _ => GameCall::BannerCaptured {
                                banner,
                                values: [by, p.dword(2)?],
                            },
                        },
                    )?;
                }
            }
            if id == 12 {
                12
            } else {
                8
            }
        }
        // 0045BC10, 0045BC20, 0045BC30, 0045BC40 and 0045BC50 (events 2, 0,
        // 1, 3 and 4) through 0045BBD0: a banner event.
        13..=17 => {
            let event = [2, 0, 1, 3, 4][id as usize - 13];
            let (handle, value) = (p.dword(0)?, p.dword(1)?);
            let object = resolve(host, state, handle)?;
            if object != 0 {
                let banner = call(host, state, GameCall::BannerOf { object })?;
                if banner != 0 {
                    call(host, state, GameCall::FirstPlayer)?;
                    call(
                        host,
                        state,
                        GameCall::BannerEvent {
                            banner,
                            event,
                            value,
                        },
                    )?;
                }
            }
            8
        }
        // 0045DFB0: damage to a local actor.
        18 => {
            let (record, handle, amount) = (p.dword(0)?, p.dword(1)?, p.dword(2)?);
            let object = resolve(host, state, handle)?;
            if object != 0 && local(host, object)? {
                call(
                    host,
                    state,
                    GameCall::Damage {
                        object,
                        record,
                        amount,
                    },
                )?;
            }
            12
        }
        // 0045E050: a world effect.
        19 => {
            call(
                host,
                state,
                GameCall::WorldEffect {
                    record: p.take(10)?,
                    point: p.take(4)?,
                    value: p.dword(0)?,
                },
            )?;
            20
        }
        // 0045EDE0 and 0045EE00.
        20 | 25 => {
            let object = resolve(host, state, p.dword(0)?)?;
            if object != 0 {
                call(
                    host,
                    state,
                    if id == 20 {
                        GameCall::SonicPulse { object }
                    } else {
                        GameCall::Effect431ef0 { object }
                    },
                )?;
            }
            4
        }
        // 0045E0E0: nothing.
        21 => 4,
        // 0045E9C0 and 0045EAD0 through 0045EA20: an effect between two
        // points, by the kind in the high WORD.
        23 | 24 => {
            let points = [p.take::<6, _>(0)?, p.take::<6, _>(6)?];
            let word = p.dword(3)?;
            let extra = if id == 24 { Some(p.dword(4)?) } else { None };
            let kind = word >> 16;
            if kind < 2 {
                call(
                    host,
                    state,
                    GameCall::PointEffect {
                        kind: kind as u8,
                        argument: word & 0xffff,
                        points,
                        extra,
                    },
                )?;
            }
            if id == 24 {
                20
            } else {
                16
            }
        }
        // 0045D300, on the host: a changed kill word is a death of the
        // sender's slot by the slot in its high WORD.
        26 => {
            let word = p.dword(0)?;
            let slot = usize::from(sender);
            let known = *state.kill_words.get(slot).ok_or(GameNetError::Slot(slot))?;
            if state.host && known as u16 != word as u16 {
                state.kill_words[slot] = word;
                state.record_kill(i32::from(sender), (word >> 16) as i32, state.game_mode)?;
            }
            4
        }
        // 0045D4A0, on the host: a score transfer.
        27 => {
            let transfer = p.take::<4, _>(0)?;
            if state.host {
                state.transfer_score(transfer);
            }
            4
        }
        // 0045D1E0: the host's game state.
        28 => state.receive_game_state(payload)?,
        // 0045D050: a request for this slot's name, answered with it.
        29 => {
            let slot = p.dword(0)?;
            if slot == u32::from(local_slot) && state.present[usize::from(local_slot)] != 0 {
                call(
                    host,
                    state,
                    GameCall::Send {
                        id: 30,
                        argument: SendArgument::Name(local_slot),
                    },
                )?;
            }
            4
        }
        // 0045D160: the sender's details and name; its slot is present.
        30 => {
            let slot = usize::from(sender);
            let details = p.take::<6, _>(0)?;
            let name = p.take::<16, _>(6)?;
            *state
                .details
                .get_mut(slot)
                .ok_or(GameNetError::Slot(slot))? = details;
            state.names[slot] = name;
            state.present[slot] = 1;
            0x20
        }
        // 0045D1B0: a text for the first player's message.
        31 => {
            let length = i32::from(p.take::<1, _>(0)?[0]) + 1;
            if call(host, state, GameCall::FirstPlayer)? != 0 {
                let text = payload
                    .get(1..)
                    .and_then(|rest| rest.split(|&b| b == 0).next().map(<[u8]>::to_vec))
                    .ok_or(ReceiveError::Short { id, offset: 1 })?;
                call(host, state, GameCall::PlayerText { text })?;
            }
            length
        }
        // 0045DB10, 0045DBA0, 0045DBB0 and 0045DBC0 (kinds 0 to 3) through
        // 0045DB20: a sound, its flags the id
        // less 32: bit 0 a first value, bit 1 a second (each else 0x10000).
        32..=35 => {
            let word = p.dword(0)?;
            let kind = id - 32;
            let mut values = [0x10000, 0x10000];
            let mut used = 4;
            for (bit, slot) in [(1, 1), (2, 0)] {
                if kind & bit != 0 {
                    values[slot] = p.dword(used / 4)?;
                    used += 4;
                }
            }
            let position =
                [word & 0xff00, (word >> 8) & 0xff00, (word >> 16) & 0xff00].map(|v| v as u16);
            let error = call(
                host,
                state,
                GameCall::Sound {
                    id: word & 0xff,
                    position,
                    values,
                },
            )?;
            call(host, state, GameCall::ErrorDispose { error })?;
            used as i32
        }
        // 0045EC20 and 0045EC70: the current world's words, and one bit.
        36 => {
            let words = [p.dword(0)?, p.dword(1)?, p.dword(2)?, p.dword(3)?];
            call(host, state, GameCall::WorldWords { words })?;
            16
        }
        37 => {
            call(host, state, GameCall::WorldBit { bit: p.dword(0)? })?;
            4
        }
        other => return Err(ReceiveError::Unported(other)),
    };
    Ok(used)
}

#[cfg(test)]
#[path = "receivers_tests.rs"]
mod tests;
