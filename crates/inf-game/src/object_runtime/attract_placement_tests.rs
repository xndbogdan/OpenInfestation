use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorConstructionRequest, ActorExtension, ActorHandleCache, ActorHandles,
        ActorRegistryConfig, ActorServiceHeapError, ActorServiceResourceKind,
        ActorServiceResourceState, ActorTerrainInputs, CameraParameterState, CreationListener,
        CreationListenerEffects, CreationListenerError, CreationListenerInterface,
        CreationListeners, DerivedActorConstructionRequest, FixtureActorServiceAllocation,
        NativeHeapRef, NativeTarget, ObjectClass, ObjectConstructionError, ObjectList,
        ObjectNodeKind, ObjectState, INITIALIZED_FLAG,
    },
    process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::{
        native_probe_fixture_terrain, TerrainHeightScale, TerrainRuntime, TerrainSampling,
        TerrainSpacing,
    },
    world_actor_terrain::{WorldActorTerrainError, WorldActorTerrainHost, WorldTerrainSource},
    world_start::{WorldStartContext, WorldStartState},
    HandlerResources, Installation, ResidencyRequest, ResidentRuntime,
};
use inf_formats::{native_basis::NativeBasisMath, HANDLER_COUNT};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const SINE: [u16; 4096] = [0; 4096];
const INITIAL_XYZ: [u16; 3] = [0x8001, 0xfffe, 0x7fff];
const WORDS: [u32; 4] = [0xff81_0181, 270, 0x0901_0781, 90];
const SETTINGS: CameraSettings = CameraSettings {
    first_person: 0,
    camera_style: 0,
};
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

/// Declared allocation outcomes still use the actual retained heap reservation.
fn allocation_fixture(arena: &mut ObjectArena, result: Result<bool, &'static str>) -> usize {
    arena.actor_service_heap_mut().script([match result {
        Ok(true) => FixtureActorServiceAllocation::Allocate,
        Ok(false) => FixtureActorServiceAllocation::Null,
        Err(_) => FixtureActorServiceAllocation::Error(71),
    }]);
    arena.actor_service_heap().attempts().len()
}

fn allocation_classes(arena: &ObjectArena, from: usize) -> Vec<NativeTargetClass> {
    arena.actor_service_heap().attempts()[from..]
        .iter()
        .map(|kind| match kind {
            ActorServiceResourceKind::WeaponFireNode => {
                panic!("unexpected firing allocation in target fixture")
            }
            ActorServiceResourceKind::Target(class) => *class,
            ActorServiceResourceKind::FamilyRadiusScoreInput => {
                panic!("unexpected radius input allocation")
            }
        })
        .collect()
}

fn camera_target(
    arena: &ObjectArena,
    camera: ObjectId,
    expected: super::super::ActorFollowTarget,
) -> NativeHeapRef {
    let reference = arena.selector16(camera).unwrap().target_18c.unwrap();
    assert_eq!(
        arena.native_target(reference).unwrap(),
        NativeTarget::ActorFollow(expected)
    );
    reference
}

fn allocation() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(INITIAL_XYZ),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0x8765_4321,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

fn setup() -> (ObjectArena, ObjectId, ObjectId, AttractActorSetupState) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let keeper = arena.allocate_actor(allocation()).unwrap();
    let camera = arena.allocate_selector16(allocation(), SETTINGS).unwrap();
    let publication = AttractActorSetupState {
        object_5e0: Some(keeper),
        handle_5dc: Some(ActorHandle::from_raw(0x1234_5678)),
    };
    (arena, keeper, camera, publication)
}

fn factory<'a>(
    runtime: &'a ResidentRuntime,
    math: &'a NativeBasisMath,
    process: &'a ParticleFactoryProcessState,
    listeners: &'a mut CreationListeners,
    effects: &'a mut dyn CreationListenerEffects,
) -> Selector93FactoryRequest<'a> {
    Selector93FactoryRequest {
        allocation: allocation(),
        process,
        local_owner: 1,
        runtime,
        math,
        listeners,
        listener_effects: effects,
        network_world_active: false,
    }
}

/// A reached interface remains required. Empty-list success never invokes it.
struct RequiredEffects;

impl CreationListenerEffects for RequiredEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        _actor: ObjectId,
        _listeners: &mut CreationListeners,
        _arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

/// All arena owners, normalizing only the selected position for an independently
/// asserted XYZ-only copy. No expected state is supplied to a runtime call.
fn custody(arena: &ObjectArena, ignore_position: Option<ObjectId>) -> String {
    relation_custody(arena, ignore_position, None, None)
}

/// Normalize only separately compared complete camera/registry owners. This
/// never changes the actual arena or supplies expected poststate to a call.
fn relation_custody(
    arena: &ObjectArena,
    ignore_position: Option<ObjectId>,
    camera: Option<(ObjectId, &ObjectState)>,
    registry: Option<&Option<ActorHandles>>,
) -> String {
    let ignored = ignore_position.map(|object| arena.object(object).unwrap());
    let camera = camera.map(|(id, before)| (arena.object(id).unwrap(), before));
    let slots: Vec<_> = arena
        .slots
        .iter()
        .map(|slot| {
            let value = slot.value.as_ref().map(|owned| {
                let mut state = owned.state.clone();
                if let Some((actual, before)) = camera {
                    if std::ptr::eq(actual, &owned.state) {
                        state = before.clone();
                    }
                }
                if ignored.is_some_and(|value| std::ptr::eq(value, &owned.state)) {
                    state.base.position_words = None;
                }
                (state, owned.memberships)
            });
            (slot.generation, value)
        })
        .collect();
    format!(
        "{:?}",
        (
            arena.identity,
            slots,
            &arena.free,
            arena.live_count,
            &arena.lists,
            arena.list_owners,
            &arena.routes,
            &arena.actors,
            &arena.map_annotations,
            registry.unwrap_or(&arena.actor_handles),
            (
                &arena.actor_payloads,
                &arena.constructor_allocations,
                &arena.selector60_rows,
                &arena.players,
                &arena.world_load,
            ),
        )
    )
}

struct Observed {
    object: ObjectId,
    state: ObjectState,
    cache: ActorHandleCache,
    custody: String,
}

struct Observe<H> {
    inner: H,
    calls: Vec<Observed>,
    destroy: bool,
}

impl<H> Observe<H> {
    fn new(inner: H) -> Self {
        Self {
            inner,
            calls: vec![],
            destroy: false,
        }
    }
}

impl<H: ActorTerrainHost> ActorTerrainHost for Observe<H> {
    type Error = H::Error;

    fn terrain(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        let state = arena.object(actor).unwrap().clone();
        let cache = arena.actor_handles().unwrap().cache();
        assert_eq!(cache.handle, arena.actor(actor).unwrap().handle);
        assert_eq!(cache.object, Some(actor));
        assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
        self.calls.push(Observed {
            object: actor,
            state,
            cache,
            custody: custody(arena, Some(actor)),
        });
        if self.destroy {
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: actor,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
        }
        self.inner.terrain(arena, actor)
    }
}

fn policy_terrain(
    xz: [u16; 2],
    height: i8,
    plane: i32,
    coarse: bool,
    scale32: bool,
) -> TerrainRuntime {
    native_probe_fixture_terrain(&json!({
        "spacing": if coarse {256} else {128},
        "height_shift": if scale32 {5} else {4},
        "plane_base": plane,"plane_shift": if scale32 {8} else {9},
        "xz":xz,"heights":[height,height,height,height],"alternate":false,
    }))
}

/// Declared structural installation: four distinct H1 records and/or fixed H13
/// shared93, disabled H8. It is neither a retail asset patch nor a loader stub.
struct Fixture {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl Fixture {
    fn new(words: Option<[u32; 4]>, h13: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-attract-placement-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed),
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(
                &(match handler {
                    1 => 4_u16,
                    12 => 1,
                    13 => 53,
                    _ => 0,
                })
                .to_le_bytes(),
            );
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for phase in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let count: i16 = match (phase, handler) {
                    (0, 1) if words.is_some() => 4,
                    (0, 13) if h13 => 53,
                    _ => 0,
                };
                preload.extend_from_slice(&count.to_le_bytes());
                preload.extend_from_slice(&u16::from(count != 0).to_le_bytes());
                if count != 0 {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut overlay = Vec::new();
        if let Some(words) = words {
            overlay.extend_from_slice(&16_u32.to_le_bytes());
            for word in words {
                overlay.extend_from_slice(&word.to_le_bytes());
            }
        }
        if h13 {
            let mut main = 92_u16.to_le_bytes().to_vec();
            main.extend_from_slice(&u16::MAX.to_le_bytes());
            let mut payload = vec![0_u8; 0x42];
            payload[..4].copy_from_slice(&0x1002_u32.to_le_bytes());
            payload[4..6].copy_from_slice(&0x2468_u16.to_le_bytes());
            payload[8..10].copy_from_slice(&7_u16.to_le_bytes());
            for channel in 0..10 {
                let at = 0x10 + channel * 4;
                payload[at..at + 2].copy_from_slice(&(3000 + channel as u16).to_le_bytes());
                payload[at + 2..at + 4].copy_from_slice(&256_u16.to_le_bytes());
            }
            payload[0x40..0x42].copy_from_slice(&873_u16.to_le_bytes());
            main.extend_from_slice(&payload);
            overlay.extend_from_slice(&(main.len() as u32).to_le_bytes());
            overlay.extend_from_slice(&main);
            overlay.extend_from_slice(&0_u32.to_le_bytes());
            for _ in 0..53 {
                overlay.extend_from_slice(&0_u32.to_le_bytes());
            }
        }
        if !overlay.is_empty() {
            fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        }
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-attract-placement-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn retail() -> (ResidentRuntime, NativeBasisMath) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    // A real current world supplies H1/H12; neither Boot nor leaf1/48 does.
    // This is sequential canonical residency, not an appended bank inventory.
    runtime
        .apply(ResidencyRequest::Activate { level: 28 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    (runtime, NativeBasisMath::new(assets.sine_table().values))
}

#[test]
fn retail_current_sources_complete_setup_preserves_placement_and_camera_custody() {
    let (runtime, math) = retail();
    let h1 = runtime.resource(1, 0).unwrap();
    let HandlerResources::Words(words) = &h1.overlay.handlers()[h1.registration.region_index]
    else {
        panic!("actual H1 bank")
    };
    let packed = words.entries[usize::from(h1.registration.local_index)].raw;
    let xz = [packed as u16, (packed >> 16) as u16];
    let h12 = runtime.resource(12, 0).unwrap();
    let HandlerResources::Terrain(bank) = &h12.overlay.handlers()[h12.registration.region_index]
    else {
        panic!("actual H12 bank")
    };
    let source = &bank.entries[usize::from(h12.registration.local_index)];
    // Explicit current sampling input, not an inferred mission/global default.
    let terrain = TerrainRuntime::new(
        source,
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units16,
        },
    );
    let retained = WorldTerrainSource::retain(&h12);
    let mut host = Observe::new(WorldActorTerrainHost {
        residency: &runtime,
        source: &retained,
        prepared: &terrain,
    });
    let h1_before = h1.overlay.bytes().to_vec();
    let h12_before = h12.overlay.bytes().to_vec();
    let revision = runtime.revision();
    let terrain_before = format!("{terrain:?}");
    let process = ParticleFactoryProcessState::native_startup();
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(true));
    let keeper_before = arena.object(keeper).unwrap().clone();
    let camera_before = arena.object(camera).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let listeners_before = format!("{listeners:?}");
    let registrations_before: Vec<_> = listeners.registrations().collect();
    let mut effects = RequiredEffects;
    let object = setup_attract_actor(
        &mut arena,
        AttractActorSetupRequest {
            publication: &mut publication,
            camera,
            settings: SETTINGS,
            factory: factory(&runtime, &math, &process, &mut listeners, &mut effects),
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(host.calls.len(), 1);
    assert_eq!(
        allocation_classes(&arena, allocation_start),
        [NativeTargetClass::ActorFollow]
    );
    let before = &host.calls[0];
    let selected =
        source.cells()[usize::from(xz[0] >> 7) * 512 + usize::from(xz[1] >> 7)].height_i8();
    let height = (i16::from(selected) << 4).max((source.prefix.plane_height_fixed >> 9) as i16);
    let position = [xz[0], (height as u16).wrapping_add(512), xz[1]];
    let mut expected = before.state.clone();
    expected.base.position_words = Some(position);
    assert_eq!(before.object, object);
    assert_eq!(before.state.base.position_words, Some(INITIAL_XYZ));
    assert_eq!(arena.object(object).unwrap(), &expected);
    let raised = [
        position[0],
        position[1].wrapping_add(arena.actor(object).unwrap().word_74),
        position[2],
    ];
    let target = super::super::ActorFollowTarget {
        handle_04: before.cache.handle,
        word_10: 2,
        focus_08: Some(raised),
        focus_14: Some(raised),
        velocity_1a: None,
    };
    assert_eq!(
        arena.object(camera).unwrap(),
        &expected_camera(
            &camera_before,
            Some(camera_target(&arena, camera, target)),
            3
        )
    );
    assert_eq!(arena.actor_handles().unwrap().cache(), before.cache);
    assert_eq!(
        relation_custody(&arena, Some(object), Some((camera, &camera_before)), None),
        before.custody
    );
    assert_eq!(publication.object_5e0, Some(object));
    assert_eq!(publication.handle_5dc, Some(before.cache.handle));
    assert_eq!(arena.actor_list(), [keeper, camera, object]);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert_eq!(runtime.revision(), revision);
    assert_eq!(h1.overlay.bytes(), h1_before);
    assert_eq!(h12.overlay.bytes(), h12_before);
    assert_eq!(format!("{terrain:?}"), terrain_before);
    // 0046020E inserts a null-payload stack cursor even for the empty walk;
    // 00460277..286 unlinks it. The port retains its logical cursor storage
    // as a free slot. Keep all identity/sentinel state and require no live
    // cursor, rather than asserting unchanged allocation history.
    let sentinels_before = listeners_before
        .strip_suffix("], free: [], count: 0 }")
        .expect("fresh two-sentinel listener owner");
    let expected_listeners = format!(
        "{sentinels_before}, Slot {{ generation: 1, node: None }}], free: [2], count: 0 }}"
    );
    assert_eq!(format!("{listeners:?}"), expected_listeners);
    assert_eq!(
        listeners.registrations().collect::<Vec<_>>(),
        registrations_before
    );
    assert_eq!(listeners.len(), 0);
    assert_eq!(process, ParticleFactoryProcessState::native_startup());
}

#[test]
fn slot_zero_is_distinct_from_world_pair_override_and_heading() {
    let fixture = Fixture::new(Some(WORDS), true);
    let terrain = policy_terrain([0x0181, 0xff81], -64, -90 << 9, false, false);
    let mut world = WorldStartState {
        pair_index: 1,
        override_position: Some([77, 999, 88]),
        vehicle_selection: 4,
        harness_spawn: None,
        port_spawn: None,
    };
    let start = WorldStartContext {
        state: &mut world,
        runtime: &fixture.runtime,
        terrain: &terrain,
    };
    assert_eq!(start.resolve_position().unwrap(), [77, 60, 88]);
    assert_eq!(start.resolve_heading().unwrap(), [0, 0x4000, 0]);
    let world_before = world.clone();
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let (mut arena, _, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects;
    let mut host = Observe::new(&terrain);
    let object = setup_attract_actor(
        &mut arena,
        AttractActorSetupRequest {
            publication: &mut publication,
            camera,
            settings: SETTINGS,
            factory: factory(
                &fixture.runtime,
                &math,
                &process,
                &mut listeners,
                &mut effects,
            ),
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([0x0181, 422, 0xff81])
    );
    assert_eq!(world, world_before);
    assert_eq!(
        allocation_classes(&arena, allocation_start),
        [NativeTargetClass::ActorFollow]
    );
}

#[test]
fn signed_plane_lowword_and_wrapping_y_cover_each_declared_sampling_policy() {
    // Literal expected Ys distinguish signed plane truncation from i32 max,
    // negative signed cells, coarse origin, and the +512 WORD wrap.
    let cases = [
        (false, false, -64, -90 << 9, 422),
        (true, true, -8, -1000 << 8, 256),
        (false, false, 10, 0x8000 << 9, 672),
        (true, true, -1, 0x7fff << 8, 0x81ff),
        (false, false, -1, -16 << 9, 496),
    ];
    let fixture = Fixture::new(Some(WORDS), true);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    for (coarse, scale32, cell, plane, y) in cases {
        let terrain = policy_terrain([0x0181, 0xff81], cell, plane, coarse, scale32);
        let (mut arena, _, camera, mut publication) = setup();
        let allocation_start = allocation_fixture(&mut arena, Ok(false));
        let mut listeners = CreationListeners::new();
        let mut effects = RequiredEffects;
        let mut host = Observe::new(&terrain);
        let object = setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                camera,
                settings: SETTINGS,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects,
                ),
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(
            arena.object(object).unwrap().base.position_words,
            Some([0x0181, y, 0xff81])
        );
        assert_eq!(custody(&arena, Some(object)), host.calls[0].custody);
        assert_eq!(
            allocation_classes(&arena, allocation_start),
            [NativeTargetClass::ActorFollow]
        );
    }
}

#[test]
fn placement_reads_raw_cell_instead_of_interpolating_unequal_corners() {
    let fixture = Fixture::new(Some(WORDS), true);
    let terrain = native_probe_fixture_terrain(&json!({
        "spacing":128,"height_shift":4,"plane_base":-1000 << 9,"plane_shift":9,
        "xz":[0x0181,0xff81],"heights":[-9,11,23,-31],"alternate":false,
    }));
    assert_eq!(terrain.raw_height_at([0x0181, 0xff81]), -144);
    assert_ne!(
        terrain.height_at([0x0181, 0xff81]),
        terrain.raw_height_at([0x0181, 0xff81])
    );
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let (mut arena, _, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects;
    let mut host = Observe::new(&terrain);
    let object = setup_attract_actor(
        &mut arena,
        AttractActorSetupRequest {
            publication: &mut publication,
            camera,
            settings: SETTINGS,
            factory: factory(
                &fixture.runtime,
                &math,
                &process,
                &mut listeners,
                &mut effects,
            ),
        },
        &mut host,
    )
    .unwrap();
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([0x0181, 368, 0xff81])
    );
    assert_eq!(custody(&arena, Some(object)), host.calls[0].custody);
    assert_eq!(
        allocation_classes(&arena, allocation_start),
        [NativeTargetClass::ActorFollow]
    );
}

#[test]
fn missing_h1_precedes_factory_null_preflight_and_all_publication() {
    let fixture = Fixture::new(None, true);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState {
        process_null_handle: None,
        ..ParticleFactoryProcessState::native_startup()
    };
    let terrain = policy_terrain([0, 0], 0, 0, false, false);
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let initial = format!("{arena:?}");
    let old_publication = publication.clone();
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let listeners_before = format!("{listeners:?}");
    let mut effects = RequiredEffects;
    let mut host = Observe::new(&terrain);
    assert!(matches!(
        setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                camera,
                settings: SETTINGS,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects
                )
            },
            &mut host,
        ),
        Err(AttractActorSetupError::Position(H1WordError::NotResident {
            slot: 0
        }))
    ));
    assert_eq!(format!("{arena:?}"), initial);
    assert_eq!(publication, old_publication);
    assert_eq!(format!("{listeners:?}"), listeners_before);
    assert!(host.calls.is_empty());
    assert!(allocation_classes(&arena, allocation_start).is_empty());
    assert!(process.process_null_handle.is_none());
}

#[test]
fn reached_factory_error_retains_actor_but_not_game_publication_or_terrain() {
    let fixture = Fixture::new(Some(WORDS), false);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let terrain = policy_terrain([0, 0], 0, 0, false, false);
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let keeper_before = arena.object(keeper).unwrap().clone();
    let old_publication = publication.clone();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects;
    let mut host = Observe::new(&terrain);
    let error = setup_attract_actor(
        &mut arena,
        AttractActorSetupRequest {
            publication: &mut publication,
            camera,
            settings: SETTINGS,
            factory: factory(
                &fixture.runtime,
                &math,
                &process,
                &mut listeners,
                &mut effects,
            ),
        },
        &mut host,
    )
    .unwrap_err();
    let AttractActorSetupError::Factory(Selector93FactoryError::Published { object, source }) =
        error
    else {
        panic!("required H13 prefix")
    };
    assert!(matches!(
        source,
        ObjectConstructionError::NotResident {
            resource_argument: 52
        }
    ));
    assert_eq!(publication, old_publication);
    assert_eq!(arena.actor_list(), [keeper, camera, object]);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.position_words, Some(INITIAL_XYZ));
    assert_eq!(state.base.dirty_word, 2);
    assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(state.base.resource_word, 0);
    assert!(host.calls.is_empty());
    assert!(allocation_classes(&arena, allocation_start).is_empty());
}

#[test]
fn current_missing_h12_retains_completed_publication_resolver_and_whole_arena() {
    let (retail, _) = retail();
    let source = WorldTerrainSource::retain(&retail.resource(12, 0).unwrap());
    let fixture = Fixture::new(Some(WORDS), true); // current H12 table is empty
    let terrain = policy_terrain([0x0181, 0xff81], 11, 0, false, false);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let keeper_before = arena.object(keeper).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects;
    let mut host = Observe::new(WorldActorTerrainHost {
        residency: &fixture.runtime,
        source: &source,
        prepared: &terrain,
    });
    assert!(matches!(
        setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                camera,
                settings: SETTINGS,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects
                )
            },
            &mut host,
        ),
        Err(AttractActorSetupError::Terrain(
            WorldActorTerrainError::RequiredTerrain
        ))
    ));
    let before = &host.calls[0];
    assert_eq!(publication.object_5e0, Some(before.object));
    assert_eq!(publication.handle_5dc, Some(before.cache.handle));
    assert_eq!(arena.object(before.object).unwrap(), &before.state);
    assert_eq!(custody(&arena, Some(before.object)), before.custody);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert_eq!(arena.actor_list(), [keeper, camera, before.object]);
    assert!(allocation_classes(&arena, allocation_start).is_empty());
}

enum ListenerEffect {
    Required,
    RemoveRegistry,
}

struct Effects {
    effect: ListenerEffect,
    called: Option<ObjectId>,
}

impl CreationListenerEffects for Effects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        actor: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.called = Some(actor);
        assert_eq!(arena.object(actor)?.base.dirty_word, 0);
        arena.copy_actor_position(actor, [7, 0x8000, 9])?;
        arena.set_flag0(actor, true)?;
        match self.effect {
            ListenerEffect::Required => Err(CreationListenerError::UnsupportedInterface(
                listener.interface,
            )),
            // Declared callback fixture, not an implementation of that retail
            // interface or a native registry mutation claim.
            ListenerEffect::RemoveRegistry => {
                arena.actor_handles = None;
                Ok(())
            }
        }
    }
}

#[test]
fn listener_failure_retains_callback_effects_without_game_publication() {
    let fixture = Fixture::new(Some(WORDS), true);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let terrain = policy_terrain([0, 0], 0, 0, false, false);
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let old_publication = publication.clone();
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let mut effects = Effects {
        effect: ListenerEffect::Required,
        called: None,
    };
    let mut host = Observe::new(&terrain);
    assert!(matches!(
        setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                camera,
                settings: SETTINGS,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects
                )
            },
            &mut host,
        ),
        Err(AttractActorSetupError::Factory(
            Selector93FactoryError::Published {
                source: ObjectConstructionError::CreationListener(_),
                ..
            }
        ))
    ));
    let object = effects.called.unwrap();
    assert_eq!(publication, old_publication);
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([7, 0x8000, 9])
    );
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 2);
    assert_eq!(
        arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
    assert!(host.calls.is_empty());
    assert!(allocation_classes(&arena, allocation_start).is_empty());
}

#[test]
fn resolver_error_retains_both_game_stores_and_no_late_terrain() {
    let fixture = Fixture::new(Some(WORDS), true);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let terrain = policy_terrain([0, 0], 0, 0, false, false);
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let mut effects = Effects {
        effect: ListenerEffect::RemoveRegistry,
        called: None,
    };
    let mut host = Observe::new(&terrain);
    assert!(matches!(
        setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                camera,
                settings: SETTINGS,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects
                )
            },
            &mut host,
        ),
        Err(AttractActorSetupError::Resolve(
            ActorConstructionError::MissingRegistry
        ))
    ));
    let object = effects.called.unwrap();
    assert_eq!(publication.object_5e0, Some(object));
    assert_eq!(
        publication.handle_5dc,
        Some(arena.actor(object).unwrap().handle)
    );
    assert_ne!(
        arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
    assert_eq!(
        arena.object(object).unwrap().base.position_words,
        Some([7, 0x8000, 9])
    );
    assert!(arena.actor_handles().is_none());
    assert!(host.calls.is_empty());
    assert!(allocation_classes(&arena, allocation_start).is_empty());
}

#[test]
fn terrain_actor_destruction_retains_publication_and_reports_late_copy_error() {
    let fixture = Fixture::new(Some(WORDS), true);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    let terrain = policy_terrain([0x0181, 0xff81], 10, 0, false, false);
    let (mut arena, keeper, camera, mut publication) = setup();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    let keeper_before = arena.object(keeper).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects;
    let mut host = Observe::new(&terrain);
    host.destroy = true;
    let error = setup_attract_actor(
        &mut arena,
        AttractActorSetupRequest {
            publication: &mut publication,
            camera,
            settings: SETTINGS,
            factory: factory(
                &fixture.runtime,
                &math,
                &process,
                &mut listeners,
                &mut effects,
            ),
        },
        &mut host,
    )
    .unwrap_err();
    let object = host.calls[0].object;
    assert!(matches!(
        error,
        AttractActorSetupError::Object(ObjectError::UnknownObject(id)) if id == object
    ));
    assert_eq!(publication.object_5e0, Some(object));
    assert_eq!(publication.handle_5dc, Some(host.calls[0].cache.handle));
    assert!(arena.object(object).is_err());
    assert_eq!(arena.actor_list(), [keeper, camera]);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert_eq!(
        arena
            .resolve_actor_handle(publication.handle_5dc.unwrap())
            .unwrap(),
        None
    );
    assert!(allocation_classes(&arena, allocation_start).is_empty());
}

/// Declared private current camera backing, not a constructor/activation claim.
fn seed_camera(arena: &mut ObjectArena, camera: ObjectId, referent: ObjectId) {
    let handle = arena.actor(referent).unwrap().handle;
    let secondary = arena
        .follow_target(FollowTargetRequest {
            handle,
            point: super::super::FollowTargetPoint::ExplicitPoint(Some([0x8001, 17, 0xffff])),
        })
        .unwrap();
    let secondary = arena.import_fixture_target(NativeTarget::ActorFollow(secondary));
    let ActorExtension::Selector16(state) = &mut arena.actor_mut(camera).unwrap().extension else {
        panic!("genuine camera")
    };
    state.byte_184 = 0x80;
    state.mode_188 = 1;
    state.block_138.word_18 = 1;
    state.block_15c.word_18 = 1;
    state.eye_138 = CameraParameterState {
        current_position: Some([101, 102, 103]),
        current_velocity: Some([0x8000, 1, 2]),
        desired_position: Some([201, 202, 203]),
        desired_velocity: Some([7, 8, 9]),
    };
    state.look_15c = CameraParameterState {
        current_position: None,
        current_velocity: Some([11, 12, 13]),
        desired_position: Some([301, 302, 303]),
        desired_velocity: None,
    };
    state.target_190 = Some(secondary);
    state.offset_194 = Some([-4, 5, -6]);
    state.dword_180 = Some(0x89ab_cdef);
    state.dword_19c = 0x7654_3210;
}

fn expected_camera(before: &ObjectState, target: Option<NativeHeapRef>, mode: u32) -> ObjectState {
    let mut expected = before.clone();
    let ObjectClass::Actor(actor) = &mut expected.class else {
        panic!("camera actor")
    };
    let ActorExtension::Selector16(state) = &mut actor.extension else {
        panic!("selector16")
    };
    state.target_18c = target;
    state.mode_188 = mode;
    state.byte_184 = 0;
    state.block_138.word_18 = 0;
    state.block_15c.word_18 = 0;
    expected
}

#[test]
fn complete_setup_constructs_raised_follow_without_refresh_or_other_camera_writes() {
    let fixture = Fixture::new(Some(WORDS), true);
    let terrain = policy_terrain([0x0181, 0xff81], -1, -16 << 9, false, false);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    for raw in [0x100, 0x101] {
        let (mut arena, keeper, camera, mut publication) = setup();
        seed_camera(&mut arena, camera, keeper);
        let camera_before = arena.object(camera).unwrap().clone();
        let source_before = fixture
            .runtime
            .resource(13, 52)
            .unwrap()
            .overlay
            .bytes()
            .to_vec();
        let keeper_before = arena.object(keeper).unwrap().clone();
        let mut listeners = CreationListeners::new();
        let mut effects = RequiredEffects;
        let mut host = Observe::new(&terrain);
        let allocation_start = allocation_fixture(&mut arena, Ok(true));
        let object = setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects,
                ),
                camera,
                settings: CameraSettings {
                    first_person: raw,
                    camera_style: 1,
                },
            },
            &mut host,
        )
        .unwrap();
        assert_eq!(
            allocation_classes(&arena, allocation_start),
            [NativeTargetClass::ActorFollow]
        );
        let actor = arena.actor(object).unwrap();
        let target = super::super::ActorFollowTarget {
            handle_04: actor.handle,
            word_10: 2,
            focus_08: Some([0x0181, 496u16.wrapping_add(actor.word_74), 0xff81]),
            focus_14: Some([0x0181, 496u16.wrapping_add(actor.word_74), 0xff81]),
            velocity_1a: None,
        };
        // Live93 refuses nonzero view: raw101 still ends in mode3. Current
        // camera-style does not reconstruct the original parameter tables.
        assert_eq!(
            arena.object(camera).unwrap(),
            &expected_camera(
                &camera_before,
                Some(camera_target(&arena, camera, target)),
                3
            )
        );
        assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
        let mut placed = host.calls[0].state.clone();
        placed.base.position_words = Some([0x0181, 496, 0xff81]);
        assert_eq!(arena.object(object).unwrap(), &placed);
        assert_eq!(arena.actor_handles().unwrap().cache(), host.calls[0].cache);
        assert_eq!(
            relation_custody(&arena, Some(object), Some((camera, &camera_before)), None),
            host.calls[0].custody
        );
        assert_eq!(
            fixture.runtime.resource(13, 52).unwrap().overlay.bytes(),
            source_before
        );
        assert_eq!(process, ParticleFactoryProcessState::native_startup());
        assert_eq!(publication.object_5e0, Some(object));
        assert_eq!(publication.handle_5dc, Some(actor.handle));
    }
}

#[test]
fn allocation_and_invalid_camera_errors_retain_whole_completed_placement() {
    let fixture = Fixture::new(Some(WORDS), true);
    let terrain = policy_terrain([0x0181, 0xff81], -1, -16 << 9, false, false);
    let math = NativeBasisMath::new(SINE);
    let process = ParticleFactoryProcessState::native_startup();
    for outcome in [Err("target heap unavailable"), Ok(true), Ok(false)] {
        let (mut arena, keeper, camera, mut publication) = setup();
        seed_camera(&mut arena, camera, keeper);
        let camera_before = arena.object(camera).unwrap().clone();
        let mut listeners = CreationListeners::new();
        let mut effects = RequiredEffects;
        let mut host = Observe::new(&terrain);
        let allocation_start = allocation_fixture(&mut arena, outcome);
        let error = setup_attract_actor(
            &mut arena,
            AttractActorSetupRequest {
                publication: &mut publication,
                factory: factory(
                    &fixture.runtime,
                    &math,
                    &process,
                    &mut listeners,
                    &mut effects,
                ),
                camera: keeper,
                settings: SETTINGS, // genuine live noncamera
            },
            &mut host,
        )
        .unwrap_err();
        match outcome {
            Err(_) => assert!(matches!(
                error,
                AttractActorSetupError::Allocation(ActorServiceHeapError::FixtureFailure(71))
            )),
            Ok(_) => assert!(
                matches!(error, AttractActorSetupError::Object(ObjectError::NotSelector16(id)) if id == keeper)
            ),
        }
        assert_eq!(
            allocation_classes(&arena, allocation_start),
            [NativeTargetClass::ActorFollow]
        );
        let before = &host.calls[0];
        let mut placed = before.state.clone();
        placed.base.position_words = Some([0x0181, 496, 0xff81]);
        assert_eq!(arena.object(before.object).unwrap(), &placed);
        assert_eq!(custody(&arena, Some(before.object)), before.custody);
        assert_eq!(arena.object(camera).unwrap(), &camera_before);
        assert_eq!(publication.object_5e0, Some(before.object));
        assert_eq!(publication.handle_5dc, Some(before.cache.handle));
    }
}

#[test]
fn null_allocation_skips_publication_and_new_registry_reads_but_uses_settings_lowbyte() {
    for raw in [0x100, 0x101] {
        let (mut arena, keeper, camera, _) = setup();
        seed_camera(&mut arena, camera, keeper);
        arena.actor_handles = None; // declared absent registry, no old primary
        let publication = AttractActorSetupState {
            object_5e0: None,
            handle_5dc: Some(ActorHandle::from_raw(0xffffffff)),
        };
        let initial = custody(&arena, None);
        let camera_before = arena.object(camera).unwrap().clone();
        let allocation_start = allocation_fixture(&mut arena, Ok(false));
        attach_attract_camera::<()>(
            &mut arena,
            AttractCameraRelationRequest {
                publication: &publication,
                camera,
                settings: CameraSettings {
                    first_person: raw,
                    camera_style: 1,
                },
            },
        )
        .unwrap();
        assert_eq!(
            allocation_classes(&arena, allocation_start),
            [NativeTargetClass::ActorFollow]
        );
        assert_eq!(
            arena.object(camera).unwrap(),
            &expected_camera(&camera_before, None, if raw == 0x100 { 3 } else { 2 })
        );
        assert_eq!(
            relation_custody(&arena, None, Some((camera, &camera_before)), None),
            initial
        );
        assert_eq!(publication.object_5e0, None);
        assert_eq!(publication.handle_5dc.unwrap().raw(), 0xffffffff);
    }
}

#[test]
fn true_allocation_reads_fresh_actor_handle_and_preserves_constructor_unknowns() {
    // These supplied lower-phase scalar domains are independent of completed
    // placement: stale +84 and unknown XYZ do not claim native writer ownership.
    for case in ["live", "stale", "unknown_xyz"] {
        let (mut arena, actor, camera, mut publication) = setup();
        seed_camera(&mut arena, camera, actor);
        arena
            .copy_actor_position(actor, [0x1111, 0xfff8, 0x2222])
            .unwrap();
        arena.actor_mut(actor).unwrap().word_74 = 0xfff0;
        if case == "stale" {
            let retired = arena.allocate_actor(allocation()).unwrap();
            let stale = arena.actor(retired).unwrap().handle;
            arena
                .destroy(crate::object_runtime::ObjectDestructionRequest {
                    object: retired,
                    sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                })
                .unwrap();
            arena.actor_mut(actor).unwrap().handle = stale;
        }
        if case == "unknown_xyz" {
            arena.owned_mut(actor).unwrap().state.base.position_words = None;
        }
        publication.object_5e0 = Some(actor);
        // A distinct real handle makes substitution of game+5DC observable.
        publication.handle_5dc = Some(arena.actor(camera).unwrap().handle);
        let handle = arena.actor(actor).unwrap().handle;
        let camera_before = arena.object(camera).unwrap().clone();
        let actor_before = arena.object(actor).unwrap().clone();
        let registry_before = arena.actor_handles.clone();
        let mut registry_expected = registry_before.clone();
        registry_expected.as_mut().unwrap().resolve(handle);
        let initial = custody(&arena, None);
        let allocation_start = allocation_fixture(&mut arena, Ok(true));
        let result = attach_attract_camera::<()>(
            &mut arena,
            AttractCameraRelationRequest {
                publication: &publication,
                camera,
                settings: CameraSettings {
                    first_person: 0x101,
                    camera_style: 1,
                },
            },
        );
        if case == "unknown_xyz" {
            assert!(
                matches!(result, Err(AttractActorSetupError::Object(ObjectError::ActorPositionUnknown(id))) if id == actor)
            );
            assert_eq!(arena.object(camera).unwrap(), &camera_before);
        } else {
            result.unwrap();
            let focus = (case == "live").then_some([0x1111, 0xffe8, 0x2222]);
            let target = super::super::ActorFollowTarget {
                handle_04: handle,
                word_10: 2,
                focus_08: focus,
                focus_14: focus,
                velocity_1a: None,
            };
            assert_eq!(
                arena.object(camera).unwrap(),
                &expected_camera(
                    &camera_before,
                    Some(camera_target(&arena, camera, target)),
                    if case == "live" { 3 } else { 2 }
                )
            );
        }
        assert_eq!(
            allocation_classes(&arena, allocation_start),
            [NativeTargetClass::ActorFollow]
        );
        assert_eq!(
            format!("{:?}", arena.actor_handles),
            format!("{registry_expected:?}")
        );
        assert_eq!(arena.object(actor).unwrap(), &actor_before);
        assert_eq!(
            relation_custody(
                &arena,
                None,
                Some((camera, &camera_before)),
                Some(&registry_before)
            ),
            initial
        );
    }
}

#[test]
fn old_family_reset_releases_only_target_and_preserves_camera_vectors_and_referents() {
    let (mut arena, keeper, camera, publication) = setup();
    let family = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: allocation(),
                world_mask_limits: [3, 3],
            },
            &mut 19,
        )
        .unwrap();
    let target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(family).unwrap().handle,
        ))
        .unwrap();
    let target = arena.import_fixture_target(NativeTarget::ActorFollow(target));
    arena
        .replace_camera_target(
            camera,
            Some(target),
            CameraSettings {
                first_person: 1,
                camera_style: 0,
            },
        )
        .unwrap();
    seed_camera(&mut arena, camera, keeper);
    arena.actor_mut(family).unwrap().byte_d0 = 0x55; // supplied current view byte
    let camera_before = arena.object(camera).unwrap().clone();
    let mut family_expected = arena.object(family).unwrap().clone();
    family_expected.base.flags |= 1;
    family_expected.base.dirty_word |= 2;
    let ObjectClass::Actor(actor) = &mut family_expected.class else {
        panic!("family")
    };
    actor.byte_d0 = 0;
    let keeper_before = arena.object(keeper).unwrap().clone();
    let roster = arena.actor_list().to_vec();
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    attach_attract_camera::<()>(
        &mut arena,
        AttractCameraRelationRequest {
            publication: &publication,
            camera,
            settings: CameraSettings {
                first_person: 0x101,
                camera_style: 1,
            },
        },
    )
    .unwrap();
    assert_eq!(
        allocation_classes(&arena, allocation_start),
        [NativeTargetClass::ActorFollow]
    );
    assert_eq!(
        arena.object(camera).unwrap(),
        &expected_camera(&camera_before, None, 2)
    );
    assert_eq!(arena.object(family).unwrap(), &family_expected);
    assert_eq!(
        arena.actor_service_heap().state(target),
        Ok(ActorServiceResourceState::Released)
    );
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert_eq!(arena.actor_list(), roster);
    let flags = |list| {
        arena
            .list(list)
            .iter()
            .filter(|node| node.kind == ObjectNodeKind::Flag0)
            .map(|node| node.object)
            .collect::<Vec<_>>()
    };
    assert!(flags(ObjectList::Flag0Primary).contains(&family));
    assert!(!flags(ObjectList::Parking).contains(&family));
    assert_eq!(arena.actor_handles().unwrap().cache().object, Some(family));
}

#[test]
fn replacement_errors_do_not_reach_attract_byte_or_parameter_stores() {
    let (mut arena, old, camera, publication) = setup();
    let old_target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(old).unwrap().handle,
        ))
        .unwrap();
    let old_target = arena.import_fixture_target(NativeTarget::ActorFollow(old_target));
    arena
        .replace_camera_target(camera, Some(old_target), SETTINGS)
        .unwrap();
    seed_camera(&mut arena, camera, old);
    arena.actor_handles = None;
    let initial = custody(&arena, None);
    let allocation_start = allocation_fixture(&mut arena, Ok(false));
    assert!(matches!(
        attach_attract_camera::<()>(
            &mut arena,
            AttractCameraRelationRequest {
                publication: &publication,
                camera,
                settings: SETTINGS,
            },
        ),
        Err(AttractActorSetupError::Object(
            ObjectError::MissingActorRegistry
        ))
    ));
    assert_eq!(
        allocation_classes(&arena, allocation_start),
        [NativeTargetClass::ActorFollow]
    );
    assert_eq!(custody(&arena, None), initial); // old primary is still installed
    assert!(matches!(
        arena.actor_service_heap().state(old_target),
        Ok(ActorServiceResourceState::Target(_))
    ));

    // Independently supplied invalid class custody after a genuine target
    // construction exposes the later publication stop of the private tail.
    let (mut arena, old, camera, _) = setup();
    let old_target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(old).unwrap().handle,
        ))
        .unwrap();
    let old_target = arena.import_fixture_target(NativeTarget::ActorFollow(old_target));
    arena
        .replace_camera_target(camera, Some(old_target), SETTINGS)
        .unwrap();
    seed_camera(&mut arena, camera, old);
    let new = arena.allocate_actor(allocation()).unwrap();
    let new_target = arena
        .follow_target(FollowTargetRequest::resolve_actor(
            arena.actor(new).unwrap().handle,
        ))
        .unwrap();
    let new_target = arena.import_fixture_target(NativeTarget::ActorFollow(new_target));
    arena.owned_mut(new).unwrap().state.class = ObjectClass::Base;
    let mut camera_expected = arena.object(camera).unwrap().clone();
    let ObjectClass::Actor(actor) = &mut camera_expected.class else {
        panic!("camera")
    };
    let ActorExtension::Selector16(state) = &mut actor.extension else {
        panic!("selector16")
    };
    state.target_18c = Some(new_target);
    state.mode_188 = 3;
    assert!(matches!(
        arena.complete_attract_camera_relation(camera, Some(new_target), SETTINGS),
        Err(ObjectError::MissingActorRegistry)
    ));
    assert_eq!(arena.object(camera).unwrap(), &camera_expected);
    assert!(arena.object(old).is_ok());
    assert!(arena.object(new).is_ok());
}
