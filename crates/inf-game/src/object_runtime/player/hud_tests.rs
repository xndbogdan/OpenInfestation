use super::*;
use crate::object_runtime::{ActorFollowTarget, FixedPositionTarget, FurniturePositionTarget};
use crate::terrain_runtime::TerrainSpacing;
use crate::world_backend::WorldSession;

const HUD_ON: HudReticleSettings = HudReticleSettings {
    hud: 1,
    settings_7c: 0,
    special_view: false,
};

/// Level 1 with its vehicle, its player and another actor to target.
fn level1() -> (WorldSession, ObjectId, PlayerId, ObjectId) {
    let (_, _, world) = crate::test_worlds::offline_world(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    let other = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| actor != vehicle)
        .unwrap();
    (world, vehicle, player, other)
}

fn target(arena: &mut ObjectArena, vehicle: ObjectId, data: NativeTarget) {
    let reference = arena.import_fixture_target(data);
    arena.derived_actor_mut(vehicle).unwrap().target_148 = Some(reference);
}

fn behavior_lead(arena: &ObjectArena, vehicle: ObjectId) -> u32 {
    arena
        .derived_actor(vehicle)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .word_20
}

/// `0041C964..0041CB89`: a type-2 target is refreshed (its focus +08 and
/// velocity written back) and read for the box with its actor's health and
/// side; no sight follows, so `0041FF20` leaves the lead alone. Framing the
/// vehicle itself reads no actor.
#[test]
fn a_type_2_target_is_refreshed_and_framed_without_the_sight() {
    let (mut world, vehicle, player, other) = level1();
    let arena = &mut world.arena;
    arena.owned_mut(other).unwrap().state.base.position_words = Some([0x1000, 0x40, 0x2000]);
    {
        let actor = arena.actor_mut(other).unwrap();
        actor.word_74 = 0x10;
        actor.health = 30;
        actor.maximum_health = Some(40);
        actor.word_b4 = 5;
    }
    let handle = arena.actor(other).unwrap().handle;
    target(
        arena,
        vehicle,
        NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: None,
            word_10: 2,
            focus_14: Some([1, 2, 3]),
            velocity_1a: None,
        }),
    );
    arena
        .derived_actor_mut(vehicle)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .word_20 = 77;
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!(
        hud.target,
        Some(HudTarget {
            kind: 2,
            focus: Some([1, 2, 3]),
            actor: Some(HudTargetActor {
                health: 30,
                maximum_health: Some(40),
                team: 5,
            }),
        })
    );
    assert_eq!(hud.sight_direction, None);
    assert_eq!(behavior_lead(arena, vehicle), 77);
    let reference = arena.derived_actor(vehicle).unwrap().target_148.unwrap();
    let NativeTarget::ActorFollow(refreshed) = arena.native_target(reference).unwrap() else {
        panic!("the target changed class");
    };
    assert_eq!(refreshed.focus_08, Some([0x1000, 0x50, 0x2000]));
    assert_eq!(refreshed.focus_14, Some([1, 2, 3]));
    assert!(refreshed.velocity_1a.is_some());
    // Settings +7C copies the refreshed focus to +14, which the box reads.
    let settings = HudReticleSettings {
        settings_7c: 1,
        ..HUD_ON
    };
    let (hud, _) = arena
        .vehicle_hud_sources(vehicle, player, settings)
        .unwrap();
    assert_eq!(hud.target.unwrap().focus, Some([0x1000, 0x50, 0x2000]));
    // 0041CB89: the vehicle's own +84 reads no actor.
    let own = arena.actor(vehicle).unwrap().handle;
    target(
        arena,
        vehicle,
        NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: own,
            focus_08: None,
            word_10: 2,
            focus_14: Some([1, 2, 3]),
            velocity_1a: None,
        }),
    );
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!(hud.target.unwrap().actor, None);
}

/// Furniture (type 4) is framed at its +14 with no actor (its virtual +10
/// is `00401300`, NULL); a point (type 8) keeps the sight, whose direction
/// runs from the hull to the point and clears the lead.
#[test]
fn furniture_frames_its_point_and_a_point_keeps_the_sight() {
    let (mut world, vehicle, player, _) = level1();
    let arena = &mut world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    target(
        arena,
        vehicle,
        NativeTarget::FurniturePosition(FurniturePositionTarget {
            handle_04: null,
            focus_08: [4, 5, 6],
            word_10: 4,
            focus_14: [7, 8, 9],
        }),
    );
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!(
        hud.target,
        Some(HudTarget {
            kind: 4,
            focus: Some([7, 8, 9]),
            actor: None,
        })
    );
    assert_eq!(hud.sight_direction, None);
    let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
    let point = [
        position[0].wrapping_add(100),
        position[1].wrapping_sub(20),
        position[2].wrapping_add(300),
    ];
    target(
        arena,
        vehicle,
        NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: null,
            focus_08: Some(point),
            word_10: 8,
        }),
    );
    arena
        .derived_actor_mut(vehicle)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .word_20 = 77;
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!(hud.target.unwrap().kind, 8);
    assert_eq!(hud.sight_direction, Some([100, -20, 300]));
    assert_eq!(behavior_lead(arena, vehicle), 0);
    assert_eq!(hud.position, position);
    assert_eq!(hud.team, arena.actor(vehicle).unwrap().word_b4);
    assert_eq!(
        hud.corners,
        crate::vehicle_hud::RETICLE_CORNER_RESIDUE_APPROXIMATION
    );
    // The special view frees the target and writes no direction.
    let settings = HudReticleSettings {
        special_view: true,
        ..HUD_ON
    };
    let (hud, _) = arena
        .vehicle_hud_sources(vehicle, player, settings)
        .unwrap();
    assert_eq!(hud.sight_direction, None);
    assert_eq!(arena.derived_actor(vehicle).unwrap().target_148, None);
    // Without a target the direction is the forward row times 4096.
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!(hud.target, None);
    let forward = arena.actor(vehicle).unwrap().rotating.basis.words;
    let expected: [i16; 3] =
        std::array::from_fn(|axis| (f32::from_bits(forward[7 + axis]) * 4096.0) as i16);
    assert_eq!(hud.sight_direction, Some(expected));
}

/// With the HUD setting off or a menu shown the reticle reads nothing: the
/// target is neither refreshed nor read, and no direction is taken.
#[test]
fn the_hud_setting_and_a_shown_menu_skip_the_reticle() {
    let (mut world, vehicle, player, other) = level1();
    let arena = &mut world.arena;
    let handle = arena.actor(other).unwrap().handle;
    target(
        arena,
        vehicle,
        NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: None,
            word_10: 2,
            focus_14: Some([1, 2, 3]),
            velocity_1a: None,
        }),
    );
    let reference = arena.derived_actor(vehicle).unwrap().target_148.unwrap();
    let before = arena.native_target(reference).unwrap();
    let off = HudReticleSettings { hud: 0, ..HUD_ON };
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, off).unwrap();
    assert_eq!((hud.target, hud.sight_direction), (None, None));
    arena
        .world_player_mut(player)
        .unwrap()
        .set_message_byte_00(1);
    let (hud, _) = arena.vehicle_hud_sources(vehicle, player, HUD_ON).unwrap();
    assert_eq!((hud.target, hud.sight_direction), (None, None));
    assert_eq!(arena.native_target(reference).unwrap(), before);
}
