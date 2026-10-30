use super::*;
use crate::process_particles::factories::ParticleFactoryProcessState;

#[test]
fn a_signed_timer_rises_to_the_floor_only() {
    for (before, after) in [
        (0_u16, 8000_u16),
        (7999, 8000),
        (8000, 8000),
        (9000, 9000),
        (0x8000, 8000),
    ] {
        let mut timer = before;
        raise(&mut timer);
        assert_eq!(timer, after, "{before}");
    }
}

fn message(code: i32) -> ActorMessage {
    ActorMessage {
        sender: None,
        distance_squared: 250_000,
        code,
        argument: 2000,
    }
}

/// The +98 of Level 1's actors, by table: a turret (00402420) raises its
/// jam timer +1B0 to 8000 on 0xB, sets it to 12000 on 0xC, and raises +17A
/// on 0xD; a family actor (004270B0) only raises +17A, on 0xD; the camera
/// and the other shared tables (004045F0) answer 0; the vehicle's
/// (00420BE0) chain arm jams its player's radar by the distance; and a
/// grenade's (004557F0) ignores the chain.
#[test]
fn level_1_receivers_take_the_chain_and_the_alert_by_their_tables() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let process = ParticleFactoryProcessState::native_startup();
    let context = PlayerActorLookupContext {
        process: Some(&process),
    };
    let arena = &mut world.arena;
    let find = |arena: &ObjectArena, receiver: Receiver| {
        arena
            .actor_list()
            .iter()
            .copied()
            .find(|&actor| arena.message_receiver(actor).unwrap() == Some(receiver))
            .unwrap_or_else(|| panic!("a {receiver:?} in Level 1"))
    };
    let turret = find(arena, Receiver::Selector11);
    let family = find(arena, Receiver::Family4270b0);
    let shared = find(arena, Receiver::Shared4045f0);
    let vehicle = find(arena, Receiver::Selector2);

    let timers = |arena: &ObjectArena, actor: ObjectId| {
        let derived = arena.derived_actor(actor).unwrap();
        let jam = match &derived.class {
            DerivedActorClass::Selector11(state) => Some(state.word_1b0),
            _ => None,
        };
        (jam, derived.word_17a)
    };
    let before = timers(arena, turret);
    assert_eq!(before, (Some(0), 0));
    let reply = arena
        .receive_actor_message(turret, message(0xb), context, false)
        .unwrap();
    assert_eq!(reply, ActorMessageReply::Done(0));
    assert_eq!(timers(arena, turret), (Some(8000), 0));
    arena
        .receive_actor_message(turret, message(0xc), context, false)
        .unwrap();
    assert_eq!(timers(arena, turret), (Some(12000), 0));
    arena
        .receive_actor_message(turret, message(0xb), context, false)
        .unwrap();
    assert_eq!(timers(arena, turret), (Some(12000), 0));
    arena
        .receive_actor_message(turret, message(0xd), context, false)
        .unwrap();
    assert_eq!(timers(arena, turret), (Some(12000), 8000));
    assert!(matches!(
        arena.receive_actor_message(turret, message(7), context, false),
        Err(ActorMessageError::Unported {
            function: 0x0040_2420,
            ..
        })
    ));

    assert_eq!(timers(arena, family).1, 0);
    arena
        .receive_actor_message(family, message(0xb), context, false)
        .unwrap();
    assert_eq!(timers(arena, family).1, 0);
    arena
        .receive_actor_message(family, message(0xd), context, false)
        .unwrap();
    assert_eq!(timers(arena, family).1, 8000);

    assert_eq!(
        arena
            .receive_actor_message(shared, message(0xb), context, false)
            .unwrap(),
        ActorMessageReply::Done(0)
    );

    assert_eq!(
        arena
            .receive_actor_message(vehicle, message(0xb), context, false)
            .unwrap(),
        ActorMessageReply::Vehicle(VehicleMessageEffect::MapJitter(
            crate::world_map::WorldMapJitterWrite::RadialImpact420BE0 {
                range: 2000,
                distance: 500,
            }
        ))
    );
}

/// 0047DAF0's zero-radius arm: every actor in G+48 order at distance 0,
/// whatever the sender and with no position read. A zero radius from
/// 0047DD20 takes the same arm.
#[test]
fn a_zero_radius_reaches_every_actor_at_distance_zero() {
    let (_resident, _assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let arena = &mut world.arena;
    let listed: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            !arena.construction_is_stopped(actor).unwrap()
                && !arena.disposal_is_stopped(actor).unwrap()
        })
        .collect();
    assert!(listed.len() > 2);
    let mut every = Vec::new();
    let sum = arena
        .walk_every_message_receiver::<ObjectError>(|_, actor| {
            every.push(actor);
            Ok(2)
        })
        .unwrap();
    assert_eq!(every, listed);
    assert_eq!(sum as usize, 2 * listed.len());
    let mut zero = Vec::new();
    arena
        .walk_message_receivers::<ObjectError>(
            listed[1],
            ActorMessageWalk::Radius { radius: 0 },
            |_, actor, distance| {
                zero.push((actor, distance));
                Ok(0)
            },
        )
        .unwrap();
    assert_eq!(
        zero,
        listed.iter().map(|&actor| (actor, 0)).collect::<Vec<_>>()
    );
}
