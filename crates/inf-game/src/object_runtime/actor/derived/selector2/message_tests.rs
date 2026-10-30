//! The vehicle's +98 `00420BE0` on Level 1's loaded vehicle. Infest03's
//! one radar jam (`vehiclemsg` query, `1A4F63:1F96`): code 0xB from 2000
//! units with the squared distance 0x4F9F1 leaves the map's jitter at
//! 0xB6E8.

use super::super::test_support::loaded;
use super::*;
use crate::{
    object_runtime::ActorExtension, process_particles::factories::ParticleFactoryProcessState,
    terrain_runtime::TerrainSpacing, world_map::WorldMapState,
};

fn message(code: i32, argument: u32) -> ActorMessage {
    ActorMessage {
        sender: None,
        distance_squared: 0x4_f9f1,
        code,
        argument,
    }
}

#[test]
fn vehicle_messages_follow_each_native_arm() {
    let (_resident, _assets, mut world) = loaded(18, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let player = arena.first_player().unwrap();
    let null = arena.actor_handles().unwrap().null_handle();
    let mut process = ParticleFactoryProcessState::native_startup();
    process.process_null_handle = Some(null);
    let context = PlayerActorLookupContext {
        process: Some(&process),
    };
    let send = |arena: &mut ObjectArena, message| {
        arena
            .vehicle_message(vehicle, message, context, false)
            .unwrap()
    };
    // 0x17, the proximity notice, raises +20B only.
    let before = arena.selector2_actor(vehicle).unwrap().clone();
    assert_eq!(send(arena, message(0x17, 0)), VehicleMessageEffect::None);
    let mut expected = before.clone();
    expected.bytes_20a_20c[1] = 1;
    assert_eq!(arena.selector2_actor(vehicle).unwrap(), &expected);
    // 0xD restarts +200 at one second.
    assert_eq!(send(arena, message(0xd, 0)), VehicleMessageEffect::None);
    expected.dwords_200_204[0] = 1_000_000;
    assert_eq!(arena.selector2_actor(vehicle).unwrap(), &expected);
    // The player's text, the process NULL's hit (with the argument's top
    // byte), and the jam.
    assert_eq!(
        send(arena, message(4, 0x0001_016d)),
        VehicleMessageEffect::PlayerText {
            player,
            string: 0x016d
        }
    );
    assert_eq!(
        send(arena, message(5, 0xab00_0000)),
        VehicleMessageEffect::Damage {
            packet: ActorDamagePacket {
                strength: 400,
                channel: 7,
                byte_03: Some(0xab),
            },
            source: null,
        }
    );
    let VehicleMessageEffect::MapJitter(write) = send(arena, message(0xb, 2000)) else {
        panic!("code 0xB jams the radar");
    };
    let mut map = WorldMapState::new();
    map.apply(write).unwrap();
    assert_eq!(map.jitter_radius(), 0xb6e8);
    assert_eq!(send(arena, message(0xc, 0)), VehicleMessageEffect::None);
    // Offline, the online grants and every other code do nothing.
    for code in [
        0,
        3,
        6,
        7,
        8,
        0xa,
        0x11,
        0x12,
        0x13,
        0x15,
        0x16,
        0x18,
        -1,
        i32::MAX,
    ] {
        assert_eq!(send(arena, message(code, 7)), VehicleMessageEffect::None);
    }
    assert_eq!(arena.selector2_actor(vehicle).unwrap(), &expected);
    // Online, the grants stop at the port's offline boundary.
    assert!(matches!(
        arena.vehicle_message(vehicle, message(0x12, 0), context, true),
        Err(VehicleMessageError::OnlineGrant(0x12))
    ));
    // A vehicle no player drives has no text or jam; the rest still act.
    arena.world_player_mut(player).unwrap().handle_d4 = null;
    assert_eq!(send(arena, message(4, 0x16d)), VehicleMessageEffect::None);
    assert_eq!(send(arena, message(0xb, 2000)), VehicleMessageEffect::None);
    expected.bytes_20a_20c[1] = 0;
    arena.selector2_actor_mut(vehicle).unwrap().bytes_20a_20c[1] = 0;
    send(arena, message(0x17, 0));
    expected.bytes_20a_20c[1] = 1;
    assert_eq!(arena.selector2_actor(vehicle).unwrap(), &expected);
    // Only the vehicle's table binds 00420BE0.
    let other = *arena
        .actor_list()
        .iter()
        .find(|&&actor| {
            matches!(
                arena.actor(actor).unwrap().extension,
                ActorExtension::Shared
            )
        })
        .unwrap();
    assert!(matches!(
        arena.vehicle_message(other, message(0x17, 0), context, false),
        Err(VehicleMessageError::Derived(_))
    ));
}
