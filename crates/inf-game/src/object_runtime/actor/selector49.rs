//! Selector 49, the Seismic Stabiliser: 00476380 over 0040D800 over the
//! shared actor (final 00527908). Its table is the bridge's 0051FF54 but for
//! +00 (004763C0), +04 (the commands 004763F0), +1C (the ground fit
//! 00476470), +24 (the update 004765A0), +34 (the after-load 0047FE90), +98
//! (the family's own 0040DE70), +AC (the death 00476520) and +B4 (the damage
//! gate 00476720).
//!
//! It starts sunk below the ground and, once no link holds it, rises,
//! carrying a square of terrain up with it. It takes damage only at full
//! height. Its death grants +7A, starts the camera shake and the terrain
//! dust (process 005483B0), starts the mission's doom countdown at +15C
//! seconds, retires the actor it is linked to, and flashes the world
//! (00487D70) when its own copy of the countdown ends.

use inf_formats::{InstanceCommandPayload, InstanceCommandTarget, InstanceCommandView};
use thiserror::Error;

use super::{
    family40d800::Actor40d800State, ActorCommandContext, ActorCommandError, ActorConstructionError,
    ActorConstructionRequest, ActorExtension, ActorHandle, BridgeTerrain,
};
use crate::object_runtime::{ObjectArena, ObjectError, ObjectId, PlayerId};
use crate::terrain_runtime::TerrainRuntime;

#[cfg(test)]
#[path = "selector49_tests.rs"]
mod tests;

/// Terrain cells are 0x80 apart across the square.
const STEP: i32 = 0x80;

/// 00476380's writes after 0040D800, and the fields its methods own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selector49State {
    pub intermediate: Actor40d800State,
    /// +158: half the raised square's side, which the fit rounds down to
    /// 0x80.
    pub word_158: u16,
    /// +15A: how far below the ground it starts.
    pub word_15a: u16,
    /// +15C: the doom countdown's seconds; 10.
    pub dword_15c: u32,
    /// +160, +162, +164, +166 (WORDs): the sunk height, the ground, the
    /// current height and the height last written to the terrain, from the
    /// fit.
    pub heights_160: Option<[u16; 4]>,
    /// +168: microseconds left after the death; zero before it.
    pub dword_168: u32,
    /// +16C: command 24's one-based placement reference, which the fit turns
    /// into that actor's handle or the process NULL. The constructor leaves
    /// it unwritten, so a placement without command 24 (leaf 27's) keeps
    /// heap residue here and its link stays unknown.
    pub dword_16c: Option<u32>,
}

impl Selector49State {
    fn new() -> Self {
        Self {
            intermediate: Actor40d800State::new(),
            word_158: 0,
            word_15a: 0,
            dword_15c: 10,
            heights_160: None,
            dword_168: 0,
            dword_16c: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum Selector49Error<E> {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Construction(#[from] ActorConstructionError),
    #[error("stabiliser {0:?} has no position")]
    Position(ObjectId),
    #[error("stabiliser {0:?}'s ground fit needs H12/0")]
    Terrain(ObjectId),
    #[error("stabiliser {0:?} reads its heights before the fit wrote them")]
    UnwrittenHeights(ObjectId),
    /// StabiliserLinkResidueBoundary: the constructor never writes +16C, and
    /// leaf 27's placement has no command 24, so retail's link is heap
    /// residue (TERRAIN_RUNTIME.md).
    #[error("stabiliser {0:?}'s linked actor +16C is allocation residue")]
    UnwrittenLink(ObjectId),
    /// `00476523` uses 00488AA0's result without a NULL test.
    #[error("stabiliser {0:?}'s death needs a first player (00488AA0)")]
    MissingPlayer(ObjectId),
    #[error("stabiliser {object:?} service at {address:08X}: {error}")]
    Host {
        object: ObjectId,
        address: u32,
        error: E,
    },
}

impl<E> Selector49Error<E> {
    /// The host's own error, or this stop without it.
    pub fn split_host(self) -> Result<Selector49Error<std::convert::Infallible>, E> {
        Ok(match self {
            Self::Host { error, .. } => return Err(error),
            Self::Object(error) => Selector49Error::Object(error),
            Self::Construction(error) => Selector49Error::Construction(error),
            Self::Position(object) => Selector49Error::Position(object),
            Self::Terrain(object) => Selector49Error::Terrain(object),
            Self::UnwrittenHeights(object) => Selector49Error::UnwrittenHeights(object),
            Self::UnwrittenLink(object) => Selector49Error::UnwrittenLink(object),
            Self::MissingPlayer(object) => Selector49Error::MissingPlayer(object),
        })
    }
}

/// What the update reaches.
pub trait Selector49UpdateHost {
    type Error;
    /// The terrain and the spacing owner `00450350`'s fine scope borrows.
    fn terrain(&mut self) -> BridgeTerrain<'_>;
    /// `0047DFD0(delta)`, the status visual, while the countdown runs.
    fn status_visual(
        &mut self,
        arena: &mut ObjectArena,
        object: ObjectId,
        delta: u32,
    ) -> Result<(), Self::Error>;
    /// `00487D70(position, 0xF00, 0)`, the world flash, as it ends.
    fn flash(&mut self, arena: &mut ObjectArena, position: [u16; 3]) -> Result<(), Self::Error>;
}

/// What the death reaches, in order.
pub trait Selector49DeathHost {
    type Error;
    /// `004750C0(+7A)` on the first player's grants +9C.
    fn grant(
        &mut self,
        arena: &mut ObjectArena,
        player: PlayerId,
        id: u16,
    ) -> Result<(), Self::Error>;
    /// Process DWORD `005483B0 = 1`: the camera shake and the terrain dust.
    fn terrain_event(&mut self);
    /// The mission's doom countdown: W+24 = `micros`, W+17 = 1.
    fn start_doom(&mut self, micros: u32);
    /// `0047CE90` on the linked actor, alive in state 3.
    fn retire(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<(), Self::Error>;
}

impl ObjectArena {
    /// `0046110E..00461131` (the switch's case 49), then 00476380: the
    /// family 0040D800, +15C = 10, +158/+15A/+168 zero.
    pub fn allocate_selector49(
        &mut self,
        request: ActorConstructionRequest<'_>,
    ) -> Result<ObjectId, ActorConstructionError> {
        let object = self.allocate_actor(request)?;
        self.actor_mut(object)?.extension =
            ActorExtension::Selector49(Box::new(Selector49State::new()));
        Ok(object)
    }

    pub fn selector49(&self, object: ObjectId) -> Result<&Selector49State, ObjectError> {
        match &self.actor(object)?.extension {
            ActorExtension::Selector49(state) => Ok(state),
            _ => Err(ObjectError::NotSelector49(object)),
        }
    }

    pub fn selector49_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut Selector49State, ObjectError> {
        match &mut self.actor_mut(object)?.extension {
            ActorExtension::Selector49(state) => Ok(state),
            _ => Err(ObjectError::NotSelector49(object)),
        }
    }

    /// 004763F0: shared commands through 15, 0040D9C0 for 16..20, then a
    /// WORD for 21 (+158) and 22 (+15A) and a DWORD for 23 (+15C) and 24
    /// (+16C); a higher code refuses.
    pub fn apply_selector49_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
        context: ActorCommandContext<'_>,
    ) -> Result<bool, ActorCommandError> {
        if command.target != InstanceCommandTarget::Constructor(49) {
            return Err(ActorCommandError::WrongTarget);
        }
        self.selector49(object)?;
        let code = command.code.value;
        if code <= 15 {
            return self.apply_actor_command(object, command, context);
        }
        let state = self.selector49_mut(object)?;
        if let Some(result) = state.intermediate.apply_command(code, &command.payload) {
            let dirty_0200 = result.map_err(ActorCommandError::Payload)?;
            if dirty_0200 {
                self.owned_mut(object)?.state.base.dirty_word |= 0x0200;
            }
            return Ok(true);
        }
        use InstanceCommandPayload::*;
        match (code, &command.payload) {
            (21, Selector49Word158(value)) => state.word_158 = value.value,
            (22, Selector49Word15a(value)) => state.word_15a = value.value,
            (23, Selector49Seconds15c(value)) => state.dword_15c = value.value,
            (24, Selector49Reference16c(value)) => state.dword_16c = Some(value.value),
            (21..=24, _) => return Err(ActorCommandError::Payload(code)),
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 00476470, the stabiliser's +1C: +158 keeps its multiple of 0x80; the
    /// ground under it (`00450A30`) is +162, sinking +15A gives +160 and the
    /// current height +164, and +166 is one above, so the first update
    /// writes the terrain. The link +16C becomes the actor placed at that
    /// one-based reference (`0047F770`) or the process NULL. Then the
    /// family's 0040E000.
    pub fn finish_selector49_initialization(
        &mut self,
        object: ObjectId,
        terrain: Option<&TerrainRuntime>,
    ) -> Result<(), Selector49Error<std::convert::Infallible>> {
        // A missing H12/0 stops at the ground read, before any write.
        self.fit_selector49(object, |xz| {
            terrain.map(|terrain| terrain.height_at(xz)).ok_or(())
        })
        .map_err(|error| {
            error
                .split_host()
                .unwrap_or(Selector49Error::Terrain(object))
        })
    }

    /// [`Self::finish_selector49_initialization`] over a supplied ground
    /// height (`00450A30`).
    pub fn fit_selector49<E>(
        &mut self,
        object: ObjectId,
        height: impl FnOnce([u16; 2]) -> Result<i16, E>,
    ) -> Result<(), Selector49Error<E>> {
        let [x, _, z] = self
            .object(object)?
            .base
            .position_words
            .ok_or(Selector49Error::Position(object))?;
        let ground = height([x, z]).map_err(|error| Selector49Error::Host {
            object,
            address: 0x0047_64a6,
            error,
        })? as u16;
        let state = self.selector49_mut(object)?;
        state.word_158 &= 0xff80;
        let sunk = ground.wrapping_sub(state.word_15a);
        state.heights_160 = Some([sunk, ground, sunk, sunk.wrapping_add(1)]);
        let reference = state.dword_16c;
        // An unwritten reference keeps its residue: the lookup's result is
        // unknown too.
        if let Some(reference) = reference {
            let handle = self.actor_handle_by_placement(reference as i32)?;
            self.selector49_mut(object)?.dword_16c = Some(handle.raw());
        }
        Ok(self.finish_family40d800_initialization(object)?)
    }

    /// 004765A0, the stabiliser's +24. Before its death it holds at the
    /// sunk height while a link holds it (+14D set, +14C clear) and steps
    /// up once; from there it rises by `delta >> 13` a call to the ground.
    /// Whenever its height differs from the height last written, every cell
    /// of the square, 0x80 apart from minus +158 to under +158 across X and
    /// Z, takes that height under the fine spacing, and it stands at it.
    /// After its death it counts +168 down with the status visual and, at
    /// the end, flashes the world and disposes itself.
    pub fn tick_selector49<H: Selector49UpdateHost>(
        &mut self,
        object: ObjectId,
        delta: u32,
        host: &mut H,
    ) -> Result<(), Selector49Error<H::Error>> {
        let fail = |address| {
            move |error| Selector49Error::Host {
                object,
                address,
                error,
            }
        };
        let left = self.selector49(object)?.dword_168 as i32;
        if left != 0 {
            if left > delta as i32 {
                self.selector49_mut(object)?.dword_168 = left.wrapping_sub(delta as i32) as u32;
                return host
                    .status_visual(self, object, delta)
                    .map_err(fail(0x0047_65c1));
            }
            let position = self.selector49_position(object)?;
            host.flash(self, position).map_err(fail(0x0047_65dc))?;
            self.dispose(object)?;
            return Ok(());
        }
        let links = self.family40d800(object)?;
        let (linked, held) = (links.byte_14d != 0, links.byte_14c != 0);
        let state = self.selector49_mut(object)?;
        let [sunk, ground, mut height, written] = state
            .heights_160
            .ok_or(Selector49Error::UnwrittenHeights(object))?;
        if height == sunk {
            if linked && !held {
                height = height.wrapping_add(1);
            }
        } else if (height as i16) < (ground as i16) {
            height = height.wrapping_add((delta >> 13) as u16);
            if (height as i16) > (ground as i16) {
                height = ground;
            }
        }
        state.heights_160 = Some([sunk, ground, height, written]);
        if height == written {
            return Ok(());
        }
        let half = i32::from(state.word_158);
        let [x, _, z] = self.selector49_position(object)?;
        let mut terrain = host.terrain();
        terrain.fine();
        let mut across = half.wrapping_neg();
        while i32::from(across as i16) < half {
            let mut along = half.wrapping_neg();
            while i32::from(along as i16) < half {
                let cell = [x.wrapping_add(across as u16), z.wrapping_add(along as u16)];
                terrain
                    .terrain
                    .write_raw_height(cell, i32::from(height as i16));
                along = along.wrapping_add(STEP);
            }
            across = across.wrapping_add(STEP);
        }
        if let Some(position) = &mut self.owned_mut(object)?.state.base.position_words {
            position[1] = height;
        }
        terrain.restore();
        if let Some(heights) = &mut self.selector49_mut(object)?.heights_160 {
            heights[3] = height;
        }
        Ok(())
    }

    /// 00476720, the stabiliser's +B4: the shared 0047CBC0 only once its
    /// height has reached the ground.
    pub fn selector49_admits_damage(&self, object: ObjectId) -> Result<bool, ObjectError> {
        Ok(match self.selector49(object)?.heights_160 {
            Some([_, ground, height, _]) => (ground as i16) <= (height as i16),
            None => false,
        })
    }

    /// 00476520, the stabiliser's +AC, which 0047D670 reaches at state 1:
    /// the first player is granted +7A, the terrain event starts, the
    /// stabiliser and the mission count +15C seconds down (wrapping
    /// microseconds) with the mission's doom flag set, and the linked actor
    /// retires if alive.
    pub fn retire_selector49<H: Selector49DeathHost>(
        &mut self,
        object: ObjectId,
        host: &mut H,
    ) -> Result<(), Selector49Error<H::Error>> {
        let fail = |address| {
            move |error| Selector49Error::Host {
                object,
                address,
                error,
            }
        };
        let player = self
            .first_player()
            .ok_or(Selector49Error::MissingPlayer(object))?;
        let grant = self.actor(object)?.word_7a;
        host.grant(self, player, grant).map_err(fail(0x0047_6535))?;
        host.terrain_event();
        let state = self.selector49_mut(object)?;
        let micros = state.dword_15c.wrapping_mul(1_000_000);
        state.dword_168 = micros;
        host.start_doom(micros);
        let link = state
            .dword_16c
            .ok_or(Selector49Error::UnwrittenLink(object))?;
        if let Some(actor) = self.resolve_actor_handle(ActorHandle::from_raw(link))? {
            if self.actor(actor)?.word_c4 == 3 {
                host.retire(self, actor).map_err(fail(0x0047_6594))?;
            }
        }
        Ok(())
    }

    /// 0047FE90, the stabiliser's and the Siren's +34: a completed placement
    /// (0048EA80 over its signed +78) disposes it.
    pub(crate) fn selector49_after_load(
        &mut self,
        object: ObjectId,
        world_completion: &[u32; 4],
    ) -> Result<(), ObjectError> {
        let index = self.actor(object)?.byte_78 as i8;
        if crate::object_runtime::world_load::world_completion_bit(world_completion, index) {
            self.dispose(object)?;
        }
        Ok(())
    }

    fn selector49_position(&self, object: ObjectId) -> Result<[u16; 3], ObjectError> {
        self.object(object)?
            .base
            .position_words
            .ok_or(ObjectError::ActorPositionUnknown(object))
    }
}
