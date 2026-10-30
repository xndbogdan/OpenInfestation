use super::*;
use crate::{
    name_entry::name_entry_rng_next,
    process_particles::{
        births::{ParticleBirthInputs, ParticleBirthRequest},
        ParticleId, ProcessParticleRuntime,
    },
};
use inf_formats::FrontendExecutable;
use serde::Deserialize;

#[derive(Deserialize)]
struct Receipt {
    executable_sha256: String,
    boundaries: String,
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    size: u16,
    dirty: u16,
    c8: u16,
    ca: u16,
    cc: u16,
    ce: u16,
    health: u16,
    maximum: u16,
    velocity: [u16; 3],
    position: [u16; 3],
    microseconds: u32,
    load_factor: i32,
    rng: u32,
    #[serde(default)]
    missing_factor: bool,
    #[serde(default)]
    missing_position: bool,
    fail_random_after: Option<usize>,
    #[serde(default)]
    fail_trail: bool,
    #[serde(default)]
    fail_retire: bool,
    mutation: Option<Mutation>,
    events: Vec<Event>,
    #[serde(rename = "final")]
    final_state: [u32; 6],
    final_rng: u32,
    error: Option<Error>,
}

#[derive(Clone, Deserialize)]
struct Mutation {
    cc: Option<u16>,
    ce: Option<u16>,
    health: Option<u16>,
    maximum: Option<u16>,
    position: Option<[u16; 3]>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum Event {
    Random {
        value: u32,
        rng: u32,
    },
    Trail {
        particle_type: u32,
        position: [u16; 3],
        velocity: [u16; 3],
        flags: u32,
        snapshot: [u32; 6],
    },
    Retire {
        snapshot: [u32; 6],
    },
}

#[derive(Deserialize)]
struct Error {
    kind: String,
    address: u32,
}

fn snapshot(arena: &ObjectArena, actor: ObjectId) -> [u32; 6] {
    let state = arena.actor(actor).unwrap();
    [
        u32::from(arena.object(actor).unwrap().base.dirty_word),
        u32::from(state.word_c8),
        u32::from(state.word_ca.expect("supplied native +CA")),
        u32::from(state.word_cc),
        u32::from(state.word_ce),
        u32::from(state.health),
    ]
}

struct NativeHost<'a> {
    rng: u32,
    draws: usize,
    events: Vec<Event>,
    fail_random_after: Option<usize>,
    fail_trail: bool,
    fail_retire: bool,
    mutation: Option<Mutation>,
    executable: Option<FrontendExecutable<'a>>,
    particles: ProcessParticleRuntime,
    birth: Option<ParticleId>,
}

impl NativeHost<'_> {
    fn from_case(case: &Case) -> Self {
        Self {
            rng: case.rng,
            draws: 0,
            events: Vec::new(),
            fail_random_after: case.fail_random_after,
            fail_trail: case.fail_trail,
            fail_retire: case.fail_retire,
            mutation: case.mutation.clone(),
            executable: None,
            particles: ProcessParticleRuntime::default(),
            birth: None,
        }
    }
}

impl FamilyUpdateHost for NativeHost<'_> {
    type Error = &'static str;
    fn process_null_handle(&self) -> Option<crate::object_runtime::ActorHandle> {
        panic!("isolated status visualization does not run the water consumer")
    }

    fn random(&mut self) -> Result<u32, Self::Error> {
        let value = name_entry_rng_next(&mut self.rng);
        self.draws += 1;
        self.events.push(Event::Random {
            value,
            rng: self.rng,
        });
        if self.fail_random_after == Some(self.draws) {
            Err("supplied RNG failure")
        } else {
            Ok(value)
        }
    }

    fn effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        effect: FamilyUpdateEffect,
    ) -> Result<(), Self::Error> {
        match effect {
            FamilyUpdateEffect::StatusTrail {
                particle_type,
                position,
                velocity,
                flags,
            } => {
                self.events.push(Event::Trail {
                    particle_type,
                    position,
                    velocity,
                    flags,
                    snapshot: snapshot(arena, actor),
                });
                if self.fail_trail {
                    return Err("supplied trail failure");
                }
                if let Some(executable) = self.executable {
                    let birth_process =
                        crate::process_particles::factories::ParticleFactoryProcessState {
                            network_flag: 0,
                            local_slot: 1,
                            host_flag: 0,
                            settings: [0; 20],
                            process_null_handle: Some(
                                crate::object_runtime::ActorHandle::from_raw(0x1234_5678),
                            ),
                        };
                    self.birth = self
                        .particles
                        .birth_trail(
                            ParticleBirthRequest {
                                particle_type,
                                position,
                                velocity: Some(velocity),
                                flags,
                                inputs: ParticleBirthInputs {
                                    network_active: false,
                                    phase: Some(4),
                                    process: &birth_process,
                                },
                            },
                            executable,
                            &mut self.rng,
                        )
                        .map_err(|_| "actual particle birth failed")?;
                }
                if let Some(mutation) = &self.mutation {
                    let state = arena.actor_mut(actor).unwrap();
                    if let Some(value) = mutation.cc {
                        state.word_cc = value;
                    }
                    if let Some(value) = mutation.ce {
                        state.word_ce = value;
                    }
                    if let Some(value) = mutation.health {
                        state.health = value;
                    }
                    if let Some(value) = mutation.maximum {
                        state.maximum_health = Some(value);
                    }
                    if let Some(position) = mutation.position {
                        arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
                    }
                }
                Ok(())
            }
            FamilyUpdateEffect::Retire => {
                self.events.push(Event::Retire {
                    snapshot: snapshot(arena, actor),
                });
                if self.fail_retire {
                    Err("supplied retirement failure")
                } else {
                    Ok(())
                }
            }
            _ => Err("status case reached an unrelated host effect"),
        }
    }
}

fn receipt() -> Receipt {
    let receipt: Receipt = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/actor_status_visual_native.json"
    ))
    .unwrap();
    assert_eq!(
        receipt.executable_sha256,
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    assert!(receipt.boundaries.contains("no instruction patches"));
    receipt
}

fn supplied_arena(case: &Case) -> (ObjectArena, ObjectId) {
    let (mut arena, actor) = fixture(FamilyUpdateClass::Selector12);
    let base = &mut arena.owned_mut(actor).unwrap().state.base;
    base.word_28 = case.size;
    base.dirty_word = case.dirty;
    base.position_words = (!case.missing_position).then_some(case.position);
    let state = arena.actor_mut(actor).unwrap();
    state.word_c8 = case.c8;
    state.word_ca = Some(case.ca);
    state.word_cc = case.cc;
    state.word_ce = case.ce;
    state.health = case.health;
    state.maximum_health = Some(case.maximum);
    state.words_8e_92 = case.velocity;
    (arena, actor)
}

#[test]
fn authenticated_status_visual_preserves_native_packets_draw_order_and_failure_prefixes() {
    let receipt = receipt();
    assert_eq!(receipt.cases.len(), 60);
    for case in receipt.cases {
        let (mut arena, actor) = supplied_arena(&case);
        let mut host = NativeHost::from_case(&case);
        let result = arena.tick_actor_status(
            actor,
            case.microseconds,
            (!case.missing_factor).then_some(case.load_factor),
            &mut host,
        );
        match (&case.error, result) {
            (None, Ok(())) => {}
            (Some(expected), Err(actual)) => match (expected.kind.as_str(), actual) {
                ("factor", FamilyUpdateError::MissingStatusLoadFactor(id)) => assert_eq!(id, actor),
                ("position", FamilyUpdateError::Uninitialized { actor: id, offset }) => {
                    assert_eq!((id, offset), (actor, 0x32));
                }
                ("division", FamilyUpdateError::StatusVisualDivision { address, .. })
                | ("random", FamilyUpdateError::StatusVisualRandom { address, .. }) => {
                    assert_eq!(address, expected.address, "{}", case.name);
                }
                (
                    "trail",
                    FamilyUpdateError::Host {
                        effect: FamilyUpdateEffect::StatusTrail { .. },
                        ..
                    },
                )
                | (
                    "retire",
                    FamilyUpdateError::Host {
                        effect: FamilyUpdateEffect::Retire,
                        ..
                    },
                ) => {}
                (_, actual) => panic!("{}: unexpected error {actual:?}", case.name),
            },
            (_, actual) => panic!("{}: mismatched outcome {actual:?}", case.name),
        }
        assert_eq!(host.events, case.events, "{}", case.name);
        assert_eq!(snapshot(&arena, actor), case.final_state, "{}", case.name);
        assert_eq!(host.rng, case.final_rng, "{}", case.name);
        assert_eq!(arena.family429e20_actor(actor).unwrap().word_230, None);
        assert_eq!(arena.family428650_actor(actor).unwrap().word_220, None);
        if let Some(position) = case.mutation.as_ref().and_then(|value| value.position) {
            assert_eq!(
                arena.object(actor).unwrap().base.position_words,
                Some(position)
            );
        }
    }
}

#[test]
fn native_status_packet_feeds_the_owned_process_trail_producer_and_shared_rng() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("this corpus test requires the original retail executable");
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    for selector in [9, 10] {
        let case = receipt()
            .cases
            .into_iter()
            .find(|case| {
                case.name
                    == if selector == 9 {
                        "size-200-velocity-300"
                    } else {
                        "size-201-velocity-300"
                    }
            })
            .unwrap();
        let (mut arena, actor) = supplied_arena(&case);
        let mut host = NativeHost::from_case(&case);
        host.executable = Some(executable);
        arena
            .tick_actor_status(actor, case.microseconds, Some(case.load_factor), &mut host)
            .unwrap();
        assert_eq!(host.events, case.events);
        let Event::Trail {
            position,
            velocity,
            particle_type,
            flags,
            ..
        } = host.events.last().unwrap()
        else {
            panic!("successful case must end at the actual trail host");
        };
        assert_eq!(*particle_type, selector);
        assert_eq!(*flags, 0);
        let id = host
            .birth
            .expect("owned particle producer must publish a birth");
        let record = host.particles.record(id).unwrap();
        assert_eq!(record.position().unwrap(), *position);
        assert_eq!(
            std::array::from_fn::<_, 3, _>(|axis| record.read_word(10 + axis * 2).unwrap()),
            *velocity
        );
        assert_eq!(record.read_dword(0).unwrap(), 0x0052_88f4);
        assert_eq!(record.read_dword(0x28).unwrap(), 0x0053_d5f0);
        assert_eq!(record.read_dword(0x14).unwrap(), 0x1234_5678);
        assert_eq!(record.read_word(0x12).unwrap(), 0x009c);
        assert_eq!(
            host.particles
                .pending(1)
                .unwrap()
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            [id]
        );
        for category in 0..crate::process_particles::PARTICLE_LIST_COUNT {
            assert!(host.particles.active(category).unwrap().is_empty());
            if category != 1 {
                assert!(host.particles.pending(category).unwrap().is_empty());
            }
        }
        let mut expected_rng = case.final_rng;
        let scale_draw = name_entry_rng_next(&mut expected_rng) & 0xffff;
        assert_eq!(
            host.rng, expected_rng,
            "birth must consume the same process stream"
        );
        let scale_base: u32 = if selector == 9 { 0x400 } else { 0x800 };
        assert_eq!(
            record.read_dword(0x2c).unwrap(),
            scale_base + (scale_draw >> 9) - 64
        );
        assert_eq!(snapshot(&arena, actor), case.final_state);
    }
}
