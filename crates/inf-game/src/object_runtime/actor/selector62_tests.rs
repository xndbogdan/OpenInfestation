//! Selector 62's construction, its states' arithmetic, its light and its
//! destruction, with values worked by hand from 00464050 and 004647E0.

use super::*;
use crate::object_runtime::{
    ActorAllocationPolicy, ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig,
    ActorTerrainInputs, ObjectDestructionRequest, ObjectList, SoundTeardownService,
};

const SINE: [u16; 4096] = [0; 4096];
const POINT: [u16; 3] = [10_000, 500, 20_000];

fn arena() -> ObjectArena {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 4,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    arena
        .select_actor_allocation_policy(
            ActorAllocationPolicy::FreshHeapZeroExtendedCarryApproximation,
        )
        .unwrap();
    arena
}

fn construction() -> ActorConstructionRequest<'static> {
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
    }
}

fn attack(arena: &mut ObjectArena) -> ObjectId {
    let attack = arena.allocate_selector62(construction()).unwrap();
    arena.place_for_tests(attack, [1000, 0, 2000]);
    attack
}

/// An attack in `state` with `clock` left, its point at [`POINT`] and five
/// ribbons.
fn attack_in(arena: &mut ObjectArena, state: u32, clock: u32) -> ObjectId {
    let attack = attack(arena);
    let ribbons =
        std::array::from_fn(|_| Some(arena.allocate_selector67(ActorHandle::from_raw(0))));
    let fields = arena.selector62_mut(attack).unwrap();
    fields.state_138 = state;
    fields.clock_13c = clock;
    fields.point_140 = Some(POINT);
    fields.ribbons_164 = ribbons;
    attack
}

#[derive(Default)]
struct Calls(Vec<String>);

impl Selector62Host for Calls {
    type Error = String;
    fn process_null(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn settings_7c(&self) -> u32 {
        0
    }
    fn create_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ActorHandle,
    ) -> Result<Option<ObjectId>, String> {
        self.0.push("ribbon".into());
        Ok(None)
    }
    fn extend_ribbon(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        delta: u32,
        point: [u16; 3],
    ) -> Result<(), String> {
        self.0.push(format!("extend {delta} {point:?}"));
        Ok(())
    }
    fn spawn(
        &mut self,
        _: &mut ObjectArena,
        resource: u32,
        position: [u16; 3],
    ) -> Result<(), String> {
        self.0.push(format!("spawn {resource:#x} {position:?}"));
        Ok(())
    }
    fn blast(
        &mut self,
        _: &mut ObjectArena,
        block_address: u32,
        position: [u16; 3],
        _: ActorHandle,
    ) -> Result<(), String> {
        self.0
            .push(format!("blast {block_address:08X} {position:?}"));
        Ok(())
    }
    fn flash(&mut self, _: &mut ObjectArena, _: ObjectId, radius: u32) -> Result<(), String> {
        self.0.push(format!("flash {radius:#x}"));
        Ok(())
    }
}

/// 00463F60 after 0047A7A0.
#[test]
fn construction_writes_the_states_fields() {
    let mut arena = arena();
    let attack = attack(&mut arena);
    let state = arena.selector62(attack).unwrap();
    assert_eq!((state.state_138, state.clock_13c), (0, 0));
    assert_eq!((state.reach_14c, state.lift_150), (4000, 4000));
    assert_eq!((state.blasts_158, state.may_target_160), (0, 1));
    assert_eq!(
        (state.point_140, state.offset_146, state.blast_154),
        (None, None, None)
    );
    assert!(state.target_15c.is_none());
    assert_eq!(state.ribbons_164, [None; 5]);
    assert!(!arena.actor_behavior_presence_7c(attack).unwrap());
}

/// 00464170 with no unit to aim from: flag 8 on and 0x40 off, a fixed
/// target at the attack itself, then the hold at 004642B4.
#[test]
fn without_the_unit_the_aim_holds_at_the_residue() {
    let mut arena = arena();
    let attack = attack(&mut arena);
    let result = arena.tick_selector62(attack, 33_333, &mut Calls::default());
    assert!(matches!(
        result,
        Err(Selector62Error::ResidueDirection { .. })
    ));
    let state = arena.selector62(attack).unwrap();
    assert_eq!((state.state_138, state.clock_13c), (1, 3_000_000));
    assert_eq!(state.point_140, Some([1000, 0, 2000]));
    let target = arena.native_target(state.target_15c.unwrap()).unwrap();
    assert!(matches!(
        target,
        crate::object_runtime::NativeTarget::FixedPosition(data) if data.focus_08 == Some([1000, 0, 2000])
    ));
    let flags = arena.object(attack).unwrap().base.flags;
    assert_eq!(flags & 0x48, 8);
}

/// BrainRibbonAimApproximation: the brain's attack enters state 1 with no
/// unit to aim from (00405FC0 clears +160), so its ribbons' cross aims along
/// the lab's WORD and the delta's low WORD. These are retail's offsets from
/// `brain-attack-aim.json`, at its 125 and 16 ms steps, with the brain where
/// it stands in world 19.
#[test]
fn the_brains_attack_aims_as_the_lab_found() {
    const BRAIN: [u16; 3] = [0x8061, 0xfd36, 0x65fd];
    for (delta, offset) in [
        (125_000, [2356, 4000, -3231_i16 as u16]),
        (16_000, [1066, 4000, 3855]),
    ] {
        let mut arena = arena();
        let brain = arena.allocate_selector85(construction()).unwrap();
        arena.place_for_tests(brain, BRAIN);
        arena.selector62_mut(brain).unwrap().may_target_160 = 0;
        let mut calls = Calls::default();
        arena.tick_selector62(brain, delta, &mut calls).unwrap();
        let state = arena.selector62(brain).unwrap();
        assert_eq!(
            (state.state_138, state.offset_146, state.point_140),
            (1, Some(offset), Some(BRAIN))
        );
        assert_eq!(calls.0, ["ribbon"; 5]);
    }
}

/// 00464631 halfway through state 1: isqrt(1500000) = 1224 and
/// isqrt(3000000) = 1732 lift the ribbons 1224 * 4000 / 1732 + 150 = 2976;
/// the offset scales by (1500000 >> 10) / (3000000 >> 10) = 1464 / 2929.
#[test]
fn state_1_swings_the_ribbons_round_the_point() {
    let mut arena = arena();
    let attack = attack_in(&mut arena, 1, 1_533_333);
    arena.selector62_mut(attack).unwrap().offset_146 = Some([3000, 4000, (-2000_i16) as u16]);
    let mut calls = Calls::default();
    arena.tick_selector62(attack, 33_333, &mut calls).unwrap();
    // x = 3000 * 1464 / 2929 = 1499, z = -2000 * 1464 / 2929 = -999.
    assert_eq!(
        calls.0,
        [
            "extend 33333 [10000, 3476, 20000]",
            "extend 33333 [11499, 3476, 19001]",
            "extend 33333 [8501, 3476, 20999]",
            "extend 33333 [10999, 3476, 21499]",
            "extend 33333 [9001, 3476, 18501]",
        ]
    );
}

/// 0046453F: the ribbons fall to (976 * 180) / 1953 - 30 = 59 above the
/// point, and a clock below +154 blasts: resource 0x38 at the fallen
/// point, the next block at the point itself, and the flash.
#[test]
fn state_2_draws_the_ribbons_in_and_blasts() {
    let mut arena = arena();
    let attack = attack_in(&mut arena, 2, 1_033_333);
    let fields = arena.selector62_mut(attack).unwrap();
    fields.blast_154 = Some(1_200_001);
    fields.blasts_158 = 1;
    let mut calls = Calls::default();
    arena.tick_selector62(attack, 33_333, &mut calls).unwrap();
    let mut expected = vec!["extend 33333 [10000, 559, 20000]".to_owned(); 5];
    expected.extend([
        "spawn 0x38 [10000, 559, 20000]".to_owned(),
        "blast 0053982A [10000, 500, 20000]".to_owned(),
        "flash 0xf00".to_owned(),
    ]);
    assert_eq!(calls.0, expected);
    let state = arena.selector62(attack).unwrap();
    assert_eq!((state.blast_154, state.blasts_158), (Some(700_001), 2));

    // At +154 or above, no blast.
    let mut calls = Calls::default();
    arena.tick_selector62(attack, 33_333, &mut calls).unwrap();
    assert_eq!(calls.0.len(), 5);
}

/// 004640C3 and 00464406: leaving state 2 spawns 0x36 at the point, blasts
/// the blocks not yet used and flashes; entering 3 disposes the ribbons
/// and deletes the target.
#[test]
fn leaving_state_2_spends_the_blocks_and_entering_3_clears_up() {
    let mut arena = arena();
    let attack = attack_in(&mut arena, 2, 10);
    let ribbons = arena.selector62(attack).unwrap().ribbons_164;
    let handle = arena.actor(attack).unwrap().handle;
    let target = arena
        .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
            handle,
        ))
        .unwrap()
        .unwrap();
    let fields = arena.selector62_mut(attack).unwrap();
    fields.blasts_158 = 2;
    fields.target_15c = Some(target);
    let mut calls = Calls::default();
    // A target in state 3 refreshes only in states 1 and 2.
    arena.tick_selector62(attack, 33_333, &mut calls).unwrap();
    assert_eq!(
        calls.0,
        [
            "spawn 0x36 [10000, 500, 20000]",
            "blast 00539834 [10000, 500, 20000]",
            "blast 0053983E [10000, 500, 20000]",
            "flash 0xf00",
        ]
    );
    let state = arena.selector62(attack).unwrap();
    assert_eq!(
        (state.state_138, state.clock_13c, state.blasts_158),
        (3, 2_000_000, 4)
    );
    assert_eq!(state.ribbons_164, [None; 5]);
    assert!(state.target_15c.is_none());
    assert!(arena.native_target(target).is_err(), "deleted target");
    for ribbon in ribbons.into_iter().flatten() {
        assert!(arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == ribbon));
    }
}

/// 004640A0: flag 0x80 sends state 0 to 4, which clears flag 8 and sets
/// 0x40; 0046452D then restarts state 4's clock every update.
#[test]
fn flag_0x80_holds_the_attack_in_state_4() {
    let mut arena = arena();
    let attack = attack(&mut arena);
    arena.owned_mut(attack).unwrap().state.base.flags |= 0x88;
    for _ in 0..2 {
        arena
            .tick_selector62(attack, 33_333, &mut Calls::default())
            .unwrap();
        let state = arena.selector62(attack).unwrap();
        assert_eq!((state.state_138, state.clock_13c), (4, 2_000_000));
    }
    assert_eq!(arena.object(attack).unwrap().base.flags & 0xc8, 0xc0);
}

/// 004647E0 by state, at the point's X and Z.
#[test]
fn the_light_fades_with_each_states_clock() {
    let mut arena = arena();
    let attack = attack_in(&mut arena, 1, 1_500_000);
    let light = |arena: &ObjectArena| {
        arena
            .selector62_light(attack)
            .unwrap()
            .map(|light| (light.world_xz, light.radius, light.rgb))
    };
    // 2048 - 1500000 / 1464 and 32 - 1500000 / 93750.
    assert_eq!(light(&arena), Some(([10_000, 20_000], 1024, [0, 16, 16])));
    let fields = arena.selector62_mut(attack).unwrap();
    fields.state_138 = 2;
    fields.clock_13c = 1_000_000;
    // 64 - 1000000 / 31250, then +0x40 and +0x20, each held below 0x7F.
    assert_eq!(
        light(&arena),
        Some(([10_000, 20_000], 0x800, [32, 96, 127]))
    );
    arena.selector62_mut(attack).unwrap().state_138 = 3;
    // 999999 / 31250 unclamped, and 1000000 / 976.
    assert_eq!(light(&arena), Some(([10_000, 20_000], 1024, [31, 31, 31])));
    arena.selector62_mut(attack).unwrap().state_138 = 4;
    assert_eq!(light(&arena), None);
}

/// 00463FF0 deletes the target +15C and leaves the ribbons.
#[test]
fn destruction_releases_the_target() {
    let mut arena = arena();
    let attack = attack_in(&mut arena, 1, 1);
    let ribbon = arena.selector62(attack).unwrap().ribbons_164[0].unwrap();
    let handle = arena.actor(attack).unwrap().handle;
    let target = arena
        .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
            handle,
        ))
        .unwrap()
        .unwrap();
    arena.selector62_mut(attack).unwrap().target_15c = Some(target);
    arena
        .destroy(ObjectDestructionRequest {
            object: attack,
            sound: SoundTeardownService::RequiredAtReach,
        })
        .unwrap();
    assert!(arena.selector62(attack).is_err());
    assert!(arena.native_target(target).is_err(), "released target");
    assert!(arena.selector67(ribbon).is_ok(), "the ribbon stays");
}
