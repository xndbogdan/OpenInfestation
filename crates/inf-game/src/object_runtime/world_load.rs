//! Loader-owned object passes in 0048DC30 around the placement loop.
//!
//! 0048DC30 zeroes the world's actor serial at +3D0 before its placements.
//! After each successful construction it calls 0047F6C0 on offline actors.
//! After every placement it runs route finalizer 00404D40, disposal drain
//! 00479840 (world-owner +8 is one only around that call), and then two inline
//! virtual +20 traversals: the current flag-0 list [G+10], then the parked
//! list G+30. Selector 88's callback binds the existing first player there;
//! the loader's own later player spawn/binding is not owned here.

use thiserror::Error;

use super::actor::FamilyLinkChange;
use super::disposal::{
    DisposalDispatch, ObjectDisposalError, ObjectDisposalFailurePolicy, ObjectDisposalRequest,
    ObjectDisposalState,
};
use super::{
    ActorExtension, CameraFactoryInputs, LiveGrantContext, ObjectAfterLoadInputs, ObjectArena,
    ObjectClass, ObjectError, ObjectId, ObjectList, ObjectNode, PlayerAttachError,
    RuntimeAllocationError, SoundTeardownService, WorldCleanupError,
};
use crate::world_mission::WorldMission;

/// 0048DC30's drain over the loader's live owners. The retained-manager load
/// supplies them, so a disposed actor's class +30 runs (placement death and
/// class cleanup) before its destructor. Without them the loader keeps the
/// strict actor boundary below.
pub struct WorldLoadDisposal<'a, 'g> {
    pub mission: &'a mut WorldMission,
    pub grants: &'a LiveGrantContext<'g>,
    pub sound: SoundTeardownService<'a>,
}

/// The loader has no mission/grant or sound teardown loan. Preserve those
/// original lazy service boundaries while sharing the actual head traversal.
struct LoadDisposalDispatch;

impl DisposalDispatch for LoadDisposalDispatch {
    type Error = ObjectError;

    fn callback(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), ObjectError> {
        if matches!(arena.object(object)?.class, ObjectClass::Actor(_)) {
            return Err(ObjectError::ActorDisposalBoundary(object));
        }
        Ok(())
    }

    fn destroy(&mut self, arena: &mut ObjectArena, object: ObjectId) -> Result<(), ObjectError> {
        arena.destroy(super::ObjectDestructionRequest {
            object,
            sound: super::SoundTeardownService::RequiredAtReach,
        })?;
        Ok(())
    }
}

/// Loader state reset together at 0048DE64: flag 0054ECE4 = 1 and world
/// actor serial +3D0 = 0. 0048DC30 also zeroes the placed-pickup serial
/// 0054ED48 just before them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldLoadState {
    pub actor_serial_3d0: u32,
    /// Cleared by selector 88's 00443930; read with selector-12 byte +29D.
    pub flag_54ece4: bool,
    /// 0054ED48, which each placed pickup's 0046D7D0 takes and increments.
    pub pickup_serial_54ed48: u32,
    /// The loader's mission pickup words for 0048E930: the current world's
    /// index into W+B0, then W+B0..W+3CF as one span, since native's
    /// unbounded word index runs from the B0 array into W+240.
    pub pickup_words: Option<PickupCompletionWords>,
    /// Session byte G+1C3, set by a completed selector-39 target with +162.
    pub session_1c3: bool,
    /// The current world's four completion words (W+240), which a furniture
    /// marker's finish tests (0048EA80) during the placements.
    pub world_completion: Option<[u32; 4]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PickupCompletionWords {
    pub current_world: u32,
    pub words_b0_3cc: [u32; 200],
}

impl Default for WorldLoadState {
    fn default() -> Self {
        Self {
            actor_serial_3d0: 0,
            flag_54ece4: false,
            pickup_serial_54ed48: 0,
            pickup_words: None,
            session_1c3: false,
            world_completion: None,
        }
    }
}

impl WorldLoadState {
    /// 0048E930: the serial's bit in word `W+B0 + world*16 + (serial/32)*4`.
    /// Unknown words (no loader mission) or a span beyond W+3CF are errors.
    pub(super) fn pickup_collected(
        &self,
        object: ObjectId,
        serial: u32,
    ) -> Result<bool, ObjectError> {
        let words = self
            .pickup_words
            .ok_or(ObjectError::PickupMissionWordsUnknown(object))?;
        let index = (words.current_world as usize)
            .checked_mul(4)
            .and_then(|base| base.checked_add(serial as usize / 32))
            .filter(|&index| index < words.words_b0_3cc.len())
            .ok_or(ObjectError::PickupMissionWordsUnknown(object))?;
        Ok(words.words_b0_3cc[index] & 1 << (serial & 31) != 0)
    }
}

/// A load-time unlock's lower service failed.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum LoadUnlockError {
    #[error("actor {0:?} has a +68 attribution answer the port does not own")]
    AttributionPolicy(ObjectId),
    #[error("0047D670 attribution needs the mission's statistics record W+0C")]
    MissingStatistics,
    #[error("attribution of {actor:?}: {reason}")]
    Attribution { actor: ObjectId, reason: String },
    #[error("placement death bookkeeping: {0}")]
    PlacementDeath(String),
    #[error("an unlock to state 2 reached lifecycle effect {0}")]
    Effect(String),
}

/// 0047D670's services for a completed target's unlock at load (state 3 down
/// to 2): the +68 attribution test and +58 placement test, attribution into
/// the mission's statistics record, the selector-18 count and placement death
/// with the live W+08.
struct LoadUnlockHost<'m> {
    mission: &'m mut WorldMission,
}

impl super::actor::ActorStateChangeHost for LoadUnlockHost<'_> {
    type Error = LoadUnlockError;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: super::actor::ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        let extension = &arena
            .actor(actor)
            .map_err(|error| LoadUnlockError::Attribution {
                actor,
                reason: error.to_string(),
            })?
            .extension;
        match query {
            // Derived tables return themselves at +58; the 0040D800 family
            // and the shared table bind 00401300.
            super::actor::ActorStateChangeQuery::HasDerived58 => {
                Ok(matches!(extension, ActorExtension::Derived(_)))
            }
            // Shared, derived and selectors 15/30/37/39 bind the
            // null-returning 00401300 at +68.
            super::actor::ActorStateChangeQuery::ExcludesAttribution68 => match extension {
                ActorExtension::Shared
                | ActorExtension::Selector78(_)
                | ActorExtension::Derived(_)
                | ActorExtension::Selector53(_)
                | ActorExtension::Selector52(_)
                | ActorExtension::Selector73(_)
                | ActorExtension::Selector74(_)
                | ActorExtension::Selector61(_)
                | ActorExtension::Selector62(_)
                | ActorExtension::Selector84(_)
                | ActorExtension::Selector57
                | ActorExtension::Selector40(_)
                | ActorExtension::Selector41
                | ActorExtension::Selector44(_)
                | ActorExtension::Selector59(_)
                | ActorExtension::Selector18(_)
                | ActorExtension::Selector69(_)
                | ActorExtension::Selector15(_)
                | ActorExtension::Selector30(_)
                | ActorExtension::Selector37(_)
                | ActorExtension::Selector47(_)
                | ActorExtension::Selector49(_)
                | ActorExtension::Selector39(_) => Ok(false),
                _ => Err(LoadUnlockError::AttributionPolicy(actor)),
            },
        }
    }

    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: super::ActorHandle,
    ) -> Result<(), Self::Error> {
        let record = self
            .mission
            .record_0c
            .as_mut()
            .ok_or(LoadUnlockError::MissingStatistics)?;
        let attribution = arena
            .retirement_attribution(actor, source)
            .map_err(|error| LoadUnlockError::Attribution {
                actor,
                reason: error.to_string(),
            })?;
        crate::world_statistics::WorldStatistics::from_record(record)
            .record_attribution(attribution);
        Ok(())
    }

    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: super::actor::ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        match effect {
            super::actor::ActorStateChangeEffect::IncrementSelector18Statistic { owner: () } => {
                let record = self
                    .mission
                    .record_0c
                    .as_mut()
                    .ok_or(LoadUnlockError::MissingStatistics)?;
                crate::world_statistics::WorldStatistics::from_record(record)
                    .increment_selector18_retirements();
                Ok(())
            }
            super::actor::ActorStateChangeEffect::RecordPlacementDeath { index } => {
                let inhibit_byte_08 = self.mission.inhibit_byte_08;
                self.mission
                    .record_placement_death(crate::world_mission::PlacementDeathRequest {
                        index: i32::from(index),
                        inhibit_byte_08,
                        force: 0,
                    })
                    .map(|_| ())
                    .map_err(|error| LoadUnlockError::PlacementDeath(error.to_string()))
            }
            other => Err(LoadUnlockError::Effect(format!("{other:?}"))),
        }
    }
}

/// 0047D670 from state 2 up to the lock state 3 reaches no host callback;
/// any callback this host receives is an unowned downward change.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Error)]
#[error("a linked 0047D670 change needs the session's lifecycle services")]
pub struct LinkHostRequired;

impl super::actor::ActorStateChangeHost for LinkHostRequired {
    type Error = LinkHostRequired;
    type AttributionOwner = ();

    fn query(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: super::actor::ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        Err(LinkHostRequired)
    }

    fn attribute(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: super::ActorHandle,
    ) -> Result<(), Self::Error> {
        Err(LinkHostRequired)
    }

    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: super::actor::ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        Err(LinkHostRequired)
    }
}

#[derive(Debug, Error)]
pub enum WorldLoadError {
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Attach(#[from] PlayerAttachError),
    #[error(transparent)]
    Allocation(#[from] RuntimeAllocationError),
    #[error(transparent)]
    Cleanup(#[from] WorldCleanupError),
    #[error(transparent)]
    Actor(#[from] super::ActorConstructionError),
    #[error(transparent)]
    Derived(#[from] super::DerivedActorError),
    #[error(transparent)]
    StateChange(#[from] super::actor::ActorStateChangeError<LinkHostRequired>),
    #[error(transparent)]
    UnlockStateChange(#[from] super::actor::ActorStateChangeError<LoadUnlockError>),
    /// A completed target's message 8 releases a linked actor at load.
    /// 0047D670's downward state 2 needs the mission's attribution record,
    /// which only the retained-manager load lends.
    #[error(
        "linked actor {0:?} unlocks at load through 0047D670, which needs session attribution"
    )]
    LinkUnlockBoundary(super::ObjectId),
    /// A completed target's message 8 releases a barrier at load whose
    /// nonzero `+7A` would go to the first player's grants, which the load
    /// does not lend.
    #[error("barrier {0:?} releases at load with a grant for the first player")]
    BarrierGrantBoundary(super::ObjectId),
    #[error("selector-88 array pointer field {field:#x} lacks its exact relocation")]
    MissingArrayRelocation { field: u32 },
    #[error("selector-88 retained offset arithmetic overflows")]
    OffsetOverflow,
}

/// Set by 00401320 after its virtual +34 callback succeeds.
pub const AFTER_LOAD_FLAG: u32 = 0x0080_0000;

/// 0048EA80 on the selected world's four words. The signed index uses C
/// truncating division and x86's masked shift, so -31..-1 address word 0.
pub(super) fn world_completion_bit(completion: &[u32; 4], index: i8) -> bool {
    let index = i32::from(index);
    let quotient = index / 32;
    (0..4).contains(&quotient)
        && completion[quotient as usize] & 1_u32.wrapping_shl(index as u32 & 31) != 0
}

impl ObjectArena {
    pub fn world_load_state(&self) -> WorldLoadState {
        self.world_load
    }

    /// 0048DE64's resets immediately before the placement loop, with
    /// 0048DC30's earlier 0054ED48 = 0. The pickup words stay as supplied.
    pub fn begin_world_placements(&mut self) {
        self.world_load = WorldLoadState {
            actor_serial_3d0: 0,
            flag_54ece4: true,
            pickup_serial_54ed48: 0,
            pickup_words: self.world_load.pickup_words,
            session_1c3: self.world_load.session_1c3,
            world_completion: self.world_load.world_completion,
        };
    }

    /// The current world's completion words for this placement pass, which
    /// a furniture marker's finish (00435310) tests.
    pub fn set_world_completion(&mut self, completion: [u32; 4]) {
        self.world_load.world_completion = Some(completion);
    }

    /// The loader's current mission pickup words, read by placed pickups'
    /// 0046D7D0 during this placement pass.
    pub fn set_world_pickup_words(&mut self, words: PickupCompletionWords) {
        self.world_load.pickup_words = Some(words);
    }

    /// 0047F6C0. A negative placement word leaves the actor and serial alone.
    /// Otherwise +78 receives the serial's low byte before post-increment.
    /// A completed index calls virtual +54; a zero result disposes the actor
    /// through 00479FA0, which the post-placement tail then drains.
    pub fn register_loaded_actor(
        &mut self,
        object: ObjectId,
        world_completion: Option<&[u32; 4]>,
    ) -> Result<(), ObjectError> {
        self.actor(object)?;
        if (self.object(object)?.base.placement_word as i16) < 0 {
            return Ok(());
        }
        let world_completion =
            world_completion.ok_or(ObjectError::MissingWorldCompletion(object))?;
        let serial = self.world_load.actor_serial_3d0 as u8;
        self.world_load.actor_serial_3d0 = self.world_load.actor_serial_3d0.wrapping_add(1);
        self.actor_mut(object)?.byte_78 = serial;
        if world_completion_bit(world_completion, serial as i8)
            && !self.actor_completion_kept(object)?
        {
            self.dispose(object)?;
        }
        Ok(())
    }

    /// Actor virtual +54 over the represented final tables. The 0040D800
    /// family's 005289C0 (selector 30), 0051FF54 (37) and 00522398 (39) bind
    /// 00404610, which returns the actor, so a completed gate, bridge or
    /// target stays for its own +34. Every other represented actor table
    /// (shared 00527C00, derived base 00520E58 and selectors 2, 10, 11, 12,
    /// 16, 34, 35, 36, 53, 60, 66, 77, 88, 90 and 93) binds 00401300, which
    /// returns zero.
    pub fn actor_completion_kept(&self, actor: ObjectId) -> Result<bool, ObjectError> {
        Ok(matches!(
            self.actor(actor)?.extension,
            ActorExtension::Selector15(_)
                | ActorExtension::Selector30(_)
                | ActorExtension::Selector37(_)
                | ActorExtension::Selector47(_)
                | ActorExtension::Selector49(_)
                | ActorExtension::Selector39(_)
        ))
    }

    /// 0048DC30's post-placement object tail. Errors stop at the reached
    /// frontier with earlier callbacks, flags and destruction retained.
    /// Selector 88's callback requires the camera factory; without one it
    /// remains the explicit after-load boundary.
    pub fn finish_world_placements(
        &mut self,
        inputs: ObjectAfterLoadInputs<'_>,
        mut camera: Option<&mut CameraFactoryInputs<'_>>,
        disposal: Option<WorldLoadDisposal<'_, '_>>,
    ) -> Result<(), WorldLoadError> {
        self.finalize_routes()?;
        let mut mission = match disposal {
            Some(mut disposal) => {
                // A placement the save completed disposes its Force Field
                // here, whose destructor unbinds the camera factory's list.
                let listeners = camera.as_deref_mut().map(|camera| &mut *camera.listeners);
                self.drain_loaded_disposal(&mut disposal, listeners)?;
                Some(disposal.mission)
            }
            None => {
                self.drain_disposal()?;
                None
            }
        };
        for role in [ObjectList::Flag0Primary, ObjectList::Parking] {
            self.run_after_load_traversal(role, inputs, &mut camera, mission.as_deref_mut())?;
        }
        Ok(())
    }

    /// Exact +20 = 00401320: class +34, then flag 00800000.
    fn after_load_callback(
        &mut self,
        object: ObjectId,
        inputs: ObjectAfterLoadInputs<'_>,
        camera: &mut Option<&mut CameraFactoryInputs<'_>>,
        mut mission: Option<&mut WorldMission>,
    ) -> Result<(), WorldLoadError> {
        if self.selector39(object).is_ok() {
            let completion = inputs
                .world_completion
                .ok_or(ObjectError::MissingWorldCompletion(object))?;
            let reached = self.initialize_selector39_after_load(
                object,
                completion,
                &mut |arena, delivery| match delivery.change {
                    FamilyLinkChange::None => Ok(()),
                    FamilyLinkChange::Lock(request) => Ok(arena.change_actor_state(
                        delivery.target,
                        request,
                        &mut LinkHostRequired,
                    )?),
                    FamilyLinkChange::Unlock(request) => match mission.as_deref_mut() {
                        Some(mission) => Ok(arena.change_actor_state(
                            delivery.target,
                            request,
                            &mut LoadUnlockHost { mission },
                        )?),
                        None => Err(WorldLoadError::LinkUnlockBoundary(delivery.target)),
                    },
                    // 00411CE0's release: without a grant, or without a first
                    // player (00488AA0), it only disposes the barrier.
                    FamilyLinkChange::Release { grant } => {
                        if grant != 0 && arena.first_player().is_some() {
                            return Err(WorldLoadError::BarrierGrantBoundary(delivery.target));
                        }
                        Ok(arena.dispose(delivery.target)?)
                    }
                },
            )?;
            if reached.sets_session_1c3 {
                self.world_load.session_1c3 = true;
                // G+1C3 is the mission's +13, which the load cleared for H14
                // flag 0x1000 (0048DD6A): a target that died on an earlier
                // visit starts the world tint's schedule again, so Artoria
                // goes dark once more.
                if let Some(mission) = mission.as_deref_mut() {
                    mission.byte_13 = Some(1);
                }
            }
            return Ok(self.set_after_load_flag(object)?);
        }
        if self.selector84(object).is_ok() {
            // 0043C260 in place of the shared +34.
            self.initialize_selector84_after_load(object)?;
            return Ok(self.set_after_load_flag(object)?);
        }
        if self.selector79(object).is_ok() {
            // 004088C0 in place of selector 77's shared +34.
            self.initialize_selector79_after_load(object)?;
            return Ok(self.set_after_load_flag(object)?);
        }
        match (self.selector88(object).is_ok(), camera.as_deref_mut()) {
            (true, Some(camera)) => {
                self.initialize_selector88_after_load(object, camera)?;
                self.set_after_load_flag(object)?;
                Ok(())
            }
            _ => Ok(self.initialize_after_load(object, inputs)?),
        }
    }

    /// 00479840 over the loader's live owners: every disposed object's +30
    /// and destructor, in native head order. The first failure stops the load.
    fn drain_loaded_disposal(
        &mut self,
        disposal: &mut WorldLoadDisposal<'_, '_>,
        listeners: Option<&mut super::CreationListeners>,
    ) -> Result<(), WorldLoadError> {
        let mut state = ObjectDisposalState::default();
        // 0048DFDB..0048DFE4: the drain runs with W+08 set, so the actors the
        // load disposed (a placement condition's, a finished placement's)
        // record no placement deaths.
        disposal.mission.inhibit_byte_08 = 1;
        let drained = self.drain_object_disposal(ObjectDisposalRequest {
            mission: &mut *disposal.mission,
            grants: disposal.grants,
            sound: disposal.sound.reborrow(),
            state: &mut state,
            failure_policy: ObjectDisposalFailurePolicy::ReturnFirstError,
            listeners,
        });
        disposal.mission.inhibit_byte_08 = 0;
        match drained {
            // A just-loaded ship has no descriptor; a Dormant exit here has
            // no world services to run on.
            Ok(report) => match report.exit_pending {
                Some(ship) => Err(ObjectError::FamilyBehaviorDestructionBoundary(ship).into()),
                None => Ok(()),
            },
            Err(ObjectDisposalError::Failed { error, .. }) => Err(error.into()),
            Err(ObjectDisposalError::PreviouslyStopped(stop)) => {
                Err(ObjectError::StoppedDisposal {
                    object: stop.object,
                    phase: stop.phase,
                }
                .into())
            }
        }
    }

    /// 00479840 with the loader's existing lazy actor-service boundary.
    /// A load failure terminates this placement pass; the live world owns a
    /// persistent stop ledger and the full callback services separately.
    fn drain_disposal(&mut self) -> Result<(), ObjectError> {
        match self.drain_disposal_with(
            &mut LoadDisposalDispatch,
            &mut ObjectDisposalState::default(),
            ObjectDisposalFailurePolicy::ReturnFirstError,
        ) {
            Ok(report) => match report.exit_pending {
                Some(ship) => Err(ObjectError::FamilyBehaviorDestructionBoundary(ship)),
                None => Ok(()),
            },
            Err(ObjectDisposalError::Failed { error, .. }) => Err(error),
            Err(ObjectDisposalError::PreviouslyStopped(stop)) => {
                Err(ObjectError::StoppedDisposal {
                    object: stop.object,
                    phase: stop.phase,
                })
            }
        }
    }

    /// One inline 0048DC30 loop. Each unflagged node's +20 runs; if the
    /// node's next link then differs from the one cached before the call,
    /// the pass restarts at the current head. Flagged objects are skipped.
    fn run_after_load_traversal(
        &mut self,
        role: ObjectList,
        inputs: ObjectAfterLoadInputs<'_>,
        camera: &mut Option<&mut CameraFactoryInputs<'_>>,
        mut mission: Option<&mut WorldMission>,
    ) -> Result<(), WorldLoadError> {
        'restart: loop {
            let physical = self.list_owners[role as usize];
            let mut cursor = self.lists[physical].first().copied();
            while let Some(node) = cursor {
                let cached = self.next_node(node, physical)?;
                if self.object(node.object)?.base.flags & AFTER_LOAD_FLAG == 0 {
                    self.after_load_callback(node.object, inputs, camera, mission.as_deref_mut())?;
                    let membership = self.owned(node.object)?.memberships[node.kind as usize];
                    if self.next_node(node, membership)? != cached {
                        continue 'restart;
                    }
                }
                cursor = cached;
            }
            return Ok(());
        }
    }

    fn next_node(
        &self,
        node: ObjectNode,
        physical: usize,
    ) -> Result<Option<ObjectNode>, ObjectError> {
        let list = &self.lists[physical];
        let index = list
            .iter()
            .position(|candidate| *candidate == node)
            .ok_or(ObjectError::UnknownObject(node.object))?;
        Ok(list.get(index + 1).copied())
    }
}

#[cfg(test)]
#[path = "world_load_tests.rs"]
mod tests;
