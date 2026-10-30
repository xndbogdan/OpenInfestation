//! The save pad's update `00475420` against Infest02's pad at (4E7C, 90,
//! 816E), which the drowned vehicle had visited (`padupdate` query).

use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorRegistryConfig, ActorTerrainInputs, ObjectClassKind,
};

const SINE: [u16; 4096] = [0; 4096];
const PAD: [u16; 3] = [0x4e7c, 0x90, 0x816e];
/// The vehicle's position at the visit, as the spawn save stored it.
const VISIT: [u16; 3] = [0x4eca, 0xd7, 0x81a6];

#[derive(Default)]
struct Host {
    chimes: Vec<([u16; 3], u32)>,
    saves: Vec<PlayerId>,
}

impl Selector58Host for Host {
    type Error = ();
    fn chime(&mut self, position: [u16; 3], pitch: u32) -> Result<(), ()> {
        self.chimes.push((position, pitch));
        Ok(())
    }
    fn save_spawn(&mut self, _: &mut ObjectArena, player: PlayerId) -> Result<(), ()> {
        self.saves.push(player);
        Ok(())
    }
}

struct Pad {
    arena: ObjectArena,
    pad: ObjectId,
    vehicle: ObjectId,
    player: PlayerId,
}

fn pad() -> Pad {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let player = arena.add_world_player([0; 6]);
    let vehicle = arena
        .allocate_actor(ActorConstructionRequest {
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
    arena.actor_mut(vehicle).unwrap().word_c4 = 3;
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some(VISIT);
    let handle = arena.actor(vehicle).unwrap().handle;
    arena.world_player_mut(player).unwrap().handle_d4 = handle;
    let pad = arena.allocate(ObjectClassKind::Selector58);
    arena.owned_mut(pad).unwrap().state.base.position_words = Some(PAD);
    Pad {
        arena,
        pad,
        vehicle,
        player,
    }
}

impl Pad {
    fn tick(&mut self, host: &mut Host) {
        self.arena.tick_selector58(self.pad, 125_000, host).unwrap();
    }
    fn state(&self) -> (u8, u16) {
        let state = self.arena.selector58(self.pad).unwrap();
        (state.byte_6c, state.word_6e)
    }
    fn place(&mut self, position: [u16; 3]) {
        self.arena
            .owned_mut(self.vehicle)
            .unwrap()
            .state
            .base
            .position_words = Some(position);
    }
}

/// The visit saves the spawn once and chimes 5555h, 8000h and 18000h; the
/// timer then rises by a quarter of the delta (7A13h, F425h), the vehicle
/// leaves, and the timer's expiry chimes 8000h, C000h and 10000h.
#[test]
fn the_pad_saves_the_spawn_and_chimes_as_retail_does() {
    let mut pad = pad();
    let mut host = Host::default();
    pad.tick(&mut host);
    assert_eq!(host.saves, [pad.player]);
    assert_eq!(host.chimes, [(PAD, 0x5555), (PAD, 0x8000), (PAD, 0x18000)]);
    assert_eq!(pad.state(), (1, 1));
    let mut host = Host::default();
    pad.tick(&mut host);
    assert_eq!(pad.state(), (1, 0x7a13));
    pad.place([0x4f7c, 0xd7, 0x816e]);
    pad.tick(&mut host);
    assert_eq!(pad.state(), (0, 0xf425));
    assert!(host.chimes.is_empty() && host.saves.is_empty());
    pad.tick(&mut host);
    assert_eq!(pad.state(), (0, 0));
    assert_eq!(host.chimes, [(PAD, 0x8000), (PAD, 0xc000), (PAD, 0x10000)]);
}

/// A visit counts once until the vehicle leaves across; rising out of the
/// pad's 100h height keeps the visit, a dying vehicle does nothing, and
/// without a player the visit clears.
#[test]
fn the_pad_counts_one_visit_until_the_vehicle_leaves() {
    let mut pad = pad();
    let mut host = Host::default();
    pad.tick(&mut host);
    assert_eq!(host.saves.len(), 1);
    pad.place([VISIT[0], 0x191, VISIT[2]]);
    pad.tick(&mut host);
    pad.place(VISIT);
    pad.tick(&mut host);
    assert_eq!(host.saves.len(), 1);
    assert_eq!(pad.state().0, 1);
    pad.arena.actor_mut(pad.vehicle).unwrap().word_c4 = 1;
    pad.place([0x5f7c, 0xd7, 0x816e]);
    pad.tick(&mut host);
    assert_eq!(pad.state().0, 1);
    pad.arena.actor_mut(pad.vehicle).unwrap().word_c4 = 3;
    pad.tick(&mut host);
    assert_eq!(pad.state().0, 0);
    pad.place(VISIT);
    pad.tick(&mut host);
    assert_eq!(host.saves.len(), 2);
    // Without a player (00488AA0 null) the visit clears.
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 16,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let alone = arena.allocate(ObjectClassKind::Selector58);
    arena.owned_mut(alone).unwrap().state.base.position_words = Some(PAD);
    arena.selector58_mut(alone).unwrap().byte_6c = 1;
    arena.tick_selector58(alone, 125_000, &mut host).unwrap();
    assert_eq!(arena.selector58(alone).unwrap().byte_6c, 0);
}
