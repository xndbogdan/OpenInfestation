//! Selector 46, an arena's territory (Buggy Madness and Buggy Platforms):
//! `00475610` over the rotating visual `0047A490` (final table `005278A0`).
//! The constructor makes its score 16 (`+6C`) and its period 5 seconds
//! (`+70`), numbers it (`+78`, one more than the count `0055EF28`) and
//! appends it to the territory list (`0055EF10`, its node at `+7C`); the
//! destructor `004756C0` counts it out and unlinks it. Its payload
//! (`004756F0`: `00475710` after `+08`) and commands (`00475CB0`) set the
//! score (four times the value) and the period (seconds).
//!
//! Its update `00475800` (also its `+3C`, through `00461C50`) charges it
//! while it has a holder and drives the first player's vehicle through it:
//! passing it from one side to the other claims it, on the host at once
//! (`00475750`), on a guest by message 39. The host scores a holder each
//! period (`00475E70`, message 41) and frees it. Messages 38 to 41 carry
//! the holders, the claims and the scores' lines.

use thiserror::Error;

use super::{ObjectArena, ObjectClass, ObjectError, ObjectId, RotatingVisual};
use crate::network::game::GameNetState;
use crate::network::lines::format_line;
use crate::network::protocol::GameSend;
use inf_formats::{InstanceCommandPayload, Selector46ConstructorPayload};

/// `0053B458`: the charge a territory settles to without two players.
const SETTLED_CHARGE: i32 = 0x8000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selector46State {
    pub rotating: RotatingVisual,
    /// +6C: what a holder scores for a whole period (four times the
    /// payload's value).
    pub score_6c: i32,
    /// +70: the period in microseconds.
    pub period_70: u32,
    /// +74: how long the holder has held it; bit 31 once the game ended.
    pub held_74: u32,
    /// +78: its number in the list, from 1.
    pub index_78: u32,
    /// +88: its charge, which rises while held (to 0xFFFF).
    pub charge_88: i32,
    /// +8C: a guest's claim waiting for the host (its slot).
    pub claim_8c: u8,
    /// +8D: the slot the host last gave it to.
    pub granted_8d: u8,
    /// +8E: its holder before the last change.
    pub previous_8e: u8,
    /// +8F: the holder its last score went to.
    pub scorer_8f: u8,
    /// +90: its holder (0: none).
    pub owner_90: u8,
    /// +91 and +92: the slot last on each side of it.
    pub sides_91: [u8; 2],
    /// +94: the last score it gave.
    pub amount_94: u32,
}

impl Selector46State {
    pub(super) fn new(index: u32) -> Self {
        Self {
            rotating: RotatingVisual::new(),
            score_6c: 0x10,
            period_70: 5_000_000,
            held_74: 0,
            index_78: index,
            charge_88: 0,
            claim_8c: 0,
            granted_8d: 0,
            previous_8e: 0,
            scorer_8f: 0,
            owner_90: 0,
            sides_91: [0; 2],
            amount_94: 0,
        }
    }
}

/// What a territory reads besides the arena and the protocol's state.
pub struct TerritoryRequest<'a> {
    /// The update's microseconds.
    pub delta: u32,
    /// `0055F2F4`.
    pub local_slot: u8,
    /// `0055F3A0`.
    pub host: bool,
    /// For its lines' H0 strings `0x72..0x76`.
    pub resident: &'a crate::ResidentRuntime,
}

#[derive(Debug, Error)]
pub enum TerritoryError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("territory {0:?} reaches the game protocol's state outside a network game")]
    NoNetworkGame(ObjectId),
    #[error("territory line: {0}")]
    Line(String),
    #[error("territory {0:?}: the first player: {1}")]
    Player(ObjectId, String),
    /// `0047595B` turns `0047A660`'s offset with no vehicle near when the
    /// extent is past 0x7FFF: stack the update never wrote.
    #[error("territory {0:?}'s extent passes 0x7FFF with no vehicle: an unwritten offset")]
    UnwrittenOffset(ObjectId),
}

/// `0054E3EC`'s byte for `slot`; slot 17 reads `0054E3FD`, past the
/// seventeen, which nothing writes.
fn present(game: &GameNetState, slot: usize) -> bool {
    game.present.get(slot).is_some_and(|&byte| byte != 0)
}

impl ObjectArena {
    pub fn selector46(&self, object: ObjectId) -> Result<&Selector46State, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::Selector46(state) => Ok(state),
            _ => Err(ObjectError::NotSelector46(object)),
        }
    }

    fn selector46_mut(&mut self, object: ObjectId) -> Result<&mut Selector46State, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::Selector46(state) => Ok(state),
            _ => Err(ObjectError::NotSelector46(object)),
        }
    }

    /// The territory list `0055EF10`, in link order.
    pub fn territories(&self) -> &[ObjectId] {
        &self.territories
    }

    /// `00475710` after the flags (`00479D70`).
    pub(super) fn initialize_selector46_payload(
        &mut self,
        object: ObjectId,
        payload: &Selector46ConstructorPayload,
    ) -> Result<(), ObjectError> {
        let state = self.selector46_mut(object)?;
        state.score_6c = payload.score.value.wrapping_shl(2);
        state.period_70 = (payload.seconds.value as u32).wrapping_mul(1_000_000);
        Ok(())
    }

    /// `00475CB0`'s codes 3 and 4.
    pub(super) fn apply_selector46_command(
        &mut self,
        object: ObjectId,
        payload: &InstanceCommandPayload,
    ) -> Result<bool, ObjectError> {
        let state = self.selector46_mut(object)?;
        match payload {
            InstanceCommandPayload::Selector46Score(value) => {
                state.score_6c = (value.value as i32).wrapping_shl(2)
            }
            InstanceCommandPayload::Selector46Seconds(value) => {
                state.period_70 = value.value.wrapping_mul(1_000_000)
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// `00475800`.
    pub fn tick_selector46(
        &mut self,
        object: ObjectId,
        request: &TerritoryRequest<'_>,
    ) -> Result<(), TerritoryError> {
        let mut hold = self
            .take_network_game()
            .ok_or(TerritoryError::NoNetworkGame(object))?;
        let result = self.tick_territory(object, request, &mut hold.game);
        self.lend_network_game(hold);
        result
    }

    fn tick_territory(
        &mut self,
        object: ObjectId,
        request: &TerritoryRequest<'_>,
        game: &mut GameNetState,
    ) -> Result<(), TerritoryError> {
        let step = request.delta >> 4;
        let players = (1..=17).filter(|&slot| present(game, slot)).count();
        let state = self.selector46_mut(object)?;
        let settle = if game.ended == 0 {
            if players < 2 {
                state.owner_90 = 0;
                state.held_74 = 0;
                true
            } else {
                let charge = state.charge_88;
                state.charge_88 = if state.owner_90 == 0 {
                    if (step as i32) < charge {
                        charge - step as i32
                    } else {
                        0
                    }
                } else if (step as i32).wrapping_add(charge) < 0xffff {
                    (step as i32).wrapping_add(charge)
                } else {
                    0xffff
                };
                false
            }
        } else {
            state.owner_90 = game.winner;
            state.held_74 |= 0x8000_0000;
            true
        };
        // 004758A2: the charge settles towards half.
        if settle && state.charge_88 != SETTLED_CHARGE {
            let charge = state.charge_88;
            state.charge_88 = if charge < SETTLED_CHARGE {
                if (step as i32).wrapping_add(charge) <= SETTLED_CHARGE {
                    (step as i32).wrapping_add(charge)
                } else {
                    SETTLED_CHARGE
                }
            } else if SETTLED_CHARGE <= charge.wrapping_sub(step as i32) {
                charge.wrapping_sub(step as i32)
            } else {
                SETTLED_CHARGE
            };
        }
        // 0047590E..004759D8: the first player's live vehicle within the
        // extent marks the side it is on; on both, it claims the territory.
        let mut offset = None;
        if players >= 2 {
            if let Some(vehicle) = self.first_player_live_unit(object)? {
                let own = self.position(object)?;
                let theirs = self.position(vehicle)?;
                let words: [i16; 3] =
                    std::array::from_fn(|axis| theirs[axis].wrapping_sub(own[axis]) as i16);
                let sum = words.iter().fold(0_i32, |sum, &word| {
                    sum.wrapping_add(i32::from(word).wrapping_mul(i32::from(word)))
                });
                let distance = if sum as u32 & 0x8000_0000 != 0 {
                    inf_formats::native_basis::isqrt_48efc0((sum as u32 >> 2) as i32) << 1
                } else {
                    inf_formats::native_basis::isqrt_48efc0(sum)
                };
                offset = Some((distance as i32, words));
            }
        }
        let extent = i32::from(self.object(object)?.base.word_28);
        if offset.is_none() && 0x7fff < extent {
            return Err(TerritoryError::UnwrittenOffset(object));
        }
        let mut keep_sides = false;
        if let Some((_, words)) = offset.filter(|&(distance, _)| distance < extent) {
            let local = self.selector46(object)?.rotating.basis.inverse_words(words);
            let state = self.selector46_mut(object)?;
            state.sides_91[usize::from(local[2] >= 1)] = request.local_slot;
            if state.sides_91[1] != state.sides_91[0] {
                keep_sides = true;
            } else if !request.host {
                state.claim_8c = request.local_slot;
                let claim = [state.index_78 as u8, state.claim_8c];
                self.queue_network_send(39, GameSend::Territory2(claim));
            } else {
                let slot = state.sides_91[1];
                self.capture_territory(object, slot, request, game)?;
            }
        }
        if !keep_sides {
            self.selector46_mut(object)?.sides_91 = [0; 2];
        }
        // 004759D8: the host scores a holder each period, then frees it.
        let state = *self.selector46(object)?;
        if request.host && game.ended == 0 && state.owner_90 != 0 {
            let held = state.held_74.wrapping_add(request.delta);
            self.selector46_mut(object)?.held_74 = held;
            if state.period_70 < held {
                if held & 0x8000_0000 == 0 {
                    let owner = usize::from(state.owner_90);
                    self.selector46_mut(object)?.scorer_8f = state.owner_90;
                    let mut score = state.score_6c;
                    if state.previous_8e != 0 {
                        score >>= 1;
                    }
                    let best = (1..=17)
                        .filter(|&slot| slot != owner && present(game, slot))
                        .map(|slot| u32::from(game.scores[slot][2]))
                        .max()
                        .unwrap_or(0);
                    let own = u32::from(game.scores[owner][2]);
                    let divisor = ((own + 1) * 5 / (best + 1) + 1).min(10) as i32;
                    let amount = score / divisor;
                    game.scores[owner][2] = game.scores[owner][2].wrapping_add(amount as u16);
                    self.territory_scored(object, amount as u32, request, game)?;
                    game.check_player_limit(state.owner_90);
                }
                let state = self.selector46_mut(object)?;
                state.previous_8e = 0;
                state.owner_90 = 0;
                state.held_74 = 0;
            }
        }
        Ok(())
    }

    /// `0046B030`: the first player's unit, when its lifecycle is 3.
    fn first_player_live_unit(
        &mut self,
        object: ObjectId,
    ) -> Result<Option<ObjectId>, TerritoryError> {
        let Some(player) = self.first_player() else {
            return Ok(None);
        };
        let handle = self
            .world_player(player)
            .map_err(|error| TerritoryError::Player(object, error.to_string()))?
            .handle_d4;
        let unit = self
            .resolve_actor_handle(handle)
            .map_err(|error| TerritoryError::Player(object, error.to_string()))?;
        let Some(unit) = unit else {
            return Ok(None);
        };
        Ok((self.actor(unit)?.word_c4 == 3).then_some(unit))
    }

    /// `00475750` on the host: a territory changing holders takes a quarter
    /// of its score from the old holder (or half from no one) for the new.
    pub(crate) fn capture_territory(
        &mut self,
        object: ObjectId,
        slot: u8,
        request: &TerritoryRequest<'_>,
        game: &mut GameNetState,
    ) -> Result<(), TerritoryError> {
        let state = *self.selector46(object)?;
        if state.owner_90 == slot {
            return Ok(());
        }
        {
            let state = self.selector46_mut(object)?;
            state.previous_8e = state.owner_90;
            state.scorer_8f = 0;
        }
        if game.ended != 0 {
            return Ok(());
        }
        let amount = if state.owner_90 == 0 {
            state.score_6c >> 1
        } else {
            let amount = state.score_6c >> 2;
            let score = &mut game.scores[usize::from(state.owner_90)][2];
            *score = if amount < i32::from(*score) {
                score.wrapping_sub(amount as u16)
            } else {
                0
            };
            amount
        };
        let score = &mut game.scores[usize::from(slot)][2];
        *score = score.wrapping_add(amount as u16);
        self.selector46_mut(object)?.owner_90 = slot;
        self.territory_scored(object, amount as u32, request, game)?;
        game.check_player_limit(slot);
        Ok(())
    }

    /// `00475E70`: the amount kept at `+94`, the line here (`00475E10`) and
    /// the same news to every machine (41).
    fn territory_scored(
        &mut self,
        object: ObjectId,
        amount: u32,
        request: &TerritoryRequest<'_>,
        game: &GameNetState,
    ) -> Result<(), TerritoryError> {
        let state = self.selector46_mut(object)?;
        state.amount_94 = amount;
        let news = [
            state.owner_90,
            state.previous_8e,
            state.scorer_8f,
            amount as u8,
        ];
        self.territory_line(news, request, game)?;
        self.queue_network_send(41, GameSend::Territory4(news));
        Ok(())
    }

    /// `00475E10` (and message 41's `00475DF0`) with `[holder, previous,
    /// scorer, amount]`: a score names its scorer and the previous holder
    /// (`0x75`, or `0x76` without one), a change names the holder (`0x73`)
    /// or, overriding it, the previous holder (`0x74`), a first claim the
    /// holder (`0x72`); each only on the machine of the slot it names, in
    /// the first player's message (`0046D050`, `0045D7C0` into 64 bytes).
    pub(crate) fn territory_line(
        &mut self,
        [owner, previous, scorer, _]: [u8; 4],
        request: &TerritoryRequest<'_>,
        game: &GameNetState,
    ) -> Result<(), TerritoryError> {
        let local = request.local_slot;
        let (text, a, b) = if scorer != 0 {
            if scorer != local {
                return Ok(());
            }
            (if previous == 0 { 0x76 } else { 0x75 }, scorer, previous)
        } else if previous != 0 {
            let mut text = (owner == local).then_some(0x73);
            if previous == local {
                text = Some(0x74);
            }
            match text {
                Some(text) => (text, owner, previous),
                None => return Ok(()),
            }
        } else {
            if owner != local {
                return Ok(());
            }
            (0x72, owner, 0)
        };
        let Some(player) = self.first_player() else {
            return Ok(());
        };
        let source = super::h0_text::capture_h0_text(super::h0_text::H0TextCaptureRequest {
            index_word: text,
            resident: Some(request.resident),
        })
        .map_err(|error| TerritoryError::Line(format!("{error:?}")))?;
        let format = match source {
            crate::grant_text::GrantTextSource::Record(Some(format)) => format,
            _ => return Ok(()),
        };
        let mut line = format_line(
            format,
            0x40,
            [i32::from(b), i32::from(a)],
            false,
            game,
            |_| None,
        );
        line.push(0);
        let recipient = self
            .player_message(player)
            .map_err(|error| TerritoryError::Line(error.to_string()))?
            .identity();
        self.apply_player_message(super::PlayerMessageRequest {
            recipient,
            operation: super::PlayerMessageOperation::Send {
                text: Some(&line),
                flag: 0,
            },
        })
        .map_err(|error| TerritoryError::Line(error.to_string()))
    }

    /// `00475B00`'s bytes: each territory's holder, in list order.
    pub fn territory_holders(&self) -> Result<Vec<u8>, ObjectError> {
        self.territories
            .iter()
            .map(|&object| Ok(self.selector46(object)?.owner_90))
            .collect()
    }

    /// `00475BD0` on the host: the territory numbered `index` goes to the
    /// guest's `slot` (`00475750`), which is told with message 40.
    pub fn receive_territory_claim(
        &mut self,
        [index, slot]: [u8; 2],
        request: &TerritoryRequest<'_>,
        game: &mut GameNetState,
    ) -> Result<(), TerritoryError> {
        if !request.host {
            return Ok(());
        }
        for object in self.territories.clone() {
            if self.selector46(object)?.index_78 != u32::from(index) {
                continue;
            }
            self.capture_territory(object, slot, request, game)?;
            self.selector46_mut(object)?.granted_8d = slot;
            self.queue_network_send(40, GameSend::Territory2([index, slot]));
        }
        Ok(())
    }

    /// `00475C60`: a grant to this machine's slot ends its claim (`+8C`).
    pub fn receive_territory_grant(
        &mut self,
        [index, slot]: [u8; 2],
        local_slot: u8,
    ) -> Result<(), ObjectError> {
        if slot != local_slot {
            return Ok(());
        }
        for object in self.territories.clone() {
            let state = self.selector46_mut(object)?;
            if state.index_78 == u32::from(index) {
                state.claim_8c = 0;
            }
        }
        Ok(())
    }

    /// `00475B50`: a list as long as this machine's gives each territory its
    /// holder; one the host granted (`+8D`) answers with message 39.
    pub fn receive_territory_holders(&mut self, holders: &[u8]) -> Result<(), ObjectError> {
        if holders.len() != self.territories.len() {
            return Ok(());
        }
        for (index, &holder) in holders.iter().enumerate() {
            let object = self.territories[index];
            let state = self.selector46_mut(object)?;
            state.owner_90 = holder;
            if state.granted_8d != 0 {
                let claim = [state.index_78 as u8, state.claim_8c];
                self.queue_network_send(39, GameSend::Territory2(claim));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "selector46_tests.rs"]
mod tests;
