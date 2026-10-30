use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, Family429e20Class, Selector12GrantContext,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn fixture(position: Option<[u16; 3]>) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let grants = inf_formats::GrantTable {
        virtual_address: inf_formats::GRANT_TABLE_ADDRESS,
        records: Vec::new(),
    };
    let actor = arena
        .allocate_selector12_actor(
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
                        sine_quarter: &UNUSED_SINE,
                    },
                },
                world_mask_limits: [3, 3],
            },
            &mut 0,
            Selector12GrantContext {
                table: &grants,
                first_player_grants: None,
            },
        )
        .unwrap();
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = 4096;
    base.dirty_word = 0;
    (arena, actor)
}

struct Terrain {
    height: Result<i16, &'static str>,
    samples: Vec<[u16; 2]>,
}

impl Selector12TerrainHost for Terrain {
    type Error = &'static str;
    fn terrain_height(&mut self, xz: [u16; 2]) -> Result<i16, Self::Error> {
        self.samples.push(xz);
        self.height
    }
}

fn terrain(height: i16) -> Terrain {
    Terrain {
        height: Ok(height),
        samples: Vec::new(),
    }
}

fn tick(
    arena: &mut ObjectArena,
    actor: ObjectId,
    terrain: &mut Terrain,
    microseconds: u32,
    snap: u8,
    controls: [i32; 2],
) -> Result<(), Selector12MotionError<&'static str>> {
    let mut sine = [0; 4096];
    sine[4095] = 32767;
    let math = NativeBasisMath::new(sine);
    arena.tick_selector12_motion(
        actor,
        Selector12MotionRequest {
            microseconds,
            turn_control: controls[0],
            forward_control: controls[1],
            surface_snap_53a128: snap,
            math: &math,
            inverse_sine: &[0; 1024],
            sine_quarter: &sine,
            terrain,
        },
    )
}

#[test]
fn terrain_failure_and_unknown_position_do_not_publish_motion() {
    let (mut arena, actor) = fixture(Some([65535, 120, 7]));
    let before = arena.object(actor).unwrap().clone();
    let mut host = Terrain {
        height: Err("H12 unavailable"),
        samples: Vec::new(),
    };
    assert_eq!(
        tick(&mut arena, actor, &mut host, 40000, 1, [0; 2]),
        Err(Selector12MotionError::Terrain("H12 unavailable"))
    );
    assert_eq!(arena.object(actor).unwrap(), &before);
    assert_eq!(host.samples, [[65535, 7]]);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    host.samples.clear();
    assert_eq!(
        tick(&mut arena, actor, &mut host, 40000, 1, [0; 2]),
        Err(Selector12MotionError::Motion(
            ActorMotionError::MissingPosition(actor)
        ))
    );
    assert!(host.samples.is_empty());
}

#[test]
fn live_snap_publishes_y_before_sub32_delta_division_trap() {
    let (mut arena, actor) = fixture(Some([10, 120, 30]));
    arena.actor_mut(actor).unwrap().words_8e_92 = [7, 9, 11];
    let mut host = terrain(100);
    assert!(matches!(
        tick(&mut arena, actor, &mut host, 31, 1, [0; 2]),
        Err(Selector12MotionError::Motion(ActorMotionError::Division {
            address: 0x0046_7e4d,
            denominator: 0,
            ..
        }))
    ));
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 100, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [7, 9, 11]);
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 0);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn airborne_live_actor_bypasses_alignment_and_table_reads_but_executes_physics() {
    let (mut arena, actor) = fixture(Some([10, 120, 30]));
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, 1000, 0];
    let Family429e20Class::Selector12(state) =
        &mut arena.family429e20_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state.table_290 = 0;
    let mut host = terrain(100);
    tick(&mut arena, actor, &mut host, 32000, 0, [10_000, 10_000]).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 992, 0]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 150, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 0);
    assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
    assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
}

#[test]
fn level_live_actor_aligns_then_runs_forward_angular_lateral_and_physics() {
    let (mut arena, actor) = fixture(Some([10, 100, 30]));
    let mut host = terrain(100);
    tick(&mut arena, actor, &mut host, 40000, 0, [0, 3000]).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 619]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 100, 53])
    );
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 2);
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0xc0);
}

#[test]
fn snap_velocity_is_integrated_after_alignment_without_an_extra_ground_clamp() {
    let (mut arena, actor) = fixture(Some([10, 120, 30]));
    let mut host = terrain(100);
    tick(&mut arena, actor, &mut host, 40000, 255, [0; 2]).unwrap();
    assert_eq!(arena.actor(actor).unwrap().words_8e_92[1] as i16, -130);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 95, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 2);
}

#[test]
fn later_table_failure_retains_alignment_and_completed_forward_velocity() {
    let (mut arena, actor) = fixture(Some([10, 100, 30]));
    let Family429e20Class::Selector12(state) =
        &mut arena.family429e20_actor_mut(actor).unwrap().class
    else {
        unreachable!()
    };
    state.table_294 = 0;
    let mut host = terrain(100);
    assert_eq!(
        tick(&mut arena, actor, &mut host, 40000, 0, [0, 3000]),
        Err(Selector12MotionError::Motion(
            ActorMotionError::UnsupportedSelector12Tables(actor)
        ))
    );
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 2);
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0, 0, 623]);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 100, 30])
    );
    assert_eq!(arena.object(actor).unwrap().base.dirty_word, 0);
}

#[test]
fn dead_branch_uses_signed_lifecycle_ten_unit_clearance_and_bounce() {
    let (mut arena, actor) = fixture(Some([10, 111, 30]));
    arena.actor_mut(actor).unwrap().word_c4 = (-1_i32) as u32;
    let mut host = terrain(100);
    tick(&mut arena, actor, &mut host, 1024, 1, [10_000; 2]).unwrap();
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 111, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([10, 109, 30]);
    arena.actor_mut(actor).unwrap().words_8e_92 = [0, (-3_i16) as u16, 0];
    tick(&mut arena, actor, &mut host, 1024, 1, [10_000; 2]).unwrap();
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([10, 110, 30])
    );
    assert_eq!(arena.actor(actor).unwrap().words_8e_92, [0; 3]);
    assert_eq!(arena.actor(actor).unwrap().rotating.basis.words[0], 0);
}

#[test]
fn ground_decay_retains_logical_shift_and_axis_thresholds() {
    assert_eq!(ground_velocity((-64_i16) as u16, 1250, 64), 0);
    assert_eq!(ground_velocity((-65_i16) as u16, 1250, 64) as i16, -60);
    assert_eq!(ground_velocity(65, 1250, 64), 61);
    assert_eq!(ground_velocity(24, 1250, 24), 0);
    assert_eq!(ground_velocity(25, 1250, 24), 24);
    assert_eq!(ground_velocity((-25_i16) as u16, 1250, 24) as i16, -23);
}
