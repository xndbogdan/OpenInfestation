//! The pickup scan against Infest01's (the `itemcontrols` query): with the
//! vehicle at (47584, 63, 27848) and +164 2500, the dropped scientist at
//! (47231, 0, 27809), flags 0x42100867, lay 126130 away squared and scored
//! 0x7FFE134D; `00421810` then took it.

use super::*;
use crate::object_runtime::{
    VehicleCarryContext, VehicleCarryEffect, VehicleCarryHost, VehicleDropHost,
};
use crate::process_particles::factories::ParticleFactoryProcessState;
use crate::terrain_runtime::TerrainSpacing;

struct Level1 {
    arena: ObjectArena,
    vehicle: ObjectId,
    scientist: ObjectId,
}

/// Level 1 with every other actor out of the scan (flag 0x80 set) and the
/// vehicle and one scientist where Infest01 had them.
fn level1() -> Level1 {
    let (_, _, world) = super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let scientist = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector12_actor(actor).is_ok())
        .expect("Level 1 has a scientist");
    for actor in arena.actor_list().to_vec() {
        arena.owned_mut(actor).unwrap().state.base.flags |= 0x80;
    }
    arena.derived_actor_mut(vehicle).unwrap().word_164 = 2500;
    let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
    base.position_words = Some([47584, 63, 27848]);
    let base = &mut arena.owned_mut(scientist).unwrap().state.base;
    base.flags = 0x4210_0867;
    base.position_words = Some([47231, 0, 27809]);
    arena.actor_mut(scientist).unwrap().word_b4 = 2;
    Level1 {
        arena,
        vehicle,
        scientist,
    }
}

fn scan(level: &Level1) -> Option<ObjectId> {
    level
        .arena
        .vehicle_pickup_scan::<()>(level.vehicle)
        .unwrap()
}

#[test]
fn the_pickup_finds_infest01s_scientist() {
    let level = level1();
    assert_eq!(scan(&level), Some(level.scientist));
    assert_eq!(
        level
            .arena
            .vehicle_pickup_score::<()>([47584, 63, 27848], level.scientist, 126_130)
            .unwrap(),
        0x7ffe_134d
    );
}

/// The scanner's flags and range, then the filter's team, distance and
/// height refusals.
#[test]
fn the_pickup_refuses_what_retail_refuses() {
    let mut level = level1();
    let flags = |level: &mut Level1, value: u32| {
        level
            .arena
            .owned_mut(level.scientist)
            .unwrap()
            .state
            .base
            .flags = value;
    };
    // Flag 0x80 set, or 0x20 or 0x40 missing.
    for value in [0x4210_08e7, 0x4210_0847, 0x4210_0827] {
        flags(&mut level, value);
        assert_eq!(scan(&level), None, "flags {value:#x}");
    }
    flags(&mut level, 0x4210_0867);
    // Team 0x12.
    level.arena.actor_mut(level.scientist).unwrap().word_b4 = 0x12;
    assert_eq!(scan(&level), None);
    level.arena.actor_mut(level.scientist).unwrap().word_b4 = 2;
    let place = |level: &mut Level1, position: [u16; 3]| {
        level
            .arena
            .owned_mut(level.scientist)
            .unwrap()
            .state
            .base
            .position_words = Some(position);
    };
    // 1500 away in X: inside the scanner's 2500, outside the filter's.
    place(&mut level, [47584 - 1500, 63, 27848]);
    assert_eq!(scan(&level), None);
    place(&mut level, [47584 - 1499, 63, 27848]);
    assert_eq!(scan(&level), Some(level.scientist));
    // 1500 below, then 1499.
    place(&mut level, [47231, 63u16.wrapping_sub(1500), 27809]);
    assert_eq!(scan(&level), None);
    place(&mut level, [47231, 63u16.wrapping_sub(1499), 27809]);
    assert_eq!(scan(&level), Some(level.scientist));
    // The nearer of two wins.
    let other = level
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| actor != level.scientist && actor != level.vehicle)
        .unwrap();
    let base = &mut level.arena.owned_mut(other).unwrap().state.base;
    base.flags = 0x60;
    base.position_words = Some([47584, 63, 27848 + 100]);
    level.arena.actor_mut(other).unwrap().word_b4 = 2;
    assert_eq!(scan(&level), Some(other));
}

/// Refuses every service: the hold reaches none of them for carried
/// scientists without status timers.
struct NoServices;

impl super::super::drive::VehicleDriveHost for NoServices {
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
    ) -> Result<crate::process_sound::ProcessSoundRef, String> {
        Err("sound".into())
    }
    fn update_sound(
        &mut self,
        _: &ObjectArena,
        _: ObjectId,
        _: crate::process_sound::ProcessSoundRef,
        _: u32,
        _: u32,
    ) -> Result<(), String> {
        Err("sound".into())
    }
    fn release_sound(&mut self, _: crate::process_sound::ProcessSoundRef) -> Result<(), String> {
        Err("sound".into())
    }
}

impl VehicleUpdateHost for NoServices {
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
        effect: crate::object_runtime::FamilyUpdateEffect,
    ) -> Result<(), String> {
        Err(format!("status {effect:?}"))
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
    fn player_text(
        &mut self,
        _: &mut ObjectArena,
        _: crate::object_runtime::PlayerId,
        _: u16,
    ) -> Result<(), String> {
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
        _: [u16; 3],
        _: [u16; 3],
    ) -> Result<crate::world_sight::Sight, String> {
        Err("sight".into())
    }
    fn damage_predicate(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: ObjectId,
    ) -> Result<bool, String> {
        Err("damage predicate".into())
    }
}

struct Hold {
    arena: ObjectArena,
    vehicle: ObjectId,
    scientists: Vec<ObjectId>,
}

const NULL: ActorHandle = ActorHandle::from_raw(0);

/// Level 1 with `count` scientists carried (state 2, captured by the
/// vehicle) and the vehicle at `health`.
fn carrying(count: usize, health: u16) -> Hold {
    let (_, _, world) = super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let source = arena.actor(vehicle).unwrap().handle;
    let scientists: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector12_actor(actor).is_ok())
        .take(count)
        .collect();
    assert_eq!(scientists.len(), count);
    for (index, &scientist) in scientists.iter().enumerate() {
        arena.owned_mut(scientist).unwrap().state.base.flags = 0x4810_08e0;
        arena.actor_mut(scientist).unwrap().capture_handle_70 = Some(source);
        let handle = arena.actor(scientist).unwrap().handle;
        let slots = &mut arena.selector2_actor_mut(vehicle).unwrap().handles_1a4;
        slots.handles[index] = handle;
        slots.states[index] = Some(2);
    }
    let actor = arena.actor_mut(vehicle).unwrap();
    assert_eq!(actor.maximum_health, Some(5000));
    actor.health = health;
    Hold {
        arena,
        vehicle,
        scientists,
    }
}

fn tick(hold: &mut Hold, delta: u32) -> Result<(), String> {
    hold.arena
        .tick_vehicle_hold(
            hold.vehicle,
            delta,
            NULL,
            Some(0x10000),
            None,
            &mut NoServices,
        )
        .map_err(|error| error.to_string())
}

fn health(hold: &Hold) -> u16 {
    hold.arena.actor(hold.vehicle).unwrap().health
}

/// Infest02's and Infest03's hold ticks (the `holdtick` queries), each at
/// 125000 µs: 7 per scientist, no higher than 5000, and from 0 too.
#[test]
fn carried_scientists_repair_the_vehicle() {
    for (count, before, after) in [
        (1, 4820, 4827),
        (2, 4841, 4855),
        (3, 4869, 4890),
        (3, 4995, 5000),
        (3, 0, 21),
        (3, 4792, 4813),
    ] {
        let mut hold = carrying(count, before);
        tick(&mut hold, 125_000).unwrap();
        assert_eq!(health(&hold), after, "{count} scientists from {before}");
    }
    // Below 2^14 µs a scientist still repairs 1.
    let mut hold = carrying(2, 100);
    tick(&mut hold, 0x3fff).unwrap();
    assert_eq!(health(&hold), 102);
}

/// Infest01's carried scientists stood at the vehicle after each tick (the
/// `holdtick` query at E084BA:17F2).
#[test]
fn carried_actors_take_the_vehicles_position() {
    let mut hold = carrying(2, 5000);
    let position = [47219, 53, 26734];
    hold.arena
        .owned_mut(hold.vehicle)
        .unwrap()
        .state
        .base
        .position_words = Some(position);
    tick(&mut hold, 125_000).unwrap();
    assert_eq!(health(&hold), 5000);
    for &scientist in &hold.scientists {
        let base = &hold.arena.object(scientist).unwrap().base;
        assert_eq!(base.position_words, Some(position));
        assert_eq!(base.flags, 0x4810_08e0);
    }
    let slots = &hold
        .arena
        .selector2_actor(hold.vehicle)
        .unwrap()
        .handles_1a4;
    assert_eq!(slots.states, [Some(2), Some(2), None]);
}

/// A slot whose actor another captor holds is released, and the hold
/// compacts: the next actor moves down with its state, and the slot it
/// left keeps its state behind its NULL handle (00411F60).
#[test]
fn the_hold_releases_what_the_vehicle_no_longer_carries() {
    let mut hold = carrying(2, 5000);
    let [first, second] = [hold.scientists[0], hold.scientists[1]];
    hold.arena.actor_mut(first).unwrap().capture_handle_70 = Some(ActorHandle::from_raw(0x1234));
    tick(&mut hold, 125_000).unwrap();
    let second_handle = hold.arena.actor(second).unwrap().handle;
    let slots = &hold
        .arena
        .selector2_actor(hold.vehicle)
        .unwrap()
        .handles_1a4;
    assert_eq!(slots.handles, [second_handle, NULL, NULL]);
    assert_eq!(slots.states, [Some(2), Some(2), None]);
    // Only the kept scientist repaired.
    assert_eq!(hold.arena.actor(hold.vehicle).unwrap().health, 5000);

    // A remote take whose capture names the vehicle becomes carried; a
    // remote drop the vehicle no longer captures leaves its slot.
    let mut hold = carrying(2, 4000);
    hold.arena
        .selector2_actor_mut(hold.vehicle)
        .unwrap()
        .handles_1a4
        .states[0] = Some(1);
    let source = hold.arena.actor(hold.vehicle).unwrap().handle;
    hold.arena
        .selector2_actor_mut(hold.vehicle)
        .unwrap()
        .handles_1a4
        .states[1] = Some(3);
    hold.arena
        .actor_mut(hold.scientists[1])
        .unwrap()
        .capture_handle_70 = Some(NULL);
    tick(&mut hold, 125_000).unwrap();
    let slots = &hold
        .arena
        .selector2_actor(hold.vehicle)
        .unwrap()
        .handles_1a4;
    let first_handle = hold.arena.actor(hold.scientists[0]).unwrap().handle;
    assert_eq!(slots.handles, [first_handle, NULL, NULL]);
    assert_eq!(slots.states, [Some(2), Some(0), None]);
    // The promoted slot follows without its +90: no repair.
    assert_eq!(hold.arena.actor(hold.vehicle).unwrap().health, 4000);
    assert_eq!(
        hold.arena
            .actor(hold.scientists[0])
            .unwrap()
            .capture_handle_70,
        Some(source)
    );

    // The network arms stop the port.
    let mut hold = carrying(1, 4000);
    hold.arena
        .selector2_actor_mut(hold.vehicle)
        .unwrap()
        .handles_1a4
        .states[0] = Some(1);
    hold.arena
        .actor_mut(hold.scientists[0])
        .unwrap()
        .capture_handle_70 = Some(NULL);
    assert!(tick(&mut hold, 125_000).unwrap_err().contains("00412468"));
}

/// Records the take's and the drop's services; a dropped rig's call to the
/// mines gets `mine_answer`.
#[derive(Default)]
struct Carrier {
    effects: usize,
    attached: Vec<ObjectId>,
    mines: Vec<(ObjectId, u32)>,
    mine_answer: u32,
}

impl VehicleCarryHost for Carrier {
    type Error = String;
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: VehicleCarryEffect,
        _: &mut u32,
    ) -> Result<(), String> {
        self.effects += 1;
        Ok(())
    }
}

impl VehicleDropHost for Carrier {
    fn call_mines(
        &mut self,
        _: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, String> {
        self.mines.push((rig, radius));
        Ok(self.mine_answer)
    }
    fn select_targets(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: &mut u32,
    ) -> Result<(), String> {
        Err("only a scientist's release searches".into())
    }
    fn attach_effect(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        _: &mut u32,
    ) -> Result<(), String> {
        self.attached.push(actor);
        Ok(())
    }
}

/// Infest04 carries Helniazine's ECM Unit twice (`ecm-pickups.js`). Where
/// it took it the first time (`7857E9`), the pickup scan finds the unit,
/// the take (00421810 through 0042D9E0) names the vehicle its holder, the
/// hold tick (0042DA60) gives the vehicle the ECM flag 0x40000 (its flags
/// went from 0x4A106937 to 0x4A146937), and the drop (0042D9E0's release)
/// takes the flag away and forgets the holder.
#[test]
fn the_vehicle_carries_helniazines_ecm_unit() {
    // The retained load lets the placements it disposes die.
    let (runtime, assets, world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let terrain = world.terrain;
    let mut arena = world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    let ecm = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector73(actor).is_ok())
        .expect("Helniazine places the ECM Unit (placement 161)");
    for actor in arena.actor_list().to_vec() {
        if actor != ecm {
            arena.owned_mut(actor).unwrap().state.base.flags |= 0x80;
        }
    }
    {
        let base = &mut arena.owned_mut(ecm).unwrap().state.base;
        base.flags = 0x4a98_0167;
        base.position_words = Some([47875, 64288, 30567]);
    }
    arena.derived_actor_mut(vehicle).unwrap().word_164 = 2500;
    {
        let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
        base.flags = 0x4a10_6937;
        base.position_words = Some([47750, 64401, 30634]);
    }
    assert_eq!(arena.vehicle_pickup_scan::<()>(vehicle).unwrap(), Some(ecm));
    let process = ParticleFactoryProcessState::native_startup();
    let context = VehicleCarryContext {
        process: &process,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        resident: &runtime,
        text_data: None,
    };
    let mut carrier = Carrier::default();
    let mut rng = 0;
    assert!(arena
        .attach_vehicle_item(vehicle, ecm, context, &mut carrier, &mut rng)
        .unwrap());
    let source = arena.actor(vehicle).unwrap().handle;
    assert_eq!(arena.selector73(ecm).unwrap().holder_144, source);
    // The slot's effect and the three attach sounds.
    assert_eq!(carrier.effects, 4);
    arena
        .tick_vehicle_hold(vehicle, 125_000, null, Some(0x10000), None, &mut NoServices)
        .unwrap();
    assert_eq!(arena.object(vehicle).unwrap().base.flags, 0x4a14_6937);
    // The carried unit rides with the vehicle.
    assert_eq!(
        arena.object(ecm).unwrap().base.position_words,
        Some([47750, 64401, 30634])
    );
    arena
        .drop_vehicle_item(vehicle, 0, &terrain, context, &mut carrier, &mut rng)
        .unwrap();
    assert_eq!(arena.selector73(ecm).unwrap().holder_144, null);
    assert_eq!(arena.object(vehicle).unwrap().base.flags, 0x4a10_6937);
    assert_eq!(carrier.attached, [ecm]);
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.handles[0],
        null
    );
}

/// A Helniazine play (2026-10-18): beaming up a portable mining
/// rig stopped the vehicle, whose take owned no rig's +8C. 004547E0 refuses
/// a rig that holds a level, with the first player's hint 0x18; an empty one
/// gives its loader hint 0x1F and the rig's grant 0x23 before the shared
/// body takes it. Dropped, it calls the mines within 0x400 (0047DD20(0x400,
/// 0xF, 0)), and an answer starts it filling.
#[test]
fn the_vehicle_carries_a_mining_rig() {
    let (runtime, assets, world) =
        crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let terrain = world.terrain;
    let mut arena = world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    let player = arena.first_player().unwrap();
    let rigs: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector52(actor).is_ok())
        .collect();
    assert_eq!(rigs.len(), 4, "Helniazine places four rigs");
    let rig = rigs[3];
    let process = ParticleFactoryProcessState::native_startup();
    // The rig's grant 0x23 announces itself, formatting grant text.
    let text = crate::grant_text::GrantTextData {
        null_substitute: None,
        pair_classification: Some(crate::grant_text::GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let context = VehicleCarryContext {
        process: &process,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        resident: &runtime,
        text_data: Some(&text),
    };
    let owned = |arena: &ObjectArena, id: u32| {
        arena.world_player(player).unwrap().command_mask_130[(id / 32) as usize] & 1 << (id % 32)
            != 0
    };
    let knows_rig = |arena: &ObjectArena| {
        arena.world_player(player).unwrap().grants_9c[1] & 1 << (0x23 - 32) != 0
    };
    let mut carrier = Carrier::default();
    let mut rng = 0;
    arena.selector52_mut(rig).unwrap().level_140 = 0x100;
    assert!(!arena
        .attach_vehicle_item(vehicle, rig, context, &mut carrier, &mut rng)
        .unwrap());
    assert!(owned(&arena, 0x18) && !owned(&arena, 0x1f));
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.handles[0],
        null
    );
    arena.selector52_mut(rig).unwrap().level_140 = 0;
    assert!(!knows_rig(&arena));
    assert!(arena
        .attach_vehicle_item(vehicle, rig, context, &mut carrier, &mut rng)
        .unwrap());
    assert!(owned(&arena, 0x1f) && knows_rig(&arena));
    let handle = arena.actor(rig).unwrap().handle;
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.handles[0],
        handle
    );
    // Dropped where no mine answers, it stays empty; where one does, it
    // fills.
    arena
        .drop_vehicle_item(vehicle, 0, &terrain, context, &mut carrier, &mut rng)
        .unwrap();
    assert_eq!(carrier.mines, [(rig, 0x400)]);
    assert_eq!(arena.selector52(rig).unwrap().filling_142, 0);
    assert_eq!(carrier.attached, [rig]);
    carrier.mine_answer = 1;
    assert!(arena
        .attach_vehicle_item(vehicle, rig, context, &mut carrier, &mut rng)
        .unwrap());
    arena
        .drop_vehicle_item(vehicle, 0, &terrain, context, &mut carrier, &mut rng)
        .unwrap();
    assert_eq!(arena.selector52(rig).unwrap().filling_142, 1);
}

/// A carried Bomb, mining rig or mine keeps the shared +90 (004026C0,
/// AL 1): the hold keeps it, it takes the vehicle's position, and nothing
/// repairs the vehicle. A vehicle stopped here (0041243F) after
/// beaming up a Bomb, and again after beaming up a mining rig.
#[test]
fn the_hold_keeps_carried_items_with_the_shared_90() {
    let (_, _, world) = crate::test_worlds::offline_world_retained(6, TerrainSpacing::Units128);
    let first = |arena: &ObjectArena, test: &dyn Fn(&ObjectArena, ObjectId) -> bool| {
        arena
            .actor_list()
            .iter()
            .copied()
            .find(|&actor| test(arena, actor))
            .expect("Helniazine places Bombs and mining rigs")
    };
    let mut arena = world.arena;
    let bomb = first(&arena, &|arena, actor| arena.is_bomb29(actor));
    let rig = first(&arena, &|arena, actor| arena.selector52(actor).is_ok());
    let sine = [0; 4096];
    let mine = arena
        .allocate_behavior_resource(
            crate::object_runtime::ActorConstructionRequest {
                allocation_position: Some([0; 3]),
                owner: 1,
                constructor_null:
                    crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                terrain: crate::object_runtime::ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: Some(0),
                    sine_quarter: &sine,
                },
            },
            38,
        )
        .unwrap();
    let player = arena.first_player().unwrap();
    let vehicle = arena.player_actor(player).unwrap().unwrap();
    let source = arena.actor(vehicle).unwrap().handle;
    let items = [bomb, rig, mine];
    let handles = items.map(|item| {
        arena.actor_mut(item).unwrap().capture_handle_70 = Some(source);
        arena.actor(item).unwrap().handle
    });
    let slots = &mut arena.selector2_actor_mut(vehicle).unwrap().handles_1a4;
    for (index, handle) in handles.iter().enumerate() {
        slots.handles[index] = *handle;
        slots.states[index] = Some(2);
    }
    let health = arena.actor(vehicle).unwrap().health;
    let mut hold = Hold {
        arena,
        vehicle,
        scientists: Vec::new(),
    };
    tick(&mut hold, 125_000).unwrap();
    let position = hold.arena.object(vehicle).unwrap().base.position_words;
    let slots = &hold.arena.selector2_actor(vehicle).unwrap().handles_1a4;
    for (index, item) in items.iter().enumerate() {
        assert_eq!(
            hold.arena.object(*item).unwrap().base.position_words,
            position
        );
        assert_eq!(
            (slots.handles[index], slots.states[index]),
            (handles[index], Some(2))
        );
    }
    assert_eq!(hold.arena.actor(vehicle).unwrap().health, health);
}

/// `00412468` and `004123F4`: another machine's actor in the hold. Taken
/// (state 1) while no record names its holder, it is captured again into
/// its slot and message 8 tells its owner; once a record names the vehicle
/// it is carried (state 2). Let go (state 3) while it still names the
/// vehicle, message 9 tells its owner and it rides on; once a record names
/// no holder, its slot empties.
#[test]
fn another_machines_actor_in_the_hold_tells_its_owner() {
    use crate::network::protocol::GameSend;
    let (runtime, assets, world) = super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let source = arena.actor(vehicle).unwrap().handle;
    let scientist = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| arena.selector12_actor(actor).is_ok())
        .unwrap();
    let handle = arena.actor(scientist).unwrap().handle;
    let identity = arena.actor(vehicle).unwrap().word_6c;
    arena.actor_mut(scientist).unwrap().word_6c = identity;
    arena.owned_mut(scientist).unwrap().state.base.flags = 0xc810_0860;
    arena.actor_mut(scientist).unwrap().capture_handle_70 = Some(NULL);
    {
        let slots = &mut arena.selector2_actor_mut(vehicle).unwrap().handles_1a4;
        slots.handles[0] = handle;
        slots.states[0] = Some(1);
    }
    let process = ParticleFactoryProcessState::native_startup();
    // The take announces the scientist, formatting grant text.
    let text = crate::grant_text::GrantTextData {
        null_substitute: None,
        pair_classification: Some(crate::grant_text::GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let context = VehicleCarryContext {
        process: &process,
        executable: inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        resident: &runtime,
        text_data: Some(&text),
    };
    let tick = |arena: &mut ObjectArena| {
        arena
            .tick_vehicle_hold(
                vehicle,
                125_000,
                NULL,
                Some(0x10000),
                Some(context),
                &mut NoServices,
            )
            .unwrap();
        arena.take_network_sends()
    };
    let pair = GameSend::Pair([source.raw(), handle.raw()]);
    assert_eq!(tick(&mut arena), [(8, pair.clone())]);
    let slots = &arena.selector2_actor(vehicle).unwrap().handles_1a4;
    assert_eq!(
        slots.saved_flags[0].bytes,
        Some(0xc810_0860_u32.to_le_bytes().map(Some))
    );
    assert_eq!((slots.handles[0], slots.states[0]), (handle, Some(1)));
    let position = |arena: &ObjectArena, actor| arena.object(actor).unwrap().base.position_words;
    assert_eq!(position(&arena, scientist), position(&arena, vehicle));

    arena.actor_mut(scientist).unwrap().capture_handle_70 = Some(source);
    assert_eq!(tick(&mut arena), []);
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.states[0],
        Some(2)
    );

    arena
        .selector2_actor_mut(vehicle)
        .unwrap()
        .handles_1a4
        .states[0] = Some(3);
    assert_eq!(tick(&mut arena), [(9, pair)]);
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.handles[0],
        handle
    );

    arena.actor_mut(scientist).unwrap().capture_handle_70 = Some(NULL);
    assert_eq!(tick(&mut arena), []);
    assert_eq!(
        arena.selector2_actor(vehicle).unwrap().handles_1a4.handles[0],
        NULL
    );
}
