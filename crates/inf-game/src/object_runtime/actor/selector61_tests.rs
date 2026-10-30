//! The Pursuit Ship's construction, death and destruction. Its update
//! matched retail's in the lab over Artoria's and the Defence Satellite's
//! first 120 ticks, every RNG step included (lab README, world sweep).

use super::*;
use crate::object_runtime::{
    ActorAllocationPolicy, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorTerrainInputs, ObjectDestructionRequest, SoundTeardownService,
};

const SINE: [u16; 4096] = [0; 4096];

fn arena(policy: ActorAllocationPolicy) -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    arena.select_actor_allocation_policy(policy).unwrap();
    arena
}

fn ship(arena: &mut ObjectArena, rng: &mut u32) -> ObjectId {
    arena
        .allocate_selector61(
            ActorConstructionRequest {
                constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                allocation_position: Some([0, 0, 0]),
                owner: 1,
                terrain: ActorTerrainInputs {
                    terrain: None,
                    coordinate_shift: 8,
                    clock: 0,
                    world_flags: None,
                    sine_quarter: &SINE,
                },
            },
            rng,
        )
        .unwrap()
}

/// 004128C0 draws once for the keep distance, (draw & 0x7F) + 0x50, and
/// builds 0048CC70(2, 1, 0, 0052F748) with +04 = 10000, +08 = 150000.
#[test]
fn construction_draws_the_keep_distance_and_builds_the_weapon() {
    for (policy, bank_up) in [
        (ActorAllocationPolicy::Unowned, None),
        (
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
            Some(0),
        ),
    ] {
        let mut arena = arena(policy);
        let mut rng = 0x1234_5678;
        let ship = ship(&mut arena, &mut rng);
        let mut expected_rng = 0x1234_5678;
        let draw = crate::name_entry::name_entry_rng_next(&mut expected_rng);
        assert_eq!(rng, expected_rng, "one draw");
        let state = arena.selector61(ship).unwrap();
        assert_eq!(state.keep_144, (draw & 0x7f) + 0x50);
        assert_eq!(
            (state.bank_up_13c, state.bank_140, state.speed_14c),
            (bank_up, 0, 0)
        );
        assert!(state.target_148.is_none());
        let behavior = state.behavior_138.as_ref().unwrap();
        assert_eq!(
            (
                behavior.byte_0d,
                behavior.byte_0e,
                behavior.byte_0f,
                behavior.byte_11,
                behavior.byte_0c
            ),
            (2, 1, 0, 1, 1)
        );
        assert_eq!((behavior.word_04, behavior.word_08), (10_000, 150_000));
        assert_eq!(behavior.table_14, ActorBehaviorTable::Selector61);
        assert_eq!(behavior.entries_18, [[u32::MAX, 0, 0, 0, 0, 0, 0, 0]]);
        assert_eq!(behavior.groups_1c.len(), 2);
        assert!(arena.actor_behavior_presence_7c(ship).unwrap());
    }
}

#[derive(Default)]
struct Death(Vec<String>);

impl Selector61DeathHost for Death {
    type Error = String;
    fn visual(&mut self, _: &mut ObjectArena, _: ObjectId, resource: u32) -> Result<(), String> {
        self.0.push(format!("visual {resource:#x}"));
        Ok(())
    }
    fn burst(&mut self, _: &mut ObjectArena, burst: ParticleBurst) -> Result<(), String> {
        self.0.push(format!(
            "burst {} {:?} {} {} {:?} {} {}",
            burst.debris_count,
            burst.position,
            burst.placement_radius,
            burst.central_type,
            burst.debris_types,
            burst.speed,
            burst.flags
        ));
        Ok(())
    }
    fn blast(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        block_address: u32,
        source: crate::object_runtime::ActorHandle,
    ) -> Result<(), String> {
        self.0
            .push(format!("blast {block_address:08X} {:08x}", source.raw()));
        Ok(())
    }
    fn explosion(
        &mut self,
        _: &mut ObjectArena,
        particle_type: u32,
        position: [u16; 3],
    ) -> Result<(), String> {
        self.0
            .push(format!("explosion {particle_type} {position:?}"));
        Ok(())
    }
}

/// 00412FA0: the visual 0x37, the burst, the blast unless flag bit 31, the
/// type-1 explosion, then the disposal.
#[test]
fn the_death_runs_in_the_original_order() {
    for flags in [0, 0x8000_0000_u32] {
        let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
        let ship = ship(&mut arena, &mut 1);
        arena.place_for_tests(ship, [100, 200, 300]);
        let base = &mut arena.owned_mut(ship).unwrap().state.base;
        base.flags |= flags;
        let handle = arena.actor(ship).unwrap().handle;
        let mut host = Death::default();
        arena.retire_selector61(ship, &mut host).unwrap();
        let mut expected = vec![
            "visual 0x37".to_owned(),
            "burst 16 [100, 200, 300] 20 1 [1, 1] 8 0".to_owned(),
        ];
        if flags == 0 {
            expected.push(format!("blast 00531250 {:08x}", handle.raw()));
        }
        expected.push("explosion 1 [100, 200, 300]".to_owned());
        assert_eq!(host.0, expected);
        assert!(arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .any(|node| node.object == ship));
    }
}

/// 00412990 deletes the target +148 and tears the weapon +138 down before
/// the shared teardown.
#[test]
fn destruction_releases_the_target_and_the_weapon() {
    let mut arena = arena(ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation);
    let unit = ship(&mut arena, &mut 1);
    let ship = ship(&mut arena, &mut 1);
    let handle = arena.actor(unit).unwrap().handle;
    let target = arena
        .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
            handle,
        ))
        .unwrap()
        .unwrap();
    arena.selector61_mut(ship).unwrap().target_148 = Some(target);
    arena
        .destroy(ObjectDestructionRequest {
            object: ship,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.selector61(ship).is_err());
    assert!(arena.native_target(target).is_err(), "released target");
}
