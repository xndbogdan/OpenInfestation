use super::*;
use crate::object_runtime::actor::selector30::tests::{arena, request};
use crate::object_runtime::ActorHandle;

/// The services the pass asks for, in order.
struct Host {
    player: Option<PlayerId>,
    camera: Option<ObjectId>,
    status: (u32, u32),
    calls: Vec<String>,
}

impl Host {
    fn new(player: Option<PlayerId>) -> Self {
        Self {
            player,
            camera: None,
            status: (0x12, 0),
            calls: Vec::new(),
        }
    }
}

impl Selector30PassHost for Host {
    type Error = String;
    fn find_player(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<Option<PlayerId>, String> {
        self.calls.push("find_player".into());
        Ok(self.player)
    }
    fn grant(&mut self, _: &mut ObjectArena, _: PlayerId, id: u16) -> Result<(), String> {
        self.calls.push(format!("grant {id}"));
        Ok(())
    }
    fn drop_item(&mut self, _: &mut ObjectArena, _: ObjectId, index: usize) -> Result<(), String> {
        self.calls.push(format!("drop {index}"));
        Ok(())
    }
    fn world_status(&mut self) -> Result<(u32, u32), String> {
        self.calls.push("world_status".into());
        Ok(self.status)
    }
    fn set_world_status(&mut self, status: u32) -> Result<(), String> {
        self.calls.push(format!("set_world_status {status}"));
        self.status.1 = status;
        Ok(())
    }
    fn grant_cheat_flag(&mut self, bit: u32) -> Result<(), String> {
        self.calls.push(format!("cheat {bit:#x}"));
        Ok(())
    }
    fn enter_pelagia(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), String> {
        self.calls.push("enter_pelagia".into());
        Ok(())
    }
    fn request_world(
        &mut self,
        _: &mut ObjectArena,
        world: u32,
        start: u16,
        _: ObjectId,
    ) -> Result<(), String> {
        self.calls.push(format!("request_world {world} {start}"));
        Ok(())
    }
    fn flash(&mut self, _: &mut ObjectArena, position: [u16; 3]) -> Result<(), String> {
        self.calls.push(format!("flash {position:?}"));
        Ok(())
    }
    fn camera(&mut self, _: &mut ObjectArena) -> Result<ObjectId, String> {
        self.calls.push("camera".into());
        self.camera.ok_or_else(|| "no camera".into())
    }
    fn notify_listeners(&mut self, _: &mut ObjectArena, _: ObjectId) -> Result<(), String> {
        self.calls.push("notify_listeners".into());
        Ok(())
    }
}

fn math() -> NativeBasisMath {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    NativeBasisMath::new(assets.sine_table().values)
}

const UNROTATED: [u32; 10] = [3, 0x3f80_0000, 0, 0, 0, 0x3f80_0000, 0, 0, 0, 0x3f80_0000];

/// Infest03's surface warpgate at 248605:76D. The vehicle crossed the gate
/// at (48675, 65472, 23327) and came out of the linked one at (19257,
/// 64928, 46049), both unrotated. Its offset (9, 288, 142), seen from the
/// gate turned half a circle, is (-9, 288, -142); 10 further, about the
/// linked gate, it lands at (19248, 65216, 45917). The velocity turns the
/// same way, and every basis row keeps Y with X and Z negated, bit for bit,
/// its counter two further.
#[test]
fn selector30_surface_gate_lays_the_actor_out_about_the_linked_gate() {
    let math = math();
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([48675, 65472, 23327])))
        .unwrap();
    let linked = arena
        .allocate_selector30_actor(request(Some([19257, 64928, 46049])))
        .unwrap();
    let actor = arena
        .allocate_actor(request(Some([48684, 224, 23469])))
        .unwrap();
    for object in [gate, linked] {
        arena.actor_mut(object).unwrap().rotating.basis.words = UNROTATED;
    }
    arena.actor_mut(gate).unwrap().word_7a = 72;
    let linked_handle = arena.actor(linked).unwrap().handle;
    let state = arena.selector30_actor_mut(gate).unwrap();
    state.word_180 = 17;
    state.handle_184 = linked_handle;
    let moving = arena.actor_mut(actor).unwrap();
    moving.words_8e_92 = [67, 60, 1396];
    moving.words_94_9c = [0; 3];
    moving.rotating.basis.words = [
        0x5a,
        0x3f7f_b5f3,
        0x39e2_55f5,
        0xbd42_8639,
        0x3d3b_63f5,
        0x3e83_82cb,
        0x3f77_17dc,
        0x3c4f_1196,
        0xbf77_5e28,
        0x3e83_5a7b,
    ];
    let mut host = Host::new(None);
    assert!(arena
        .pass_selector30(gate, actor, false, &math, &mut host)
        .unwrap());
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([19248, 65216, 45917])
    );
    let moved = arena.actor(actor).unwrap();
    assert_eq!(moved.words_8e_92, [65469, 60, 64140]);
    assert_eq!(moved.words_94_9c, [0; 3]);
    assert_eq!(
        moved.rotating.basis.words,
        [
            0x5c,
            0xbf7f_b5f3,
            0x39e2_55f5,
            0x3d42_8639,
            0xbd3b_63f5,
            0x3e83_82cb,
            0xbf77_17dc,
            0xbc4f_1196,
            0xbf77_5e28,
            0xbe83_5a7b,
        ]
    );
    // Without a player there is no grant, flash or camera snap.
    assert_eq!(host.calls, ["find_player", "notify_listeners"]);
}

/// Infest04's warp on Helniazine (world 6): the vehicle, reversing north
/// through the mutants' gate the player's scientist had taken (`363AE00`,
/// link 0x4B to `363AC60`), comes out of the linked gate (`gate-pass04.txt`,
/// the pass at `7BB2C9:9AC`).
#[test]
fn selector30_helniazine_gate_lays_the_vehicle_out_as_infest04() {
    let math = math();
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([48054, 64320, 34129])))
        .unwrap();
    let linked = arena
        .allocate_selector30_actor(request(Some([27209, 64320, 23135])))
        .unwrap();
    let actor = arena
        .allocate_actor(request(Some([48025, 64396, 34157])))
        .unwrap();
    for object in [gate, linked] {
        arena.actor_mut(object).unwrap().rotating.basis.words = UNROTATED;
    }
    arena.actor_mut(gate).unwrap().word_7a = 72;
    let linked_handle = arena.actor(linked).unwrap().handle;
    let state = arena.selector30_actor_mut(gate).unwrap();
    state.word_180 = 0x4b;
    state.handle_184 = linked_handle;
    let moving = arena.actor_mut(actor).unwrap();
    moving.words_8e_92 = [5, 10, 659];
    moving.words_94_9c = [62, 1919, 442];
    moving.rotating.basis.words = [
        0x21,
        0x3f7f_d5ea,
        0xbc18_6e41,
        0xbcf5_bedf,
        0x3c1d_e288,
        0x3f7f_f4f1,
        0x3c30_f067,
        0x3cf4_2275,
        0xbc35_b0d6,
        0x3f7f_d759,
    ];
    let mut host = Host::new(None);
    assert!(arena
        .pass_selector30(gate, actor, false, &math, &mut host)
        .unwrap());
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([27238, 64396, 23117])
    );
    let moved = arena.actor(actor).unwrap();
    assert_eq!(moved.words_8e_92, [65531, 10, 64877]);
    // The turned spin lands on the gate (0048C1ED stores through EBP); the
    // vehicle keeps its own.
    assert_eq!(moved.words_94_9c, [62, 1919, 442]);
    assert_eq!(
        arena.actor(gate).unwrap().words_94_9c,
        [(-62_i32) as u32, 1919, (-442_i32) as u32]
    );
    let moved = arena.actor(actor).unwrap();
    assert_eq!(
        moved.rotating.basis.words,
        [
            0x23,
            0xbf7f_d5ea,
            0xbc18_6e41,
            0x3cf5_bedf,
            0xbc1d_e288,
            0x3f7f_f4f1,
            0xbc30_f067,
            0xbcf4_2275,
            0xbc35_b0d6,
            0xbf7f_d759,
        ]
    );
}

/// A player's actor through a linked gate takes the grant and flashes at
/// the gate; the camera snaps (+187) and the linked gate is marked used
/// (+38 bit 2, +B4 = 2). A lost link is forgotten (+180 = 0).
#[test]
fn selector30_surface_gate_snaps_the_camera_and_forgets_a_lost_link() {
    let math = math();
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([1000, 0, 1000])))
        .unwrap();
    let linked = arena
        .allocate_selector30_actor(request(Some([5000, 0, 5000])))
        .unwrap();
    let actor = arena.allocate_actor(request(Some([1000, 0, 990]))).unwrap();
    let camera = arena.allocate_actor(request(Some([0, 0, 0]))).unwrap();
    for object in [gate, linked, actor] {
        arena.actor_mut(object).unwrap().rotating.basis.words = UNROTATED;
    }
    arena.actor_mut(gate).unwrap().word_7a = 0x95;
    let linked_handle = arena.actor(linked).unwrap().handle;
    let state = arena.selector30_actor_mut(gate).unwrap();
    state.word_180 = 1;
    state.handle_184 = linked_handle;
    let mut host = Host::new(Some(PlayerId::unissued(1)));
    host.camera = Some(camera);
    // The camera lookup needs a selector-16 camera; a plain actor stops
    // there, after the flash.
    let result = arena.pass_selector30(gate, actor, false, &math, &mut host);
    assert!(result.is_err());
    assert_eq!(
        host.calls,
        [
            "find_player",
            "grant 149",
            "flash [1000, 0, 1000]",
            "camera"
        ]
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([5000, 0, 5000 + 10 + 10])
    );

    let lost = arena
        .allocate_selector30_actor(request(Some([2000, 0, 2000])))
        .unwrap();
    let state = arena.selector30_actor_mut(lost).unwrap();
    state.word_180 = 5;
    state.handle_184 = ActorHandle::from_raw(0x0bad_0001);
    let mut host = Host::new(None);
    assert!(!arena
        .pass_selector30(lost, actor, false, &math, &mut host)
        .unwrap());
    assert_eq!(arena.selector30_actor(lost).unwrap().word_180, 0);
}

/// A world gate: only a player's vehicle leaves; anything else that
/// crosses is disposed, and a remote actor is left alone.
#[test]
fn selector30_world_gate_disposes_what_is_not_a_players_vehicle() {
    let math = math();
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([0, 0, 0])))
        .unwrap();
    arena.selector30_actor_mut(gate).unwrap().word_15c = 2;
    let walker = arena.allocate_actor(request(Some([0, 0, 0]))).unwrap();
    let mut host = Host::new(None);
    assert!(!arena
        .pass_selector30(gate, walker, false, &math, &mut host)
        .unwrap());
    assert!(arena
        .list(crate::object_runtime::ObjectList::Disposal)
        .iter()
        .any(|entry| entry.object == walker));
    assert_eq!(host.calls, ["find_player"]);

    let remote = arena.allocate_actor(request(Some([0, 0, 0]))).unwrap();
    arena.owned_mut(remote).unwrap().state.base.flags |= 0x8000_0000;
    let mut host = Host::new(None);
    assert!(!arena
        .pass_selector30(gate, remote, false, &math, &mut host)
        .unwrap());
    assert!(host.calls.is_empty());
}

/// Pelagia (the port's): on the Command Centre, gate 81 is the dead gate
/// the load marks, and its gate 2 to Terra Nova is not. A scientist's wake
/// counts the lock to 0, retires hint 9 for the first player and gives
/// world 1, start 1; the gate then takes the ACAM by the Teleport's load,
/// never the world end.
#[test]
fn pelagias_gate_wakes_and_takes_the_vehicle_by_the_teleports_load() {
    let math = math();
    let (resident, _assets, mut world) = crate::test_worlds::offline_world_retained(
        crate::pelagia::COMMAND_CENTRE,
        crate::terrain_runtime::TerrainSpacing::Units256,
    );
    let gate = world.report.placements[crate::pelagia::DEAD_GATE].unwrap();
    let terra_nova = world.report.placements[2].unwrap();
    let placements = world.report.placements.clone();
    crate::pelagia::prepare_world(
        &mut world.arena,
        1,
        &placements,
        &resident,
        &mut world.terrain,
        true,
    );
    assert!(
        !world.arena.pelagia_gate_locked(gate),
        "only on the Command Centre"
    );
    let placements = world.report.placements.clone();
    crate::pelagia::prepare_world(
        &mut world.arena,
        crate::pelagia::COMMAND_CENTRE,
        &placements,
        &resident,
        &mut world.terrain,
        true,
    );
    let arena = &mut world.arena;
    // The loader marks gate 81 but never moves it.
    assert!(arena.pelagia_gate_locked(gate));
    assert!(!arena.pelagia_gate_locked(terra_nova));
    let null = arena.actor_handles().unwrap().null_handle();
    let process = crate::process_particles::factories::ParticleFactoryProcessState {
        network_flag: 0,
        local_slot: 1,
        host_flag: 0,
        settings: [0; 20],
        process_null_handle: Some(null),
    };
    let context = crate::object_runtime::PlayerCommandContext {
        process: Some(&process),
        executable: None,
        resident: None,
    };
    let player = arena.first_player().unwrap();
    let hint_seen = |arena: &mut ObjectArena| {
        arena
            .apply_player_command(
                crate::object_runtime::PlayerCommandRequest {
                    player,
                    operation: crate::object_runtime::PlayerCommandOperation::Query { raw_id: 9 },
                },
                &context,
            )
            .unwrap()
    };
    use crate::object_runtime::PlayerCommandOutcome::Owned;
    assert_eq!(hint_seen(arena), Owned(false));
    let scientist = world.report.placements[32].expect("Level 1 places a scientist");
    assert!(arena.selector12_actor(scientist).is_ok());
    arena.wake_pelagia_gate(gate, scientist, &context).unwrap();
    let state = arena.selector30_actor(gate).unwrap();
    assert_eq!((state.word_164, state.word_15c, state.word_160), (0, 1, 1));
    assert!(!arena.pelagia_gate_locked(gate));
    // The reporter lights its bulb: it has something to tell the player.
    assert_eq!(arena.selector12_actor(scientist).unwrap().byte_29d, 1);
    assert_eq!(hint_seen(arena), Owned(true));
    let vehicle = arena.first_player_primary_actor().unwrap().unwrap();
    let mut host = Host::new(Some(player));
    assert!(!arena
        .pass_selector30(gate, vehicle, false, &math, &mut host)
        .unwrap());
    assert!(
        host.calls.contains(&"enter_pelagia".to_string()),
        "{:?}",
        host.calls
    );
    assert!(!host
        .calls
        .iter()
        .any(|call| call.starts_with("request_world")));
}

/// `0048BD90` against the 64 gate cases of `world_interactions_native.json`,
/// run on the original instructions with the real `0048EAD0`/`0048EB30`
/// over a supplied mission: the gate's team, its dirty byte +38, the
/// world's completion words and the `+18E` pass.
#[test]
fn gate_entry_matches_the_native_receipt() {
    let fixture: serde_json::Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/world_interactions_native.json"
    ))
    .unwrap();
    let cases = fixture["gate_entry"].as_array().unwrap();
    assert_eq!(cases.len(), 64);
    let number = |value: &serde_json::Value| value.as_u64().unwrap();
    let mut marked = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let mut arena = arena();
        let gate = arena
            .allocate_selector30_actor(request(Some([0; 3])))
            .unwrap();
        let scientist = arena
            .allocate_selector30_actor(request(Some([0; 3])))
            .unwrap();
        {
            let state = arena.selector30_actor_mut(gate).unwrap();
            state.word_188 = Some(number(&case["first_team"]) as u32);
            state.byte_192 = number(&case["held"]) as u8;
            state.byte_18e = number(&case["pass"]) as u8;
        }
        let actor = arena.actor_mut(gate).unwrap();
        actor.word_b4 = number(&case["gate_team"]) as u32;
        actor.byte_78 = number(&case["index"]) as u8;
        let dirty = &mut arena.owned_mut(gate).unwrap().state.base.dirty_word;
        *dirty = (*dirty & !0xff) | number(&case["dirty"]) as u16;
        arena.actor_mut(scientist).unwrap().word_b4 = number(&case["scientist_team"]) as u32;
        let entry = arena.gate_takes_scientist(gate, scientist).unwrap();
        let mut mission = crate::world_mission::WorldMission::native_constructor();
        let world = number(&case["world"]) as usize;
        mission.current_world_a0 = world as u32;
        mission.inhibit_byte_08 = number(&case["inhibit"]) as u8;
        for (i, word) in case["completion"].as_array().unwrap().iter().enumerate() {
            mission.save.owner_240[world * 4 + i] = number(word) as u32;
        }
        if let Some(record) = entry.record {
            if mission.apply_gate_record(record).unwrap() {
                marked += 1;
            }
        }
        assert_eq!(
            arena.actor(gate).unwrap().word_b4,
            number(&case["gate_team_after"]) as u32,
            "{name}"
        );
        assert_eq!(
            arena.object(gate).unwrap().base.dirty_word & 0xff,
            number(&case["dirty_after"]) as u16,
            "{name}"
        );
        let after: Vec<u32> = case["completion_after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| number(word) as u32)
            .collect();
        assert_eq!(
            &mission.save.owner_240[world * 4..world * 4 + 4],
            &after[..],
            "{name}"
        );
        assert_eq!(entry.pass, case["passed"].as_bool().unwrap(), "{name}");
    }
    assert!(marked > 0);
}

/// Infest03's 112 gate entries (`gate-entry.js`): a mutant scientist walks
/// into gate `037DCC20` (team 1, first team 1, index 4) and takes it for
/// team 0x12 with `0048EAD0`'s mark; the 111 entries after it find the
/// gate already the mutants' and change nothing. Every one answers 0.
#[test]
fn gate_entries_match_infest03() {
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([36896, 64576, 21498])))
        .unwrap();
    let scientist = arena
        .allocate_selector30_actor(request(Some([36800, 64576, 21593])))
        .unwrap();
    arena.selector30_actor_mut(gate).unwrap().word_188 = Some(1);
    let actor = arena.actor_mut(gate).unwrap();
    actor.word_b4 = 1;
    actor.byte_78 = 4;
    arena.actor_mut(scientist).unwrap().word_b4 = 0x12;

    let entry = arena.gate_takes_scientist(gate, scientist).unwrap();
    assert_eq!(arena.actor(gate).unwrap().word_b4, 0x12);
    assert_eq!(entry.record, Some(GateRecord::Mark(4)));
    assert!(!entry.pass);
    for _ in 0..111 {
        let entry = arena.gate_takes_scientist(gate, scientist).unwrap();
        assert_eq!(arena.actor(gate).unwrap().word_b4, 0x12);
        assert_eq!(entry.record, None);
        assert!(!entry.pass);
    }
}

/// Infest04's 255 gate entries on Helniazine (`gate-entry.js`). At
/// `7AFEF5:1B5B` the player's scientist (team 2) walks into the mutants'
/// gate `363AE00` (team 0x12, first team 0x12, index 6) and takes it for
/// team 2 with `0048EAD0`'s mark; one later entry finds it already team 2.
/// Earlier, at `6A61E0:861`, a mutant takes gate `363D260` (team 1, first
/// team 1, index 4) as in Infest03, and 252 entries after it change
/// nothing. Every one answers 0.
#[test]
fn gate_entries_match_infest04() {
    let mut arena = arena();
    let enemy_gate = arena
        .allocate_selector30_actor(request(Some([48054, 64320, 34129])))
        .unwrap();
    let ours = arena
        .allocate_selector30_actor(request(Some([47983, 64317, 34222])))
        .unwrap();
    arena.selector30_actor_mut(enemy_gate).unwrap().word_188 = Some(0x12);
    let actor = arena.actor_mut(enemy_gate).unwrap();
    actor.word_b4 = 0x12;
    actor.byte_78 = 6;
    arena.actor_mut(ours).unwrap().word_b4 = 2;
    let entry = arena.gate_takes_scientist(enemy_gate, ours).unwrap();
    assert_eq!(arena.actor(enemy_gate).unwrap().word_b4, 2);
    assert_eq!(entry.record, Some(GateRecord::Mark(6)));
    assert!(!entry.pass);
    let entry = arena.gate_takes_scientist(enemy_gate, ours).unwrap();
    assert_eq!(arena.actor(enemy_gate).unwrap().word_b4, 2);
    assert_eq!(entry.record, None);

    let human_gate = arena
        .allocate_selector30_actor(request(Some([36896, 64576, 21498])))
        .unwrap();
    let mutant = arena
        .allocate_selector30_actor(request(Some([36796, 64576, 21595])))
        .unwrap();
    arena.selector30_actor_mut(human_gate).unwrap().word_188 = Some(1);
    let actor = arena.actor_mut(human_gate).unwrap();
    actor.word_b4 = 1;
    actor.byte_78 = 4;
    arena.actor_mut(mutant).unwrap().word_b4 = 0x12;
    let entry = arena.gate_takes_scientist(human_gate, mutant).unwrap();
    assert_eq!(arena.actor(human_gate).unwrap().word_b4, 0x12);
    assert_eq!(entry.record, Some(GateRecord::Mark(4)));
    for _ in 0..252 {
        let entry = arena.gate_takes_scientist(human_gate, mutant).unwrap();
        assert_eq!(arena.actor(human_gate).unwrap().word_b4, 0x12);
        assert_eq!(entry.record, None);
        assert!(!entry.pass);
    }
}

/// Online, a gate whose link is still the NULL (a client's, built from the
/// host's record) finds it by placement (0048BF29): the first actor on the
/// list placed at +180 less one, whose handle +184 keeps. With none, the
/// walk ends on the last actor it read, and the actor moves there.
#[test]
fn selector30_online_gate_finds_its_link_by_placement() {
    let math = math();
    let mut arena = arena();
    let gate = arena
        .allocate_selector30_actor(request(Some([1000, 0, 1000])))
        .unwrap();
    let linked = arena
        .allocate_selector30_actor(request(Some([5000, 0, 5000])))
        .unwrap();
    let actor = arena.allocate_actor(request(Some([1000, 0, 990]))).unwrap();
    let unmatched = arena
        .allocate_selector30_actor(request(Some([5000, 0, 5030])))
        .unwrap();
    let last = arena
        .allocate_actor(request(Some([7000, 0, 7000])))
        .unwrap();
    for object in [gate, linked, actor, unmatched, last] {
        arena.actor_mut(object).unwrap().rotating.basis.words = UNROTATED;
        arena.owned_mut(object).unwrap().state.base.placement_word = 3;
    }
    arena.owned_mut(linked).unwrap().state.base.placement_word = 16;
    let null = arena.actor_handles().unwrap().null_handle();
    for (object, reference) in [(gate, 17), (unmatched, 40)] {
        let state = arena.selector30_actor_mut(object).unwrap();
        state.word_180 = reference;
        state.handle_184 = null;
    }
    let mut host = Host::new(None);
    assert!(arena
        .pass_selector30(gate, actor, true, &math, &mut host)
        .unwrap());
    assert_eq!(
        arena.selector30_actor(gate).unwrap().handle_184,
        arena.actor(linked).unwrap().handle
    );
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([5000, 0, 5020])
    );
    assert!(arena
        .pass_selector30(unmatched, actor, true, &math, &mut host)
        .unwrap());
    assert_eq!(arena.selector30_actor(unmatched).unwrap().handle_184, null);
    assert_eq!(
        arena.object(actor).unwrap().base.position_words,
        Some([7000, 0, 7020])
    );
}
