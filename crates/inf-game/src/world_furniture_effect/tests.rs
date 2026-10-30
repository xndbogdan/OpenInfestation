use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
use crate::world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime};
use inf_formats::{
    parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
    TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
};

fn terrain(spacing: TerrainSpacing, lookup: u8) -> TerrainRuntime {
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = (-1_i8) as u8;
    }
    bytes[TERRAIN_PREFIX_BYTES + 1] = 4;
    let bucket_offset = TERRAIN_PREFIX_BYTES + TERRAIN_CELL_COUNT * 2;
    bytes[bucket_offset..bucket_offset + 2].copy_from_slice(&4_u16.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0xa5, lookup]);
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainRuntime::new(
        &bank.entries[0],
        TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units32,
        },
    )
}

fn request(strength: i16) -> FurnitureDamageRequest {
    FurnitureDamageRequest {
        xz: [7, 9],
        effect: crate::world_radius_effect::ScaledRadiusEffect {
            strength,
            channel: 9,
            byte_03: None,
            impulse_magnitude: 0,
            impulse_vector: None,
        },
    }
}

#[derive(Default)]
struct Resources {
    slot_calls: Vec<u8>,
    rule_calls: Vec<(u8, u8)>,
    rule: Option<FurnitureDamageRule>,
    null_slot: bool,
    fail_rule: bool,
}

impl FurnitureDamageResources for Resources {
    type Error = &'static str;

    fn placement_slot(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<MissionPlacementSlot>, Self::Error> {
        self.slot_calls.push(lookup.raw());
        Ok((!self.null_slot).then_some(MissionPlacementSlot {
            slot: u16::from(lookup.raw()),
            resource_word: 123,
            parameter_word: 0xa507,
        }))
    }

    fn damage_rule(&mut self, kind: u8, channel: u8) -> Result<FurnitureDamageRule, Self::Error> {
        self.rule_calls.push((kind, channel));
        if self.fail_rule {
            return Err("unowned live resistance allocation");
        }
        Ok(self.rule.unwrap_or(FurnitureDamageRule {
            resistance: DamageResistance {
                threshold: 0,
                multiplier: 256,
            },
            threshold: 10,
            program: FurnitureEffectProgram::new(0x0053_2c88),
        }))
    }
}

#[test]
fn resistance_preserves_negative_strength_unsigned_threshold_and_q31_modifier() {
    let resistance = DamageResistance {
        threshold: 10,
        multiplier: 257,
    };
    assert_eq!(resisted_effect_damage(-300, resistance, i32::MAX), -300);
    assert_eq!(resisted_effect_damage(10, resistance, 0), 0);
    assert_eq!(resisted_effect_damage(266, resistance, 0), 257);
    assert_eq!(resisted_effect_damage(266, resistance, i32::MAX), 256);
    assert_eq!(resisted_effect_damage(266, resistance, i32::MIN), -257);
    assert_eq!(
        resisted_effect_damage(
            i16::MAX,
            DamageResistance {
                threshold: 0x8000,
                multiplier: 65535,
            },
            0
        ),
        0
    );
}

#[test]
fn network_and_h12_gates_precede_resources_and_special_lookup_precedes_rule() {
    let terrain = terrain(TerrainSpacing::Units128, 0xfd);
    let mut resources = Resources::default();
    let mut queue = FurnitureEffectQueue::default();
    let mut rng = 1000;
    let mut runtime = FurnitureDamageRuntime {
        network_active: true,
        resources: &mut resources,
        queue: &mut queue,
    };
    assert!(matches!(
        runtime.apply(&terrain, request(100), &mut rng).unwrap(),
        FurnitureDamageOutcome::Rejected(FurnitureDamageRejection::NetworkSession)
    ));
    assert!(runtime.resources.slot_calls.is_empty());
    runtime.network_active = false;
    let mut outside = request(100);
    outside.xz = [128, 128];
    assert!(matches!(
        runtime.apply(&terrain, outside, &mut rng).unwrap(),
        FurnitureDamageOutcome::Rejected(FurnitureDamageRejection::NoFurniture)
    ));
    assert!(runtime.resources.slot_calls.is_empty());
    assert!(matches!(
        runtime.apply(&terrain, request(100), &mut rng).unwrap(),
        FurnitureDamageOutcome::Rejected(FurnitureDamageRejection::SpecialLookup)
    ));
    assert_eq!(runtime.resources.slot_calls, [0xfd]);
    assert!(runtime.resources.rule_calls.is_empty());
    assert_eq!(rng, 1000);
    assert!(queue.records().is_empty());
}

#[test]
fn threshold_gate_uses_one_shared_rng_draw_and_accepts_equality_without_rng() {
    let terrain = terrain(TerrainSpacing::Units128, 5);
    let before = terrain.sparse_record_at([7, 9]).unwrap().1;
    let mut resources = Resources::default();
    let mut queue = FurnitureEffectQueue::default();
    let mut rng = 1000;
    let mut expected = rng;
    assert!(name_entry_rng_next(&mut expected) > 6553);
    let mut runtime = FurnitureDamageRuntime {
        network_active: false,
        resources: &mut resources,
        queue: &mut queue,
    };
    assert!(matches!(
        runtime.apply(&terrain, request(1), &mut rng).unwrap(),
        FurnitureDamageOutcome::Rejected(FurnitureDamageRejection::RandomThreshold)
    ));
    assert_eq!(rng, expected);
    assert!(runtime.queue.records().is_empty());
    assert_eq!(runtime.resources.rule_calls, [(7, 9)]);
    assert_eq!(
        runtime.apply(&terrain, request(10), &mut rng).unwrap(),
        FurnitureDamageOutcome::Accepted {
            damage: 10,
            queue: Some(FurnitureQueueOutcome::Enqueued { index: 0 })
        }
    );
    assert_eq!(rng, expected);
    assert_eq!(terrain.sparse_record_at([7, 9]).unwrap().1, before);
    assert_eq!(
        runtime.apply(&terrain, request(10), &mut rng).unwrap(),
        FurnitureDamageOutcome::Accepted {
            damage: 10,
            queue: Some(FurnitureQueueOutcome::Duplicate)
        }
    );
    assert_eq!(runtime.queue.records().len(), 1);
    assert_eq!(rng, expected);
}

#[test]
fn wrapped_signed_ratio_keeps_unsigned_rng_comparison() {
    let terrain = terrain(TerrainSpacing::Units128, 5);
    let mut resources = Resources {
        rule: Some(FurnitureDamageRule {
            resistance: DamageResistance {
                threshold: 0,
                multiplier: 65535,
            },
            threshold: 100_000,
            program: None,
        }),
        ..Resources::default()
    };
    let mut queue = FurnitureEffectQueue::default();
    let mut rng = 1000;
    let mut expected = rng;
    name_entry_rng_next(&mut expected);
    let outcome = FurnitureDamageRuntime {
        network_active: false,
        resources: &mut resources,
        queue: &mut queue,
    }
    .apply(&terrain, request(129), &mut rng)
    .unwrap();
    // 33023*65535 wraps negative before IDIV. The resulting negative
    // quotient compares as unsigned, admitting every low-WORD sample.
    assert_eq!(
        outcome,
        FurnitureDamageOutcome::Accepted {
            damage: 33_023,
            queue: None
        }
    );
    assert_eq!(rng, expected);
    assert!(queue.records().is_empty());
}

#[test]
fn unowned_resource_errors_do_not_consume_rng_or_queue_effects() {
    let terrain = terrain(TerrainSpacing::Units128, 5);
    let mut resources = Resources {
        fail_rule: true,
        ..Resources::default()
    };
    let mut queue = FurnitureEffectQueue::default();
    let mut rng = 1000;
    let error = FurnitureDamageRuntime {
        network_active: false,
        resources: &mut resources,
        queue: &mut queue,
    }
    .apply(&terrain, request(100), &mut rng)
    .unwrap_err();
    assert!(matches!(error, FurnitureDamageError::Resource(_)));
    assert_eq!(rng, 1000);
    assert!(queue.records().is_empty());
}

#[test]
fn queue_keys_align_but_positions_remain_unaligned_and_append_in_order() {
    let terrain = terrain(TerrainSpacing::Units256, 5);
    let first = FurnitureEffectProgram::new(0x0053_2c88).unwrap();
    let second = FurnitureEffectProgram::new(0x0053_2cd0).unwrap();
    let mut queue = FurnitureEffectQueue::default();
    assert_eq!(
        queue
            .enqueue(
                &terrain,
                FurnitureQueueRequest {
                    xz: [7, 9],
                    program: first
                }
            )
            .unwrap(),
        FurnitureQueueOutcome::Enqueued { index: 0 }
    );
    assert_eq!(queue.records()[0].position_words, [7, (-32_i16) as u16, 9]);
    assert_eq!(queue.records()[0].aligned_key, 0);
    assert_eq!(queue.records()[0].program, first);
    assert_eq!(
        (
            queue.records()[0].age_microseconds,
            queue.records()[0].removal_flag,
            queue.records()[0].remaining_microseconds
        ),
        (0, 0, 0)
    );
    assert_eq!(
        queue
            .enqueue(
                &terrain,
                FurnitureQueueRequest {
                    xz: [255, 255],
                    program: second
                }
            )
            .unwrap(),
        FurnitureQueueOutcome::Duplicate
    );
    assert_eq!(
        queue
            .enqueue(
                &terrain,
                FurnitureQueueRequest {
                    xz: [256, 65535],
                    program: second
                }
            )
            .unwrap(),
        FurnitureQueueOutcome::Enqueued { index: 1 }
    );
    assert_eq!(queue.records()[1].aligned_key, 0xff00_0100);
    assert_eq!(
        queue.records()[1].position_words,
        [256, (-32_i16) as u16, 65535]
    );
    assert_eq!(queue.records()[1].program, second);
    queue.clear();
    assert!(queue.records().is_empty());
}

#[test]
fn actual_executable_rules_resolve_original_file_backed_zero_table() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let table = inf_formats::MissionPlacementTable {
        local_index: 0,
        span: ByteSpan::new(0, 0),
        slots: Vec::new(),
    };
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    assert_eq!(executable.mapped(0x0054_6e38, 40).unwrap().bytes, [0; 40]);
    let mut resources = ExecutableFurnitureDamageResources {
        placement_table: &table,
        executable,
    };
    let rule = resources.damage_rule(0, 9).unwrap();
    assert_eq!(rule.threshold, 1000);
    assert_eq!(rule.program.unwrap().address(), 0x0053_2c88);
    let rule = resources.damage_rule(2, 9).unwrap();
    assert_eq!(rule.threshold, 1000);
    assert_eq!(rule.program.unwrap().address(), 0x0053_2cd0);
    assert_eq!(
        rule.resistance,
        DamageResistance {
            threshold: 0,
            multiplier: 0
        }
    );
    assert_eq!(resisted_effect_damage(2000, rule.resistance, 0), 0);
}

#[test]
fn mission22_initial_retirement_walks_have_no_furniture_candidates() {
    use crate::{
        frontend_assets::FrontendAssets,
        object_runtime::{ObjectArena, ObjectId},
        terrain_runtime::TerrainFurnitureEffectHost,
        world_radius_effect::RadiusEffectBlock,
        Installation, ResidencyRequest, ResidentRuntime,
    };
    use inf_formats::FrontendExecutable;
    struct Host<'a> {
        arena: &'a ObjectArena,
        model_lookups: Vec<u8>,
        damages: Vec<FurnitureDamageRequest>,
    }
    impl TerrainFurnitureEffectHost for Host<'_> {
        type Error = String;
        type Source = ObjectId;
        fn source_block(&mut self, actor: ObjectId) -> Result<RadiusEffectBlock, Self::Error> {
            let actor = self
                .arena
                .selector11_actor(actor)
                .map_err(|error| error.to_string())?;
            Ok(RadiusEffectBlock {
                strength: actor.word_1a0 as i16,
                channel: actor.byte_1a2,
                byte_03: actor.byte_1a3,
                inner_radius: actor.word_1a4,
                outer_radius: actor.word_1a6,
                impulse: actor.word_1a8,
            })
        }
        fn source_position_words(&mut self, actor: ObjectId) -> Result<[u16; 3], Self::Error> {
            self.arena
                .object(actor)
                .map_err(|error| error.to_string())?
                .base
                .position_words
                .ok_or_else(|| "source position is unwritten".into())
        }
        fn furniture_model_radius(
            &mut self,
            lookup: TerrainResourceLookup,
        ) -> Result<Option<u16>, Self::Error> {
            self.model_lookups.push(lookup.raw());
            Err("initial corpus walk unexpectedly reached a model candidate".into())
        }
        fn apply_furniture_damage(
            &mut self,
            _: &mut TerrainRuntime,
            request: FurnitureDamageRequest,
            _: &mut u32,
        ) -> Result<bool, Self::Error> {
            self.damages.push(request);
            Err("initial corpus walk unexpectedly reached furniture damage".into())
        }
    }
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut residency = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    residency.apply(ResidencyRequest::Boot).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let offline =
        crate::offline_prepare::offline_game(&inf_formats::parse_grant_table(executable).unwrap())
            .unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
    .unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut residency,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let mut world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    for reference in [30, 31] {
        let actor = world.arena.find_actor_by_placement(reference).unwrap();
        let mut host = Host {
            arena: &world.arena,
            model_lookups: Vec::new(),
            damages: Vec::new(),
        };
        let rng = world.rng;
        assert_eq!(
            world
                .terrain
                .apply_furniture_radius_effect(actor, &mut host, &mut world.rng)
                .unwrap(),
            0
        );
        assert_eq!(world.rng, rng);
        assert!(
            host.model_lookups.is_empty(),
            "initial placement reference {reference}"
        );
        assert!(
            host.damages.is_empty(),
            "initial placement reference {reference}"
        );
    }
}
