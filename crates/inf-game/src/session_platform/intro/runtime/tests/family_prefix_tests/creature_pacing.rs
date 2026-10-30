//! Terra Nova's mydons (resource 96, selector 10) in contact, at the port's
//! 16.384 ms world tick. A contact's bump, a channel-1 packet through
//! `0047C6E0`, runs `00428040`, which sets `+22E` bit 0; the creature's next
//! update (`00424830`'s `+D8`, `00425AF0`) then chooses its behavior
//! (`004294A0`), and one choice in four calls (`00429626`, `004296F9`: H9
//! slot 43 for a mydon). Retail updates a creature every frame, so in
//! contact it calls twice as often at 60 fps as at 30. Under
//! `PortFixes::paced_actors` the creature updates every other tick with both
//! ticks' time: a held tick's bump waits for the next update, and no call
//! comes between.

use super::*;
use crate::frontend_settings::PortFixes;
use crate::object_runtime::{ActorDamagePacket, ActorHandle};
use crate::session_platform::particles::damage::{apply_world_actor_damage, WorldDamageOwners};
use crate::session_platform::{ActorPacing, WorldTickCarry};

/// A mydon's call, its `+17C` and `+17E`.
const MYDON_CALL: i16 = 43;
/// One exact world tick.
const TICK: u32 = 16_384;

fn call_records(owners: &TickOwners) -> usize {
    owners
        .sounds
        .records()
        .constructed()
        .iter()
        .filter(|(slot, record)| *slot == Some(MYDON_CALL) && record.volume == Some(0x1_0000))
        .count()
}

/// The pair contact's packet (`0047B9BB..`): channel 1 from the other mydon,
/// with no speed change.
fn bump(
    world: &mut WorldSession,
    owners: &mut TickOwners,
    runtime: &ResidentRuntime,
    assets: &FrontendAssets,
    mydon: ObjectId,
    source: ActorHandle,
) {
    let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut damage = WorldDamageOwners {
        grant_text: None,
        camera_settings: None,
        flash: Some(&mut world.flash),
        mission: &mut world.mission,
        tasks: &mut owners.tasks,
        terrain: &mut world.terrain,
        particles: &mut owners.particles,
        sounds: &mut owners.sounds,
        furniture: &mut owners.furniture,
        residency: runtime,
        executable,
        factory_process: &mut owners.factory_process,
        behavior_tables: &mut owners.behavior_tables,
        animation_clock: &owners.clock,
        session: &mut owners.state,
        particle_load_factor: 0x10000,
        behavior_factory: Some(BehaviorFactoryLoan {
            listeners: &mut world.listeners,
            sine: &sine,
            world_flags: world.world_flags,
        }),
        map: None,
    };
    apply_world_actor_damage(
        &mut world.arena,
        mydon,
        ActorDamagePacket {
            strength: 0,
            channel: 1,
            byte_03: None,
        },
        source,
        &mut damage,
        &mut world.rng,
    )
    .unwrap();
}

/// One mydon bumped by the other before each of `ticks` world ticks under
/// `pacing`, once awake (its command 18 holds it at `+144 = -1` until a
/// bump makes that a 1 ms delay, which its next update spends). For each
/// tick: whether the bump's bit 0 was still set after it (no update took
/// it; an update that ran can also leave it set, as the herding's update
/// `00427A70` does when it finds no herd) and the calls.
fn bumped_mydon(pacing: ActorPacing, ticks: usize) -> Vec<(bool, usize)> {
    bumped_mydon_when(pacing, ticks, |_| true, false)
}

/// [`bumped_mydon`] bumping only where `when(paced)` holds (`paced`: the
/// tick runs the paced update), and with `sampled` keeping the bump's
/// request for a choice as it was on a tick the paced update skips, as the
/// contact pass does under `PortFixes::paced_actors` (item 14).
fn bumped_mydon_when(
    pacing: ActorPacing,
    ticks: usize,
    when: impl Fn(bool) -> bool,
    sampled: bool,
) -> Vec<(bool, usize)> {
    let (runtime, assets, mut world) = crate::test_worlds::offline_world_retained(
        2,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let mydons: Vec<ObjectId> = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| world.arena.object(actor).unwrap().base.resource_word == 96)
        .collect();
    let &[mydon, other] = mydons.as_slice() else {
        panic!("Terra Nova places two mydons: {mydons:?}");
    };
    only_callbacks(&mut world, &[mydon]);
    let mut owners = TickOwners::new(&assets, &mut world, &runtime);
    owners.state = gameplay_state();
    // A session draws before its ticks: the furniture the mydon's avoidance
    // probes reads the selected projection.
    super::super::fall_tests::select_projection(&owners.model_process);
    let source = world.arena.actor(other).unwrap().handle;
    for _ in 0..3 {
        bump(&mut world, &mut owners, &runtime, &assets, mydon, source);
        owners.tick(&mut world, &runtime, &assets, TICK);
    }
    assert_eq!(world.arena.derived_actor(mydon).unwrap().word_144, 0);
    let mut carry = WorldTickCarry::default();
    let mut ticked = Vec::with_capacity(ticks);
    for _ in 0..ticks {
        let mut peek = carry;
        let runs = peek.paced_delta(pacing, TICK).is_some();
        if when(runs) {
            let was = world.arena.family428650_actor(mydon).unwrap().word_22e & 1 != 0;
            bump(&mut world, &mut owners, &runtime, &assets, mydon, source);
            assert_ne!(
                world.arena.family428650_actor(mydon).unwrap().word_22e & 1,
                0
            );
            if sampled && !runs {
                world.arena.keep_choice_request(mydon, was).unwrap();
            }
        }
        let before = call_records(&owners);
        let paced = carry.paced_delta(pacing, TICK);
        owners.tick_paced(&mut world, &runtime, &assets, TICK, paced);
        let kept = world.arena.family428650_actor(mydon).unwrap().word_22e & 1 != 0;
        ticked.push((kept, call_records(&owners) - before));
    }
    assert!(
        !world
            .actor_updates
            .family_prefix_stops
            .iter()
            .any(|stop| stop.actor == mydon),
        "the mydon's update stopped: {:?}",
        world
            .actor_updates
            .family_prefix_stops
            .iter()
            .filter(|stop| stop.actor == mydon)
            .collect::<Vec<_>>()
    );
    ticked
}

#[test]
fn a_bumped_creature_chooses_and_calls_only_at_its_paced_updates() {
    // Four seconds of world ticks.
    const TICKS: usize = 244;
    // Retail's step: every update in contact chooses, so calls come on any
    // tick, about one choice in four.
    let retail = bumped_mydon(ActorPacing::Retail, TICKS);
    assert!(retail.iter().step_by(2).any(|&(_, calls)| calls > 0));
    let retail_calls: usize = retail.iter().map(|&(_, calls)| calls).sum();
    // Paced: no update on every other tick; that tick keeps the bump's bit 0
    // for the next update and makes no call.
    let paced = bumped_mydon(PortFixes::default().actor_pacing(), TICKS);
    for (tick, &(kept, calls)) in paced.iter().enumerate().step_by(2) {
        assert!(
            kept && calls == 0,
            "held tick {tick}: bit 0 kept {kept}, {calls} calls"
        );
    }
    let paced_calls: usize = paced.iter().map(|&(_, calls)| calls).sum();
    assert!(
        paced_calls > 0 && paced_calls < retail_calls,
        "{paced_calls} calls paced, {retail_calls} at every tick"
    );
}

/// Contact that comes only between a paced creature's updates (the camera's
/// vehicle nudging it on the ticks it holds): with every bump kept, each
/// still asks for a choice at the next update and the creature calls; with
/// the contact pass sampling bumps on the paced ticks alone, as retail's
/// 30 fps saw them, it hardly chooses or calls (creatures stayed "in contact"
/// for 93% of a minute at 8 ms against 61% at 33 ms, because a bump on any
/// tick counted).
#[test]
fn bumps_between_paced_updates_ask_for_no_choice() {
    const TICKS: usize = 244;
    let pacing = PortFixes::default().actor_pacing();
    let kept: usize = bumped_mydon_when(pacing, TICKS, |runs| !runs, false)
        .iter()
        .map(|&(_, calls)| calls)
        .sum();
    let sampled: usize = bumped_mydon_when(pacing, TICKS, |runs| !runs, true)
        .iter()
        .map(|&(_, calls)| calls)
        .sum();
    // 33 against 2: the few left come from the creature's own update,
    // which can leave its request set (the herding's, finding no herd).
    assert!(kept > 0, "a held tick's bump calls at the next update");
    assert!(
        sampled * 8 < kept,
        "sampled at the paced ticks: {sampled} calls, {kept} with every bump kept"
    );
}
