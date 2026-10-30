//! Capture the Banner's banner (selector 40) and base (selector 41). The base
//! is the shared actor `0047A7A0` with the final table `005267E0`; the
//! banner (`0045B4A0`, table `005260A0`) builds on it. Both tables are the
//! shared actor's `00527C00` but for `+00` (`004110E0`), the payload `+0C`
//! (`0045BEE0`, `0045B9A0`: the shared `0047AD70`, then the ground
//! `00479DD0`; the banner keeps that spot as its home) and the update `+24`
//! (`0045BDB0`, `0045B5A0`), which `+3C` (`00461C50`) also runs. The banner
//! adds its draw `+2C` (`0045BB50`), the shots' `+40` (`004863C0`), `+B8`
//! (`00404610`, itself) and its record sync `+C0`/`+C4` (`0045B820`,
//! `0045B860`).
//!
//! A player's vehicle of another team that reaches a banner takes it
//! (`004212B0`, `+598`) and carries it home to its own team's base, which
//! captures it (`0045BA90`: a capture and 1,000 points for the carrier's
//! team, a loss and 500 points off for the banner's). A dropped banner
//! waits 60 seconds, then goes home. Messages 10 to 17 keep the banner's
//! owner and the carrier's machine in step.

use inf_formats::native_basis::{NativeBasisMath, RotationPlane};
use thiserror::Error;

use super::{ActorConstructionError, ActorConstructionRequest, ActorExtension, ActorHandle};
use crate::network::protocol::GameSend;
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId};
use crate::terrain_runtime::TerrainRuntime;

/// `0045B54A`'s 60 seconds before a dropped banner goes home.
const AWAY_MICROSECONDS: u32 = 60_000_000;
/// `0045BA90`: a capture's points for the carrier's team, and the banner's
/// team's loss.
const CAPTURE_POINTS: u16 = 1000;
const LOSS_POINTS: u16 = 500;
/// The process tasks `0055F100`: nineteen teams.
const TEAMS: u32 = 0x13;

/// The banner's events (`0045BCB0`, messages 13 to 17).
pub const BANNER_SHOWN: u8 = 0;
pub const BANNER_DROPPED: u8 = 1;
pub const BANNER_TAKEN: u8 = 2;
pub const BANNER_CAPTURED: u8 = 3;
pub const BANNER_HOME: u8 = 4;
/// `0045BCB0`'s message for each event.
const EVENT_MESSAGES: [u32; 5] = [0xe, 0xf, 0xd, 0x10, 0x11];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector40State {
    /// +138: away from home; a local banner goes home when +150 runs out.
    pub away_138: u8,
    /// +139: carried by +144.
    pub carried_139: u8,
    /// +13A: a base is capturing it.
    pub capturing_13a: u8,
    /// +13C: the capturing team, written at the capture.
    pub team_13c: Option<u32>,
    /// +140: the carrier when the capture began.
    pub carrier_140: Option<ActorHandle>,
    /// +144: the carrier, the process NULL when none.
    pub carrier_144: ActorHandle,
    /// +148 and +14C: home, the payload's grounded position.
    pub home_148: Option<[u16; 3]>,
    /// +150: the time left away, written when it is dropped.
    pub away_150: Option<u32>,
    /// +154: its spin's phase.
    pub spin_154: u32,
}

/// What the banner and the base reach besides the arena.
pub trait BannerHost {
    type Error;
    /// `0046CC50(banner, event, value)` on the first player: its line and,
    /// when its own unit captured the banner, the pickup (4, 5000).
    fn announce(
        &mut self,
        arena: &mut ObjectArena,
        banner: ObjectId,
        event: u8,
        value: ActorHandle,
    ) -> Result<(), Self::Error>;
}

/// The process words a banner reads.
#[derive(Clone, Copy)]
pub struct BannerRequest<'a> {
    pub delta: u32,
    /// `0055F2F4`.
    pub local_slot: u8,
    /// `0054E894`.
    pub null: ActorHandle,
    pub math: &'a NativeBasisMath,
    /// The ground `00479DD0` reads.
    pub terrain: &'a TerrainRuntime,
}

#[derive(Debug, Error)]
pub enum BannerError<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error("banner {0:?} reads its unwritten {1}")]
    Unwritten(ObjectId, &'static str),
    #[error("banner {0:?} names no live carrier (0045B9E0 reads through it)")]
    Carrier(ObjectId),
    #[error("{0:?}'s +64 view is ported only for a vehicle and the camera")]
    View(ObjectId),
    #[error("banner {0:?} scores outside a network game")]
    NoNetworkGame(ObjectId),
    #[error("banner {0:?} scores for team {1}, which has no task")]
    Team(ObjectId, u32),
    #[error("banner host: {0}")]
    Host(E),
}

fn owner_slot(handle: ActorHandle) -> u8 {
    (handle.raw() >> 27) as u8
}

impl ObjectArena {
    /// Case 0x29 (selector 41): the shared `0047A7A0`.
    pub fn allocate_selector41(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension = ActorExtension::Selector41;
        Ok(object)
    }

    /// Case 0x28 (selector 40), `0045B4A0`: the base's construction, then
    /// nothing carried and the process NULL as its carrier.
    pub fn allocate_selector40(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        // 0045B4C4 reads 0054E894 again, as 0047A7A0 did for +88.
        let null = self.actor(object)?.owner_handle;
        self.actor_mut(object)?.extension = ActorExtension::Selector40(Box::new(Selector40State {
            away_138: 0,
            carried_139: 0,
            capturing_13a: 0,
            team_13c: None,
            carrier_140: None,
            carrier_144: null,
            home_148: None,
            away_150: None,
            spin_154: 0,
        }));
        Ok(object)
    }

    pub fn selector40(&self, object: ObjectId) -> Result<&Selector40State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector40(state) => Ok(state),
            _ => Err(ObjectError::NotSelector40(object)),
        }
    }

    fn selector40_mut(&mut self, object: ObjectId) -> Result<&mut Selector40State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector40(state) => Ok(state),
            _ => Err(ObjectError::NotSelector40(object)),
        }
    }

    pub fn is_selector41(&self, object: ObjectId) -> bool {
        self.actor(object)
            .is_ok_and(|actor| matches!(actor.extension, ActorExtension::Selector41))
    }

    /// `00479DD0`: onto the ground under it.
    pub(crate) fn ground_actor(
        &mut self,
        object: ObjectId,
        terrain: &TerrainRuntime,
    ) -> Result<(), ObjectError> {
        let [x, _, z] = self.position(object)?;
        self.owned_mut(object)?.state.base.position_words =
            Some([x, terrain.height_at([x, z]) as u16, z]);
        Ok(())
    }

    /// `0045BEE0` and `0045B9A0` after the shared `0047AD70`: grounded, and
    /// a banner keeps the spot as its home.
    pub(in crate::object_runtime) fn initialize_banner_payload(
        &mut self,
        object: ObjectId,
        terrain: &TerrainRuntime,
    ) -> Result<(), ObjectError> {
        self.ground_actor(object, terrain)?;
        if self.selector40(object).is_ok() {
            let home = self.position(object)?;
            self.selector40_mut(object)?.home_148 = Some(home);
        }
        Ok(())
    }

    /// Virtual `+B8`: the banner an actor is (`00404610` returns itself;
    /// every other table's `00401300` gives none).
    pub fn banner_of(&self, object: ObjectId) -> Option<ObjectId> {
        self.selector40(object).is_ok().then_some(object)
    }

    /// `00441639..00441696`: over the actor list in order, each banner
    /// (`+B8`) carried (`+139`) by other than `null`, as its carrier's
    /// owner bits (`+146 >> 11`) and its team (`+B4`).
    pub fn banner_carriers(&self, null: ActorHandle) -> Result<Vec<(u8, u32)>, ObjectError> {
        let mut carriers = Vec::new();
        for &actor in self.actor_list() {
            let Some(banner) = self.banner_of(actor) else {
                continue;
            };
            let state = self.selector40(banner)?;
            if state.carried_139 != 0 && state.carrier_144 != null {
                carriers.push((state.carrier_144.owner(), self.actor(banner)?.word_b4));
            }
        }
        Ok(carriers)
    }

    /// Virtual `+64` of a player's unit: a vehicle is its own view, the
    /// camera has none (`00401300`).
    fn unit_view<E>(&self, unit: ObjectId) -> Result<Option<ObjectId>, BannerError<E>> {
        if self.selector2_actor(unit).is_ok() {
            Ok(Some(unit))
        } else if self.selector16(unit).is_ok() {
            Ok(None)
        } else {
            Err(BannerError::View(unit))
        }
    }

    /// `004212B0` on a vehicle: with its wait `+59C` over and nothing held,
    /// it takes the banner (`+598`).
    fn vehicle_claims_banner<E>(
        &mut self,
        vehicle: ObjectId,
        banner: ObjectId,
        null: ActorHandle,
    ) -> Result<bool, BannerError<E>> {
        let handle = self.actor(banner)?.handle;
        let state = self
            .selector2_actor_mut(vehicle)
            .map_err(|_| ObjectError::NotDerivedActor(vehicle))?;
        if state.dwords_59c_5a0[0] != 0 {
            return Ok(false);
        }
        let held = state
            .update
            .handle_598
            .ok_or(BannerError::Unwritten(vehicle, "vehicle +598"))?;
        if held != null {
            return Ok(false);
        }
        state.update.handle_598 = Some(handle);
        Ok(true)
    }

    /// `0045BDB0`, the base's `+24`: the first player's vehicle within one
    /// and a half of its radius (three of them vertically), live and playing
    /// for no team, team 1 or the base's own, brings home the banner it
    /// carries: the banner's capture begins, scored at once when the banner
    /// is this machine's (`0045BA90`), else asked of its owner (message 12).
    pub fn tick_selector41<H: BannerHost>(
        &mut self,
        base: ObjectId,
        request: &BannerRequest<'_>,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        let Some(unit) = self.first_player_primary_actor()? else {
            return Ok(());
        };
        let reach = u32::from(self.object(unit)?.base.word_28) * 3 / 2;
        let at = self.position(base)?;
        let theirs = self.position(unit)?;
        let delta = |axis: usize| i32::from(at[axis].wrapping_sub(theirs[axis]) as i16);
        let unit_state = self.actor(unit)?;
        let team = unit_state.word_b4;
        if delta(1).abs() >= (reach * 3) as i32
            || delta(0) * delta(0) + delta(2) * delta(2) > (reach * reach) as i32
            || unit_state.word_c4 <= 2
            || !(team == 0 || team == 1 || team == self.actor(base)?.word_b4)
        {
            return Ok(());
        }
        let Some(view) = self.unit_view(unit)? else {
            return Ok(());
        };
        let held = self
            .selector2_actor(view)
            .map_err(|_| ObjectError::NotDerivedActor(view))?
            .update
            .handle_598
            .ok_or(BannerError::Unwritten(view, "vehicle +598"))?;
        if held == request.null {
            return Ok(());
        }
        let Some(banner) = self
            .resolve_actor_handle(held)
            .map_err(|_| ObjectError::MissingActorRegistry)?
            .and_then(|held| self.banner_of(held))
        else {
            return Ok(());
        };
        let carrier_team = self.actor(view)?.word_b4;
        {
            let state = self.selector40_mut(banner)?;
            state.capturing_13a = 1;
            state.team_13c = Some(carrier_team);
            state.carrier_140 = Some(state.carrier_144);
        }
        if self.object(banner)?.base.flags & 0x8000_0000 == 0 {
            let carrier = self.selector40(banner)?.carrier_144;
            return self.capture_banner(banner, carrier, carrier_team, request, host);
        }
        let state = self.selector40(banner)?;
        let words = [
            self.actor(banner)?.handle.raw(),
            state.carrier_140.unwrap_or(request.null).raw(),
            carrier_team,
        ];
        self.queue_network_send(12, GameSend::Capture(words));
        Ok(())
    }

    /// `0045B5A0`, the banner's `+24` (and through `00461C50` its `+3C`).
    pub fn tick_selector40<H: BannerHost>(
        &mut self,
        banner: ObjectId,
        request: &BannerRequest<'_>,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        // 0045B5A0..0045B61B: it turns a sixteenth of the delta (a
        // thirty-second while carried); each whole turn starts again from
        // the axes 0055F290, 0055F280 and 0055F270.
        let carried = self.selector40(banner)?.carried_139 != 0;
        let step = request.delta >> if carried { 5 } else { 4 };
        let spin = self.selector40(banner)?.spin_154.wrapping_add(step);
        let mut angle = step;
        self.selector40_mut(banner)?.spin_154 = spin;
        if spin > 0xffff {
            let spin = spin - 0x1_0000;
            self.selector40_mut(banner)?.spin_154 = spin;
            let basis = &mut self.actor_mut(banner)?.rotating.basis;
            for (row, axis) in [[1.0_f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
                .iter()
                .enumerate()
            {
                for (column, value) in axis.iter().enumerate() {
                    basis.words[1 + row * 3 + column] = value.to_bits();
                }
            }
            angle = spin;
        }
        let basis = &mut self.actor_mut(banner)?.rotating.basis;
        request
            .math
            .rotate_in_plane(basis, angle as i32, RotationPlane::Xz);
        // 0045B620..0045B63E: in the draw list until the game ends.
        let ended = self
            .network_game_mut()
            .is_some_and(|hold| hold.game.ended != 0);
        let listed = self.object(banner)?.base.flags & 1 != 0;
        if listed == ended {
            self.set_flag0(banner, !ended)?;
        }
        if self.selector40(banner)?.capturing_13a != 0 {
            return Ok(());
        }
        let local = self.object(banner)?.base.flags & 0x8000_0000 == 0;
        let mut view = None;
        let changed = if self.selector40(banner)?.carried_139 == 0 {
            match self.banner_reached(banner, request)? {
                Some(reached) => {
                    view = Some(reached);
                    let carrier = self.actor(reached)?.handle;
                    let state = self.selector40_mut(banner)?;
                    state.carried_139 = 1;
                    state.carrier_144 = carrier;
                    true
                }
                None => false,
            }
        } else {
            let carrier = self.selector40(banner)?.carrier_144;
            let held = match self
                .resolve_actor_handle(carrier)
                .map_err(|_| ObjectError::MissingActorRegistry)?
            {
                Some(unit) => self.unit_view(unit)?,
                None => None,
            };
            let handle = self.actor(banner)?.handle;
            let still = match held {
                Some(held) => {
                    let actor = self.actor(held)?;
                    let holds = self
                        .selector2_actor(held)
                        .map_err(|_| ObjectError::NotDerivedActor(held))?
                        .update
                        .handle_598
                        == Some(handle);
                    actor.word_c4 > 2 && (self.object(held)?.base.flags & 0x8000_0000 != 0 || holds)
                }
                None => false,
            };
            if still {
                view = held;
                false
            } else {
                self.selector40_mut(banner)?.carried_139 = 0;
                true
            }
        };
        if changed {
            let carried = self.selector40(banner)?.carried_139 != 0;
            if local {
                self.banner_event(
                    banner,
                    if carried {
                        BANNER_TAKEN
                    } else {
                        BANNER_DROPPED
                    },
                    host,
                )?;
            }
            self.announce_banner_state(banner, request)?;
        }
        // 0045B7A3: a carried banner rides with its carrier.
        if self.selector40(banner)?.carried_139 != 0 {
            if let Some(view) = view {
                let position = self.position(view)?;
                self.owned_mut(banner)?.state.base.position_words = Some(position);
            }
            return Ok(());
        }
        // 0045B7B8: a local banner away from home goes back when its time
        // runs out.
        let state = self.selector40(banner)?;
        if state.away_138 != 0 && local {
            let left = state
                .away_150
                .ok_or(BannerError::Unwritten(banner, "+150"))?;
            if left <= request.delta {
                let home = state
                    .home_148
                    .ok_or(BannerError::Unwritten(banner, "+148"))?;
                self.selector40_mut(banner)?.away_138 = 0;
                self.owned_mut(banner)?.state.base.position_words = Some(home);
                self.banner_event(banner, BANNER_HOME, host)?;
            } else {
                self.selector40_mut(banner)?.away_150 = Some(left - request.delta);
            }
        }
        Ok(())
    }

    /// `0045B6A0..0045B70D`: the first player's vehicle that reaches a free
    /// banner of another team and can take it (`004212B0`).
    fn banner_reached<E>(
        &mut self,
        banner: ObjectId,
        request: &BannerRequest<'_>,
    ) -> Result<Option<ObjectId>, BannerError<E>> {
        let Some(unit) = self.first_player_primary_actor()? else {
            return Ok(None);
        };
        let reach = u32::from(self.object(unit)?.base.word_28) * 3 / 2;
        let at = self.position(banner)?;
        let theirs = self.position(unit)?;
        let delta = |axis: usize| i32::from(at[axis].wrapping_sub(theirs[axis]) as i16);
        let unit_state = self.actor(unit)?;
        if delta(1).abs() >= (reach * 2) as i32
            || delta(0) * delta(0) + delta(2) * delta(2) > (reach * reach) as i32
            || unit_state.word_c4 < 3
            || self.actor(banner)?.word_b4 == unit_state.word_b4
        {
            return Ok(None);
        }
        let Some(view) = self.unit_view(unit)? else {
            return Ok(None);
        };
        Ok(self
            .vehicle_claims_banner(view, banner, request.null)?
            .then_some(view))
    }

    /// `0045B4E0`: a banner another machine owns tells it what this
    /// machine's carrier did (10 taken, 11 dropped); this machine's own sends
    /// its record soon, and when carried keeps it away for 60 seconds with
    /// its position left to the carrier, else grounds it with its position
    /// sent and no carrier.
    fn announce_banner_state<E>(
        &mut self,
        banner: ObjectId,
        request: &BannerRequest<'_>,
    ) -> Result<(), BannerError<E>> {
        let handle = self.actor(banner)?.handle;
        let state = self.selector40(banner)?;
        let carried = state.carried_139 != 0;
        let carrier = state.carrier_144;
        if self.object(banner)?.base.flags & 0x8000_0000 != 0 {
            if owner_slot(carrier) == request.local_slot {
                let id = if carried { 10 } else { 11 };
                self.queue_network_send(id, GameSend::Pair([carrier.raw(), handle.raw()]));
            }
            return Ok(());
        }
        self.actor_mut(banner)?.word_dc = 0;
        if carried {
            self.owned_mut(banner)?.state.base.dirty_word &= !0x80;
            let state = self.selector40_mut(banner)?;
            state.away_138 = 1;
            state.away_150 = Some(AWAY_MICROSECONDS);
            return Ok(());
        }
        self.ground_actor(banner, request.terrain)?;
        self.owned_mut(banner)?.state.base.dirty_word |= 0x80;
        self.selector40_mut(banner)?.carrier_144 = request.null;
        Ok(())
    }

    /// `0045BCB0` on a banner this machine owns: the first player's line
    /// (`0046CC50`), then the same event to every machine (13 to 17).
    fn banner_event<H: BannerHost>(
        &mut self,
        banner: ObjectId,
        event: u8,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        if self.object(banner)?.base.flags & 0x8000_0000 != 0 {
            return Ok(());
        }
        let carrier = self.selector40(banner)?.carrier_144;
        host.announce(self, banner, event, carrier)
            .map_err(BannerError::Host)?;
        let words = [self.actor(banner)?.handle.raw(), carrier.raw()];
        self.queue_network_send(EVENT_MESSAGES[usize::from(event)], GameSend::Banner(words));
        Ok(())
    }

    /// `0045B9E0` (message 10 to this machine's banner): the carrier's
    /// machine says its vehicle took the free banner.
    pub fn banner_taken<H: BannerHost>(
        &mut self,
        banner: ObjectId,
        by: ActorHandle,
        request: &BannerRequest<'_>,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        if self.selector40(banner)?.carried_139 != 0 {
            return Ok(());
        }
        let unit = self
            .resolve_actor_handle(by)
            .map_err(|_| ObjectError::MissingActorRegistry)?
            .ok_or(BannerError::Carrier(banner))?;
        if self.actor(unit)?.word_c4 <= 2 {
            return Ok(());
        }
        let Some(view) = self.unit_view(unit)? else {
            return Ok(());
        };
        if !self.vehicle_claims_banner(view, banner, request.null)? {
            return Ok(());
        }
        let carrier = self.actor(view)?.handle;
        {
            let state = self.selector40_mut(banner)?;
            state.carrier_144 = carrier;
            state.carried_139 = 1;
        }
        self.banner_event(banner, BANNER_TAKEN, host)?;
        self.announce_banner_state(banner, request)
    }

    /// `0045BA50` (message 11): its carrier dropped it.
    pub fn banner_dropped<H: BannerHost>(
        &mut self,
        banner: ObjectId,
        by: ActorHandle,
        request: &BannerRequest<'_>,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        let state = self.selector40(banner)?;
        if state.carried_139 == 0 || state.carrier_144 != by {
            return Ok(());
        }
        self.selector40_mut(banner)?.carried_139 = 0;
        self.announce_banner_state(banner, request)?;
        self.banner_event(banner, BANNER_DROPPED, host)
    }

    /// `0045BA90` (and message 12): the carrier brought it to its team's
    /// base. It goes home, the carrier's team gains a capture and 1,000
    /// points, and its own team a loss and 500 points off (`0045C190`
    /// then checks the score limit).
    pub fn capture_banner<H: BannerHost>(
        &mut self,
        banner: ObjectId,
        carrier: ActorHandle,
        team: u32,
        request: &BannerRequest<'_>,
        host: &mut H,
    ) -> Result<(), BannerError<H::Error>> {
        let state = self.selector40(banner)?;
        if state.carried_139 == 0 || state.carrier_144 != carrier {
            return Ok(());
        }
        let home = state
            .home_148
            .ok_or(BannerError::Unwritten(banner, "+148"))?;
        {
            let state = self.selector40_mut(banner)?;
            state.carried_139 = 0;
            state.away_138 = 0;
            state.capturing_13a = 0;
        }
        self.banner_event(banner, BANNER_CAPTURED, host)?;
        self.announce_banner_state(banner, request)?;
        self.owned_mut(banner)?.state.base.position_words = Some(home);
        let own = self.actor(banner)?.word_b4;
        if team >= TEAMS {
            return Ok(());
        }
        let hold = self
            .network_game_mut()
            .ok_or(BannerError::NoNetworkGame(banner))?;
        let teams = &mut hold.game.teams;
        let Some(scorer) = teams[team as usize].as_mut() else {
            return Ok(());
        };
        scorer.words_0c[0] = scorer.words_0c[0].wrapping_add(1);
        scorer.score = scorer.score.wrapping_add(CAPTURE_POINTS);
        let loser = teams
            .get_mut(own as usize)
            .and_then(Option::as_mut)
            .ok_or(BannerError::Team(banner, own))?;
        loser.words_0c[1] = loser.words_0c[1].wrapping_add(1);
        loser.score = loser.score.saturating_sub(LOSS_POINTS);
        let scorer = teams[team as usize].expect("checked above");
        hold.game.check_team_limit(scorer);
        Ok(())
    }

    /// `0045BB50`: a carried banner is drawn where its carrier is.
    pub(in crate::object_runtime) fn place_banner_for_draw(
        &mut self,
        banner: ObjectId,
    ) -> Result<(), ObjectError> {
        let state = self.selector40(banner)?;
        if state.carried_139 == 0 {
            return Ok(());
        }
        let carrier = state.carrier_144;
        if let Some(carrier) = self
            .resolve_actor_handle(carrier)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        {
            let position = self.position(carrier)?;
            self.owned_mut(banner)?.state.base.position_words = Some(position);
        }
        Ok(())
    }

    /// `0045B820`, the banner's `+C0`: the carrier while carried.
    pub(in crate::object_runtime) fn banner_sync_out(
        &self,
        banner: ObjectId,
        out: &mut [u8],
    ) -> Result<i32, ObjectError> {
        let state = self.selector40(banner)?;
        if state.carried_139 == 0 {
            return Ok(0);
        }
        let Some(out) = out.get_mut(..4) else {
            return Ok(-1);
        };
        out.copy_from_slice(&state.carrier_144.raw().to_le_bytes());
        Ok(4)
    }

    /// `0045B860`, the banner's `+C4` on another machine: its owner's word
    /// on the carrier. A capture under way is asked again (12) while the
    /// owner still names the same carrier. A banner the owner says this
    /// machine's vehicle carries, which here is free, is dropped (11); one
    /// the owner calls free that this machine's vehicle carries is taken
    /// (10); otherwise the owner's word stands.
    pub(in crate::object_runtime) fn banner_sync_in(
        &mut self,
        banner: ObjectId,
        carrier: Option<ActorHandle>,
        local_slot: u8,
        null: ActorHandle,
    ) -> Result<(), ObjectError> {
        let handle = self.actor(banner)?.handle;
        let state = self.selector40(banner)?;
        if state.capturing_13a != 0 {
            if carrier.is_some() && carrier == state.carrier_140 {
                let words = [
                    handle.raw(),
                    state.carrier_140.unwrap_or(null).raw(),
                    state.team_13c.unwrap_or(0),
                ];
                self.queue_network_send(12, GameSend::Capture(words));
                return Ok(());
            }
            let state = self.selector40_mut(banner)?;
            state.capturing_13a = 0;
            state.carried_139 = 0;
            state.carrier_144 = null;
        }
        let state = self.selector40(banner)?;
        match carrier {
            Some(carrier) => {
                if state.carried_139 == 0 && owner_slot(carrier) == local_slot {
                    self.queue_network_send(11, GameSend::Pair([carrier.raw(), handle.raw()]));
                    return Ok(());
                }
                let state = self.selector40_mut(banner)?;
                state.carrier_144 = carrier;
                state.carried_139 = 1;
                state.away_138 = 1;
            }
            None => {
                if state.carried_139 != 0 && owner_slot(state.carrier_144) == local_slot {
                    let pair = [state.carrier_144.raw(), handle.raw()];
                    self.queue_network_send(10, GameSend::Pair(pair));
                    return Ok(());
                }
                let state = self.selector40_mut(banner)?;
                state.carried_139 = 0;
                state.away_138 = 0;
                state.capturing_13a = 0;
            }
        }
        Ok(())
    }

    /// `0046CC50`'s choice: the H0 line for the first player's unit, and
    /// whether that unit itself captured the banner (its pickup).
    pub fn banner_line(
        &mut self,
        banner: ObjectId,
        event: u8,
        value: ActorHandle,
    ) -> Result<Option<(u16, bool)>, ObjectError> {
        let Some(player) = self.first_player() else {
            return Ok(None);
        };
        let Some(unit) = self
            .player_actor(player)
            .map_err(|_| ObjectError::MissingActorRegistry)?
        else {
            return Ok(None);
        };
        let unit_state = self.actor(unit)?;
        let (own, team) = (unit_state.handle, unit_state.word_b4);
        let same_team = self.actor(banner)?.word_b4 == team;
        // Each event's lines: by this unit, for the banner's team, for the
        // carrier's team.
        let lines: [u16; 3] = match event {
            BANNER_SHOWN => [0x66, 0x6e, 0x6a],
            BANNER_DROPPED => [0x67, 0x6f, 0x6b],
            BANNER_TAKEN => [0x64, 0x6c, 0x68],
            BANNER_CAPTURED => [0x65, 0x6d, 0x69],
            BANNER_HOME => {
                return Ok(Some((if same_team { 0x70 } else { 0x71 }, false)));
            }
            _ => return Ok(None),
        };
        if value == own {
            return Ok(Some((lines[0], event == BANNER_CAPTURED)));
        }
        if same_team {
            return Ok(Some((lines[1], false)));
        }
        let carrier = self
            .resolve_actor_handle(value)
            .map_err(|_| ObjectError::MissingActorRegistry)?;
        Ok(match carrier {
            Some(carrier) if self.actor(carrier)?.word_b4 == team => Some((lines[2], false)),
            _ => None,
        })
    }
}

#[cfg(test)]
#[path = "selector40_tests.rs"]
mod tests;
