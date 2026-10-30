//! The pickup spawner's update `004548E0` against Infest01's four Level 1
//! spawners (`spawner` query): each spawner's first update, and the 13
//! updates that drop a pickup, with the process RNG around each.

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, BaseContext,
    ObjectClassKind, TargetNullHandleSource,
};

const SINE: [u16; 4096] = [0; 4096];
/// The fixed step after the first frame, and the first frame's delta.
const STEP: u32 = 0x1e848;
const FIRST: u32 = 0x8ca0;

/// The four spawners in update-list order (`+78` 12..15): position and
/// `+138..+13B` at their first update, whose `+13C` is the constructor's 0.
const SPAWNERS: [([u16; 3], [u8; 4]); 4] = [
    ([0xc156, 0x40, 0x79e6], [2, 8, 6, 1]),
    ([0x4ac2, 0xffd0, 0x6418], [2, 5, 0x23, 1]),
    ([0x34c1, 0xffd0, 0x8b47], [2, 5, 0x2d, 1]),
    ([0x4b32, 0xffd0, 0x8aa9], [2, 0xa, 0x1e, 1]),
];

/// The 13 drops in recorded order: the spawner, the RNG at its update, the
/// position 0046D870 receives, the RNG after, and the spawner's `+13C` and
/// `+139` after.
const DROPS: [(usize, u32, [u16; 3], u32, u32, u8); 13] = [
    (
        0,
        0x05f5_3ae0,
        [0xc170, 0x40, 0x7bc0],
        0x33da_8c4a,
        0x5b_8d80,
        7,
    ),
    (
        0,
        0x707c_6346,
        [0xc12b, 0x40, 0x7b11],
        0x972b_7e20,
        0x5b_8d80,
        6,
    ),
    (
        0,
        0x715c_042d,
        [0xc268, 0x40, 0x7a90],
        0xf6aa_6cdf,
        0x5b_8d80,
        5,
    ),
    (
        0,
        0xb453_96ec,
        [0xc317, 0x40, 0x798b],
        0x5da5_0d36,
        0x5b_8d80,
        4,
    ),
    (
        0,
        0x62d8_8c11,
        [0xbf6a, 0x40, 0x7804],
        0x6c1e_6963,
        0x5b_8d80,
        3,
    ),
    (
        3,
        0x6c1e_6963,
        [0x4a41, 0xffd0, 0x8be5],
        0x7b3c_5005,
        0x1c9_c380,
        9,
    ),
    (
        1,
        0xb897_555c,
        [0x491e, 0xffd0, 0x62e5],
        0x0ccd_2926,
        0x216_0ec0,
        4,
    ),
    (
        0,
        0xcfaf_bc92,
        [0xbff8, 0x40, 0x79ab],
        0xc9c5_de4c,
        0x5b_8d80,
        2,
    ),
    (
        0,
        0x5b11_819e,
        [0xc170, 0x40, 0x78ec],
        0x0106_7038,
        0x5b_8d80,
        1,
    ),
    (
        2,
        0x0523_1032,
        [0x32f2, 0xffd0, 0x8be4],
        0xba9d_2aec,
        0x2ae_a540,
        4,
    ),
    (
        0,
        0xc86b_e028,
        [0xc1d1, 0x40, 0x7b42],
        0xc75c_d6d2,
        0x5b_8d80,
        0,
    ),
    (
        3,
        0x7ccf_863c,
        [0x4ac5, 0xffd0, 0x8c80],
        0x7bd7_3506,
        0x1c9_c380,
        8,
    ),
    (
        1,
        0xecae_a19e,
        [0x4a63, 0xffd0, 0x6290],
        0x9c78_9038,
        0x216_0ec0,
        3,
    ),
];

/// 0046D870's services over a fresh selector-89 pickup on flat ground.
#[derive(Default)]
struct Drops {
    positions: Vec<[u16; 3]>,
    pickups: Vec<ObjectId>,
}

impl TargetNullHandleSource for Drops {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
}

impl PickupBirthHost for Drops {
    type Error = ();
    fn create_pickup(
        &mut self,
        arena: &mut ObjectArena,
        resource: u32,
        position: &mut [u16; 3],
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, ()> {
        assert_eq!(resource, 9);
        self.positions.push(*position);
        let pickup = arena.allocate(ObjectClassKind::TimedPickup);
        arena
            .initialize_context(
                pickup,
                BaseContext {
                    position_words: Some(*position),
                    placement_index: u32::MAX,
                },
            )
            .unwrap();
        self.pickups.push(pickup);
        Ok(Some(pickup))
    }
    fn refresh_model(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), ()> {
        Ok(())
    }
    fn ground_height(&mut self, _: &mut ObjectArena, _: [u16; 2]) -> Result<u16, ()> {
        Ok(0)
    }
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn spawner(arena: &mut ObjectArena, position: [u16; 3], bytes: [u8; 4]) -> ObjectId {
    let spawner = arena
        .allocate_selector53(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: None,
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &SINE,
            },
        })
        .unwrap();
    arena.owned_mut(spawner).unwrap().state.base.position_words = Some(position);
    arena.set_flag1(spawner, true).unwrap();
    let state = arena.selector53_mut(spawner).unwrap();
    [
        state.kind_138,
        state.count_139,
        state.seconds_13a,
        state.repeat_13b,
    ] = bytes;
    spawner
}

fn updating(arena: &ObjectArena, spawner: ObjectId) -> bool {
    arena.object(spawner).unwrap().base.flags & 2 != 0
}

#[test]
fn the_constructor_sets_ten_kind_one_pickups_three_seconds_apart() {
    let mut arena = arena();
    let spawner = spawner(&mut arena, [0; 3], [1, 10, 3, 0]);
    assert_eq!(
        arena.selector53(spawner).unwrap(),
        &Selector53State {
            kind_138: 1,
            count_139: 10,
            seconds_13a: 3,
            repeat_13b: 0,
            timer_13c: 0,
        }
    );
}

/// Replays the four spawners frame by frame from their first update, as
/// the recording's 707 frames ran them: each drop matches its receipt, the
/// first spawner leaves the update list at its eighth (385 updates), and
/// the others update in all 707 frames.
#[test]
fn level1_spawners_drop_the_recorded_pickups() {
    let mut arena = arena();
    let spawners: Vec<_> = SPAWNERS
        .iter()
        .map(|&(position, bytes)| spawner(&mut arena, position, bytes))
        .collect();
    let mut host = Drops::default();
    let mut seen = 0;
    let mut updates = [0; 4];
    for frame in 0..707 {
        for (index, &spawner) in spawners.iter().enumerate() {
            if !updating(&arena, spawner) {
                continue;
            }
            updates[index] += 1;
            // Only a drop draws the RNG; seed it with the next receipt's.
            let mut rng = DROPS.get(seen).map_or(0, |drop| drop.1);
            let before = rng;
            let delta = if frame == 0 { FIRST } else { STEP };
            arena
                .tick_selector53(spawner, delta, &mut host, &mut rng)
                .unwrap();
            if host.positions.len() == seen {
                assert_eq!(
                    rng, before,
                    "frame {frame} spawner {index} drew without a drop"
                );
                continue;
            }
            let (owner, _, position, after, timer, count) = DROPS[seen];
            assert_eq!(index, owner, "frame {frame}");
            assert_eq!(host.positions.last(), Some(&position), "frame {frame}");
            assert_eq!(rng, after, "frame {frame}");
            let state = arena.selector53(spawner).unwrap();
            assert_eq!((state.timer_13c, state.count_139), (timer, count));
            let pickup = arena.timed_pickup(*host.pickups.last().unwrap()).unwrap();
            assert_eq!((pickup.kind_44, pickup.value_46), (Some(15), 1));
            seen += 1;
        }
    }
    assert_eq!(seen, DROPS.len());
    assert_eq!(updates, [385, 707, 707, 707]);
}

#[test]
fn the_timer_count_and_repeat_flag_follow_the_signed_bytes() {
    let mut arena = arena();
    let mut host = Drops::default();
    let mut rng = 0;
    // No repeat: the first update leaves the update list at once.
    let once = spawner(&mut arena, [0x100, 0, 0x100], [1, 2, 3, 0]);
    arena
        .tick_selector53(once, STEP, &mut host, &mut rng)
        .unwrap();
    assert!(!updating(&arena, once));
    assert_eq!(arena.selector53(once).unwrap().timer_13c, 0);
    // A due timer without repeat drops, then leaves the timer at zero.
    arena.set_flag1(once, true).unwrap();
    arena.selector53_mut(once).unwrap().timer_13c = STEP;
    arena
        .tick_selector53(once, STEP, &mut host, &mut rng)
        .unwrap();
    assert_eq!(host.positions.len(), 1);
    let state = arena.selector53(once).unwrap();
    assert_eq!((state.timer_13c, state.count_139), (0, 1));
    assert!(updating(&arena, once));
    let pickup = arena.timed_pickup(host.pickups[0]).unwrap();
    assert_eq!(pickup.kind_44, Some(14));
    // Every byte is signed: kind -1, count -3, -2 s. A timer above the
    // delta, compared signed, only counts down.
    let repeat = spawner(&mut arena, [0; 3], [0xff, 0xfd, 0xfe, 1]);
    arena.selector53_mut(repeat).unwrap().timer_13c = STEP + 1;
    arena
        .tick_selector53(repeat, STEP, &mut host, &mut rng)
        .unwrap();
    assert_eq!(arena.selector53(repeat).unwrap().timer_13c, 1);
    // A negative count takes itself: the spawner drops kind 12 anyway,
    // empties the count and leaves; the repeat restarts the timer at -2 s.
    arena
        .tick_selector53(repeat, STEP, &mut host, &mut rng)
        .unwrap();
    assert_eq!(host.positions.len(), 2);
    let pickup = arena.timed_pickup(host.pickups[1]).unwrap();
    assert_eq!(pickup.kind_44, Some(12));
    let state = arena.selector53(repeat).unwrap();
    assert_eq!(
        (state.timer_13c, state.count_139),
        ((-2_000_000_i32) as u32, 0)
    );
    assert!(!updating(&arena, repeat));
    // An empty count drops nothing but still restarts the timer.
    arena.set_flag1(repeat, true).unwrap();
    let before = rng;
    arena
        .tick_selector53(repeat, STEP, &mut host, &mut rng)
        .unwrap();
    assert_eq!((host.positions.len(), rng), (2, before));
    assert_eq!(
        arena.selector53(repeat).unwrap().timer_13c,
        (-2_000_000_i32) as u32
    );
    assert!(updating(&arena, repeat));
}

#[derive(serde::Deserialize)]
struct RigReceipts {
    mines: Vec<MineCall>,
}

#[derive(serde::Deserialize)]
struct MineCall {
    at: String,
    before: String,
    after: String,
    reply: u32,
}

/// The mine's fields `00454AB0` reads and writes: flags `+2C`, the kind,
/// count, seconds and repeat BYTEs `+138..+13B` and the timer `+13C`.
fn mine_fields(hex: &str) -> (u32, [u8; 4], u32) {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&hex[at..at + 2], 16).unwrap())
        .collect();
    let dword = |at: usize| u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
    (
        dword(0x2c),
        bytes[0x138..0x13c].try_into().unwrap(),
        dword(0x13c),
    )
}

/// The first twelve message-0xF calls Helniazine's three mines take from
/// the linked mining rigs in Infest04: an idle mine arms 3 or 12 seconds
/// and joins the update list, and every mine answers its kind.
#[test]
fn mine_calls_match_infest04() {
    let receipts: RigReceipts = serde_json::from_str(include_str!(
        "../../../tests/fixtures/mining_rig_infest04.json"
    ))
    .unwrap();
    assert_eq!(receipts.mines.len(), 12);
    let mut armed = 0;
    for call in &receipts.mines {
        let (flags, bytes, timer) = mine_fields(&call.before);
        let mut arena = arena();
        let mine = spawner(&mut arena, [0; 3], bytes);
        arena.set_flag1(mine, flags & 2 != 0).unwrap();
        let base = &mut arena.owned_mut(mine).unwrap().state.base;
        base.flags = flags;
        arena.selector53_mut(mine).unwrap().timer_13c = timer;
        let reply = arena.call_selector53(mine).unwrap();
        let (after_flags, after_bytes, after_timer) = mine_fields(&call.after);
        assert_eq!(reply, call.reply, "{}", call.at);
        assert_eq!(
            arena.object(mine).unwrap().base.flags,
            after_flags,
            "{}",
            call.at
        );
        assert!(updating(&arena, mine), "{}", call.at);
        let state = arena.selector53(mine).unwrap();
        assert_eq!(
            [
                state.kind_138,
                state.count_139,
                state.seconds_13a,
                state.repeat_13b
            ],
            after_bytes,
            "{}",
            call.at
        );
        assert_eq!(state.timer_13c, after_timer, "{}", call.at);
        armed += usize::from(timer == 0);
    }
    assert_eq!(armed, 3);
}

/// 00454AB0 arms only an idle mine with pickups left, and answers only
/// while the mine is on the update list, with its signed kind.
#[test]
fn an_empty_or_resting_mine_neither_arms_nor_answers() {
    let mut arena = arena();
    // Empty and idle: no timer, no list, no answer.
    let empty = spawner(&mut arena, [0; 3], [2, 0, 3, 0]);
    arena.set_flag1(empty, false).unwrap();
    assert_eq!(arena.call_selector53(empty).unwrap(), 0);
    assert_eq!(arena.selector53(empty).unwrap().timer_13c, 0);
    assert!(!updating(&arena, empty));
    // Armed but off the list: the timer stands and the answer is 0.
    let resting = spawner(&mut arena, [0; 3], [2, 3, 3, 0]);
    arena.set_flag1(resting, false).unwrap();
    arena.selector53_mut(resting).unwrap().timer_13c = 5;
    assert_eq!(arena.call_selector53(resting).unwrap(), 0);
    assert_eq!(arena.selector53(resting).unwrap().timer_13c, 5);
    assert!(!updating(&arena, resting));
    // The kind is a signed BYTE; a negative count still arms.
    let negative = spawner(&mut arena, [0; 3], [0xff, 0xfe, 2, 0]);
    arena.set_flag1(negative, false).unwrap();
    assert_eq!(arena.call_selector53(negative).unwrap(), u32::MAX);
    assert_eq!(arena.selector53(negative).unwrap().timer_13c, 2_000_000);
    assert!(updating(&arena, negative));
}
