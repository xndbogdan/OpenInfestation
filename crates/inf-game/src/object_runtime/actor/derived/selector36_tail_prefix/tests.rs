use super::*;
use crate::frontend_camera::AnimationClock;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, FixedPositionTarget, NativeTarget, ObjectModelCallbacks,
};

pub(super) fn assets() -> &'static crate::frontend_assets::FrontendAssets {
    static ASSETS: std::sync::OnceLock<crate::frontend_assets::FrontendAssets> =
        std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        crate::frontend_assets::FrontendAssets::open(&root).unwrap()
    })
}
pub(super) fn executable() -> FrontendExecutable<'static> {
    FrontendExecutable::parse(assets().executable_bytes()).unwrap()
}
pub(super) fn process() -> ProcessActorMotionState {
    ProcessActorMotionState::from_executable(executable()).unwrap()
}
pub(super) fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut random_state = 0;
    let actor = arena
        .allocate_selector36_actor(construction(), &mut random_state)
        .unwrap();
    (arena, actor)
}
pub(super) fn construction() -> DerivedActorConstructionRequest<'static> {
    DerivedActorConstructionRequest {
        actor: ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([384, 96, 512]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &assets().sine_table().values,
            },
        },
        world_mask_limits: [3; 2],
    }
}
pub(super) struct RequiredMissFixture;
pub(super) struct RequiredChildFixture;
impl crate::object_runtime::ActorChildHost for RequiredChildFixture {
    type Error = Selector36ChildServiceError;
    fn create_child_object(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::ActorChildFactoryCall,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, Selector36ChildServiceError> {
        Err("fixture child factory unavailable".into())
    }
    fn resolve_child_actor(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<ObjectId>, Selector36ChildServiceError> {
        panic!("factory stops first")
    }
    fn child_source_basis(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<inf_formats::native_basis::NativeBasis>, Selector36ChildServiceError> {
        panic!("factory stops first")
    }
    fn create_registry_object(
        &mut self,
        _: &mut ObjectArena,
        _: u16,
    ) -> Result<Option<ObjectId>, Selector36ChildServiceError> {
        panic!("factory stops first")
    }
    fn take_shot(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: crate::object_runtime::ActorHandle,
    ) -> Result<bool, Selector36ChildServiceError> {
        panic!("factory stops first")
    }
    fn refresh_child_parent_model(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<(), Selector36ChildServiceError> {
        panic!("factory stops first")
    }
}
impl FamilyTerrainProbeMissHost for RequiredMissFixture {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&crate::terrain_runtime::TerrainRuntime, Self::Error> {
        Err("fixture miss service unavailable")
    }
    fn collide(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        panic!("fixture H12 service stops first")
    }
}

pub(super) fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    process: &ProcessActorMotionState,
    delta: u32,
) -> Result<Selector36TailNextPhase, Selector36TailPrefixError<&'static str>> {
    let math = NativeBasisMath::new(assets().sine_table().values);
    let inverse = inf_formats::parse_frontend_inverse_sine_table(
        executable().data(),
        inf_formats::FRONTEND_INVERSE_SINE_TABLE_ADDRESS,
    )
    .unwrap();
    let draw = crate::object_runtime::ObjectDrawProcessState::native_startup(executable()).unwrap();
    let mut random_state = 0;
    let mut terrain = RequiredMissFixture;
    arena.tick_selector36_tail_prefix(
        actor,
        Selector36TailPrefixRequest {
            behavior_tables: &mut crate::object_runtime::ProcessActorBehaviorTables::from_executable(executable()).unwrap(),
            process_velocity: None,
            microseconds: delta,
            process,
            settings_7c: 0,
            math: &math,
            inverse_sine: &inverse.values,
            shared_basis: &draw.shared_basis,
            random_state: &mut random_state,
            terrain: &mut terrain,
            child: &mut RequiredChildFixture,
        },
    )
}

#[test]
fn startup_requires_actual_file_backed_counter_word_without_fallback() {
    let state = process();
    assert_eq!(state.selector36_counter_multiplier, 0x4000);
    // Declared structural negative fixture only: shorten a copied .data raw
    // range beforeBDB4. Original EXE/data remains untouched.
    let mut copied = assets().executable_bytes().to_vec();
    let pe = u32::from_le_bytes(copied[0x3c..0x40].try_into().unwrap()) as usize;
    let optional = u16::from_le_bytes(copied[pe + 20..pe + 22].try_into().unwrap()) as usize;
    let sections = pe + 24 + optional;
    let count = u16::from_le_bytes(copied[pe + 6..pe + 8].try_into().unwrap()) as usize;
    let data = (0..count)
        .map(|i| sections + 40 * i)
        .find(|&offset| &copied[offset..offset + 8] == b".data\0\0\0")
        .unwrap();
    copied[data + 16..data + 20].copy_from_slice(&0x8000_u32.to_le_bytes());
    let mapped = FrontendExecutable::parse(&copied).unwrap();
    assert!(matches!(
        ProcessActorMotionState::from_executable(mapped),
        Err(FrontendExecutableError::DataAddress(0x0053_bdb4))
    ));
}

#[test]
fn null_target_halves_only_native_control_words_and_publishes_actual_model_getters() {
    let (mut arena, actor) = fixture();
    let process = process();
    {
        let state = arena
            .selector36_tail_state_mut::<&'static str>(actor)
            .unwrap();
        state.word_268 = 0xabcd_8001;
        state.word_26c = 0x8001_abcd;
        state.word_270 = 0xfff0_fff0;
    }
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 300];
    arena.actor_mut(actor).unwrap().words_94_9c[1] = 64;
    assert_eq!(
        visit(&mut arena, actor, &process, 125_000).unwrap(),
        Selector36TailNextPhase::Motion { controls: [0; 2] }
    );
    let state = arena.selector36_actor(actor).unwrap();
    assert_eq!(
        (state.word_268, state.word_26c, state.word_270),
        (0xabcd_4000, 0x4000_abcd, 0x4010_3fd0)
    );
    let clock = AnimationClock::default();
    let callbacks = ObjectModelCallbacks {
        arena: &arena,
        source: actor,
        animation: &clock,
        executable: executable(),
    };
    assert_eq!(
        (1..=6)
            .map(|selector| callbacks.packed_value(selector).unwrap())
            .collect::<Vec<_>>(),
        vec![0x4000, 0xabcd, 0xabcd, 0x4000, 0x3fd0, 0x4010]
    );
    let derived = arena.derived_actor(actor).unwrap();
    assert!(derived.target_148.is_none());
    assert_eq!(
        (
            derived.word_14c,
            derived.word_150,
            derived.word_158,
            derived.word_15c
        ),
        (None, None, None, None)
    );
    assert_eq!(
        arena.selector36_actor(actor).unwrap().words_278_27c,
        [None; 3]
    );
}

#[test]
fn counter_factor_is_independent_of_visit_delta_and_process_owner_is_reused() {
    let process = process();
    let mut results = Vec::new();
    for delta in [0, 1, 20_000, 125_000, u32::MAX] {
        let (mut arena, actor) = fixture();
        arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 300];
        visit(&mut arena, actor, &process, delta).unwrap();
        results.push(arena.selector36_actor(actor).unwrap().word_270);
    }
    assert_eq!(results, vec![0x4000_4000; 5]);
}

#[test]
fn signed_velocity_ftol_and_negative_angular_sar_retain_native_word_results() {
    let process = process();
    let (mut arena, actor) = fixture();
    {
        let state = arena.actor_mut(actor).unwrap();
        state.rotating.basis.words[7] = 0.5_f32.to_bits();
        state.rotating.basis.words[8] = (-0.25_f32).to_bits();
        state.rotating.basis.words[9] = 1.0_f32.to_bits();
        state.words_8e_92 = [(-5_i16) as u16, 3, 7];
        state.words_94_9c[1] = 32;
    }
    visit(&mut arena, actor, &process, 0).unwrap();
    //3.75→FTOL3;49152/300 truncates163; angular32→16.
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0x00b3_0093);
    arena
        .selector36_tail_state_mut::<&'static str>(actor)
        .unwrap()
        .word_270 = 0;
    arena.actor_mut(actor).unwrap().words_8e_92 = [5, 3, (-7_i16) as u16];
    arena.actor_mut(actor).unwrap().words_94_9c[1] = (-33_i32) as u32;
    visit(&mut arena, actor, &process, 0).unwrap();
    //-5.25→FTOL-5; signed /300→-273; angular-33 arithmeticSAR→-17.
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0xfede_ff00);
}

#[test]
fn post_d8_decay_uses_lowword_wrap_signed_clamp_and_zero_bypass() {
    let process = process();
    for (initial, delta, expected) in [
        (0, u32::MAX, 0),
        (1, 1024, 0),
        (2, 1024, 1),
        (0xffff, 0, 0),
        (0x8001, 0, 0),
        (1, 0x0400_0000, 1),
        (1, 0x03ff_fc00, 2),
    ] {
        let (mut arena, actor) = fixture();
        arena.derived_actor_mut(actor).unwrap().word_17a = initial;
        visit(&mut arena, actor, &process, delta).unwrap();
        assert_eq!(arena.derived_actor(actor).unwrap().word_17a, expected);
    }
}

#[test]
fn nonnull_target_refresh_publishes_status_then_stops_on_actual_focus_after_decay() {
    let process = process();
    let (mut arena, actor) = fixture();
    let target = arena.import_fixture_target(NativeTarget::FixedPosition(FixedPositionTarget {
        handle_04: ActorHandle::from_raw(0),
        focus_08: None,
        word_10: 8,
    }));
    arena.derived_actor_mut(actor).unwrap().target_148 = Some(target);
    arena.derived_actor_mut(actor).unwrap().word_17a = 5;
    let before = format!("{arena:?}");
    assert_eq!(
        visit(&mut arena, actor, &process, 1024),
        Err(Selector36TailPrefixError::RequiredTargetFocus {
            reference: target,
            address: 0x4817d4
        })
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_150, Some(4));
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 4);
    //Fixture-only normalization of the reached decay/status fields proves
    //no target/cache/heap/counter change; no production carry is seeded.
    arena.derived_actor_mut(actor).unwrap().word_17a = 5;
    arena.derived_actor_mut(actor).unwrap().word_150 = None;
    assert_eq!(format!("{arena:?}"), before);
    arena.derived_actor_mut(actor).unwrap().word_17a = 4;
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0);
}

#[test]
fn actual_behavior_presence_consumes_real_weapon_tick_after_counter_prefix() {
    let process = process();
    let (mut arena, actor) = fixture();
    // Explicit supplied guard mode through its actual public admission owner,
    // not a fabricated successful WeaponTick or a production constructor claim.
    arena.derived_actor_mut(actor).unwrap().byte_13d = Some(1);
    arena.admit_derived_actor(actor).unwrap();
    assert!(arena.derived_actor(actor).unwrap().behavior_140.is_some());
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 0, 300];
    let before = arena.derived_actor(actor).unwrap().behavior_140.clone();
    assert!(before
        .as_ref()
        .unwrap()
        .groups_1c
        .iter()
        .all(|g| g.word_0c & 2 == 0));
    assert_eq!(
        visit(&mut arena, actor, &process, 0).unwrap(),
        Selector36TailNextPhase::Motion { controls: [0; 2] }
    );
    assert_eq!(arena.selector36_actor(actor).unwrap().word_270, 0x4000_4000);
    assert_eq!(arena.derived_actor(actor).unwrap().behavior_140, before);
}

#[test]
fn other_real_class_is_rejected_before_any_prefix_write() {
    let process = process();
    let (mut arena, _) = fixture();
    let mut random_state = 0;
    let actor = arena
        .allocate_selector10_actor(construction(), &mut random_state)
        .unwrap();
    arena.derived_actor_mut(actor).unwrap().word_17a = 5;
    let before = format!("{arena:?}");
    assert_eq!(
        visit(&mut arena, actor, &process, 1024),
        Err(Selector36TailPrefixError::UnsupportedClass(actor))
    );
    assert_eq!(format!("{arena:?}"), before);
}
