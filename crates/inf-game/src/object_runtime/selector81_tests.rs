//! World 19's spawner against 00409250, 00409320, 00409360 and 00409390,
//! with values worked by hand.

use inf_formats::{ByteSpan, InstanceCommandPayload, InstanceField};

use super::*;
use crate::object_runtime::actor::CaptureNodeCounts;
use crate::object_runtime::{
    ActorAllocationPolicy, ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, ObjectClassKind, ObjectList,
};
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};

const SINE: [u16; 4096] = [0; 4096];
/// Every entry at a quarter turn's full value, so each offset is half the
/// range, signed by the turn's top bit.
const FULL: [u16; 4096] = [0x4000; 4096];
const UNIT: [u16; 3] = [10_000, 0, 10_000];
const DELTA: u32 = 33_333;

fn arena(policy: ActorAllocationPolicy) -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    arena.select_actor_allocation_policy(policy).unwrap();
    arena.capture_nodes = CaptureNodeCounts {
        aliens_547620: 1,
        others_547624: 0,
    };
    arena
}

fn request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some(UNIT),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

/// The first player's unit at [`UNIT`], upright.
fn unit(arena: &mut ObjectArena) -> ObjectId {
    let unit = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: request(),
                world_mask_limits: [3, 3],
            },
            &mut 0,
        )
        .unwrap();
    arena.place_for_tests(unit, UNIT);
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(unit).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    unit
}

/// A spawner on the unit, slow for five seconds, its list built.
fn spawner(arena: &mut ObjectArena) -> ObjectId {
    let spawner = arena.allocate(ObjectClassKind::Selector81);
    arena.place_for_tests(spawner, UNIT);
    arena.finish_selector81(spawner).unwrap();
    arena.selector81_mut(spawner).unwrap().still_50 = 5_000_001;
    spawner
}

/// Ground at `height` under a liquid at 64.
fn terrain(height: u8) -> TerrainRuntime {
    TerrainRuntime::from_raw_cells(
        TerrainSampling {
            spacing: TerrainSpacing::Units128,
            height_scale: TerrainHeightScale::Units32,
        },
        64 << 8,
        std::iter::repeat(u16::from(height)).take(512 * 512),
    )
}

struct Host {
    terrain: TerrainRuntime,
    spawns: Vec<[u16; 3]>,
}

impl Selector81Host for Host {
    type Error = String;
    fn terrain(&self) -> &TerrainRuntime {
        &self.terrain
    }
    fn spawn(
        &mut self,
        arena: &mut ObjectArena,
        position: [u16; 3],
        _: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.spawns.push(position);
        Ok(Some(arena.allocate_actor(request()).unwrap()))
    }
}

fn host(height: u8) -> Host {
    Host {
        terrain: terrain(height),
        spawns: Vec::new(),
    }
}

/// The three draws of one attempt: the bearing's percentage bucket, the
/// turn within it and the range.
fn attempt(rng: &mut u32) -> (u32, i32) {
    let percent = ((name_entry_rng_next(rng) & 0xffff) * 100) >> 16;
    let bucket = [15, 35, 45, 50, 55, 65, 85, 101]
        .iter()
        .position(|&limit| percent <= limit)
        .unwrap() as u32;
    let angle = ((name_entry_rng_next(rng) & 0xffff) >> 3) + (bucket << 13);
    let range = ((name_entry_rng_next(rng) & 0xffff) >> 7) as i32 + 500;
    (angle, range)
}

/// 00409250 and 00409360: five members, 1000, 20000; the list unwritten.
#[test]
fn construction_keeps_five_members_and_commands_set_them() {
    for (policy, member) in [
        (ActorAllocationPolicy::Unowned, None),
        (
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
            Some(0),
        ),
    ] {
        let mut arena = arena(policy);
        let spawner = arena.allocate(ObjectClassKind::Selector81);
        let state = arena.selector81(spawner).unwrap();
        assert_eq!(
            (
                state.timer_40,
                state.count_44,
                state.distance_48,
                state.speed_4c
            ),
            (0, 5, 1000, 20_000)
        );
        let dword = |value| InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value,
        };
        assert!(arena
            .apply_selector81_command(
                spawner,
                &InstanceCommandPayload::Selector81Count44(dword(3))
            )
            .unwrap());
        assert!(arena
            .apply_selector81_command(
                spawner,
                &InstanceCommandPayload::Selector81Distance48(dword(700))
            )
            .unwrap());
        assert!(!arena
            .apply_selector81_command(spawner, &InstanceCommandPayload::NoPayloadRead)
            .unwrap());
        arena.finish_selector81(spawner).unwrap();
        let state = arena.selector81(spawner).unwrap();
        assert_eq!((state.count_44, state.distance_48), (3, 700));
        assert_eq!(state.members_58, vec![member; 3]);
    }
}

/// 004093CC..0040943C: past +48 the spawner moves onto the unit and +4C
/// is its speed, (2000 << 10) / (1000000 >> 10) = 2098.
#[test]
fn the_spawner_follows_a_moving_unit_and_measures_its_speed() {
    let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
    let unit = unit(&mut arena);
    let spawner = spawner(&mut arena);
    let state = arena.selector81_mut(spawner).unwrap();
    state.still_50 = 1_000_000 - DELTA;
    state.timer_40 = 10_000_000;
    arena.place_for_tests(unit, [12_000, 0, 10_000]);
    let mut rng = 1;
    arena
        .tick_selector81(spawner, DELTA, &SINE, &mut rng, &mut host(0))
        .unwrap();
    let state = arena.selector81(spawner).unwrap();
    assert_eq!((state.speed_4c, state.still_50), (2098, 0));
    assert_eq!(state.timer_40, 10_000_000 - DELTA);
    assert_eq!(
        arena.object(spawner).unwrap().base.position_words,
        Some([12_000, 0, 10_000])
    );
    assert_eq!(rng, 1, "no attempt while the timer runs");
}

/// 0040945D..00409850: a slow unit's last missing member rises half the
/// range out along the drawn bearing, and the next attempt waits
/// ((draw & 0xFFFF) + 31250) << 5.
#[test]
fn a_slow_unit_gets_a_member_round_it() {
    let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
    unit(&mut arena);
    let spawner = spawner(&mut arena);
    let mut rng = 0x1234_5678;
    let mut host = host(0);
    arena
        .tick_selector81(spawner, DELTA, &FULL, &mut rng, &mut host)
        .unwrap();
    let mut expected = 0x1234_5678;
    let (angle, range) = attempt(&mut expected);
    let half = |turn: u32| {
        let sign = if turn & 0x8000 != 0 { -1 } else { 1 };
        ((sign * 0x4000 * range) >> 15) as u16
    };
    let point = [
        UNIT[0].wrapping_add(half(angle + 0x4000)),
        0,
        UNIT[2].wrapping_add(half(angle)),
    ];
    assert_eq!(host.spawns, [point]);
    let timer = ((name_entry_rng_next(&mut expected) & 0xffff) + 31_250) << 5;
    assert_eq!(rng, expected);
    let state = arena.selector81(spawner).unwrap();
    assert_eq!(state.timer_40, timer);
    assert!(state.members_58[4].is_some_and(|handle| handle != 0));
    assert_eq!(state.members_58[..4], [Some(0); 4]);
}

/// 004095B7: the first attempt's Y is SPAWNER_FIRST_HEIGHT_APPROXIMATION,
/// 0, so a tilted unit's first attempt lands where an upright unit's does.
#[test]
fn a_tilted_units_first_attempt_starts_from_height_zero() {
    let run = |tilt: f32| {
        let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
        let unit = unit(&mut arena);
        arena.actor_mut(unit).unwrap().rotating.basis.words[4] = tilt.to_bits();
        let spawner = spawner(&mut arena);
        let mut rng = 0x1234_5678;
        let mut host = host(0);
        arena
            .tick_selector81(spawner, DELTA, &FULL, &mut rng, &mut host)
            .unwrap();
        (rng, host.spawns)
    };
    assert_eq!(run(0.01), run(0.0));
}

/// 004096F5: ground within 32 of the liquid skips the attempt; only a
/// delta whose low byte is zero leaves the retry flag clear, for up to
/// eight attempts.
#[test]
fn ground_near_the_liquid_skips_and_retries_only_on_a_round_delta() {
    for (delta, attempts) in [(DELTA, 1), (0x8200, 8)] {
        let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
        unit(&mut arena);
        let spawner = spawner(&mut arena);
        let mut rng = 7;
        // 1 << 5 = 32 is not below 64 - 32.
        let mut host = host(1);
        arena
            .tick_selector81(spawner, delta, &FULL, &mut rng, &mut host)
            .unwrap();
        let mut expected = 7;
        for _ in 0..attempts {
            attempt(&mut expected);
        }
        assert_eq!(rng, expected, "{attempts} attempts");
        assert!(host.spawns.is_empty());
        assert_eq!(arena.selector81(spawner).unwrap().timer_40, 0);
    }
}

/// 00409852: with the aliens' nodes all taken the spawner disposes itself.
#[test]
fn with_the_aliens_nodes_taken_the_spawner_goes() {
    let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
    arena.capture_nodes.aliens_547620 = 0;
    unit(&mut arena);
    let spawner = spawner(&mut arena);
    let mut host = host(0);
    arena
        .tick_selector81(spawner, DELTA, &FULL, &mut 1, &mut host)
        .unwrap();
    assert!(host.spawns.is_empty());
    assert!(arena
        .list(ObjectList::Disposal)
        .iter()
        .any(|node| node.object == spawner));
}

/// 0040949D: the list's entries are read as handles; unwritten ones stop.
#[test]
fn an_unwritten_member_list_stops_the_attempt() {
    let mut arena = arena(ActorAllocationPolicy::Unowned);
    unit(&mut arena);
    let spawner = spawner(&mut arena);
    let result = arena.tick_selector81(spawner, DELTA, &FULL, &mut 1, &mut host(0));
    assert!(matches!(
        result,
        Err(Selector81Error::UnwrittenMember { index: 0, .. })
    ));
}
