use serde_json::Value;

use super::*;
use crate::network::notices::NetworkGameHold;
use crate::object_runtime::Selector69Projected;
use crate::process_particles::factories::ParticleFactoryProcessState;

fn fixture() -> Value {
    serde_json::from_str(include_str!("../../tests/fixtures/net_race_native.json")).unwrap()
}

fn int(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

/// `[grid, rank, best, lap, total, handle, gate, next, distance, laps, key,
/// flags]`, as `check_net_race.py` lists a record.
fn record(value: &Value) -> RacerRecord {
    let v: Vec<i64> = value.as_array().unwrap().iter().map(int).collect();
    RacerRecord {
        grid_00: v[0] as u8,
        rank_01: v[1] as u8,
        best_02: v[2] as u16,
        lap_04: v[3] as u16,
        total_06: v[4] as u16,
        handle_08: v[5] as u32,
        gate_0c: v[6] as u32,
        next_10: v[7] as u32,
        distance_14: v[8] as i32,
        laps_18: v[9] as u32,
        key_1c: v[10] as u32 as i32,
        flags_20: v[11] as u8,
    }
}

fn race_of(case: &Value) -> RaceState {
    let mut state = RaceState::new();
    for (slot, value) in case["records"].as_array().unwrap().iter().enumerate() {
        state.records[slot] = record(value);
    }
    state.countdown_2d4 = int(&case["countdown"]) as u16;
    state
}

/// `check_net_race.py`: 00512521 with 0046F2D0 over seventeen keys, ties
/// and orders included.
#[test]
fn the_records_sort_as_00512521_sorts_them() {
    for case in fixture()["sorts"].as_array().unwrap() {
        let keys: Vec<i32> = case["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| int(k) as i32)
            .collect();
        let mut order: Vec<usize> = (0..RACERS).collect();
        native_qsort(&mut order, |a, b| keys[*b].wrapping_sub(keys[*a]));
        let expected: Vec<usize> = case["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| int(v) as usize)
            .collect();
        assert_eq!(order, expected, "{case}");
    }
}

/// `check_net_race.py`: message 42's encoder 0046F410 at every room that
/// cuts it short.
#[test]
fn message_42_encodes_as_0046f410() {
    for case in fixture()["encodes"].as_array().unwrap() {
        let state = race_of(case);
        let room = int(&case["room"]) as usize;
        let mut out = vec![0xcc; room];
        let length = encode_race_records(&state, &mut out);
        assert_eq!(i64::from(length), int(&case["length"]), "{case}");
        let expected = case["out"].as_str().unwrap();
        let written: String = out.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(written, expected, "{case}");
    }
}

/// `check_net_race.py`: 0046F5E0 over the host's frames and over noise.
#[test]
fn message_42_receives_as_0046f5e0() {
    for case in fixture()["receives"].as_array().unwrap() {
        let mut state = race_of(case);
        let payload: Vec<u8> = (0..case["payload"].as_str().unwrap().len() / 2)
            .map(|k| {
                u8::from_str_radix(&case["payload"].as_str().unwrap()[2 * k..2 * k + 2], 16)
                    .unwrap()
            })
            .collect();
        let used = receive_race_records(&mut state, &payload).unwrap();
        assert_eq!(i64::from(used), int(&case["used"]), "{case}");
        for (slot, value) in case["after"].as_array().unwrap().iter().enumerate() {
            assert_eq!(state.records[slot], record(value), "slot {slot} of {case}");
        }
        assert_eq!(
            i64::from(state.countdown_2d4),
            int(&case["countdown_after"])
        );
    }
}

/// `check_net_race.py`: the host's restart key (0046F3F0 then 0046F390).
#[test]
fn the_restart_key_runs_as_0046f3f0_and_0046f390() {
    for case in fixture()["restarts"].as_array().unwrap() {
        let mut state = race_of(case);
        state.wait_354 = int(&case["wait"]) as i32;
        if case["stop"].as_bool().unwrap() {
            state.stop();
        }
        state.restart();
        let flags: Vec<i64> = state
            .records
            .iter()
            .map(|r| i64::from(r.flags_20))
            .collect();
        let expected: Vec<i64> = case["flags"].as_array().unwrap().iter().map(int).collect();
        assert_eq!(flags, expected, "{case}");
        assert_eq!(
            i64::from(state.countdown_2d4),
            int(&case["countdown_after"]),
            "{case}"
        );
        assert_eq!(
            i64::from(state.wait_354),
            int(&case["wait_after"]),
            "{case}"
        );
    }
}

/// `check_net_race.py`: a vehicle's claim 0046F760.
#[test]
fn a_vehicle_claims_its_record_as_0046f760() {
    for case in fixture()["claims"].as_array().unwrap() {
        let mut state = race_of(case);
        let handle = int(&case["handle"]) as u32;
        state
            .claim(
                handle,
                int(&case["lifecycle"]) as u32,
                int(&case["null"]) as u32,
            )
            .unwrap();
        let record = state.records[(handle >> 27) as usize];
        assert_eq!(
            i64::from(record.handle_08),
            int(&case["slot_handle"]),
            "{case}"
        );
        assert_eq!(
            i64::from(record.flags_20),
            int(&case["slot_flags"]),
            "{case}"
        );
    }
}

/// Jet Race 1 (arena 8: world 28, mode 3), as the host (slot 1) or a guest
/// (slot 2).
fn jet_race(host: bool) -> (crate::ResidentRuntime, crate::world_backend::WorldSession) {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 3;
    settings[0x1c / 4] = 8;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: u32::from(host),
        local_slot: if host { 1 } else { 2 },
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (resident, _assets, world) = crate::test_worlds::network_world(28, process);
    (resident, world)
}

fn request(sine: &[u16; 4096], host: bool, delta: u32, null: u32) -> RaceRequest<'_> {
    RaceRequest {
        delta,
        local_slot: if host { 1 } else { 2 },
        host,
        sine,
        null,
    }
}

fn sine_table() -> [u16; 4096] {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    crate::frontend_assets::FrontendAssets::open(&root)
        .unwrap()
        .sine_table()
        .values
}

/// Every selector 71 on the world's lists.
fn wrong_way_markers(world: &crate::world_backend::WorldSession) -> Vec<ObjectId> {
    use crate::object_runtime::ObjectList;
    let mut found = Vec::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ] {
        for node in world.arena.list(list) {
            let marker = matches!(
                world.arena.race_object(node.object).map(|race| &race.part),
                Ok(RacePart::WrongWay(_))
            );
            if marker && !found.contains(&node.object) {
                found.push(node.object);
            }
        }
    }
    found
}

fn null(world: &crate::world_backend::WorldSession) -> u32 {
    let unit = world.arena.first_player().unwrap();
    world.arena.world_player(unit).unwrap().handle_d8.raw()
}

fn lend(world: &mut crate::world_backend::WorldSession) {
    world
        .arena
        .lend_network_game(Box::new(NetworkGameHold::default()));
}

#[test]
fn jet_race_builds_its_race_its_gates_and_its_grid() {
    let (_resident, world) = jet_race(true);
    let race = world.arena.race().expect("0054ED70");
    let state = world.arena.race_state(race).unwrap();
    assert_eq!(state.laps_338, 3);
    assert_eq!(state.countdown_2d4, IDLE);
    for (slot, record) in state.records.iter().enumerate().skip(1) {
        assert_eq!(usize::from(record.grid_00), slot - 1);
    }
    let places: std::collections::BTreeSet<_> = state.grid_2d6.iter().collect();
    assert_eq!(places.len(), 16, "sixteen distinct grid places");
    let gates = world.arena.race_gates();
    assert!(!gates.is_empty());
    let numbers: std::collections::BTreeSet<u32> = gates
        .iter()
        .map(|&gate| world.arena.gate_state(gate).unwrap().number_6c)
        .collect();
    assert!(numbers.contains(&1), "the line is gate 1: {numbers:?}");
    for &gate in gates {
        let state = world.arena.gate_state(gate).unwrap();
        assert!(
            numbers.contains(&state.next_70),
            "gate {} leads to {}",
            state.number_6c,
            state.next_70
        );
        assert!(state.half_width_74 > 0 && state.depth_78 > 0);
    }
}

/// 0046F390 then 0046E880 on the host: an idle race counts down three
/// seconds; under 0x186 every other racer races, at 0 this machine's; a
/// record whose vehicle is gone stops racing at once.
#[test]
fn the_host_counts_down_then_lets_the_racers_go() {
    let (_resident, mut world) = jet_race(true);
    let race = world.arena.race().unwrap();
    let sine = sine_table();
    let null = null(&world);
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let handle = world.arena.actor(vehicle).unwrap().handle.raw();
    {
        let state = world.arena.race_state_mut(race).unwrap();
        state.records[1].handle_08 = handle;
        state.records[2].handle_08 = handle;
        state.restart();
    }
    assert_eq!(
        world.arena.race_state(race).unwrap().countdown_2d4,
        COUNTDOWN
    );
    lend(&mut world);
    let mut rng = 1;
    // 0x2DC6 - 0x2C80 leaves 0x146, under 0x186.
    world
        .arena
        .tick_race(race, &request(&sine, true, 0x2c80 << 8, null), &mut rng)
        .unwrap();
    let state = world.arena.race_state(race).unwrap();
    assert_eq!(state.countdown_2d4, 0x146);
    assert_eq!(state.records[1].flags_20 & 1, 0, "the host's own waits");
    for (slot, record) in state.records.iter().enumerate().skip(2) {
        let racing = u8::from(slot == 2);
        assert_eq!(
            (record.flags_20 & 1, record.next_10, record.laps_18),
            (racing, 1, 0)
        );
    }
    world
        .arena
        .tick_race(race, &request(&sine, true, 0x146 << 8, null), &mut rng)
        .unwrap();
    let state = world.arena.race_state(race).unwrap();
    assert_eq!(state.countdown_2d4, 0);
    assert_eq!(state.records[1].flags_20 & 1, 1);
}

/// 0046ECFB: this machine's vehicle, not racing, is held on its grid place
/// (swaying by at most 16 + 8 + 4), turned as the race and stopped.
#[test]
fn a_waiting_racer_is_held_on_its_grid_place() {
    let (_resident, mut world) = jet_race(true);
    let race = world.arena.race().unwrap();
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    world.arena.actor_mut(vehicle).unwrap().words_8e_92 = [100, 200, 300];
    let sine = sine_table();
    let null = null(&world);
    lend(&mut world);
    let mut rng = 7;
    world
        .arena
        .tick_race(race, &request(&sine, true, 16_000, null), &mut rng)
        .unwrap();
    let state = world.arena.race_state(race).unwrap();
    let place = state.grid_2d6[usize::from(state.records[1].grid_00)];
    let position = world.arena.position(vehicle).unwrap();
    for axis in 0..3 {
        let off = position[axis].wrapping_sub(place[axis]) as i16;
        assert!(off.abs() <= 28, "axis {axis}: {off}");
    }
    let actor = world.arena.actor(vehicle).unwrap();
    assert_eq!(actor.words_8e_92, [0; 3]);
    assert_eq!(
        actor.rotating.basis,
        world.arena.race_object(race).unwrap().rotating.basis
    );
    assert!(state.sway_340.is_some());
}

/// A gate's next point inside its box: a quarter of its depth along its
/// facing.
fn inside(world: &crate::world_backend::WorldSession, gate: ObjectId) -> [u16; 3] {
    let depth = world.arena.gate_state(gate).unwrap().depth_78;
    let basis = world.arena.race_object(gate).unwrap().rotating.basis;
    let along = basis.transform_words([0, 0, (depth / 4) as i16]);
    let position = world.arena.position(gate).unwrap();
    std::array::from_fn(|axis| position[axis].wrapping_add(along[axis]))
}

/// 0046EA25..0046EBAF: a racer inside its next gate passes it; through
/// the line (gate 1) it counts a lap; inside another gate than its next it
/// is through the wrong gate.
#[test]
fn racers_pass_their_gates_and_count_laps() {
    let (_resident, mut world) = jet_race(true);
    let race = world.arena.race().unwrap();
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let handle = world.arena.actor(vehicle).unwrap().handle.raw();
    let gates = world.arena.race_gates().to_vec();
    let line = *gates
        .iter()
        .find(|&&gate| world.arena.gate_state(gate).unwrap().number_6c == 1)
        .unwrap();
    let after = world.arena.gate_state(line).unwrap().next_70;
    {
        let state = world.arena.race_state_mut(race).unwrap();
        state.countdown_2d4 = 0;
        // Another slot's record, so this machine's vehicle is not held.
        state.records[3] = RacerRecord {
            handle_08: handle,
            next_10: 1,
            laps_18: 1,
            lap_04: 40,
            flags_20: 1,
            ..RacerRecord::default()
        };
        state.records[1].flags_20 = 1;
        state.records[1].handle_08 = handle;
        state.records[1].next_10 = 99;
    }
    let point = inside(&world, line);
    world
        .arena
        .owned_mut(vehicle)
        .unwrap()
        .state
        .base
        .position_words = Some(point);
    let sine = sine_table();
    let null = null(&world);
    lend(&mut world);
    let mut rng = 3;
    world
        .arena
        .tick_race(race, &request(&sine, true, 100_000, null), &mut rng)
        .unwrap();
    let state = world.arena.race_state(race).unwrap();
    let record = state.records[3];
    assert_eq!((record.gate_0c, record.next_10), (1, after));
    assert_eq!((record.laps_18, record.best_02, record.lap_04), (2, 42, 0));
    assert_eq!(record.total_06, 2);
    assert_eq!(record.flags_20, 1);
    // Slot 1 expected gate 99: inside gate 1, whose next is not 99.
    assert_eq!(state.records[1].flags_20 & 2, 2);
    assert_eq!(
        record.key_1c,
        ((2 << 24) + (1 << 16) - 0x7fff + 0xffff) as i32
    );
}

/// 0046EC3C: while the race runs the host hands out places, each slot's
/// score 17 less its place, and grid places from the places before.
#[test]
fn the_host_ranks_the_racers_into_the_scores() {
    let (_resident, mut world) = jet_race(true);
    let race = world.arena.race().unwrap();
    {
        let state = world.arena.race_state_mut(race).unwrap();
        state.countdown_2d4 = 0;
        for (slot, record) in state.records.iter_mut().enumerate() {
            record.laps_18 = slot as u32 % 4;
            record.rank_01 = (16 - slot) as u8;
        }
    }
    let mut game = GameNetState::default();
    world.arena.rank_racers(race, &mut game).unwrap();
    let state = world.arena.race_state(race).unwrap();
    assert_eq!(state.records[0].rank_01, 16, "record 0 sorts last");
    let mut places: Vec<u8> = state.records.iter().map(|r| r.rank_01).collect();
    places.sort();
    assert_eq!(places, (0..17).collect::<Vec<u8>>());
    for (slot, record) in state.records.iter().enumerate() {
        assert_eq!(
            game.scores[slot][2],
            17 - u16::from(record.rank_01),
            "slot {slot}"
        );
        assert!(record.grid_00 <= 15);
    }
}

/// 0046F8C0: the first player's vehicle inside a wrong-way marker, facing
/// against it, is told so for a second.
#[test]
fn the_wrong_way_marker_tells_a_racer_facing_back() {
    let (_resident, mut world) = jet_race(true);
    let marker = *wrong_way_markers(&world)
        .first()
        .expect("a selector 71 in world 28");
    let vehicle = world.arena.first_player_primary_actor().unwrap().unwrap();
    let position = world.arena.position(marker).unwrap();
    world
        .arena
        .owned_mut(vehicle)
        .unwrap()
        .state
        .base
        .position_words = Some(position);
    let mut basis = world.arena.race_object(marker).unwrap().rotating.basis;
    for index in 7..10 {
        basis.words[index] ^= 0x8000_0000;
    }
    world.arena.actor_mut(vehicle).unwrap().rotating.basis = basis;
    let null = null(&world);
    world.arena.tick_wrong_way(marker, null).unwrap();
    let player = world.arena.first_player().unwrap();
    assert_eq!(
        world.arena.world_player(player).unwrap().dword_19c,
        1_000_000
    );
}

/// 0046FC59: eight posts, `t` and `-t` along the gate, 100 above it, `t`
/// scrolling with the race's clock.
#[test]
fn a_gates_posts_scroll_with_the_race_clock() {
    let (_resident, mut world) = jet_race(true);
    let gate = world.arena.race_gates()[0];
    let tops = world.arena.gate_post_tops(gate).unwrap();
    assert_eq!(tops.len(), 8);
    let position = world.arena.position(gate).unwrap();
    for top in &tops {
        let rise = top[1].wrapping_sub(position[1]) as i16;
        let state = world.arena.gate_state(gate).unwrap();
        assert!((rise - 100).abs() <= state.half_width_74 as i16, "{rise}");
    }
    world.arena.race_clock = world.arena.race_clock.wrapping_add(1 << 11);
    assert_ne!(world.arena.gate_post_tops(gate).unwrap(), tops);
}

/// 0046FD45..00470113: a post on screen, its foot deeper than 0x40,
/// becomes a quad widened by each end's size.
#[test]
fn a_post_becomes_a_quad_between_its_top_and_foot() {
    let top = Selector69Projected {
        xy: 400 | 50 << 16,
        depth: 0x100,
        clip: 1,
        fog: Some(0),
    };
    let foot = Selector69Projected {
        xy: 400 | 150 << 16,
        depth: 0x200,
        clip: 0,
        fog: Some(3),
    };
    let quad = gate_beam(top, foot, 0x100).unwrap().unwrap();
    // Sizes 0x96 (capped) and 0x4C; the step is straight down.
    assert!(quad.far);
    assert_eq!(quad.depth, 0x180);
    let xy = |x: u16, y: u16| u32::from(x) | u32::from(y) << 16;
    assert_eq!(
        quad.corners.map(|corner| corner.xy),
        [
            xy(400 - 0x96, 50),
            xy(400 - 0x4c, 150),
            xy(400 + 0x4c, 150),
            xy(400 + 0x96, 50)
        ]
    );
    assert_eq!(quad.fog, [0, 3, 3, 0]);
    // A foot at 0x40 or nearer draws nothing.
    let near = Selector69Projected {
        depth: 0x40,
        ..foot
    };
    assert_eq!(gate_beam(top, near, 0x100).unwrap(), None);
}

/// 0046F410 then 0046F5E0 over the world's race: a guest takes the host's
/// records.
#[test]
fn message_42_carries_the_hosts_records_to_a_guest() {
    let (_resident, mut host) = jet_race(true);
    let race = host.arena.race().unwrap();
    {
        let state = host.arena.race_state_mut(race).unwrap();
        state.countdown_2d4 = 0x100;
        state.records[2] = RacerRecord {
            rank_01: 3,
            grid_00: 5,
            best_02: 10,
            lap_04: 11,
            total_06: 12,
            laps_18: 2,
            next_10: 4,
            flags_20: 3,
            ..RacerRecord::default()
        };
    }
    let mut out = [0; 256];
    let length = host.arena.encode_race(&mut out).unwrap();
    assert_eq!(length, 24 + 8);
    let (_resident, mut guest) = jet_race(false);
    let used = guest.arena.receive_race(&out[..length as usize]).unwrap();
    assert_eq!(used, length);
    let race = guest.arena.race().unwrap();
    let state = guest.arena.race_state(race).unwrap();
    assert_eq!(state.countdown_2d4, 0x100);
    assert_eq!(
        state.records[2],
        RacerRecord {
            handle_08: state.records[2].handle_08,
            gate_0c: state.records[2].gate_0c,
            distance_14: state.records[2].distance_14,
            key_1c: state.records[2].key_1c,
            ..host
                .arena
                .race_state(host.arena.race().unwrap())
                .unwrap()
                .records[2]
        }
    );
}
