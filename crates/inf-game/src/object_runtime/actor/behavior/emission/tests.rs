use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorServiceAllocationRequest,
    ActorServiceResourceKind, ActorServiceResourceState, ActorTerrainInputs,
    DerivedActorConstructionRequest,
};
use serde_json::{json, Value};

fn fixture() -> (ObjectArena, ObjectId, ProcessActorBehaviorTables) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector36_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([100, 200, 300]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &[0; 4096],
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    let mut behavior = super::super::build_actor_behavior(1, 0, 1).unwrap();
    behavior.byte_10 = 0;
    arena.derived_actor_mut(actor).unwrap().behavior_140 = Some(behavior);
    (
        arena,
        actor,
        ProcessActorBehaviorTables {
            descriptor_bytes: Some(Box::new([Some(0); 860])),
            registry_bytes: Some(Box::new([Some(0); 528])),
            default_packet_bytes: Some([Some(0); 4]),
            pursuit_ship_bytes: None,
        },
    )
}
fn u(v: &Value) -> u32 {
    v.as_u64()
        .map(|v| v as u32)
        .unwrap_or_else(|| v.as_i64().unwrap() as u32)
}
fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|i| u(&v[i]) as u16)
}
fn write(tables: &mut ProcessActorBehaviorTables, address: u32, bytes: &[u8]) {
    tables
        .write_process_bytes(
            address,
            &bytes.iter().copied().map(Some).collect::<Vec<_>>(),
        )
        .unwrap();
}
fn node(arena: &mut ObjectArena, value: NativeWeaponFireNode) -> NativeHeapRef {
    let reference = arena
        .actor_service_heap_mut()
        .allocate(ActorServiceAllocationRequest {
            kind: ActorServiceResourceKind::WeaponFireNode,
        })
        .unwrap()
        .unwrap();
    arena
        .actor_service_heap_mut()
        .construct_weapon_fire_node(reference, value)
        .unwrap();
    reference
}

struct BoundaryEmitter {
    refs: Vec<NativeHeapRef>,
    visits: Vec<Value>,
    stop: usize,
}
impl WeaponFiringEmitter for BoundaryEmitter {
    type Error = &'static str;
    fn emit(
        &mut self,
        arena: &mut ObjectArena,
        _: &mut WeaponFiringContext<'_>,
        request: WeaponEmissionRequest,
    ) -> Result<(), Self::Error> {
        let value = arena
            .actor_service_heap()
            .weapon_fire_node(request.node)
            .unwrap();
        let behavior = arena
            .derived_actor(request.actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        self.visits.push(
            json!({"node":self.refs.iter().position(|r|*r==request.node),
            "direction_float_bits":value.direction_bits,"position":request.position,
            "group_flags":behavior.groups_1c[value.group as usize].word_0c,
            "head":self.refs.iter().position(|r|Some(*r)==behavior.fire_head_00)}),
        );
        if self.stop == self.visits.len() {
            Err("native emitter boundary")
        } else {
            Ok(())
        }
    }
}

#[test]
fn firing_drain_matches_native_adjustment_order_signed_time_and_retained_failure_nodes() {
    let source: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/firing_drain_native.json"
    ))
    .unwrap();
    assert_eq!(source["provenance"]["fpcw"], 0x27f);
    for (case_index, case) in source["cases"].as_array().unwrap().iter().enumerate() {
        let (mut arena, actor, mut tables) = fixture();
        let input = &case["input"];
        let output = &case["output"];
        write(
            &mut tables,
            0x530e62,
            &(u(&input["speed"]) as u16).to_le_bytes(),
        );
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(words(&input["actor_position"]));
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&input["actor_velocity"]);
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.word_20 = u(&input["word20"]);
        let group = &mut behavior.groups_1c[0];
        group.word_0c = u(&input["group_flags"]);
        for axis in 0..3 {
            group.coordinates_00_0a[axis] = Some(words(&input["group_local"])[axis]);
            group.coordinates_00_0a[axis + 3] = Some(words(&input["group_world"])[axis]);
        }
        let mut refs = Vec::new();
        let mut next = None;
        for item in input["nodes"].as_array().unwrap().iter().rev() {
            let reference = node(
                &mut arena,
                NativeWeaponFireNode {
                    direction_bits: std::array::from_fn(|axis| {
                        u(&item["direction_float_bits"][axis])
                    }),
                    compensation: u(&item["time"]) as i32,
                    group: 0,
                    selection: u(&item["entry"]),
                    speed_scale: 0,
                    next,
                },
            );
            refs.push(reference);
            next = Some(reference);
        }
        refs.reverse();
        arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap()
            .fire_head_00 = next;
        let mut emitter = BoundaryEmitter {
            refs: refs.clone(),
            visits: Vec::new(),
            stop: u(&input["stop_at_emitter"]) as usize,
        };
        let mut rng = 0;
        let result = arena.drain_actor_firing(
            actor,
            &mut WeaponFiringContext {
                tables: &mut tables,
                rng: &mut rng,
            },
            &mut emitter,
        );
        assert_eq!(
            json!(result.is_ok()),
            output["normal_return"],
            "case{case_index}: {result:?}"
        );
        assert_eq!(json!(emitter.visits), output["emitted"], "case{case_index}");
        let behavior = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert_eq!(
            json!(refs.iter().position(|r| Some(*r) == behavior.fire_head_00)),
            output["head"]
        );
        assert_eq!(json!(behavior.groups_1c[0].word_0c), output["group_flags"]);
        assert_eq!(
            json!(std::array::from_fn::<_, 3, _>(|i| behavior.groups_1c[0]
                .coordinates_00_0a[i + 3]
                .unwrap())),
            output["group_world"]
        );
        let freed = output["freed"].as_array().unwrap().len();
        for (index, reference) in refs.iter().enumerate() {
            if index < freed {
                assert!(matches!(
                    arena.actor_service_heap().state(*reference),
                    Ok(ActorServiceResourceState::Released)
                ));
            } else {
                assert_eq!(
                    json!(
                        arena
                            .actor_service_heap()
                            .weapon_fire_node(*reference)
                            .unwrap()
                            .direction_bits
                    ),
                    output["nodes_direction_float_bits"][index]
                );
            }
        }
        if !result.is_ok() {
            let visited = emitter.visits.len();
            assert!(matches!(
                arena.drain_actor_firing(
                    actor,
                    &mut WeaponFiringContext {
                        tables: &mut tables,
                        rng: &mut rng
                    },
                    &mut emitter
                ),
                Err(WeaponFiringDrainError::PreviouslyStopped(_))
            ));
            assert_eq!(visited, emitter.visits.len());
        }
    }
}

fn retail_bytes() -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"))
        .unwrap()
}
fn residency() -> ResidentRuntime {
    ResidentRuntime::new(
        crate::Installation::open(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn firing_emitter_matches_native_velocity_statistics_and_sound_prefixes() {
    let source: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/firing_emitter_native.json"
    ))
    .unwrap();
    let bytes = retail_bytes();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let resident = residency();
    let mut compared = 0;
    for (index, case) in source["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let output = &case["output"];
        // Native supplied factory-return mutation tests the reentrant ABI.
        // This production bridge admits no reentrant services. A supplied
        // stop-loop boundary also does not supply a constructed sound record.
        if input["mutate_after_factory"] == true
            || (u(&input["old_sound"]) != 0 && input["stop_audio"] == true)
        {
            continue;
        }
        let (mut arena, actor, _) = fixture();
        let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
        write(
            &mut tables,
            0x530e60,
            &(u(&input["kind"]) as u16).to_le_bytes(),
        );
        write(
            &mut tables,
            0x530e62,
            &(u(&input["speed"]) as u16).to_le_bytes(),
        );
        write(
            &mut tables,
            0x530e64,
            &(u(&input["sound"]) as u16).to_le_bytes(),
        );
        write(&mut tables, 0x530e80, &[u(&input["loop_audio"]) as u8]);
        write(&mut tables, 0x530e83, &[u(&input["loop_byte"]) as u8]);
        arena.actor_mut(actor).unwrap().handle = ActorHandle::from_raw(u(&input["handle"]));
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&input["actor_velocity"]);
        arena.actor_mut(actor).unwrap().word_b4 = u(&input["category_input"]);
        arena.owned_mut(actor).unwrap().state.base.flags = u(&input["actor_flags"]);
        let reference = node(
            &mut arena,
            NativeWeaponFireNode {
                direction_bits: std::array::from_fn(|i| u(&input["direction_float_bits"][i])),
                compensation: 0,
                group: 0,
                selection: 0,
                speed_scale: u(&input["speed_scale"]),
                next: None,
            },
        );
        let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
        let old = if u(&input["old_sound"]) != 0 {
            Some(
                sounds
                    .create_persistent(&resident, 56, 0x10000, 0x10000, 0, || {
                        Ok::<_, std::convert::Infallible>([0; 3])
                    })
                    .unwrap(),
            )
        } else {
            None
        };
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.word_38 = 1234;
        behavior.word_2c = u(&input["old_sound_kind"]);
        behavior.sound_28 = old;
        if input["stop_audio"] == true {
            sounds.fail_next_record_allocation(3);
        }
        let mut statistics = [0_u8; RECORD_0C_BYTES];
        for (i, count) in [10_u32, 20, 30, 40, 50].into_iter().enumerate() {
            statistics[0x14 + i * 4..0x18 + i * 4].copy_from_slice(&count.to_le_bytes());
        }
        let mut particles = ProcessParticleRuntime::default();
        let mut process = ParticleFactoryProcessState::native_startup();
        // The native run stops inside the online notice's 00492500.
        if input["stop_factory"] == true {
            process.network_flag = 1;
            particles.refuse_network_sends = true;
        }
        let mut rng = u(&input["rng"]);
        let result = WeaponFiringRuntime {
            executable,
            factory_process: &process,
            particles: &mut particles,
            statistics: input["statistics"]
                .as_bool()
                .unwrap()
                .then_some(&mut statistics),
            sounds: &mut sounds,
            residency: &resident,
            phase: Some(4),
            children: None,
            pulse: None,
        }
        .emit(
            &mut arena,
            &mut WeaponFiringContext {
                tables: &mut tables,
                rng: &mut rng,
            },
            WeaponEmissionRequest {
                actor,
                node: reference,
                position: words(&input["position"]),
            },
        );
        assert_eq!(
            json!(result.is_ok()),
            output["normal_return"],
            "case{index}: {result:?}"
        );
        assert_eq!(json!(rng), output["rng_after"]);
        let counts: Vec<_> = (0..5)
            .map(|i| u32::from_le_bytes(statistics[0x14 + i * 4..0x18 + i * 4].try_into().unwrap()))
            .collect();
        assert_eq!(json!(counts), output["statistics"], "case{index}");
        let behavior = arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap();
        assert_eq!(json!(behavior.word_38), output["sound_lifetime"]);
        assert_eq!(json!(behavior.word_2c), output["sound_kind"]);
        if input["stop_factory"] != true {
            let particle = particles.pending(3).unwrap()[0];
            let record = particles.record(particle).unwrap();
            let expected = &case["boundaries"][0]["request"];
            assert_eq!(json!(record.position().unwrap()), expected["position"]);
            assert_eq!(
                json!(std::array::from_fn::<_, 3, _>(|axis| record
                    .read_word(10 + axis * 2)
                    .unwrap())),
                expected["velocity"]
            );
            assert_eq!(json!(record.read_dword(0x14).unwrap()), expected["handle"]);
            assert_eq!(record.read_dword(0).unwrap(), 0x526efc);
            assert_eq!(
                json!(record.read_dword(0x3c).unwrap()),
                expected["selector"]
            );
        }
        if result.is_ok() {
            let retained =
                input["loop_audio"] == 1 && u(&input["old_sound_kind"]) == u(&input["sound"]);
            assert_eq!(sounds.pending_len(), usize::from(!retained));
            if input["loop_audio"] == 1 && !retained {
                assert!(behavior.sound_28.is_some());
            }
        }
        compared += 1;
    }
    assert_eq!(compared, 18);
}

#[test]
fn firing_completed_factory_survives_later_audio_failure_without_replay() {
    let bytes = retail_bytes();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    let resident = residency();
    let (mut arena, actor, _) = fixture();
    let mut tables = ProcessActorBehaviorTables::from_executable(executable).unwrap();
    let reference = node(
        &mut arena,
        NativeWeaponFireNode {
            direction_bits: [0; 3],
            compensation: 0,
            group: 0,
            selection: 0,
            speed_scale: 0,
            next: None,
        },
    );
    arena
        .derived_actor_mut(actor)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .fire_head_00 = Some(reference);
    let mut particles = ProcessParticleRuntime::default();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    sounds.fail_next_record_allocation(9);
    let mut statistics = [0; RECORD_0C_BYTES];
    let process = ParticleFactoryProcessState::native_startup();
    let mut rng = 0x1234;
    let mut host = WeaponFiringRuntime {
        executable,
        factory_process: &process,
        particles: &mut particles,
        statistics: Some(&mut statistics),
        sounds: &mut sounds,
        residency: &resident,
        phase: Some(4),
        children: None,
        pulse: None,
    };
    let mut context = WeaponFiringContext {
        tables: &mut tables,
        rng: &mut rng,
    };
    assert!(matches!(
        arena.drain_actor_firing(actor, &mut context, &mut host),
        Err(WeaponFiringDrainError::Emission {
            source: WeaponEmissionError::Sound(_),
            ..
        })
    ));
    assert_eq!(host.particles.pending(3).unwrap().len(), 1);
    assert!(matches!(
        arena.drain_actor_firing(actor, &mut context, &mut host),
        Err(WeaponFiringDrainError::PreviouslyStopped(_))
    ));
    assert_eq!(host.particles.pending(3).unwrap().len(), 1);
    assert_eq!(
        arena
            .derived_actor(actor)
            .unwrap()
            .behavior_140
            .as_ref()
            .unwrap()
            .fire_head_00,
        Some(reference)
    );
    let particle = host.particles.pending(3).unwrap()[0];
    let source_handle = arena.actor(actor).unwrap().handle;
    arena
        .destroy(crate::object_runtime::ObjectDestructionRequest {
            object: actor,
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(matches!(
        arena.actor_service_heap().state(reference),
        Ok(ActorServiceResourceState::Released)
    ));
    assert_eq!(host.particles.pending(3).unwrap().len(), 1);
    assert_eq!(
        host.particles
            .record(particle)
            .unwrap()
            .read_dword(0x14)
            .unwrap(),
        source_handle.raw()
    );
}

#[test]
fn firing_prefix_and_post_unlink_release_failures_stop_the_actor_chain() {
    for fail_release in [false, true] {
        let (mut arena, actor, mut tables) = fixture();
        let reference = node(
            &mut arena,
            NativeWeaponFireNode {
                direction_bits: [0; 3],
                compensation: 0,
                group: 0,
                selection: 0,
                speed_scale: 0,
                next: None,
            },
        );
        let behavior = arena
            .derived_actor_mut(actor)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        behavior.fire_head_00 = Some(reference);
        if fail_release {
            arena.actor_service_heap_mut().fixture_fail_next_release(9);
        } else {
            // Native cached flag2 demands the unowned world WORD instead of
            // using the actor position fallback. No callback may run.
            behavior.groups_1c[0].word_0c = 2;
        }
        let mut emitter = BoundaryEmitter {
            refs: vec![reference],
            visits: Vec::new(),
            stop: 0,
        };
        let mut rng = 7;
        let mut context = WeaponFiringContext {
            tables: &mut tables,
            rng: &mut rng,
        };
        let result = arena.drain_actor_firing(actor, &mut context, &mut emitter);
        if fail_release {
            assert!(matches!(result, Err(WeaponFiringDrainError::Heap(_))));
        } else {
            assert!(matches!(result, Err(WeaponFiringDrainError::Position(_))));
        }
        let before = arena.derived_actor(actor).unwrap().behavior_140.clone();
        assert_eq!(emitter.visits.len(), usize::from(fail_release));
        assert!(
            matches!(arena.drain_actor_firing(actor, &mut context, &mut emitter),
            Err(WeaponFiringDrainError::PreviouslyStopped(r)) if r == reference)
        );
        assert_eq!(arena.derived_actor(actor).unwrap().behavior_140, before);
        assert_eq!(emitter.visits.len(), usize::from(fail_release));
    }
}
