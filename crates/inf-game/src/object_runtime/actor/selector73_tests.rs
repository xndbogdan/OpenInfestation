//! The ECM Unit against Infest03's and Infest04's Helniazine
//! (`ecm-receipts.js`): its updates, which keep it bouncing on the ground
//! at the recordings' 125 ms updates, and Infest03's glows.

use serde::Deserialize;

use super::*;
use crate::object_runtime::{ActorConstructorNull, ActorRegistryConfig, ActorTerrainInputs};

const UNUSED_SINE: [u16; 4096] = [0; 4096];

#[derive(Deserialize)]
struct Fields {
    flags: u32,
    position: [u16; 3],
    velocity: [u16; 3],
    position_138: [u16; 3],
    rest_140: u32,
    holder_144: u32,
}

#[derive(Deserialize)]
struct Update {
    recording: String,
    at: String,
    delta: u32,
    null: u32,
    entry: Fields,
    exit: Fields,
}

#[derive(Deserialize)]
struct Light {
    x: u32,
    z: u32,
    radius: u32,
    rgb: u32,
    phase: u32,
}

#[derive(Deserialize)]
struct Glow {
    at: String,
    phase: u32,
    world_delta: u32,
    position: [u16; 3],
    light: Light,
}

#[derive(Deserialize)]
struct Fixture {
    updates: Vec<Update>,
    glows: Vec<Glow>,
}

fn fixture() -> Fixture {
    serde_json::from_str(include_str!("../../../tests/fixtures/ecm_unit_infest.json")).unwrap()
}

fn unit(null: u32) -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(null),
    })
    .unwrap();
    let unit = arena
        .allocate_selector73(ActorConstructionRequest {
            constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
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
    (arena, unit)
}

fn observed(arena: &ObjectArena, unit: ObjectId) -> (u32, [u16; 3], [u16; 3], [u16; 3], u32, u32) {
    let base = &arena.object(unit).unwrap().base;
    let state = arena.selector73(unit).unwrap();
    (
        base.flags,
        base.position_words.unwrap(),
        arena.actor(unit).unwrap().words_8e_92,
        state.position_138.unwrap(),
        state.rest_140,
        state.holder_144.raw(),
    )
}

/// All 93 of Infest03's updates and 31 of Infest04's 574 (its first 16 and
/// those where the rest or the flags change). After it lands the unit never
/// settles at 125 ms: each update damps its rebound of 44 to 34 and gravity
/// takes 122, and the terrain contact sends it back up at 44.
#[test]
fn every_recorded_update_moves_the_unit_as_retail_did() {
    let fixture = fixture();
    assert_eq!(fixture.updates.len(), 124);
    let mut failures = Vec::new();
    for update in &fixture.updates {
        let (mut arena, unit) = unit(update.null);
        {
            let base = &mut arena.owned_mut(unit).unwrap().state.base;
            base.flags = update.entry.flags;
            base.position_words = Some(update.entry.position);
        }
        arena.actor_mut(unit).unwrap().words_8e_92 = update.entry.velocity;
        {
            let state = arena.selector73_mut(unit).unwrap();
            state.position_138 = Some(update.entry.position_138);
            state.rest_140 = update.entry.rest_140;
            state.holder_144 = ActorHandle::from_raw(update.entry.holder_144);
        }
        arena
            .tick_selector73(unit, update.delta, &UNUSED_SINE)
            .unwrap();
        let exit = &update.exit;
        let expected = (
            exit.flags,
            exit.position,
            exit.velocity,
            exit.position_138,
            exit.rest_140,
            exit.holder_144,
        );
        let port = observed(&arena, unit);
        if port != expected {
            failures.push(format!(
                "{} {}: port {port:x?}, recorded {expected:x?}",
                update.recording, update.at
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The constructor leaves `+138` unwritten; the first update finds the unit
/// moved, copies its position and runs its physics, as both recordings'
/// first updates (a 9 ms delta, residue `[-5383, 0, 16000]`) did.
#[test]
fn a_fresh_unit_copies_its_position_and_falls() {
    let (mut arena, unit) = unit(0);
    arena.owned_mut(unit).unwrap().state.base.position_words = Some([1000, 200, 3000]);
    assert_eq!(arena.selector73(unit).unwrap().position_138, None);
    arena.tick_selector73(unit, 9000, &UNUSED_SINE).unwrap();
    let state = arena.selector73(unit).unwrap();
    assert_eq!(state.position_138, Some([1000, 200, 3000]));
    assert_eq!(state.rest_140, 0);
    // Gravity's 9000 >> 10.
    assert_eq!(
        arena.actor(unit).unwrap().words_8e_92,
        [0, (-8i16) as u16, 0]
    );
}

/// Still for 500 ms, the unit sets flag 0x4000000 and stops its physics;
/// flag 0x400000 wakes it, clearing 0x200000 with them. Between updates the
/// ground puts it back where it was, as the terrain contact does.
#[test]
fn a_still_unit_settles_and_wakes() {
    const PLACE: [u16; 3] = [1000, 200, 3000];
    let (mut arena, unit) = unit(0);
    arena.selector73_mut(unit).unwrap().position_138 = Some(PLACE);
    arena.owned_mut(unit).unwrap().state.base.flags = 0x0800_0167;
    let ground = |arena: &mut ObjectArena| {
        arena.owned_mut(unit).unwrap().state.base.position_words = Some(PLACE);
        arena.actor_mut(unit).unwrap().words_8e_92 = [0; 3];
    };
    // Three 125 ms updates and one of 100 ms rest 475 ms, short of 500.
    for (delta, rest) in [
        (125_000, 125_000),
        (125_000, 250_000),
        (125_000, 375_000),
        (100_000, 475_000),
    ] {
        ground(&mut arena);
        arena.tick_selector73(unit, delta, &UNUSED_SINE).unwrap();
        assert_eq!(arena.selector73(unit).unwrap().rest_140, rest);
        assert_eq!(arena.object(unit).unwrap().base.flags & SETTLED_FLAG, 0);
        // Its physics still ran: gravity's delta >> 10.
        let fall = (delta >> 10) as u16;
        assert_eq!(
            arena.actor(unit).unwrap().words_8e_92[1],
            fall.wrapping_neg()
        );
    }
    // 25 ms more reaches 500: settled, with no physics.
    ground(&mut arena);
    arena.tick_selector73(unit, 25_000, &UNUSED_SINE).unwrap();
    assert_eq!(arena.selector73(unit).unwrap().rest_140, 500_000);
    assert_ne!(arena.object(unit).unwrap().base.flags & SETTLED_FLAG, 0);
    assert_eq!(arena.actor(unit).unwrap().words_8e_92, [0; 3]);
    arena.tick_selector73(unit, 125_000, &UNUSED_SINE).unwrap();
    assert_eq!(arena.actor(unit).unwrap().words_8e_92, [0; 3]);
    assert_eq!(arena.object(unit).unwrap().base.position_words, Some(PLACE));
    arena.owned_mut(unit).unwrap().state.base.flags |= WAKE_FLAG | 0x0020_0000;
    arena.tick_selector73(unit, 125_000, &UNUSED_SINE).unwrap();
    assert_eq!(arena.selector73(unit).unwrap().rest_140, 0);
    assert_eq!(
        arena.object(unit).unwrap().base.flags,
        0x0800_0167,
        "flags 0x4000000, 0x400000 and 0x200000 clear"
    );
    assert_eq!(arena.actor(unit).unwrap().words_8e_92[1], (-122i16) as u16);
}

/// A resting unit that moves forgets its rest and clears flags 0x4000000,
/// 0x400000 and 0x200000 before it damps and falls.
#[test]
fn a_moving_unit_forgets_its_rest() {
    let (mut arena, unit) = unit(0);
    arena.owned_mut(unit).unwrap().state.base.position_words = Some([1000, 200, 3000]);
    arena.owned_mut(unit).unwrap().state.base.flags =
        0x0800_0167 | SETTLED_FLAG | WAKE_FLAG | 0x0020_0000;
    let state = arena.selector73_mut(unit).unwrap();
    state.position_138 = Some([1000, 210, 3000]);
    state.rest_140 = 300_000;
    arena.tick_selector73(unit, 125_000, &UNUSED_SINE).unwrap();
    let state = arena.selector73(unit).unwrap();
    assert_eq!(state.rest_140, 0);
    assert_eq!(state.position_138, Some([1000, 200, 3000]));
    assert_eq!(arena.object(unit).unwrap().base.flags, 0x0800_0167);
}

fn sine_table() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

/// Infest03's 100 glows: the phase gains the world's delta, and the light
/// sits at the unit with radius 0x400 in the three waves' colour.
#[test]
fn every_recorded_glow_lights_what_retail_lit() {
    let fixture = fixture();
    assert_eq!(fixture.glows.len(), 100);
    let sine = sine_table();
    for glow in &fixture.glows {
        let (mut arena, unit) = unit(0);
        arena.owned_mut(unit).unwrap().state.base.position_words = Some(glow.position);
        arena.selector73_mut(unit).unwrap().phase_148 = glow.phase;
        let light = arena
            .selector73_glow(unit, glow.world_delta, &sine)
            .unwrap()
            .unwrap();
        let [r, g, b, _] = glow.light.rgb.to_le_bytes();
        assert_eq!(
            light,
            TerrainLightRequest {
                world_xz: [glow.light.x as i32, glow.light.z as i32],
                radius: glow.light.radius as i32,
                rgb: [r as i8, g as i8, b as i8],
            },
            "{}",
            glow.at
        );
        assert_eq!(
            arena.selector73(unit).unwrap().phase_148,
            glow.light.phase,
            "{}",
            glow.at
        );
    }
}
