//! Declared contact inputs; genuine constructor/EXE/sine and prepared H12
//! algorithms execute. This does not provide a native stack address, collision
//! success, motion/audio completion or a loaded intro acceptance witness.
use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_camera::AnimationClock,
    frontend_screen::FrontendProcessState,
    object_runtime::{
        ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
        DerivedActorConstructionRequest,
    },
    terrain_runtime::native_probe_fixture_terrain,
};
use std::{cell::RefCell, rc::Rc};

pub(super) fn assets() -> &'static FrontendAssets {
    static ASSETS: std::sync::OnceLock<FrontendAssets> = std::sync::OnceLock::new();
    ASSETS.get_or_init(|| {
        FrontendAssets::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
            .unwrap()
    })
}
fn executable() -> FrontendExecutable<'static> {
    FrontendExecutable::parse(assets().executable_bytes()).unwrap()
}
fn actor(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let mut constructor_rng = 0;
    let actor = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: position,
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
            },
            &mut constructor_rng,
        )
        .unwrap();
    assert_eq!(
        arena.family4236d0_actor(actor).unwrap().table_25c,
        0x0052_fe58
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, None);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 64;
    (arena, actor)
}

fn terrain(plane: i32, heights: [i8; 4]) -> TerrainRuntime {
    native_probe_fixture_terrain(&serde_json::json!({
        "spacing":128,"height_shift":4,"plane_base":plane << 9,"plane_shift":9,
        "xz":[0,0],"heights":heights,"alternate":false,
    }))
}
struct Terrain<'a> {
    initial: &'a TerrainRuntime,
    normal: Option<&'a TerrainRuntime>,
    calls: usize,
    fail_on: Option<usize>,
    events: Rc<RefCell<Vec<&'static str>>>,
}
impl<'a> Terrain<'a> {
    fn new(initial: &'a TerrainRuntime) -> Self {
        Self {
            initial,
            normal: None,
            calls: 0,
            fail_on: None,
            events: Rc::new(RefCell::new(Vec::new())),
        }
    }
}
impl ActorTerrainHost for Terrain<'_> {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        self.events.borrow_mut().push("H12");
        if self.fail_on == Some(self.calls) {
            return Err("required H12 fixture source");
        }
        Ok(if self.calls == 1 {
            self.initial
        } else {
            self.normal.unwrap_or(self.initial)
        })
    }
}
struct Header {
    bytes: Vec<u8>,
    calls: usize,
    fail: bool,
    position: Option<Option<[u16; 3]>>,
    water: Option<u8>,
}
impl Header {
    fn new() -> Self {
        Self {
            bytes: vec![0; 5],
            calls: 0,
            fail: false,
            position: None,
            water: None,
        }
    }
}
impl ActorContactHeaderHost for Header {
    type Error = &'static str;
    fn header(&mut self, arena: &mut ObjectArena, actor: ObjectId) -> Result<&[u8], Self::Error> {
        self.calls += 1;
        if let Some(position) = self.position {
            arena.owned_mut(actor).unwrap().state.base.position_words = position;
        }
        if let Some(water) = self.water {
            arena.derived_actor_mut(actor).unwrap().byte_160 = water;
        }
        if self.fail {
            return Err("required H14 fixture source");
        }
        Ok(&self.bytes)
    }
}
const UP: [f32; 3] = [0.0, 1.0, 0.0];
struct Inputs<'a> {
    table: u32,
    step: u32,
    clock: &'a dyn AnimationClockSource,
    plane_policy: &'a TerrainRuntime,
    up: &'a [f32; 3],
    stack: ActorContactStackPolicy,
}
impl<'a> Inputs<'a> {
    fn new(plane_policy: &'a TerrainRuntime, clock: &'a dyn AnimationClockSource) -> Self {
        Self {
            table: 0x0052_fe58,
            step: 0,
            clock,
            plane_policy,
            up: &UP,
            stack: ActorContactStackPolicy::RequiredNativeAddress,
        }
    }
}
fn run(
    arena: &mut ObjectArena,
    actor: ObjectId,
    inputs: Inputs<'_>,
    terrain: &mut Terrain<'_>,
    header: &mut Header,
    output: &mut ActorContactOutput,
) -> Result<(), ActorContactError<&'static str, &'static str>> {
    let environment = ActorContactEnvironment {
        animation_clock: inputs.clock,
        plane_policy: inputs.plane_policy,
        up: inputs.up,
        sine_quarter: &assets().sine_table().values,
    };
    arena.apply_actor_contact(
        actor,
        ActorContactRequest {
            table: inputs.table,
            step: inputs.step,
            executable: executable(),
            environment: &environment,
            stack_policy: inputs.stack,
        },
        terrain,
        header,
        output,
    )
}
fn prior_output() -> ActorContactOutput {
    ActorContactOutput {
        normal: [Some(7.0), Some(8.0), Some(9.0)],
        impulse: Some(0xabcd),
        padding_02: Some(0x55aa),
        alignment: Some(0x12345678),
        motor: Some(0x87654321),
    }
}
fn normal_bits(output: &ActorContactOutput) -> [u32; 3] {
    output.normal.map(|v| v.unwrap().to_bits())
}

#[test]
fn clock_and_required_h12_precede_unknown_xyz_and_leave_all_outputs_unwritten() {
    struct Clock<'a> {
        actual: &'a FrontendProcessState,
        events: Rc<RefCell<Vec<&'static str>>>,
    }
    impl AnimationClockSource for Clock<'_> {
        fn animation_value(&self) -> u32 {
            self.events.borrow_mut().push("clock");
            self.actual.animation_value()
        }
    }
    let flat = terrain(0, [0; 4]);
    let mut source = Terrain::new(&flat);
    source.fail_on = Some(1);
    let process = FrontendProcessState::default();
    process.advance_animation(20_000);
    let clock = Clock {
        actual: &process,
        events: Rc::clone(&source.events),
    };
    let (mut arena, id) = actor(None);
    let mut header = Header::new();
    let mut output = prior_output();
    let before = output;
    assert!(matches!(
        run(
            &mut arena,
            id,
            Inputs::new(&flat, &clock),
            &mut source,
            &mut header,
            &mut output
        ),
        Err(ActorContactError::Terrain("required H12 fixture source"))
    ));
    assert_eq!(*source.events.borrow(), ["clock", "H12"]);
    assert_eq!(process.animation().value(), 1);
    assert_eq!(output, before);
    assert_eq!(arena.derived_actor(id).unwrap().word_138, None);
    assert_eq!(header.calls, 0);
}

#[test]
fn acquired_h12_precedes_unknown_xyz_and_second_h12_failure_precedes_normal_stores() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    for position in [None, Some([64, 0, 64])] {
        let (mut arena, id) = actor(position);
        let mut source = Terrain::new(&flat);
        source.fail_on = Some(2);
        let mut header = Header::new();
        let mut output = prior_output();
        let before = output;
        let error = run(
            &mut arena,
            id,
            Inputs::new(&flat, &clock),
            &mut source,
            &mut header,
            &mut output,
        )
        .unwrap_err();
        if position.is_none() {
            assert!(matches!(error,ActorContactError::MissingPosition(actual) if actual==id));
            assert_eq!(source.calls, 1);
        } else {
            assert!(matches!(
                error,
                ActorContactError::Terrain("required H12 fixture source")
            ));
            assert_eq!(source.calls, 2);
        }
        assert_eq!(output, before);
        assert_eq!(arena.derived_actor(id).unwrap().word_138, None);
        assert_eq!(header.calls, 0);
    }
}

#[test]
fn terrain_normal_crosses_the_actual_four_signed_samples_without_flipping_its_sign() {
    let gradient = terrain(0, [0, 8, 16, 8]);
    assert_eq!(
        [
            gradient.height_at([64, 0]),
            gradient.height_at([64, 128]),
            gradient.height_at([0, 64]),
            gradient.height_at([128, 64])
        ],
        [64, 192, 64, 192]
    );
    let (mut arena, id) = actor(Some([64, 0, 64]));
    let mut source = Terrain::new(&gradient);
    let mut normal = [None; 3];
    arena
        .actor_terrain_normal::<_, &'static str>(id, &mut source, &mut normal)
        .unwrap();
    assert_eq!(
        normal.map(|v| v.unwrap().to_bits()),
        [(-0.5f32).to_bits(), 0.5f32.to_bits(), (-0.5f32).to_bits()]
    );
    assert_eq!(source.calls, 1);
    assert_eq!(arena.derived_actor(id).unwrap().word_138, None);
}

#[test]
fn initial_ground_and_fresh_normal_keep_their_distinct_captured_h12_loans() {
    let flat = terrain(0, [0; 4]);
    let gradient = terrain(0, [0, 8, 16, 8]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 64, 64]));
    let mut source = Terrain::new(&flat);
    source.normal = Some(&gradient);
    let mut header = Header::new();
    header.fail = true;
    let mut output = ActorContactOutput::default();
    run(
        &mut arena,
        id,
        Inputs::new(&flat, &clock),
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(source.calls, 2);
    assert_eq!(header.calls, 0);
    assert_eq!(
        normal_bits(&output),
        [(-0.5f32).to_bits(), 0.5f32.to_bits(), (-0.5f32).to_bits()]
    );
    assert_eq!(arena.derived_actor(id).unwrap().word_138, Some(50));
}

#[test]
fn nonwater_zero_step_still_publishes_contact_and_preserves_padding_xyz_and_velocity() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, (-16_i16) as u16, 64]));
    let before = arena.object(id).unwrap().base.position_words;
    let mut source = Terrain::new(&flat);
    let mut header = Header::new();
    header.fail = true;
    let mut output = prior_output();
    run(
        &mut arena,
        id,
        Inputs::new(&flat, &clock),
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(normal_bits(&output), [0, 1.0f32.to_bits(), 0]);
    assert_eq!(source.calls, 2);
    assert_eq!(header.calls, 0);
    assert_eq!(
        arena.derived_actor(id).unwrap().word_138,
        Some((-25_i32) as u32)
    );
    assert_eq!(
        (
            output.impulse,
            output.alignment,
            output.motor,
            output.padding_02
        ),
        (Some(1360), Some(0xffff), Some(0xffff), Some(0x55aa))
    );
    assert_eq!(arena.object(id).unwrap().base.position_words, before);
    assert_eq!(arena.actor(id).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn size_zero_traps_after_normal_before_scalar_or_contact_packet_fields() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 64, 64]));
    arena.owned_mut(id).unwrap().state.base.word_28 = 0;
    let mut source = Terrain::new(&flat);
    let mut header = Header::new();
    let mut output = prior_output();
    assert!(matches!(
        run(
            &mut arena,
            id,
            Inputs::new(&flat, &clock),
            &mut source,
            &mut header,
            &mut output
        ),
        Err(ActorContactError::Motion(ActorMotionError::Division {
            address: 0x00426434,
            denominator: 0,
            ..
        }))
    ));
    assert_eq!(normal_bits(&output), [0, 1.0f32.to_bits(), 0]);
    assert_eq!(arena.derived_actor(id).unwrap().word_138, None);
    assert_eq!(
        (
            output.impulse,
            output.alignment,
            output.motor,
            output.padding_02
        ),
        (
            Some(0xabcd),
            Some(0x12345678),
            Some(0x87654321),
            Some(0x55aa)
        )
    );
}

#[test]
fn size_word_is_unsigned_before_native_lower_contact_alignment_override() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 256, 64]));
    arena.owned_mut(id).unwrap().state.base.word_28 = 0x8000;
    let mut source = Terrain::new(&flat);
    let mut header = Header::new();
    let mut output = ActorContactOutput::default();
    run(
        &mut arena,
        id,
        Inputs::new(&flat, &clock),
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(arena.derived_actor(id).unwrap().word_138, Some(0));
    // Unsigned32768 produces ratio32 and scalar0. Native table+4 is2048;
    // the later lower-contact arm at426506 overwrites initial alignment.
    assert_eq!(output.alignment, Some(0xffff));
    assert_eq!(output.impulse, Some(1360));
}

#[test]
fn required_table_read_is_after_scalar_and_alignment_but_before_impulse_and_motor() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 0, 64]));
    let mut source = Terrain::new(&flat);
    let mut header = Header::new();
    let mut output = prior_output();
    let mut inputs = Inputs::new(&flat, &clock);
    inputs.table = 0;
    assert!(matches!(
        run(
            &mut arena,
            id,
            inputs,
            &mut source,
            &mut header,
            &mut output
        ),
        Err(ActorContactError::Table(
            FrontendExecutableError::DataAddress(6)
        ))
    ));
    assert_eq!(arena.derived_actor(id).unwrap().word_138, Some(0));
    assert_eq!(output.alignment, Some(0xffff));
    assert_eq!(
        (output.impulse, output.motor, output.padding_02),
        (Some(0xabcd), Some(0x87654321), Some(0x55aa))
    );
}

#[test]
fn nonzero_step_corrects_y_then_truncates_negative_velocity_before_contact_impulse() {
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, (-16_i16) as u16, 64]));
    arena.actor_mut(id).unwrap().words_8e_92[1] = (-5_i16) as u16;
    let mut source = Terrain::new(&flat);
    let mut header = Header::new();
    let mut output = prior_output();
    let mut inputs = Inputs::new(&flat, &clock);
    inputs.step = 1;
    run(
        &mut arena,
        id,
        inputs,
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(
        arena.object(id).unwrap().base.position_words,
        Some([64, 16, 64])
    );
    assert_eq!(arena.actor(id).unwrap().words_8e_92, [0, 1, 0]);
    assert_eq!(
        arena.derived_actor(id).unwrap().word_138,
        Some((-25_i32) as u32)
    );
    assert_eq!(output.impulse, Some(1354));
    assert_eq!(output.padding_02, Some(0x55aa));
}

#[test]
fn water_header_failures_preserve_the_prefix_and_fresh_xyz_precedes_byte4() {
    let water = terrain(100, [0; 4]);
    let clock = AnimationClock::default();
    for lose_xyz in [false, true] {
        let (mut arena, id) = actor(Some([64, 116, 64]));
        arena.derived_actor_mut(id).unwrap().byte_160 = 1;
        let mut source = Terrain::new(&water);
        let mut header = Header::new();
        header.bytes.clear();
        if lose_xyz {
            header.position = Some(None);
        }
        let mut output = prior_output();
        let before = output;
        let error = run(
            &mut arena,
            id,
            Inputs::new(&water, &clock),
            &mut source,
            &mut header,
            &mut output,
        )
        .unwrap_err();
        if lose_xyz {
            assert!(matches!(error,ActorContactError::MissingPosition(actual)if actual==id));
            assert_eq!(arena.object(id).unwrap().base.position_words, None);
        } else {
            assert!(matches!(error, ActorContactError::RequiredHeaderByte4));
        }
        assert_eq!(source.calls, 1);
        assert_eq!(header.calls, 1);
        assert_eq!(output, before);
        assert_eq!(arena.derived_actor(id).unwrap().word_138, None);
    }
}

#[test]
fn water_stack_requirement_is_lazy_after_real_packet_stores_and_zero_step_bypasses_it() {
    let water = terrain(100, [0; 4]);
    let clock = AnimationClock::default();
    for step in [0, 1] {
        let (mut arena, id) = actor(Some([64, 116, 64]));
        arena.derived_actor_mut(id).unwrap().byte_160 = 1;
        let mut source = Terrain::new(&water);
        let mut header = Header::new();
        let mut output = prior_output();
        let mut inputs = Inputs::new(&water, &clock);
        inputs.table = 0x0052_fe68;
        inputs.step = step;
        let result = run(
            &mut arena,
            id,
            inputs,
            &mut source,
            &mut header,
            &mut output,
        );
        if step == 0 {
            result.unwrap();
            assert_eq!(output.motor, Some(0xffff));
        } else {
            assert!(matches!(
                result,
                Err(ActorContactError::RequiredNativeStackComparison)
            ));
            assert_eq!(output.motor, Some(0x87654321));
        }
        assert_eq!(source.calls, 1);
        assert_eq!(header.calls, 1);
        assert_eq!(normal_bits(&output), [0, 1.0f32.to_bits(), 0]);
        assert_eq!(arena.derived_actor(id).unwrap().word_138, Some(25));
        assert_eq!(output.impulse, Some(1360));
        assert_eq!(output.alignment, Some(0xffff));
        assert_eq!(output.padding_02, Some(0x55aa));
        assert_eq!(
            arena.object(id).unwrap().base.position_words,
            Some([64, 116, 64])
        );
    }
}

#[test]
fn supplied_class36_stack_domain_keeps_the_no_correction_branch_without_a_fake_address() {
    let water = terrain(100, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 116, 64]));
    arena.derived_actor_mut(id).unwrap().byte_160 = 1;
    let mut source = Terrain::new(&water);
    let mut header = Header::new();
    let mut output = ActorContactOutput::default();
    let mut inputs = Inputs::new(&water, &clock);
    inputs.table = 0x0052_fe68;
    inputs.step = 1;
    inputs.stack = ActorContactStackPolicy::Bf09Class36Windows8PlusStack;
    run(
        &mut arena,
        id,
        inputs,
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(
        arena.object(id).unwrap().base.position_words,
        Some([64, 116, 64])
    );
    assert_eq!(arena.actor(id).unwrap().words_8e_92, [0; 3]);
    assert_eq!(output.motor, Some(0xffff));
}

#[test]
fn water_uses_the_actual_supplied_up_and_fresh_header_callback_xyz_and_water_byte() {
    let water = terrain(100, [0; 4]);
    let clock = AnimationClock::default();
    let (mut arena, id) = actor(Some([64, 100, 64]));
    arena.derived_actor_mut(id).unwrap().byte_160 = 1;
    let mut source = Terrain::new(&water);
    let mut header = Header::new();
    header.position = Some(Some([2048, 116, 4096]));
    header.water = Some(0);
    let up = [0.25, 0.5, -0.25];
    let mut output = ActorContactOutput::default();
    let mut inputs = Inputs::new(&water, &clock);
    inputs.table = 0x0052_fe68;
    inputs.step = 1;
    inputs.up = &up;
    run(
        &mut arena,
        id,
        inputs,
        &mut source,
        &mut header,
        &mut output,
    )
    .unwrap();
    assert_eq!(normal_bits(&output), up.map(f32::to_bits));
    assert_eq!(arena.derived_actor(id).unwrap().word_138, Some(12));
    //Fresh160=0 avoids the native pointer comparison and uses scalar512<682.
    //It then corrects Y from captured surface100 before storing fresh threshold.
    assert_eq!(
        arena.object(id).unwrap().base.position_words,
        Some([2048, 110, 4096])
    );
    assert_eq!(arena.derived_actor(id).unwrap().byte_160, 0);
    assert_eq!(source.calls, 1);
    assert_eq!(header.calls, 1);
}

#[test]
fn negative_water_recompute_keeps_original_scalar_and_suppresses_positive_dot_reduction() {
    let water = terrain(100, [0; 4]);
    let flat = terrain(0, [0; 4]);
    let clock = AnimationClock::default();
    for use_water in [false, true] {
        let (mut arena, id) = actor(Some([64, (-100_i16) as u16, 64]));
        arena.derived_actor_mut(id).unwrap().byte_160 = u8::from(use_water);
        arena.actor_mut(id).unwrap().words_8e_92[1] = 100;
        let current = if use_water { &water } else { &flat };
        let mut source = Terrain::new(current);
        let mut header = Header::new();
        let mut output = ActorContactOutput::default();
        let mut inputs = Inputs::new(current, &clock);
        inputs.table = 0x0052_fe68;
        run(
            &mut arena,
            id,
            inputs,
            &mut source,
            &mut header,
            &mut output,
        )
        .unwrap();
        assert_eq!(source.calls, 2);
        assert_eq!(header.calls, if use_water { 1 } else { 0 });
        assert_eq!(output.impulse, Some(if use_water { 1360 } else { 977 }));
        assert_eq!(
            arena.derived_actor(id).unwrap().word_138,
            Some((if use_water { -313_i32 } else { -157_i32 }) as u32)
        );
        assert_eq!(
            arena.object(id).unwrap().base.position_words,
            Some([64, (-100_i16) as u16, 64])
        );
        assert_eq!(arena.actor(id).unwrap().words_8e_92, [0, 100, 0]);
    }
}
