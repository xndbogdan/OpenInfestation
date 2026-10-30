use super::super::selector10_tail::tests::{assets, executable, fixture, resident, sound};
use super::*;
use crate::frontend_camera::AnimationClock;
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};
struct Terrain {
    current: TerrainRuntime,
    calls: usize,
    fail_at: Option<usize>,
}
impl Terrain {
    fn new(plane: i32) -> Self {
        Self {
            current: native_probe_fixture_terrain(&serde_json::json!({
                "spacing":256,"height_shift":5,"plane_base":plane,"plane_shift":8,
                "xz":[384,640],"heights":[2,2,2,2],"alternate":false,
            })),
            calls: 0,
            fail_at: None,
        }
    }
}
impl ActorTerrainHost for Terrain {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        if self.fail_at == Some(self.calls) {
            Err("actual H12 required")
        } else {
            Ok(&self.current)
        }
    }
}
struct Header {
    calls: usize,
    bytes: [u8; 5],
}
impl ActorContactHeaderHost for Header {
    type Error = &'static str;
    fn header(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<&[u8], Self::Error> {
        self.calls += 1;
        Ok(&self.bytes)
    }
}
fn install(arena: &mut ObjectArena, actor: ObjectId, mode: u32) {
    arena.selector10_actor_mut(actor).unwrap().word_268 = Some(mode);
    arena.selector10_actor_mut(actor).unwrap().helper_26c =
        super::super::selector10_helper::Selector10Helper::for_selector(mode);
    arena.owned_mut(actor).unwrap().state.base.word_28 = 1024;
}
fn visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    status: i32,
    delta: u32,
    controls: [i32; 2],
    terrain: &mut Terrain,
    header: &mut Header,
    rng: &mut u32,
    sounds: &mut ProcessSoundRuntime,
) -> Result<Selector10MotionNextPhase, Selector10MotionError<&'static str, &'static str>> {
    arena.actor_mut(actor).unwrap().word_c4 = status as u32;
    let clock = AnimationClock::default();
    let plane = Terrain::new(0).current;
    arena.tick_selector10_motion(
        actor,
        Selector10MotionRequest {
            microseconds: delta,
            controls,
            executable: executable(),
            environment: ActorContactEnvironment {
                animation_clock: &clock,
                plane_policy: &plane,
                up: &[0.0, 1.0, 0.0],
                sine_quarter: &assets().sine_table().values,
            },
            random_state: rng,
            residency: &resident(),
            sounds,
        },
        terrain,
        header,
    )
}
fn header() -> Header {
    Header {
        calls: 0,
        bytes: [0; 5],
    }
}
#[test]
fn mode_one_nonpositive_contact_still_runs_real_physics_and_installed_four_child_callback() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    arena.family4236d0_actor_mut(actor).unwrap().table_254 = 0xdead0000;
    arena.derived_actor_mut(actor).unwrap().word_17a = 0x432;
    let mut terrain = Terrain::new(0);
    let mut header = header();
    let mut rng = 17;
    let mut sounds = sound();
    assert_eq!(
        visit(
            &mut arena,
            actor,
            1,
            65536,
            [31, 1000],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds
        )
        .unwrap(),
        Selector10MotionNextPhase::PersistentAudio
    );
    assert_eq!(terrain.calls, 2);
    assert_eq!(header.calls, 0);
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
    assert_eq!(
        arena.actor(actor).unwrap().words_8e_92,
        [0, (-63_i16) as u16, 0]
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([384, 60, 640])
    );
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        1
    );
    assert_eq!(arena.derived_actor(actor).unwrap().word_17a, 0x432);
    assert_eq!(rng, 17);
    assert!(sounds.is_empty());
}
#[test]
fn unknown_mode_stops_after_contact_and_special_modes_run_on() {
    for (mode, status) in [(None, 1), (Some(3), 1), (Some(4), 3)] {
        let (mut arena, actor) = fixture();
        arena.owned_mut(actor).unwrap().state.base.word_28 = 1024;
        arena.selector10_actor_mut(actor).unwrap().word_268 = mode;
        let mut terrain = Terrain::new(0);
        let mut header = header();
        let mut rng = 0;
        let mut sounds = sound();
        let result = visit(
            &mut arena,
            actor,
            status,
            32,
            [0, 0],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds,
        );
        match mode {
            None => assert!(matches!(
                result,
                Err(Selector10MotionError::RequiredMode {
                    address: 0x42544c,
                    ..
                })
            )),
            Some(_) => assert!(result.is_ok(), "mode {mode:?}: {result:?}"),
        }
        assert_eq!(arena.derived_actor(actor).unwrap().word_138, Some(0));
        assert_eq!(rng, 0);
        assert!(sounds.is_empty());
    }
}

/// 0042544C..00425562: modes other than 3 and 4 turn toward the normal;
/// modes 3 and 4 alive toward the world up, dying toward the normal crossed
/// with basis row 2, each component one product difference rounded to f32.
#[test]
fn special_modes_align_to_the_world_up_or_the_normal_across_row_two() {
    let mut basis = NativeBasis::INITIAL;
    let row2 = [0.25_f32, -0.5, 0.8];
    for (i, value) in row2.iter().enumerate() {
        basis.words[7 + i] = value.to_bits();
    }
    let normal = [0.1_f32, 0.9, -0.3];
    for mode in [0, 1, 2, 5] {
        assert_eq!(alignment_axis(mode, 3, normal, basis), normal);
        assert_eq!(alignment_axis(mode, 1, normal, basis), normal);
    }
    for mode in [3, 4] {
        assert_eq!(alignment_axis(mode, 3, normal, basis), [0.0, 1.0, 0.0]);
        assert_eq!(alignment_axis(mode, 4, normal, basis), [0.0, 1.0, 0.0]);
        let [x, y, z] = normal.map(f64::from);
        let [a, b, c] = row2.map(f64::from);
        let expected = [
            (y * c - z * b) as f32,
            (z * a - c * x) as f32,
            (b * x - y * a) as f32,
        ];
        assert_eq!(alignment_axis(mode, 2, normal, basis), expected);
        assert_eq!(alignment_axis(mode, -1, normal, basis), expected);
    }
}
#[test]
fn forward_prefix_survives_angular_table_error_before_physics_and_helper() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    arena.family4236d0_actor_mut(actor).unwrap().table_258 = 0xdead0000;
    let mut terrain = Terrain::new(0);
    let mut header = header();
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(
            &mut arena,
            actor,
            2,
            1024,
            [400, 1000],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds
        ),
        Err(Selector10MotionError::Table(
            FrontendExecutableError::DataAddress(0xdead0000)
        ))
    ));
    assert!(arena.actor(actor).unwrap().words_8e_92[2] > 0);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}
#[test]
fn count_two_raw_boundary_is_reached_after_physics_without_a_mode_shortcut() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 3);
    //Positive separation makes real contact alignment0, so special-vector
    //state is genuinely bypassed; count2 still reaches its independent holes.
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 32000, 640]);
    let before = arena.object(actor).unwrap().base.position_words;
    let mut terrain = Terrain::new(0);
    let mut header = header();
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(
            &mut arena,
            actor,
            1,
            65536,
            [0, 0],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds
        ),
        Err(Selector10MotionError::Helper(
            Selector10HelperTickError::RequiredChildField {
                index: 1,
                offset: 0x28,
                ..
            }
        ))
    ));
    assert_ne!(arena.object(actor).unwrap().base.position_words, before);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
    assert_eq!(rng, 0);
}
#[test]
fn class10_water_nonzero_step_retains_the_native_stack_boundary_and_zero_step_is_a_real_bypass() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    arena.derived_actor_mut(actor).unwrap().byte_160 = 1;
    //Actual52FE58 thresholds are1024 and2048. Size1024 and supplied
    //Y556 against plane256 give ratio1200: water leaves comparison unwritten.
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 556, 640]);
    let mut terrain = Terrain::new(65536);
    let mut header = header();
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(
            &mut arena,
            actor,
            2,
            32,
            [0, 0],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds
        ),
        Err(Selector10MotionError::Contact(
            ActorContactError::RequiredNativeStackComparison
        ))
    ));
    assert_eq!(header.calls, 1);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
    visit(
        &mut arena,
        actor,
        1,
        32,
        [0, 0],
        &mut terrain,
        &mut header,
        &mut rng,
        &mut sounds,
    )
    .unwrap();
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        1
    );
}
#[test]
fn h12_error_keeps_unknown_scalar_and_does_not_enter_helper_or_audio() {
    let (mut arena, actor) = fixture();
    install(&mut arena, actor, 1);
    let mut terrain = Terrain::new(0);
    terrain.fail_at = Some(1);
    let mut header = header();
    let mut rng = 0;
    let mut sounds = sound();
    assert!(matches!(
        visit(
            &mut arena,
            actor,
            2,
            125_000,
            [0, 0],
            &mut terrain,
            &mut header,
            &mut rng,
            &mut sounds
        ),
        Err(Selector10MotionError::Contact(ActorContactError::Terrain(
            "actual H12 required"
        )))
    ));
    assert_eq!(arena.derived_actor(actor).unwrap().word_138, None);
    assert_eq!(
        arena
            .selector10_actor(actor)
            .unwrap()
            .helper_26c
            .as_ref()
            .unwrap()
            .word_08,
        0
    );
    assert_eq!(header.calls, 0);
    assert_eq!(rng, 0);
    assert!(sounds.is_empty());
}
#[test]
fn damping_keeps_signed_words_logical_dword_shift_and_axis_thresholds() {
    assert_eq!(damp((-100_i16) as u16, 32768, 64), 100);
    assert_eq!(damp(64, 32, 64), 0);
    assert_eq!(damp((-64_i16) as u16, 32, 64), 0);
    assert_eq!(damp(25, 32, 24), 25);
    assert_eq!(damp(24, 32, 24), 0);
}
