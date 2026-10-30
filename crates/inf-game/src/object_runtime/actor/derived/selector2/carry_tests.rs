//! The drop against Infest01's (the `itemcontrols`, `dropspawn`, `dropbefore`
//! and `dropafter` queries). The vehicle at (47681, 63, 27848), radius +28
//! 109, carried scientists in slots 0 and 1 (saved flags 0x48100867, state
//! 2) and dropped slot 1's, radius 132: 459 along the forward row from +44
//! (-0.99606, 0.00668, 0.08437) put it at (47224, 0, 27886). The release
//! and the attachment's first update (4051C0) left its flags 0x48100840,
//! and the slot held the process NULL with state 0 and its saved flags.

use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    object_runtime::{
        ActorAttachmentHost, ActorConstructionRequest, ActorConstructorNull, ActorTerrainInputs,
        Selector66Host, Selector66Particle, TargetNullHandleSource,
    },
    terrain_runtime::TerrainSpacing,
    world_backend::WorldSession,
};

const VEHICLE_POSITION: [u16; 3] = [47681, 63, 27848];
/// The vehicle's +44 (its rotating basis) at the drop.
const VEHICLE_BASIS: [u32; 10] = [
    0x30,
    0x3dac_9b22,
    0x3c2d_1ad5,
    0x3f7e_fbb7,
    0x3bbc_8d6d,
    0x3f7f_fa7e,
    0xbc35_b9bc,
    0xbf7e_fdd6,
    0x3bda_f751,
    0x3dac_cdca,
];
const SAVED_FLAGS: u32 = 0x4810_0867;
const CARRIED_FLAGS: u32 = 0x4810_08e0;

fn saved(flags: u32) -> ActorCaptureSavedFlags {
    ActorCaptureSavedFlags {
        bytes: Some(flags.to_le_bytes().map(Some)),
    }
}

struct Level1 {
    runtime: ResidentRuntime,
    assets: FrontendAssets,
    world: WorldSession,
    vehicle: ObjectId,
    kept: ObjectId,
    dropped: ObjectId,
}

/// Level 1 with two of its scientists in the hold as Infest01 had them.
fn level1() -> Level1 {
    let (runtime, assets, mut world) =
        super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let mut scientists = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector12_actor(actor).is_ok());
    let (kept, dropped) = (scientists.next().unwrap(), scientists.next().unwrap());
    let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
    base.position_words = Some(VEHICLE_POSITION);
    base.word_28 = 109;
    arena.actor_mut(vehicle).unwrap().rotating.basis.words = VEHICLE_BASIS;
    arena.actor_mut(vehicle).unwrap().word_6c = 0;
    for scientist in [kept, dropped] {
        let base = &mut arena.owned_mut(scientist).unwrap().state.base;
        base.flags = CARRIED_FLAGS;
        base.position_words = Some(VEHICLE_POSITION);
        base.word_28 = 132;
        let actor = arena.actor_mut(scientist).unwrap();
        actor.words_8e_92 = [0; 3];
        actor.word_6c = 0;
        actor.word_b4 = 2;
    }
    let handles = [kept, dropped].map(|actor| arena.actor(actor).unwrap().handle);
    let slots = &mut arena.selector2_actor_mut(vehicle).unwrap().handles_1a4;
    for (index, handle) in handles.into_iter().enumerate() {
        slots.handles[index] = handle;
        slots.saved_flags[index] = saved(SAVED_FLAGS);
        slots.states[index] = Some(2);
    }
    Level1 {
        runtime,
        assets,
        world,
        vehicle,
        kept,
        dropped,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Effect(VehicleCarryEffect),
    Search(ObjectId),
    Attach(ObjectId),
    Created(ObjectId),
    Refresh(ObjectId),
}

/// Records the drop's services; the attachment allocates a bare selector 66
/// and runs its real first update.
struct DropHost {
    null: ActorHandle,
    events: Vec<Event>,
}

impl VehicleCarryHost for DropHost {
    type Error = String;
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        effect: VehicleCarryEffect,
        _: &mut u32,
    ) -> Result<(), String> {
        self.events.push(Event::Effect(effect));
        Ok(())
    }
}

impl VehicleDropHost for DropHost {
    fn call_mines(&mut self, _: &mut ObjectArena, rig: ObjectId, _: u32) -> Result<u32, String> {
        Err(format!("only a rig's drop calls the mines, not {rig:?}'s"))
    }
    fn select_targets(
        &mut self,
        _: &mut ObjectArena,
        actor: ObjectId,
        _: &mut u32,
    ) -> Result<(), String> {
        self.events.push(Event::Search(actor));
        Ok(())
    }
    fn attach_effect(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        rng: &mut u32,
    ) -> Result<(), String> {
        self.events.push(Event::Attach(actor));
        arena
            .attach_actor_effect(Some(actor), self, rng)
            .map_err(|error| error.to_string())
    }
}

impl TargetNullHandleSource for DropHost {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
}

impl ActorAttachmentHost for DropHost {
    type Error = String;
    fn create_attachment(
        &mut self,
        arena: &mut ObjectArena,
        _: ObjectId,
        _: ActorHandle,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        let attachment = arena
            .allocate_selector66(ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: None,
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &[0; 4096],
                },
            })
            .map_err(|error| error.to_string())?;
        self.events.push(Event::Created(attachment));
        Ok(Some(attachment))
    }
    fn resolve_attachment_actor(
        &mut self,
        _: &mut ObjectArena,
        raw: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        Ok(Some(raw))
    }
    fn update_attachment(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        microseconds: u32,
        rng: &mut u32,
    ) -> Result<(), String> {
        arena
            .tick_selector66(actor, microseconds, self, rng)
            .map_err(|error| error.to_string())
    }
}

impl Selector66Host for DropHost {
    type Error = String;
    fn refresh_model(&mut self, _: &mut ObjectArena, actor: ObjectId) -> Result<(), String> {
        self.events.push(Event::Refresh(actor));
        Ok(())
    }
    fn ground_height(&mut self, _: &mut ObjectArena, _: [u16; 2]) -> Result<u16, String> {
        Err("the first update samples no ground".into())
    }
    fn particle(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        _: Selector66Particle,
        _: &mut u32,
    ) -> Result<(), String> {
        Err("the first update births no particle".into())
    }
}

fn drop_slot(level: &mut Level1, index: usize) -> (DropHost, Result<(), String>) {
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let executable = FrontendExecutable::parse(level.assets.executable_bytes()).unwrap();
    let context = VehicleCarryContext {
        process: &process,
        executable,
        resident: &level.runtime,
        text_data: None,
    };
    let mut host = DropHost {
        null: process.process_null_handle.unwrap(),
        events: Vec::new(),
    };
    let mut rng = 0;
    let result = level
        .world
        .arena
        .drop_vehicle_item(
            level.vehicle,
            index,
            &level.world.terrain,
            context,
            &mut host,
            &mut rng,
        )
        .map_err(|error| error.to_string());
    (host, result)
}

#[test]
fn the_drop_lands_infest01s_scientist_beside_the_vehicle() {
    let mut level = level1();
    let (host, result) = drop_slot(&mut level, 1);
    result.unwrap();
    let arena = &level.world.arena;
    let landed = [47224, 0, 27886];
    let base = &arena.object(level.dropped).unwrap().base;
    assert_eq!(base.position_words, Some(landed));
    assert_eq!(base.flags, 0x4810_0840);
    let actor = arena.actor(level.dropped).unwrap();
    assert_eq!(actor.words_8e_92, [0; 3]);
    assert_eq!(actor.capture_handle_70, Some(host.null));
    assert_eq!(
        arena.family428650_actor(level.dropped).unwrap().word_22e & 0xc,
        0xc
    );

    let [Event::Effect(flash), Event::Search(searched), Event::Attach(attached), Event::Created(attachment), Event::Refresh(refreshed)] =
        host.events.as_slice()
    else {
        panic!("{:?}", host.events);
    };
    assert_eq!(
        *flash,
        VehicleCarryEffect::Particle {
            selector: 2,
            position: landed,
            flags: 0,
        }
    );
    assert_eq!([*searched, *attached], [level.dropped; 2]);
    assert_eq!(refreshed, attachment);
    // 00405420 hands the attachment the dropped actor as owner and team;
    // its first update saved the owner's released flags.
    let child = arena.actor(*attachment).unwrap();
    assert_eq!(child.owner_handle, actor.handle);
    assert_eq!(child.word_b4, 2);
    assert_eq!(
        arena.selector66(*attachment).unwrap().saved_flags_138,
        Some(SAVED_FLAGS)
    );

    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    let kept = arena.actor(level.kept).unwrap().handle;
    assert_eq!(slots.handles, [kept, host.null, host.null]);
    assert_eq!(slots.states, [Some(2), Some(0), None]);
    assert_eq!(slots.saved_flags[..2], [saved(SAVED_FLAGS); 2]);
}

/// 00412110 passes over remote slots (state 3) and leaves a remote actor
/// where it put it, marking its slot 3; an unwritten state stops the port.
#[test]
fn the_drop_keeps_remote_actors_in_their_slots() {
    let mut level = level1();
    let remote = CARRIED_FLAGS | 0x8000_0000;
    level
        .world
        .arena
        .owned_mut(level.kept)
        .unwrap()
        .state
        .base
        .flags = remote;
    let (host, result) = drop_slot(&mut level, 0);
    result.unwrap();
    let arena = &level.world.arena;
    let base = &arena.object(level.kept).unwrap().base;
    assert_eq!(base.flags, remote);
    assert_eq!(base.position_words, Some([47224, 0, 27886]));
    assert!(matches!(host.events.as_slice(), [Event::Effect(_)]));
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    assert_eq!(slots.states, [Some(3), Some(2), None]);

    // The next drop from slot 0 passes the remote slot and takes slot 1.
    let (_, result) = drop_slot(&mut level, 0);
    result.unwrap();
    let slots = &level
        .world
        .arena
        .selector2_actor(level.vehicle)
        .unwrap()
        .handles_1a4;
    assert_eq!(slots.states, [Some(3), Some(0), None]);

    let (_, result) = drop_slot(&mut level, 2);
    assert!(result.unwrap_err().contains("unwritten slot 2 state"));
}

/// Infest02's takes of Level 1's team-1 scientists (the `teamchange`
/// query): each joined team 2 through 00468C40, which set +22E bit 0,
/// dropped the +1EC target (score 0x17D784) and walked the team's
/// family-428650 actors, Level 1's two selector-34 actors among them,
/// through 00482F90. Their tables were empty in Infest02; here one guard
/// remembers the scientist behind another actor, and forgets it.
#[test]
fn a_team_1_scientist_joins_the_vehicles_team() {
    let (runtime, assets, mut world) =
        super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &mut world.arena;
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let null = process.process_null_handle.unwrap();
    let scientist = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena.selector12_actor(actor).is_ok() && arena.actor(actor).unwrap().word_b4 == 1
        })
        .unwrap();
    let guards: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| {
            arena.actor(actor).unwrap().word_b4 == 2
                && matches!(
                    &arena.family428650_actor(actor).map(|family| &family.class),
                    Ok(crate::object_runtime::Family428650Class::Selector34(_))
                )
        })
        .collect();
    assert_eq!(guards.len(), 2);
    arena
        .publish_family_target(
            scientist,
            crate::object_runtime::FamilyTargetSlot::Target1ec,
            crate::object_runtime::WeightedTargetRequest {
                referent: crate::object_runtime::WeightedTargetReferent::Actor(Some(vehicle)),
                score: 0x17_d784,
                constructor_null: crate::object_runtime::WeightedTargetConstructorNull::Process(
                    &process,
                ),
            },
        )
        .unwrap();
    let handle = arena.actor(scientist).unwrap().handle;
    let other = arena.actor(guards[1]).unwrap().handle;
    let family = arena.family428650_actor_mut(guards[0]).unwrap();
    family.count_1a0 = 2;
    family.entries_1a4[0] = crate::object_runtime::FamilyHandleEntry {
        handle: other,
        word_04: 5,
        word_08: 6,
    };
    family.entries_1a4[1] = crate::object_runtime::FamilyHandleEntry {
        handle,
        word_04: 7,
        word_08: 8,
    };

    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    // The capture merges the scientist's grants into the player, whose
    // notice formats grant text.
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: Some(crate::grant_text::GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let context = VehicleCarryContext {
        process: &process,
        executable,
        resident: &runtime,
        text_data: Some(&text),
    };
    let mut host = DropHost {
        null,
        events: Vec::new(),
    };
    let mut rng = 0;
    assert!(arena
        .attach_vehicle_item(vehicle, scientist, context, &mut host, &mut rng)
        .unwrap());

    let actor = arena.actor(scientist).unwrap();
    assert_eq!(actor.word_b4, 2);
    assert_eq!(Some(actor.health), actor.maximum_health);
    assert_eq!(arena.object(scientist).unwrap().base.dirty_word & 2, 2);
    let family = arena.family428650_actor(scientist).unwrap();
    assert_eq!(family.word_22e & 1, 1);
    assert_eq!(family.count_1a0, 0);
    assert_eq!(
        arena
            .family_weighted_target(
                scientist,
                crate::object_runtime::FamilyTargetSlot::Target1ec
            )
            .unwrap(),
        crate::object_runtime::NativeWeightedTarget::new(None, 0)
    );
    let guard = arena.family428650_actor(guards[0]).unwrap();
    assert_eq!(guard.count_1a0, 1);
    assert_eq!(guard.entries_1a4[0].handle, other);
    assert_eq!(guard.entries_1a4[1].handle, null);
    assert_eq!(
        [guard.entries_1a4[1].word_04, guard.entries_1a4[1].word_08],
        [7, 8]
    );
}

struct Boarding {
    runtime: ResidentRuntime,
    assets: FrontendAssets,
    world: WorldSession,
    vehicle: ObjectId,
    /// Level 1's four loose scientists, all team 1.
    scientists: Vec<ObjectId>,
}

fn boarding() -> Boarding {
    let (runtime, assets, world) = super::super::test_support::loaded(18, TerrainSpacing::Units128);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let arena = &world.arena;
    let scientists: Vec<ObjectId> = arena
        .actor_list()
        .iter()
        .copied()
        .filter(|&actor| arena.selector12_actor(actor).is_ok())
        .collect();
    assert_eq!(scientists.len(), 4);
    assert!(scientists
        .iter()
        .all(|&scientist| arena.actor(scientist).unwrap().word_b4 == 1));
    Boarding {
        runtime,
        assets,
        world,
        vehicle,
        scientists,
    }
}

fn board(level: &mut Boarding, scientist: ObjectId) -> (DropHost, bool) {
    let process =
        crate::process_particles::factories::ParticleFactoryProcessState::native_startup();
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: Some(crate::grant_text::GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let context = VehicleCarryContext {
        process: &process,
        executable: FrontendExecutable::parse(level.assets.executable_bytes()).unwrap(),
        resident: &level.runtime,
        text_data: Some(&text),
    };
    let mut host = DropHost {
        null: process.process_null_handle.unwrap(),
        events: Vec::new(),
    };
    let mut rng = 0;
    let boarded = level
        .world
        .arena
        .board_vehicle(scientist, level.vehicle, context, &mut host, &mut rng)
        .unwrap();
    (host, boarded)
}

fn player_holds_grants(level: &Boarding, scientist: ObjectId) -> bool {
    let arena = &level.world.arena;
    let crate::object_runtime::Family429e20Class::Selector12(state) =
        &arena.family429e20_actor(scientist).unwrap().class
    else {
        panic!("not selector 12");
    };
    let player = arena.world_player(level.world.player.unwrap()).unwrap();
    state
        .grants_24c
        .mask_04
        .iter()
        .zip(&player.grants_9c)
        .all(|(&grants, &held)| grants & held == grants)
}

/// 00468160: a team-1 scientist touching the vehicle merges its grants into
/// the player, then the take carries it and moves it to team 2.
#[test]
fn a_scientist_boards_the_vehicle_it_touches() {
    let mut level = boarding();
    let scientist = level.scientists[0];
    let (host, boarded) = board(&mut level, scientist);
    assert!(boarded);
    assert!(player_holds_grants(&level, scientist));
    let arena = &level.world.arena;
    assert_eq!(arena.actor(scientist).unwrap().word_b4, 2);
    let slots = &arena.selector2_actor(level.vehicle).unwrap().handles_1a4;
    assert_eq!(slots.handles[0], arena.actor(scientist).unwrap().handle);
    assert_eq!(slots.states[0], Some(2));
    let [Event::Effect(VehicleCarryEffect::Particle { selector: 2, .. }), Event::Effect(VehicleCarryEffect::Sound(_)), Event::Effect(VehicleCarryEffect::Sound(_)), Event::Effect(VehicleCarryEffect::Sound(_))] =
        host.events.as_slice()
    else {
        panic!("{:?}", host.events);
    };
}

/// Infest02's one boarding (the `boarding` query at 3F5584:9C8): a team-1
/// scientist touched the vehicle with three scientists aboard. The grants
/// merged and event 22 went out, but the take refused, and the scientist
/// stayed team 1.
#[test]
fn a_full_hold_refuses_a_boarding_scientist() {
    let mut level = boarding();
    let handles: Vec<ActorHandle> = level.scientists[1..]
        .iter()
        .map(|&actor| level.world.arena.actor(actor).unwrap().handle)
        .collect();
    let slots = &mut level
        .world
        .arena
        .selector2_actor_mut(level.vehicle)
        .unwrap()
        .handles_1a4;
    for (index, handle) in handles.into_iter().enumerate() {
        slots.handles[index] = handle;
        slots.states[index] = Some(2);
    }
    let scientist = level.scientists[0];
    let (host, boarded) = board(&mut level, scientist);
    assert!(!boarded);
    assert!(host.events.is_empty());
    assert!(player_holds_grants(&level, scientist));
    assert_eq!(level.world.arena.actor(scientist).unwrap().word_b4, 1);
}

/// 00411E70 against `check_hold_rotate.py`: Level 1's vehicle with the
/// receipt's slots, whose handles resolve to nothing, so the compaction
/// leaves them where they are and the first N turn, holes included.
#[test]
fn the_hold_turns_as_00411e70_does() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/hold_rotate_native.json"
    ))
    .unwrap();
    let (_, _, world) =
        super::super::test_support::loaded(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut arena = world.arena;
    let null = arena.actor_handles().unwrap().null_handle();
    let receipt_null = fixture["null"].as_u64().unwrap() as u32;
    let handle = |raw: u32| {
        if raw == receipt_null {
            null
        } else {
            ActorHandle::from_raw(raw)
        }
    };
    for case in fixture["cases"].as_array().unwrap() {
        let slots: Vec<[u32; 3]> = serde_json::from_value(case["input"]["slots"].clone()).unwrap();
        let hold = &mut arena.selector2_actor_mut(vehicle).unwrap().handles_1a4;
        hold.handles = slots.iter().map(|slot| handle(slot[0])).collect();
        hold.saved_flags = slots
            .iter()
            .map(|slot| ActorCaptureSavedFlags {
                bytes: Some(slot[1].to_le_bytes().map(Some)),
            })
            .collect();
        hold.states = slots.iter().map(|slot| Some(slot[2])).collect();
        let forward = case["input"]["forward"].as_u64().unwrap() & 0xff != 0;
        arena
            .rotate_vehicle_slots::<DerivedActorError>(vehicle, forward, null)
            .unwrap();
        let expected: Vec<[u32; 3]> = serde_json::from_value(case["output"].clone()).unwrap();
        let hold = &arena.selector2_actor(vehicle).unwrap().handles_1a4;
        let actual: Vec<[u32; 3]> = (0..hold.handles.len())
            .map(|index| {
                let raw = hold.handles[index];
                [
                    if raw == null { receipt_null } else { raw.raw() },
                    u32::from_le_bytes(hold.saved_flags[index].bytes.unwrap().map(Option::unwrap)),
                    hold.states[index].unwrap(),
                ]
            })
            .collect();
        assert_eq!(actual, expected, "{case}");
    }
}
