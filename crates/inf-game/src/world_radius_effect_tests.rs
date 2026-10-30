use super::*;
use crate::world_map::{MapLoadContext, MapLoadState, ProcessWorldMapRuntime};

fn block() -> RadiusEffectBlock {
    RadiusEffectBlock {
        strength: -1001,
        channel: 9,
        byte_03: None,
        inner_radius: 100,
        outer_radius: 300,
        impulse: 256,
    }
}

#[test]
fn falloff_keeps_signed_strength_quantization_and_unknown_copied_byte() {
    let effect = scale_radius_effect(block(), 200, None).unwrap();
    assert_eq!(effect.strength, -501);
    assert_eq!(effect.impulse_magnitude, 128);
    assert_eq!(effect.channel, 9);
    assert_eq!(effect.byte_03, None);
    assert!(effect.accepted());
    assert_eq!(
        scale_radius_effect(block(), 100, None).unwrap().strength,
        -1001
    );
    assert_eq!(scale_radius_effect(block(), 300, None), None);
    assert_eq!(scale_radius_effect(block(), 301, None), None);
}

#[test]
fn unsigned_outer_radius_and_signed_distance_are_independent() {
    let block = RadiusEffectBlock {
        outer_radius: 0x8000,
        ..block()
    };
    assert!(scale_radius_effect(block, 0x7fff, None).is_some());
    assert_eq!(scale_radius_effect(block, 0x8000, None), None);
    let effect = scale_radius_effect(block, -1, None).unwrap();
    assert_eq!(effect.strength, -1001);
    assert_eq!(effect.impulse_magnitude, 256);
}

#[test]
fn impulse_destination_changes_acceptance_and_uses_truncating_word_division() {
    let block = RadiusEffectBlock {
        strength: 0,
        ..block()
    };
    let effect = scale_radius_effect(block, 200, Some([-199, 3, 200])).unwrap();
    assert!(effect.accepted());
    assert_eq!(effect.impulse_vector, Some([-127, 1, 128]));
    assert!(!scale_radius_effect(block, 200, None).unwrap().accepted());
    assert_eq!(
        scale_radius_effect(block, 0, Some([0, -1, 1]))
            .unwrap()
            .impulse_vector,
        Some([0, -256, 256])
    );
}

#[test]
fn authored_bytes_and_large_word_length_preserve_original_bit_patterns() {
    let block =
        RadiusEffectBlock::from_authored_bytes([0xe8, 0x03, 9, 0xa5, 0, 0, 0xf4, 1, 0xc8, 0]);
    assert_eq!(block.strength, 1000);
    assert_eq!((block.channel, block.byte_03), (9, Some(0xa5)));
    assert_eq!(
        (block.inner_radius, block.outer_radius, block.impulse),
        (0, 500, 200)
    );
    assert_eq!(radius_word_distance([3, 4, 0]), 5);
    assert_eq!(radius_word_distance([i16::MIN, i16::MIN, 0]), 46_340);
    assert_eq!(radius_word_distance([i16::MIN; 3]), 56_754);
}

#[test]
fn mission22_sequential_actors_keep_unwritten_byte_1a3_without_command20() {
    use crate::{frontend_assets::FrontendAssets, Installation, ResidencyRequest, ResidentRuntime};
    use inf_formats::FrontendExecutable;

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let grants = inf_formats::parse_grant_table(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let offline = crate::offline_prepare::offline_game(&grants).unwrap();
    let mut maps = ProcessWorldMapRuntime::native_startup();
    let mut map_load = MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut load_owner = crate::world_backend::WorldLoadOwner::import_saved(
        crate::world_backend::OfflineWorldLoadRequest {
            mission: offline.mission,
            player: offline.player,
            previous_world: 1,
            visibility: None,
        },
    )
    .unwrap();
    let mut load_listener_effects = crate::world_backend::EmptyListenerEffects;
    //Explicit startup-proven fixture process owner, shared by retries below.
    let mut furniture_radius = crate::terrain_runtime::ProcessFurnitureRadius::native_startup(
        inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap();
    let load_metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                crate::terrain_runtime::TerrainSpacing::Units256,
            ),
            sound: crate::object_runtime::SoundTeardownService::RequiredAtReach,
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut load_owner,
            listener_effects: &mut load_listener_effects,
            policy: crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
            particles: None,
            phase: None,
            assets: &assets,
            process: Some(
                &crate::process_particles::factories::ParticleFactoryProcessState::native_startup(),
            ),
            map: MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            furniture: None,
            settings_84: 0,
            ultra: None,
        },
    )
    .unwrap();
    let world = crate::world_backend::WorldSession::from_loaded(load_owner, load_metadata);
    let placements = crate::world_placement::WorldPlacements::new(
        &runtime,
        crate::terrain_runtime::TerrainSpacing::Units256,
    )
    .unwrap();
    for reference in [30, 31] {
        let input = placements.input(reference - 1).unwrap();
        assert_eq!(input.resource_argument, 0xffff_000b);
        assert_eq!(
            input
                .instance
                .commands
                .iter()
                .map(|command| command.code.value)
                .collect::<Vec<_>>(),
            [2, 6],
            "mission22 placement reference {reference} has no authored block replacement"
        );
        let actor = world
            .arena
            .find_actor_by_placement(reference as i32)
            .unwrap();
        // 0047F770 accepts a one-based reference; the API performs the
        // subtraction before matching the constructor's zero-based +2A.
        assert_eq!(
            u32::from(world.arena.object(actor).unwrap().base.placement_word),
            input.placement_index
        );
        let state = world.arena.selector11_actor(actor).unwrap();
        assert_eq!(state.byte_1a3, None);
        assert_eq!(
            (
                state.word_1a0,
                state.byte_1a2,
                state.word_1a4,
                state.word_1a6,
                state.word_1a8
            ),
            (2000, 9, 50, 512, 256),
            "the known 004014F0 block fields retain constructor defaults"
        );
    }
}
