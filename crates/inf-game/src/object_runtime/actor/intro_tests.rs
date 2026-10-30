use super::*;
use crate::intro_sequence::{IntroAllocationWords, IntroHost};
use crate::world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime};
use crate::{frontend_assets::FrontendAssets, Installation, ResidencyRequest};
use inf_formats::{native_basis::NativeBasis, parse_constructor_asset_bank, FrontendExecutable};

#[test]
fn flag1_pass_observes_same_pass_admission_and_membership_changes() {
    use crate::object_runtime::{ObjectClassKind, ObjectList};
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::Base);
    let removed = arena.allocate(ObjectClassKind::Base);
    let mut admitted = None;
    let mut visits = Vec::new();
    arena
        .visit_flag1_update_callbacks(|arena, object, alternate| {
            assert!(!alternate);
            assert!(arena
                .list(ObjectList::Flag1Secondary)
                .iter()
                .any(|node| node.object == object));
            visits.push(object);
            if object == first {
                arena.set_flag1(removed, false).unwrap();
                admitted = Some(arena.allocate(ObjectClassKind::Base));
            }
        })
        .unwrap();
    let admitted = admitted.unwrap();
    assert_eq!(visits, [first, admitted]);
    // Each visited node went to the front, so the next pass runs backwards.
    assert_eq!(
        arena
            .list(ObjectList::Flag1Primary)
            .iter()
            .map(|node| node.object)
            .collect::<Vec<_>>(),
        [admitted, first]
    );
    assert!(arena.list(ObjectList::Flag1Secondary).is_empty());
    assert_eq!(arena.object(removed).unwrap().base.flags & 2, 0);
}

/// 00478F20 inserts each visited +10 node at the alternate list's front,
/// so consecutive passes alternate direction, as Infest01's turrets do.
#[test]
fn flag1_passes_alternate_direction_like_00478f20() {
    use crate::object_runtime::ObjectClassKind;
    let mut arena = ObjectArena::new();
    let objects = [(); 4].map(|_| arena.allocate(ObjectClassKind::Base));
    let mut passes = Vec::new();
    for _ in 0..3 {
        let mut visits = Vec::new();
        arena
            .visit_flag1_update_callbacks(|_, object, _| visits.push(object))
            .unwrap();
        passes.push(visits);
    }
    let reversed = objects.iter().rev().copied().collect::<Vec<_>>();
    assert_eq!(passes, [objects.to_vec(), reversed, objects.to_vec()]);
}

#[test]
fn stopped_construction_added_during_update_never_runs_and_keeps_native_state() {
    use crate::object_runtime::{ObjectClassKind, ObjectList};
    let mut arena = ObjectArena::new();
    let first = arena.allocate(ObjectClassKind::Base);
    let later = arena.allocate(ObjectClassKind::Base);
    let mut stopped = None;
    let mut healthy = None;
    let mut retained = None;
    let mut visits = Vec::new();
    arena
        .visit_flag1_update_callbacks(|arena, object, _| {
            visits.push(object);
            if object == first {
                let child = arena.allocate(ObjectClassKind::Base);
                arena.owned_mut(child).unwrap().state.base.dirty_word = 0xa55a;
                let native = arena.object(child).unwrap().clone();
                arena.stop_object_construction(child).unwrap();
                assert_eq!(arena.object(child).unwrap(), &native);
                stopped = Some(child);
                retained = Some(native);
                healthy = Some(arena.allocate(ObjectClassKind::Base));
            }
        })
        .unwrap();
    let stopped = stopped.unwrap();
    let healthy = healthy.unwrap();
    assert_eq!(visits, [first, later, healthy]);
    assert!(arena.construction_is_stopped(stopped).unwrap());
    assert_eq!(arena.object(stopped).unwrap(), retained.as_ref().unwrap());
    assert_eq!(
        arena
            .list(ObjectList::Flag1Primary)
            .iter()
            .map(|node| node.object)
            .collect::<Vec<_>>(),
        [healthy, stopped, later, first]
    );
    assert!(arena.list(ObjectList::Flag1Secondary).is_empty());
    visits.clear();
    arena
        .visit_flag1_update_callbacks(|_, object, _| visits.push(object))
        .unwrap();
    assert_eq!(visits, [healthy, later, first]);
    assert_eq!(arena.object(stopped).unwrap(), retained.as_ref().unwrap());
}

#[test]
fn stopped_construction_marker_does_not_survive_destruction_and_slot_reuse() {
    use crate::object_runtime::{ObjectClassKind, ObjectDestructionRequest, SoundTeardownService};
    let mut arena = ObjectArena::new();
    let stopped = arena.allocate(ObjectClassKind::Base);
    arena.stop_object_construction(stopped).unwrap();
    let before = arena.object(stopped).unwrap().clone();
    let destroyed = arena
        .destroy(ObjectDestructionRequest {
            object: stopped,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(destroyed, before);
    assert!(arena.construction_is_stopped(stopped).is_err());
    let replacement = arena.allocate(ObjectClassKind::Base);
    assert_eq!(replacement.slot, stopped.slot);
    assert_ne!(replacement, stopped);
    assert!(!arena.construction_is_stopped(replacement).unwrap());
    let mut visits = Vec::new();
    arena
        .visit_flag1_update_callbacks(|_, object, _| visits.push(object))
        .unwrap();
    assert_eq!(visits, [replacement]);
}

fn loaded() -> (
    ResidentRuntime,
    crate::world_backend::WorldSession,
    ProcessWorldMapRuntime,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 47 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    let grants = inf_formats::parse_grant_table(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
    .unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (runtime, world, maps)
}

/// The world sky's height: 004408B0 stores H14/0 header DWORD +10 at
/// 00547668, FFFFF1A0 in Infest01's first world.
#[test]
fn loaded_world_backdrop_height_matches_infest01() {
    let (_, world, _) = loaded();
    assert_eq!(world.presentation.backdrop_height, -3680);
}

/// 00486600 in the loaded first world: H13 resource 4 admits the ribbon as
/// a base payload with its base commands; +50 then holds the source.
#[test]
fn loaded_world_admits_the_selector67_ribbon_through_resource_4() {
    use crate::object_runtime::{
        construct_selector67_ribbon, ObjectList, ObjectNodeKind, Selector67FactoryRequest,
    };
    let (runtime, mut world, _maps) = loaded();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let source = ActorHandle::from_raw(0x0003_0005);
    let ribbon = construct_selector67_ribbon(
        &mut world.arena,
        &Selector67FactoryRequest {
            runtime: &runtime,
            process: &process,
            network_active: false,
        },
        source,
    )
    .unwrap()
    .expect("resource 4's commands accept");
    // The base constructor's 0800_0003, resource 4's payload, then 00460120's
    // initialized flag; both memberships stay published.
    let base = &world.arena.object(ribbon).unwrap().base;
    assert_eq!(
        (base.flags, base.resource_word, base.placement_word),
        (0x0810_0003, 4, 0xffff)
    );
    // 00486530 never writes +32: Selector67PositionApproximation's zero.
    assert_eq!(
        base.position_words,
        Some(crate::object_runtime::SELECTOR67_POSITION_APPROXIMATION)
    );
    let state = world.arena.selector67(ribbon).unwrap();
    assert_eq!(
        (
            state.word_40,
            state.word_42,
            state.word_44,
            state.word_46,
            state.dword_48
        ),
        (150, 3500, 0, 2, 250)
    );
    assert!(state.nodes.is_empty());
    assert_eq!(state.owner_50, source);
    for (role, kind) in [
        (ObjectList::Flag0Primary, ObjectNodeKind::Flag0),
        (ObjectList::Flag1Primary, ObjectNodeKind::Flag1),
    ] {
        assert!(world
            .arena
            .list(role)
            .iter()
            .any(|node| node.object == ribbon && node.kind == kind));
    }
}

fn inputs() -> ArenaIntroInputs {
    ArenaIntroInputs {
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        settings_7c: 0,
        motion_mode: Selector90MotionMode::FractionalThousandths,
        up: [0.0, 1.0, 0.0],
    }
}

struct UnownedServices;

impl crate::object_runtime::TargetNullHandleSource for UnownedServices {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
}
// Explicit native fixture process source, independent of production retirement owners.
impl ArenaIntroServices for UnownedServices {
    type Error = &'static str;
    fn create_helper(&mut self, _arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        Err("helper factory")
    }
    fn camera(&mut self, _arena: &mut ObjectArena) -> Result<Option<ObjectId>, Self::Error> {
        Err("camera factory")
    }
    fn retire_actor(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<(), Self::Error> {
        Err("retirement callbacks")
    }
    fn world_present(&mut self) -> Result<bool, Self::Error> {
        Ok(true)
    }
    fn complete_mission(
        &mut self,
        _arena: &mut ObjectArena,
        _selection: u32,
        _start_ordinal: u16,
        _argument: u32,
    ) -> Result<(), Self::Error> {
        Err("mission completion")
    }
}

#[test]
fn mission22_retained_references_pin_selector_and_virtual_cc_policy() {
    let (runtime, mut world, _maps) = loaded();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let controller = world.arena.find_actor_by_placement(118).unwrap();
    let script = world.arena.selector88_intro_script(controller).unwrap();
    let main = world
        .arena
        .resolve_actor_handle(ActorHandle::from_raw(
            script.main_reference_handle().unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(Some(main), world.arena.find_actor_by_placement(53));
    let placements = crate::world_placement::WorldPlacements::new(
        &runtime,
        crate::terrain_runtime::TerrainSpacing::Units128,
    )
    .unwrap();
    let expected = [
        (48, 0xffff_0003, 12, 0x0052_7058, true),
        (49, 0xffff_0003, 12, 0x0052_7058, true),
        (50, 0xffff_0003, 12, 0x0052_7058, true),
        (51, 0xffff_0003, 12, 0x0052_7058, true),
        (83, 0xffff_0003, 12, 0x0052_7058, true),
        (84, 0xffff_0003, 12, 0x0052_7058, true),
        (30, 0xffff_000b, 11, 0x0051_f080, false),
        (31, 0xffff_000b, 11, 0x0051_f080, false),
        (68, 0xffff_0057, 10, 0x0052_0cc8, true),
        (69, 0xffff_0057, 10, 0x0052_0cc8, true),
        (52, 0xffff_006c, 36, 0x0052_7ee0, true),
        (79, 0xffff_006c, 36, 0x0052_7ee0, true),
        (80, 0xffff_006c, 36, 0x0052_7ee0, true),
        (97, 0xffff_0091, 35, 0x0052_273c, true),
        (98, 0xffff_0091, 35, 0x0052_273c, true),
        (53, 0xffff_002d, 60, 0x0052_7de8, false),
    ];
    for (reference, argument, selector, vtable, cc_self) in expected {
        // Constructor table identities come from each native constructor's
        // vtable write. Pin the actual retail functions, not just our enum.
        assert_eq!(executable.word(vtable + 0x58).unwrap(), 0x0040_4610);
        assert_eq!(
            executable.word(vtable + 0xcc).unwrap(),
            if cc_self { 0x0040_4610 } else { 0x0040_1300 }
        );
        let input = placements.input(reference as usize - 1).unwrap();
        assert_eq!(input.resource_argument, argument);
        let resource = runtime
            .resource(13, input.resource_argument as u16)
            .unwrap();
        let region = &resource.overlay.layout().regions[resource.registration.region_index];
        let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
        assert_eq!(
            bank.prefixes.entries[usize::from(resource.registration.local_index)]
                .constructor_selector,
            selector
        );
        let actor = world.arena.find_actor_by_placement(reference).unwrap();
        assert_eq!(
            world.arena.actor_derived_component(actor).unwrap(),
            Some(actor)
        );
        assert!(!world.arena.intro_active_component(actor).unwrap());
        if cc_self {
            let DerivedActorClass::Family428650(state) =
                &mut world.arena.derived_actor_mut(actor).unwrap().class
            else {
                panic!("native self-returning +CC must read owned +22C");
            };
            state.byte_22c = 1;
            assert!(world.arena.intro_active_component(actor).unwrap());
        }
    }
    assert_eq!(
        world.arena.actor_derived_component(controller).unwrap(),
        None
    );
}

#[test]
fn arena_position_from_copies_only_xyz_and_reads_source_before_target_validation() {
    let (runtime, mut world, _maps) = loaded();
    let controller = world.arena.find_actor_by_placement(118).unwrap();
    let sine = [0_u16; 4096];
    let request = crate::object_runtime::ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: crate::object_runtime::ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &sine,
        },
    };
    let helper = world.arena.allocate_selector90(request).unwrap();
    let source = world.arena.allocate_actor(request).unwrap();
    let unknown_source = world.arena.allocate_actor(request).unwrap();
    let nonactor = world
        .arena
        .allocate(crate::object_runtime::ObjectClassKind::Base);
    world
        .arena
        .copy_actor_position(source, [0x8001, 0xffff, 0x7fff])
        .unwrap();
    world
        .arena
        .write_selector90_word(helper, Selector90Word::Speed13c, 0x1234)
        .unwrap();
    let mut expected = world.arena.object(helper).unwrap().clone();
    expected.base.position_words = Some([0x8001, 0xffff, 0x7fff]);
    let source_before = world.arena.object(source).unwrap().clone();
    let mut services = UnownedServices;
    let mut host = ArenaIntroHost::new(
        &mut world.arena,
        controller,
        ArenaIntroRequest {
            runtime: &runtime,
            inputs: inputs(),
            services: &mut services,
        },
    )
    .unwrap();
    //443C0E/443CB3 use shared4404D0. This placement leaves helper route,
    //speed/basis/velocity/dirty state to their separate subsequent effects.
    host.effect(IntroEffect::PositionFrom {
        actor: helper,
        source,
    })
    .unwrap();
    assert_eq!(host.arena.object(helper).unwrap(), &expected);
    assert_eq!(host.arena.object(source).unwrap(), &source_before);
    let before = format!("{:?}", host.arena);
    assert!(matches!(
        host.effect(IntroEffect::PositionFrom { actor: nonactor, source: unknown_source }),
        Err(ArenaIntroError::Object(ObjectError::ActorPositionUnknown(actor))) if actor == unknown_source
    ));
    assert_eq!(format!("{:?}", host.arena), before);
    assert!(matches!(
        host.effect(IntroEffect::PositionFrom { actor: nonactor, source }),
        Err(ArenaIntroError::Object(ObjectError::NotActor(actor))) if actor == nonactor
    ));
    assert_eq!(format!("{:?}", host.arena), before);
}

#[test]
fn arena_host_effects_update_live_helpers_camera_targets_and_full_pose() {
    let (runtime, mut world, _maps) = loaded();
    let controller = world.arena.find_actor_by_placement(118).unwrap();
    // This test uses a real selector90 allocation but exercises intrinsic host
    // effects independently of the still-required production helper factory.
    let sine = [0_u16; 4096];
    let helper = world
        .arena
        .allocate_selector90(crate::object_runtime::ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([10, 20, 30]),
            owner: 1,
            terrain: crate::object_runtime::ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &sine,
            },
        })
        .unwrap();
    let player = world
        .arena
        .player_actor(world.player.unwrap())
        .unwrap()
        .unwrap();
    let camera_handle = world.arena.camera_handle().unwrap();
    let camera = world
        .arena
        .resolve_actor_handle(camera_handle)
        .unwrap()
        .unwrap();
    let mut services = UnownedServices;
    let mut host = ArenaIntroHost::new(
        &mut world.arena,
        controller,
        ArenaIntroRequest {
            runtime: &runtime,
            inputs: inputs(),
            services: &mut services,
        },
    )
    .unwrap();
    let route = host.route(IntroRoute::Primary00).unwrap().unwrap();
    host.effect(IntroEffect::HelperRoute {
        helper,
        route: Some(route),
    })
    .unwrap();
    host.effect(IntroEffect::Word {
        actor: helper,
        field: IntroWord::Helper13c,
        value: 1234,
    })
    .unwrap();
    assert_eq!(host.word(helper, IntroWord::Helper13c).unwrap(), 1234);
    host.effect(IntroEffect::Byte {
        actor: helper,
        field: IntroByte::Helper140,
        value: 0,
    })
    .unwrap();
    let handle = host.actor_handle(helper).unwrap();
    host.effect(IntroEffect::CameraPrimary { camera, handle })
        .unwrap();
    host.effect(IntroEffect::CameraSecondaryFromPrimary { camera })
        .unwrap();
    host.effect(IntroEffect::CameraParameterMode { camera, mode: 1 })
        .unwrap();
    host.effect(IntroEffect::Byte {
        actor: camera,
        field: IntroByte::Camera184,
        value: 1,
    })
    .unwrap();
    host.effect(IntroEffect::Membership {
        actor: helper,
        list: IntroMembership::Flag0,
        enabled: false,
    })
    .unwrap();
    host.effect(IntroEffect::Flags {
        actor: helper,
        and_mask: u32::MAX,
        or_mask: 0x20,
    })
    .unwrap();
    host.effect(IntroEffect::Dirty {
        actor: helper,
        bits: 0x100,
    })
    .unwrap();
    host.effect(IntroEffect::Resource42 {
        actor: helper,
        value: u16::MAX,
    })
    .unwrap();
    host.effect(IntroEffect::RefreshVisual { actor: helper })
        .unwrap();
    host.effect(IntroEffect::PlayerPoseFromHelper { player, helper })
        .unwrap();
    assert_eq!(
        host.arena.object(player).unwrap().base.position_words,
        Some([10, 20, 30])
    );
    assert_eq!(
        host.arena.actor(player).unwrap().rotating.basis,
        NativeBasis::INITIAL
    );
    let helper_state = host.arena.selector90(helper).unwrap();
    assert_eq!(
        host.arena
            .object_position_target_data(helper_state.route_150.unwrap())
            .unwrap()
            .object_14,
        route
    );
    assert_eq!(helper_state.byte_140, 0);
    let camera_state = host.arena.selector16(camera).unwrap();
    assert_eq!(
        host.arena
            .actor_follow_target(camera_state.target_18c.unwrap())
            .unwrap()
            .handle_04
            .raw(),
        handle
    );
    assert_eq!(
        host.arena
            .actor_follow_target(camera_state.target_190.unwrap())
            .unwrap()
            .handle_04
            .raw(),
        handle
    );
    assert_eq!(camera_state.byte_184, 1);
    assert_eq!(camera_state.block_138.word_18, 1);
    assert_eq!(camera_state.block_15c.word_18, 1);
    assert_eq!(host.arena.object(helper).unwrap().base.flags & 1, 0);
    assert_ne!(host.arena.object(helper).unwrap().base.flags & 0x20, 0);
    assert_ne!(
        host.arena.object(helper).unwrap().base.dirty_word & 0x100,
        0
    );
}

#[test]
fn arena_controller_publishes_reached_state_when_required_factory_fails() {
    let (runtime, mut world, _maps) = loaded();
    let controller = world.arena.find_actor_by_placement(118).unwrap();
    world
        .arena
        .supply_selector88_intro_allocation(
            controller,
            IntroAllocationWords {
                elapsed_138: 0,
                deadline_140: 10000,
                sequential_timer_150: None,
            },
        )
        .unwrap();
    let player = world
        .arena
        .player_actor(world.player.unwrap())
        .unwrap()
        .unwrap();
    let expected_position = world.arena.object(controller).unwrap().base.position_words;
    let mut services = UnownedServices;
    let result = world.arena.tick_selector88_intro_live(
        controller,
        1024,
        ArenaIntroRequest {
            runtime: &runtime,
            inputs: inputs(),
            services: &mut services,
        },
    );
    assert!(matches!(
        result,
        Err(ArenaIntroTickError::Kernel(IntroError::Host(
            ArenaIntroError::Host("helper factory")
        )))
    ));
    let state = world.arena.selector88_intro_sequence(controller).unwrap();
    assert_eq!(state.stage_148, 0);
    assert_eq!(state.elapsed_138, 0); // State0 exit resets before the factory.
    assert_eq!(
        world.arena.object(player).unwrap().base.position_words,
        expected_position
    );
    assert_eq!(world.arena.object(player).unwrap().base.flags & 3, 0);
}

struct UnownedCallbacks;

impl IntroCallbackHost for UnownedCallbacks {
    type Error = &'static str;

    fn retire_actor(
        &mut self,
        _arena: &mut ObjectArena,
        _actor: ObjectId,
    ) -> Result<(), Self::Error> {
        Err("retirement callbacks")
    }

    fn world_present(&mut self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn complete_mission(
        &mut self,
        _arena: &mut ObjectArena,
        _selection: u32,
        _start_ordinal: u16,
        _argument: u32,
    ) -> Result<(), Self::Error> {
        Err("mission completion")
    }
}

#[derive(Default)]
struct UnownedListenerEffects(usize);

impl crate::object_runtime::CreationListenerEffects for UnownedListenerEffects {
    fn invoke(
        &mut self,
        listener: crate::object_runtime::CreationListener,
        _actor: ObjectId,
        _listeners: &mut crate::object_runtime::CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), crate::object_runtime::CreationListenerError> {
        self.0 += 1;
        Err(crate::object_runtime::CreationListenerError::UnsupportedInterface(listener.interface))
    }
}

#[test]
fn native_services_admit_real_helper_reuse_camera_and_require_callbacks() {
    let (runtime, mut world, _maps) = loaded();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let sine = assets.sine_table().values;
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let allocation = crate::object_runtime::ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: crate::object_runtime::ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &sine,
        },
    };
    let camera = world
        .arena
        .resolve_actor_handle(world.arena.camera_handle().unwrap())
        .unwrap()
        .unwrap();
    let controller = world.arena.find_actor_by_placement(118).unwrap();
    let mut effects = UnownedListenerEffects::default();
    let mut callbacks = UnownedCallbacks;
    {
        let mut services = NativeIntroServices {
            admission: IntroAdmissionInputs {
                // Helper admission and resolved-camera reuse do not reach NULL.
                process: None,
                runtime: &runtime,
                math: &math,
                helper_allocation: allocation,
                camera_allocation: allocation,
                local_owner: 1,
                camera_settings: inputs().camera_settings,
                network_active: false,
                network_world_active: false,
                listeners: &mut world.listeners,
                listener_effects: &mut effects,
            },
            callbacks: &mut callbacks,
        };
        assert!(services.admission.listeners.is_empty());
        let helper = services.create_helper(&mut world.arena).unwrap().unwrap();
        assert_eq!(world.arena.selector90(helper).unwrap().word_138, 256);
        assert_eq!(world.arena.object(helper).unwrap().base.resource_word, 142);
        assert_eq!(
            world.arena.object(helper).unwrap().base.position_words,
            None
        );
        assert_ne!(
            world.arena.object(helper).unwrap().base.flags
                & crate::object_runtime::INITIALIZED_FLAG,
            0
        );
        let count = world.arena.actor_list().len();
        assert_eq!(services.camera(&mut world.arena).unwrap(), Some(camera));
        assert_eq!(world.arena.actor_list().len(), count);
        assert!(matches!(
            services.retire_actor(&mut world.arena, controller),
            Err(NativeIntroServicesError::Callback("retirement callbacks"))
        ));
        assert_eq!(world.arena.actor(controller).unwrap().word_c4, 3);
        assert!(matches!(
            services.complete_mission(&mut world.arena, 18, 1, 0),
            Err(NativeIntroServicesError::Callback("mission completion"))
        ));
        services.admission.listeners.register(
            controller,
            crate::object_runtime::CreationListenerInterface::Selector18,
        );
        assert!(matches!(
            services.create_helper(&mut world.arena),
            Err(NativeIntroServicesError::Admission(
                IntroAdmissionError::Construction(
                    crate::object_runtime::ObjectConstructionError::CreationListener(
                        crate::object_runtime::CreationListenerError::UnsupportedInterface(
                            crate::object_runtime::CreationListenerInterface::Selector18
                        )
                    )
                )
            ))
        ));
    }
    assert_eq!(effects.0, 1);
}
