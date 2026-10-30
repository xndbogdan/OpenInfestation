use super::*;
use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
    },
};
use sha2::{Digest, Sha256};
use std::path::Path;

static UNUSED_SINE: [u16; 4096] = [0; 4096];

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Material(i32),
    Fade(SceneFade),
}

struct Host {
    events: Vec<Event>,
    material: Option<Result<u32, ObjectDrawServiceError<&'static str>>>,
    fail_scene: bool,
}

impl Host {
    fn color(value: u32) -> Self {
        Self {
            events: Vec::new(),
            material: Some(Ok(value)),
            fail_scene: false,
        }
    }
}

impl Flag0ObjectDrawHost for Host {
    type Error = &'static str;

    fn scene(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        let SceneCommand::PublishFade(fade) = command else {
            panic!("flash publishes only the fade owner");
        };
        self.events.push(Event::Fade(fade));
        if self.fail_scene {
            Err("fade publication stopped")
        } else {
            Ok(())
        }
    }

    fn attract_demo(&mut self) -> u8 {
        panic!("flash does not query the attract policy");
    }

    fn model(
        &mut self,
        _request: super::super::ObjectModelDrawSubmission<'_>,
    ) -> Result<(), ObjectDrawServiceError<Self::Error>> {
        panic!("the enclosing owner submits the model");
    }

    fn material_color(
        &mut self,
        argument: i32,
    ) -> Result<u32, ObjectDrawServiceError<Self::Error>> {
        self.events.push(Event::Material(argument));
        self.material.take().expect("one reached H7 lookup")
    }
}

fn fixture(position: Option<[u16; 3]>, extent: u16) -> (ObjectArena, ObjectId, ObjectDrawContext) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 3,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    // This completed shared constructor branch does not require H12/XYZ/sine.
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &UNUSED_SINE,
            },
        })
        .unwrap();
    arena
        .initialize_context(
            actor,
            BaseContext {
                position_words: position,
                placement_index: 7,
            },
        )
        .unwrap();
    // Authored current extent, independent of the H8 header allocation owner.
    arena.owned_mut(actor).unwrap().state.base.word_28 = extent;
    let context = ObjectDrawContext {
        view: SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        },
        near: 7,
        far: 100,
        far_color: 0x7654_3210,
    };
    (arena, actor, context)
}

fn saved(arena: &ObjectArena, actor: ObjectId) -> (Option<u32>, Option<i32>, Option<i32>) {
    let state = arena.actor(actor).unwrap();
    (
        state.flash_far_color_b8,
        state.flash_near_bc,
        state.flash_far_c0,
    )
}

fn missing(actor: ObjectId, offset: u16) -> Flag0ObjectDrawError<(), &'static str> {
    unowned(
        actor,
        ObjectDrawCallbackBoundary::ActorFlashField { offset },
    )
}

fn assert_error(
    actual: Result<(), Flag0ObjectDrawError<(), &'static str>>,
    expected: Flag0ObjectDrawError<(), &'static str>,
) {
    match (actual, expected) {
        (
            Err(Flag0ObjectDrawError::UnownedCallback {
                object: actual,
                reason,
            }),
            Flag0ObjectDrawError::UnownedCallback {
                object: expected,
                reason: expected_reason,
            },
        ) => assert_eq!((actual, reason), (expected, expected_reason)),
        (Err(Flag0ObjectDrawError::Host(actual)), Flag0ObjectDrawError::Host(expected)) => {
            assert_eq!(actual, expected);
        }
        (actual, expected) => panic!("unexpected flash error: {actual:?}; expected {expected:?}"),
    }
}

#[test]
fn constructor_keeps_flash_save_and_color_holes_unknown() {
    let (arena, actor, _) = fixture(None, 0);
    assert_eq!(saved(&arena, actor), (None, None, None));
    assert_eq!(arena.actor(actor).unwrap().word_c8, 0);
    assert_eq!(arena.actor(actor).unwrap().word_ca, None);
}

#[test]
fn missing_position_stops_before_overwriting_actual_saves() {
    let (mut arena, actor, mut context) = fixture(None, 10);
    let entry = context;
    let state = arena.actor_mut(actor).unwrap();
    state.flash_far_color_b8 = Some(1);
    state.flash_near_bc = Some(2);
    state.flash_far_c0 = Some(3);
    let mut host = Host::color(0xabcdef01);
    assert_error(
        enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host),
        unowned(actor, ObjectDrawCallbackBoundary::Position),
    );
    assert_eq!(saved(&arena, actor), (Some(1), Some(2), Some(3)));
    assert_eq!(context, entry);
    assert!(host.events.is_empty());
}

#[test]
fn exact_gate_equality_saves_without_reaching_h7_or_fade() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 73]), u16::MAX);
    let entry = context;
    let mut host = Host::color(0);
    enter::<(), _>(&mut arena, actor, &mut context, i32::MAX, -1, &mut host).unwrap();
    assert_eq!(
        saved(&arena, actor),
        (Some(entry.far_color), Some(7), Some(100))
    );
    assert_eq!(context, entry);
    assert!(host.events.is_empty());
    restore::<(), _>(&arena, actor, &mut context, &mut host).unwrap();
    assert_eq!(host.events, [Event::Fade(SceneFade::new([7, 100]))]);
}

#[test]
fn negative_gate_uses_signed_division_toward_zero() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0xffff]), 10);
    context.near = 0;
    context.far = -1; //(-3)/4 is0; SAR2 alone would be-1 and skip entry.
    let mut host = Host::color(0x4433_2211);
    enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host).unwrap();
    assert_eq!(
        (context.near, context.far, context.far_color),
        (19, 19, 0x4433_2211)
    );
    assert_eq!(
        host.events,
        [Event::Material(55), Event::Fade(SceneFade::new([19, 19]))]
    );
}

#[test]
fn gate_threshold_preserves_wrapped_dword_far_arithmetic() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), 10);
    context.near = 1;
    context.far = 0x4000_0001;
    let entry = context;
    let mut host = Host::color(0);
    //3*far-near wraps to-1073741822; signed threshold=-268435455.
    enter::<(), _>(&mut arena, actor, &mut context, 0, -1, &mut host).unwrap();
    assert_eq!(context, entry);
    assert_eq!(
        saved(&arena, actor),
        (Some(entry.far_color), Some(1), Some(entry.far))
    );
    assert!(host.events.is_empty());
}

#[test]
fn coordinate_deltas_subtract_eye_low_words_with_word_wrap() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0x8001]), 4);
    context.view.eye = [0, 0, 0x17fff]; //LOW WORD7FFF; native Z delta=2.
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host).unwrap();
    assert_eq!((context.near, context.far), (10, 10));
}

#[test]
fn forward_dot_keeps_z_then_y_then_x_accumulation_before_truncation() {
    let (mut arena, actor, mut context) = fixture(Some([1, 1, 1]), 0);
    context.view.basis.words[7] = 1.0_f32.to_bits();
    context.view.basis.words[8] = (-18_014_398_509_481_984.0_f32).to_bits();
    context.view.basis.words[9] = 18_014_398_509_481_984.0_f32.to_bits();
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host).unwrap();
    assert_eq!((context.near, context.far), (1, 1));
}

#[test]
fn unsigned_high_extent_and_wrapped_dword_product_remain_native() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0xfffd]), u16::MAX);
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 32767, 55, &mut host).unwrap();
    //diameter131070; product/shift wrap gives-12; depth-3.
    assert_eq!((context.near, context.far), (131079, 131067));
}

#[test]
fn shifted_correction_is_sign_extended_from_its_low_word() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 5]), 40000);
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 10000, 55, &mut host).unwrap();
    //1600000000>>15=48828; signed LOW WORD=-16708, not48828.
    assert_eq!((context.near, context.far), (96713, 80005));
}

#[test]
fn full_intensity_high_word_does_not_change_the_signed_low_word_consumer() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), u16::MAX);
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 0x1234_8001, 55, &mut host).unwrap();
    //Signed intensity-32767; wrapped correction11.
    assert_eq!((context.near, context.far), (131059, 131070));
}

#[test]
fn supplied_full_h7_argument_reaches_the_host_intact() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), 10);
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 0, 0x1234_0037, &mut host).unwrap();
    assert_eq!(host.events[0], Event::Material(0x1234_0037));
}

#[test]
fn h7_feature_omission_retains_saves_and_original_context() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), 10);
    let entry = context;
    let reason = ObjectDrawCallbackBoundary::MaterialFeature {
        argument: -1,
        reason: "signed low-word index is negative".into(),
    };
    let mut host = Host::color(0);
    host.material = Some(Err(ObjectDrawServiceError::UnownedCallback(reason)));
    assert_error(
        enter::<(), _>(&mut arena, actor, &mut context, 0, -1, &mut host),
        unowned(
            actor,
            ObjectDrawCallbackBoundary::MaterialFeature {
                argument: -1,
                reason: "signed low-word index is negative".into(),
            },
        ),
    );
    assert_eq!(
        saved(&arena, actor),
        (Some(entry.far_color), Some(7), Some(100))
    );
    assert_eq!(context, entry);
    assert_eq!(host.events, [Event::Material(-1)]);
}

#[test]
fn h7_backend_error_is_hard_and_preserves_the_completed_save_prefix() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), 10);
    let entry = context;
    let mut host = Host::color(0);
    host.material = Some(Err(ObjectDrawServiceError::Backend(
        "resource custody stopped",
    )));
    assert_error(
        enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host),
        Flag0ObjectDrawError::Host("resource custody stopped"),
    );
    assert_eq!(
        saved(&arena, actor),
        (Some(entry.far_color), Some(7), Some(100))
    );
    assert_eq!(context, entry);
}

#[test]
fn enter_fade_failure_retains_all_context_stores_without_restoration() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 5]), 10);
    let entry = context;
    let mut host = Host::color(0x8765_4321);
    host.fail_scene = true;
    assert_error(
        enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host),
        Flag0ObjectDrawError::Host("fade publication stopped"),
    );
    assert_eq!(
        (context.near, context.far, context.far_color),
        (25, 25, 0x8765_4321)
    );
    assert_eq!(
        saved(&arena, actor),
        (Some(entry.far_color), Some(7), Some(100))
    );
    assert_eq!(
        host.events,
        [Event::Material(55), Event::Fade(SceneFade::new([25, 25]))]
    );
}

#[test]
fn restore_unknown_fields_preserves_each_preceding_context_store() {
    for (known, offset, expected) in [
        (0, 0xb8, (0x7654_3210, 7, 100)),
        (1, 0xbc, (0xaabb_ccdd, 7, 100)),
        (2, 0xc0, (0xaabb_ccdd, -33, 100)),
    ] {
        let (mut arena, actor, mut context) = fixture(None, 0);
        let state = arena.actor_mut(actor).unwrap();
        if known >= 1 {
            state.flash_far_color_b8 = Some(0xaabb_ccdd);
        }
        if known >= 2 {
            state.flash_near_bc = Some(-33);
        }
        let mut host = Host::color(0);
        assert_error(
            restore::<(), _>(&arena, actor, &mut context, &mut host),
            missing(actor, offset),
        );
        assert_eq!((context.far_color, context.near, context.far), expected);
        assert!(host.events.is_empty());
    }
}

#[test]
fn restore_rereads_current_actor_saves_and_retains_them_after_publication_failure() {
    let (mut arena, actor, mut context) = fixture(Some([0, 0, 0]), 10);
    let mut host = Host::color(123);
    enter::<(), _>(&mut arena, actor, &mut context, 0, 55, &mut host).unwrap();
    let state = arena.actor_mut(actor).unwrap();
    state.flash_far_color_b8 = Some(0xabcd_ef01);
    state.flash_near_bc = Some(-17);
    state.flash_far_c0 = Some(301);
    host.fail_scene = true;
    assert_error(
        restore::<(), _>(&arena, actor, &mut context, &mut host),
        Flag0ObjectDrawError::Host("fade publication stopped"),
    );
    assert_eq!(
        (context.far_color, context.near, context.far),
        (0xabcd_ef01, -17, 301)
    );
    assert_eq!(
        saved(&arena, actor),
        (Some(0xabcd_ef01), Some(-17), Some(301))
    );
    assert_eq!(
        host.events.last(),
        Some(&Event::Fade(SceneFade::new([-17, 301])))
    );
}

#[test]
fn complete_original_flash_bodies_and_h7_empty_callback_are_guarded() {
    let raw = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .expect("required original retail executable");
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = inf_formats::FrontendExecutable::parse(&raw).unwrap();
    // Complete .text spans use fixed raw offsets after whole EXE/profile
    // authentication. FrontendExecutable::mapped intentionally covers data.
    for (offset, length, digest) in [
        (
            515744,
            248,
            "8b6859bc8be76a8e196370acd94d8d8dc3a191b91f8b582c11c16dd05016e2cd",
        ),
        (
            516000,
            44,
            "0a7192191207dc748a93ff51fac061d4beb3f9bc33ef7b3778aebb92bc882f11",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[offset..offset + length])),
            digest
        );
    }
    assert_eq!(
        executable.mapped(0x0052_76b0, 4).unwrap().bytes,
        0x004e_fbf0_u32.to_le_bytes()
    );
    assert_eq!(&raw[982000..982005], [0x33, 0xc0, 0xc2, 0x04, 0x00]);
}
