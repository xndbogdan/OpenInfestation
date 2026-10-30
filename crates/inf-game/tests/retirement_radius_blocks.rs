//! Corpus compositions of both 00402250 radius blocks at initial placements.
//! These intentionally isolate the two scans: they do not substitute for the
//! intervening particle burst, scripted motion or a complete live retirement.

use inf_formats::{FrontendExecutable, TerrainResourceLookup};
use inf_game::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorDamageRequest, ActorHandle, ActorStateChangeEffect, ActorStateChangeHost,
        ActorStateChangeQuery, ObjectArena, ObjectId, Selector11RadiusBlock,
        Selector11RadiusOrigin,
    },
    process_particles::factories::ParticleFactoryProcessState,
    process_tasks::{
        damage::{
            services::NativeProcessActorDamageServices, ProcessActorDamageHost,
            ProcessDamageContext,
        },
        ProcessTaskRuntime,
    },
    terrain_runtime::{FurnitureDamageRequest, TerrainFurnitureEffectHost, TerrainRuntime},
    world_actor_radius_effect::{
        apply_actor_radius_effect, ActorRadiusDamageRequest, ActorRadiusEffectHost,
        ActorRadiusEffectRequest,
    },
    world_backend::{load_offline_world, OfflineWorldLoadRequest, WorldSession},
    world_furniture_effect::{
        resident::{resident_furniture_model_radius, resident_placement_table},
        ExecutableFurnitureDamageResources, FurnitureDamageRuntime, FurnitureEffectQueue,
    },
    world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime},
    world_mission::{PlacementDeathRequest, WorldMission},
    world_radius_effect::RadiusEffectBlock,
    world_statistics::WorldStatistics,
    Installation, ResidencyRequest, ResidentRuntime,
};

fn loaded(
    selection: u32,
    assets: &FrontendAssets,
) -> (
    WorldSession,
    ResidentRuntime,
    ProcessWorldMapRuntime,
    ParticleFactoryProcessState,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    resident.apply(ResidencyRequest::Boot).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let offline =
        inf_game::offline_prepare::offline_game_with_selection(&grants, Some(selection)).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner =
        inf_game::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        })
        .unwrap();
    let mut load_listener_effects = inf_game::world_backend::EmptyListenerEffects;
    let process = ParticleFactoryProcessState::native_startup();
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = inf_game::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = load_offline_world(
        &mut resident,
        inf_game::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut inf_game::terrain_runtime::ProcessTerrainPolicy::supplied(
                inf_game::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: inf_game::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: inf_game::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: inf_game::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets,
            process: Some(&process),
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
    let world = inf_game::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (world, resident, maps, process)
}

struct StopAtAc<'a>(&'a mut WorldMission);
impl ActorStateChangeHost for StopAtAc<'_> {
    type Error = String;
    type AttributionOwner = ();

    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, Self::Error> {
        arena
            .selector11_actor(actor)
            .map_err(|error| error.to_string())?;
        match query {
            //0051F080/+68 is the original 00401300 null-returning method.
            ActorStateChangeQuery::ExcludesAttribution68 => Ok(false),
            ActorStateChangeQuery::HasDerived58 => arena
                .actor_derived_component(actor)
                .map(|value| value.is_some())
                .map_err(|error| error.to_string()),
        }
    }
    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), Self::Error> {
        let categories = arena
            .retirement_attribution(actor, source)
            .map_err(|error| error.to_string())?;
        WorldStatistics::from_record(self.0.record_0c.as_mut().ok_or("missing W+0C statistics")?)
            .record_attribution(categories);
        Ok(())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), Self::Error> {
        match effect {
            ActorStateChangeEffect::StateOne { .. } => {
                Err("fixture reached actual virtual+AC".into())
            }
            ActorStateChangeEffect::IncrementSelector18Statistic { .. } => {
                WorldStatistics::from_record(
                    self.0.record_0c.as_mut().ok_or("missing W+0C statistics")?,
                )
                .increment_selector18_retirements();
                Ok(())
            }
            ActorStateChangeEffect::RecordPlacementDeath { index } => self
                .0
                .record_placement_death(PlacementDeathRequest {
                    index: i32::from(index),
                    inhibit_byte_08: self.0.inhibit_byte_08,
                    force: 0,
                })
                .map(|_| ())
                .map_err(|error| error.to_string()),
            _ => Err("unexpected earlier lifecycle service".into()),
        }
    }
}

fn retired_prefix(
    world: &mut WorldSession,
    reference: i32,
    process: &ParticleFactoryProcessState,
) -> ObjectId {
    let actor = world.arena.find_actor_by_placement(reference).unwrap();
    world.arena.selector11_actor(actor).unwrap();
    let error = world
        .arena
        .retire_actor(
            actor,
            process.process_null_handle,
            &mut StopAtAc(&mut world.mission),
        )
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("fixture reached actual virtual+AC"),
        "reference {reference}: {error:?}"
    );
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 1);
    assert_eq!(world.arena.actor(actor).unwrap().health, 0);
    actor
}

struct FurnitureHost<'a> {
    arena: &'a ObjectArena,
    resident: &'a ResidentRuntime,
    executable: FrontendExecutable<'a>,
    queue: &'a mut FurnitureEffectQueue,
    candidates: Vec<u8>,
    damages: Vec<FurnitureDamageRequest>,
}

impl TerrainFurnitureEffectHost for FurnitureHost<'_> {
    type Error = String;
    type Source = Selector11RadiusOrigin;

    fn source_block(&mut self, source: Self::Source) -> Result<RadiusEffectBlock, String> {
        source
            .effect_block(self.arena, self.executable)
            .map_err(|error| error.to_string())
    }
    fn source_position_words(&mut self, source: Self::Source) -> Result<[u16; 3], String> {
        source
            .position_words(self.arena)
            .map_err(|error| error.to_string())
    }
    fn furniture_model_radius(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<u16>, String> {
        self.candidates.push(lookup.raw());
        resident_furniture_model_radius(self.resident, lookup).map_err(|error| error.to_string())
    }
    fn apply_furniture_damage(
        &mut self,
        terrain: &mut TerrainRuntime,
        request: FurnitureDamageRequest,
        rng: &mut u32,
    ) -> Result<bool, String> {
        self.damages.push(request);
        let mut resources = ExecutableFurnitureDamageResources {
            placement_table: resident_placement_table(self.resident)
                .map_err(|error| error.to_string())?,
            executable: self.executable,
        };
        FurnitureDamageRuntime {
            network_active: false,
            resources: &mut resources,
            queue: self.queue,
        }
        .apply(terrain, request, rng)
        .map(|outcome| outcome.accepted())
        .map_err(|error| format!("{error:?}"))
    }
}

struct ActorHost<'a> {
    executable: FrontendExecutable<'a>,
    process: &'a ParticleFactoryProcessState,
    tasks: &'a mut ProcessTaskRuntime,
    mission: &'a mut WorldMission,
    hits: Vec<ActorRadiusDamageRequest>,
}

impl ActorRadiusEffectHost for ActorHost<'_> {
    type Error = String;
    type Source = Selector11RadiusOrigin;

    fn source_block(
        &mut self,
        arena: &ObjectArena,
        source: Self::Source,
    ) -> Result<RadiusEffectBlock, String> {
        source
            .effect_block(arena, self.executable)
            .map_err(|error| error.to_string())
    }
    fn source_position_words(
        &mut self,
        arena: &ObjectArena,
        source: Self::Source,
    ) -> Result<[u16; 3], String> {
        source
            .position_words(arena)
            .map_err(|error| error.to_string())
    }
    fn apply_damage(
        &mut self,
        arena: &mut ObjectArena,
        request: ActorRadiusDamageRequest,
        rng: &mut u32,
    ) -> Result<(), String> {
        self.hits.push(request);
        arena
            .apply_actor_damage(
                request.actor,
                ActorDamageRequest {
                    packet: request.effect.into(),
                    source: request.source,
                },
                &mut ProcessActorDamageHost::new(
                    ProcessDamageContext {
                        tasks: self.tasks,
                        mission: self.mission,
                    },
                    &mut NativeProcessActorDamageServices::new(rng, self.process),
                ),
            )
            .map_err(|error| format!("{error:?}"))
    }
    fn network_active(&mut self) -> Result<bool, String> {
        // Actual supplied offline process state; the production callback reads
        // this input only in the native mass>=128 impulse arm.
        Ok(self.process.network_flag != 0)
    }
    fn network_role(&mut self) -> Result<i32, String> {
        Err("required network role reached in offline fixture".into())
    }
}

#[test]
fn mission22_both_retirement_blocks_complete_real_damage_and_furniture_scans() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let (mut world, resident, _maps, process) = loaded(22, &assets);
    let null = process.process_null_handle.unwrap();
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(null).unwrap();
    let mut queue = FurnitureEffectQueue::default();
    let initial_rng = world.rng;
    let mut total_damage = 0;
    for reference in [30, 31] {
        let actor = retired_prefix(&mut world, reference, &process);
        // Authored H13/11 channel9 threshold equals the second block's
        // strength.0042A8F0 therefore returns zero for that unconditional B4.
        assert_eq!(
            world.arena.actor(actor).unwrap().damage_responses()[9],
            (1000, 256)
        );
        let categories = world.arena.retirement_attribution(actor, null).unwrap();
        for (block, strength, byte_03, adjusted) in [
            (Selector11RadiusBlock::Actor1A0, 2000, None, 1000),
            (Selector11RadiusBlock::Executable531250, 1000, Some(0), 0),
        ] {
            let origin = Selector11RadiusOrigin { actor, block };
            let mut furniture = FurnitureHost {
                arena: &world.arena,
                resident: &resident,
                executable,
                queue: &mut queue,
                candidates: Vec::new(),
                damages: Vec::new(),
            };
            let count = world
                .terrain
                .apply_furniture_radius_effect(origin, &mut furniture, &mut world.rng)
                .unwrap();
            assert_eq!(count, 0, "reference {reference} {block:?}");
            assert!(
                furniture.candidates.is_empty(),
                "reference {reference} {block:?}: {:?}",
                furniture.candidates
            );
            assert!(furniture.damages.is_empty());
            let mut host = ActorHost {
                executable,
                process: &process,
                tasks: &mut tasks,
                mission: &mut world.mission,
                hits: Vec::new(),
            };
            let outcome = apply_actor_radius_effect(
                &mut world.arena,
                ActorRadiusEffectRequest {
                    origin,
                    damage_source: null,
                },
                &mut host,
                &mut world.rng,
            )
            .unwrap();
            assert_eq!(outcome.alternate_hits, 0);
            assert_eq!(
                host.hits.len(),
                1,
                "reference {reference} {block:?}: {:?}",
                host.hits
            );
            assert_eq!(host.hits[0].actor, actor);
            assert_eq!(host.hits[0].effect.strength, strength);
            assert_eq!(host.hits[0].effect.byte_03, byte_03);
            assert_eq!(host.hits[0].source, null);
            total_damage += adjusted;
            assert_eq!(
                WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap())
                    .damage_total(categories),
                total_damage
            );
            assert_eq!(world.rng, initial_rng);
            assert_eq!(world.arena.actor(actor).unwrap().health, 0);
            assert_eq!(world.arena.actor(actor).unwrap().words_8e_92, [0; 3]);
        }
    }
    assert!(queue.records().is_empty());
}

#[test]
fn mission18_both_initial_radius_blocks_retain_reached_service_census() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    for reference in [5, 6, 7, 8, 93, 105, 106] {
        for block in [
            Selector11RadiusBlock::Actor1A0,
            Selector11RadiusBlock::Executable531250,
        ] {
            // Each case starts at initial custody. Failure of the first block
            // does not imply that 00402250 reaches its second call in production.
            let (mut world, resident, _maps, process) = loaded(18, &assets);
            let initial_rng = world.rng;
            let actor = retired_prefix(&mut world, reference, &process);
            let null = process.process_null_handle.unwrap();
            let mut tasks = ProcessTaskRuntime::default();
            tasks.retain(null).unwrap();
            let mut queue = FurnitureEffectQueue::default();
            let origin = Selector11RadiusOrigin { actor, block };
            let mut furniture = FurnitureHost {
                arena: &world.arena,
                resident: &resident,
                executable,
                queue: &mut queue,
                candidates: Vec::new(),
                damages: Vec::new(),
            };
            let terrain =
                world
                    .terrain
                    .apply_furniture_radius_effect(origin, &mut furniture, &mut world.rng);
            let candidates = furniture.candidates.clone();
            let damages = furniture.damages.len();
            drop(furniture);
            let mut host = ActorHost {
                executable,
                process: &process,
                tasks: &mut tasks,
                mission: &mut world.mission,
                hits: Vec::new(),
            };
            let actors = apply_actor_radius_effect(
                &mut world.arena,
                ActorRadiusEffectRequest {
                    origin,
                    damage_source: null,
                },
                &mut host,
                &mut world.rng,
            );
            let hits: Vec<_> = host
                .hits
                .iter()
                .map(|hit| {
                    (
                        world.arena.object(hit.actor).unwrap().base.placement_word,
                        hit.effect.strength,
                    )
                })
                .collect();
            let label = format!("mission18 reference{reference} {block:?}");
            assert_eq!(terrain.unwrap(), 0, "{label}");
            let expected_candidates: &[u8] = if reference == 93 { &[51] } else { &[] };
            assert_eq!(candidates, expected_candidates, "{label}");
            assert_eq!(damages, 0, "{label}");
            assert!(queue.records().is_empty(), "{label}");
            assert_eq!(actors.unwrap().alternate_hits, 0, "{label}");
            let expected_strength = match block {
                Selector11RadiusBlock::Actor1A0 => 2000,
                Selector11RadiusBlock::Executable531250 => 1000,
            };
            assert_eq!(hits, [(reference as u16 - 1, expected_strength)], "{label}");
            assert!(host.hits.iter().all(|hit| hit.source == null), "{label}");
            assert_eq!(world.arena.actor(actor).unwrap().health, 0, "{label}");
            assert_eq!(world.rng, initial_rng, "{label}");
        }
    }
}
