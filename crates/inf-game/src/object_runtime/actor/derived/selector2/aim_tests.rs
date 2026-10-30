use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{ActorFollowTarget, PlayerCommandContext, PlayerId},
    process_sound::ProcessSoundRef,
    terrain_runtime::TerrainSpacing,
};
use inf_formats::FrontendExecutable;
use serde_json::Value;

use super::super::drive::{VehicleDriveHost, VehicleDriveRequest, VehicleGround};
use super::super::test_support::loaded;

/// The recorded sight result; anything else the update host offers is out
/// of the aim's reach.
struct AimHost {
    rng: u32,
    request: Option<([u16; 3], [u16; 3])>,
    sight: Option<Sight>,
    /// The damage predicate's recorded answer; false when none is.
    predicate: Option<bool>,
    predicates: usize,
}

impl VehicleDriveHost for AimHost {
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

impl VehicleUpdateHost for AimHost {
    fn tow_beam(&mut self, _: u32, _: [u16; 3]) -> Result<(), Self::Error> {
        unreachable!("no tow in this test")
    }
    fn random(&mut self) -> Result<u32, String> {
        self.rng = self.rng.wrapping_mul(0x13_d56d).wrapping_add(0x13_d573);
        Ok(self.rng >> 16)
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
        _: crate::object_runtime::ActorWeaponFireRequest,
    ) -> Result<(), String> {
        Err("fire".into())
    }
    fn queue_weapon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: u32,
        _: [u32; 3],
        _: u32,
    ) -> Result<(), String> {
        Err("queue".into())
    }
    fn sight(
        &mut self,
        _: &ObjectArena,
        origin: [u16; 3],
        target: [u16; 3],
    ) -> Result<Sight, String> {
        assert!(self.request.is_none(), "one sight test per aim");
        self.request = Some((origin, target));
        self.sight.ok_or_else(|| "no sight recorded".into())
    }
    fn damage_predicate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        self.predicates += 1;
        Ok(self.predicate.unwrap_or(false))
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

/// The recorded target: pointer, vtable, +04, +08 (three WORDs), +10 and
/// +14 (three WORDs).
fn recorded_target(value: &Value) -> Option<NativeTarget> {
    if value.is_null() {
        return None;
    }
    let target = words(value);
    let handle = ActorHandle::from_raw(target[2]);
    let focus_08 = triple(&target[3..6]);
    let focus_14 = triple(&target[7..10]);
    Some(match target[1] {
        0x0052_8038 => NativeTarget::ActorFollow(ActorFollowTarget {
            handle_04: handle,
            focus_08: Some(focus_08),
            word_10: target[6],
            focus_14: Some(focus_14),
            velocity_1a: None,
        }),
        0x0052_8060 => NativeTarget::FurniturePosition(FurniturePositionTarget {
            handle_04: handle,
            focus_08,
            word_10: target[6],
            focus_14,
        }),
        0x0051_f154 => NativeTarget::FixedPosition(FixedPositionTarget {
            handle_04: handle,
            focus_08: Some(focus_08),
            word_10: target[6],
        }),
        vtable => panic!("unrecorded target class {vtable:08X}"),
    })
}

/// Infest01's 707 aims (`aim_infest01.json`), each from its captured
/// vehicle and target over the Level-1 world: the scan finds nothing, so the
/// look-ahead decides the sight test's request, and its recorded result
/// becomes the new target. Type 2's +08 is heap residue and not compared;
/// its handle is the stand-in hit actor's.
#[test]
fn the_aim_matches_every_infest01_receipt() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/aim_infest01.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 707);
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
    let mut kinds = std::collections::BTreeMap::<u32, usize>::new();
    for (index, case) in cases.iter().enumerate() {
        let arena = &mut world.arena;
        {
            let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
            base.position_words = Some(triple(&words(&case["position"])));
            let flags = case["flags"].as_u64().unwrap() as u32;
            base.flags = (base.flags & !4) | (flags & 4);
            base.dirty_word = case["dirty"].as_u64().unwrap() as u16;
        }
        {
            let actor = arena.actor_mut(vehicle).unwrap();
            actor.rotating.basis.words = words(&case["basis"]).try_into().unwrap();
            actor.word_b4 = case["team"].as_u64().unwrap() as u32;
        }
        {
            let derived = arena.derived_actor_mut(vehicle).unwrap();
            derived.word_164 = case["reach"].as_u64().unwrap() as i32;
            derived.behavior_140.as_mut().unwrap().byte_10 = words(&case["weapon"])[0] as u8;
        }
        {
            let state = arena.selector2_actor_mut(vehicle).unwrap();
            state.byte_1c0 = case["free"].as_u64().unwrap() as u8;
            state.dwords_1e8_1f4[0] = case["mode"].as_u64().unwrap() as u32;
            state.dwords_200_204[0] = case["w200"].as_u64().unwrap() as u32;
        }
        let weapon = words(&case["weapon"]);
        assert_eq!(
            arena.vehicle_weapon_dword::<String>(vehicle, 0x14).unwrap(),
            weapon[1],
            "case {index} spread"
        );
        assert_eq!(
            arena.vehicle_weapon_dword::<String>(vehicle, 0x18).unwrap(),
            weapon[2],
            "case {index} accuracy"
        );
        if let Some(old) = arena.derived_actor_mut(vehicle).unwrap().target_148.take() {
            arena.release_actor_service(old).unwrap();
        }
        let target = recorded_target(&case["target"]).map(|data| arena.import_fixture_target(data));
        arena.derived_actor_mut(vehicle).unwrap().target_148 = target;

        let sight = words(&case["sight"]);
        let after = &case["after"];
        let mut expected = recorded_target(&after["target"]);
        if let Some(NativeTarget::ActorFollow(follow)) = &mut expected {
            follow.handle_04 = arena.actor(stand_in).unwrap().handle;
        }
        let point = triple(&sight[7..10]);
        let hit = match sight[6] {
            0 => None,
            1 => Some(SightHit::Terrain { point }),
            2 => Some(SightHit::Furniture {
                point,
                position: triple(&sight[10..13]),
            }),
            // Retail's handles are not the port's, and two of the three hit
            // actors are attachments born in play: a stand-in actor takes
            // their place, and the target must carry its handle.
            3 => Some(SightHit::Actor {
                point,
                actor: stand_in,
            }),
            kind => panic!("case {index}: sight type {kind}"),
        };
        let mut host = AimHost {
            rng: case["rng"].as_u64().unwrap() as u32,
            request: None,
            sight: Some(Sight {
                hit,
                length: sight[14] as u16,
            }),
            predicate: None,
            predicates: 0,
        };
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
            settings_7c: case["s7c"].as_u64().unwrap() as u32,
            special_view: false,
            carry: None,
        };
        world
            .arena
            .vehicle_aim(vehicle, &request, &mut host)
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        let arena = &world.arena;
        assert_eq!(
            host.request,
            Some((triple(&sight[0..3]), triple(&sight[3..6]))),
            "case {index} sight request"
        );
        let actual = arena
            .derived_actor(vehicle)
            .unwrap()
            .target_148
            .map(|reference| arena.native_target(reference).unwrap());
        match (actual, expected) {
            (None, None) => {}
            (Some(NativeTarget::ActorFollow(port)), Some(NativeTarget::ActorFollow(native))) => {
                assert_eq!(
                    (port.handle_04, port.word_10, port.focus_14),
                    (native.handle_04, native.word_10, native.focus_14),
                    "case {index} type 2"
                );
            }
            (actual, expected) => assert_eq!(actual, expected, "case {index} target"),
        }
        *kinds
            .entry(expected.map_or(0, |target| target.kind_word()))
            .or_default() += 1;
        let base = &arena.object(vehicle).unwrap().base;
        let flags = after["flags"].as_u64().unwrap() as u32;
        assert_eq!(base.flags & 4, flags & 4, "case {index} flag 4");
        assert_eq!(
            u64::from(base.dirty_word),
            after["dirty"].as_u64().unwrap(),
            "case {index} dirty"
        );
        assert_eq!(
            u64::from(host.rng),
            after["rng"].as_u64().unwrap(),
            "case {index} rng"
        );
    }
    assert_eq!(
        kinds.into_iter().collect::<Vec<_>>(),
        [(0, 229), (2, 134), (4, 134), (8, 210)]
    );
}

/// `0048D820`'s callers' tables whose virtual +54 is the actor itself: the
/// factory, gate, bridge and course target.
const KEPT_TABLES: [u32; 4] = [0x0052_1940, 0x0052_89c0, 0x0051_ff54, 0x0052_2398];

/// The scorer (004153A0) over 521 calls from Infest02 and Infest03
/// (`aim_scorer_native.json`): modes 1 to 5, five weapons and their
/// accuracies, current targets and both predicate answers. The vehicle is
/// Level 1's with the recorded pose, side, mode and weapon row; a stand-in
/// candidate whose virtual +54 matches the recorded table takes the
/// recorded position, side and lifecycle.
#[test]
fn the_aim_scorer_matches_its_receipts() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/aim_scorer_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 521);
    let (_resident, _assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let arena = &mut world.arena;
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let actors: Vec<ObjectId> = arena.actor_list().to_vec();
    let stand_in = |kept: bool| {
        actors
            .iter()
            .copied()
            .find(|&actor| actor != vehicle && arena.actor_completion_kept(actor).unwrap() == kept)
            .unwrap()
    };
    let (kept, plain) = (stand_in(true), stand_in(false));
    let mut scored = 0;
    for (index, case) in cases.iter().enumerate() {
        let car = &case["vehicle"];
        let weapon = car["weapon"].as_i64().unwrap() as i8;
        {
            arena.owned_mut(vehicle).unwrap().state.base.position_words =
                Some(triple(&words(&car["position"])));
            let actor = arena.actor_mut(vehicle).unwrap();
            actor.rotating.basis.words = words(&car["basis"]).try_into().unwrap();
            actor.word_b4 = car["side"].as_u64().unwrap() as u32;
            arena
                .derived_actor_mut(vehicle)
                .unwrap()
                .behavior_140
                .as_mut()
                .unwrap()
                .byte_10 = weapon as u8;
            let state = arena.selector2_actor_mut(vehicle).unwrap();
            state.dwords_1e8_1f4[0] = car["mode"].as_u64().unwrap() as u32;
            state.dwords_200_204[0] = car["w200"].as_u64().unwrap() as u32;
            state.table_26c.as_mut().unwrap()[weapon as usize * 11 + 6] =
                car["accuracy"].as_u64().unwrap() as u32;
        }
        let other = &case["candidate"];
        let table = other["vtable"].as_u64().unwrap() as u32;
        let candidate = if KEPT_TABLES.contains(&table) {
            kept
        } else {
            plain
        };
        {
            arena
                .owned_mut(candidate)
                .unwrap()
                .state
                .base
                .position_words = Some(triple(&words(&other["position"])));
            let actor = arena.actor_mut(candidate).unwrap();
            actor.word_b4 = other["side"].as_u64().unwrap() as u32;
            actor.word_c4 = other["lifecycle"].as_u64().unwrap() as u32;
        }
        let recorded = words(&case["evaluator"]);
        let candidate_handle = arena.actor(candidate).unwrap().handle;
        let handle = if recorded[0] == other["handle"].as_u64().unwrap() as u32 {
            candidate_handle
        } else {
            ActorHandle::from_raw(candidate_handle.raw() ^ 0x8000_0000)
        };
        let evaluator = AimEvaluator {
            forward: std::array::from_fn(|axis| f64::from(f32::from_bits(recorded[1 + axis]))),
            handle,
            armed: recorded[4] != 0,
        };
        let mut host = AimHost {
            rng: 0,
            request: None,
            sight: None,
            predicate: case["predicate"].as_u64().map(|answer| answer != 0),
            predicates: 0,
        };
        let score = arena
            .aim_score(
                vehicle,
                &evaluator,
                candidate,
                case["s7c"].as_u64().unwrap() as u32,
                &mut host,
            )
            .unwrap_or_else(|error| panic!("case {index}: {error}"));
        assert_eq!(
            u64::from(score),
            case["score"].as_u64().unwrap(),
            "case {index} ({} call {})",
            case["trace"],
            case["call"]
        );
        assert_eq!(
            host.predicates,
            usize::from(!case["predicate"].is_null()),
            "case {index} predicate"
        );
        scored += usize::from(score != 0);
    }
    assert_eq!(scored, 109);
}

/// 0041FCE8: in the free look the aim casts from 00421D50's eye, 50 above
/// the look's saved position, 7500 along its forward row, and a clear line
/// frees the target as the look-ahead's does.
#[test]
fn the_free_look_aims_from_its_eye() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world_retained(18, TerrainSpacing::Units128);
    let assets: &FrontendAssets = &assets;
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = assets.sine_table().values;
    let player = world.arena.first_player().unwrap();
    let vehicle = world.arena.player_actor(player).unwrap().unwrap();
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let zoom = std::cell::Cell::new(0);
    let mut controls = crate::object_runtime::PlayerControlBlock::default();
    controls.set_byte(0x3f, 1);
    world
        .arena
        .vehicle_free_look::<String>(vehicle, &mut controls, 40_000, &zoom)
        .unwrap();
    let mut host = AimHost {
        rng: 0,
        request: None,
        sight: Some(Sight {
            hit: None,
            length: 0,
        }),
        predicate: None,
        predicates: 0,
    };
    let request = VehicleUpdateRequest {
        drive: VehicleDriveRequest {
            delta: 40_000,
            executable,
            ground: VehicleGround {
                terrain: &world.terrain,
                world_flags: world.world_flags,
                clock: 0,
                sine_quarter: &sine,
            },
            process_null: ActorHandle::from_raw(0),
            position_step: super::super::drive::VehiclePositionStep::Retail,
            view_zoom_5c0: &zoom,
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
        settings_7c: 0,
        special_view: true,
        carry: None,
    };
    world
        .arena
        .vehicle_aim(vehicle, &request, &mut host)
        .unwrap();
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut eye = [None; 3];
    let mut basis = world.arena.actor(vehicle).unwrap().rotating.basis;
    world
        .arena
        .special_view_421d50(vehicle, &mut eye, &mut basis, &math)
        .unwrap();
    let eye = eye.map(Option::unwrap);
    let position = world
        .arena
        .object(vehicle)
        .unwrap()
        .base
        .position_words
        .unwrap();
    assert_eq!(
        eye,
        [position[0], position[1].wrapping_add(50), position[2]]
    );
    let target: [u16; 3] = std::array::from_fn(|axis| {
        let forward = f64::from(f32::from_bits(basis.words[7 + axis]));
        (ftol(forward * 7500.0) as u16).wrapping_add(eye[axis])
    });
    assert_eq!(host.request, Some((eye, target)));
    assert!(world
        .arena
        .derived_actor(vehicle)
        .unwrap()
        .target_148
        .is_none());
}
