use super::*;
use crate::object_runtime::{
    ActorChildFactoryCall, ActorChildHost, ActorChildResource, ObjectArena,
};
use crate::session_platform::world_actor_factory::{RequiredListenerEffects, WorldActorChildHost};

fn loaded() -> PortSession<Recording> {
    let (mut session, _) = fixture();
    let mut state = SessionState::default();
    session.prepare_offline_player(&mut state).unwrap();
    session
        .transition_effect(TransitionEffect::LoadWorld, &mut state)
        .unwrap();
    session
}

fn with_host(
    session: &mut PortSession<Recording>,
    action: impl FnOnce(&mut ObjectArena, &mut WorldActorChildHost<'_, '_>, &mut u32),
) {
    let mut effects = RequiredListenerEffects;
    with_host_effects(session, &mut effects, action);
}

fn with_host_effects(
    session: &mut PortSession<Recording>,
    effects: &mut dyn crate::object_runtime::CreationListenerEffects,
    action: impl FnOnce(&mut ObjectArena, &mut WorldActorChildHost<'_, '_>, &mut u32),
) {
    let assets = session.catalog.assets();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(assets.sine_table().values);
    let world = session.world.as_mut().unwrap();
    let mut host = WorldActorChildHost {
        residency: &session.residency,
        executable,
        process: &session.factory_process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: session.process.animation().value(),
        sine: &assets.sine_table().values,
        math: &math,
        camera_settings: Some(session.platform.world_tick_inputs().camera_settings),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: effects,
    };
    action(&mut world.arena, &mut host, &mut world.rng);
}

#[derive(Default)]
struct ObserveConstructorNull {
    seen: Vec<crate::object_runtime::ActorHandle>,
}

impl crate::object_runtime::CreationListenerEffects for ObserveConstructorNull {
    fn invoke(
        &mut self,
        listener: crate::object_runtime::CreationListener,
        child: crate::object_runtime::ObjectId,
        _: &mut crate::object_runtime::CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), crate::object_runtime::CreationListenerError> {
        self.seen.push(arena.actor(child)?.owner_handle);
        assert_eq!(arena.object(child)?.base.flags & 0x10_0000, 0);
        Err(crate::object_runtime::CreationListenerError::UnsupportedInterface(listener.interface))
    }
}

#[test]
fn selector36_child_listener_and_retained_prefix_use_current_process_null() {
    use crate::object_runtime::{
        ActorHandle, CreationListenerInterface, Selector36ChildServiceError,
    };
    let mut session = loaded();
    let null = ActorHandle::from_raw(0x7654_3210);
    session.factory_process.process_null_handle = Some(null);
    let mut effects = ObserveConstructorNull::default();
    with_host_effects(&mut session, &mut effects, |arena, host, rng| {
        assert_ne!(arena.actor_handles().unwrap().null_handle(), null);
        let parent = arena
            .actor_list()
            .iter()
            .copied()
            .find(|&id| arena.selector36_actor(id).is_ok())
            .unwrap();
        let payload = arena
            .selector36_actor(parent)
            .unwrap()
            .payload_274
            .as_ref()
            .unwrap();
        let bytes = payload.allocation().bytes();
        let start = payload.allocation_offset as usize + 0x78;
        let resource = u16::from_le_bytes(bytes.get(start..start + 2).unwrap().try_into().unwrap());
        host.listeners
            .register(parent, CreationListenerInterface::Selector18);
        let error = host
            .create_child_object(
                arena,
                ActorChildFactoryCall {
                    parent,
                    resource: ActorChildResource::StackPositionLowWord(resource),
                    spawn_mode_1: false,
                },
                rng,
            )
            .unwrap_err();
        let Selector36ChildServiceError::RetainedConstruction { object, .. } = error else {
            panic!("listener failure must retain the newly allocated child")
        };
        assert_eq!(arena.actor(object).unwrap().owner_handle, null);
        assert_eq!(arena.object(object).unwrap().base.flags & 0x10_0000, 0);
        assert!(arena.construction_is_stopped(object).unwrap());
    });
    assert_eq!(effects.seen, [null]);
}

#[test]
fn selector36_child_live_factory_retains_child_and_changes_parent_model() {
    let mut session = loaded();
    with_host(&mut session, |arena, host, rng| {
        let parent = arena
            .actor_list()
            .iter()
            .copied()
            .find(|id| {
                arena.selector36_actor(*id).is_ok()
                    && arena.actor(*id).unwrap().word_b4 != 0
                    && arena.family4236d0_actor(*id).unwrap().word_264 & 0x1000 == 0
            })
            .expect("loaded intro selector36 actor");
        let payload = arena
            .selector36_actor(parent)
            .unwrap()
            .payload_274
            .as_ref()
            .unwrap();
        let payload_bytes = payload.allocation().bytes();
        let read_word = |offset: usize| {
            let start = payload.allocation_offset as usize + offset;
            u16::from_le_bytes(
                payload_bytes
                    .get(start..start + 2)
                    .unwrap()
                    .try_into()
                    .unwrap(),
            )
        };
        let model = read_word(0x76);
        let resource = read_word(0x78);
        let before = arena.actor_list().to_vec();
        let source = arena.actor(parent).unwrap().handle;
        let classification = arena.actor(parent).unwrap().word_b4;
        let basis = arena.actor(parent).unwrap().rotating.basis;
        arena
            .spawn_selector36_child(parent, host, rng)
            .unwrap_or_else(|error| panic!("actual child H13/{resource} failed: {error:?}"));
        let child = arena
            .actor_list()
            .iter()
            .copied()
            .find(|id| !before.contains(id))
            .expect("actual child actor remains published");
        assert_eq!(arena.actor_list().len(), before.len() + 1);
        let state = arena.actor(child).unwrap();
        assert_eq!(state.owner_handle, source);
        assert_eq!(state.word_b4, classification);
        assert_eq!(state.rotating.basis, basis);
        assert_eq!(arena.object(child).unwrap().base.resource_word, resource);
        assert_eq!(arena.object(child).unwrap().base.placement_word, u16::MAX);
        assert_ne!(arena.object(child).unwrap().base.flags & 0x10_0000, 0);
        assert_eq!(arena.object(child).unwrap().base.dirty_word & 6, 6);
        assert_eq!(
            arena.actor(parent).unwrap().rotating.visual.model_word,
            model
        );
        assert_eq!(
            arena.family4236d0_actor(parent).unwrap().word_264 & 0x1800,
            0x1000
        );
        assert_eq!(arena.actor(parent).unwrap().word_74, 0);
        assert_eq!(arena.selector36_actor(parent).unwrap().word_27e, u16::MAX);
        //Normal completion's1000 gate prevents another admission.
        arena.spawn_selector36_child(parent, host, rng).unwrap();
        assert_eq!(arena.actor_list().len(), before.len() + 1);
    });
}

#[test]
fn selector36_child_factory_resource_reject_precedes_parent_read() {
    let mut session = loaded();
    session.factory_process.network_flag = 1;
    with_host(&mut session, |arena, host, rng| {
        let parent = arena.allocate(crate::object_runtime::ObjectClassKind::Base);
        assert_eq!(arena.object(parent).unwrap().base.position_words, None);
        let count = arena.len();
        assert_eq!(
            host.create_child_object(
                arena,
                ActorChildFactoryCall {
                    parent,
                    resource: ActorChildResource::Full(0xffff),
                    spawn_mode_1: false,
                },
                rng
            )
            .unwrap(),
            None
        );
        assert_eq!(arena.len(), count);
    });
}

/// A launcher's shot takes the launcher's own basis: selector 11's final
/// table 0051F080 binds +48 to 004026B0 (`lea eax,[ecx+0x44]`), as every
/// shooter's does. Admitting only selectors 2 and 36 failed a SAM's or
/// grenade launcher's first missile, and its stopped firing node then
/// blocked every later shot. A parent outside the shooters stays refused.
#[test]
fn a_launcher_lends_its_own_basis_to_the_shot_it_builds() {
    let mut session = loaded();
    with_host(&mut session, |arena, host, _| {
        let actors: Vec<_> = arena.actor_list().to_vec();
        let turret = actors
            .iter()
            .copied()
            .find(|&actor| arena.selector11_actor(actor).is_ok())
            .expect("the world builds selector-11 turrets");
        let basis = arena.actor(turret).unwrap().rotating.basis;
        assert_eq!(host.child_source_basis(arena, turret).unwrap(), Some(basis));
        let other = actors
            .iter()
            .copied()
            .find(|&actor| {
                arena.selector2_actor(actor).is_err()
                    && arena.selector10_actor(actor).is_err()
                    && arena.selector11_actor(actor).is_err()
                    && arena.selector12_actor(actor).is_err()
                    && arena.selector35_actor(actor).is_err()
                    && arena.selector36_actor(actor).is_err()
            })
            .expect("the world builds other actors");
        assert!(host.child_source_basis(arena, other).is_err());
    });
}
