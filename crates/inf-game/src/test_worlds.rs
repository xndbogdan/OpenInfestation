//! Offline worlds for tests: the retail installation loaded into a world
//! session at a mission selection, with the given terrain spacing.

use inf_formats::FrontendExecutable;

use crate::{
    frontend_assets::FrontendAssets,
    process_particles::factories::ParticleFactoryProcessState,
    world_backend::{OfflineWorldLoadRequest, WorldSession},
    Installation, ResidencyRequest, ResidentRuntime,
};

pub(crate) fn offline_world(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load(
        selection,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
    )
}

/// [`offline_world`] at display resource tier `tier` (the `%dd%d.ovl`
/// variant): tier 2 serves a 640x480 frame.
pub(crate) fn offline_world_at_tier(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
    tier: u8,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_from(
        selection,
        None,
        Some(tier),
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::CleanImport,
    )
}

/// [`offline_world`] through production's retained-manager load, which lets
/// placement actors the load disposes run their death; a clean import stops
/// at them (Tabuchi's world 9 has one).
pub(crate) fn offline_world_retained(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load(
        selection,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
    )
}

/// [`offline_world`] from a save whose start pair (mission `+A4`) is
/// `start_pair` and whose player also knows `grants` (the `+9C` mask), with
/// the world owner's start left at 0 as the menu's load leaves it: the loader
/// must take the saved start.
pub(crate) fn offline_world_from_saved_start(
    selection: u32,
    start_pair: u16,
    grants: &[u32],
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_from(
        selection,
        Some((start_pair, grants)),
        None,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
    )
}

fn load(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
    policy: crate::world_backend::OfflineWorldLoadPolicy<'_>,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_from(selection, None, None, spacing, policy)
}

/// [`offline_world_retained`] from a save whose current world's four
/// completion words (`W+240`) are `completion`.
pub(crate) fn offline_world_with_completion(
    selection: u32,
    completion: [u32; 4],
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_with(
        selection,
        None,
        None,
        Some(completion),
        0,
        false,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        None,
    )
}

/// [`offline_world_retained`] with settings +84, the cheat word the load's
/// respawn reads, at `settings_84`.
pub(crate) fn offline_world_with_cheat_word(
    selection: u32,
    settings_84: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_with(
        selection,
        None,
        None,
        None,
        settings_84,
        false,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        None,
    )
}

fn load_from(
    selection: u32,
    saved: Option<(u16, &[u32])>,
    tier: Option<u8>,
    spacing: crate::terrain_runtime::TerrainSpacing,
    policy: crate::world_backend::OfflineWorldLoadPolicy<'_>,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_with(
        selection, saved, tier, None, 0, false, spacing, policy, None,
    )
}

/// An arena world loaded under a network session's process words: the
/// loader's network branches run (no 0047F6C0, the network flags on each
/// construction, the player's mode branch).
pub(crate) fn network_world(
    selection: u32,
    process: ParticleFactoryProcessState,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_with(
        selection,
        None,
        None,
        None,
        0,
        false,
        crate::terrain_runtime::TerrainSpacing::Units256,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        Some(process),
    )
}

/// [`offline_world_retained`] with the process sounds and particle pool
/// lent, as production's load lends them: a respawn whose vehicle carries
/// items runs their carry effect (worlds that start the player holding some,
/// such as the Alien Brain's).
pub(crate) fn offline_world_with_services(
    selection: u32,
    spacing: crate::terrain_runtime::TerrainSpacing,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    load_with(
        selection,
        None,
        None,
        None,
        0,
        true,
        spacing,
        crate::world_backend::OfflineWorldLoadPolicy::RetainedManager { text_data: None },
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn load_with(
    selection: u32,
    saved: Option<(u16, &[u32])>,
    tier: Option<u8>,
    completion: Option<[u32; 4]>,
    settings_84: u32,
    services: bool,
    spacing: crate::terrain_runtime::TerrainSpacing,
    policy: crate::world_backend::OfflineWorldLoadPolicy<'_>,
    network: Option<ParticleFactoryProcessState>,
) -> (ResidentRuntime, FrontendAssets, WorldSession) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    if let Some(difficulty) = tier {
        runtime
            .apply(ResidencyRequest::Select {
                selection: crate::ResourceSelection {
                    difficulty,
                    language: 0,
                },
            })
            .unwrap();
    }
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let grants = inf_formats::parse_grant_table(executable).unwrap();
    let mut offline =
        crate::offline_prepare::offline_game_with_selection(&grants, Some(selection)).unwrap();
    if let Some(words) = completion {
        let world = selection as usize * 4;
        offline.mission.owner_240[world..world + 4].copy_from_slice(&words);
    }
    if let Some((start_pair, extra)) = saved {
        offline.mission.owner_a4 = start_pair;
        for &id in extra {
            inf_formats::grant_into(&mut offline.player.owner_a0, &grants, id).unwrap();
        }
    }
    let mut maps = crate::world_map::ProcessWorldMapRuntime::native_startup();
    let mut map_load = crate::world_map::MapLoadState::native_mission_constructor();
    let mut rng = 0;
    let mut owner = crate::world_backend::WorldLoadOwner::import_saved(OfflineWorldLoadRequest {
        mission: offline.mission,
        player: offline.player,
        previous_world: 1,
        visibility: None,
    })
    .unwrap();
    if saved.is_some() {
        owner.world_start.pair_index = 0;
    }
    let mut listener_effects = crate::world_backend::EmptyListenerEffects;
    let mut furniture_radius =
        crate::terrain_runtime::ProcessFurnitureRadius::native_startup(executable).unwrap();
    let mut furniture_query =
        crate::world_furniture_effect::ProcessFurnitureQueryState::native_startup(executable)
            .unwrap();
    let mut sounds = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let mut voices = crate::process_sound::RequiredProcessSoundVoiceTeardown;
    let mut particles = crate::process_particles::ProcessParticleRuntime::default();
    let metadata = crate::world_backend::load_offline_world(
        &mut runtime,
        crate::world_backend::OfflineWorldLoadInputs {
            terrain_policy: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(spacing),
            sound: if services {
                crate::object_runtime::SoundTeardownService::Available {
                    sounds: &mut sounds,
                    voices: &mut voices,
                }
            } else {
                crate::object_runtime::SoundTeardownService::RequiredAtReach
            },
            furniture_radius: &mut furniture_radius,
            reads: crate::loading_integration::WorldLoadReads::NoCallbacks,
            owner: &mut owner,
            listener_effects: &mut listener_effects,
            policy,
            particles: services.then_some(&mut particles),
            // Session +64C during a load: phase 3.
            phase: services.then_some(3),
            assets: &assets,
            map: crate::world_map::MapLoadContext {
                process: &mut maps,
                state: &mut map_load,
            },
            rng: &mut rng,
            process: Some(&network.unwrap_or(ParticleFactoryProcessState::native_startup())),
            // Settings +70 that selects this spacing.
            furniture: Some(crate::world_backend::OfflineFurnitureLoan {
                detail: u32::from(spacing == crate::terrain_runtime::TerrainSpacing::Units128),
                query: &mut furniture_query,
            }),
            settings_84,
            ultra: None,
        },
    )
    .unwrap();
    (runtime, assets, WorldSession::from_loaded(owner, metadata))
}
