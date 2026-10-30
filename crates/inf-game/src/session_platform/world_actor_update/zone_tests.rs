//! Helniazine's zone in its world (Leaf 16, world 6), as Infest03 built it
//! (`zone-build.js`), and its retirements through the world's lifecycle.

use super::super::weapon_test_support::{fire, Owners};
use super::*;
use crate::object_runtime::ActorMessage;
use crate::session_platform::particles::damage::{BehaviorFactoryLoan, WorldDamageOwners};
use inf_formats::FrontendExecutable;

fn zone(world: &crate::world_backend::WorldSession) -> ObjectId {
    let zones: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector74(actor).is_ok())
        .collect();
    assert_eq!(zones.len(), 1);
    zones[0]
}

/// One update of the zone through the world's host.
fn tick(
    world: &mut crate::world_backend::WorldSession,
    resident: &crate::ResidentRuntime,
    executable: FrontendExecutable<'_>,
    owners: &mut Owners,
    sine: &[u16; 4096],
    zone: ObjectId,
    rng: &mut u32,
) -> Result<(), String> {
    let mut host = WorldZoneHost {
        family: WorldFamilyUpdateHost {
            owners: WorldDamageOwners {
                grant_text: None,
                camera_settings: None,
                flash: None,
                mission: &mut world.mission,
                tasks: &mut owners.tasks,
                terrain: &mut world.terrain,
                particles: &mut owners.particles,
                sounds: &mut owners.sounds,
                furniture: &mut owners.furniture,
                residency: resident,
                executable,
                factory_process: &mut owners.process,
                behavior_tables: &mut owners.tables,
                animation_clock: &owners.clock,
                session: &owners.state,
                particle_load_factor: 0x10000,
                behavior_factory: Some(BehaviorFactoryLoan {
                    listeners: &mut world.listeners,
                    sine,
                    world_flags: world.world_flags,
                }),
                map: None,
            },
            rng,
            sine_quarter: sine,
        },
    };
    world
        .arena
        .tick_selector74(zone, 125_000, &mut host)
        .map_err(|error| error.to_string())
}

/// Infest03's zone after its construction (2838E6) and its link (2838FC):
/// the payload's 256 high, 4000 wide and 2000 deep, then commands 16 (1000)
/// and 17 (600), give the half extents 256, 500 and 300; the finish's
/// sphere 0x63020, circle 0x247 and one point; one link from the after-load.
/// Nothing writes its timer; the fresh-heap carry starts it at 0, as
/// Infest03's held.
#[test]
fn helniazines_zone_is_built_and_linked_as_infest03s() {
    let (_resident, _assets, world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let zone = zone(&world);
    let state = world.arena.selector74(zone).unwrap();
    assert_eq!(state.extents_150, Some([256, 500, 300]));
    assert_eq!(state.radius2_138, Some(0x63020));
    assert_eq!(state.circle_140, Some(0x247));
    assert_eq!(state.count_144, Some(1));
    assert_eq!((state.linked_148, state.links_14c), (1, 1));
    assert_eq!(state.timer_13c, Some(0));
    let base = &world.arena.object(zone).unwrap().base;
    assert_eq!(base.position_words, Some([34016, 64576, 23433]));
    // 0048EFC0(0x63020), and the shared constructor's +74.
    assert_eq!(base.word_28, 636);
    assert_eq!(world.arena.actor(zone).unwrap().word_74, 0);
}

/// A grenade thrown at the zone's middle is inside it: the zone's update
/// retires it (0047CE90), and the grenade's state 1 (00458F80) marks it to
/// go off at its next update. A shared actor there is left alone.
#[test]
fn the_zone_sets_off_a_grenade_inside_it() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let zone = zone(&world);
    let middle = world
        .arena
        .object(zone)
        .unwrap()
        .base
        .position_words
        .unwrap();
    let before: Vec<_> = world.arena.actor_list().to_vec();
    let mut rng = 0x1234_5678;
    fire(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        8,
        middle,
        &mut rng,
    );
    let thrown: Vec<_> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|actor| !before.contains(actor) && world.arena.is_thrown(*actor))
        .collect();
    assert_eq!(thrown.len(), 1);
    let grenade = thrown[0];
    assert_eq!(world.arena.actor(grenade).unwrap().word_c4, 3);
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        zone,
        &mut rng,
    )
    .unwrap();
    assert_eq!(world.arena.actor(grenade).unwrap().word_c4, 1);
    assert_eq!(world.arena.grenade26(grenade).unwrap().word_13a, 5000);
    assert_eq!(world.arena.actor(zone).unwrap().word_c4, 3);
}

/// The link's message 8 leaves the zone linked with none: its next update
/// retires it, and its shared +AC (00404600) disposes of it.
#[test]
fn an_unlinked_zone_retires_itself() {
    let (resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut owners = Owners::new(executable);
    let zone = zone(&world);
    let mut rng = 0x1234_5678;
    {
        let mut owners = WorldDamageOwners {
            grant_text: None,
            camera_settings: None,
            flash: None,
            mission: &mut world.mission,
            tasks: &mut owners.tasks,
            terrain: &mut world.terrain,
            particles: &mut owners.particles,
            sounds: &mut owners.sounds,
            furniture: &mut owners.furniture,
            residency: &resident,
            executable,
            factory_process: &mut owners.process,
            behavior_tables: &mut owners.tables,
            animation_clock: &owners.clock,
            session: &owners.state,
            particle_load_factor: 0x10000,
            behavior_factory: None,
            map: None,
        };
        super::super::deliver_world_actor_message(
            &mut world.arena,
            zone,
            ActorMessage {
                sender: None,
                distance_squared: 0,
                code: 8,
                argument: 0,
            },
            &mut owners,
            &mut rng,
        )
        .unwrap();
    }
    let state = world.arena.selector74(zone).unwrap();
    assert_eq!((state.linked_148, state.links_14c), (1, 0));
    tick(
        &mut world,
        &resident,
        executable,
        &mut owners,
        &sine,
        zone,
        &mut rng,
    )
    .unwrap();
    assert_eq!(world.arena.actor(zone).unwrap().word_c4, 1);
    assert!(super::super::weapon_test_support::disposed(
        &world.arena,
        zone
    ));
}

#[derive(serde::Deserialize)]
struct RecordedDraw {
    recording: String,
    at: String,
    timer: u32,
    point: [u16; 3],
}

#[derive(serde::Deserialize)]
struct RecordedDraws {
    draws: Vec<RecordedDraw>,
}

/// Every draw Infest03 (10) and Infest04 (101) recorded (`zone-life.js`
/// `draws()`): from the recorded timer, Helniazine's zone puts its one point
/// where retail's did, on the world's own ground plus 0x32.
#[test]
fn the_zone_draws_where_infest03_and_infest04_did() {
    let recorded: RecordedDraws = serde_json::from_str(include_str!(
        "../../../tests/fixtures/zone_draws_infest.json"
    ))
    .unwrap();
    assert_eq!(recorded.draws.len(), 111);
    let (_resident, assets, mut world) = crate::test_worlds::offline_world_retained(
        6,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let sine = assets.sine_table().values;
    let zone = zone(&world);
    for draw in &recorded.draws {
        world
            .arena
            .selector74_set_timer_for_tests(zone, draw.timer)
            .unwrap();
        let points: Vec<_> = world
            .arena
            .selector74_draw_points(zone, &sine)
            .unwrap()
            .into_iter()
            .map(|[x, z]| {
                [
                    x,
                    crate::object_runtime::selector74_point_height(world.terrain.height_at([x, z])),
                    z,
                ]
            })
            .collect();
        assert_eq!(points, [draw.point], "{} {}", draw.recording, draw.at);
    }
}
