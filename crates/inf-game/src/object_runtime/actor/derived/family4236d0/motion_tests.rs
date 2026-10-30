use super::super::super::selector10_tail::tests::{assets, executable, fixture};
use super::*;
use crate::frontend_camera::AnimationClock;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs,
    DerivedActorConstructionRequest, Selector10MotionRequest,
};
use crate::terrain_runtime::{native_probe_fixture_terrain, TerrainRuntime};

struct Terrain {
    current: TerrainRuntime,
    calls: usize,
}

impl Terrain {
    fn new(plane: i32) -> Self {
        Self {
            current: native_probe_fixture_terrain(&serde_json::json!({
                "spacing":256,"height_shift":5,"plane_base":plane,"plane_shift":8,
                "xz":[384,640],"heights":[2,2,2,2],"alternate":false,
            })),
            calls: 0,
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
        Ok(&self.current)
    }
}

struct Header;

impl ActorContactHeaderHost for Header {
    type Error = &'static str;
    fn header(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<&[u8], Self::Error> {
        Ok(&[0; 5])
    }
}

/// The family's own class (selector 9) at the selector-10 fixture's place.
fn family_fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_family4236d0_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([384, 64, 640]),
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
            &mut 0,
        )
        .unwrap();
    arena.owned_mut(actor).unwrap().state.base.word_28 = 1024;
    (arena, actor)
}

fn family_visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    status: i32,
    delta: u32,
    controls: [i32; 2],
    terrain: &mut Terrain,
) -> Result<(), Family4236d0MotionError<&'static str, &'static str>> {
    arena.actor_mut(actor).unwrap().word_c4 = status as u32;
    let clock = AnimationClock::default();
    let plane = Terrain::new(0).current;
    arena.tick_family4236d0_motion(
        actor,
        Family4236d0MotionRequest {
            microseconds: delta,
            controls,
            executable: executable(),
            environment: ActorContactEnvironment {
                animation_clock: &clock,
                plane_policy: &plane,
                up: &[0.0, 1.0, 0.0],
                sine_quarter: &assets().sine_table().values,
            },
        },
        terrain,
        &mut Header,
    )
}

/// Selector 10's 00425400 with a mode outside 3/4 and no helper: the same
/// arms in the same order.
fn selector10_visit(
    arena: &mut ObjectArena,
    actor: ObjectId,
    status: i32,
    delta: u32,
    controls: [i32; 2],
    terrain: &mut Terrain,
) {
    arena.actor_mut(actor).unwrap().word_c4 = status as u32;
    let clock = AnimationClock::default();
    let plane = Terrain::new(0).current;
    let mut sounds = super::super::super::selector10_tail::tests::sound();
    arena
        .tick_selector10_motion(
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
                random_state: &mut 0,
                residency: &super::super::super::selector10_tail::tests::resident(),
                sounds: &mut sounds,
            },
            terrain,
            &mut Header,
        )
        .unwrap();
}

fn motion_state(arena: &ObjectArena, actor: ObjectId) -> impl PartialEq + std::fmt::Debug {
    let actor_state = arena.actor(actor).unwrap();
    (
        arena.object(actor).unwrap().base.position_words,
        actor_state.words_8e_92,
        actor_state.words_94_9c,
        actor_state.rotating.basis,
        arena.derived_actor(actor).unwrap().word_138,
    )
}

#[test]
fn the_family_motion_drives_as_selector_10s_without_its_mode_and_helper() {
    for status in [1, 3] {
        for (delta, controls) in [
            (32, [0, 0]),
            (65536, [31, 1000]),
            (125_000, [-500, -2000]),
            (125_000, [4000, 30000]),
        ] {
            for (flags, boost) in [(0, 0), (0x80000, 0), (0, 0x20)] {
                let (mut family, family_actor) = family_fixture();
                let (mut class10, class10_actor) = fixture();
                class10
                    .selector10_actor_mut(class10_actor)
                    .unwrap()
                    .word_268 = Some(1);
                class10.owned_mut(class10_actor).unwrap().state.base.word_28 = 1024;
                for (arena, actor) in [(&mut family, family_actor), (&mut class10, class10_actor)] {
                    arena.owned_mut(actor).unwrap().state.base.flags |= flags;
                    arena.family428650_actor_mut(actor).unwrap().word_22e |= boost;
                    arena.actor_mut(actor).unwrap().words_8e_92 = [300, (-100_i16) as u16, 20];
                }
                let mut terrain = Terrain::new(0);
                family_visit(
                    &mut family,
                    family_actor,
                    status,
                    delta,
                    controls,
                    &mut terrain,
                )
                .unwrap();
                let mut terrain = Terrain::new(0);
                selector10_visit(
                    &mut class10,
                    class10_actor,
                    status,
                    delta,
                    controls,
                    &mut terrain,
                );
                assert_eq!(
                    motion_state(&family, family_actor),
                    motion_state(&class10, class10_actor),
                    "status {status} delta {delta} controls {controls:?} flags {flags:#x} boost {boost:#x}"
                );
            }
        }
    }
}

#[test]
fn a_live_family_actor_without_contact_silences_its_engine_and_falls() {
    let (mut arena, actor) = family_fixture();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 30000, 640]);
    arena.derived_actor_mut(actor).unwrap().word_16c = Some(0xaaaa);
    arena.derived_actor_mut(actor).unwrap().word_170 = Some(0xaaaa);
    let mut terrain = Terrain::new(0);
    family_visit(&mut arena, actor, 3, 125_000, [0, 1000], &mut terrain).unwrap();
    let derived = arena.derived_actor(actor).unwrap();
    assert_eq!((derived.word_16c, derived.word_170), (Some(0), Some(0)));
    // Shared physics still ran: gravity moved the actor.
    assert_ne!(
        arena.object(actor).unwrap().base.position_words,
        Some([384, 30000, 640])
    );
    // A dying one keeps its scales.
    let (mut arena, actor) = family_fixture();
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 30000, 640]);
    arena.derived_actor_mut(actor).unwrap().word_16c = Some(0xaaaa);
    let mut terrain = Terrain::new(0);
    family_visit(&mut arena, actor, 1, 125_000, [0, 1000], &mut terrain).unwrap();
    assert_eq!(arena.derived_actor(actor).unwrap().word_16c, Some(0xaaaa));
}

#[test]
fn live_water_contact_keeps_the_native_stack_boundary_and_dying_bypasses_it() {
    let (mut arena, actor) = family_fixture();
    arena.derived_actor_mut(actor).unwrap().byte_160 = 1;
    // As selector 10's: size 1024 and Y 556 against plane 256.
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([384, 556, 640]);
    let mut terrain = Terrain::new(65536);
    assert!(matches!(
        family_visit(&mut arena, actor, 2, 32, [0, 0], &mut terrain),
        Err(Family4236d0MotionError::Contact(
            ActorContactError::RequiredNativeStackComparison
        ))
    ));
    family_visit(&mut arena, actor, 1, 32, [0, 0], &mut terrain).unwrap();
}

#[test]
fn only_the_family_class_runs_its_motion() {
    let (mut arena, actor) = fixture();
    let mut terrain = Terrain::new(0);
    assert!(matches!(
        family_visit(&mut arena, actor, 3, 32, [0, 0], &mut terrain),
        Err(Family4236d0MotionError::Derived(
            DerivedActorError::NotFamilyUpdate424830(found)
        )) if found == actor
    ));
    assert_eq!(terrain.calls, 0);
}

#[derive(serde::Deserialize)]
struct Receipts {
    updates: Vec<Update>,
}

#[derive(serde::Deserialize)]
struct Update {
    at: String,
    world: u32,
    clock: u32,
    coarse: bool,
    delta: u32,
    controls: [i32; 2],
    entry: Fields,
    exit: Fields,
}

/// The actor's bytes the motion reads or writes, by native offset.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
struct Fields {
    word_28: u16,
    flags: u32,
    position: [u16; 3],
    dirty: u16,
    basis: [u32; 10],
    velocity: [u16; 3],
    angular: [u32; 3],
    state: u32,
    word_138: u32,
    byte_160: u8,
    scales: [u32; 2],
    word_22e: u16,
    tables: [u32; 4],
}

struct FixedClock(u32);

impl crate::frontend_camera::AnimationClockSource for FixedClock {
    fn animation_value(&self) -> u32 {
        self.0
    }
}

struct WorldTerrain<'a>(&'a TerrainRuntime);

impl ActorTerrainHost for WorldTerrain<'_> {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        Ok(self.0)
    }
}

fn seed(arena: &mut ObjectArena, actor: ObjectId, fields: &Fields) {
    {
        let base = &mut arena.owned_mut(actor).unwrap().state.base;
        base.word_28 = fields.word_28;
        base.flags = fields.flags;
        base.position_words = Some(fields.position);
        base.dirty_word = fields.dirty;
    }
    let state = arena.actor_mut(actor).unwrap();
    state.rotating.basis.words = fields.basis;
    state.words_8e_92 = fields.velocity;
    state.words_94_9c = fields.angular;
    state.word_c4 = fields.state;
    let derived = arena.derived_actor_mut(actor).unwrap();
    derived.word_138 = Some(fields.word_138);
    derived.byte_160 = fields.byte_160;
    derived.word_16c = Some(fields.scales[0]);
    derived.word_170 = Some(fields.scales[1]);
    arena.family428650_actor_mut(actor).unwrap().word_22e = fields.word_22e;
    let family = arena.family4236d0_actor_mut(actor).unwrap();
    [
        family.table_250,
        family.table_254,
        family.table_258,
        family.table_25c,
    ] = fields.tables;
}

fn read(arena: &ObjectArena, actor: ObjectId) -> Fields {
    let base = &arena.object(actor).unwrap().base;
    let state = arena.actor(actor).unwrap();
    let derived = arena.derived_actor(actor).unwrap();
    let family = arena.family4236d0_actor(actor).unwrap();
    Fields {
        word_28: base.word_28,
        flags: base.flags,
        position: base.position_words.unwrap(),
        dirty: base.dirty_word,
        basis: state.rotating.basis.words,
        velocity: state.words_8e_92,
        angular: state.words_94_9c,
        state: state.word_c4,
        word_138: derived.word_138.unwrap(),
        byte_160: derived.byte_160,
        scales: [derived.word_16c.unwrap(), derived.word_170.unwrap()],
        word_22e: arena.family428650_actor(actor).unwrap().word_22e,
        tables: [
            family.table_250,
            family.table_254,
            family.table_258,
            family.table_25c,
        ],
    }
}

/// Sixteen calls of 00424CF0 that Infest04 made on trucks and assault craft
/// in Oolbryte (world 20), Tabuchi (9) and Helniazine (6), every 167th of
/// its 2,673, on coarse terrain: one dying, the rest driving. Each runs
/// from its captured entry on its loaded world, at the recorded animation
/// clock, and every field the motion writes matches.
#[test]
fn family_motion_matches_infest04_calls() {
    let receipts: Receipts = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/family4236d0_motion_infest04.json"
    ))
    .unwrap();
    let mut failures = Vec::new();
    let mut worlds = std::collections::BTreeMap::new();
    for update in &receipts.updates {
        assert!(update.coarse, "every recorded call ran on coarse terrain");
        let (runtime, assets, world) = worlds.entry(update.world).or_insert_with(|| {
            crate::test_worlds::offline_world_retained(
                update.world,
                crate::terrain_runtime::TerrainSpacing::Units256,
            )
        });
        let actor = world
            .arena
            .allocate_family4236d0_actor(
                DerivedActorConstructionRequest {
                    actor: ActorConstructionRequest {
                        constructor_null:
                            crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                        allocation_position: Some(update.entry.position),
                        owner: 1,
                        terrain: ActorTerrainInputs {
                            terrain: None,
                            coordinate_shift: 8,
                            clock: 0,
                            world_flags: None,
                            sine_quarter: &assets.sine_table().values,
                        },
                    },
                    world_mask_limits: [3; 2],
                },
                &mut 0,
            )
            .unwrap();
        seed(&mut world.arena, actor, &update.entry);
        let clock = FixedClock(update.clock);
        let result = world.arena.tick_family4236d0_motion(
            actor,
            Family4236d0MotionRequest {
                microseconds: update.delta,
                controls: update.controls,
                executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes())
                    .unwrap(),
                environment: ActorContactEnvironment {
                    animation_clock: &clock,
                    plane_policy: &world.terrain,
                    up: &[0.0, 1.0, 0.0],
                    sine_quarter: &assets.sine_table().values,
                },
            },
            &mut WorldTerrain(&world.terrain),
            &mut crate::world_actor_contact::WorldActorContactHeaderHost { residency: runtime },
        );
        match result {
            Ok(()) => {
                let actual = read(&world.arena, actor);
                if actual != update.exit {
                    failures.push(format!(
                        "{}: {actual:?}\n   != {:?}",
                        update.at, update.exit
                    ));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", update.at)),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert_eq!(receipts.updates.len(), 16);
    assert_eq!(worlds.keys().copied().collect::<Vec<_>>(), [6, 9, 20]);
    // The dying truck's spin damps and the live ones drive.
    let dying = &receipts.updates[1];
    assert_eq!(dying.entry.state, 1);
    assert_ne!(dying.entry.velocity, dying.exit.velocity);
    assert!(receipts.updates[2..]
        .iter()
        .all(|update| update.entry.state == 3));
}
