use super::*;
use crate::network::notices::NetworkGameHold;
use crate::process_particles::factories::ParticleFactoryProcessState;

// World 27's eight territories, each with a payload score of 100 (four
// times kept) and 20 seconds.
const TERRITORIES: usize = 8;
const SCORE: i32 = 400;
const PERIOD: u32 = 20_000_000;

/// Buggy Madness (arena 5: world 27, mode 2), as the host (slot 1) or a
/// guest (slot 2).
fn buggy_madness(host: bool) -> (crate::ResidentRuntime, crate::world_backend::WorldSession) {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    settings[0x1c / 4] = 5;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: u32::from(host),
        local_slot: if host { 1 } else { 2 },
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (resident, _assets, world) = crate::test_worlds::network_world(27, process);
    (resident, world)
}

/// Slots 1 and 2 in the game.
fn two_players() -> GameNetState {
    let mut game = GameNetState::default();
    game.present[1] = 1;
    game.present[2] = 1;
    game
}

fn request(resident: &crate::ResidentRuntime, host: bool) -> TerritoryRequest<'_> {
    TerritoryRequest {
        delta: 0,
        local_slot: if host { 1 } else { 2 },
        host,
        resident,
    }
}

#[test]
fn buggy_madness_numbers_its_territories_in_list_order() {
    let (_resident, world) = buggy_madness(true);
    let territories = world.arena.territories();
    assert_eq!(territories.len(), TERRITORIES);
    for (index, &territory) in territories.iter().enumerate() {
        let state = world.arena.selector46(territory).unwrap();
        assert_eq!(state.index_78, index as u32 + 1);
        assert_eq!((state.score_6c, state.period_70), (SCORE, PERIOD));
        assert_eq!((state.owner_90, state.charge_88), (0, 0));
    }
    assert_eq!(world.arena.territory_holders().unwrap(), [0; TERRITORIES]);
}

/// 00475750: an unheld territory gives its new holder half its score, a held
/// one a quarter, taken from the old holder; each change goes out as 41.
#[test]
fn a_capture_takes_half_from_no_one_and_a_quarter_from_the_holder() {
    let (resident, mut world) = buggy_madness(true);
    let territory = world.arena.territories()[0];
    let mut game = two_players();
    let request = request(&resident, true);
    world
        .arena
        .capture_territory(territory, 2, &request, &mut game)
        .unwrap();
    let half = (SCORE >> 1) as u16;
    assert_eq!(game.scores[2][2], half);
    assert_eq!(
        world.arena.take_network_sends(),
        [(41, GameSend::Territory4([2, 0, 0, half as u8]))]
    );
    world
        .arena
        .capture_territory(territory, 1, &request, &mut game)
        .unwrap();
    let quarter = (SCORE >> 2) as u16;
    assert_eq!(
        [game.scores[1][2], game.scores[2][2]],
        [quarter, half - quarter]
    );
    let state = world.arena.selector46(territory).unwrap();
    assert_eq!((state.owner_90, state.previous_8e), (1, 2));
    assert_eq!(
        world.arena.take_network_sends(),
        [(41, GameSend::Territory4([1, 2, 0, quarter as u8]))]
    );
    // The holder taking it again changes nothing.
    world
        .arena
        .capture_territory(territory, 1, &request, &mut game)
        .unwrap();
    assert!(world.arena.take_network_sends().is_empty());
}

/// 004759D8: past its period the host scores the holder, divided by how far
/// ahead it is (here 5 / 1 + 1), and frees the territory.
#[test]
fn the_host_scores_a_holder_each_period_then_frees_it() {
    let (resident, mut world) = buggy_madness(true);
    let territory = world.arena.territories()[0];
    {
        let state = world.arena.selector46_mut(territory).unwrap();
        state.owner_90 = 2;
        state.held_74 = PERIOD - 1;
    }
    world.arena.lend_network_game(Box::new(NetworkGameHold {
        game: two_players(),
        ..NetworkGameHold::default()
    }));
    let request = TerritoryRequest {
        delta: 32,
        ..request(&resident, true)
    };
    world.arena.tick_selector46(territory, &request).unwrap();
    let amount = SCORE / 6;
    let hold = world.arena.take_network_game().unwrap();
    assert_eq!(hold.game.scores[2][2], amount as u16);
    let state = world.arena.selector46(territory).unwrap();
    assert_eq!(
        (
            state.owner_90,
            state.held_74,
            state.scorer_8f,
            state.charge_88
        ),
        (0, 0, 2, 2)
    );
    assert_eq!(
        world.arena.take_network_sends(),
        [(41, GameSend::Territory4([2, 0, 2, amount as u8]))]
    );
}

/// Without two players a territory has no holder and its charge settles
/// towards half.
#[test]
fn a_lone_player_leaves_territories_unheld() {
    let (resident, mut world) = buggy_madness(true);
    let territory = world.arena.territories()[0];
    world.arena.selector46_mut(territory).unwrap().owner_90 = 2;
    let mut game = GameNetState::default();
    game.present[1] = 1;
    world.arena.lend_network_game(Box::new(NetworkGameHold {
        game,
        ..NetworkGameHold::default()
    }));
    let request = TerritoryRequest {
        delta: 0x100,
        ..request(&resident, true)
    };
    world.arena.tick_selector46(territory, &request).unwrap();
    let state = world.arena.selector46(territory).unwrap();
    assert_eq!((state.owner_90, state.charge_88), (0, 0x10));
    assert!(world.arena.take_network_sends().is_empty());
}

/// 00475BD0 runs only on the host: the claim captures it, keeps the slot at
/// +8D and tells everyone with 40; 00475C60 ends the claimer's +8C.
#[test]
fn the_host_grants_a_guests_claim() {
    let (resident, mut world) = buggy_madness(true);
    let territory = world.arena.territories()[1];
    let mut game = two_players();
    world
        .arena
        .receive_territory_claim([2, 2], &request(&resident, false), &mut game)
        .unwrap();
    assert_eq!(world.arena.selector46(territory).unwrap().owner_90, 0);
    world
        .arena
        .receive_territory_claim([2, 2], &request(&resident, true), &mut game)
        .unwrap();
    let state = world.arena.selector46(territory).unwrap();
    assert_eq!((state.owner_90, state.granted_8d), (2, 2));
    let half = (SCORE >> 1) as u8;
    assert_eq!(
        world.arena.take_network_sends(),
        [
            (41, GameSend::Territory4([2, 0, 0, half])),
            (40, GameSend::Territory2([2, 2])),
        ]
    );
    world.arena.selector46_mut(territory).unwrap().claim_8c = 2;
    world.arena.receive_territory_grant([2, 2], 1).unwrap();
    assert_eq!(world.arena.selector46(territory).unwrap().claim_8c, 2);
    world.arena.receive_territory_grant([2, 2], 2).unwrap();
    assert_eq!(world.arena.selector46(territory).unwrap().claim_8c, 0);
}

/// 00475B50: only a list as long as this machine's sets the holders; a
/// granted territory answers with its claim (39).
#[test]
fn the_holders_list_must_match_the_territories() {
    let (_resident, mut world) = buggy_madness(false);
    world
        .arena
        .receive_territory_holders(&[2; TERRITORIES + 1])
        .unwrap();
    assert_eq!(world.arena.territory_holders().unwrap(), [0; TERRITORIES]);
    let first = world.arena.territories()[0];
    {
        let state = world.arena.selector46_mut(first).unwrap();
        state.granted_8d = 2;
        state.claim_8c = 2;
    }
    world
        .arena
        .receive_territory_holders(&[2; TERRITORIES])
        .unwrap();
    assert_eq!(world.arena.territory_holders().unwrap(), [2; TERRITORIES]);
    assert_eq!(
        world.arena.take_network_sends(),
        [(39, GameSend::Territory2([1, 2]))]
    );
}

/// 0047590E..004759D8: the first player's vehicle within the extent marks
/// the side it is on (by the territory's local Z); on both, the host claims
/// it at once and a guest asks with 39. Territory 1 hangs 600 above the
/// floor: a buggy jumps through it.
#[test]
fn jumping_through_a_territory_claims_it() {
    for host in [true, false] {
        let (resident, mut world) = buggy_madness(host);
        let territory = world.arena.territories()[0];
        let centre = world
            .arena
            .object(territory)
            .unwrap()
            .base
            .position_words
            .unwrap();
        let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
        world.arena.lend_network_game(Box::new(NetworkGameHold {
            game: two_players(),
            ..NetworkGameHold::default()
        }));
        let request = request(&resident, host);
        let slot = request.local_slot;
        for (z, sides) in [(-100_i16, [slot, 0]), (100, [0, 0])] {
            let at = [centre[0], centre[1], centre[2].wrapping_add(z as u16)];
            world.arena.set_position_for_tests(vehicle, at);
            world.arena.tick_selector46(territory, &request).unwrap();
            assert_eq!(world.arena.selector46(territory).unwrap().sides_91, sides);
        }
        let state = *world.arena.selector46(territory).unwrap();
        let hold = world.arena.take_network_game().unwrap();
        let half = (SCORE >> 1) as u16;
        if host {
            assert_eq!((state.owner_90, hold.game.scores[1][2]), (1, half));
            assert_eq!(
                world.arena.take_network_sends(),
                [(41, GameSend::Territory4([1, 0, 0, half as u8]))]
            );
        } else {
            assert_eq!((state.owner_90, state.claim_8c), (0, 2));
            assert_eq!(hold.game.scores[2][2], 0);
            assert_eq!(
                world.arena.take_network_sends(),
                [(39, GameSend::Territory2([1, 2]))]
            );
        }
    }
}
