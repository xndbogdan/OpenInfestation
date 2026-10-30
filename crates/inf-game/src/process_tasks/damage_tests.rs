use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorDamagePacket, ActorDamageReactionEffect, ActorDamageReactionHost, ActorDamageRequest,
        ActorStateChangeEffect, ActorStateChangeHost, ActorStateChangeQuery,
    },
    world_actor_radius_effect::{
        apply_actor_radius_effect, ActorRadiusDamageRequest, ActorRadiusEffectHost,
        ActorRadiusEffectRequest,
    },
    world_backend::{load_offline_world, OfflineWorldLoadRequest, WorldSession},
    world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime},
    world_mission::PlacementDeathRequest,
    world_radius_effect::RadiusEffectBlock,
    world_statistics::{ActorRetirementAttribution, ActorStatisticCategory},
    Installation, ResidencyRequest, ResidentRuntime,
};

fn loaded() -> (WorldSession, ProcessWorldMapRuntime) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let grants = inf_formats::parse_grant_table(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner =
        crate::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        })
        .unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = load_offline_world(
        &mut runtime,
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
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    (world, maps)
}

struct StopAtStateOne<'a>(&'a mut WorldMission);

impl ActorStateChangeHost for StopAtStateOne<'_> {
    type Error = String;
    type AttributionOwner = ();
    fn query(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        query: ActorStateChangeQuery,
    ) -> Result<bool, String> {
        // This fixture admits only the actual selector11 vtable. It executes
        // reached bookkeeping and fails at the still-required +AC callback.
        arena
            .selector11_actor(actor)
            .map_err(|error| error.to_string())?;
        Ok(match query {
            ActorStateChangeQuery::ExcludesAttribution68 => false,
            ActorStateChangeQuery::HasDerived58 => true,
        })
    }
    fn attribute(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        source: ActorHandle,
    ) -> Result<(), String> {
        let record = self.0.record_0c.as_mut().ok_or("missing statistics")?;
        let categories = arena
            .retirement_attribution(actor, source)
            .map_err(|error| error.to_string())?;
        WorldStatistics::from_record(record).record_attribution(categories);
        Ok(())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: ActorStateChangeEffect<()>,
    ) -> Result<(), String> {
        match effect {
            ActorStateChangeEffect::RecordPlacementDeath { index } => self
                .0
                .record_placement_death(PlacementDeathRequest {
                    index: i32::from(index),
                    inhibit_byte_08: self.0.inhibit_byte_08,
                    force: 0,
                })
                .map(|_| ())
                .map_err(|error| error.to_string()),
            ActorStateChangeEffect::IncrementSelector18Statistic { .. } => {
                WorldStatistics::from_record(
                    self.0.record_0c.as_mut().ok_or("missing statistics")?,
                )
                .increment_selector18_retirements();
                Ok(())
            }
            _ => Err(format!("required retirement callback {effect:?}")),
        }
    }
}

fn retired(world: &mut WorldSession, reference: i32) -> ObjectId {
    let actor = world.arena.find_actor_by_placement(reference).unwrap();
    let error = world
        .arena
        .retire_actor(
            actor,
            Some(ActorHandle::from_raw(0)),
            &mut StopAtStateOne(&mut world.mission),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("StateOne"));
    assert_eq!(world.arena.actor(actor).unwrap().word_c4, 1);
    assert_eq!(world.arena.actor(actor).unwrap().health, 0);
    actor
}

/// These services are required errors, not successful defaults. Initial
/// selector11/C4=1 skips them through the actual derived response owner.
struct RequiredReaction;
impl ActorDamageReactionHost for RequiredReaction {
    type Error = String;
    type RecentSource = ();
    fn process_null_handle(&self) -> Option<ActorHandle> {
        None
    }
    fn network_active(&mut self) -> Result<bool, String> {
        Err("required network-active reaction query".into())
    }
    fn difficulty_multiplier(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<i16, String> {
        Err("required difficulty reaction query".into())
    }
    fn source_bonuses(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<[i8; 6]>, String> {
        Err("required player bonus query".into())
    }
    fn animation_tick(&mut self) -> Result<u32, String> {
        Err("required animation clock query".into())
    }
    fn network_role(&mut self) -> Result<i32, String> {
        Err("required network-role query".into())
    }
    fn local_player_byte(&mut self) -> Result<u8, String> {
        Err("required local-player query".into())
    }
    fn excludes_status(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<bool, String> {
        Err("required status exclusion query".into())
    }
    fn recent_damage_source(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<()>, String> {
        Err("required recent-source query".into())
    }
    fn recent_source_tick(&mut self, _: &mut ObjectArena, _: ()) -> Result<u32, String> {
        Err("required recent-source timestamp".into())
    }
    fn recent_source_owner_word(&mut self, _: &mut ObjectArena, _: ()) -> Result<u16, String> {
        Err("required recent-source owner".into())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ActorDamageReactionEffect,
        _: &mut u32,
    ) -> Result<(), String> {
        Err("required damage reaction effect".into())
    }
}

struct DerivedServices<'a> {
    rng: &'a mut u32,
    calls: Vec<(ObjectId, ActorDamageCallback)>,
    notifications: Vec<(ObjectId, ObjectId)>,
    replace_task: bool,
}

impl<'a> DerivedServices<'a> {
    fn new(rng: &'a mut u32) -> Self {
        Self {
            rng,
            calls: Vec::new(),
            notifications: Vec::new(),
            replace_task: false,
        }
    }
}

impl ProcessActorDamageServices for DerivedServices<'_> {
    type Error = String;
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0)) //actual fixture process startup
    }
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
        context: ProcessDamageContext<'_>,
    ) -> Result<(), String> {
        arena
            .selector11_actor(actor)
            .map_err(|error| error.to_string())?;
        self.calls.push((actor, callback));
        arena
            .react_derived_actor_damage(actor, callback, self.rng, &mut RequiredReaction)
            .map_err(|error| format!("{error:?}"))?;
        if self.replace_task {
            let classification = arena
                .actor(actor)
                .map_err(|error| error.to_string())?
                .word_b4;
            let old = context
                .tasks
                .owner(classification)
                .map_err(|error| error.to_string())?
                .unwrap();
            context
                .tasks
                .destroy(old)
                .map_err(|error| error.to_string())?;
            let null = arena.actor_handles().unwrap().null_handle();
            let replacement = context
                .tasks
                .construct(classification, null)
                .map_err(|error| error.to_string())?;
            context
                .tasks
                .write_source(replacement, callback.source)
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
    fn source_component_108(
        &mut self,
        _: &mut ObjectArena,
        component: ObjectId,
        target: ObjectId,
        _: ProcessDamageContext<'_>,
    ) -> Result<(), String> {
        self.notifications.push((component, target));
        Err("required real source+108 callback".into())
    }
}

struct RadiusHost<'a> {
    tasks: &'a mut ProcessTaskRuntime,
    mission: &'a mut WorldMission,
    hits: Vec<ActorRadiusDamageRequest>,
}
impl ActorRadiusEffectHost for RadiusHost<'_> {
    type Error = String;
    type Source = ObjectId;
    fn source_block(
        &mut self,
        arena: &ObjectArena,
        source: ObjectId,
    ) -> Result<RadiusEffectBlock, String> {
        let state = arena
            .selector11_actor(source)
            .map_err(|error| error.to_string())?;
        Ok(RadiusEffectBlock {
            strength: state.word_1a0 as i16,
            channel: state.byte_1a2,
            byte_03: state.byte_1a3,
            inner_radius: state.word_1a4,
            outer_radius: state.word_1a6,
            impulse: state.word_1a8,
        })
    }
    fn source_position_words(
        &mut self,
        arena: &ObjectArena,
        source: ObjectId,
    ) -> Result<[u16; 3], String> {
        arena
            .object(source)
            .map_err(|error| error.to_string())?
            .base
            .position_words
            .ok_or_else(|| "missing source position".into())
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
                    &mut DerivedServices::new(rng),
                ),
            )
            .map_err(|error| format!("{error:?}"))
    }
    fn network_active(&mut self) -> Result<bool, String> {
        Err("required radius network query".into())
    }
    fn network_role(&mut self) -> Result<i32, String> {
        Err("required radius network role".into())
    }
}

fn damage_request(source: ActorHandle) -> ActorDamageRequest {
    ActorDamageRequest {
        packet: ActorDamagePacket {
            strength: 2000,
            channel: 9,
            byte_03: None,
        },
        source,
    }
}

#[test]
fn mission22_retiring_actors_each_self_hit_with_real_task_source_and_damage_statistics() {
    let (mut world, _maps) = loaded();
    let mut tasks = ProcessTaskRuntime::default();
    let null = world.arena.actor_handles().unwrap().null_handle();
    tasks.retain(null).unwrap();
    let owner = tasks.owner(2).unwrap().unwrap();
    let initial_rng = world.rng;
    let categories = ActorRetirementAttribution {
        target: ActorStatisticCategory::OtherB4,
        source: ActorStatisticCategory::B4ZeroOrUnresolved,
    };
    for (index, reference) in [30, 31].into_iter().enumerate() {
        let actor = retired(&mut world, reference);
        let mut host = RadiusHost {
            tasks: &mut tasks,
            mission: &mut world.mission,
            hits: Vec::new(),
        };
        let outcome = apply_actor_radius_effect(
            &mut world.arena,
            ActorRadiusEffectRequest {
                origin: actor,
                damage_source: null,
            },
            &mut host,
            &mut world.rng,
        )
        .unwrap();
        assert_eq!(outcome.alternate_hits, 0);
        assert_eq!(host.hits.len(), 1);
        assert_eq!(host.hits[0].actor, actor);
        assert_eq!(host.hits[0].effect.strength, 2000);
        assert_eq!(host.hits[0].effect.byte_03, None);
        assert_eq!(tasks.owner(2).unwrap(), Some(owner));
        assert_eq!(tasks.record(owner).unwrap().source, null);
        assert_eq!(tasks.record(owner).unwrap().counters, [None; 5]);
        assert_eq!(world.rng, initial_rng);
        assert_eq!(world.arena.actor(actor).unwrap().health, 0);
        assert_eq!(world.arena.actor(actor).unwrap().words_8e_92, [0; 3]);
        assert_eq!(
            WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap())
                .damage_total(categories),
            1000 * (index as u32 + 1)
        );
    }
}

#[test]
fn live_task_replacement_at_b4_is_read_then_cleared_before_missing_statistics_error() {
    let (mut world, _maps) = loaded();
    let actor = retired(&mut world, 30);
    let null = world.arena.actor_handles().unwrap().null_handle();
    let source = world.arena.actor(actor).unwrap().handle;
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(null).unwrap();
    let old = tasks.owner(2).unwrap().unwrap();
    world.mission.record_0c = None;
    let mut services = DerivedServices::new(&mut world.rng);
    services.replace_task = true;
    let error = world
        .arena
        .apply_actor_damage(
            actor,
            damage_request(source),
            &mut ProcessActorDamageHost::new(
                ProcessDamageContext {
                    tasks: &mut tasks,
                    mission: &mut world.mission,
                },
                &mut services,
            ),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("MissingStatistics"));
    let replacement = tasks.owner(2).unwrap().unwrap();
    assert_ne!(old, replacement);
    assert_eq!(tasks.record(replacement).unwrap().source, null);
    assert!(tasks.record(old).is_err());
}

#[test]
fn live_health_reaction_and_source_notification_remain_required_services() {
    let (mut world, _maps) = loaded();
    let actor = world.arena.find_actor_by_placement(30).unwrap();
    let null = world.arena.actor_handles().unwrap().null_handle();
    let mut tasks = ProcessTaskRuntime::default();
    tasks.retain(null).unwrap();
    let mut services = DerivedServices::new(&mut world.rng);
    let error = world
        .arena
        .apply_actor_damage(
            actor,
            damage_request(null),
            &mut ProcessActorDamageHost::new(
                ProcessDamageContext {
                    tasks: &mut tasks,
                    mission: &mut world.mission,
                },
                &mut services,
            ),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("required network-active"));
    assert_eq!(world.arena.actor(actor).unwrap().word_ca, Some(55));
    drop(services);
    let actor = retired(&mut world, 30);
    let family_source = world.arena.find_actor_by_placement(48).unwrap();
    let source = world.arena.actor(family_source).unwrap().handle;
    let mut services = DerivedServices::new(&mut world.rng);
    let error = world
        .arena
        .apply_actor_damage(
            actor,
            damage_request(source),
            &mut ProcessActorDamageHost::new(
                ProcessDamageContext {
                    tasks: &mut tasks,
                    mission: &mut world.mission,
                },
                &mut services,
            ),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("required real source+108"));
    assert_eq!(services.notifications, [(family_source, actor)]);
    let categories = world.arena.retirement_attribution(actor, source).unwrap();
    assert_eq!(
        WorldStatistics::from_record(world.mission.record_0c.as_mut().unwrap())
            .damage_total(categories),
        0
    );
}

#[test]
fn statistics_publication_rejects_replacement_after_capture_without_writing_the_new_record() {
    let (mut world, _maps) = loaded();
    let mut tasks = ProcessTaskRuntime::default();
    let mut services = DerivedServices::new(&mut world.rng);
    let mut host = ProcessActorDamageHost::new(
        ProcessDamageContext {
            tasks: &mut tasks,
            mission: &mut world.mission,
        },
        &mut services,
    );
    let captured = host.statistics_owner(&mut world.arena).unwrap();
    // Keep the old allocation alive: allocator reuse cannot mask the changed
    // owner. Arbitrary post-capture aliases are a required boundary.
    let old = host.context.mission.record_0c.take().unwrap();
    host.context.mission.record_0c = Some(Box::new([0; crate::world_mission::RECORD_0C_BYTES]));
    let categories = ActorRetirementAttribution {
        target: ActorStatisticCategory::OtherB4,
        source: ActorStatisticCategory::B4ZeroOrUnresolved,
    };
    assert!(matches!(
        host.record_statistics(
            &mut world.arena,
            captured,
            ActorDamageStatistic {
                amount: 1000,
                categories
            }
        ),
        Err(ProcessActorDamageError::ReplacedStatistics)
    ));
    assert!(old.iter().all(|byte| *byte == 0));
    assert!(host
        .context
        .mission
        .record_0c
        .as_ref()
        .unwrap()
        .iter()
        .all(|byte| *byte == 0));
}
