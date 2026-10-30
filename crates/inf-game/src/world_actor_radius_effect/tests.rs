use std::{collections::VecDeque, path::Path, sync::OnceLock};

use inf_formats::{
    decode_instance_command, native_basis::NativeBasis, ByteSpan, InstanceCommandReadScope,
    InstanceCommandRequest, InstanceCommandTarget,
};

use super::*;
use crate::{
    object_runtime::{
        ActorCommandContext, ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
        BaseContext,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

fn new_arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn actor(arena: &mut ObjectArena, position: [u16; 3]) -> ObjectId {
    arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some(position),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &UNUSED_SINE,
            },
        })
        .unwrap()
}

fn runtime() -> &'static ResidentRuntime {
    static RUNTIME: OnceLock<ResidentRuntime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        runtime.apply(ResidencyRequest::Boot).unwrap();
        runtime
    })
}

// Shared command 5 is the existing owner of actor +8C. Do not broaden arena
// mutation visibility just to manufacture these post-callback test states.
fn set_mass(arena: &mut ObjectArena, actor: ObjectId, mass: u16) {
    let mut source = 5_i32.to_le_bytes().to_vec();
    source.extend_from_slice(&mass.to_le_bytes());
    let command = decode_instance_command(InstanceCommandRequest {
        read_scope: InstanceCommandReadScope::Inspection,
        source: &source,
        main_payload: ByteSpan::new(0, source.len()),
        relocations: &[],
        command_offset: 0,
        target: InstanceCommandTarget::SharedActor,
    })
    .unwrap();
    assert!(arena
        .apply_actor_command(
            actor,
            &command,
            ActorCommandContext {
                runtime: runtime(),
                first_player_grants: None,
                terrain: None,
            },
        )
        .unwrap());
}

fn block() -> RadiusEffectBlock {
    RadiusEffectBlock {
        strength: 2000,
        channel: 9,
        byte_03: None,
        inner_radius: 50,
        outer_radius: 512,
        impulse: 256,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Block,
    Position,
    Damage(ObjectId),
    NetworkActive,
    NetworkRole,
}

#[derive(Clone, Copy)]
enum Mutation {
    Mass(u16),
    Flags(u32),
    Move([u16; 3]),
    Append([u16; 3]),
    Destroy(ObjectId),
    DestroyCurrent,
    Source(RadiusEffectBlock, [u16; 3]),
}

struct Host {
    block: RadiusEffectBlock,
    position: [u16; 3],
    events: Vec<Event>,
    damages: Vec<ActorRadiusDamageRequest>,
    mutations: Vec<(usize, Mutation)>,
    active: VecDeque<bool>,
    roles: VecDeque<i32>,
    fail_damage_at: Option<usize>,
    rng_on_damage: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            block: block(),
            position: [0; 3],
            events: Vec::new(),
            damages: Vec::new(),
            mutations: Vec::new(),
            active: VecDeque::new(),
            roles: VecDeque::new(),
            fail_damage_at: None,
            rng_on_damage: false,
        }
    }
}

impl ActorRadiusEffectHost for Host {
    type Error = &'static str;
    type Source = ();

    fn source_block(&mut self, _: &ObjectArena, _: ()) -> Result<RadiusEffectBlock, Self::Error> {
        self.events.push(Event::Block);
        Ok(self.block)
    }

    fn source_position_words(&mut self, _: &ObjectArena, _: ()) -> Result<[u16; 3], Self::Error> {
        self.events.push(Event::Position);
        Ok(self.position)
    }

    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorRadiusDamageRequest,
        rng: &mut u32,
    ) -> Result<(), Self::Error> {
        self.events.push(Event::Damage(request.actor));
        self.damages.push(request);
        if self.rng_on_damage {
            crate::name_entry::name_entry_rng_next(rng);
        }
        for (_, mutation) in self
            .mutations
            .iter()
            .copied()
            .filter(|(call, _)| *call == self.damages.len())
            .collect::<Vec<_>>()
        {
            match mutation {
                Mutation::Mass(mass) => set_mass(arena, request.actor, mass),
                Mutation::Flags(flags) => arena.apply_payload_flags(request.actor, flags).unwrap(),
                Mutation::Move(position_words) => arena
                    .initialize_context(
                        request.actor,
                        BaseContext {
                            position_words: Some(position_words),
                            placement_index: 0,
                        },
                    )
                    .unwrap(),
                Mutation::Append(position) => {
                    actor(arena, position);
                }
                Mutation::Destroy(object) => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: object,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                Mutation::DestroyCurrent => {
                    arena
                        .destroy(crate::object_runtime::ObjectDestructionRequest {
                            object: request.actor,
                            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
                        })
                        .unwrap();
                }
                Mutation::Source(block, position) => {
                    self.block = block;
                    self.position = position;
                }
            }
        }
        if self.fail_damage_at == Some(self.damages.len()) {
            Err("required damage service failed after its reached mutation")
        } else {
            Ok(())
        }
    }

    fn network_active(&mut self) -> Result<bool, Self::Error> {
        self.events.push(Event::NetworkActive);
        Ok(self.active.pop_front().expect("unexpected network read"))
    }

    fn network_role(&mut self) -> Result<i32, Self::Error> {
        self.events.push(Event::NetworkRole);
        Ok(self.roles.pop_front().expect("unexpected settings read"))
    }
}

fn apply(
    arena: &mut ObjectArena,
    host: &mut Host,
) -> Result<ActorRadiusEffectOutcome, ActorRadiusEffectError<&'static str>> {
    apply_actor_radius_effect(
        arena,
        ActorRadiusEffectRequest {
            origin: (),
            damage_source: ActorHandle::from_raw(0x1234_5678),
        },
        host,
        &mut 0,
    )
}

#[test]
fn stopped_construction_radius_candidate_keeps_native_state_and_healthy_damage_continues() {
    let mut arena = new_arena();
    let stopped = arena
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
    arena.stop_object_construction(stopped).unwrap();
    let healthy = actor(&mut arena, [30, 0, 0]);
    set_mass(&mut arena, healthy, 0x800);
    let before = arena.object(stopped).unwrap().clone();
    let nodes = arena.actor_list().to_vec();
    let mut host = Host::default();

    for _ in 0..2 {
        host.events.clear();
        host.damages.clear();
        assert_eq!(apply(&mut arena, &mut host).unwrap().alternate_hits, 0);
        assert_eq!(host.damages.len(), 1);
        assert_eq!(host.damages[0].actor, healthy);
        assert_eq!(
            host.events,
            [
                Event::Block,
                Event::Position,
                Event::Block,
                Event::Damage(healthy)
            ]
        );
        assert_eq!(arena.object(stopped).unwrap(), &before);
        assert_eq!(arena.actor_list(), nodes);
    }
}

#[test]
fn direct_length_keeps_signed_dword_and_signed_low_ax_asymmetry() {
    let square = math::squared_word_distance([i16::MIN; 3]);
    assert_eq!(square, 0xc000_0000);
    assert_eq!(math::direct_word_distance(square), 0);
    assert_eq!(
        crate::world_radius_effect::radius_word_distance([i16::MIN; 3]),
        56_754
    );
    let square = math::squared_word_distance([i16::MAX, i16::MAX, 0]);
    assert_eq!(math::direct_word_distance(square), 46_339_u16 as i16 as i32);
    assert_eq!(math::direct_word_distance(25), 5);
}

#[test]
fn impulse_quantization_and_mass_arms_use_word_then_dword_wrapping() {
    let impulse = math::impulse_vectors(NativeBasis::INITIAL, [100, 0, 0], [256, 0, 0], 1, 800);
    assert_eq!(impulse.linear, [256, 0, 0]); // Small mass ignores network scale.
    assert_eq!(impulse.angular, [0, 53_504, (-26_624_i32) as u32]);
    let normal = math::impulse_vectors(NativeBasis::INITIAL, [100, 0, 0], [256, 0, 0], 128, 256);
    assert_eq!(normal.linear, [512, 0, 0]);
    assert_eq!(normal.angular, impulse.angular);
    let high = math::impulse_vectors(NativeBasis::INITIAL, [100, 0, 0], [256, 0, 0], 2047, 800);
    assert_eq!(high.linear, [100, 0, 0]);
    assert_eq!(high.angular, [0, 3344, (-1664_i32) as u32]);
    let negative =
        math::impulse_vectors(NativeBasis::INITIAL, [-100, -101, 0], [-256, 0, 0], 1, 256);
    assert_eq!(negative.linear, [(-256_i16) as u16, 0, 0]);
    assert_eq!(
        negative.angular,
        [0, (-53_504_i32) as u32, (-26_624_i32) as u32]
    );
    let wrapped = math::impulse_vectors(
        NativeBasis::INITIAL,
        [-1, -101, -1],
        [i16::MAX, i16::MIN, i16::MAX],
        1,
        256,
    );
    assert_eq!(wrapped.linear, [0x7fff, 0x8000, 0x7fff]);
    // Cross-X truncates to 40127, then wraps to signed WORD -25409 before
    // angular scaling. Saturation or keeping the full integer changes X.
    assert_eq!(
        wrapped.angular,
        [6_504_704, (-3_424_000_i32) as u32, 6_848_256]
    );
}

#[test]
fn impulse_direction_uses_both_inverse_words_and_forward_stored_float_basis() {
    // X=(0,0,1), Y=(0,1,0), Z=(-1,0,0): an exact quarter-turn basis.
    // The original world X delta becomes local -Z before the sign gates.
    let basis = NativeBasis {
        words: [
            0,
            0,
            0,
            1_f32.to_bits(),
            0,
            1_f32.to_bits(),
            0,
            (-1_f32).to_bits(),
            0,
            0,
        ],
    };
    let impulse = math::impulse_vectors(basis, [100, 0, 0], [256, 0, 0], 1, 256);
    assert_eq!(impulse.linear, [256, 0, 0]);
    assert_eq!(impulse.angular, [0, 26_624, (-26_624_i32) as u32]);
    let mut basis = NativeBasis::INITIAL;
    basis.words[5] = 0.25_f32.to_bits();
    // Inverse Y = -100.75 rounds to -101 before the strict -100 sign gate.
    // Truncating the inverse transform incorrectly reverses angular Z.
    let impulse = math::impulse_vectors(basis, [0, -403, 0], [256, 0, 0], 1, 256);
    assert_eq!(impulse.angular, [0, 53_504, 6656]);
}

#[test]
fn flags_strict_radius_and_zero_strength_choose_the_native_destination_arm() {
    let mut arena = new_arena();
    let disabled = actor(&mut arena, [0; 3]);
    arena.apply_payload_flags(disabled, 0).unwrap();
    actor(&mut arena, [512, 0, 0]); // Equality is rejected before falloff.
    let fixed = actor(&mut arena, [0; 3]);
    arena.apply_payload_flags(fixed, 4 | 8).unwrap();
    let mobile = actor(&mut arena, [0; 3]);
    let mut host = Host {
        block: RadiusEffectBlock {
            strength: 0,
            ..block()
        },
        ..Host::default()
    };
    assert_eq!(apply(&mut arena, &mut host).unwrap().alternate_hits, 0);
    assert_eq!(host.damages.len(), 1);
    assert_eq!(host.damages[0].actor, mobile);
    assert_eq!(host.damages[0].effect.impulse_vector, Some([0; 3]));
    assert_eq!(host.damages[0].effect.byte_03, None);
    assert_eq!(host.damages[0].source, ActorHandle::from_raw(0x1234_5678));
    assert_eq!(arena.actor(fixed).unwrap().words_8e_92, [0; 3]);
    assert_eq!(
        host.events,
        [
            Event::Block,
            Event::Position,
            Event::Position,
            Event::Block,
            Event::Position,
            Event::Block,
            Event::Damage(mobile)
        ]
    );
}

#[test]
fn alternate_count_skips_local_damage_but_keeps_mobile_impulse() {
    let mut arena = new_arena();
    let mobile = actor(&mut arena, [25, 0, 0]);
    arena.apply_payload_flags(mobile, 0x8000_0004).unwrap();
    let fixed = actor(&mut arena, [25, 0, 0]);
    arena.apply_payload_flags(fixed, 0x8000_000c).unwrap();
    let mut host = Host::default();
    assert_eq!(apply(&mut arena, &mut host).unwrap().alternate_hits, 2);
    assert!(host.damages.is_empty());
    assert_eq!(arena.actor(mobile).unwrap().words_8e_92, [256, 0, 0]);
    assert_eq!(arena.actor(fixed).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn post_damage_mass_suppresses_impulse_but_flag8_and_moved_position_do_not() {
    let mut arena = new_arena();
    let heavy_after_damage = actor(&mut arena, [25, 0, 0]);
    let moved_after_damage = actor(&mut arena, [25, 0, 0]);
    let mut host = Host {
        mutations: vec![
            (1, Mutation::Mass(2048)),
            (2, Mutation::Flags(4 | 8)),
            (2, Mutation::Move([0, 25, 0])),
        ],
        ..Host::default()
    };
    apply(&mut arena, &mut host).unwrap();
    assert_eq!(arena.actor(heavy_after_damage).unwrap().words_8e_92, [0; 3]);
    let moved = arena.actor(moved_after_damage).unwrap();
    assert_eq!(moved.words_8e_92, [256, 0, 0]); // Saved pre-callback delta.
    assert_eq!(moved.words_94_9c, [0, 53_504, (-26_624_i32) as u32]);
    assert_eq!(host.damages.len(), 2);
}

#[test]
fn network_settings_are_reread_only_in_the_large_mass_impulse_arm() {
    for (active, roles, linear, events) in [
        (vec![false], vec![], 512, vec![Event::NetworkActive]),
        (
            vec![true],
            vec![2],
            1600,
            vec![Event::NetworkActive, Event::NetworkRole],
        ),
        (
            vec![true, true],
            vec![1, 3],
            128,
            vec![
                Event::NetworkActive,
                Event::NetworkRole,
                Event::NetworkActive,
                Event::NetworkRole,
            ],
        ),
        (
            vec![true, false],
            vec![1],
            512,
            vec![
                Event::NetworkActive,
                Event::NetworkRole,
                Event::NetworkActive,
            ],
        ),
    ] {
        let mut arena = new_arena();
        let target = actor(&mut arena, [25, 0, 0]);
        set_mass(&mut arena, target, 128);
        let mut host = Host {
            active: active.into(),
            roles: roles.into(),
            ..Host::default()
        };
        apply(&mut arena, &mut host).unwrap();
        assert_eq!(arena.actor(target).unwrap().words_8e_92, [linear, 0, 0]);
        assert_eq!(&host.events[4..], events);
        assert!(host.active.is_empty() && host.roles.is_empty());
    }
}

#[test]
fn source_reads_stay_live_while_initial_radius_square_stays_cached() {
    let mut arena = new_arena();
    let first = actor(&mut arena, [25, 0, 0]);
    let second = actor(&mut arena, [100, 0, 0]);
    let changed = RadiusEffectBlock {
        strength: 17,
        outer_radius: 50,
        ..block()
    };
    let mut host = Host {
        mutations: vec![(1, Mutation::Source(changed, [90, 0, 0]))],
        ..Host::default()
    };
    apply(&mut arena, &mut host).unwrap();
    assert_eq!(
        host.damages
            .iter()
            .map(|call| call.actor)
            .collect::<Vec<_>>(),
        [first, second]
    );
    assert_eq!(host.damages[1].effect.strength, 17);
    assert_eq!(arena.actor(second).unwrap().words_8e_92, [256, 0, 0]);

    let mut arena = new_arena();
    actor(&mut arena, [25, 0, 0]);
    let second = actor(&mut arena, [600, 0, 0]);
    let mut host = Host {
        mutations: vec![(
            1,
            Mutation::Source(
                RadiusEffectBlock {
                    outer_radius: 1000,
                    ..block()
                },
                [0; 3],
            ),
        )],
        ..Host::default()
    };
    apply(&mut arena, &mut host).unwrap();
    assert_eq!(host.damages.len(), 1); // Initial squared radius still rejects 600.
    assert_eq!(arena.actor(second).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn live_list_appends_and_earlier_removals_keep_next_pointer_order() {
    let mut arena = new_arena();
    let first = actor(&mut arena, [25, 0, 0]);
    let second = actor(&mut arena, [25, 0, 0]);
    let third = actor(&mut arena, [25, 0, 0]);
    let mut host = Host {
        mutations: vec![
            (1, Mutation::Append([25, 0, 0])),
            (2, Mutation::Destroy(first)),
        ],
        ..Host::default()
    };
    apply(&mut arena, &mut host).unwrap();
    assert_eq!(host.damages.len(), 4);
    assert_eq!(
        host.damages
            .iter()
            .take(3)
            .map(|call| call.actor)
            .collect::<Vec<_>>(),
        [first, second, third]
    );
    assert_eq!(host.damages[3].actor, arena.actor_list()[2]);
    assert_eq!(arena.actor(third).unwrap().words_8e_92, [256, 0, 0]);
}

#[test]
fn required_damage_failure_keeps_reached_mutations_rng_and_prior_impulses() {
    let mut arena = new_arena();
    let first = actor(&mut arena, [25, 0, 0]);
    let second = actor(&mut arena, [25, 0, 0]);
    let mut host = Host {
        mutations: vec![(2, Mutation::Mass(2048))],
        fail_damage_at: Some(2),
        rng_on_damage: true,
        ..Host::default()
    };
    let mut rng = 0x1234_5678;
    let mut expected = rng;
    crate::name_entry::name_entry_rng_next(&mut expected);
    crate::name_entry::name_entry_rng_next(&mut expected);
    assert!(matches!(
        apply_actor_radius_effect(
            &mut arena,
            ActorRadiusEffectRequest {
                origin: (),
                damage_source: ActorHandle::from_raw(0)
            },
            &mut host,
            &mut rng,
        ),
        Err(ActorRadiusEffectError::Host(_))
    ));
    assert_eq!(rng, expected);
    assert_eq!(arena.actor(first).unwrap().words_8e_92, [256, 0, 0]);
    assert_eq!(arena.actor(second).unwrap().word_8c, 2048);
    assert_eq!(arena.actor(second).unwrap().words_8e_92, [0; 3]);
}

#[test]
fn current_node_destruction_is_an_explicit_reached_effect_boundary() {
    let mut arena = new_arena();
    let current = actor(&mut arena, [25, 0, 0]);
    arena.apply_payload_flags(current, 4 | 8).unwrap(); // No post-damage actor read.
    let next = actor(&mut arena, [25, 0, 0]);
    let mut host = Host {
        mutations: vec![(1, Mutation::DestroyCurrent)],
        ..Host::default()
    };
    assert!(
        matches!(apply(&mut arena, &mut host), Err(ActorRadiusEffectError::RemovedCurrentNode(id)) if id == current)
    );
    assert_eq!(host.damages.len(), 1);
    assert_eq!(arena.actor_list(), [next]);
}
