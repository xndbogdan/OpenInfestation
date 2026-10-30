use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorHandle, ActorMessageReply, ActorRegistryConfig,
    ActorTerrainInputs, DerivedActorConstructionRequest, ObjectClassKind,
};
use crate::process_particles::factories::ParticleFactoryProcessState;

const SINE: [u16; 4096] = [0; 4096];
const NULL: u32 = 0x1234;

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(NULL),
    })
    .unwrap()
}

/// A first player whose primary actor (+D4) is a live derived actor.
fn player_vehicle(arena: &mut ObjectArena, position: [u16; 3]) -> (PlayerId, ObjectId) {
    let vehicle = arena
        .allocate_derived_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null:
                        crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some(position),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [0, 0],
            },
            &mut 0,
        )
        .unwrap();
    arena.owned_mut(vehicle).unwrap().state.base.position_words = Some(position);
    let player = arena.add_world_player([0; 6]);
    let handle = arena.actor(vehicle).unwrap().handle;
    let state = arena.world_player_mut(player).unwrap();
    state.handle_d4 = handle;
    state.handle_d8 = ActorHandle::from_raw(NULL);
    (player, vehicle)
}

fn move_to(arena: &mut ObjectArena, object: ObjectId, position: [u16; 3]) {
    arena.owned_mut(object).unwrap().state.base.position_words = Some(position);
}

/// A gate at the origin with the identity basis and an extent of 100.
fn gate(arena: &mut ObjectArena) -> ObjectId {
    let gate = arena.allocate(ObjectClassKind::Selector64);
    assert_eq!(*arena.selector64(gate).unwrap(), Selector64State::new());
    let base = &mut arena.owned_mut(gate).unwrap().state.base;
    base.position_words = Some([0; 3]);
    base.word_28 = 100;
    gate
}

/// 00440520's first block: delta >> 4 eases +6C to 0xFFFF once passed and
/// to 0 before, with signed comparisons.
#[test]
fn the_animation_eases_with_the_passed_byte() {
    let mut arena = arena();
    let gate = gate(&mut arena);
    arena.selector64_mut(gate).unwrap().animation_6c = 100;
    arena.tick_selector64(gate, 16 * 40).unwrap();
    assert_eq!(arena.selector64(gate).unwrap().animation_6c, 60);
    arena.tick_selector64(gate, 16 * 60).unwrap();
    assert_eq!(arena.selector64(gate).unwrap().animation_6c, 0);
    arena.selector64_mut(gate).unwrap().passed_72 = 1;
    arena.tick_selector64(gate, 16 * 0xfff0).unwrap();
    assert_eq!(arena.selector64(gate).unwrap().animation_6c, 0xfff0);
    arena.tick_selector64(gate, 16 * 0x20).unwrap();
    assert_eq!(arena.selector64(gate).unwrap().animation_6c, 0xffff);
    assert_eq!(arena.selector64(gate).unwrap().model_value(), 0xffff);
}

/// The live first actor inside the extent marks its side (local Z above 0
/// sets +70); both sides return the crosser and clear the bytes, as does
/// leaving the extent.
#[test]
fn crossing_both_sides_returns_the_actor() {
    let mut arena = arena();
    let gate = gate(&mut arena);
    let (_, vehicle) = player_vehicle(&mut arena, [0, 0, 50]);
    assert_eq!(arena.tick_selector64(gate, 0).unwrap(), None);
    assert_eq!(
        (
            arena.selector64(gate).unwrap().side_70,
            arena.selector64(gate).unwrap().side_71
        ),
        (1, 0)
    );
    // Out of the extent (exactly 100 away) both bytes clear.
    move_to(&mut arena, vehicle, [0, 0, 100]);
    assert_eq!(arena.tick_selector64(gate, 0).unwrap(), None);
    assert_eq!(arena.selector64(gate).unwrap().side_70, 0);
    move_to(&mut arena, vehicle, [0, 0, 20]);
    assert_eq!(arena.tick_selector64(gate, 0).unwrap(), None);
    // Z = 0 counts as the other side.
    move_to(&mut arena, vehicle, [10, 0, 0]);
    assert_eq!(arena.tick_selector64(gate, 0).unwrap(), Some(vehicle));
    let state = arena.selector64(gate).unwrap();
    assert_eq!((state.side_70, state.side_71), (0, 0));
    // A dead actor (+C4 other than 3) is never seen.
    arena.actor_mut(vehicle).unwrap().word_c4 = 2;
    assert_eq!(arena.tick_selector64(gate, 0).unwrap(), None);
    assert_eq!(arena.selector64(gate).unwrap().side_71, 0);
}

#[derive(Default)]
struct Recorder {
    messages: Vec<(ObjectId, ActorMessageReply)>,
    repairs: Vec<PlayerId>,
    sounds: Vec<[u16; 3]>,
    process: Option<ParticleFactoryProcessState>,
}

impl Selector64PassHost for Recorder {
    fn message(
        &mut self,
        arena: &mut ObjectArena,
        target: ObjectId,
        message: ActorMessage,
    ) -> Result<(), String> {
        let reply = arena
            .receive_actor_message(
                target,
                message,
                PlayerActorLookupContext {
                    process: self.process.as_ref(),
                },
                false,
            )
            .map_err(|error| error.to_string())?;
        self.messages.push((target, reply));
        Ok(())
    }
    fn process(&self) -> Option<&ParticleFactoryProcessState> {
        self.process.as_ref()
    }
    fn repair(&mut self, _: &mut ObjectArena, player: PlayerId) -> Result<(), String> {
        self.repairs.push(player);
        Ok(())
    }
    fn sound(&mut self, position: [u16; 3]) -> Result<(), String> {
        self.sounds.push(position);
        Ok(())
    }
    fn command_context(&self) -> PlayerCommandContext<'_> {
        PlayerCommandContext {
            process: self.process.as_ref(),
            executable: None,
            resident: None,
        }
    }
}

/// 00440810 runs once: the player's actor is repaired and the gate sounds
/// at its position; +72 then blocks later passes.
#[test]
fn a_pass_repairs_and_sounds_once() {
    let mut arena = arena();
    let gate = gate(&mut arena);
    move_to(&mut arena, gate, [5, 6, 7]);
    let (player, vehicle) = player_vehicle(&mut arena, [0, 0, 0]);
    let mut host = Recorder {
        process: Some(ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(NULL)),
        }),
        ..Recorder::default()
    };
    arena
        .pass_selector64_gate(gate, vehicle, &mut host)
        .unwrap();
    assert_eq!(host.repairs, [player]);
    assert_eq!(host.sounds, [[5, 6, 7]]);
    assert_eq!(arena.selector64(gate).unwrap().passed_72, 1);
    arena
        .pass_selector64_gate(gate, vehicle, &mut host)
        .unwrap();
    assert_eq!(host.repairs.len(), 1);
}

/// Infest01's two Level 1 gate passes (bundle 20261008-154702-626-gates2): the
/// gates' basis, extent and position, and every update in which the side,
/// animation or passed bytes change, with the player's actor position. The
/// tick and, on a crossing, the pass reproduce every byte.
#[test]
fn updates_match_infest01_gate_passes() {
    type Row = (&'static str, usize, u32, [i32; 4], [u16; 3], [i32; 4], bool);
    const ROWS: [Row; 24] = [
        (
            "EBFE28:1A4B",
            0,
            0x1e848,
            [0x0, 0, 0, 0],
            [0xa019, 0x0315, 0x6aef],
            [0x0, 1, 0, 0],
            false,
        ),
        (
            "EC16B3:569",
            0,
            0x1e848,
            [0x0, 1, 0, 0],
            [0x9f72, 0x033b, 0x6ae8],
            [0x0, 1, 0, 0],
            false,
        ),
        (
            "EC2F59:568",
            0,
            0x1e848,
            [0x0, 1, 0, 0],
            [0x9ed2, 0x0351, 0x6ae1],
            [0x0, 0, 0, 1],
            true,
        ),
        (
            "EC49C5:1589",
            0,
            0x1e848,
            [0x0, 0, 0, 1],
            [0x9e39, 0x0358, 0x6ada],
            [0x1e84, 0, 1, 1],
            false,
        ),
        (
            "EC6228:EAD",
            0,
            0x1e848,
            [0x1e84, 0, 1, 1],
            [0x9da5, 0x0352, 0x6ad4],
            [0x3d08, 0, 1, 1],
            false,
        ),
        (
            "EC7A6D:1514",
            0,
            0x1e848,
            [0x3d08, 0, 1, 1],
            [0x9d17, 0x033e, 0x6ace],
            [0x5b8c, 0, 0, 1],
            false,
        ),
        (
            "EC932A:1C8C",
            0,
            0x1e848,
            [0x5b8c, 0, 0, 1],
            [0x9c8e, 0x031d, 0x6ac8],
            [0x7a10, 0, 0, 1],
            false,
        ),
        (
            "ECABEF:429",
            0,
            0x1e848,
            [0x7a10, 0, 0, 1],
            [0x9c0a, 0x02ef, 0x6ac3],
            [0x9894, 0, 0, 1],
            false,
        ),
        (
            "ECC49E:64A",
            0,
            0x1e848,
            [0x9894, 0, 0, 1],
            [0x9b8b, 0x02b5, 0x6abe],
            [0xb718, 0, 0, 1],
            false,
        ),
        (
            "ECDCFC:429",
            0,
            0x1e848,
            [0xb718, 0, 0, 1],
            [0x9b11, 0x026f, 0x6ab9],
            [0xd59c, 0, 0, 1],
            false,
        ),
        (
            "ECF8C9:E1B",
            0,
            0x1e848,
            [0xd59c, 0, 0, 1],
            [0x9a9b, 0x021e, 0x6ab4],
            [0xf420, 0, 0, 1],
            false,
        ),
        (
            "ED11E4:42F",
            0,
            0x1e848,
            [0xf420, 0, 0, 1],
            [0x9a2a, 0x01c2, 0x6aaf],
            [0xffff, 0, 0, 1],
            false,
        ),
        (
            "EEF20A:24B",
            1,
            0x1e848,
            [0x0, 0, 0, 0],
            [0x8c88, 0x02e6, 0x6aa0],
            [0x0, 1, 0, 0],
            false,
        ),
        (
            "EF051B:F86",
            1,
            0x1e848,
            [0x0, 1, 0, 0],
            [0x8bd7, 0x0315, 0x6aa5],
            [0x0, 1, 0, 0],
            false,
        ),
        (
            "EF1828:2E5",
            1,
            0x1e848,
            [0x0, 1, 0, 0],
            [0x8b2e, 0x0334, 0x6aaa],
            [0x0, 0, 0, 1],
            true,
        ),
        (
            "EF2DB4:AF",
            1,
            0x1e848,
            [0x0, 0, 0, 1],
            [0x8a8c, 0x0344, 0x6aae],
            [0x1e84, 0, 1, 1],
            false,
        ),
        (
            "EF41C5:3DA",
            1,
            0x1e848,
            [0x1e84, 0, 1, 1],
            [0x89f1, 0x0345, 0x6ab2],
            [0x3d08, 0, 1, 1],
            false,
        ),
        (
            "EF55A3:2BC",
            1,
            0x1e848,
            [0x3d08, 0, 1, 1],
            [0x895c, 0x0339, 0x6ab6],
            [0x5b8c, 0, 0, 1],
            false,
        ),
        (
            "EF6918:AA6",
            1,
            0x1e848,
            [0x5b8c, 0, 0, 1],
            [0x88cd, 0x0320, 0x6aba],
            [0x7a10, 0, 0, 1],
            false,
        ),
        (
            "EF7D26:1BF0",
            1,
            0x1e848,
            [0x7a10, 0, 0, 1],
            [0x8843, 0x02fa, 0x6abd],
            [0x9894, 0, 0, 1],
            false,
        ),
        (
            "EF90FD:229",
            1,
            0x1e848,
            [0x9894, 0, 0, 1],
            [0x87be, 0x02c7, 0x6ac0],
            [0xb718, 0, 0, 1],
            false,
        ),
        (
            "EFA4F0:20EF",
            1,
            0x1e848,
            [0xb718, 0, 0, 1],
            [0x873e, 0x0288, 0x6ac3],
            [0xd59c, 0, 0, 1],
            false,
        ),
        (
            "EFB9AB:37F",
            1,
            0x1e848,
            [0xd59c, 0, 0, 1],
            [0x86c3, 0x023e, 0x6ac6],
            [0xf420, 0, 0, 1],
            false,
        ),
        (
            "EFCD1B:459",
            1,
            0x1e848,
            [0xf420, 0, 0, 1],
            [0x864d, 0x01e8, 0x6ac8],
            [0xffff, 0, 0, 1],
            false,
        ),
    ];
    const BASIS: [u32; 10] = [3, 0, 0, 0xbf80_0000, 0, 0x3f80_0000, 0, 0x3f80_0000, 0, 0];
    let mut arena = arena();
    let (_, vehicle) = player_vehicle(&mut arena, [0; 3]);
    let gates = [[0x9f0c, 0x350, 0x6a5e], [0x8b69, 0x354, 0x6a50]].map(|position| {
        let gate = gate(&mut arena);
        let base = &mut arena.owned_mut(gate).unwrap().state.base;
        base.position_words = Some(position);
        base.word_28 = 0x190;
        arena.selector64_mut(gate).unwrap().rotating.basis.words = BASIS;
        gate
    });
    let mut host = Recorder {
        process: Some(ParticleFactoryProcessState {
            network_flag: 0,
            local_slot: 1,
            host_flag: 0,
            settings: [0; 20],
            process_null_handle: Some(ActorHandle::from_raw(NULL)),
        }),
        ..Recorder::default()
    };
    for (time, index, delta, before, at, after, passed) in ROWS {
        let gate = gates[index];
        {
            let state = arena.selector64_mut(gate).unwrap();
            state.animation_6c = before[0];
            state.side_70 = before[1] as u8;
            state.side_71 = before[2] as u8;
            state.passed_72 = before[3] as u8;
        }
        move_to(&mut arena, vehicle, at);
        let crosser = arena.tick_selector64(gate, delta).unwrap();
        assert_eq!(crosser.is_some(), passed, "{time}");
        if let Some(crosser) = crosser {
            arena
                .pass_selector64_gate(gate, crosser, &mut host)
                .unwrap();
        }
        let state = arena.selector64(gate).unwrap();
        assert_eq!(
            [
                state.animation_6c,
                i32::from(state.side_70),
                i32::from(state.side_71),
                i32::from(state.passed_72)
            ],
            after,
            "{time}"
        );
    }
    assert_eq!(host.repairs.len(), 2);
}

/// Each loaded world's gates whose link resolves to an actor, with it.
fn linked_gates(arena: &mut ObjectArena) -> Vec<(ObjectId, ObjectId)> {
    let mut objects: Vec<ObjectId> = [
        crate::object_runtime::ObjectList::Flag0Primary,
        crate::object_runtime::ObjectList::Flag0Secondary,
        crate::object_runtime::ObjectList::Flag1Primary,
        crate::object_runtime::ObjectList::Flag1Secondary,
        crate::object_runtime::ObjectList::Parking,
    ]
    .iter()
    .flat_map(|list| arena.list(*list).iter().map(|node| node.object))
    .collect();
    objects.sort_by_key(|object| object.slot);
    objects.dedup();
    let mut linked = Vec::new();
    for gate in objects {
        let Some(link) = arena.selector64(gate).ok().and_then(|g| g.link_74.clone()) else {
            continue;
        };
        let handle = link
            .allocation()
            .runtime_allocation()
            .dword(link.allocation_offset)
            .unwrap();
        if let Some(target) = arena
            .resolve_actor_handle(ActorHandle::from_raw(handle))
            .unwrap()
        {
            linked.push((gate, target));
        }
    }
    linked
}

/// Shak'tahh's guns are remembered going when its hoops are flown
/// through. Its gates 87, 88, 90 and 93 link its four turrets (resource
/// 0xB), which retire on 0x15 (`00402420`); gate 89 links a Force Field,
/// which ignores it (`004787E0`). Defence Satellite's thirteen link its
/// barriers, which grant their `+7A` and go (`00411CE0`). The port stopped
/// the turrets' gates and ignored the barriers'.
#[test]
fn linked_gates_retire_turrets_and_release_barriers() {
    for (selection, retired, released, ignored) in [(17, 4, 0, 1), (16, 0, 13, 0)] {
        let (_resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
            selection,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let arena = &mut world.arena;
        let null = arena.actor_handles().unwrap().null_handle();
        let mut counts = (0, 0, 0);
        for (gate, target) in linked_gates(arena) {
            if matches!(
                arena.actor(target).unwrap().extension,
                ActorExtension::Selector30(_)
            ) {
                continue;
            }
            let mut host = Recorder {
                process: Some(ParticleFactoryProcessState {
                    network_flag: 0,
                    local_slot: 1,
                    host_flag: 0,
                    settings: [0; 20],
                    process_null_handle: Some(null),
                }),
                ..Recorder::default()
            };
            arena.pass_selector64_gate(gate, target, &mut host).unwrap();
            assert_eq!(arena.selector64(gate).unwrap().passed_72, 1);
            match host.messages[..] {
                [(t, ActorMessageReply::Retire)] if t == target => counts.0 += 1,
                [(t, ActorMessageReply::Release { .. })] if t == target => counts.1 += 1,
                [(t, ActorMessageReply::Done(0))] if t == target => counts.2 += 1,
                ref other => panic!("world {selection} gate {gate:?}: {other:?}"),
            }
        }
        assert_eq!(counts, (retired, released, ignored), "world {selection}");
    }
}
