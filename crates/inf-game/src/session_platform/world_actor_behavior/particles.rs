//! Canonical current component leaves and real predicate/particle composition.
use super::WorldFamilyBehaviorError;
use crate::{
    frontend_camera::AnimationClockSource,
    object_runtime::{
        ActorExtension, BehaviorActorResourceRequest, BehaviorComponentPacketSourceRequest,
        BehaviorComponentQuery, BehaviorPacketCopy, BehaviorParticleEffectRequest,
        BehaviorParticleServiceContext, BehaviorParticleServices, BehaviorPredicateContext,
        DerivedActorClass, Family428650Class, Family429e20Class, FamilySelectionServices,
        ObjectArena, ObjectClass, ObjectId,
    },
    process_tasks::ProcessTaskRuntime,
    session_platform::world_actor_factory::BehaviorResourceFactory,
};

pub(crate) struct WorldSelectionServices<'a> {
    pub tasks: &'a mut ProcessTaskRuntime,
    pub animation_clock: &'a dyn AnimationClockSource,
    pub session: &'a crate::session::SessionState,
    /// 00460310 for registry resources; None where no world factory is lent.
    pub resources: Option<&'a mut dyn BehaviorResourceFactory>,
}
impl FamilySelectionServices for WorldSelectionServices<'_> {
    fn process_tasks(&mut self) -> &mut ProcessTaskRuntime {
        self.tasks
    }
    fn animation_clock(&self) -> Option<&dyn AnimationClockSource> {
        Some(self.animation_clock)
    }
}
impl BehaviorParticleServices for WorldSelectionServices<'_> {
    type Error = WorldFamilyBehaviorError;
    type Component = ObjectId;

    fn current_phase(&mut self) -> Result<Option<u32>, Self::Error> {
        Ok(Some(u32::from(self.session.current_phase().id())))
    }

    fn create_actor_resource(
        &mut self,
        arena: &mut ObjectArena,
        context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorActorResourceRequest,
    ) -> Result<Option<ObjectId>, Self::Error> {
        // The eight scratch bytes are unwritten: 00479D40 copies the first
        // six to the new object's position, which therefore stays unknown,
        // and nothing reads the last two. Any known byte would be a
        // different source than the one the factory below models.
        let factory = match (&mut self.resources, request.scratch_bytes) {
            (Some(factory), [None, None, None, None, None, None, None, None]) => factory,
            _ => {
                return Err(WorldFamilyBehaviorError::RequiredActorResource460310 {
                    resource: request.resource,
                    scratch_bytes: request.scratch_bytes,
                })
            }
        };
        let _ = context;
        factory
            .create_behavior_resource(arena, request.resource)
            .map_err(|reason| WorldFamilyBehaviorError::BehaviorResourceFactory {
                resource: request.resource,
                reason,
            })
    }
    fn query_component(
        &mut self,
        arena: &mut ObjectArena,
        _context: &mut BehaviorPredicateContext<'_>,
        request: BehaviorComponentQuery<ObjectId>,
    ) -> Result<Option<ObjectId>, Self::Error> {
        match request {
            BehaviorComponentQuery::Actor18(actor) => {
                //All admitted actor final tables use404610 at+18. Nonactors
                //use401300. This is unrelated to derived-component+58.
                Ok(match &arena.object(actor)?.class {
                    ObjectClass::Actor(_) => Some(actor),
                    ObjectClass::Base
                    | ObjectClass::Selector81(_)
                    | ObjectClass::Selector94(_)
                    | ObjectClass::Selector75(_)
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
                    | ObjectClass::RotatingVisual(_)
                    | ObjectClass::Selector45(_)
                    | ObjectClass::Selector46(_)
                    | ObjectClass::Race(_)
                    | ObjectClass::Selector58(_)
                    | ObjectClass::Selector83(_)
                    | ObjectClass::Selector76(_)
                    | ObjectClass::TimedPickup(_)
                    | ObjectClass::TerrainLight { .. }
                    | ObjectClass::RotatingTerrainLight { .. }
                    | ObjectClass::RouteNode { .. } => None,
                })
            }
            BehaviorComponentQuery::Component68(component)
            | BehaviorComponentQuery::Component6c(component) => {
                //Only the registry's resource classes answer either slot.
                //Keep exhaustive class admission: future concrete
                //interfaces must not inherit an accidental NULL default.
                match &arena.actor(component)?.extension {
                    // +6C on 00485F50's pair, +68 on 00455560's family:
                    // 00404610, the actor itself.
                    ActorExtension::BehaviorResource(state) => {
                        let answers = match request {
                            BehaviorComponentQuery::Component6c(_) => state.component_6c(),
                            BehaviorComponentQuery::Component68(_) => state.component_68(),
                            BehaviorComponentQuery::Actor18(_) => unreachable!(),
                        };
                        return Ok(answers.then_some(component));
                    }
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
                    | ActorExtension::Selector66(_)
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
                    | ActorExtension::Selector93(_) => {}
                    ActorExtension::Derived(state) => match &state.class {
                        DerivedActorClass::Base
                        | DerivedActorClass::Selector2(_)
                        | DerivedActorClass::Selector11(_)
                        | DerivedActorClass::Selector60(_)
                        | DerivedActorClass::Selector77(_) => {}
                        DerivedActorClass::Family428650(state) => match &state.class {
                            Family428650Class::Base
                            | Family428650Class::Selector34(_)
                            | Family428650Class::Selector35(_) => {}
                            Family428650Class::Family429e20(state) => match &state.class {
                                Family429e20Class::Base
                                | Family429e20Class::Family4236d0(_)
                                | Family429e20Class::Selector12(_) => {}
                            },
                        },
                    },
                }
                Ok(None)
            }
        }
    }
    fn component_packet_source(
        &mut self,
        arena: &ObjectArena,
        _context: &BehaviorPredicateContext<'_>,
        request: BehaviorComponentPacketSourceRequest<ObjectId>,
    ) -> Result<Option<BehaviorPacketCopy>, Self::Error> {
        let (component, offset) = match request {
            BehaviorComponentPacketSourceRequest::Channel148(source) => (source, 0x148),
            BehaviorComponentPacketSourceRequest::Strength146(source) => (source, 0x146),
            BehaviorComponentPacketSourceRequest::Complete13e(source) => (source, 0x13e),
        };
        // 004863D0 reads BYTE +148 then WORD +146; 00458F50 the DWORD +13E.
        if let Ok(state) = arena.behavior_resource(component) {
            let copy = match request {
                BehaviorComponentPacketSourceRequest::Channel148(_) => {
                    state.packet_channel_148().map(BehaviorPacketCopy::Byte)
                }
                BehaviorComponentPacketSourceRequest::Strength146(_) => {
                    state.packet_strength_146().map(BehaviorPacketCopy::Word)
                }
                BehaviorComponentPacketSourceRequest::Complete13e(_) => {
                    state.packet_13e().map(BehaviorPacketCopy::Full)
                }
            };
            if let Some(copy) = copy {
                return Ok(Some(copy));
            }
        }
        //The canonical leaves above bypass these reads for admitted classes.
        //A reached future packet interface still requires its actual backing.
        Err(WorldFamilyBehaviorError::RequiredPacketSource { component, offset })
    }
    fn particle_effect(
        &mut self,
        _context: BehaviorParticleServiceContext<'_, '_, '_>,
        request: BehaviorParticleEffectRequest<'_>,
    ) -> Result<(), Self::Error> {
        Err(WorldFamilyBehaviorError::RequiredParticleEffect {
            phase: match request {
                BehaviorParticleEffectRequest::NetworkNotice { .. } => "creation network notice",
                BehaviorParticleEffectRequest::Impact489600(_) => "00489600 impact",
            },
        })
    }
}
