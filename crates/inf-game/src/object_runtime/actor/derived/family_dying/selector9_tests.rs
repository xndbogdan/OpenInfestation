//! Selector 9's two deaths on their own loaded worlds. The shared entry
//! 00424280 and stage 00423EE0 read the payload's dying kind +174 (payload
//! +4C) and flags +264 (payload +6C). A fuel truck (kind 2) dies in its
//! fuel's blast; an assault craft (kind 0, flags 0xA0) ejects one unit, its
//! pilot, toward the player and explodes again at stage 1.

use super::*;
use crate::terrain_runtime::TerrainSpacing;
use inf_formats::FrontendExecutable;

struct Recorder<'e> {
    executable: FrontendExecutable<'e>,
    null: ActorHandle,
    player: [u16; 3],
    stages: Vec<u32>,
    effects: Vec<FamilyDyingEffect>,
    spawns: Vec<FamilyDyingSpawnRequest>,
}

impl TargetNullHandleSource for Recorder<'_> {
    fn target_null_handle(&self) -> Option<ActorHandle> {
        Some(self.null)
    }
}

impl FamilyDyingHost for Recorder<'_> {
    type Error = String;
    fn read_dword(&mut self, address: u32) -> Result<u32, String> {
        self.executable
            .word(address)
            .map_err(|error| error.to_string())
    }
    fn stage_change(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        entering: u32,
        _: &mut u32,
    ) -> Result<(), String> {
        self.stages.push(entering);
        Ok(())
    }
    fn effect(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        effect: FamilyDyingEffect,
        _: &mut u32,
    ) -> Result<(), String> {
        self.effects.push(effect);
        Ok(())
    }
    fn sound_present(&mut self, _: i16) -> Result<bool, String> {
        Ok(true)
    }
    fn player_position(&mut self, _: &mut ObjectArena) -> Result<Option<[u16; 3]>, String> {
        Ok(Some(self.player))
    }
    fn create_death_object(
        &mut self,
        _: &mut ObjectArena,
        request: FamilyDyingSpawnRequest,
        _: &mut u32,
    ) -> Result<Option<ObjectId>, String> {
        self.spawns.push(request);
        Ok(None)
    }
    fn resolve_spawned_actor(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<ObjectId>, String> {
        Ok(None)
    }
    fn actor_basis(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
    ) -> Result<NativeBasis, String> {
        Ok(arena
            .actor(actor)
            .map_err(|error| error.to_string())?
            .rotating
            .basis)
    }
    fn rotate_pickup_vector(&mut self, _: &mut [u32; 3], _: u32, _: u32) -> Result<(), String> {
        Ok(())
    }
}

/// The selector 9 actor placed at ground `x, z` on loaded world `world`,
/// its death recorded through the entry and then stage 1. The player
/// stands 3000 across and 4000 along from it, 5000 away.
fn die(
    world: u32,
    at: [u16; 2],
) -> (
    ObjectId,
    [u16; 3],
    Recorder<'static>,
    Vec<FamilyDyingEffect>,
    ObjectArena,
) {
    let (_, assets, session) =
        crate::test_worlds::offline_world_retained(world, TerrainSpacing::Units256);
    let mut arena = session.arena;
    let actor = arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            arena.family4236d0_actor(actor).is_ok_and(|state| {
                matches!(state.class, crate::object_runtime::Family4236d0Class::Base)
            }) && arena
                .object(actor)
                .ok()
                .and_then(|object| object.base.position_words)
                .is_some_and(|[x, _, z]| [x, z] == at)
        })
        .expect("the placement");
    let position = arena.object(actor).unwrap().base.position_words.unwrap();
    let assets: &'static crate::frontend_assets::FrontendAssets = Box::leak(Box::new(assets));
    let mut recorder = Recorder {
        executable: FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
        null: arena.actor_handles().unwrap().null_handle(),
        player: [
            position[0].wrapping_add(3000),
            position[1],
            position[2].wrapping_add(4000),
        ],
        stages: Vec::new(),
        effects: Vec::new(),
        spawns: Vec::new(),
    };
    let mut rng = 0x1357_9bdf;
    arena
        .run_family_dying(
            actor,
            FamilyDyingRequest::Change { entering: 1 },
            &mut recorder,
            &mut rng,
        )
        .unwrap();
    let entry = std::mem::take(&mut recorder.effects);
    // The update's 7.6 s timer moves +240 to the next stage.
    arena.family4236d0_actor_mut(actor).unwrap().word_240 = 1;
    arena
        .run_family_dying(
            actor,
            FamilyDyingRequest::Stage { entering: 1 },
            &mut recorder,
            &mut rng,
        )
        .unwrap();
    (actor, position, recorder, entry, arena)
}

fn pickups(effects: &[FamilyDyingEffect]) -> usize {
    effects
        .iter()
        .filter(|effect| {
            matches!(
                effect,
                FamilyDyingEffect::Pickup {
                    kind: 0x83,
                    value: 500,
                    ..
                }
            )
        })
        .count()
}

fn burst16(effect: &FamilyDyingEffect) -> bool {
    matches!(effect, FamilyDyingEffect::Burst(burst)
        if burst.debris_count == 16 && burst.debris_types == [1, 1])
}

/// Oolbryte's supply truck (placement 98): kind 2 brings the camera effect
/// 0xF00 and the fuel blast 005307D8 before the death sound 0x28, the
/// burst and the blast 00531250, as Infest04 recorded its death; it
/// ejects nobody, and stage 1 only drops three pickups worth 500.
#[test]
fn a_fuel_truck_dies_in_its_fuel_blast_and_ejects_nobody() {
    let (truck, _, recorder, entry, arena) = die(20, [37202, 20987]);
    let source = arena.actor(truck).unwrap().handle;
    assert_eq!(arena.derived_actor(truck).unwrap().word_174, Some(2));
    assert_eq!(recorder.stages, [1]);
    assert_eq!(
        arena.family428650_actor(truck).unwrap().word_228,
        0x0053_0d3c
    );
    assert_eq!(entry.len(), 5, "{entry:?}");
    assert_eq!(
        entry[0],
        FamilyDyingEffect::CameraEffect {
            strength: 0xf00,
            argument: 0
        }
    );
    assert_eq!(
        entry[1],
        FamilyDyingEffect::Radius {
            block_address: 0x0053_07d8,
            source
        }
    );
    assert_eq!(
        entry[2],
        FamilyDyingEffect::Audio(FamilyDyingAudio::OneShot {
            sound: 0x28,
            volume: 65_536,
            pitch: 65_536
        })
    );
    assert!(burst16(&entry[3]), "{:?}", entry[3]);
    assert_eq!(
        entry[4],
        FamilyDyingEffect::Radius {
            block_address: 0x0053_1250,
            source
        }
    );
    assert!(recorder.spawns.is_empty());
    assert_eq!(pickups(&recorder.effects), 3);
    assert_eq!(recorder.effects.len(), 3, "{:?}", recorder.effects);
}

/// Terra Nova's assault craft (placement 83): kind 0 skips the fuel blast,
/// and its flags 0xA0 eject one unit, resource 2 (a scientist) for its
/// team 2, at its size +28 (240) from it toward the player. Stage 1 drops
/// the three pickups and then explodes again (bit 0x20).
#[test]
fn an_assault_craft_ejects_its_pilot_and_explodes_twice() {
    let (craft, position, recorder, entry, arena) = die(2, [26154, 23827]);
    let source = arena.actor(craft).unwrap().handle;
    assert_eq!(arena.derived_actor(craft).unwrap().word_174, Some(0));
    assert_eq!(arena.family4236d0_actor(craft).unwrap().word_264, 0xa0);
    assert_eq!(arena.actor(craft).unwrap().word_b4, 2);
    assert_eq!(arena.object(craft).unwrap().base.word_28, 240);
    assert_eq!(recorder.stages, [1]);
    assert_eq!(entry.len(), 3, "{entry:?}");
    assert_eq!(
        entry[0],
        FamilyDyingEffect::Audio(FamilyDyingAudio::OneShot {
            sound: 0x28,
            volume: 65_536,
            pitch: 65_536
        })
    );
    assert!(burst16(&entry[1]), "{:?}", entry[1]);
    assert_eq!(
        entry[2],
        FamilyDyingEffect::Radius {
            block_address: 0x0053_1250,
            source
        }
    );
    // 240 toward a player 5000 away along (3000, 0, 4000): (144, 0, 192).
    assert_eq!(recorder.spawns.len(), 1);
    assert_eq!(recorder.spawns[0].resource, 2);
    assert_eq!(
        recorder.spawns[0].position,
        [
            position[0].wrapping_add(144),
            position[1],
            position[2].wrapping_add(192)
        ]
    );
    assert_eq!(pickups(&recorder.effects), 3);
    assert_eq!(recorder.effects.len(), 5, "{:?}", recorder.effects);
    assert!(burst16(&recorder.effects[3]), "{:?}", recorder.effects[3]);
    assert_eq!(
        recorder.effects[4],
        FamilyDyingEffect::Radius {
            block_address: 0x0053_1250,
            source
        }
    );
}
