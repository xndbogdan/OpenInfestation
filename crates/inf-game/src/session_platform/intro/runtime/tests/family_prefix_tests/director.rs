//! The Alien Brain's ending director (selector 78) over world 19 and the
//! production updates. The tests stand in for the brain where it steps the
//! phase (3 when its attack ends, 5 when it has opened). Offline worlds hold
//! no player input references, so the player's update stops and the update
//! list keeps running the director after the take, where retail's player
//! update would run its +44 (00407340), which runs the same +24.

use super::*;
use crate::object_runtime::{ActorHandle, CreationListenerInterface, DerivedActorClass};

fn director(world: &WorldSession) -> ObjectId {
    let directors: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.selector78(actor).is_ok())
        .collect();
    assert_eq!(directors.len(), 1, "the Alien Brain places one director");
    directors[0]
}

fn camera(world: &WorldSession) -> ObjectId {
    world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| world.arena.selector16(actor).is_ok())
        .expect("the world's camera")
}

/// The handle the camera's primary (00410E70) or secondary (0040F710)
/// target follows.
fn followed(world: &WorldSession, secondary: bool) -> ActorHandle {
    let camera = world.arena.selector16(camera(world)).unwrap();
    let target = if secondary {
        camera.target_190
    } else {
        camera.target_18c
    };
    world
        .arena
        .actor_follow_target(target.expect("a camera target"))
        .unwrap()
        .handle_04
}

fn handle(world: &WorldSession, actor: ObjectId) -> ActorHandle {
    world.arena.actor(actor).unwrap().handle
}

struct Ending {
    runtime: ResidentRuntime,
    assets: FrontendAssets,
    world: WorldSession,
    owners: TickOwners,
    director: ObjectId,
}

impl Ending {
    fn start() -> Self {
        let (runtime, assets, mut world) = crate::test_worlds::offline_world_with_services(
            19,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let director = director(&world);
        only_callbacks(&mut world, &[director]);
        let mut owners = TickOwners::new(&assets, &mut world, &runtime);
        owners.state = gameplay_state();
        Self {
            runtime,
            assets,
            world,
            owners,
            director,
        }
    }

    fn tick(&mut self) {
        self.owners
            .tick(&mut self.world, &self.runtime, &self.assets, 40_000);
        assert_eq!(self.world.actor_updates.selector78_stops, Vec::new());
    }

    fn state(&self) -> u32 {
        self.world
            .arena
            .selector78(self.director)
            .unwrap()
            .state_160
    }

    fn phase(&self) -> u32 {
        self.world.arena.alien_brain_words().phase_546910
    }

    fn set_phase(&mut self, phase: u32) {
        self.world.arena.alien_brain_words_mut().phase_546910 = phase;
    }

    fn run_until(&mut self, limit: usize, done: impl Fn(&Self) -> bool) {
        for _ in 0..limit {
            if done(self) {
                return;
            }
            self.tick();
        }
        panic!("not reached in {limit} ticks: state {}", self.state());
    }

    fn helpers(&self) -> (ObjectId, ObjectId) {
        let state = self.world.arena.selector78(self.director).unwrap();
        (state.helper_164.unwrap(), state.helper_168.unwrap())
    }

    /// Leaves state 1 at the lock and flies helper +164 to the end of route
    /// 0, which sets phase 2 (the brain's attack); then the attack ends.
    fn through_the_attack(&mut self) {
        self.tick();
        self.set_phase(1);
        self.run_until(1000, |ending| ending.phase() == 2);
        self.set_phase(3);
        self.run_until(10, |ending| ending.state() == 3);
    }
}

/// 00406640 binds the listener and command 16 keeps the route list. The
/// director waits in state 1 for the lock; then it builds the helpers, the
/// first player takes it, and the camera follows helper +164 until it has
/// flown route 0 (phase 2). In state 3 the camera follows each of the
/// aliens' units in turn with five sparks round it, and retires it: world
/// 19's are its eyes, each dying (00408560) into a selector-82 remains at
/// its stalk's tip, and once none is left state 4 follows.
#[test]
fn the_director_takes_the_player_and_shows_the_aliens_dying() {
    let mut ending = Ending::start();
    let world = &ending.world;
    assert!(world.listeners.registrations().any(|listener| {
        listener.owner == ending.director
            && listener.interface == CreationListenerInterface::Selector78
    }));
    assert!(world
        .arena
        .selector78(ending.director)
        .unwrap()
        .routes_154
        .is_some());
    for _ in 0..3 {
        ending.tick();
    }
    assert_eq!(ending.state(), 1, "it waits for the lock");
    ending.set_phase(1);
    ending.run_until(5, |ending| ending.state() == 2);
    let (guide, bug) = ending.helpers();
    assert!(ending.world.arena.selector90(guide).is_ok());
    assert!(ending.world.arena.selector90(bug).is_ok());
    let player = ending.world.arena.first_player().unwrap();
    assert_eq!(
        ending.world.arena.world_player(player).unwrap().handle_d8,
        handle(&ending.world, ending.director)
    );
    assert_eq!(followed(&ending.world, false), handle(&ending.world, guide));
    assert_eq!(
        ending
            .world
            .arena
            .selector16(camera(&ending.world))
            .unwrap()
            .byte_184,
        1
    );
    ending.run_until(1000, |ending| ending.phase() == 2);
    ending.set_phase(3);
    ending.run_until(10, |ending| ending.state() == 3);

    let free = ending.owners.particles.free_count();
    let null = ending.world.arena.actor_handles().unwrap().null_handle();
    ending.run_until(100, |ending| {
        ending
            .world
            .arena
            .selector78(ending.director)
            .unwrap()
            .victim_16c
            != null
    });
    let victim = ending
        .world
        .arena
        .selector78(ending.director)
        .unwrap()
        .victim_16c;
    let eye = ending
        .world
        .arena
        .resolve_actor_handle(victim)
        .unwrap()
        .unwrap();
    assert!(matches!(
        ending.world.arena.derived_actor(eye).unwrap().class,
        DerivedActorClass::Selector77(_)
    ));
    assert_eq!(followed(&ending.world, false), victim);
    ending.tick();
    assert!(ending.owners.particles.free_count() < free, "sparks");
    let remains = |world: &WorldSession| {
        world
            .arena
            .actor_list()
            .iter()
            .filter(|&&actor| {
                world
                    .arena
                    .selector59(actor)
                    .is_ok_and(|state| state.timer_148.is_some())
            })
            .count()
    };
    assert_eq!(remains(&ending.world), 0);
    // Past 500 ms the eye is retired: it dies into its remains.
    ending.run_until(20, |ending| ending.world.arena.actor(eye).is_err());
    assert_eq!(remains(&ending.world), 1);
    ending.run_until(2000, |ending| ending.state() == 4);
    let eyes = ending
        .world
        .arena
        .actor_list()
        .iter()
        .filter(|&&actor| {
            ending
                .world
                .arena
                .derived_actor(actor)
                .is_ok_and(|derived| matches!(derived.class, DerivedActorClass::Selector77(_)))
        })
        .count();
    assert_eq!(eyes, 0, "every eye died");
    // Each remains bursts and goes after 5 s (004098C0); the victims came
    // 2 s apart, so only the last few are left.
    assert!((1..=3).contains(&remains(&ending.world)));
    assert_eq!(ending.phase(), 4);
}

/// With the aliens' units gone (disposed), state 3 ends; state 4 has the
/// camera ride +164 looking at +168 and sets phase 4. Once the brain has
/// opened (phase 5) the bug (+168) is drawn and flies route 1, and 600 ms
/// after it arrives state 6 ends the world (00487560(1, 1, 0)) 750 ms in,
/// with the completion bonus.
#[test]
fn the_bug_flies_out_and_the_world_ends() {
    let mut ending = Ending::start();
    let units: Vec<ObjectId> = ending
        .world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| ending.world.arena.actor(actor).unwrap().word_b4 == 0x12)
        .collect();
    for unit in units {
        ending.world.arena.dispose(unit).unwrap();
    }
    ending.through_the_attack();
    ending.run_until(100, |ending| ending.state() == 4);
    assert_eq!(ending.phase(), 4, "the brain opens");
    let (guide, bug) = ending.helpers();
    assert_eq!(followed(&ending.world, false), handle(&ending.world, guide));
    assert_eq!(followed(&ending.world, true), handle(&ending.world, bug));
    ending.set_phase(5);
    ending.run_until(10, |ending| ending.state() == 5);
    let flags = ending.world.arena.object(bug).unwrap().base.flags;
    assert_eq!(flags & 0x801, 0x801, "the bug is drawn");
    ending.run_until(3000, |ending| ending.state() == 6);
    assert_eq!(ending.world.arena.selector90(bug).unwrap().byte_140, 1);
    ending.run_until(30, |ending| ending.world.actor_updates.cheat_flags_84 != 0);
    assert_eq!(ending.world.actor_updates.cheat_flags_84, 0x40);
    assert!(ending.owners.state.transition_pending());
}
