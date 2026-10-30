use super::*;
use crate::frontend_assets::FrontendAssets;
use crate::object_runtime::{
    construct_world_object, ActorDamageTable, ActorExtension, ActorFactoryInputs, ActorHandle,
    ActorRegistryConfig, ActorTerrainInputs, CreationListener, CreationListenerError,
    CreationListenerInterface, ObjectConstructionOutcome, ObjectConstructionRequest, ObjectList,
    ObjectState, INITIALIZED_FLAG,
};
use crate::{Installation, ResidencyRequest};
use inf_formats::{
    native_basis::NativeBasis, parse_constructor_asset_bank, ConstructorPayloadView, HANDLER_COUNT,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

const SINE: [u16; 4096] = [0; 4096];
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

fn allocation(position: Option<[u16; 3]>) -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null:
            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: position,
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

fn arena(null: u32) -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(null),
    })
    .unwrap()
}

fn resident() -> (ResidentRuntime, NativeBasisMath) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    // Actual selection unloads leaf2 and loads leaf1/48; H13[52] is leaf1.
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let assets = FrontendAssets::open(&root).unwrap();
    (runtime, NativeBasisMath::new(assets.sine_table().values))
}

/// A reached interface is required. Empty-list success never invokes this
/// host, and does not stand in for an unimplemented native callback.
#[derive(Default)]
struct RequiredEffects {
    calls: Vec<ObjectId>,
    owner_handles: Vec<ActorHandle>,
}

impl CreationListenerEffects for RequiredEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        _listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push(object);
        self.owner_handles.push(arena.actor(object)?.owner_handle);
        Err(CreationListenerError::UnsupportedInterface(
            listener.interface,
        ))
    }
}

fn request<'a>(
    runtime: &'a ResidentRuntime,
    math: &'a NativeBasisMath,
    process: &'a ParticleFactoryProcessState,
    listeners: &'a mut CreationListeners,
    effects: &'a mut dyn CreationListenerEffects,
) -> Selector93FactoryRequest<'a> {
    Selector93FactoryRequest {
        allocation: allocation(Some([0x8001, 0xfffe, 0x7fff])),
        process,
        local_owner: 1,
        runtime,
        math,
        listeners,
        listener_effects: effects,
        network_world_active: false,
    }
}

#[test]
fn direct_factory_uses_real_leaf1_model_payload_and_separate_published_owners() {
    let (runtime, math) = resident();
    let resource = runtime.resource(13, 52).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    let index = usize::from(resource.registration.local_index);
    assert_eq!(bank.prefixes.entries[index].constructor_selector, 93);
    let asset = &bank.assets[index];
    assert_eq!(asset.model_binding.unwrap().raw_word, 77);
    let ConstructorPayloadView::SharedActor(payload) = &asset.payload else {
        panic!("original leaf1 slot52 must have the actual shared payload")
    };
    let source = resource.overlay.bytes().to_vec();
    let revision = runtime.revision();
    let process = ParticleFactoryProcessState::native_startup();
    let mut arena = arena(0);
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects::default();
    let first = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap();
    let first_before = arena.object(first).unwrap().clone();
    let second = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap();
    assert_eq!(arena.actor_list(), [first, second]);
    assert_eq!(arena.object(first).unwrap(), &first_before);
    let mut shared_payload: Option<&Arc<inf_formats::ActorConstructorPayload>> = None;
    for object in [first, second] {
        let state = arena.object(object).unwrap();
        let actor = arena.actor(object).unwrap();
        assert_eq!(state.base.position_words, Some([0x8001, 0xfffe, 0x7fff]));
        assert_eq!(state.base.placement_word, u16::MAX);
        assert_eq!(state.base.resource_word, 52);
        assert_eq!(state.base.dirty_word, 0);
        assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(actor.rotating.visual.model_word, 77);
        assert_eq!(actor.rotating.basis, NativeBasis::INITIAL);
        assert_eq!(actor.owner_handle, ActorHandle::from_raw(0));
        assert_eq!(actor.health, payload.initial_health.value);
        assert_eq!(actor.maximum_health, Some(payload.initial_health.value));
        assert_eq!(actor.word_8c, payload.word_08.value);
        assert_eq!(actor.word_76, Some(payload.word_40.value));
        assert_eq!(actor.word_74, state.base.word_28 >> 3);
        let demo = arena.selector93(object).unwrap();
        assert_eq!(demo.dwords_138_140, [0; 3]);
        assert_eq!(demo.dword_144, 0);
        assert!(demo.target_148.is_none());
        let ActorDamageTable::Payload(retained) = &actor.damage_table else {
            panic!("the actual actor retains its shared response allocation")
        };
        if let Some(first_payload) = shared_payload {
            assert!(Arc::ptr_eq(first_payload, retained));
        } else {
            shared_payload = Some(retained);
        }
    }
    assert_ne!(
        arena.actor(first).unwrap().handle,
        arena.actor(second).unwrap().handle
    );
    assert!(effects.calls.is_empty());
    assert_eq!(runtime.revision(), revision);
    assert_eq!(resource.overlay.bytes(), source);
    assert_eq!(process, ParticleFactoryProcessState::native_startup());
    // Destruction is an explicit caller action after successful ownership.
    let first_handle = arena.actor(first).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: first,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.resolve_actor_handle(first_handle).unwrap(), None);
    assert_eq!(arena.actor_list(), [second]);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: second,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.is_empty());
}

#[test]
fn nonzero_matching_process_null_is_retained_without_numeric_default() {
    let (runtime, math) = resident();
    let mut arena = arena(0x1234_5678);
    let session_phase =
        crate::process_particles::factories::phase_fixture::FixtureSessionPhase::supplied(Some(
            0xfeed_face,
        ));
    let process = ParticleFactoryProcessState {
        process_null_handle: Some(ActorHandle::from_raw(0x1234_5678)),
        ..ParticleFactoryProcessState::native_startup()
    };
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects::default();
    let object = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap();
    assert_eq!(
        arena.actor(object).unwrap().owner_handle,
        process.process_null_handle.unwrap()
    );
    assert_eq!(arena.object(object).unwrap().base.dirty_word, 0);
    assert!(effects.calls.is_empty());
    assert_eq!(session_phase.snapshot(), Some(0xfeed_face));
}

#[test]
fn constructor_null_domain_errors_preserve_all_existing_custody() {
    let (runtime, math) = resident();
    let mut arena = arena(0x1234_5678);
    let keeper = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let before = format!("{arena:?}");
    let listener_before = format!("{listeners:?}");
    let mut effects = RequiredEffects::default();
    for supplied in [None] {
        let process = ParticleFactoryProcessState {
            process_null_handle: supplied,
            ..ParticleFactoryProcessState::native_startup()
        };
        let error = construct_selector93_direct(
            &mut arena,
            request(&runtime, &math, &process, &mut listeners, &mut effects),
        )
        .unwrap_err();
        assert!(matches!(error, Selector93FactoryError::MissingProcessNull));
        assert_eq!(format!("{arena:?}"), before);
        assert_eq!(format!("{listeners:?}"), listener_before);
        assert_eq!(process.process_null_handle, supplied);
        assert!(effects.calls.is_empty());
    }
    let mut unconfigured = ObjectArena::new();
    let before = format!("{unconfigured:?}");
    assert!(matches!(
        construct_selector93_direct(
            &mut unconfigured,
            request(
                &runtime,
                &math,
                &ParticleFactoryProcessState::native_startup(),
                &mut listeners,
                &mut effects
            )
        ),
        Err(Selector93FactoryError::Allocation(
            ActorConstructionError::MissingRegistry
        ))
    ));
    assert_eq!(format!("{unconfigured:?}"), before);
    assert_eq!(format!("{listeners:?}"), listener_before);
}

#[test]
fn selector93_factory_listener_and_retained_child_use_current_process_null() {
    let (runtime, math) = resident();
    let mut arena = arena(0x1234_5678);
    let keeper = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let process = ParticleFactoryProcessState {
        process_null_handle: Some(ActorHandle::from_raw(0x7654_3210)),
        ..ParticleFactoryProcessState::native_startup()
    };
    let mut effects = RequiredEffects::default();
    let error = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap_err();
    let Selector93FactoryError::Published { object, source } = error else {
        panic!("listener failure must preserve constructed selector93")
    };
    assert!(matches!(
        source,
        ObjectConstructionError::CreationListener(_)
    ));
    assert_eq!(effects.calls, [object]);
    assert_eq!(effects.owner_handles, [ActorHandle::from_raw(0x7654_3210)]);
    assert_eq!(arena.actor(object).unwrap().owner_handle.raw(), 0x7654_3210);
    assert_eq!(
        arena.actor_handles().unwrap().null_handle().raw(),
        0x1234_5678
    );
    assert_eq!(
        arena.object(object).unwrap().base.flags & INITIALIZED_FLAG,
        0
    );
}

#[test]
fn missing_h13_retains_direct_constructor_but_generic_preflight_publishes_nothing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let inactive = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    let (_, math) = resident();
    let process = ParticleFactoryProcessState::native_startup();
    let mut arena = arena(0);
    let keeper = arena
        .allocate_actor(allocation(Some([11, 22, 33])))
        .unwrap();
    let keeper_before = arena.object(keeper).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects::default();
    assert!(matches!(
        construct_world_object(
            &mut arena,
            &inactive,
            ObjectConstructionRequest {
                resource_argument: 52,
                context: BaseContext {
                    position_words: None,
                    placement_index: u32::MAX
                },
                suffix: None,
                command_allocation: None,
                network_active: false,
                actor: None,
            }
        ),
        Err(ObjectConstructionError::NotResident {
            resource_argument: 52
        })
    ));
    assert_eq!(arena.actor_list(), [keeper]);
    let error = construct_selector93_direct(
        &mut arena,
        request(&inactive, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap_err();
    let Selector93FactoryError::Published { object, source } = error else {
        panic!("missing H13 must retain actual constructor93 publication")
    };
    assert!(matches!(
        source,
        ObjectConstructionError::NotResident {
            resource_argument: 52
        }
    ));
    assert_eq!(arena.actor_list(), [keeper, object]);
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.position_words, Some([0x8001, 0xfffe, 0x7fff]));
    assert_eq!(state.base.dirty_word, 2); // genuine shared allocation flag124 change
    assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
    assert_eq!(state.base.resource_word, 0);
    assert_eq!(
        arena.actor(object).unwrap().rotating.visual.model_word,
        u16::MAX
    );
    assert_eq!(
        arena.actor(object).unwrap().damage_table,
        ActorDamageTable::ConstructorDefault
    );
    assert!(arena.selector93(object).unwrap().target_148.is_none());
    assert!(effects.calls.is_empty());
    let handle = arena.actor(object).unwrap().handle;
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
    assert_eq!(arena.actor_list(), [keeper]);
}

/// Declared arena/list effects exercise completion ordering. This is not an
/// implementation or acceptance claim for either original listener method.
struct MutatingEffects {
    fail: bool,
    calls: Vec<ObjectId>,
    before: Vec<ObjectState>,
}

impl CreationListenerEffects for MutatingEffects {
    fn invoke(
        &mut self,
        listener: CreationListener,
        object: ObjectId,
        listeners: &mut CreationListeners,
        arena: &mut ObjectArena,
    ) -> Result<(), CreationListenerError> {
        self.calls.push(object);
        let state = arena.object(object)?;
        assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
        assert_eq!(state.base.dirty_word, 0);
        assert_eq!(state.base.resource_word, 52);
        assert_eq!(arena.actor(object)?.word_74, state.base.word_28 >> 3);
        self.before.push(state.clone());
        arena.copy_actor_position(object, [7, 0x8000, 9])?;
        arena.set_flag0(object, true)?; // genuine membership/dirty effect
        if self.fail {
            listeners.register(object, CreationListenerInterface::Selector78);
            return Err(CreationListenerError::UnsupportedInterface(
                listener.interface,
            ));
        }
        Ok(())
    }
}

#[test]
fn listener_error_retains_shared_postinit_and_real_callback_prefix_for_manual_cleanup() {
    let (runtime, math) = resident();
    let mut arena = arena(0);
    let keeper = arena.allocate_actor(allocation(Some([1, 2, 3]))).unwrap();
    let keeper_before = arena.object(keeper).unwrap().clone();
    let mut listeners = CreationListeners::new();
    let binding = listeners.register(keeper, CreationListenerInterface::Selector18);
    let mut effects = MutatingEffects {
        fail: true,
        calls: vec![],
        before: vec![],
    };
    let process = ParticleFactoryProcessState {
        network_flag: 0x100,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        ..ParticleFactoryProcessState::native_startup()
    };
    let error = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap_err();
    let Selector93FactoryError::Published { object, source } = error else {
        panic!("a required listener error retains the published object")
    };
    assert!(matches!(
        source,
        ObjectConstructionError::CreationListener(CreationListenerError::UnsupportedInterface(
            CreationListenerInterface::Selector18
        ))
    ));
    assert_eq!(effects.calls, [object]);
    let state = arena.object(object).unwrap();
    assert_eq!(state.base.position_words, Some([7, 0x8000, 9]));
    assert_eq!(state.base.dirty_word, 2); // callback dirty survives; network80 not reached
    assert_eq!(state.base.flags & INITIALIZED_FLAG, 0);
    let mut expected = effects.before[0].clone();
    expected.base.position_words = Some([7, 0x8000, 9]);
    expected.base.flags |= 1;
    expected.base.dirty_word = 2;
    assert_eq!(state, &expected);
    assert_eq!(arena.actor(object).unwrap().rotating.visual.model_word, 77);
    assert_eq!(arena.actor_list(), [keeper, object]);
    assert!(arena
        .list(ObjectList::Flag0Primary)
        .iter()
        .any(|node| node.object == object));
    assert_eq!(arena.object(keeper).unwrap(), &keeper_before);
    assert_eq!(
        listeners
            .registrations()
            .map(|entry| entry.owner)
            .collect::<Vec<_>>(),
        [keeper, object]
    );
    assert_eq!(listeners.get(binding).unwrap().owner, keeper);
    let handle = arena.actor(object).unwrap().handle;
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), Some(object));
    assert_eq!(listeners.unregister_owner(object), 1);
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: object,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert_eq!(arena.actor_list(), [keeper]);
    assert_eq!(listeners.len(), 1);
    assert_eq!(arena.resolve_actor_handle(handle).unwrap(), None);
}

#[test]
fn network_is_current_per_call_and_runs_after_listener_effects_with_remote_bypass() {
    let (runtime, math) = resident();
    let mut arena = arena(0);
    let keeper = arena.allocate_actor(allocation(None)).unwrap();
    let mut listeners = CreationListeners::new();
    listeners.register(keeper, CreationListenerInterface::Selector18);
    let mut effects = MutatingEffects {
        fail: false,
        calls: vec![],
        before: vec![],
    };
    let mut process = ParticleFactoryProcessState::native_startup();
    let offline = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap();
    assert_eq!(arena.object(offline).unwrap().base.dirty_word, 2);
    let offline_before = arena.object(offline).unwrap().clone();
    // Real retained process state changes between calls, then is reborrowed.
    // No mutable native callback/global alias is claimed during one call.
    process.network_flag = 0x100;
    let local = construct_selector93_direct(
        &mut arena,
        request(&runtime, &math, &process, &mut listeners, &mut effects),
    )
    .unwrap();
    assert_eq!(arena.object(local).unwrap().base.dirty_word, 0x80);
    assert_eq!(arena.object(offline).unwrap(), &offline_before);
    let mut remote_request = request(&runtime, &math, &process, &mut listeners, &mut effects);
    remote_request.local_owner = 2;
    remote_request.network_world_active = true;
    let remote = construct_selector93_direct(&mut arena, remote_request).unwrap();
    assert_ne!(arena.object(remote).unwrap().base.flags & 0x8000_0000, 0);
    assert_eq!(arena.object(remote).unwrap().base.dirty_word, 0);
    assert_eq!(effects.calls, [offline, local, remote]);
    assert_eq!(arena.actor_list(), [keeper, offline, local, remote]);
    for (index, (object, dirty)) in [(offline, 2), (local, 0x80), (remote, 0)]
        .into_iter()
        .enumerate()
    {
        let state = arena.object(object).unwrap();
        assert_eq!(state.base.position_words, Some([7, 0x8000, 9]));
        assert_eq!(state.base.placement_word, u16::MAX);
        assert_ne!(state.base.flags & INITIALIZED_FLAG, 0);
        let mut expected = effects.before[index].clone();
        expected.base.position_words = Some([7, 0x8000, 9]);
        expected.base.flags |= 1 | INITIALIZED_FLAG;
        expected.base.dirty_word = dirty;
        assert_eq!(state, &expected);
    }
    assert_eq!(process.network_flag, 0x100);
}

/// Supplied structural installation, independent of the immutable retail
/// corpus. All 53 H13 slots alias one genuine SharedActor layout tagged90.
/// This is a parser/current-input discriminator, not a native header writer.
struct SharedTagFixture {
    root: PathBuf,
    runtime: ResidentRuntime,
}

impl SharedTagFixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "inf-selector93-fixed-class-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("Overlay")).unwrap();
        let mut preload = vec![0, 0];
        for handler in 0..HANDLER_COUNT {
            preload.extend_from_slice(&(if handler == 13 { 53_u16 } else { 0 }).to_le_bytes());
        }
        preload.extend_from_slice(&0_u32.to_le_bytes());
        preload.extend_from_slice(&[1, 1, 0, 0]);
        for phase in 0..4 {
            for handler in 0..HANDLER_COUNT {
                let populated = phase == 0 && handler == 13;
                preload.extend_from_slice(&(if populated { 53_i16 } else { 0 }).to_le_bytes());
                preload.extend_from_slice(&u16::from(populated).to_le_bytes());
                if populated {
                    preload.extend_from_slice(&0_u32.to_le_bytes());
                }
            }
        }
        fs::write(root.join("PreLoad.dat"), preload).unwrap();
        let mut main = 89_u16.to_le_bytes().to_vec(); // header tag89 -> generic selector90
        main.extend_from_slice(&u16::MAX.to_le_bytes()); // genuinely disabled H8
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
        let mut overlay = (main.len() as u32).to_le_bytes().to_vec();
        overlay.extend_from_slice(&main);
        overlay.extend_from_slice(&0_u32.to_le_bytes()); // no relocations
        for _ in 0..53 {
            overlay.extend_from_slice(&0_u32.to_le_bytes());
        }
        fs::write(root.join("Overlay/0.ovl"), overlay).unwrap();
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        Self { root, runtime }
    }
}

impl Drop for SharedTagFixture {
    fn drop(&mut self) {
        assert_eq!(self.root.parent(), Some(std::env::temp_dir().as_path()));
        assert!(self
            .root
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("inf-selector93-fixed-class-"));
        fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn direct_class_is_fixed_while_generic_dispatch_uses_the_same_shared_header_tag() {
    let fixture = SharedTagFixture::new();
    let resource = fixture.runtime.resource(13, 52).unwrap();
    let region = &resource.overlay.layout().regions[resource.registration.region_index];
    let bank = parse_constructor_asset_bank(resource.overlay.bytes(), region).unwrap();
    let index = usize::from(resource.registration.local_index);
    assert_eq!(bank.prefixes.entries[index].constructor_selector, 90);
    assert!(matches!(
        bank.assets[index].payload,
        ConstructorPayloadView::SharedActor(_)
    ));
    let original = resource.overlay.bytes().to_vec();
    let math = NativeBasisMath::new(SINE); // no rotation or terrain consumes it
    let process = ParticleFactoryProcessState::native_startup();
    let mut arena = arena(0);
    let mut listeners = CreationListeners::new();
    let mut effects = RequiredEffects::default();
    let direct = construct_selector93_direct(
        &mut arena,
        request(
            &fixture.runtime,
            &math,
            &process,
            &mut listeners,
            &mut effects,
        ),
    )
    .unwrap();
    let generic = construct_world_object(
        &mut arena,
        &fixture.runtime,
        ObjectConstructionRequest {
            resource_argument: 52,
            context: BaseContext {
                position_words: None,
                placement_index: u32::MAX,
            },
            suffix: None,
            command_allocation: None,
            network_active: false,
            actor: Some(ActorFactoryInputs {
                allocation: allocation(Some([0x8001, 0xfffe, 0x7fff])),
                rotation_words: None,
                local_owner: 1,
                first_player_grants: None,
                grant_table: None,
                math: &math,
                derived: None,
                listeners: &mut listeners,
                listener_effects: &mut effects,
                network_world_active: false,
                terrain: None,
                world_start: None,
                camera_settings: None,
                spawn_mode_1: false,
                network_handle: None,
                vehicle_table: None,
            }),
        },
    )
    .unwrap();
    let ObjectConstructionOutcome::Constructed(generic) = generic else {
        panic!("the supplied shared90 descriptor must have no rejection")
    };
    assert!(matches!(
        arena.actor(direct).unwrap().extension,
        ActorExtension::Selector93(_)
    ));
    assert!(matches!(
        arena.actor(generic).unwrap().extension,
        ActorExtension::Selector90(_)
    ));
    let direct_actor = arena.actor(direct).unwrap();
    let generic_actor = arena.actor(generic).unwrap();
    let mut expected = direct_actor.clone();
    expected.handle = generic_actor.handle;
    expected.extension = generic_actor.extension.clone();
    assert_eq!(generic_actor, &expected);
    assert_eq!(
        arena.object(direct).unwrap().base,
        arena.object(generic).unwrap().base
    );
    let (ActorDamageTable::Payload(first), ActorDamageTable::Payload(second)) =
        (&direct_actor.damage_table, &generic_actor.damage_table)
    else {
        unreachable!()
    };
    assert!(Arc::ptr_eq(first, second));
    assert_eq!(direct_actor.health, 0x2468);
    assert_eq!(direct_actor.word_8c, 7);
    assert_eq!(direct_actor.rotating.visual.model_word, u16::MAX);
    assert_eq!(arena.actor_list(), [direct, generic]);
    assert!(effects.calls.is_empty());
    assert_eq!(resource.overlay.bytes(), original);
}
