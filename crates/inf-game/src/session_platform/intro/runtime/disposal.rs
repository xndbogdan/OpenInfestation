//! The actual 478F4B manager pass, before this frame's actor callbacks.

use super::{WorldActorTickContext, WorldSession};
use crate::object_runtime::{
    LiveGrantContext, ObjectDisposalFailurePolicy, ObjectDisposalRequest, SoundTeardownService,
};

/// 0043D0F0's Dormant exit for the ship the drain paused at, over the
/// tick's owners.
fn run_dormant_exit(
    world: &mut WorldSession,
    context: &mut WorldActorTickContext<'_>,
    ship: crate::object_runtime::ObjectId,
) -> Result<(), String> {
    let executable = inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
        .expect("FrontendAssets retains its validated executable");
    let mut owners = crate::session_platform::particles::damage::WorldDamageOwners {
        grant_text: context.grant_text,
        camera_settings: Some(context.inputs.camera_settings),
        flash: Some(&mut world.flash),
        mission: &mut world.mission,
        tasks: &mut *context.tasks,
        terrain: &mut world.terrain,
        particles: &mut *context.particles,
        sounds: &mut *context.sounds,
        furniture: &mut *context.furniture,
        residency: context.residency,
        executable,
        factory_process: &mut *context.factory_process,
        behavior_tables: &mut *context.behavior_tables,
        animation_clock: context.animation_clock,
        session: &*context.state,
        particle_load_factor: world.fade.performance.particle_load_factor,
        behavior_factory: Some(
            crate::session_platform::particles::damage::BehaviorFactoryLoan {
                listeners: &mut world.listeners,
                sine: &context.assets.sine_table().values,
                world_flags: world.world_flags,
            },
        ),
        map: None,
    };
    crate::session_platform::world_actor_behavior::run_selector35_dormant_exit(
        &mut world.arena,
        ship,
        &mut owners,
        &mut world.rng,
    )
}

pub(super) fn drain_world_disposal(
    world: &mut WorldSession,
    context: &mut WorldActorTickContext<'_>,
) {
    let executable = inf_formats::FrontendExecutable::parse(context.assets.executable_bytes())
        .expect("FrontendAssets retains its validated executable");
    let grants = LiveGrantContext {
        process: Some(&*context.factory_process),
        executable: Some(executable),
        resident: Some(context.residency),
        text_data: context.grant_text,
    };
    let report = world.arena.drain_object_disposal(ObjectDisposalRequest {
        mission: &mut world.mission,
        grants: &grants,
        sound: SoundTeardownService::Available {
            sounds: &mut *context.sounds,
            voices: &mut context.voices,
        },
        state: &mut world.actor_updates.disposal,
        failure_policy: ObjectDisposalFailurePolicy::RetainStoppedObjectApproximation,
        listeners: Some(&mut world.listeners),
    });
    match report {
        Ok(report) => {
            for stop in report.newly_stopped {
                eprintln!(
                    "World object disposal stopped: {:?} {:?}: {} (RetainStoppedObjectApproximation)",
                    stop.object, stop.phase, stop.reason,
                );
            }
            if !report.destroyed.is_empty() {
                eprintln!(
                    "World object disposal: clock {}, {} object(s) destroyed: {:?}",
                    context.clock,
                    report.destroyed.len(),
                    report.destroyed,
                );
            }
            if let Some(ship) = report.exit_pending {
                if let Err(reason) = run_dormant_exit(world, context, ship) {
                    eprintln!("World object disposal: {ship:?}'s exit 0043E120 stopped: {reason}");
                }
                // The paused ship's destructor, then the rest of the drain.
                drain_world_disposal(world, context);
            }
        }
        Err(error) => {
            // The kernel retains its stop before returning an error. This
            // diagnostic never authorizes replaying a reached destructor.
            eprintln!("World object disposal pass stopped: {error}");
        }
    }
}
