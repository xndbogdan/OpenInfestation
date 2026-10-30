//! `004212F0` against Infest01's 26 calls on the Level 1 vehicle: two
//! deep-water hits (416 on channel 5 at Shielding 3, which take 403 health)
//! and 24 collisions with nothing to hurt (damage 0 on channel 1), each
//! sounding 0x1D by the speed change.

use super::super::test_support::{DamageEvent as Event, DamageServices};
use super::*;
use crate::object_runtime::{actor::damage::ActorDamagePacket, ActorHandle, PlayerId};
use crate::terrain_runtime::TerrainSpacing;

struct Level1 {
    arena: ObjectArena,
    vehicle: ObjectId,
    player: PlayerId,
    host: DamageServices,
}

/// The Level 1 vehicle as Infest01's first deep-water hit found it: buggy
/// mode, 5000 health, Shielding 3 and no upgrade 0x29.
fn level1() -> Level1 {
    let (_, _, world) = super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let player = arena.first_player().unwrap();
    let null = arena.actor_handles().unwrap().null_handle();
    arena.actor_mut(vehicle).unwrap().health = 5000;
    let state = arena.selector2_actor_mut(vehicle).unwrap();
    state.dwords_1e8_1f4[0] = 1;
    state.drive.dword_578 = Some(59);
    arena.world_player_mut(player).unwrap().bytes_13c_143[6] = 3;
    Level1 {
        arena,
        vehicle,
        player,
        host: DamageServices::new(null),
    }
}

impl Level1 {
    fn hit(&mut self, damage: i32, strength: i16, channel: u8, source: u32) {
        let callback = ActorDamageCallback {
            adjusted_damage: damage,
            packet: ActorDamagePacket {
                strength,
                channel,
                byte_03: None,
            },
            source: ActorHandle::from_raw(source),
        };
        let mut rng = 0;
        self.arena
            .react_vehicle_damage(self.vehicle, callback, &mut rng, &mut self.host)
            .unwrap();
    }

    fn health(&self) -> u16 {
        self.arena.actor(self.vehicle).unwrap().health
    }

    fn position(&self) -> [u16; 3] {
        self.arena
            .object(self.vehicle)
            .unwrap()
            .base
            .position_words
            .unwrap()
    }

    fn set(&mut self, mode: u32, change: u32) {
        let state = self.arena.selector2_actor_mut(self.vehicle).unwrap();
        state.dwords_1e8_1f4[0] = mode;
        state.drive.dword_578 = Some(change);
    }
}

/// Infest01's two deep-water hits: 416 * 100 / 103 = 403 each, no sound.
#[test]
fn deep_water_hits_match_infest01() {
    let mut level = level1();
    level.hit(416, 10000, 5, 0);
    assert_eq!(level.health(), 4597);
    level.hit(416, 10000, 5, 0);
    assert_eq!(level.health(), 4194);
    assert!(level.host.events.is_empty());
}

/// Every collision sounds 0x1D at `(min(+578, 1000) << 16) / 1000`;
/// Infest01's collisions hurt nothing.
#[test]
fn collisions_sound_by_the_speed_change() {
    let mut level = level1();
    let position = level.position();
    for (change, volume) in [
        (8, 0x20c),
        (1, 0x41),
        (49, 0xc8b),
        (0, 0),
        (98, 0x1916),
        (114, 0x1d2f),
        (104, 0x1a9f),
        (5000, 0x10000),
    ] {
        level.set(1, change);
        level.hit(0, 1172, 1, 0x08d6_0001);
        assert_eq!(
            level.host.events.pop(),
            Some(Event::Sound { position, volume }),
            "change {change}"
        );
    }
    assert_eq!(level.health(), 5000);
}

/// A collision before the vehicle's first update, as at a respawn inside the
/// arrival gate, reads the unpublished speed change as 0: the sound plays
/// silent and the collision's damage still follows.
#[test]
fn a_collision_before_the_first_update_sounds_silent() {
    let mut level = level1();
    let position = level.position();
    let state = level.arena.selector2_actor_mut(level.vehicle).unwrap();
    state.dwords_1e8_1f4[0] = 2;
    state.drive.dword_578 = None;
    // (1200 / 3 - 250) * 100 / 103 = 145.
    level.hit(1200, 1172, 1, 0x08d6_0001);
    assert_eq!(
        level.host.events.first(),
        Some(&Event::Sound {
            position,
            volume: 0
        })
    );
    assert_eq!(level.health(), 5000 - 145);
    // The update's first publication is still the field's.
    let state = level.arena.selector2_actor(level.vehicle).unwrap();
    assert_eq!(state.drive.dword_578, None);
}

/// The truck takes a third of a collision less 250, nothing below 1; the
/// VTOL and the skimmer take a strength above 200 instead of the damage.
#[test]
fn modes_scale_collisions() {
    let mut level = level1();
    // (900 / 3 - 250) * 100 / 103 = 48.
    level.set(2, 10);
    level.hit(900, 0, 1, 0);
    assert_eq!(level.health(), 5000 - 48);
    level.hit(700, 0, 1, 0);
    assert_eq!(level.health(), 5000 - 48);
    // 1172 * 100 / 103 = 1137; a strength of 200 keeps the damage.
    level.set(4, 10);
    level.hit(10, 1172, 1, 0);
    assert_eq!(level.health(), 5000 - 48 - 1137);
    level.set(5, 10);
    level.hit(103, 200, 1, 0);
    assert_eq!(level.health(), 5000 - 48 - 1137 - 100);
    // Other channels keep the damage in every mode.
    level.hit(103, 1172, 5, 0);
    assert_eq!(level.health(), 5000 - 48 - 1137 - 200);
}

/// Upgrade 0x29 makes a nonzero hit miss the health; with +D0 set the hit
/// flashes 512 along the forward row, style 0x27 (immune) or 0x28.
#[test]
fn upgrade_0x29_and_the_hit_flash() {
    let mut level = level1();
    level.arena.actor_mut(level.vehicle).unwrap().byte_d0 = 1;
    let basis = level.arena.actor(level.vehicle).unwrap().rotating.basis;
    let position = level.position();
    let nose: [u16; 3] = std::array::from_fn(|axis| {
        let offset = (f32::from_bits(basis.words[7 + axis]) * 512.0) as i32;
        position[axis].wrapping_add(offset as u16)
    });
    level.hit(416, 10000, 5, 0);
    assert_eq!(level.health(), 4597);
    assert_eq!(
        level.host.events,
        [Event::Flash {
            position: nose,
            style: 0x28
        }]
    );
    level.host.events.clear();
    let player = level.player;
    level.arena.world_player_mut(player).unwrap().bits_100[1] |= 1 << (0x29 - 32);
    level.hit(416, 10000, 5, 0);
    assert_eq!(level.health(), 4597);
    assert_eq!(
        level.host.events,
        [Event::Flash {
            position: nose,
            style: 0x27
        }]
    );
    // A zero hit skips both and reaches the derived damage.
    level.host.events.clear();
    level.hit(0, 10000, 5, 0);
    assert!(level.host.events.is_empty());
}

/// The selector-70 actor keeps the vehicle at one health at least.
#[test]
fn the_selector_70_actor_keeps_one_health() {
    let mut level = level1();
    level.host.selector70 = true;
    level
        .arena
        .world_player_mut(level.player)
        .unwrap()
        .bytes_13c_143[6] = 0;
    level.hit(9000, 10000, 5, 0);
    assert_eq!(level.health(), 1);
}

#[test]
fn a_network_vehicle_keeps_its_attacker_and_buggy_madness_takes_no_damage() {
    let mut level = level1();
    level.host.network = true;
    level.host.clock = Some(0x500);
    level.host.process.settings[0x10 / 4] = 2;
    let source = ActorHandle::from_raw(0x1000_0012 | 1 << 26);
    let health = level.arena.actor(level.vehicle).unwrap().health;
    let callback = ActorDamageCallback {
        adjusted_damage: 416,
        packet: ActorDamagePacket {
            strength: 10000,
            channel: 1,
            byte_03: None,
        },
        source,
    };
    // Before the hit, a death's credit reads the fresh heap's zero.
    assert_eq!(
        level.arena.vehicle_attacked_clock(level.vehicle).unwrap(),
        0
    );
    level
        .arena
        .react_vehicle_damage(level.vehicle, callback, &mut 0, &mut level.host)
        .unwrap();
    let state = level.arena.selector2_actor(level.vehicle).unwrap();
    assert_eq!(
        (state.attacker_5a4, state.attacked_5a8),
        (source.raw(), Some(0x500))
    );
    // A death reads the clock (+5A8) and the handle's high WORD (+5A6).
    assert_eq!(
        level.arena.vehicle_attacked_clock(level.vehicle).unwrap(),
        0x500
    );
    assert_eq!(
        level.arena.vehicle_attacker_word(level.vehicle).unwrap(),
        (source.raw() >> 16) as u16
    );
    assert_eq!(level.arena.actor(level.vehicle).unwrap().health, health);
    // The collision sound comes first.
    assert!(matches!(level.host.events[..], [Event::Sound { .. }]));
    // Nor do Powerball's (mode 4), with no ball to knock loose.
    level.host.process.settings[0x10 / 4] = 4;
    level
        .arena
        .react_vehicle_damage(level.vehicle, callback, &mut 0, &mut level.host)
        .unwrap();
    assert_eq!(level.arena.actor(level.vehicle).unwrap().health, health);
}
