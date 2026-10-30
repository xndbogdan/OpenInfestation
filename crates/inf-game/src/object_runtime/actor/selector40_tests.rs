use super::*;
use crate::network::notices::NetworkGameHold;
use crate::process_particles::factories::ParticleFactoryProcessState;

/// Capture the Banner's world 33 as the host (slot 1). Mode 2 gives the
/// first player a vehicle at once, which mode 0 holds back until a team is
/// chosen.
fn banner_world() -> (crate::world_backend::WorldSession, NativeBasisMath) {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    settings[0x1c / 4] = 2;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, assets, mut world) = crate::test_worlds::network_world(33, process);
    // The link's nineteen team tasks (0055F100).
    let mut hold = NetworkGameHold::default();
    hold.game.teams = std::array::from_fn(|team| {
        Some(crate::network::game::TeamScore {
            index: team as u32,
            ..Default::default()
        })
    });
    world.arena.lend_network_game(Box::new(hold));
    (world, NativeBasisMath::new(assets.sine_table().values))
}

#[derive(Default)]
struct Announcements(Vec<(ObjectId, u8, ActorHandle)>);

impl BannerHost for Announcements {
    type Error = std::convert::Infallible;
    fn announce(
        &mut self,
        _: &mut ObjectArena,
        banner: ObjectId,
        event: u8,
        value: ActorHandle,
    ) -> Result<(), Self::Error> {
        self.0.push((banner, event, value));
        Ok(())
    }
}

fn of_class(arena: &ObjectArena, banner: bool) -> Vec<ObjectId> {
    arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            if banner {
                arena.selector40(actor).is_ok()
            } else {
                arena.is_selector41(actor)
            }
        })
        .collect()
}

/// The banner of `team` (+B4).
fn team_banner(arena: &ObjectArena, team: u32) -> ObjectId {
    of_class(arena, true)
        .into_iter()
        .find(|&banner| arena.actor(banner).unwrap().word_b4 == team)
        .unwrap()
}

fn vehicle(world: &mut crate::world_backend::WorldSession) -> ObjectId {
    world.arena.first_player_primary_actor().unwrap().unwrap()
}

/// The world's arena and the request over its terrain.
fn split<'a>(
    world: &'a mut crate::world_backend::WorldSession,
    math: &'a NativeBasisMath,
) -> (&'a mut ObjectArena, BannerRequest<'a>) {
    let null = world.arena.actor_handles().unwrap().null_handle();
    let request = BannerRequest {
        delta: 40_000,
        local_slot: 1,
        null,
        math,
        terrain: &world.terrain,
    };
    (&mut world.arena, request)
}

#[test]
fn capture_the_banner_builds_two_banners_and_two_bases_on_the_ground() {
    let (world, _) = banner_world();
    let banners = of_class(&world.arena, true);
    let bases = of_class(&world.arena, false);
    assert_eq!((banners.len(), bases.len()), (2, 2));
    let null = world.arena.actor_handles().unwrap().null_handle();
    for &banner in &banners {
        let state = world.arena.selector40(banner).unwrap();
        let at = world
            .arena
            .object(banner)
            .unwrap()
            .base
            .position_words
            .unwrap();
        assert_eq!(state.home_148, Some(at));
        assert_eq!(at[1], world.terrain.height_at([at[0], at[2]]) as u16);
        assert_eq!((state.carried_139, state.carrier_144), (0, null));
    }
    let teams: Vec<_> = banners
        .iter()
        .map(|&banner| world.arena.actor(banner).unwrap().word_b4)
        .collect();
    assert_eq!(teams, [3, 2]);
}

/// 0045B5A0: a vehicle of another team that reaches the banner takes it,
/// carries it, and keeps it away; dropped (+598 cleared) it falls where
/// the vehicle is and goes home 60 seconds later.
#[test]
fn a_vehicle_takes_a_banner_and_drops_it() {
    let (mut world, math) = banner_world();
    let banner = team_banner(&world.arena, 2);
    let vehicle = vehicle(&mut world);
    world.arena.actor_mut(vehicle).unwrap().word_b4 = 3;
    let home = world.arena.selector40(banner).unwrap().home_148.unwrap();
    world.arena.set_position_for_tests(vehicle, home);
    let mut host = Announcements::default();
    let (arena, request) = split(&mut world, &math);
    arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .update
        .handle_598 = Some(request.null);
    arena.tick_selector40(banner, &request, &mut host).unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    let banner_handle = arena.actor(banner).unwrap().handle;
    let state = arena.selector40(banner).unwrap();
    assert_eq!((state.carried_139, state.carrier_144), (1, handle));
    assert_eq!((state.away_138, state.away_150), (1, Some(60_000_000)));
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().update.handle_598,
        Some(banner_handle)
    );
    assert_eq!(host.0, [(banner, BANNER_TAKEN, handle)]);
    assert_eq!(
        arena.take_network_sends(),
        [(0xd, GameSend::Banner([banner_handle.raw(), handle.raw()]))]
    );
    // It rides with the vehicle.
    let moved = [home[0].wrapping_add(300), home[1], home[2]];
    arena.set_position_for_tests(vehicle, moved);
    arena.tick_selector40(banner, &request, &mut host).unwrap();
    assert_eq!(
        arena.object(banner).unwrap().base.position_words,
        Some(moved)
    );
    // Dropped (00417750: the vehicle waits a second before taking another):
    // grounded where it is, its position sent, no carrier.
    {
        let state = arena.selector2_actor_mut(vehicle).unwrap();
        state.update.handle_598 = Some(request.null);
        state.dwords_59c_5a0[0] = 1_000_000;
    }
    arena.tick_selector40(banner, &request, &mut host).unwrap();
    let state = arena.selector40(banner).unwrap();
    assert_eq!((state.carried_139, state.carrier_144), (0, request.null));
    assert_ne!(arena.object(banner).unwrap().base.dirty_word & 0x80, 0);
    assert_eq!(host.0[1], (banner, BANNER_DROPPED, handle));
    // Sixty seconds later it is home.
    arena.selector40_mut(banner).unwrap().away_150 = Some(40_000);
    arena.take_network_sends();
    arena.tick_selector40(banner, &request, &mut host).unwrap();
    assert_eq!(
        arena.object(banner).unwrap().base.position_words,
        Some(home)
    );
    assert_eq!(arena.selector40(banner).unwrap().away_138, 0);
    assert_eq!(host.0[2], (banner, BANNER_HOME, request.null));
    assert_eq!(
        arena.take_network_sends(),
        [(
            0x11,
            GameSend::Banner([banner_handle.raw(), request.null.raw()])
        )]
    );
}

/// 0045BDB0 and 0045BA90: brought to its own team's base, the banner goes
/// home; the carrier's team gains a capture and 1,000 points, the banner's
/// team a loss and 500 points off.
#[test]
fn a_base_captures_a_carried_banner() {
    let (mut world, math) = banner_world();
    let banner = team_banner(&world.arena, 2);
    let base = of_class(&world.arena, false)
        .into_iter()
        .find(|&base| world.arena.actor(base).unwrap().word_b4 == 3)
        .unwrap();
    let vehicle = vehicle(&mut world);
    world.arena.actor_mut(vehicle).unwrap().word_b4 = 3;
    let home = world.arena.selector40(banner).unwrap().home_148.unwrap();
    world.arena.set_position_for_tests(vehicle, home);
    let (arena, request) = split(&mut world, &math);
    arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .update
        .handle_598 = Some(request.null);
    let mut host = Announcements::default();
    arena.tick_selector40(banner, &request, &mut host).unwrap();
    {
        let teams = &mut arena.network_game_mut().unwrap().game.teams;
        teams[2].as_mut().unwrap().score = 700;
    }
    let at = arena.object(base).unwrap().base.position_words.unwrap();
    arena.set_position_for_tests(vehicle, at);
    arena.take_network_sends();
    arena.tick_selector41(base, &request, &mut host).unwrap();
    let state = arena.selector40(banner).unwrap();
    assert_eq!(
        (state.carried_139, state.capturing_13a, state.away_138),
        (0, 0, 0)
    );
    assert_eq!(
        arena.object(banner).unwrap().base.position_words,
        Some(home)
    );
    let handle = arena.actor(vehicle).unwrap().handle;
    assert_eq!(host.0[1], (banner, BANNER_CAPTURED, handle));
    let teams = arena.network_game_mut().unwrap().game.teams;
    let [scorer, loser] = [teams[3].unwrap(), teams[2].unwrap()];
    assert_eq!((scorer.words_0c[0], scorer.score), (1, 1000));
    assert_eq!((loser.words_0c[1], loser.score), (1, 200));
}

/// 0045B860 on a banner another machine owns.
#[test]
fn the_owners_word_on_the_carrier_settles_a_remote_banner() {
    let (mut world, _) = banner_world();
    let banner = of_class(&world.arena, true)[0];
    let null = world.arena.actor_handles().unwrap().null_handle();
    let handle = world.arena.actor(banner).unwrap().handle;
    let ours = ActorHandle::from_raw(1 << 27 | 5);
    let theirs = ActorHandle::from_raw(2 << 27 | 5);
    let arena = &mut world.arena;
    // The owner says our vehicle carries it, which here is free: dropped.
    arena.banner_sync_in(banner, Some(ours), 1, null).unwrap();
    assert_eq!(
        arena.take_network_sends(),
        [(11, GameSend::Pair([ours.raw(), handle.raw()]))]
    );
    assert_eq!(arena.selector40(banner).unwrap().carried_139, 0);
    // Another machine's vehicle carries it.
    arena.banner_sync_in(banner, Some(theirs), 1, null).unwrap();
    let state = arena.selector40(banner).unwrap();
    assert_eq!(
        (state.carried_139, state.away_138, state.carrier_144),
        (1, 1, theirs)
    );
    let mut out = [0; 4];
    assert_eq!(arena.banner_sync_out(banner, &mut out), Ok(4));
    assert_eq!(u32::from_le_bytes(out), theirs.raw());
    // The owner calls it free: so it is.
    arena.banner_sync_in(banner, None, 1, null).unwrap();
    assert_eq!(arena.selector40(banner).unwrap().carried_139, 0);
    assert_eq!(arena.banner_sync_out(banner, &mut out), Ok(0));
    // Our vehicle carries it and the owner calls it free: taken.
    arena.selector40_mut(banner).unwrap().carried_139 = 1;
    arena.selector40_mut(banner).unwrap().carrier_144 = ours;
    arena.banner_sync_in(banner, None, 1, null).unwrap();
    assert_eq!(
        arena.take_network_sends(),
        [(10, GameSend::Pair([ours.raw(), handle.raw()]))]
    );
}

/// 0046A587: the host's restart of an arena clears every score, the game's
/// end and the teams' words, and disposes each banner away from home,
/// handing back its placement to build again.
#[test]
fn the_hosts_restart_sends_the_banners_home() {
    use crate::network::game::TeamScore;
    let (mut world, _) = banner_world();
    let [away, home] = [team_banner(&world.arena, 2), team_banner(&world.arena, 3)];
    world.arena.selector40_mut(away).unwrap().away_138 = 1;
    let placement = world.arena.object(away).unwrap().base.placement_word as i16;
    {
        let game = &mut world.arena.network_game_mut().unwrap().game;
        game.scores[1] = [3, 1, 700];
        game.ended = 1;
        let team = game.teams[2].as_mut().unwrap();
        (team.words_0c, team.score, team.kills) = ([2, 1], 1000, 4);
    }
    assert_eq!(world.arena.restart_arena().unwrap(), [placement]);
    let disposed = |object| {
        world
            .arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .any(|node| node.object == object)
    };
    assert!(disposed(away) && !disposed(home));
    let game = &world.arena.network_game_mut().unwrap().game;
    assert_eq!((game.scores, game.ended), ([[0; 3]; 17], 0));
    assert_eq!(
        game.teams[2],
        Some(TeamScore {
            index: 2,
            ..Default::default()
        })
    );
}

/// 0046A632: a banner sent home is built again from its placement in mode 0
/// (0048E170 with the process NULL), on its team and at home.
#[test]
fn the_restart_builds_its_banners_again_at_home() {
    use crate::session_platform::world_actor_factory::{
        placement_request, RequiredListenerEffects, WorldActorChildHost,
    };
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    settings[0x1c / 4] = 2;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (resident, assets, mut world) = crate::test_worlds::network_world(33, process.clone());
    world
        .arena
        .lend_network_game(Box::new(NetworkGameHold::default()));
    let away = team_banner(&world.arena, 2);
    let home = world.arena.selector40(away).unwrap().home_148.unwrap();
    world.arena.selector40_mut(away).unwrap().away_138 = 1;
    let placements = world.arena.restart_arena().unwrap();
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = &assets.sine_table().values;
    let math = NativeBasisMath::new(*sine);
    let mut host = WorldActorChildHost {
        residency: &resident,
        executable,
        process: &process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: 0,
        sine,
        math: &math,
        camera_settings: None,
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut RequiredListenerEffects,
    };
    let request = placement_request(
        &world.report.instances,
        &world.terrain,
        placements[0] as u32,
        None,
    )
    .unwrap()
    .unwrap();
    let built = host
        .create_object(&mut world.arena, request, &mut world.rng)
        .unwrap()
        .unwrap();
    assert_ne!(built, away);
    let state = world.arena.selector40(built).unwrap();
    assert_eq!((state.away_138, state.home_148), (0, Some(home)));
    assert_eq!(world.arena.actor(built).unwrap().word_b4, 2);
    assert!(
        placement_request(&world.report.instances, &world.terrain, u32::MAX, None)
            .unwrap()
            .is_none()
    );
}
