//! Complete 00478CD0 world-manager drain, retaining the actual player FIFO.
//!
//! The supplied mission belongs to both manager+70 and current global game in
//! this stable canonical domain. Each selected embedded node is reread after
//! its authentic +30, then the current head is selected again after destruction.
//! Raw NULL/replaced node payloads, owner rebinding and arbitrary virtual
//! overrides are outside the typed domain. Errors retain inhibit1, all earlier
//! callbacks/deletions and class-destructor prefixes; this is not resumable,
//! but for `DestructionExitPending`: a destructor exit that needs world
//! services pauses the walk after its object's +30, and `cleanup_world`
//! resumes it at that object's destructor.
//! The retained-player child composes the actual post-effects 0048E890 suffix
//! without owning its earlier map/effects or later deleting-caller phases.

use thiserror::Error;

use super::{
    ActorExtension, ActorVehicleCleanupNotificationError, ActorVehicleCleanupNotificationRequest,
    DerivedActorClass, DerivedActorError, Family4236d0Class, Family428650Class, Family429e20Class,
    GrantOwner, LiveGrantContext, LiveGrantError, LiveGrantOperation, LiveGrantRequest,
    ObjectArena, ObjectClass, ObjectError, ObjectId, ObjectList, ObjectNode, ObjectNodeKind,
    PlayerActorLookupContext, PlayerAttachError, PlayerId, PlayerMessageError,
};
use crate::world_mission::{PlacementDeathRequest, WorldMission, WorldMissionError};

/// Actual current owners; missing process/grant data fails only when reached.
/// Keep this same immutable source loan through all callbacks. No player,
/// registry, input, source table or allocation owner is replaced by this pass.
pub struct WorldCleanupRequest<'a, 'g> {
    pub mission: &'a mut WorldMission,
    pub grants: &'a LiveGrantContext<'g>,
    pub sound: super::SoundTeardownService<'a>,
    /// The bindings a Force Field's destructor 004768D0 removes.
    pub listeners: &'a mut super::CreationListeners,
    /// The world's terrain and furniture loans a furniture marker's +30
    /// (00435360) removes from. None stops at a marker.
    pub furniture: Option<crate::world_furniture_effect::MarkerFurniture<'a>>,
}

#[derive(Debug, Error)]
pub enum WorldCleanupError {
    #[error(transparent)]
    Selector66(#[from] super::Selector66Error<std::convert::Infallible>),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Derived(#[from] DerivedActorError),
    #[error(transparent)]
    Vehicle(#[from] ActorVehicleCleanupNotificationError),
    #[error(transparent)]
    Grant(#[from] LiveGrantError),
    #[error(transparent)]
    Mission(#[from] WorldMissionError),
    #[error(transparent)]
    Selector75(#[from] super::Selector75Error),
    #[error(
        "furniture marker {0:?}'s cleanup 00435360 needs the world's terrain and furniture loans"
    )]
    Selector75Furniture(ObjectId),
    #[error(transparent)]
    Player(#[from] PlayerAttachError),
    #[error(transparent)]
    Message(#[from] PlayerMessageError),
    #[error("player {0:?} retirement requires the second 0046AE60 controlled actor")]
    RequiredSecondControlledActor(PlayerId),
    #[error("actor {0:?} cleanup requires current full network DWORD0055F448")]
    MissingProcess(ObjectId),
    #[error("selector35 {actor:?} cleanup byte+{offset:03X} is unknown")]
    UnknownSelector35Byte { actor: ObjectId, offset: u16 },
    #[error("cleanup's selected node for {object:?}, kind {kind:?}, is unavailable")]
    SelectedNodeUnavailable {
        object: ObjectId,
        kind: Option<ObjectNodeKind>,
    },
    #[error("selector35 cleanup's captured player node {0:?} was removed")]
    RemovedPlayerNode(PlayerId),
    /// 0043D0F0's Dormant exit 0043E120 bursts and radiates even on false,
    /// with world services the arena does not hold. The walk paused after
    /// the ship's +30; the caller runs the exit, then resumes.
    #[error("selector35 {0:?}'s destructor exit 0043E120 needs the world's burst and radius")]
    DestructionExitPending(ObjectId),
    #[error("the cleanup walk paused at {paused:?} but resumed at {head:?}")]
    ResumedElsewhere { paused: ObjectId, head: ObjectId },
    #[error("selector35 {actor:?}'s destructor exit 0043E120 stopped: {reason}")]
    DestructionExit { actor: ObjectId, reason: String },
}

#[derive(Clone, Copy)]
enum SelectedNode {
    Actor(ObjectId),
    Common(ObjectNode),
}

#[derive(Clone, Copy)]
enum Callback {
    Selector66,
    NoOp,
    Common,
    Vehicle,
    Selector35,
    Selector75,
}

impl ObjectArena {
    /// Actor FIFO -> flag1 pair -> flag0 pair -> disposal -> parked. Every
    /// class callback precedes its deleting destructor, including stopped
    /// lower destructors. Player FIFO G+60 and its components remain live.
    pub fn cleanup_world(
        &mut self,
        mut request: WorldCleanupRequest<'_, '_>,
    ) -> Result<(), WorldCleanupError> {
        request.mission.inhibit_byte_08 = 1; // 00478CD7, before any head read.
        while let Some(&actor) = self.actor_list().first() {
            self.cleanup_selected_node(SelectedNode::Actor(actor), &mut request)?;
        }
        // Actual header custody at the reached phase. Canonical callbacks do
        // not replace/swap these headers; only their current nodes change.
        let headers = [
            ObjectList::Flag1Primary,
            ObjectList::Flag1Secondary,
            ObjectList::Flag0Primary,
            ObjectList::Flag0Secondary,
            ObjectList::Disposal,
            ObjectList::Parking,
        ]
        .map(|role| self.list_owners[role as usize]);
        for physical in headers {
            while let Some(&node) = self.lists[physical].first() {
                self.cleanup_selected_node(SelectedNode::Common(node), &mut request)?;
            }
        }
        request.mission.inhibit_byte_08 = 0; // 00478DDF/DEC, normal exits only.
        Ok(())
    }

    fn cleanup_selected_node(
        &mut self,
        node: SelectedNode,
        request: &mut WorldCleanupRequest<'_, '_>,
    ) -> Result<(), WorldCleanupError> {
        let object = match node {
            SelectedNode::Actor(object) => object,
            SelectedNode::Common(node) => node.object,
        };
        match self.cleanup_exit_pending.take() {
            // Resumed: this object's +30 ran before the pause.
            Some(paused) if paused == object => {}
            Some(paused) => {
                return Err(WorldCleanupError::ResumedElsewhere {
                    paused,
                    head: object,
                })
            }
            None => self.cleanup_object_callback(
                object,
                request.mission,
                request.grants,
                request.furniture.as_mut(),
            )?,
        }
        // Read the captured node's actual current entry, not the fresh head or
        // a snapshot of the next node. Canonical payloads are immutable IDs;
        // missing/replaced raw storage is a typed boundary, not NULL success.
        let object = self.cleanup_selected_payload(node)?;
        if self.selector35_exit_needs_world_effects(object) {
            self.cleanup_exit_pending = Some(object);
            return Err(WorldCleanupError::DestructionExitPending(object));
        }
        if self.selector18(object).is_ok() || self.selector78(object).is_ok() {
            request.listeners.unregister_owner(object);
        }
        self.destroy(crate::object_runtime::ObjectDestructionRequest {
            object,
            sound: request.sound.reborrow(),
        })?;
        Ok(())
    }

    fn cleanup_selected_payload(&self, node: SelectedNode) -> Result<ObjectId, WorldCleanupError> {
        let (object, kind) = match node {
            SelectedNode::Actor(object) => (object, None),
            SelectedNode::Common(node) => (node.object, Some(node.kind)),
        };
        let owned = self.owned(object)?;
        let payload = match node {
            SelectedNode::Actor(object) => {
                self.actor_list().iter().find(|&&id| id == object).copied()
            }
            SelectedNode::Common(node) => self.lists[owned.memberships[node.kind as usize]]
                .iter()
                .find(|&&current| current == node)
                .map(|current| current.object),
        };
        payload.ok_or(WorldCleanupError::SelectedNodeUnavailable { object, kind })
    }

    pub(super) fn cleanup_object_callback(
        &mut self,
        object: ObjectId,
        mission: &mut WorldMission,
        grants: &LiveGrantContext<'_>,
        furniture: Option<&mut crate::world_furniture_effect::MarkerFurniture<'_>>,
    ) -> Result<(), WorldCleanupError> {
        self.ensure_disposal_active(object)?;
        match callback(&self.object(object)?.class) {
            Callback::Selector66 => return self.cleanup_selector66(object).map_err(Into::into),
            Callback::NoOp => return Ok(()),
            // 00435360 never reaches the common 0047EC50.
            Callback::Selector75 => {
                let furniture = furniture.ok_or(WorldCleanupError::Selector75Furniture(object))?;
                self.cleanup_selector75(object, furniture, mission)?;
                return Ok(());
            }
            Callback::Vehicle => {
                // 00415BA9 -> 0047DD20(0,10,0), before common47EC50.
                self.notify_actor_vehicle_cleanup(
                    ActorVehicleCleanupNotificationRequest { source: object },
                    PlayerActorLookupContext {
                        process: grants.process,
                    },
                )?;
            }
            Callback::Selector35 => self.cleanup_selector35_grants(object, grants)?,
            Callback::Common => {}
        }
        let network = grants
            .process
            .ok_or(WorldCleanupError::MissingProcess(object))?
            .network_flag;
        if network != 0 && self.object(object)?.base.flags & 0x8000_0000 == 0 {
            let state = self.actor(object)?.word_c4;
            let event = match state {
                0 => Some(6_u32),
                1 => Some(5),
                2 => None,
                3 => Some(4),
                4 => Some(3),
                _ => None, // Original unsigned DWORD gate >4.
            };
            // 0047EC50: the others hear of a local actor's end by its state.
            if let Some(event) = event {
                let handle = self.actor(object)?.handle;
                self.queue_network_send(
                    event,
                    crate::network::protocol::GameSend::Dword(handle.raw()),
                );
            }
        }
        let index = i32::from(self.actor(object)?.byte_78 as i8);
        let inhibit = mission.inhibit_byte_08;
        mission.record_placement_death(PlacementDeathRequest {
            index,
            inhibit_byte_08: inhibit,
            force: 0,
        })?; // 0047ECCA, including -1's early return and live inhibit read.
        Ok(())
    }

    /// `0043D160`, selector 35's cleanup before the shared tail `0047EC50`:
    /// each player takes grant +7A while `0 < +242 <= +243`.
    fn cleanup_selector35_grants(
        &mut self,
        actor: ObjectId,
        context: &LiveGrantContext<'_>,
    ) -> Result<(), WorldCleanupError> {
        let first = self.selector35_actor(actor)?.bytes_240[2].ok_or(
            WorldCleanupError::UnknownSelector35Byte {
                actor,
                offset: 0x242,
            },
        )?;
        if first == 0 {
            return Ok(()); // 0043D16C bypasses +243, +7A and player FIFO.
        }
        let second = self.selector35_actor(actor)?.bytes_240[3].ok_or(
            WorldCleanupError::UnknownSelector35Byte {
                actor,
                offset: 0x243,
            },
        )?;
        if first > second || self.actor(actor)?.word_7a == 0 {
            return Ok(());
        }
        let mut index = 0;
        while let Some(player) = self.world_players().get(index).map(|player| player.id) {
            // 0043D18E rereads current WORD+7A for each actual player.
            let id = u32::from(self.actor(actor)?.word_7a);
            self.mutate_live_grants(
                LiveGrantRequest {
                    target: GrantOwner::Player9c(player),
                    operation: LiveGrantOperation::SetRaw { id },
                },
                context,
            )?;
            index = self
                .world_players()
                .iter()
                .position(|current| current.id == player)
                .ok_or(WorldCleanupError::RemovedPlayerNode(player))?
                + 1;
        }
        Ok(())
    }
}

/// All represented final classes, including selector66's and the furniture
/// marker's (selector 75) distinct +30 bodies. Mutable B4 or resource/header
/// words do not bind +30.
fn callback(class: &ObjectClass) -> Callback {
    match class {
        ObjectClass::Base
        | ObjectClass::Selector81(_)
        | ObjectClass::Selector94(_)
        | ObjectClass::Selector86(_)
        | ObjectClass::Selector67(_)
        | ObjectClass::MapAnnotation(_)
        | ObjectClass::Visual(_)
        | ObjectClass::Selector22(_)
        | ObjectClass::Selector65(_)
        | ObjectClass::Selector50(_)
        | ObjectClass::Selector48(_)
        | ObjectClass::Selector43(_)
        | ObjectClass::Selector42(_)
        | ObjectClass::Selector64(_)
        | ObjectClass::TimedPickup(_)
        | ObjectClass::RotatingVisual(_)
        | ObjectClass::Selector45(_)
        | ObjectClass::Selector46(_)
        | ObjectClass::Race(_)
        | ObjectClass::Selector58(_)
        | ObjectClass::Selector83(_)
        | ObjectClass::Selector76(_)
        | ObjectClass::TerrainLight { .. }
        | ObjectClass::RotatingTerrainLight { .. }
        | ObjectClass::RouteNode { .. } => Callback::NoOp,
        ObjectClass::Selector75(_) => Callback::Selector75,
        ObjectClass::Actor(actor) => match &actor.extension {
            ActorExtension::Selector66(_) => Callback::Selector66,
            ActorExtension::Derived(derived) => match &derived.class {
                DerivedActorClass::Selector2(_) => Callback::Vehicle,
                DerivedActorClass::Family428650(family) => match &family.class {
                    Family428650Class::Selector35(_) => Callback::Selector35,
                    Family428650Class::Family429e20(probe) => match &probe.class {
                        Family429e20Class::Family4236d0(family) => match &family.class {
                            Family4236d0Class::Base
                            | Family4236d0Class::Selector10(_)
                            | Family4236d0Class::Selector36(_) => Callback::Common,
                        },
                        Family429e20Class::Base | Family429e20Class::Selector12(_) => {
                            Callback::Common
                        }
                    },
                    Family428650Class::Base | Family428650Class::Selector34(_) => Callback::Common,
                },
                DerivedActorClass::Base
                | DerivedActorClass::Selector11(_)
                | DerivedActorClass::Selector60(_)
                | DerivedActorClass::Selector77(_) => Callback::Common,
            },
            ActorExtension::Shared
            | ActorExtension::Selector78(_)
            | ActorExtension::Selector61(_)
            | ActorExtension::Selector62(_)
            | ActorExtension::Selector16(_)
            | ActorExtension::Selector30(_)
            | ActorExtension::Selector15(_)
            | ActorExtension::Selector37(_)
            | ActorExtension::Selector47(_)
            | ActorExtension::Selector49(_)
            | ActorExtension::Selector39(_)
            | ActorExtension::Selector88(_)
            | ActorExtension::Selector90(_)
            | ActorExtension::Selector53(_)
            | ActorExtension::Selector52(_)
            | ActorExtension::Selector73(_)
            | ActorExtension::Selector55(_)
            | ActorExtension::Selector17(_)
            | ActorExtension::Selector56
            | ActorExtension::Selector74(_)
            | ActorExtension::Selector84(_)
            | ActorExtension::Selector57
            | ActorExtension::Selector40(_)
            | ActorExtension::Selector41
            | ActorExtension::Selector44(_)
            | ActorExtension::Selector59(_)
            | ActorExtension::Selector18(_)
            | ActorExtension::Selector69(_)
            | ActorExtension::Selector93(_)
            | ActorExtension::BehaviorResource(_) => Callback::Common,
        },
    }
}

#[cfg(test)]
mod tests;

mod retained_player;
