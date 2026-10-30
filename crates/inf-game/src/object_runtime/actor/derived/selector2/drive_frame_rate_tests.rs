//! The high frame-rate hang: Level 1's buggy driven with Climb held at
//! fixed scheduler deltas, under the retail position step and the port's
//! carried one. See VEHICLE_DRIVE_RUNTIME.md, "The high frame-rate hang".

use inf_formats::FrontendExecutable;

use super::*;
use crate::process_sound::ProcessSoundRuntime;
use crate::terrain_runtime::TerrainSpacing;

use super::super::test_support::{loaded, NoVoices};

#[test]
fn retail_position_step_drops_each_fraction() {
    let mut position = [100, 200, 300];
    let mut remainder = [0; 3];
    for _ in 0..10 {
        step_vehicle_position(
            &mut position,
            [0, (-24i16) as u16, 300],
            250,
            VehiclePositionStep::Retail,
            &mut remainder,
        );
    }
    // -24 * 250 = -6000 and 300 * 250 = 75000: 0 and 2 units a tick.
    assert_eq!(position, [100, 200, 320]);
    assert_eq!(remainder, [0; 3]);
}

#[test]
fn carried_position_step_keeps_each_fraction() {
    let mut position = [100, 200, 300];
    let mut remainder = [0; 3];
    for _ in 0..10 {
        step_vehicle_position(
            &mut position,
            [0, (-24i16) as u16, 300],
            250,
            VehiclePositionStep::CarryRemainder,
            &mut remainder,
        );
    }
    // -60000 / 0x8000 and 750000 / 0x8000, toward zero, over the ten ticks.
    assert_eq!(position, [100, 199, 322]);
    assert_eq!(remainder, [0, -60_000 + 0x8000, 750_000 - 22 * 0x8000]);
    // The fraction keeps its axis's sign, so a reversal cancels it.
    step_vehicle_position(
        &mut position,
        [0, 24, (-300i16) as u16],
        250,
        VehiclePositionStep::CarryRemainder,
        &mut remainder,
    );
    // -27232 + 6000 stays inside one unit; 29104 - 75000 crosses one back.
    assert_eq!(position, [100, 199, 321]);
    assert_eq!(remainder, [0, -21_232, -13_128]);
}

#[test]
fn the_mode_reset_clears_the_carried_fractions() {
    let (_, _, mut world) = loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    world
        .arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .drive
        .position_remainder = [5, -6, 7];
    world
        .arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .dwords_1e8_1f4 = [1, 0xffff, 1, 0];
    world.arena.reset_vehicle_ground_drive(vehicle).unwrap();
    assert_eq!(
        world
            .arena
            .selector2_actor(vehicle)
            .unwrap()
            .drive
            .position_remainder,
        [0; 3]
    );
}

struct Host<'a> {
    sounds: &'a mut ProcessSoundRuntime,
    resident: &'a crate::ResidentRuntime,
}

impl VehicleDriveHost for Host<'_> {
    type Error = String;
    fn play_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn create_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        sound: u32,
        gain: u32,
        pitch: u32,
    ) -> Result<ProcessSoundRef, String> {
        self.sounds
            .create_persistent(self.resident, sound as i16, gain, pitch, 0, || {
                Ok::<_, std::convert::Infallible>([0, 0, 0])
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Ok(())
    }
    fn release_sound(&mut self, reference: ProcessSoundRef) -> Result<(), String> {
        self.sounds
            .delete_record(reference, &mut NoVoices)
            .map_err(|error| format!("{error:?}"))
    }
}

/// The magnitude 00416A8E takes for +574/+578.
fn magnitude(vector: [i32; 3]) -> u32 {
    let sum = vector.iter().fold(0i32, |sum, value| {
        sum.wrapping_add(value.wrapping_mul(*value))
    });
    if sum as u32 & 0x8000_0000 != 0 {
        isqrt_48efc0((sum as u32 >> 2) as i32) << 1
    } else {
        isqrt_48efc0(sum)
    }
}

#[derive(Debug)]
struct Drive {
    /// Horizontal distance covered, summed per tick.
    path: f64,
    /// The longest run of ticks whose hull height stayed put while its
    /// vertical velocity was not zero and its traction +57C was below half.
    longest_hang: u32,
    /// The velocity WORDs after the last tick.
    velocity: [i16; 3],
    /// The terrain normal under the hull after the last tick.
    normal: [f32; 3],
}

/// Level 1's buggy from the world start, mode 1 settled, with Climb (Up)
/// held every tick and, with `turn_every`, Left held for the last second
/// of every `turn_every` seconds. The update's +574/+578 are refreshed as
/// 00416A62 does before each drive.
fn drive(delta: u32, seconds: u32, turn_every: Option<u32>, step: VehiclePositionStep) -> Drive {
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    let mut host = Host {
        sounds: &mut sounds,
        resident: &resident,
    };
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4 = [1, 0xffff, 1, 0];
    arena.reset_vehicle_ground_drive(vehicle).unwrap();
    let mut elapsed = 0u64;
    let mut last = arena.object(vehicle).unwrap().base.position_words.unwrap();
    let mut path = 0.0;
    let (mut hang, mut longest_hang) = (0, 0);
    while elapsed < u64::from(seconds) * 1_000_000 {
        {
            let velocity = arena.actor(vehicle).unwrap().words_8e_92;
            let state = arena.selector2_actor_mut(vehicle).unwrap();
            let change = std::array::from_fn(|axis| {
                i32::from(velocity[axis].wrapping_sub(state.words_56c_570[axis]) as i16)
            });
            state.drive.dword_574 = Some(magnitude(velocity.map(|word| i32::from(word as i16))));
            state.drive.dword_578 = Some(magnitude(change));
            state.words_56c_570 = velocity;
        }
        let mut controls = PlayerControlBlock {
            pitch_18: -(delta as i32),
            ..PlayerControlBlock::default()
        };
        if let Some(every) = turn_every {
            if (elapsed / 1_000_000) as u32 % every == every - 1 {
                controls.turn_14 = delta as i32;
            }
        }
        let request = VehicleDriveRequest {
            delta,
            executable,
            ground: VehicleGround {
                terrain: &world.terrain,
                world_flags: world.world_flags,
                clock: (elapsed * 48 / 1_000_000) as u32,
                sine_quarter: &sine,
            },
            process_null: ActorHandle::from_raw(0),
            position_step: step,
            view_zoom_5c0: &std::cell::Cell::new(0),
            network: None,
        };
        arena
            .tick_vehicle_ground_drive(vehicle, &mut controls, &request, &mut host)
            .unwrap();
        elapsed += u64::from(delta);
        let position = arena.object(vehicle).unwrap().base.position_words.unwrap();
        let moved = |axis: usize| f64::from(position[axis].wrapping_sub(last[axis]) as i16);
        path += (moved(0) * moved(0) + moved(2) * moved(2)).sqrt();
        let velocity_y = arena.actor(vehicle).unwrap().words_8e_92[1];
        let traction = arena
            .selector2_actor(vehicle)
            .unwrap()
            .drive
            .dword_57c
            .unwrap() as i32;
        if position[1] == last[1] && velocity_y != 0 && traction < 0x8000 {
            hang += 1;
            longest_hang = longest_hang.max(hang);
        } else {
            hang = 0;
        }
        last = position;
    }
    Drive {
        path,
        longest_hang,
        velocity: arena
            .actor(vehicle)
            .unwrap()
            .words_8e_92
            .map(|word| word as i16),
        normal: arena
            .selector2_actor(vehicle)
            .unwrap()
            .drive
            .normal_198
            .unwrap()
            .map(f32::from_bits),
    }
}

/// At 125 ticks a second the retail step leaves the buggy's hull hanging
/// past Level 1's first slope: its height stays put for over two seconds
/// while it falls at 24 units with under half its traction, and it covers a
/// fifth less ground than at 60 ticks or at the recordings' 8.
#[test]
fn retail_step_hangs_the_buggy_at_125_ticks_a_second() {
    let fast = drive(8_000, 10, None, VehiclePositionStep::Retail);
    assert!(fast.longest_hang >= 250, "{fast:?}");
    let sixty = drive(16_666, 10, None, VehiclePositionStep::Retail);
    let recorded = drive(125_000, 10, None, VehiclePositionStep::Retail);
    assert!(sixty.longest_hang < 10, "{sixty:?}");
    assert!(recorded.longest_hang < 10, "{recorded:?}");
    assert!(fast.path < sixty.path * 0.8, "{fast:?} {sixty:?}");
    assert!(fast.path < recorded.path * 0.8, "{fast:?} {recorded:?}");
}

/// The carried step lets the hull settle: no hang, and the distance stays
/// within a tenth of the 60-tick drive's.
#[test]
fn carried_step_keeps_the_buggy_on_the_ground_at_125_ticks_a_second() {
    let fast = drive(8_000, 10, None, VehiclePositionStep::CarryRemainder);
    let sixty = drive(16_666, 10, None, VehiclePositionStep::Retail);
    assert!(fast.longest_hang < 10, "{fast:?}");
    assert!(fast.path > sixty.path * 0.9, "{fast:?} {sixty:?}");
}

/// Two minutes of driving in circles at 100 ticks a second: retail leaves
/// the buggy frozen for good on flat ground after about a minute, falling at
/// 36 units with no horizontal speed and an eighth of its traction, too
/// little for the forward motion to accelerate at all. The carried step keeps
/// it going. Even 60 ticks a second hang for over a second on that course,
/// so capping the frame rate there would not remove the hang.
#[test]
fn retail_step_strands_the_buggy_on_flat_ground_and_the_fix_does_not() {
    let retail = drive(10_000, 120, Some(3), VehiclePositionStep::Retail);
    assert_eq!(retail.velocity, [0, -36, 0], "{retail:?}");
    assert_eq!(retail.normal, [0.0, 1.0, 0.0], "{retail:?}");
    assert!(retail.longest_hang > 5_000, "{retail:?}");
    let fixed = drive(10_000, 120, Some(3), VehiclePositionStep::CarryRemainder);
    assert!(fixed.longest_hang < 50, "{fixed:?}");
    assert_ne!([fixed.velocity[0], fixed.velocity[2]], [0, 0], "{fixed:?}");
    let sixty = drive(16_666, 120, Some(3), VehiclePositionStep::Retail);
    assert!(sixty.longest_hang > 60, "{sixty:?}");
}
