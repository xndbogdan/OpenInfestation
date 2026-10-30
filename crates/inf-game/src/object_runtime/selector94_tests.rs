use super::*;
use crate::object_runtime::ObjectList;
use crate::process_particles::factories::ParticleFactoryProcessState;

/// Records each missile the launcher asks for.
#[derive(Default)]
struct Missiles(Vec<([u16; 3], ActorHandle)>);

impl Selector94Host for Missiles {
    type Error = std::convert::Infallible;
    fn fire(
        &mut self,
        _: &mut ObjectArena,
        position: [u16; 3],
        target: ActorHandle,
        _: &mut u32,
    ) -> Result<(), Self::Error> {
        self.0.push((position, target));
        Ok(())
    }
}

/// Capture the Banner's world 33 as the host; mode 2 gives the first
/// player a vehicle at once.
fn launcher_world() -> (crate::world_backend::WorldSession, ObjectId) {
    let mut settings = [0; 20];
    settings[0x10 / 4] = 2;
    let process = ParticleFactoryProcessState {
        network_flag: 1,
        host_flag: 1,
        local_slot: 1,
        settings,
        ..ParticleFactoryProcessState::native_startup()
    };
    let (_resident, _assets, world) = crate::test_worlds::network_world(33, process);
    let launcher = [
        ObjectList::Flag0Primary,
        ObjectList::Flag1Primary,
        ObjectList::Parking,
    ]
    .into_iter()
    .flat_map(|list| world.arena.list(list).iter().map(|node| node.object))
    .find(|&object| world.arena.selector94(object).is_ok())
    .expect("world 33 places its missile launcher");
    (world, launcher)
}

/// 004458E0 and command 2: world 33's launcher keeps its two zones, a
/// wide one of kind 0 and a kind-1 one inside it.
#[test]
fn the_launcher_keeps_its_zones() {
    let (world, launcher) = launcher_world();
    let state = world.arena.selector94(launcher).unwrap();
    assert_eq!(state.countdown_44, 0);
    let zones = state.zones_40.clone().expect("command 2's zones");
    assert_eq!(
        world
            .arena
            .selector94_zones::<std::convert::Infallible>(launcher, &zones)
            .unwrap(),
        [
            [0, -31768, 0, 32767, 12800, 24000],
            [1, 23665, 0, -32768, 5000, 3000],
        ]
    );
}

/// 00445930: spent, it fires 1500 past the vehicle along its bearing and
/// 500 above, homing on the vehicle, then waits 1.5 s; the last zone
/// holding the vehicle must be of kind 1, or none hold it.
#[test]
fn the_launcher_fires_unless_its_last_zone_forbids() {
    let (mut world, launcher) = launcher_world();
    let arena = &mut world.arena;
    let vehicle = arena.first_player_primary_actor().unwrap().unwrap();
    let handle = arena.actor(vehicle).unwrap().handle;
    let place = |arena: &mut ObjectArena, vehicle_at: [u16; 3]| {
        arena.set_position_for_tests(vehicle, vehicle_at);
        arena.set_position_for_tests(
            launcher,
            [vehicle_at[0].wrapping_sub(1000), 900, vehicle_at[2]],
        );
    };
    let mut host = Missiles::default();
    let mut rng = 0;
    // In the kind-1 zone, which is also inside the kind-0 one: it fires.
    place(arena, [23665, 300, 32768]);
    arena
        .tick_selector94(launcher, 40_000, &mut rng, &mut host)
        .unwrap();
    assert_eq!(host.0, [([23665 + 1500, 800, 32768], handle)]);
    assert_eq!(arena.selector94(launcher).unwrap().countdown_44, 1_500_000);
    arena
        .tick_selector94(launcher, 40_000, &mut rng, &mut host)
        .unwrap();
    assert_eq!(arena.selector94(launcher).unwrap().countdown_44, 1_460_000);
    assert_eq!(host.0.len(), 1);
    // Only in the kind-0 zone: it holds its fire, and keeps trying.
    arena.selector94_mut(launcher).unwrap().countdown_44 = 0;
    place(arena, [40000, 300, 32768]);
    arena
        .tick_selector94(launcher, 40_000, &mut rng, &mut host)
        .unwrap();
    assert_eq!(host.0.len(), 1);
    assert_eq!(arena.selector94(launcher).unwrap().countdown_44, 0);
    // In no zone: it fires.
    place(arena, [2000, 300, 2000]);
    arena
        .tick_selector94(launcher, 40_000, &mut rng, &mut host)
        .unwrap();
    assert_eq!(host.0[1], ([2000 + 1500, 800, 2000], handle));
}
