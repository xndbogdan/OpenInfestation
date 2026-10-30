use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorFollowTarget, ActorHandle, FixedPositionTarget, FurniturePositionTarget, NativeTarget,
        PlayerCommandContext, PlayerId,
    },
    process_sound::ProcessSoundRef,
    terrain_runtime::TerrainSpacing,
    world_sight::Sight,
};
use inf_formats::FrontendExecutable;
use serde_json::Value;

use super::super::drive::{VehicleDriveHost, VehicleDriveRequest, VehicleGround};
use super::super::test_support::loaded;

#[derive(Debug, PartialEq)]
enum Shot {
    Predictive(ActorWeaponFireRequest),
    Queued {
        time: u32,
        direction: [u32; 3],
        fraction: u32,
    },
}

/// Records the shots; nothing else is reachable from the fire region.
#[derive(Default)]
struct FireHost {
    shots: Vec<Shot>,
}

impl VehicleDriveHost for FireHost {
    type Error = String;
    fn play_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Err("sound".into())
    }
    fn create_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: u32,
        _: u32,
        _: u32,
    ) -> Result<ProcessSoundRef, String> {
        Err("sound".into())
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Err("sound".into())
    }
    fn release_sound(&mut self, _: ProcessSoundRef) -> Result<(), String> {
        Err("sound".into())
    }
}

impl VehicleUpdateHost for FireHost {
    fn tow_beam(&mut self, _: u32, _: [u16; 3]) -> Result<(), Self::Error> {
        unreachable!("no tow in this test")
    }
    fn random(&mut self) -> Result<u32, String> {
        Err("random".into())
    }
    fn hull_spray(&mut self, _: &ObjectArena, _: ObjectId, _: i32) -> Result<(), String> {
        Err("hull spray".into())
    }
    fn water_splash(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), Self::Error> {
        Err("water splash".into())
    }
    fn wake_particle(
        &mut self,
        _: crate::object_runtime::WakeParticle,
        _: [u16; 3],
        _: [u16; 3],
    ) -> Result<(), String> {
        Err("wake particle".into())
    }
    fn status_effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: crate::object_runtime::FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err("status".into())
    }
    fn timed_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: crate::object_runtime::ActorTimedDamageRequest,
    ) -> Result<(), String> {
        Err("timed damage".into())
    }
    fn drop_item(&mut self, _: &mut ObjectArena, _: ObjectId, _: usize) -> Result<(), String> {
        Err("drop".into())
    }
    fn carry_item(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        Err("carry".into())
    }
    fn player_text(&mut self, _: &mut ObjectArena, _: PlayerId, _: u16) -> Result<(), String> {
        Err("text".into())
    }
    fn fire_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorWeaponFireRequest,
    ) -> Result<(), String> {
        self.shots.push(Shot::Predictive(request));
        Ok(())
    }
    fn queue_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        time: u32,
        direction: [u32; 3],
        fraction: u32,
    ) -> Result<(), String> {
        self.shots.push(Shot::Queued {
            time,
            direction,
            fraction,
        });
        Ok(())
    }
    fn sight(&mut self, _: &ObjectArena, _: [u16; 3], _: [u16; 3]) -> Result<Sight, String> {
        Err("sight".into())
    }
    fn damage_predicate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        Err("predicate".into())
    }
}

fn words(value: &Value) -> Vec<u32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|word| word.as_u64().unwrap() as u32)
        .collect()
}

fn triple(values: &[u32]) -> [u16; 3] {
    std::array::from_fn(|axis| values[axis] as u16)
}

/// The recorded target (vtable, +04, +08, +10, +14, +1A); type 2's handle is
/// the stand-in's.
fn recorded_target(target: &[u32], stand_in: ActorHandle) -> NativeTarget {
    let focus_08 = triple(&target[2..5]);
    let focus_14 = triple(&target[6..9]);
    match target[0] {
        0x0052_8038 => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: stand_in,
            focus_08: Some(focus_08),
            word_10: target[5],
            focus_14: Some(focus_14),
            velocity_1a: Some(triple(&target[9..12])),
        }),
        0x0052_8060 => NativeTarget::FurniturePosition(FurniturePositionTarget {
            handle_04: ActorHandle::from_raw(target[1]),
            focus_08,
            word_10: target[5],
            focus_14,
        }),
        0x0051_f154 => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: ActorHandle::from_raw(target[1]),
            focus_08: Some(focus_08),
            word_10: target[5],
        }),
        vtable => panic!("unrecorded target class {vtable:08X}"),
    }
}

/// The fire region's receipts from all three recordings
/// (`vehicle_fire_native.json`): 33 primary shots, 9 predictive at a live
/// type-2 target and the rest queued (no target, types 8 and 4, and type 2
/// targets whose refresh answers 1 or less), and 6 charged secondaries.
/// The vehicle is Level 1's with the recorded pose, mode, behavior entry and
/// row; a type-2 target's actor is a stand-in at its refreshed point with
/// its recorded velocity.
#[test]
fn the_vehicle_fires_as_retail_does() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_fire_native.json"
    ))
    .unwrap();
    let (resident, assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let stand_in = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| actor != vehicle)
        .unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let primaries = fixture["primary"].as_array().unwrap();
    let secondaries = fixture["secondary"].as_array().unwrap();
    assert_eq!((primaries.len(), secondaries.len()), (33, 6));
    let mut predictive = 0;
    for (index, case) in primaries.iter().chain(secondaries).enumerate() {
        let primary = index < primaries.len();
        let arena = &mut world.arena;
        let forward = words(&case["forward"]);
        arena.actor_mut(vehicle).unwrap().rotating.basis.words[7..10].copy_from_slice(&forward);
        if let Some(position) = case.get("position") {
            arena.owned_mut(vehicle).unwrap().state.base.position_words =
                Some(triple(&words(position)));
        }
        arena.selector2_actor_mut(vehicle).unwrap().dwords_1e8_1f4[0] =
            case["mode"].as_u64().unwrap() as u32;
        let (selected, entry_index) = if primary {
            let b10 = case["b10"].as_u64().unwrap() as u8;
            (b10, b10)
        } else {
            (
                case["b12"].as_u64().unwrap() as u8,
                case["b11"].as_u64().unwrap() as u8,
            )
        };
        {
            let behavior = arena
                .derived_actor_mut(vehicle)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap();
            behavior.byte_10 = selected;
            behavior.byte_11 = case["b11"].as_u64().unwrap() as u8;
            behavior.entries_18[entry_index as usize] = words(&case["entry"]).try_into().unwrap();
            if primary {
                behavior.word_20 = case["w20"].as_u64().unwrap() as u32;
            }
        }
        if primary {
            let row = words(&case["row"]);
            let table = arena
                .selector2_actor_mut(vehicle)
                .unwrap()
                .table_26c
                .as_mut()
                .unwrap();
            table[selected as usize * 11] = row[0];
            table[selected as usize * 11 + 1] = row[1];
        }
        if let Some(old) = arena.derived_actor_mut(vehicle).unwrap().target_148.take() {
            arena.release_actor_service(old).unwrap();
        }
        let mut controls = PlayerControlBlock::default();
        if primary {
            controls.fire_20 = case["fire"].as_u64().unwrap() as i32;
            let target = case["target"].as_array().map(|_| words(&case["target"]));
            if let (Some(target), Some(after)) = (
                &target,
                case["after"]["target"]
                    .as_array()
                    .map(|_| words(&case["after"]["target"])),
            ) {
                if target[0] == 0x0052_8038 {
                    // The stand-in stands at the refreshed point with the
                    // refreshed velocity, live unless the refresh failed.
                    let base = &mut arena.owned_mut(stand_in).unwrap().state.base;
                    base.position_words = Some(triple(&after[2..5]));
                    base.flags &= !0x0004_0000;
                    let actor = arena.actor_mut(stand_in).unwrap();
                    actor.word_74 = 0;
                    actor.words_8e_92 = triple(&after[9..12]);
                    actor.word_c4 = if case["shot"]["kind"] == "predictive" {
                        3
                    } else {
                        1
                    };
                }
            }
            let handle = arena.actor(stand_in).unwrap().handle;
            let reference =
                target.map(|target| arena.import_fixture_target(recorded_target(&target, handle)));
            arena.derived_actor_mut(vehicle).unwrap().target_148 = reference;
        } else {
            arena.selector2_actor_mut(vehicle).unwrap().dwords_210_218[2] =
                case["w218"].as_u64().unwrap() as u32 + 200_000;
            controls.grenade_24 = case["c20"].as_u64().unwrap() as i32;
            controls.set_byte(0x39, case["c35"].as_u64().unwrap() as u8);
        }
        let request = VehicleUpdateRequest {
            drive: VehicleDriveRequest {
                delta: 0,
                executable,
                ground: VehicleGround {
                    terrain: &world.terrain,
                    world_flags: world.world_flags,
                    clock: 0,
                    sine_quarter: &sine,
                },
                process_null: ActorHandle::from_raw(0),
                position_step: super::super::drive::VehiclePositionStep::Retail,
                view_zoom_5c0: &std::cell::Cell::new(0),
                network: None,
            },
            attract: false,
            current_world: world.mission.current_world_a0,
            status_load_factor: Some(0x10000),
            command: PlayerCommandContext {
                process: Some(&process),
                executable: Some(executable),
                resident: Some(&resident),
            },
            settings_7c: case["s7c"].as_u64().unwrap_or(1) as u32,
            special_view: case["special_view"].as_u64().unwrap_or(0) != 0,
            carry: None,
        };
        let mut host = FireHost::default();
        world
            .arena
            .vehicle_fire(vehicle, &mut controls, &request, &mut host)
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        let arena = &world.arena;
        if primary {
            let shot = &case["shot"];
            let expected = if shot["kind"] == "predictive" {
                predictive += 1;
                Shot::Predictive(ActorWeaponFireRequest {
                    target: arena.derived_actor(vehicle).unwrap().target_148,
                    distance: shot["distance"].as_u64().unwrap() as u32,
                    world: triple(&words(&shot["delta"])).map(|word| word as i16),
                    microseconds: shot["time"].as_u64().unwrap() as u32,
                    mode: ActorWeaponFireMode::Predictive { publish_lead: true },
                })
            } else {
                Shot::Queued {
                    time: shot["time"].as_u64().unwrap() as u32,
                    direction: words(&shot["direction"]).try_into().unwrap(),
                    fraction: shot["fraction"].as_u64().unwrap() as u32,
                }
            };
            assert_eq!(host.shots, [expected], "case {index} shot");
            let behavior = arena
                .derived_actor(vehicle)
                .unwrap()
                .behavior_140
                .as_ref()
                .unwrap();
            assert_eq!(
                u64::from(behavior.word_20),
                case["after"]["w20"].as_u64().unwrap(),
                "case {index} lead"
            );
        } else {
            assert_eq!(
                host.shots,
                [Shot::Queued {
                    time: case["time"].as_u64().unwrap() as u32,
                    direction: words(&case["direction"]).try_into().unwrap(),
                    fraction: case["fraction"].as_u64().unwrap() as u32,
                }],
                "case {index} secondary"
            );
            let behavior = arena
                .derived_actor(vehicle)
                .unwrap()
                .behavior_140
                .as_ref()
                .unwrap();
            assert_eq!(
                u64::from(behavior.byte_10),
                case["b12"].as_u64().unwrap(),
                "case {index} primary restored"
            );
            assert_eq!(arena.selector2_actor(vehicle).unwrap().dwords_210_218[2], 0);
            assert_eq!(controls.byte(0x39), 0);
        }
    }
    assert_eq!(predictive, 9);
}
