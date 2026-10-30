use super::*;
use crate::grant_text::{GrantTextData, GrantTextPairClassification};
use crate::session::SessionPhase;

struct NoVoices;
impl ProcessSoundVoiceTeardown for NoVoices {
    fn delete_voice(
        &mut self,
        _: ProcessSoundRef,
        _: crate::process_sound::ProcessSoundVoiceRef,
    ) -> Result<(), crate::process_sound::ProcessSoundRecordError> {
        Ok(())
    }
}

/// Level 1's gate to Terra Nova ends the world for the vehicle, as
/// Infest02's pass at 4E9955 did. Approaching, the player reads "Warp to
/// Terra Nova". The pass keeps the player's grant 72, marks the world's
/// status with bit 2 (+167 is 1) before 00487560 adds bit 1, selects world
/// 2 from start 1 (+A4 = 0) and requests the results, phase 8.
#[test]
fn level1_exit_gate_ends_the_world_for_terra_nova() {
    let (resident, assets, mut world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units256);
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let gate = world
        .arena
        .actor_list()
        .iter()
        .copied()
        .find(|&actor| {
            world
                .arena
                .selector30_actor(actor)
                .is_ok_and(|state| state.word_15c == 2)
        })
        .expect("Level 1's gate to world 2");
    let state_167 = world.arena.selector30_actor(gate).unwrap().byte_167;
    assert_eq!(state_167, 1);
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let player = world.arena.first_player().unwrap();
    // The gameplay screen's message environment, with the message enabled.
    world
        .arena
        .set_player_message_environment(crate::object_runtime::PlayerMessageEnvironment {
            menus: crate::object_runtime::PlayerMessageMenus::parse(&assets).unwrap(),
            math: crate::menu_basis::MenuBasisMath::new(assets.sine_table().values),
            network: 0,
            visibility: crate::menu::MenuVisibility::OwnerFlagSet,
            pause_on_hints: false,
            attract: false,
            ui_state: 4,
            cheats: Default::default(),
        });
    let recipient = world.arena.player_message(player).unwrap().identity();
    world
        .arena
        .apply_player_message(crate::object_runtime::PlayerMessageRequest {
            recipient,
            operation: crate::object_runtime::PlayerMessageOperation::Enable,
        })
        .unwrap();
    let mut sounds = ProcessSoundRuntime::new(executable).unwrap();
    // The process the offline load ran with, and its null handle.
    let process = ParticleFactoryProcessState::native_startup();
    let mut state = SessionState::default();
    let sine = assets.sine_table().values;
    let math = NativeBasisMath::new(sine);
    let text = GrantTextData {
        null_substitute: None,
        pair_classification: Some(GrantTextPairClassification {
            colon_high: Some(0),
            space_high: Some(0),
        }),
    };
    let mut cheat_flags = 0;
    let mut voices = NoVoices;
    assert_eq!(world.mission.current_world_a0, 18);
    let status = world.mission.save.owner_3c[18];
    let mut host = WorldSelector30Host {
        sounds: &mut sounds,
        residency: &resident,
        voices: &mut voices,
        executable,
        factory_process: &process,
        mission: &mut world.mission,
        state: &mut state,
        grant_text: Some(&text),
        terrain: &world.terrain,
        flash: &mut world.flash,
        listeners: &mut world.listeners,
        camera_settings: CameraSettings {
            first_person: 0,
            camera_style: 0,
        },
        sine: &sine,
        math: &math,
        animation_clock: 0,
        world_flags: world.world_flags,
        rng: &mut world.rng,
        cheat_flags: &mut cheat_flags,
    };
    let arena = &mut world.arena;
    let shown = |arena: &ObjectArena| {
        arena
            .player_message(player)
            .unwrap()
            .text_38()
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
    };
    Selector30UpdateHost::destination(&mut host, arena, player, 2).unwrap();
    assert_eq!(shown(arena).as_deref(), Some("Warp to Terra Nova"));
    Selector30UpdateHost::destination(&mut host, arena, player, 0).unwrap();
    assert_eq!(shown(arena).as_deref(), Some("Surface-bound warpgate"));
    // Grant 0x95's record names what a gate with +18C 0x95 wants.
    Selector30UpdateHost::missing_grant(&mut host, arena, player, 0x95).unwrap();
    let missing = shown(arena).unwrap();
    assert!(
        missing.starts_with("Before warping, you need the following information: "),
        "{missing}"
    );

    assert_eq!(
        Selector30PassHost::find_player(&mut host, arena, vehicle).unwrap(),
        Some(player)
    );
    // The update's pass: 0048BE70 through the world's services.
    Selector30UpdateHost::pass(&mut host, arena, gate, vehicle).unwrap();
    drop(host);
    assert_eq!(world.mission.save.mission_selection, 2);
    assert_eq!(world.mission.save.owner_a4, 0);
    assert_eq!(world.mission.save.owner_3c[18], status | 3);
    assert_eq!(state.requested_phase(), SessionPhase::Phase8);
    assert_eq!(cheat_flags, 0);
}
