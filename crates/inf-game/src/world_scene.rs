//! Native gameplay world draw: screen draw `00442080` through world draw
//! `00488020` using the retained world, resources and explicit view policy.
//!
//! Provenance (see `GAMEPLAY_RUNTIME.md`, observed first-frame submission):
//! - View setup publishes focal globals `00547660/64` from `00546F20/24`
//!   (`view_focals`, exact `00442080` math, BSS-zero start).
//! - Depth state goes through renderer virtual `+10` (`004BC9B0`) with
//!   `(1,1)` when settings `+50` (Z Buffering) is set, else `(0,0)`.
//! - Shared Flag0 manager 00479570 owns traversal, terrain/cone admission,
//!   near class callbacks, packet lighting and late model/component custody.
//!   Actual source/clock callbacks and both explicit H8 roots share the queue.
//! - Terrain traverses the shared native domains/strips through `0044AA10`;
//!   its pixel callbacks retain successfully submitted prefixes when later
//!   per-item H1/primitive effects reach an unowned service.
//! - Sky/backdrop draws first through `00440DA0` (H8 `0x116`) at the camera
//!   eye's X/Z and the world's H14/0 backdrop height; the session-flag and
//!   capacity gates are vacuous here.
//!
//! Labeled approximations (replaceable, never captured values as behavior):
//! - `WorldProjectionSetupApproximation`: the existing Setup command groups
//!   near/far fade, fog and projection publication. Native near precedes H7
//!   material, far follows it, and fog follows camera publication. This
//!   composition preserves acquisition/slew/material/late-camera order, not
//!   individual native context-store fault prefixes.
//! - Genuine NULL camera acquisition finishes fade/material, then requires
//!   complete retained native context/global view ownership. It preserves the
//!   backend view and shift without treating them as proven synchronized stores.
//! - Infrared vision (the world manager's +5C3) selects H7 0x20 far colour,
//!   no sky, a green player light and green-scaled deposits; particle and
//!   admitted object callbacks share the live settings+80 grid (see
//!   OBJECT_LIGHTING.md).
//! - UnownedObjectDrawCallbackApproximation retains reached manager/model
//!   prefixes and omits only the current object's remaining callback feature.
//!   Network vehicle draw, custom vertices and required backing stay explicit
//!   (see OBJECT_DRAW_RUNTIME.md); queue corruption remains hard.
//!
//! Skipped per feature (logged, counted, never per-phase):
//! - unsupported per-item furniture H1/primitive effects, terrain emitter draws,
//!   particle shadow suffixes,
//!   `00441510` element draws
//!   (HUD/overlays), text, per-tick simulation. The worker tail (`+84`
//!   virtual loop) is out of scope with the elements.

use crate::frontend_camera::{AnimationClock, AnimationClockSource};
use crate::frontend_scene::{
    FrontendSceneHost, ModelCallbackPolicy, SceneCommand, SceneModelSubmission, SceneProjection,
    SceneSetup, SceneView,
};
use crate::render_resources::{
    RenderResourceData, RenderResourceError, RenderResourceKind, RenderResourceRequest,
};
use crate::scene_resources::{ResidentSceneHost, SceneResourceBackend, SceneResourceError};
use crate::session_platform::{WorldCameraFactoryContext, WorldCameraInputs};
use crate::software_scene::{SoftwareObjectDrawHost, SoftwareScene, SoftwareSceneError};
use crate::world_backend::WorldSession;

/// Screen draw `00442080` view setup over the focal globals.
///
/// `current` is `00547660/64`, `src` is `00546F20/24`. Zero currents
/// initialize from `src`; gameplay `+5C5 == 0` selects the `-0x50` decay
/// (`decay = true`), while special view selects quarter easing toward the
/// `+0x5C0`-scaled target. All arithmetic wraps like the original.
pub fn view_focals(current: [i32; 2], src: [i32; 2], decay: bool, scale_5c0: i16) -> [i32; 2] {
    let mut focal = current;
    for axis in 0..2 {
        if focal[axis] == 0 {
            focal[axis] = src[axis];
        }
    }
    if decay {
        for axis in 0..2 {
            if src[axis] < focal[axis] {
                focal[axis] = focal[axis].wrapping_sub(0x50);
            } else {
                focal[axis] = src[axis];
            }
        }
        return focal;
    }
    for axis in 0..2 {
        let scaled = (i32::from(scale_5c0).wrapping_mul(src[axis]).wrapping_mul(3) >> 0xf) as i16;
        let delta = focal[axis]
            .wrapping_sub(i32::from(scaled))
            .wrapping_sub(src[axis]);
        focal[axis] = focal[axis].wrapping_sub((delta.wrapping_add((delta >> 31) & 3)) >> 2);
    }
    focal
}

/// Renderer virtual `+10` (`004BC9B0`) arguments from settings `+50`.
/// Observed `(1,1)` on the first frame with Z Buffering at its default.
pub fn depth_policy(z_buffering: bool) -> (u32, u32) {
    if z_buffering {
        (1, 1)
    } else {
        (0, 0)
    }
}

/// Inputs that vary per frame around the world's retained native state.
#[derive(Clone, Copy, Debug)]
pub struct WorldFrameInputs {
    /// Template `005474E8` focals (`00546F20/24`), retail's game's; the
    /// world path publishes them as-is, never doubled like the frontend.
    pub template_focal: [i32; 2],
    /// The view shown's focal source: the template's, or the remastered
    /// camera's wider one ([`crate::camera_clearance::focal`]), which the
    /// remaster's game takes too.
    pub focal_src: [i32; 2],
    /// Port option, the remastered camera's speed kick: the scale on the
    /// slewed focals for the view drawn (1 none), past [`view_focals`],
    /// whose `0x50` step answers any drop in its source at all.
    pub focal_kick: f32,
    /// G+5C0 controls both focal easing and header-distance adjustment.
    /// The current production input policy retains constructor zero until
    /// 00421DC0's player look/zoom controls are owned.
    pub scale_5c0: i16,
    /// Retained G+5C5, published by selector16 pose before its target callbacks.
    /// False selects native focal decay; true selects special-view easing.
    pub special_view_active: bool,
    /// Live surface pixels for viewport and center.
    pub surface: [i32; 2],
    /// Live settings `+50` (Z Buffering) word.
    pub z_buffering: bool,
    /// Live settings +80 controls dynamic terrain-light accumulation/readback.
    pub dynamic_lighting: bool,
    /// Bytes of the view shown's command arena (the remaster's game's too):
    /// [`world_arena_bytes`] of `surface` and its focal. Retail's game keeps
    /// [`RETAIL_ARENA_BYTES`].
    pub arena_bytes: u32,
    /// Port fix, not a retail setting: opcode 2A's mirror copy, which joins
    /// a right-side gun mount to the right of the hull in the hover morph.
    pub mirror_copy: crate::model_vertices::MirrorCopySource,
    /// Port fix, not a retail setting: the settled hovercraft's rear fans
    /// drawn the right way out
    /// ([`crate::model_execution::hovercraft_fans_unmirrored`]).
    pub hovercraft_fans: bool,
    /// Port fix, not a retail setting: the guided missiles' mounts fold and
    /// unfold through a mode change ([`crate::object_runtime::guided_mount_deploy`]).
    pub guided_mounts: bool,
    /// Live settings +68/+6C, for the child factory a weapon shot uses.
    pub camera_settings: crate::object_runtime::CameraSettings,
    /// Live settings +7C: a type-2 target's refresh also moves its point.
    pub settings_7c: u32,
    /// Live settings +70, the detail a Force Field's draw rings by.
    pub settings_70: u32,
    /// Each network slot's initials (`0054E71C`), for the vehicles' name
    /// tags.
    pub network_initials: [[u8; 6]; crate::network::game::SLOTS],
    /// The host's team flag `0054E81C`, for Powerball's vehicles' draw.
    pub network_team_flag: u8,
    /// The world manager's byte +5C3, infrared vision.
    pub infrared: bool,
    /// The game the frame plays ([`crate::presentation::GameView::for_frame`]).
    pub game: crate::presentation::GameView,
    /// The fog's `far` and `past` the traced remaster's game sees this
    /// world by, as its session keeps them; none: the build's tuning.
    pub game_far: Option<[f32; 2]>,
}

/// Retail's command arena: the arena preamble `00439270` replaces it with
/// this many bytes before every world frame.
pub const RETAIL_ARENA_BYTES: u32 = 0xc8000;

/// Bytes of a world frame's command arena for a frame `surface` pixels
/// drawn with `focal` (the frame's own, the Remastered camera's widening
/// and speed kick in it). Retail's [`RETAIL_ARENA_BYTES`] holds retail's
/// view, a focal half the 4:3 width (1 across, 3/4 down); at a fast
/// machine's full draw distance with fine terrain, Tabuchi's start already
/// fills 85% of it. A view reaching wider or taller keeps terrain tiles and
/// objects that retail's rejects, so the arena grows with its reach on each
/// axis: a widescreen frame at retail's focal by its width over 4:3's, the
/// Remastered camera's 100 degrees by its narrower focal (a
/// Tabuchi run, 2026-10-28: retail's bytes overflowed on 1,253 of 3,196
/// world frames and the ground's strips went missing). Retail's view keeps
/// retail's arena, and with it retail's overflow.
pub fn world_arena_bytes([width, height]: [i32; 2], focal: [i32; 2]) -> u32 {
    let reach = |size: i32, focal: i32, retail: f64| {
        (f64::from(size.max(0)) / 2.0 / f64::from(focal.max(1)) / retail).max(1.0)
    };
    let scale = reach(width, focal[0], 1.0) * reach(height, focal[1], 0.75);
    (f64::from(RETAIL_ARENA_BYTES) * scale).min(f64::from(u32::MAX)) as u32
}

/// Mutable world state and process allocations have different lifetimes.
/// Keep them distinct when the world submits into the shared renderer.
pub struct WorldDrawRequest<'a> {
    pub pickup_text_data: Option<crate::grant_text::PickupTextData<'a>>,
    pub world: &'a mut WorldSession,
    /// Actual process005471C0 owner, independently of camera/scalar inputs.
    pub animation: &'a dyn AnimationClockSource,
    pub object_draw_process: &'a mut crate::object_runtime::ObjectDrawProcessState,
    pub attract_demo: &'a bool,
    pub particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    pub light_grid: &'a mut crate::world_lighting::WorldLightGrid,
    pub executable: inf_formats::FrontendExecutable<'a>,
    pub object_light_policy: &'a mut dyn crate::object_runtime::ObjectLightPolicySource,
    pub object_light_failure_policy: crate::object_runtime::Flag0LightFailurePolicy,
    pub camera: WorldCameraInputs<'a>,
    pub furniture: WorldFurnitureInputs<'a>,
    /// Production supplies the retained process queue. Pixel-only fixtures
    /// can explicitly leave audio outside their validation scope.
    pub sound: Option<WorldDrawSound<'a>>,
    pub weapon_tables: Option<&'a mut crate::object_runtime::ProcessActorBehaviorTables>,
    /// Process DWORD 005483B0, the terrain event (a stabiliser's death),
    /// which gates the near strips' dust (`0044C4A0`).
    pub terrain_event_5483b0: u32,
    /// G+1B4's jitter, which a weapon's Sonic Shock pulse sets; None stops
    /// the pulse there.
    pub map: Option<&'a mut crate::world_map::WorldMapState>,
    /// Port option, not retail: the remastered camera's owners, which lift
    /// the drawn view over what blocks it ([`crate::camera_clearance`]);
    /// None draws retail's camera.
    pub camera_clearance: Option<crate::camera_clearance::CameraClearanceLoan<'a>>,
}

/// World and process owners needed by the shared furniture phase. Birth phase
/// and source handle come from the caller's live session, not sound policy.
pub struct WorldFurnitureInputs<'a> {
    pub state: &'a mut crate::terrain_furniture::draw::FurnitureDrawState,
    pub pending: &'a crate::world_furniture_effect::FurnitureEffectQueue,
    pub birth_inputs: crate::process_particles::births::ParticleBirthInputs<'a>,
}

pub struct WorldDrawSound<'a> {
    pub queue: &'a mut crate::process_sound::ProcessSoundRuntime,
    pub sink: Option<
        &'a mut dyn crate::process_sound::ProcessSoundAudio<
            Error = crate::platform_audio::PlatformAudioError,
        >,
    >,
    pub effects_volume: u32,
    pub phase: u32,
}

/// Per-feature outcome of one native world draw.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WorldDrawSkips {
    /// Current object features omitted after their reached native prefixes.
    pub object_draw: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDrawOutcome {
    pub submitted_models: usize,
    pub skips: WorldDrawSkips,
    /// True when no owned terrain command reached the backend.
    pub skipped_terrain: bool,
    /// Actual queue submissions, including clip-generated triangles and
    /// prefixes retained before a later feature/strip failure.
    pub terrain_commands: usize,
    /// Actual water queue subset; clipped fan triangles count separately.
    pub water_commands: usize,
    pub terrain_omissions: usize,
    pub furniture_models: usize,
    pub furniture_omissions: usize,
    /// Rejections from completed strips; a failed strip may retain an
    /// uncounted prefix. Submission totals instead use the backend counter.
    pub terrain_completed_strip_rejections: usize,
    /// The sky model was missing or its submission failed.
    pub skipped_sky: bool,
    pub particle_commands: usize,
    pub particle_omissions: usize,
    /// Completed pass counts. A hard/strict stop retains writes but does not
    /// return its partial counters, so that receipt remains unknown.
    pub object_light_callbacks: Option<usize>,
    pub object_light_deposits: Option<usize>,
    pub object_light_omissions: usize,
    pub sounds_started: usize,
    pub sound_omissions: usize,
    /// The draw arena filled outside the terrain's strips (native 0x301).
    pub arena_overflow: bool,
    /// Port option: the view shown was drawn apart from the game's draw.
    pub two_pass: bool,
    /// Port option: the objects drawn for the tracer alone, outside its
    /// view or round its first-person eye (their near callbacks).
    pub outside: usize,
    pub focals: [i32; 2],
    pub depth: (u32, u32),
}

/// Passive receipt of the most recently reached world draw, independent of
/// the first-frame skip log. Raw basis words retain exact published values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldDrawReceipt {
    pub native_camera: bool,
    pub animation_clock: u32,
    pub eye: [i32; 3],
    pub basis_words: [u32; 10],
    pub coordinate_shift: u8,
    pub projection: crate::model_context::ModelProjection,
    pub terrain_height_at_eye: i16,
    pub outcome: WorldDrawOutcome,
    /// Port option: the remastered eye was under the water's live surface,
    /// whose sound the mix muffles.
    pub under_water: bool,
    /// Port option: where the view shown was drawn apart from the game's
    /// draw ([`WorldDrawOutcome::two_pass`]), the game's eye and projection
    /// and its draw's outcome; the fields above are the view shown's. Only
    /// what plays the game reads them: the carried actors' draw, and the
    /// status line's eye.
    pub game_eye: [i32; 3],
    pub game_projection: crate::model_context::ModelProjection,
    pub game_outcome: WorldDrawOutcome,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldDrawError {
    #[error("NULL predraw camera requires complete retained native context/global view ownership")]
    RequiredRetainedWorldView,
    #[error(transparent)]
    CameraAcquisition(#[from] crate::object_runtime::PlayerAttachError),
    #[error(transparent)]
    Camera(#[from] crate::object_runtime::CameraPoseError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Submit(#[from] SceneResourceError<SoftwareSceneError>),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Executable(#[from] inf_formats::FrontendExecutableError),
    #[error(transparent)]
    Profiling(#[from] crate::object_runtime::ProfilingError),
}

struct WorldParticleLighting<'a> {
    grid: &'a mut crate::world_lighting::WorldLightGrid,
    policy: crate::world_lighting::TerrainLightPolicy,
}

impl crate::process_particles::draw::ParticleDrawLighting for WorldParticleLighting<'_> {
    type Error = crate::world_lighting::TerrainLightError;
    fn terrain_light(
        &mut self,
        request: crate::world_lighting::TerrainLightRequest,
        emitter: [i32; 3],
    ) -> Result<(), Self::Error> {
        self.grid
            .add_from(request, self.policy, emitter, true)
            .map(|_| ())
    }
}

fn submit_particles<'r>(
    particles: &mut crate::process_particles::ProcessParticleRuntime,
    request: crate::process_particles::draw::ParticleDrawRequest<'_>,
    executable: inf_formats::FrontendExecutable<'_>,
    lighting: &mut WorldParticleLighting<'_>,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
    outcome: &mut WorldDrawOutcome,
    head: &str,
) {
    let report = host
        .backend
        .draw_particles(particles, request, executable, lighting);
    outcome.particle_commands += report.commands;
    outcome.particle_omissions += report.omissions.len();
    for omission in report.omissions {
        eprintln!(
            "{head} particle draw {:?} skipped after reached writes: {:?}",
            omission.id, omission.error
        );
    }
}

/// `00488072..004880C2`: under infrared vision the first player's unit
/// (`00488AA0`, `0046AE60`) lights the grid green at its X and Z, radius
/// `0xC18`, through `0045B1A0`. Retail reads NULL without a unit; the port
/// skips the light and logs that once.
fn infrared_player_light(
    arena: &mut crate::object_runtime::ObjectArena,
    grid: &mut crate::world_lighting::WorldLightGrid,
    policy: crate::world_lighting::TerrainLightPolicy,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static STOP_LOGGED: AtomicBool = AtomicBool::new(false);
    let result = (|| -> Result<(), String> {
        let player = arena
            .first_player()
            .ok_or("no first player (00488AA0 returns NULL)")?;
        let unit = arena
            .player_actor(player)
            .map_err(|error| error.to_string())?
            .ok_or("the first player has no unit (0046AE60 returns NULL)")?;
        let [x, _, z] = arena
            .object(unit)
            .map_err(|error| error.to_string())?
            .base
            .position_words
            .ok_or("the unit's position is unknown")?;
        let before = grid.sources().len();
        grid.add(
            crate::world_lighting::TerrainLightRequest {
                world_xz: [x, z].map(|word| i32::from(word as i16)),
                radius: 0xc18,
                rgb: crate::world_tint::INFRARED_RGB.map(|byte| byte as i8),
            },
            policy,
        )
        .map_err(|error| error.to_string())?;
        // The thermal camera sees no heat in it.
        grid.mark_cold(before);
        Ok(())
    })();
    if let Err(error) = result {
        if !STOP_LOGGED.swap(true, Ordering::Relaxed) {
            eprintln!("Infrared vision's player light skipped: {error}");
        }
    }
}

const SKY_SLOT: i16 = 0x116;

/// `00440DA0`'s packet: H8 slot `0x116` with an identity basis. X and Z
/// are the render context's eye (`00440E03/0A`). Y is process DWORD
/// `00547668` (`00440DF5`), which gameplay screen constructor `004408B0`
/// stores from H14/0 header DWORD +10 (`0048EDA0`) at `00440910`, the same
/// height the briefing backdrop uses. The sky therefore follows the eye
/// horizontally but keeps the world's authored height.
pub(crate) fn world_sky_submission(eye: [i32; 3], backdrop_height: i32) -> SceneModelSubmission {
    SceneModelSubmission {
        model_slot: SKY_SLOT,
        position: [eye[0], backdrop_height, eye[2]],
        basis: crate::menu::MenuBasis::INITIAL,
        animation: AnimationClock::default(),
        primitive_context: crate::frontend_scene::ModelPrimitiveContext::Inherited,
        failure_policy: crate::frontend_scene::ModelFailurePolicy::StopScene,
        surface_table: crate::frontend_scene::ModelSurfaceTableOrigin::InheritedRenderer,
        policy: ModelCallbackPolicy::WorldSkyStaticValuesApproximation,
    }
}

/// Sky/backdrop submission (`00440DA0`): H8 slot `0x116` through H1,
/// drawn before entities like the worker does. The retail session-flag and
/// H8-capacity gates are vacuous on this path (world draw implies the
/// gameplay phase with active residency). Fail closed per feature:
/// on any failure `skipped_sky` stays set and entities still draw.
fn submit_sky<'r>(
    camera_eye: [i32; 3],
    backdrop_height: i32,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
    outcome: &mut WorldDrawOutcome,
) {
    let resolved = host
        .resources
        .resolve(crate::render_resources::RenderResourceRequest {
            kind: crate::render_resources::RenderResourceKind::Model,
            argument: i32::from(SKY_SLOT),
        });
    let Ok(resource) = resolved else {
        return;
    };
    if !matches!(
        resource.data,
        crate::render_resources::RenderResourceData::Model(_)
    ) {
        return;
    }
    let submission = world_sky_submission(camera_eye, backdrop_height);
    let result = host
        .submit(SceneCommand::BeginBatch)
        .and_then(|()| host.submit(SceneCommand::Model(submission)))
        .and_then(|()| host.submit(SceneCommand::FlushBatch));
    if result.is_ok() {
        outcome.submitted_models += 1;
        outcome.skipped_sky = false;
    }
}

/// `004501A0`: the world light. `00490030` first publishes the tint word
/// `005474E0` as the directional colour along the H12 light direction, while
/// the previous ambient is still current, so the effect colour becomes that
/// ambient plus half the tint. `00490130` then sets the ambient to the tint's
/// bytes shifted right by three, and the terrain surface callbacks follow.
/// The world draw calls it in pre-draw and again before the terrain pass.
fn publish_world_light<'r>(
    world: &WorldSession,
    animation_clock: u32,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<(), WorldDrawError> {
    for command in world_light_commands(&world.presentation) {
        host.submit(command)?;
    }
    host.backend
        .install_terrain_surface_callbacks(world.world_flags, animation_clock);
    Ok(())
}

/// `004501A0`'s two light publications, directional before ambient.
fn world_light_commands(
    presentation: &crate::world_backend::WorldPresentationState,
) -> [SceneCommand; 2] {
    let [r, g, b, _] = presentation.tint_word.to_le_bytes();
    [
        SceneCommand::DirectionalLight {
            direction: presentation.terrain_light_direction,
            rgb: [r, g, b],
        },
        SceneCommand::AmbientLight([r, g, b].map(|value| i32::from(value >> 3))),
    ]
}

/// Actual manager inputs vary by caller; retained world/RNG custody is shared.
struct WorldObjectDrawRequest<'a> {
    pickup_text_data: Option<crate::grant_text::PickupTextData<'a>>,
    context: &'a mut crate::object_runtime::ObjectDrawContext,
    process: &'a mut crate::object_runtime::ObjectDrawProcessState,
    residency: &'a crate::ResidentRuntime,
    light_grid: &'a crate::world_lighting::WorldLightGrid,
    executable: inf_formats::FrontendExecutable<'a>,
    network_flag: &'a u32,
    animation: &'a dyn AnimationClockSource,
    attract_demo: &'a bool,
    weapon_tables: Option<&'a mut crate::object_runtime::ProcessActorBehaviorTables>,
    particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    sounds: Option<&'a mut crate::process_sound::ProcessSoundRuntime>,
    birth_inputs: crate::process_particles::births::ParticleBirthInputs<'a>,
    camera_settings: crate::object_runtime::CameraSettings,
    settings_7c: u32,
    settings_70: u32,
    network_initials: &'a [[u8; 6]; crate::network::game::SLOTS],
    network_team_flag: u8,
    /// G+1B4's jitter, which Sonic Shock's pulse sets.
    map: Option<&'a mut crate::world_map::WorldMapState>,
}

/// Which objects [`submit_world_object_models`] draws.
#[derive(Clone, Copy)]
enum ObjectPass<'a> {
    /// The manager's pass `00479570`, its near callbacks in this cone.
    Manager(crate::object_runtime::ObjectDrawCone),
    /// Port option: these objects' near callbacks alone, on copies of the
    /// game ([`crate::object_runtime::ObjectArena::draw_near_callbacks`]).
    Listed(&'a [crate::object_runtime::ObjectId]),
}

/// Complete world-manager phase, including immediate reached RNG custody.
/// No recovery branch can return with a stale world owner after model failure.
fn submit_world_object_models(
    world: &mut WorldSession,
    scene: &mut SoftwareScene<'_, '_>,
    request: &mut WorldObjectDrawRequest<'_>,
    pass: ObjectPass<'_>,
) -> Result<
    crate::object_runtime::Flag0ObjectDrawOutcome,
    crate::object_runtime::Flag0ObjectDrawError<
        crate::world_actor_terrain::WorldActorTerrainError,
        SoftwareSceneError,
    >,
> {
    let executable = request.executable;
    scene.set_process_rng(Some(world.rng));
    // The owner's +4C for the shots a vehicle's draw fires (0047AAF0).
    let sine: [u16; 4096] = *scene.sine_table();
    let math = inf_formats::native_basis::NativeBasisMath::new(sine);
    let mut listener_effects =
        crate::session_platform::world_actor_factory::RequiredListenerEffects;
    let mut children = crate::session_platform::world_actor_factory::WorldActorChildHost {
        residency: request.residency,
        executable,
        process: request.birth_inputs.process,
        terrain: &world.terrain,
        world_flags: world.world_flags,
        clock: request.animation.animation_value(),
        sine: &sine,
        math: &math,
        camera_settings: Some(request.camera_settings),
        world_start: Some(&mut world.world_start),
        listeners: &mut world.listeners,
        listener_effects: &mut listener_effects,
    };
    let mut pulse = crate::session_platform::DrawPulseHost {
        flash: &mut world.flash,
        map: request.map.as_deref_mut(),
        residency: request.residency,
    };
    let effects = Some(crate::software_scene::SoftwareObjectEffectServices {
        tables: request.weapon_tables.as_deref_mut(),
        factory_process: request.birth_inputs.process,
        particles: &mut *request.particles,
        statistics: world.mission.record_0c.as_deref_mut(),
        sounds: request.sounds.as_deref_mut(),
        residency: request.residency,
        phase: request.birth_inputs.phase,
        pickup: Some(crate::software_scene::SoftwarePickupServices {
            text_data: request.pickup_text_data,
            terrain: &world.terrain,
            mission_save: &mut world.mission.save,
            current_world: &world.mission.current_world_a0,
            load_factor: world.fade.performance.particle_load_factor as i32,
        }),
        custom: Some(crate::software_scene::SoftwareCustomWorld {
            load_factor: world.fade.performance.particle_load_factor as i32,
            terrain: &world.terrain,
            flags: world.world_flags,
            update_byte: request.process.completion_53b890,
        }),
        children: Some(&mut children),
        pulse: Some(&mut pulse),
        settings_7c: Some(request.settings_7c),
        settings_70: Some(request.settings_70),
        network_initials: Some(request.network_initials),
        network_team_flag: Some(request.network_team_flag),
    });
    let draw = crate::object_runtime::Flag0ObjectDrawRequest {
        context: &mut *request.context,
        cone: match pass {
            ObjectPass::Manager(cone) => cone,
            ObjectPass::Listed(_) => crate::object_runtime::ObjectDrawCone::Retail,
        },
        process: &mut *request.process,
        network_flag: request.network_flag,
        terrain: &mut crate::world_actor_terrain::WorldActorTerrainHost {
            residency: request.residency,
            source: &world.terrain_source,
            prepared: &world.terrain,
        },
        lighting: crate::object_runtime::ObjectDrawLighting {
            grid: request.light_grid,
            tint_word: &mut world.presentation.tint_word,
            direction: world.presentation.terrain_light_direction,
            spacing: world.terrain.sampling().spacing,
            executable,
        },
        host: &mut SoftwareObjectDrawHost {
            terrain: crate::software_scene::ModelTerrainRead::Live(&world.terrain),
            effects,
            scene,
            animation: request.animation,
            executable,
            attract_demo: request.attract_demo,
            failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
        },
        failure_policy: crate::object_runtime::Flag0ObjectDrawFailurePolicy::UnownedObjectDrawCallbackApproximation,
    };
    let result = match pass {
        ObjectPass::Manager(_) => world.arena.run_flag0_object_draw_pass(draw),
        ObjectPass::Listed(objects) => world.arena.draw_near_callbacks(objects, draw),
    };
    world.rng = scene.process_rng().unwrap_or(world.rng);
    result
}

/// Native world draw for one frame: `00440F00` preamble, view setup, depth
/// state, sky, active particles, object lights, entity models, pending particles
/// and terrain.
/// Unowned elements, text and simulation skip per feature (see the module docs).
/// Fresh 00478E00 acquisition precedes fade/material and current camera reads.
/// NULL retains the backend view/shift and stops at its named context boundary.
/// How far above the ground the tracer places a deposited light, which the
/// grid holds by X/Z alone, and the least height it gives an emitter: an
/// explosion's particle may lie on the ground or under it.
const LIGHT_SOURCE_HEIGHT: i32 = 64;

/// Port option, the remastered camera under the water, where the world
/// draw lays its own look ([`UnderwaterLook::Wet`]): the fog's end (a
/// little nearer than the world manager's least far, `0x1400`; the terrain's
/// strips held there on Altair's and Arcturus's deep water), how bright the
/// water's colour lays it (Vulkan's raster lays its murk as this fog,
/// [`UnderwaterLook::Murk`]), what the world's colours keep, and how far
/// they take the water's hue.
const UNDERWATER_FAR: i32 = 0x1000;
const UNDERWATER_FOG_LIGHT: f32 = 0.35;
const UNDERWATER_DIM: f32 = 0.7;
const UNDERWATER_HUE: f32 = 0.5;

/// The tracer's water round an eye under it (`pt_frame.glsl`'s
/// `water_murk`): the distance over which it leaves 1/e of what lies
/// beyond, so that it hides half where that fog's fade is half done (2,176
/// units) and reads as the same water as Vulkan's raster's murk. Matched
/// at that fog's mean distance instead (1/e at 2,176 units), it showed 63%
/// of what lay 1,000 units off where the raster's shows 81% (this, 73%),
/// and read murkier than the raster's beside it.
pub(crate) const MURK_REACH: f32 = ((UNDERWATER_FAR / 16) as f32
    + (UNDERWATER_FAR - UNDERWATER_FAR / 16) as f32 / 2.0)
    / std::f32::consts::LN_2;

/// The water's colour where its texture is not found (the tracer's).
const WATER_HUE: [f32; 3] = [0.3, 0.5, 0.55];

/// Port option, the remastered camera: how the view from under the water's
/// live surface is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnderwaterLook {
    /// As any other: the eye is above the water, or the view lays its own
    /// medium there (the tracer, infrared vision's camera).
    Plain,
    /// The world draw's own (the software and OpenGL rasters, and Vulkan's
    /// where it cannot shade the water): V2000's wobble, the world's colours
    /// dimmed in a short fog of the water's colour with that fog for its
    /// sky, and no surface drawn. Only the view shown takes it: the game's
    /// draw sees the world there as above the water, its fade and all (the
    /// short fade would set the objects' pass radius, the terrain's
    /// footprints and the particles' flags).
    Wet,
    /// Vulkan's raster where it shades the water itself (not lava): the
    /// world draws as the same eye above the water has it, its fade, sky and
    /// water strips and all (as the tracer's does, and nearer retail, whose
    /// camera never goes under), and the device shows the view through the
    /// murk, the surface from below.
    Murk,
}

/// How the view from an eye `under_water` (its live surface) is drawn, on
/// a frame that `traces_rays` or is under `infrared` vision, whose device
/// shades the world's water itself or not (`device_water`).
pub(crate) fn underwater_look(
    under_water: bool,
    traces_rays: bool,
    infrared: bool,
    device_water: bool,
) -> UnderwaterLook {
    if !under_water || traces_rays || infrared {
        UnderwaterLook::Plain
    } else if device_water {
        UnderwaterLook::Murk
    } else {
        UnderwaterLook::Wet
    }
}

/// The view one pass of the world draw draws from, built before it
/// ([`world_view`]): the eye, the projection and what the view decides of
/// the draw (the fade, the underwater look, the objects' cone and the
/// arena).
#[derive(Clone, Copy, Debug)]
struct ViewSpec {
    view: SceneView,
    coordinate_shift: u8,
    /// The slewed focals, the speed kick in them.
    focals: [i32; 2],
    viewport: [i32; 2],
    /// The pre-draw fade; [`UnderwaterLook::Wet`] lays its own over it.
    fade: [i32; 2],
    /// H7 0x74's converted colour, 0x20's under infrared vision.
    far_color: u32,
    /// Port option: the remastered eye is under the water's live surface.
    under_water: bool,
    look: UnderwaterLook,
    cone: crate::object_runtime::ObjectDrawCone,
    arena_bytes: u32,
}

/// A view whose build failed, and what the draw had published by then in
/// 00488380's order: the pass's preamble always, the setup and the far
/// colour where they were reached.
struct ViewFailure {
    error: WorldDrawError,
    setup: Option<SceneSetup>,
    far_color: Option<u32>,
}

impl ViewFailure {
    fn at_preamble(error: impl Into<WorldDrawError>) -> Self {
        Self {
            error: error.into(),
            setup: None,
            far_color: None,
        }
    }
}

impl ViewSpec {
    /// Whether a draw from this view plays the same game as one from
    /// `other`: all of it but how the frame shows it (under the water,
    /// Vulkan's murk draws the world as above it, its fade and all).
    fn plays_as(&self, other: &Self) -> bool {
        let wet = |spec: &Self| spec.look == UnderwaterLook::Wet;
        self.view == other.view
            && self.coordinate_shift == other.coordinate_shift
            && self.focals == other.focals
            && self.viewport == other.viewport
            && self.fade == other.fade
            && self.far_color == other.far_color
            && wet(self) == wet(other)
            && self.cone == other.cone
            && self.arena_bytes == other.arena_bytes
    }
}

/// Which of the world draw's passes [`draw_world_pass`] draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorldPass {
    /// The frame's one pass: the view shown plays the game.
    Whole,
    /// Port option: the game's draw, apart from the view shown, which
    /// shows nothing ([`SoftwareScene::set_game_draw`]): no device world,
    /// trace or thermal camera.
    Game,
    /// Port option: the view shown after it, apart from the game
    /// ([`GameAsDrawn`]), which serves no sound.
    View,
}

impl WorldDrawRequest<'_> {
    /// The same owners lent again, for one pass of the world draw (the
    /// remastered camera's already taken by [`world_views`]).
    fn reborrow(&mut self) -> WorldDrawRequest<'_> {
        WorldDrawRequest {
            pickup_text_data: self.pickup_text_data,
            world: &mut *self.world,
            animation: self.animation,
            object_draw_process: &mut *self.object_draw_process,
            attract_demo: self.attract_demo,
            particles: &mut *self.particles,
            light_grid: &mut *self.light_grid,
            executable: self.executable,
            object_light_policy: &mut *self.object_light_policy,
            object_light_failure_policy: self.object_light_failure_policy,
            camera: self.camera,
            furniture: WorldFurnitureInputs {
                state: &mut *self.furniture.state,
                pending: self.furniture.pending,
                birth_inputs: self.furniture.birth_inputs,
            },
            sound: self.sound.as_mut().map(|sound| WorldDrawSound {
                queue: &mut *sound.queue,
                sink: match &mut sound.sink {
                    Some(sink) => Some(&mut **sink),
                    None => None,
                },
                effects_volume: sound.effects_volume,
                phase: sound.phase,
            }),
            weapon_tables: self.weapon_tables.as_deref_mut(),
            terrain_event_5483b0: self.terrain_event_5483b0,
            map: self.map.as_deref_mut(),
            camera_clearance: None,
        }
    }
}

pub fn draw_world_frame<'r>(
    mut request: WorldDrawRequest<'_>,
    frame: WorldFrameInputs,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<WorldDrawOutcome, WorldDrawError> {
    crate::crash_log::step(crate::crash_log::Step::WorldDraw);
    let _world_draw = crate::cpu_profile::scope("world: draw (all)");
    let views = world_views(&mut request, &frame, host);
    draw_world_views(request, frame, views, host)
}

/// The world draw from its views ([`world_views`], the game's and the
/// view shown): one pass where the view shown plays the game's (retail's
/// view drawn as retail draws it, or the remaster's as the frame shows it).
/// Otherwise (the AGENTS.md renderers' roles: software and OpenGL play
/// retail's game under any camera or scaling) the game's draw runs on the
/// live game and shows nothing, then the view shown draws apart from it,
/// on copies of all it could write ([`GameAsDrawn`]): its runtime
/// allocations refuse writes (an object whose draw meets one is left out
/// of the picture), it makes no particles and writes no event line, and
/// it serves no sound. Its picture alone stays. It draws from the game as
/// the game's draw left it: the shots that draw fired, the pickups it
/// collected and the particles it made are the frame's.
fn draw_world_views<'r>(
    mut request: WorldDrawRequest<'_>,
    frame: WorldFrameInputs,
    views: Result<[ViewSpec; 2], ViewFailure>,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<WorldDrawOutcome, WorldDrawError> {
    let [game, shown] = match views {
        Ok(views) => views,
        Err(failure) => {
            // The scene as far as the game's draw had reached when the view
            // failed.
            let arena_bytes = if frame.game == crate::presentation::GameView::Retail {
                RETAIL_ARENA_BYTES
            } else {
                frame.arena_bytes
            };
            begin_world_pass(
                request.world,
                &frame,
                arena_bytes,
                request.camera.animation_clock,
                host,
            )?;
            if let Some(setup) = failure.setup {
                host.submit(SceneCommand::Setup(setup))?;
            }
            if let Some(far_color) = failure.far_color {
                host.submit(SceneCommand::FarColor(far_color))?;
            }
            return Err(failure.error);
        }
    };
    if game.plays_as(&shown) {
        let receipt = draw_world_pass(request.reborrow(), frame, &shown, WorldPass::Whole, host)?;
        request.world.last_draw = Some(receipt);
        return Ok(receipt.outcome);
    }
    host.backend.set_game_draw(Some(game.viewport))?;
    let drawn = draw_world_pass(request.reborrow(), frame, &game, WorldPass::Game, host);
    host.backend.set_game_draw(None)?;
    let mut game = drawn?;
    game.outcome.two_pass = true;
    game.game_outcome = game.outcome;
    // A view shown that fails leaves the game's receipt.
    request.world.last_draw = Some(game);
    let shown_scope = crate::cpu_profile::scope("world: view shown");
    let drawn = crate::event_log::silenced(|| {
        crate::object_runtime::refusing_writes(|| {
            let kept = GameAsDrawn::set_aside(&mut request.owners(), host.backend)?;
            let drawn = draw_world_pass(request.reborrow(), frame, &shown, WorldPass::View, host);
            kept.put_back(&mut request.owners(), host.backend)?;
            drawn
        })
    });
    drop(shown_scope);
    let mut receipt = drawn?;
    receipt.outcome.two_pass = true;
    receipt.game_eye = game.eye;
    receipt.game_projection = game.projection;
    receipt.game_outcome = game.outcome;
    request.world.last_draw = Some(receipt);
    Ok(receipt.outcome)
}

/// Port option: the game as its draw left it, set aside while the view
/// shown draws on copies of all it could write, and put back after, the
/// copies dropped (a copy of the arena keeps its identity and shares its
/// runtime allocations, which refuse writes meanwhile, and a copy of a
/// network menu never closes the session): the world's RNG, arena, tint,
/// mission save and statistics, world start, creation listeners, flash,
/// light delta and the ground's furniture and plane; the object process
/// state, particles (the copy making none), light grid and furniture bases
/// lent, and the sounds, weapon tables and map jitter where lent; the
/// scene's process globals and model scratch.
struct GameAsDrawn<'r> {
    rng: u32,
    tint: u32,
    light_delta: u32,
    arena: crate::object_runtime::ObjectArena,
    statistics: Option<Box<[u8; crate::world_mission::RECORD_0C_BYTES]>>,
    save: inf_formats::MissionSaveState,
    world_start: crate::world_start::WorldStartState,
    listeners: crate::object_runtime::CreationListeners,
    flash: crate::world_flash::WorldFlashState,
    terrain: (Vec<inf_formats::TerrainBucket>, i32),
    process: crate::object_runtime::ObjectDrawProcessState,
    particles: crate::process_particles::ProcessParticleRuntime,
    light_grid: crate::world_lighting::WorldLightGrid,
    furniture: crate::terrain_furniture::draw::FurnitureDrawState,
    sounds: Option<crate::process_sound::ProcessSoundRuntime>,
    tables: Option<crate::object_runtime::ProcessActorBehaviorTables>,
    map: Option<crate::world_map::WorldMapState>,
    globals: crate::software_scene::SceneGlobals<'r>,
    scratch: crate::model_scratch::ModelScratchSave,
}

/// `live` as it is, a copy left in its place.
fn set_aside<T: Clone>(live: &mut T) -> T {
    let copy = live.clone();
    std::mem::replace(live, copy)
}

/// The owners of the game a world draw lends that [`GameAsDrawn`] copies.
struct GameOwners<'a> {
    world: &'a mut WorldSession,
    process: &'a mut crate::object_runtime::ObjectDrawProcessState,
    particles: &'a mut crate::process_particles::ProcessParticleRuntime,
    light_grid: &'a mut crate::world_lighting::WorldLightGrid,
    furniture: &'a mut crate::terrain_furniture::draw::FurnitureDrawState,
    sounds: Option<&'a mut crate::process_sound::ProcessSoundRuntime>,
    tables: Option<&'a mut crate::object_runtime::ProcessActorBehaviorTables>,
    map: Option<&'a mut crate::world_map::WorldMapState>,
}

impl WorldDrawRequest<'_> {
    fn owners(&mut self) -> GameOwners<'_> {
        GameOwners {
            world: &mut *self.world,
            process: &mut *self.object_draw_process,
            particles: &mut *self.particles,
            light_grid: &mut *self.light_grid,
            furniture: &mut *self.furniture.state,
            sounds: self.sound.as_mut().map(|sound| &mut *sound.queue),
            tables: self.weapon_tables.as_deref_mut(),
            map: self.map.as_deref_mut(),
        }
    }
}

impl<'r> GameAsDrawn<'r> {
    fn set_aside(
        owners: &mut GameOwners<'_>,
        scene: &mut SoftwareScene<'_, 'r>,
    ) -> Result<Self, SoftwareSceneError> {
        // The fallible steps first: a failed one leaves the game as it was.
        let globals = scene.save_globals()?;
        let scratch = scene.scratch.save()?;
        let world = &mut *owners.world;
        let kept = Self {
            rng: world.rng,
            tint: world.presentation.tint_word,
            light_delta: world.light_delta,
            arena: set_aside(&mut world.arena),
            statistics: set_aside(&mut world.mission.record_0c),
            save: set_aside(&mut world.mission.save),
            world_start: set_aside(&mut world.world_start),
            listeners: set_aside(&mut world.listeners),
            flash: set_aside(&mut world.flash),
            terrain: world.terrain.drawn(),
            process: set_aside(owners.process),
            particles: set_aside(owners.particles),
            light_grid: set_aside(owners.light_grid),
            furniture: set_aside(owners.furniture),
            sounds: owners.sounds.as_deref_mut().map(set_aside),
            tables: owners.tables.as_deref_mut().map(set_aside),
            map: owners.map.as_deref().copied(),
            globals,
            scratch,
        };
        owners.particles.refusing_births = true;
        Ok(kept)
    }

    fn put_back(
        self,
        owners: &mut GameOwners<'_>,
        scene: &mut SoftwareScene<'_, 'r>,
    ) -> Result<(), SoftwareSceneError> {
        let world = &mut *owners.world;
        world.rng = self.rng;
        world.presentation.tint_word = self.tint;
        world.light_delta = self.light_delta;
        world.arena = self.arena;
        world.mission.record_0c = self.statistics;
        world.mission.save = self.save;
        world.world_start = self.world_start;
        world.listeners = self.listeners;
        world.flash = self.flash;
        world.terrain.put_back_drawn(self.terrain);
        *owners.process = self.process;
        *owners.particles = self.particles;
        *owners.light_grid = self.light_grid;
        *owners.furniture = self.furniture;
        if let (Some(live), Some(queue)) = (owners.sounds.as_deref_mut(), self.sounds) {
            *live = queue;
        }
        if let (Some(live), Some(tables)) = (owners.tables.as_deref_mut(), self.tables) {
            *live = tables;
        }
        if let (Some(live), Some(map)) = (owners.map.as_deref_mut(), self.map) {
            *live = map;
        }
        // The fallible steps last, each tried.
        let scratch = scene.scratch.restore(self.scratch);
        scene.restore_globals(self.globals)?;
        Ok(scratch?)
    }
}

/// The frame's two views, in pre-draw 00488380's order of what it writes
/// of the world: the focals' slews, the camera's acquisition, the fades'
/// slews, then the camera's eye (the remastered camera's where it is
/// lent). The game's view is retail's in retail's game: the native eye,
/// the template's focal slewed with no kick, a 4:3 viewport the frame's
/// height, the header's fade, retail's cone and arena. In the remaster's
/// it is the view shown as any renderer would show it: no underwater look,
/// and the tracer's longer draw distance only where the session started
/// traced. The view shown is the frame's own.
fn world_views<'r>(
    request: &mut WorldDrawRequest<'_>,
    frame: &WorldFrameInputs,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<[ViewSpec; 2], ViewFailure> {
    let world = &mut *request.world;
    let camera = request.camera;
    let animation_clock = camera.animation_clock;
    let terrain_residency = camera.residency;
    let retail = frame.game == crate::presentation::GameView::Retail;
    // View setup first: the worker reads the published focals.
    //0040FD10 publishes +5C5 before mode-specific camera targets. Draw reads
    // that retained state even when a later camera feature fails. Production
    // supplies constructor-zero +5C0 until player look/zoom input is connected.
    let decay = !frame.special_view_active;
    let game_src = if retail {
        frame.template_focal
    } else {
        frame.focal_src
    };
    world.view_focals = view_focals(world.view_focals, game_src, decay, frame.scale_5c0);
    world.shown_focals = view_focals(world.shown_focals, frame.focal_src, decay, frame.scale_5c0);
    let kicked = |focals: [i32; 2]| {
        if frame.focal_kick == 1.0 {
            focals
        } else {
            focals.map(|focal| (focal as f32 * frame.focal_kick).round() as i32)
        }
    };
    let game_focals = if retail {
        world.view_focals
    } else {
        kicked(world.view_focals)
    };
    let shown_focals = kicked(world.shown_focals);
    // 0048E7D0 captures W+20 before W+1C, before 004883AA acquires anew.
    let header_near = world.mission.dword_20.expect("loaded world header");
    let header_far = world.mission.dword_1c;
    let mut preparation = WorldCameraFactoryContext::new(camera);
    let acquired = world
        .arena
        .world_camera_with_rng(
            &mut preparation.factory(&world.terrain, world.world_flags, &mut world.listeners),
            Some(&mut world.rng),
        )
        .map_err(ViewFailure::at_preamble)?;
    // Diagnostic identity only. Draw does not erase a preceding pose error.
    world.native_camera = acquired;

    // Port option: under the tracer a world may see further than retail's
    // draw distance (its fog tuning's `far`), its objects and far ground
    // drawn out to the new fog's end; the raster keeps retail's.
    // It also draws a little past its fog's end (`past`), so what comes
    // into view does so inside the whole fog (the feed's fog keeps the
    // end, `feed_world`). The remaster's game takes it from a session
    // that started traced.
    let fog = crate::modern_tuning::current().fog(world.mission.current_world_a0);
    let tuned = [fog.far, fog.past];
    let traced_far = |[far, past]: [f32; 2]| {
        let scale = far.max(0.1) * past.max(1.0);
        if scale != 1.0 {
            (header_far as f32 * scale).min(25_088.0 * past.max(1.0)) as u32
        } else {
            header_far
        }
    };
    let game_far = if frame.game == (crate::presentation::GameView::Remaster { traced: true }) {
        traced_far(frame.game_far.unwrap_or(tuned))
    } else {
        header_far
    };
    let shown_far = if host.backend.traces_rays() {
        traced_far(tuned)
    } else {
        header_far
    };
    let fade = world.fade.predraw(game_far, header_near, frame.scale_5c0);
    let shown_fade = world
        .fade
        .shown_predraw(shown_far, header_near, frame.scale_5c0);
    let [width, height] = frame.surface;
    let game_viewport = if retail {
        [width.min(height * 4 / 3), height]
    } else {
        frame.surface
    };
    let setup = SceneSetup::context(SceneProjection {
        focal: game_focals,
        viewport: game_viewport,
        fade,
    });
    // 00488446..004884B7: converted H7 0x74, then 0x20 under infrared vision.
    let mut far_color = 0;
    for argument in std::iter::once(0x74).chain(frame.infrared.then_some(0x20)) {
        let set_up = |error: RenderResourceError| ViewFailure {
            error: error.into(),
            setup: Some(setup),
            far_color: None,
        };
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        };
        let RenderResourceData::Material(material) =
            host.resources.resolve(request).map_err(set_up)?.data
        else {
            return Err(set_up(RenderResourceError::Registration { request }));
        };
        far_color = host.backend.color_layout().convert_material(material);
    }
    let coloured = |error: WorldDrawError| ViewFailure {
        error,
        setup: Some(setup),
        far_color: Some(far_color),
    };
    // 004884BA/004884C1: only now consume the captured camera's current
    // signed XYZ and all ten basis DWORDs. Whole-position error containment
    // does not claim native per-axis publication/fault parity.
    let camera = acquired
        .ok_or(WorldDrawError::RequiredRetainedWorldView)
        .map_err(coloured)?;
    let native_camera = world
        .arena
        .camera_draw_request(camera)
        .map_err(|error| coloured(error.into()))?;
    let mut shown_camera = native_camera;
    let remastered = request.camera_clearance.is_some();
    let sine: [u16; 4096] = *host.backend.sine_table();
    if let Some(loan) = request.camera_clearance.take() {
        shown_camera = crate::camera_clearance::view(
            loan,
            crate::camera_clearance::CameraClearanceWorld {
                arena: &world.arena,
                camera,
                residency: terrain_residency,
                source: &world.terrain_source,
                terrain: &world.terrain,
                furniture: request.furniture.state,
                animation: request.animation,
                executable: request.executable,
                shaking: request.terrain_event_5483b0 != 0,
                wave_clock: animation_clock,
                world_flags: world.world_flags,
                sine: &sine,
            },
            native_camera,
        );
    }
    let view = SceneView {
        eye: shown_camera.eye,
        basis: shown_camera.basis,
    };
    // Port option: the remastered eye under the water's live surface. The
    // software and OpenGL rasters draw the world there wobbling and dimmed,
    // in a short fog of the water's colour (V2000's underwater projection,
    // Infestation's own fog); Vulkan's raster, where it shades the water,
    // keeps the world as above and lays the murk itself; the tracer lays
    // its own medium ([`UnderwaterLook`]).
    let under_water = remastered
        && crate::camera_clearance::water_surface(
            &world.terrain,
            [view.eye[0] as u16, view.eye[2] as u16],
            animation_clock,
            world.world_flags,
            &sine,
        )
        .is_some_and(|surface| view.eye[1] < surface);
    let water = crate::modern_tuning::current().water(world.mission.current_world_a0);
    let look = underwater_look(
        under_water,
        host.backend.traces_rays(),
        frame.infrared,
        water.glow <= 0.0 && host.backend.pixels.frame.shades_water(),
    );
    // The objects' cone reaches the corners of the view drawn.
    let cone = |setup: SceneSetup| {
        crate::object_runtime::ObjectDrawCone::for_projection(
            crate::model_context::ModelProjection {
                focal: setup.focal,
                viewport: setup.viewport,
                center: setup.center,
                fade: setup.fade,
            },
        )
    };
    let shown = ViewSpec {
        view,
        coordinate_shift: shown_camera.coordinate_shift,
        focals: shown_focals,
        viewport: frame.surface,
        fade: shown_fade,
        far_color,
        under_water,
        look,
        cone: cone(SceneSetup::context(SceneProjection {
            focal: shown_focals,
            viewport: frame.surface,
            fade: shown_fade,
        })),
        arena_bytes: frame.arena_bytes,
    };
    let game = if retail {
        ViewSpec {
            view: SceneView {
                eye: native_camera.eye,
                basis: native_camera.basis,
            },
            coordinate_shift: native_camera.coordinate_shift,
            focals: game_focals,
            viewport: game_viewport,
            fade,
            far_color,
            under_water: false,
            look: UnderwaterLook::Plain,
            cone: crate::object_runtime::ObjectDrawCone::Retail,
            arena_bytes: RETAIL_ARENA_BYTES,
        }
    } else {
        ViewSpec {
            focals: game_focals,
            fade,
            under_water: false,
            look: UnderwaterLook::Plain,
            cone: cone(setup),
            ..shown
        }
    };
    Ok([game, shown])
}

/// 00440F00's preamble, as the existing arena/depth preparation with the
/// arena `arena_bytes` long, then pre-draw 00488380's first step, 004501A0,
/// before the camera and sky.
fn begin_world_pass<'r>(
    world: &WorldSession,
    frame: &WorldFrameInputs,
    arena_bytes: u32,
    animation_clock: u32,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<(), WorldDrawError> {
    host.backend.begin_frame_with_arena(arena_bytes)?;
    host.backend.set_underwater(None);
    host.backend.set_mirror_copy(frame.mirror_copy);
    host.backend.set_hovercraft_fans(frame.hovercraft_fans);
    host.backend.set_guided_mounts(frame.guided_mounts);
    publish_world_light(world, animation_clock, host)
}

/// How one pass's lines begin, so the view shown's never read as the
/// game's own stops (the harness counts those) and never take their
/// de-dupe keys (`draw_omissions_logged`).
struct PassLines {
    head: &'static str,
    terrain: &'static str,
    key: &'static str,
}

impl PassLines {
    fn of(pass: WorldPass) -> Self {
        match pass {
            WorldPass::View => Self {
                head: "World view shown",
                terrain: "World view shown",
                key: "shown ",
            },
            WorldPass::Whole | WorldPass::Game => Self {
                head: "World",
                terrain: "world",
                key: "",
            },
        }
    }
}

/// One pass of the world draw from `spec`: its scene, then everything the
/// draw submits and the game its callbacks play, and its receipt.
fn draw_world_pass<'r>(
    request: WorldDrawRequest<'_>,
    frame: WorldFrameInputs,
    spec: &ViewSpec,
    pass: WorldPass,
    host: &mut ResidentSceneHost<'r, '_, SoftwareScene<'_, 'r>>,
) -> Result<WorldDrawReceipt, WorldDrawError> {
    let lines = PassLines::of(pass);
    let WorldDrawRequest {
        pickup_text_data,
        world,
        animation,
        object_draw_process,
        attract_demo,
        particles,
        light_grid,
        executable,
        object_light_policy,
        object_light_failure_policy,
        camera,
        furniture,
        mut sound,
        mut weapon_tables,
        terrain_event_5483b0,
        mut map,
        camera_clearance: _,
    } = request;
    let animation_clock = camera.animation_clock;
    let terrain_residency = camera.residency;
    let depth = depth_policy(frame.z_buffering);
    begin_world_pass(world, &frame, spec.arena_bytes, animation_clock, host)?;
    let setup = SceneSetup::context(SceneProjection {
        focal: spec.focals,
        viewport: spec.viewport,
        fade: spec.fade,
    });
    // WorldProjectionSetupApproximation: one typed publication groups native
    // near/far/fog stores; it does not claim their individual fault prefixes.
    host.submit(SceneCommand::Setup(setup))?;
    host.submit(SceneCommand::FarColor(spec.far_color))?;
    let view = spec.view;
    host.submit(SceneCommand::PublishView(view))?;
    host.submit(SceneCommand::CoordinateShift(spec.coordinate_shift))?;
    let sine: [u16; 4096] = *host.backend.sine_table();
    let water = crate::modern_tuning::current().water(world.mission.current_world_a0);
    let look = spec.look;
    let mut backdrop = None;
    let (fade, far_color, setup) = if look == UnderwaterLook::Wet {
        let fade = [UNDERWATER_FAR / 16, UNDERWATER_FAR];
        host.submit(SceneCommand::PublishFade(
            crate::frontend_scene::SceneFade::new(fade),
        ))?;
        let hue = host.backend.water_colour().unwrap_or(WATER_HUE);
        let rgb = hue.map(|channel| (channel * UNDERWATER_FOG_LIGHT * 255.0).round() as u8);
        let far_color =
            crate::terrain_draw::fog_material(rgb, host.backend.pixels.renderer.material_shifts);
        host.submit(SceneCommand::FarColor(far_color))?;
        let dim =
            hue.map(|channel| UNDERWATER_DIM * (1.0 - UNDERWATER_HUE + UNDERWATER_HUE * channel));
        // Where the fog is whole, as the dimmed draws reach it.
        let [r, g, b] = std::array::from_fn(|c| (f32::from(rgb[c]) * dim[c]).round() as u8);
        backdrop = Some([r, g, b, 0xff]);
        let wet = crate::software_frame::WetFrame::new(
            animation_clock,
            &sine,
            host.backend.pixels.frame.height(),
            dim,
        );
        host.backend.set_underwater(Some((wet, far_color)));
        let setup = SceneSetup::context(SceneProjection {
            focal: spec.focals,
            viewport: spec.viewport,
            fade,
        });
        (fade, far_color, setup)
    } else {
        (spec.fade, spec.far_color, setup)
    };
    // The modern mode: a frame whose device traces rays collects the world's
    // scene beside the draw (crate::scene_feed); the draw itself runs as it
    // always does, for the game's sake, and the tracer replaces its pixels.
    let camera = scene_camera(view, setup);
    let tracing = pass != WorldPass::Game && host.backend.begin_scene_feed(camera, frame.infrared);
    // An untraced world takes the player's anti-aliasing, except under
    // infrared vision, whose camera has a resolution of its own.
    let raster = pass != WorldPass::Game && !tracing && !frame.infrared;
    // The craft the camera follows, and its wake through the clouds, laid
    // by the view shown's pass.
    let craft = followed_craft(world);
    let micros = animation.animation_micros();
    if pass != WorldPass::Game {
        world.cloud_wake.follow(craft, micros);
    }
    let craft = craft.map(|words| {
        std::array::from_fn(|axis| {
            f32::from(i32::from(words[axis]).wrapping_sub(view.eye[axis]) as i16)
        })
    });
    let inputs = WorldFeedInputs {
        terrain: &world.terrain,
        presentation: &world.presentation,
        world_flags: world.world_flags,
        world: world.mission.current_world_a0,
        eye: view.eye,
        fade,
        far_color,
        infrared: frame.infrared,
        darkened: darkened(&world.mission),
        // The view drawn's, which the game's pass (`under_water` false)
        // never traces.
        eye_under: spec.under_water && water.glow <= 0.0 && !frame.infrared,
        weather: world.weather,
        // The tracer's, and the raster's light, whose modern sky lays the
        // arcs (vk_sky.comp).
        sparks: crate::modern_tuning::current()
            .crystals()
            .flare
            .map_or_else(Vec::new, |look| {
                world.crystal_flares.sparks(view.eye.map(f64::from), &look)
            }),
        clock: animation_clock,
        clock_micros: micros,
        craft,
        cloud_wake: world.cloud_wake.from_eye(view.eye, craft, micros),
        particles: &*particles,
        executable,
    };
    // The raster's world light, kept for the lamps the light pass finds
    // where the device shades the water.
    let mut raster_light = None;
    // Port option, Vulkan's remaster: the spray the player's craft throws
    // driving through the rain's puddles, a picture only, from the craft as
    // the game left it (crate::craft_spray). Its rings go to the puddles'
    // shading, its drops over the world once it is drawn.
    let spray_now = inputs.clock_micros;
    let sprays = (tracing || raster) && !frame.infrared && host.backend.pixels.frame.is_vulkan();
    let spray_rings = if sprays {
        let plane = (world.terrain.plane_base() >> world.terrain.plane_shift()) as i16;
        let look = crate::scene_ground_detail::CarveLook::for_world(
            world.weather.puddles(),
            f32::from(plane),
            &water,
        );
        let terrain = &world.terrain;
        let water_at = |xz: [f32; 2]| {
            look.as_ref().map_or(0.0, |look| {
                crate::scene_ground_detail::water_at(terrain, xz, look)
            })
        };
        let craft = look
            .as_ref()
            .and_then(|_| crate::craft_spray::SprayCraft::of_player(world, executable, water_at));
        world.craft_spray.update(spray_now, craft, water_at);
        world.craft_spray.rings(spray_now, view.eye.map(f64::from))
    } else {
        Vec::new()
    };
    if raster {
        let tuning = crate::modern_tuning::current();
        // The water's least share mirrored, as the tracer's (its Fresnel
        // term at least the world's tuned share); lava mirrors nothing.
        let water_reflect = (water.glow <= 0.0).then_some(water.reflect);
        // Under the water the device's murk, as the world draw's own fog
        // lays it: the water's colour, as near and as far.
        let murk = (look == UnderwaterLook::Murk).then(|| {
            let hue = host.backend.water_colour().unwrap_or(WATER_HUE);
            crate::vk_device::Murk {
                colour: hue.map(|channel| channel * UNDERWATER_FOG_LIGHT),
                fade: [UNDERWATER_FAR / 16, UNDERWATER_FAR].map(|end| end as f32),
            }
        });
        // The world's light, which the device's own passes take (its water,
        // its sky), and its modern sky, which the device draws in the
        // painting's place where it can; not under the water's own fog,
        // which is the sky there.
        // The device wets the ground where it shades the water, above it.
        let wets =
            water_reflect.is_some() && murk.is_none() && host.backend.pixels.frame.shades_water();
        let mut light = raster_sky_feed(host.backend, &host.resources, &inputs, camera, wets);
        light.spray_rings = spray_rings.clone();
        let sky = backdrop.is_none()
            && tuning.sky(inputs.world).style == crate::modern_tuning::SkyStyle::Modern;
        host.backend
            .begin_raster_world(&camera, water_reflect, murk, Some(&light), sky);
        raster_light = host.backend.pixels.frame.shades_water().then_some(light);
    }
    if tracing {
        let _feed = crate::cpu_profile::scope("world: feed_world (ground, water, sky)");
        feed_world(host.backend, host.resources, inputs);
        if let Some(builder) = host.backend.scene_feed_mut() {
            builder.feed.spray_rings = spray_rings;
        }
    }

    let mut outcome = WorldDrawOutcome {
        submitted_models: 0,
        skips: WorldDrawSkips::default(),
        skipped_terrain: true,
        terrain_commands: 0,
        water_commands: 0,
        terrain_omissions: 0,
        furniture_models: 0,
        furniture_omissions: 0,
        terrain_completed_strip_rejections: 0,
        skipped_sky: true,
        particle_commands: 0,
        particle_omissions: 0,
        object_light_callbacks: None,
        object_light_deposits: None,
        object_light_omissions: 0,
        sounds_started: 0,
        sound_omissions: 0,
        arena_overflow: false,
        two_pass: false,
        outside: 0,
        focals: spec.focals,
        depth,
    };
    // Sky first, like the worker's `00440DA0` ahead of the world draw; under
    // infrared vision it draws nothing (00440DA8).
    // Port option: under the water's own fog ([`UnderwaterLook::Wet`]) the
    // backdrop is the fog's colour in place of the sky, so the ground the
    // fog ends fades into the water as retail's fades into its sky.
    let murk = backdrop.is_some_and(|rgba| host.backend.pixels.frame.clear_backdrop(rgba));
    if !frame.infrared && !murk {
        let _sky = crate::cpu_profile::scope("world: sky");
        submit_sky(
            view.eye,
            world.presentation.backdrop_height,
            host,
            &mut outcome,
        );
    }
    //00440F00 flushes sky, then renderer+14(NULL) clears depth before
    //00488020 publishes the live settings+50 policy. Saved sorting state
    // survives this reset.
    host.backend.reset_renderer();
    host.backend.set_depth_policy(depth.0, depth.1)?;
    // Vulkan's raster draws the modern sky behind what it kept of the
    // painting, before the world.
    host.backend.draw_raster_sky();

    let light_policy = crate::world_lighting::TerrainLightPolicy {
        enabled: frame.dynamic_lighting,
        spacing: world.terrain.sampling().spacing,
        color: crate::world_lighting::TerrainLightColor::for_infrared(frame.infrared),
    };
    //00488020 clears the process grid before publishing its current anchor;
    // infrared vision's player light lands under the retained one.
    light_grid.clear();
    if frame.infrared {
        infrared_player_light(&mut world.arena, light_grid, light_policy);
    }
    light_grid.set_anchor(crate::world_lighting::world_light_anchor_coordinates(view));
    let particle_context = crate::process_particles::draw::ParticleDrawContext {
        position: view.eye,
        basis: view.basis,
        view,
        projection: host.backend.environment.projection,
        coordinate_shift: host.backend.environment.coordinate_shift,
        fog_factor: host.backend.environment.fog_factor,
        projection_dispatch: crate::model_context::ModelDispatch::Far,
        far_color,
    };
    let particle_request = crate::process_particles::draw::ParticleDrawRequest {
        bank: crate::process_particles::draw::ParticleDrawBank::Active,
        context: particle_context,
        terrain: Some(&world.terrain),
    };
    let mut object_context = crate::object_runtime::ObjectDrawContext {
        view,
        near: fade[0],
        far: fade[1],
        far_color,
    };
    host.submit(SceneCommand::BeginBatch)?;
    // The vehicles whose headlights the traced frame's lamps take, and the
    // raster's water.
    let mut lamps = Vec::new();
    // The objects the pass's cone turned away, for the tracer.
    let mut culled = Vec::new();
    // 00440F00's frame around 00488020: a full arena (0x301) skips the rest
    // of the world draw, and the batch the frame queued still replays.
    'world: {
        let particles_scope = crate::cpu_profile::scope("world: particles (active)");
        submit_particles(
            particles,
            particle_request,
            executable,
            &mut WorldParticleLighting {
                grid: light_grid,
                policy: light_policy,
            },
            host,
            &mut outcome,
            lines.head,
        );

        drop(particles_scope);
        if host.backend.arena_overflowed() {
            break 'world;
        }
        // 00488147: active particles have written their prefix; fresh object
        // callbacks now update the same grid before model/pending/terrain work.
        let sine_quarter: [u16; 4096] = *host.backend.sine_table();
        let light_scope = crate::cpu_profile::scope("world: object light pass");
        match world
            .arena
            .run_flag0_light_pass(crate::object_runtime::Flag0LightPassRequest {
                grid: light_grid,
                policy_source: object_light_policy,
                failure_policy: object_light_failure_policy,
                terrain: Some(&world.terrain),
                rng: Some(&mut world.rng),
                world_delta: std::mem::take(&mut world.light_delta),
                sine_quarter: &sine_quarter,
                lamps: (tracing || raster_light.is_some()).then_some(&mut lamps),
                lamps_light: tracing,
            }) {
            Ok(light) => {
                outcome.object_light_callbacks = Some(light.callbacks);
                outcome.object_light_deposits = Some(light.deposits);
                outcome.object_light_omissions = light.omissions.len();
                for omission in light.omissions {
                    let key = format!(
                        "{}light {:?} {:?}",
                        lines.key, omission.object, omission.reason
                    );
                    if world.draw_omissions_logged.insert(key) {
                        eprintln!(
                            "{} object light {:?} skipped after reached writes (clock {}): {:?}",
                            lines.head,
                            omission.object,
                            animation.animation_value(),
                            omission.reason
                        );
                    }
                }
            }
            Err(error) => {
                // A grid/custody error stops this feature without swapping list
                // owners. Other independent world draws can retain their prefix.
                outcome.object_light_omissions += 1;
                if !world.draw_skip_logged {
                    eprintln!(
                        "{} object light pass stopped after reached writes ({error}); remaining callbacks skipped",
                        lines.head
                    );
                }
            }
        }
        drop(light_scope);
        // The headlamps, before the furniture's crystals catch them.
        feed_lamps(host.backend, &lamps, view.eye);
        // The raster's water takes the same lamps, in the water's own colour
        // as the tracer's (its ground keeps retail's pools).
        if let Some(light) = raster_light.as_mut().filter(|_| !lamps.is_empty()) {
            let tuning = crate::modern_tuning::current();
            light.lamps = place_lamps(&lamps, view.eye, tuning.headlight());
            light.water_hue = Some(host.backend.water_colour().unwrap_or(WATER_HUE));
            host.backend.pixels.frame.raster_lamps(light);
        }
        // The vehicles thrusting, whose exhausts leave trails, and their
        // engines' loads, which their thruster lights follow.
        if let Some(builder) = host.backend.scene_feed_mut() {
            builder.feed.engine_loads = world
                .arena
                .engine_loads()
                .into_iter()
                .map(|(words, load)| {
                    let at = std::array::from_fn(|c| {
                        f32::from(i32::from(words[c]).wrapping_sub(view.eye[c]) as i16)
                    });
                    (at, load as f32)
                })
                .collect();
            builder.feed.thrusts = world
                .arena
                .thrusting_vehicles()
                .into_iter()
                .map(|words| {
                    std::array::from_fn(|c| {
                        f32::from(i32::from(words[c]).wrapping_sub(view.eye[c]) as i16)
                    })
                })
                .collect();
        }
        //00488160/165: ECX=0x13 (decimal19), EDX=1. This world caller
        //profiles slot46; the vehicle's separate0x0D pair profiles slot40.
        object_draw_process
            .profiling
            .record(crate::object_runtime::ProfilingRequest {
                index: 19,
                bank: 1,
                phase: crate::object_runtime::ProfilingPhase::Begin,
            })?;
        //0048816E: same fresh Flag0 owner as briefing, before pending particles.
        let mut object_request = WorldObjectDrawRequest {
            pickup_text_data,
            weapon_tables: weapon_tables.as_deref_mut(),
            particles,
            sounds: sound.as_mut().map(|sound| &mut *sound.queue),
            birth_inputs: furniture.birth_inputs,
            context: &mut object_context,
            process: object_draw_process,
            residency: terrain_residency,
            light_grid,
            executable,
            network_flag: &furniture.birth_inputs.process.network_flag,
            animation,
            attract_demo,
            camera_settings: frame.camera_settings,
            settings_7c: frame.settings_7c,
            settings_70: frame.settings_70,
            network_initials: &frame.network_initials,
            network_team_flag: frame.network_team_flag,
            map: map.as_deref_mut(),
        };
        let objects_scope = crate::cpu_profile::scope("world: object models");
        let object_result = submit_world_object_models(
            world,
            host.backend,
            &mut object_request,
            ObjectPass::Manager(spec.cone),
        );
        drop(objects_scope);
        if object_result.is_ok() {
            //48817D normal-return suffix only, before pending particles. Briefing
            //has no outer marker pair; its shared manager does not acquire one.
            object_draw_process
                .profiling
                .record(crate::object_runtime::ProfilingRequest {
                    index: 19,
                    bank: 1,
                    phase: crate::object_runtime::ProfilingPhase::End,
                })?;
        }
        match object_result {
            Ok(mut draw) => {
                culled = std::mem::take(&mut draw.culled);
                if draw.emitted_shots != 0 {
                    eprintln!(
                        "{} weapon firing: {} shot(s) emitted by {} nonempty drain(s), clock {}",
                        lines.head,
                        draw.emitted_shots,
                        draw.firing_drains,
                        animation.animation_value()
                    );
                }
                outcome.submitted_models += draw.common_models + draw.late_models;
                outcome.skips.object_draw += draw.omissions.len();
                for omission in draw.omissions {
                    let shown = pass == WorldPass::View;
                    let key = format!(
                        "{}draw {:?} {:?}",
                        if shown { "shown " } else { "" },
                        omission.object,
                        omission.reason
                    );
                    if !world.draw_omissions_logged.insert(key) {
                    } else if shown {
                        eprintln!(
                            "World view shown {:?} omitted, its writes put back (clock {}): {:?}",
                            omission.object,
                            animation.animation_value(),
                            omission.reason
                        );
                    } else {
                        eprintln!(
                            "World object draw {:?} omitted after reached writes (clock {}): {:?}",
                            omission.object,
                            animation.animation_value(),
                            omission.reason
                        );
                    }
                }
            }
            Err(crate::object_runtime::Flag0ObjectDrawError::Host(_))
                if host.backend.arena_overflowed() =>
            {
                break 'world;
            }
            Err(crate::object_runtime::Flag0ObjectDrawError::Host(error)) => {
                return Err(error.into())
            }
            Err(error) => {
                outcome.skips.object_draw += 1;
                if !world.draw_skip_logged {
                    eprintln!(
                        "{} object manager stopped after reached writes: {error}",
                        lines.head
                    );
                }
            }
        }
        if host.backend.arena_overflowed() {
            break 'world;
        }
        let pending_scope = crate::cpu_profile::scope("world: particles (pending)");
        submit_particles(
            particles,
            crate::process_particles::draw::ParticleDrawRequest {
                bank: crate::process_particles::draw::ParticleDrawBank::Pending,
                //488193 passes the same EBP used by479570, including reached
                //fade/color stores retained after an omitted object callback.
                context: crate::process_particles::draw::ParticleDrawContext {
                    view: object_context.view,
                    projection: crate::model_context::ModelProjection {
                        fade: [object_context.near, object_context.far],
                        ..particle_context.projection
                    },
                    far_color: object_context.far_color,
                    ..particle_context
                },
                terrain: Some(&world.terrain),
            },
            executable,
            &mut WorldParticleLighting {
                grid: light_grid,
                policy: light_policy,
            },
            host,
            &mut outcome,
            lines.head,
        );
        if host.backend.arena_overflowed() {
            break 'world;
        }
        drop(pending_scope);
        particles.transfer_pending();
        // 004881B3 publishes the world light again before the flash, sounds and
        // terrain, so furniture starts from the world's light rather than from
        // the last object drawn.
        publish_world_light(world, animation_clock, host)?;
        //004881B8..00488326 submits the screen flash after pending particles
        //transfer and before positional sounds. Sequence advances on submission,
        //independently of animation time; failed carry stays local to this effect.
        if !world.flash.stopped_draw {
            if let Err(error) = host.backend.draw_world_flash(
                &mut world.flash,
                setup.viewport.map(|v| v as u32),
                executable,
            ) {
                if host.backend.arena_overflowed() {
                    break 'world;
                }
                world.flash.stopped_draw = true;
                eprintln!("{} flash stopped after reached writes: {error}", lines.head);
            }
        }
        //00480110/004800E0 follow particle transfer and precede terrain44AA10.
        // Actual process0055F0F4 remains its constructor-zero value until a writer
        // is owned; no phase number stands in for that independent policy.
        // The view shown, drawn apart from the game's, plays no sound.
        if let Some(sound) = sound.as_mut().filter(|_| pass != WorldPass::View) {
            if let Some(sink) = sound.sink.as_deref_mut() {
                // A world without water holds its plane at -4096 or below.
                let plane = world.terrain.plane_base() >> world.terrain.plane_shift();
                let listener = crate::process_sound::ProcessSoundListener {
                    view,
                    phase: sound.phase,
                    suppressed: false,
                    water: (plane > -4096).then_some(plane),
                };
                match sound.queue.service_world_draw(
                    listener,
                    sink,
                    sound.effects_volume,
                    &mut world.rng,
                ) {
                    Ok(started) => outcome.sounds_started = started,
                    Err(error) => {
                        outcome.sounds_started = sound.queue.started_in_last_service();
                        outcome.sound_omissions += 1;
                        eprintln!(
                        "World positional sound skipped after reached voice/RNG writes: {error}"
                    );
                    }
                }
            } else if !sound.queue.is_empty() {
                outcome.sound_omissions += 1;
                eprintln!("World positional sound requires a playback host; queued one-shots disposed by004800E0");
                if let Err(error) = sound.queue.cleanup_one_shots() {
                    eprintln!("World sound disposal stopped after reached record writes: {error}");
                }
            }
        }
        // Same 0044AA10 owner as the briefing pane, with this caller's retained
        // view/context and live process light grid. Briefing supplies its separate
        // lighting policy; neither path branches on campaign selection.
        let tint = world.presentation.tint_word.to_le_bytes();
        // Sound aliases above consume the same process RNG. Re-publish it before
        // terrain callbacks; their reached effects write it back even on failure.
        host.backend.set_process_rng(Some(world.rng));
        let night = frame.infrared.then(|| {
            crate::terrain_draw::night_material(
                crate::world_tint::INFRARED_RGB,
                host.backend.pixels.renderer.material_shifts,
            )
        });
        let terrain_inputs = crate::terrain_draw::TerrainDrawInputs {
            view,
            context_projection: crate::model_context::ModelProjection {
                focal: setup.focal,
                viewport: setup.viewport,
                center: setup.center,
                //44AA53/56 capture the same caller context after object drawing.
                fade: [object_context.near, object_context.far],
            },
            // The objects' cone and the footprints reach as far as the view.
            footprint: match spec.cone {
                crate::object_runtime::ObjectDrawCone::Retail => {
                    crate::terrain_draw::TerrainFootprintWidth::Retail
                }
                crate::object_runtime::ObjectDrawCone::View { half, focal } => {
                    crate::terrain_draw::TerrainFootprintWidth::for_view(half, focal)
                }
            },
            coordinate_shift: host.backend.environment.coordinate_shift,
            tint: [tint[0], tint[1], tint[2]],
            lighting: crate::world_lighting::terrain_lighting_inputs(
                light_grid,
                frame.dynamic_lighting,
                executable,
            )?,
            // Direct ground does not read the water/particle animation clock.
            clock: animation_clock,
            world_flags: world.world_flags,
            night,
            particles_enabled: terrain_event_5483b0 != 0,
            tables: host.backend.primitive_tables(),
        };
        let terrain_scope = crate::cpu_profile::scope("world: terrain and furniture");
        let terrain_before = host.backend.stats.terrain_commands;
        let water_before = host.backend.stats.water_commands;
        match host
            .backend
            .draw_terrain(crate::software_scene::SoftwareTerrainDrawRequest {
                traversal: crate::terrain_draw::TerrainDrawRequest {
                    terrain: &mut world.terrain,
                    inputs: terrain_inputs,
                },
                furniture: crate::software_scene::SoftwareFurnitureInputs {
                    state: furniture.state,
                    pending: furniture.pending,
                    light_grid,
                    particles,
                    executable,
                    birth_inputs: furniture.birth_inputs,
                    load_factor: world.fade.performance.particle_load_factor as i32,
                    crystal_flares: Some(&world.crystal_flares),
                },
            }) {
            Ok(result) => {
                outcome.terrain_omissions = result.traversal.omitted_features;
                outcome.furniture_models = result.furniture.submitted_models;
                outcome.furniture_omissions = result.furniture.omissions.len();
                outcome.terrain_completed_strip_rejections = result.traversal.rejected;
                for omission in result.omissions {
                    let line = format!(
                        "{} terrain: {:?}: {}",
                        lines.terrain, omission.feature, omission.reason
                    );
                    if world.draw_omissions_logged.insert(line.clone()) {
                        eprintln!("{line}");
                    }
                }
            }
            Err(error) => {
                outcome.terrain_omissions += 1;
                let line = format!(
                    "{} terrain: remaining terrain skipped ({error:?})",
                    lines.terrain
                );
                if world.draw_omissions_logged.insert(line.clone()) {
                    eprintln!("{line}");
                }
            }
        }
        drop(terrain_scope);
        world.rng = host.backend.process_rng().unwrap_or(world.rng);
        outcome.terrain_commands = host.backend.stats.terrain_commands - terrain_before;
        outcome.water_commands = host.backend.stats.water_commands - water_before;
        outcome.skipped_terrain = outcome.terrain_commands == 0;
        // Port option: last of all, what lies outside the traced view and
        // reaches it, and the craft round a first-person eye, drawn for the
        // tracer alone on copies of the game, as the view shown is drawn
        // apart from it ([`GameAsDrawn`]).
        let mut outside = match host.backend.scene_feed_mut() {
            Some(builder) if !frame.infrared => {
                let eye = view.eye;
                let ground = |at: [f32; 3]| {
                    let x = (eye[0] + at[0] as i32) as u16;
                    let z = (eye[2] + at[2] as i32) as u16;
                    f32::from(world.terrain.height_at([x, z])) - eye[1] as f32
                };
                let mut outside = if OUTSIDE_VIEW_MODELS {
                    reaching_view(
                        &culled,
                        &builder.feed.camera,
                        builder.feed.sun.direction,
                        builder.water_plane,
                        ground,
                    )
                } else {
                    Vec::new()
                };
                outside.extend(world.arena.first_person_craft());
                #[cfg(test)]
                if NOTHING_OUTSIDE.get() {
                    outside.clear();
                }
                outside
            }
            _ => Vec::new(),
        };
        if !outside.is_empty() && !host.backend.stopped() {
            let _outside_scope = crate::cpu_profile::scope("world: outside the view");
            let drawn = crate::event_log::silenced(|| {
                crate::object_runtime::refusing_writes(|| -> Result<_, SoftwareSceneError> {
                    let mut owners = GameOwners {
                        world: &mut *world,
                        process: &mut *object_draw_process,
                        particles: &mut *particles,
                        light_grid: &mut *light_grid,
                        furniture: &mut *furniture.state,
                        sounds: sound.as_mut().map(|sound| &mut *sound.queue),
                        tables: weapon_tables.as_deref_mut(),
                        map: map.as_deref_mut(),
                    };
                    // A failed set-aside leaves the game as it was.
                    let kept = match GameAsDrawn::set_aside(&mut owners, host.backend) {
                        Ok(kept) => kept,
                        Err(error) => return Ok(Err(error.to_string())),
                    };
                    // The craft drawn whole: its first-person mode cleared
                    // on the copy.
                    if let Some(craft) = owners.world.arena.show_first_person_hull() {
                        outside.retain(|&object| object != craft);
                        outside.push(craft);
                    }
                    let mut context = object_context;
                    let mut request = WorldObjectDrawRequest {
                        pickup_text_data,
                        weapon_tables: owners.tables.as_deref_mut(),
                        particles: &mut *owners.particles,
                        sounds: owners.sounds.as_deref_mut(),
                        birth_inputs: furniture.birth_inputs,
                        context: &mut context,
                        process: &mut *owners.process,
                        residency: terrain_residency,
                        light_grid: &*owners.light_grid,
                        executable,
                        network_flag: &furniture.birth_inputs.process.network_flag,
                        animation,
                        attract_demo,
                        camera_settings: frame.camera_settings,
                        settings_7c: frame.settings_7c,
                        settings_70: frame.settings_70,
                        network_initials: &frame.network_initials,
                        network_team_flag: frame.network_team_flag,
                        map: owners.map.as_deref_mut(),
                    };
                    let drawn = host.backend.feed_only(|scene| {
                        submit_world_object_models(
                            &mut *owners.world,
                            scene,
                            &mut request,
                            ObjectPass::Listed(&outside),
                        )
                    });
                    drop(request);
                    kept.put_back(&mut owners, host.backend)?;
                    Ok(match drawn {
                        Some(Ok(drawn)) => Ok(drawn),
                        Some(Err(error)) => Err(error.to_string()),
                        None => Ok(Default::default()),
                    })
                })
            })?;
            let said: Vec<_> = match drawn {
                Ok(drawn) => {
                    outcome.outside = drawn.near_callbacks;
                    drawn
                        .omissions
                        .into_iter()
                        .map(|omission| {
                            format!(
                                "World outside the view {:?} omitted, its writes put back: {:?}",
                                omission.object, omission.reason
                            )
                        })
                        .collect()
                }
                Err(error) => {
                    vec![format!(
                        "World outside the view stopped, its writes put back: {error}"
                    )]
                }
            };
            for line in said {
                if world.draw_omissions_logged.insert(line.clone()) {
                    eprintln!("{line}");
                }
            }
        }
    }
    if host.backend.recover_native_301() {
        outcome.arena_overflow = true;
        let line = format!(
            "{} draw arena full (native 0x301): the rest of the world draw skipped, as 00440F00's catch does",
            lines.head
        );
        if world.draw_omissions_logged.insert(line.clone()) {
            eprintln!("{line}");
        }
    }
    // The tracer draws the world now; the flush then replays only what the
    // scene feed left to the raster (blended effects, billboards).
    if tracing {
        // The draws above have laid the player's wheels.
        let wake = player_wake(world, view.eye);
        if let Some(builder) = host.backend.scene_feed_mut() {
            builder.feed.wakes = craft_wakes(&builder.feed, wake, &world.terrain, view.eye);
        }
        // The draws above have left the light grid complete.
        let grid_scope = crate::cpu_profile::scope("world: feed_light_grid");
        feed_light_grid(
            host.backend,
            light_grid,
            WorldLightFeed {
                spacing: light_policy.spacing.units(),
                ground: frame.dynamic_lighting,
                terrain: &world.terrain,
                eye: view.eye,
                executable,
            },
        );
        drop(grid_scope);
        let _trace = crate::cpu_profile::scope("world: trace_scene_feed");
        host.backend.trace_scene_feed();
    }
    let flush_scope = crate::cpu_profile::scope("world: flush batch (raster replay)");
    host.submit(SceneCommand::FlushBatch)?;
    drop(flush_scope);
    // The craft's spray over the world drawn, traced or not.
    if sprays {
        let drops = world.craft_spray.drops(spray_now, view.eye.map(f64::from));
        if !drops.is_empty() {
            let fans = crate::craft_spray::fans(&camera, &drops, spray_colour(&world.presentation));
            host.backend.pixels.frame.add_spray_fans(&fans);
        }
    }
    host.backend.set_underwater(None);
    if tracing {
        host.backend.end_scene_feed();
    }
    if raster {
        host.backend.finish_raster_world();
    }
    // Under Vulkan, infrared vision is a thermal camera over the world.
    if frame.infrared && pass != WorldPass::Game {
        host.backend.finish_thermal();
    }
    // 00440FF9: renderer +14(NULL) follows the final world/player flush.
    // It clears depth and disables device Z before immediate HUD/intro masks;
    // the saved renderer policy and world projection survive this call.
    host.backend.reset_renderer();
    host.submit(SceneCommand::BeginBatch)?;
    let projection = crate::model_context::ModelProjection {
        focal: setup.focal,
        viewport: setup.viewport,
        center: setup.center,
        fade: [object_context.near, object_context.far],
    };
    Ok(WorldDrawReceipt {
        native_camera: true,
        animation_clock: animation.animation_value(),
        eye: view.eye,
        basis_words: view.basis.words,
        coordinate_shift: spec.coordinate_shift,
        projection,
        terrain_height_at_eye: world
            .terrain
            .height_at([view.eye[0] as u16, view.eye[2] as u16]),
        outcome,
        under_water: spec.under_water,
        game_eye: view.eye,
        game_projection: projection,
        game_outcome: outcome,
    })
}

/// What a traced view of the world takes from it beside its draw: the
/// ground and its textures, the water within sight, the world's light, fog
/// and sky, and its splashes. The world draw and the briefing's terrain pane
/// both feed the tracer through it.
pub(crate) struct WorldFeedInputs<'a> {
    pub terrain: &'a crate::terrain_runtime::TerrainRuntime,
    pub presentation: &'a crate::world_backend::WorldPresentationState,
    /// Retained H14 entry +4 flags (bit 0x40: authored water waves).
    pub world_flags: u32,
    /// The world's number, for its tuning.
    pub world: u32,
    pub eye: [i32; 3],
    /// The view's fog start and end.
    pub fade: [i32; 2],
    /// Retail's converted fog colour (H7 0x74, or 0x20 under infrared).
    pub far_color: u32,
    pub infrared: bool,
    /// How far a darkened world's second fog has rolled in, 0 to 1.
    pub darkened: f32,
    /// Port option, the remastered camera: the eye is under the water's
    /// live surface, which the tracer lays as a medium (not lava, not under
    /// infrared vision).
    pub eye_under: bool,
    /// The world's weather ([`crate::world_weather`]).
    pub weather: crate::world_weather::WorldWeather,
    /// The sparks running through struck crystals.
    pub sparks: Vec<crate::crystal_flares::CrystalSpark>,
    pub clock: u32,
    /// The same clock in microseconds.
    pub clock_micros: u64,
    /// The player's craft the camera follows, eye-relative, and its wake
    /// ([`crate::scene_feed::SceneFeed::craft`]).
    pub craft: Option<[f32; 3]>,
    pub cloud_wake: Vec<[f32; 4]>,
    pub particles: &'a crate::process_particles::ProcessParticleRuntime,
    pub executable: inf_formats::FrontendExecutable<'a>,
}

/// Fill the scene feed `scene` is collecting with the world in `inputs`.
/// The seconds a darkened world's second fog takes to roll in.
const FOG_ROLLS_IN: f32 = 6.0;

/// How far a darkened world's second fog has rolled in (0 to 1): while
/// mission `+13` is set, by its timer `+2C`. Artoria's load clears the byte
/// and its four generators' deaths set it (the world tint's dark
/// schedule), so there that is the time since the city lost its power, the
/// user's "a second fog profile that makes it get closer when you destroy
/// the generators". Worlds whose fog has no dark profile ignore it.
pub(crate) fn darkened(mission: &crate::world_mission::WorldMission) -> f32 {
    match (mission.byte_13, mission.dword_2c) {
        (Some(set), Some(timer)) if set != 0 => (timer as f32 / 1000.0 / FOG_ROLLS_IN).min(1.0),
        _ => 0.0,
    }
}

pub(crate) fn feed_world<'r>(
    scene: &mut SoftwareScene<'_, 'r>,
    resources: crate::render_resources::RenderResources<'r>,
    inputs: WorldFeedInputs<'_>,
) {
    let tones = world_tones(scene, &resources, &inputs);
    let [r, g, b, _] = inputs.presentation.tint_word.to_le_bytes();
    // The whole ground, and the 55 blend textures its tiles take, as the
    // raster resolves them (H3 0x4FC on, in mode 0).
    let ground = crate::scene_terrain::terrain_mesh(
        inputs.terrain,
        inputs.presentation.terrain_light_direction,
    );
    // The water's look: the texture of retail's all-water tile.
    let tables = scene.primitive_tables();
    let water_slot = tables.water.shoreline[15].sprite_slot;
    let water_sprite = resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Sprite,
            argument: i32::from(water_slot),
        })
        .ok();
    let blends: Vec<_> = (0..inf_formats::TERRAIN_BLEND_COUNT as u16)
        .map(|rank| {
            let id = (inf_formats::TERRAIN_BLEND_TEXTURE_RESOURCE_BASE + rank) as i16;
            resources
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: i32::from(id),
                })
                .ok()
                .map(|resource| (id, resource))
        })
        .collect();
    if let Some(builder) = scene.scene_feed_mut() {
        builder.terrain_sprites = blends
            .into_iter()
            .map(|blend| {
                blend.map(|(id, resource)| {
                    builder.sprite(id, resource, inf_formats::SpriteRenderMode::Mode0)
                })
            })
            .collect();
        let texture = water_sprite.map(|resource| {
            builder.sprite(water_slot, resource, inf_formats::SpriteRenderMode::Mode0)
        });
        builder.water_sprite = texture;
        let water_reach = water_reach(inputs.fade);
        builder.feed.water_reach = water_reach as f32;
        builder.feed.eye_under = inputs.eye_under;
        builder.feed.water = crate::scene_water::add_water(
            &mut builder.feed,
            crate::scene_water::WaterRequest {
                terrain: inputs.terrain,
                eye: inputs.eye,
                clock: inputs.clock,
                world_flags: inputs.world_flags,
                sine: &tables.sine_table,
                reach: water_reach,
                texture,
            },
        );
        // The guns' bolts light what they pass (some guns emit
        // light, the plasma weapon and its bolts for example),
        // each in its sprite's colour and each its own light: gathered, the
        // twin guns' side-by-side bolts lit and glinted from their middle.
        // The cannon's sparks light a little too, less than the bolts.
        let tuning = crate::modern_tuning::current();
        let materials = tuning.materials();
        for ((strength, reach), bolts) in [
            (materials.shots, inputs.particles.bolts(inputs.executable)),
            (materials.sparks, inputs.particles.sparks(inputs.executable)),
        ] {
            // Where they were born this tick, their gun is: thermal vision
            // warms it.
            builder.feed.shots.extend(
                bolts
                    .iter()
                    .filter(|bolt| bolt.fresh)
                    .map(|bolt| bolt.position),
            );
            if strength <= 0.0 || reach <= 0.0 {
                continue;
            }
            for bolt in bolts {
                let Ok(resource) = resources.resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Sprite,
                    argument: i32::from(bolt.sprite),
                }) else {
                    continue;
                };
                let texture =
                    builder.sprite(bolt.sprite, resource, inf_formats::SpriteRenderMode::Mode0);
                let centre = std::array::from_fn(|axis| {
                    f32::from(i32::from(bolt.position[axis]).wrapping_sub(inputs.eye[axis]) as i16)
                });
                builder.lone_glow(
                    centre,
                    crate::scene_feed::SceneGlow {
                        texture: Some(texture),
                        color: [255; 4],
                        strength,
                        reach: reach * bolt.scale.clamp(0.5, 2.0),
                        model: SHOT_GLOWS,
                        ..Default::default()
                    },
                );
            }
        }
        // Pelagia's rework lights its colony, its way home and a beacon over
        // the outer islets as night falls ([`crate::pelagia::night_lights`]).
        if inputs.world == crate::pelagia::WORLD
            && resources
                .world_strings()
                .is_some_and(|strings| strings.rework)
        {
            for (centre, light) in crate::pelagia::night_lights(
                [r, g, b],
                inputs.clock_micros,
                inputs.eye,
                inputs.terrain,
            ) {
                builder.lone_glow(centre, light);
            }
        }
        // Worlds without water hold the plane at -4096 or below.
        let plane = inputs.terrain.plane_base() >> inputs.terrain.plane_shift();
        builder.water_plane = (plane > -4096).then(|| (plane - inputs.eye[1]) as f32);
        builder.world_fog = Some((inputs.fade, inputs.far_color));
        let feed = &mut builder.feed;
        world_light(feed, &tones, &resources, &inputs);
        feed.terrain = Some(ground);
        feed.sparks = inputs.sparks.clone();
        feed_splashes(feed, &inputs);
    }
}

/// Give `feed` the world's splashes and blasts in `inputs`, which the water
/// takes (the tracer's, and Vulkan's raster's).
fn feed_splashes(feed: &mut crate::scene_feed::SceneFeed, inputs: &WorldFeedInputs<'_>) {
    // The splashes, eye-relative across the 16-bit wrap.
    let eye = inputs.eye;
    feed.splashes = inputs
        .particles
        .splashes(inputs.executable)
        .into_iter()
        .map(|splash| crate::scene_feed::SceneSplash {
            position: std::array::from_fn(|axis| {
                f32::from(i32::from(splash.position[axis]).wrapping_sub(eye[axis]) as i16)
            }),
            age: splash.age,
            scale: splash.scale,
        })
        .collect();
    // The blasts, and how far each lies under the water's plane where
    // there is water under it (worlds without water hold the plane at
    // -4096 or below).
    let terrain = inputs.terrain;
    let plane = terrain.plane_base() >> terrain.plane_shift();
    feed.blasts = inputs
        .particles
        .blasts()
        .into_iter()
        .map(|blast| {
            let [x, y, z] = blast.position;
            let water = plane > -4096 && i32::from(terrain.height_at([x, z])) < plane;
            crate::scene_feed::SceneBlast {
                world: blast.position,
                depth: water.then(|| (plane - i32::from(y as i16)) as f32),
                large: blast.large,
            }
        })
        .collect();
}

/// Retail's colours a world's light and fog are made from: its sky's (H7
/// 0x72), its far terrain's (0x73, or 0x20 under infrared vision,
/// 0044B14B) and the material shifts that unpack them.
pub(crate) struct WorldTones {
    sky: Option<u32>,
    far_terrain: u32,
    shifts: [u32; 3],
}

pub(crate) fn world_tones(
    scene: &SoftwareScene<'_, '_>,
    resources: &crate::render_resources::RenderResources<'_>,
    inputs: &WorldFeedInputs<'_>,
) -> WorldTones {
    let material = |argument: i32| match resources
        .resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument,
        })
        .map(|resource| resource.data)
    {
        Ok(RenderResourceData::Material(material)) => {
            Some(scene.color_layout().convert_material(material))
        }
        _ => None,
    };
    WorldTones {
        sky: material(0x72),
        far_terrain: material(if inputs.infrared { 0x20 } else { 0x73 })
            .unwrap_or(inputs.far_color),
        shifts: scene.pixels.renderer.material_shifts,
    }
}

/// The world's light, fog, weather, clocks and sky colour into `feed`: the
/// frame block's inputs beside the tracer's geometry, which Vulkan's raster
/// takes for its own modern sky too ([`raster_sky_feed`]).
pub(crate) fn world_light(
    feed: &mut crate::scene_feed::SceneFeed,
    tones: &WorldTones,
    resources: &crate::render_resources::RenderResources<'_>,
    inputs: &WorldFeedInputs<'_>,
) {
    let linear = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 255.0);
    // Retail lights a model with the tint along N.L plus an ambient of a
    // quarter of it (objects, scene_terrain::AMBIENT_SHARE), and the
    // consumers apply a shade of 128 as one: the same scale keeps the
    // traced world as bright as retail's.
    let shade = |rgb: [u8; 3]| rgb.map(|channel| f32::from(channel) / 128.0);
    let [r, g, b, _] = inputs.presentation.tint_word.to_le_bytes();
    feed.nightfall = nightfall([r, g, b]);
    feed.night_weather = inputs.world != crate::pelagia::WORLD
        || resources
            .world_strings()
            .is_some_and(|strings| strings.rework);
    feed.water_clock = inputs.clock;
    feed.clock_micros = inputs.clock_micros;
    // Worlds without water hold the plane at -4096 or below; as the ground
    // mesh holds it (scene_terrain), in the world's 16-bit heights.
    let plane = inputs.terrain.plane_base() >> inputs.terrain.plane_shift();
    feed.water_level = (plane > -4096).then(|| f32::from(plane as i16));
    feed.sun = crate::scene_feed::SceneSun {
        direction: inputs.presentation.terrain_light_direction,
        color: shade([r, g, b]),
        ambient: shade([r >> 2, g >> 2, b >> 2]),
    };
    // A world's tuned fog colour stands in for retail's fog and far
    // colours (the Command Centre's open blue).
    let tuned = crate::modern_tuning::current().fog(inputs.world);
    let tuned_color = tuned.color;
    // The view draws `past` beyond the fog's end (the world draw's
    // predraw); the fog keeps its own end.
    let past = tuned.past.max(1.0);
    let shifts = tones.shifts;
    feed.fog = crate::scene_feed::SceneFog {
        color: tuned_color
            .unwrap_or_else(|| linear(crate::d3d_effect::far_rgb(inputs.far_color, shifts))),
        start: inputs.fade[0] as f32 / past,
        end: inputs.fade[1] as f32 / past,
        far_color: tuned_color
            .unwrap_or_else(|| linear(crate::d3d_effect::far_rgb(tones.far_terrain, shifts))),
        far_ring: true,
    };
    feed.world = inputs.world;
    feed.darkened = inputs.darkened;
    feed.weather = inputs
        .weather
        .changes()
        .then(|| crate::scene_feed::SceneWeather {
            fall: inputs.weather.fall(),
            wet_ground: inputs.weather.wet_ground(),
            wet_models: inputs.weather.wet_models(),
            puddles: inputs.weather.puddles(),
        });
    feed.infrared = inputs.infrared;
    feed.craft = inputs.craft;
    feed.cloud_wake.clone_from(&inputs.cloud_wake);
    let [r, g, b, _] = inputs.presentation.base_tint_word.to_le_bytes();
    feed.light_base = Some(shade([r, g, b]));
    if let Some(sky) = tones.sky {
        feed.sky = linear(crate::d3d_effect::far_rgb(sky, shifts));
    }
}

/// The world as Vulkan's raster takes it for its own modern sky: the
/// camera and the world's light alone, never the scene feed's geometry
/// (whose ground and water would take retail's draws' place); where the
/// device `wets` the ground in the rain, the ground's grid.
pub(crate) fn raster_sky_feed(
    scene: &SoftwareScene<'_, '_>,
    resources: &crate::render_resources::RenderResources<'_>,
    inputs: &WorldFeedInputs<'_>,
    camera: crate::scene_feed::SceneCamera,
    wets: bool,
) -> crate::scene_feed::SceneFeed {
    let mut feed = crate::scene_feed::SceneFeed::default();
    feed.begin(camera);
    world_light(
        &mut feed,
        &world_tones(scene, resources, inputs),
        resources,
        inputs,
    );
    // On a world the rain may wet, the ground's grid for the device's wet
    // ground and puddles, from the world's load (kept until the ground
    // changes), not the frame the rain first wets it; while it does, the
    // water's reach their shore takes, as the tracer's.
    if wets && inputs.weather.may_wet() {
        feed.raster_ground = Some(crate::scene_terrain::terrain_mesh(
            inputs.terrain,
            inputs.presentation.terrain_light_direction,
        ));
        if feed.weather.is_some_and(|weather| weather.wet_ground > 0.0) {
            feed.water_reach = water_reach(inputs.fade) as f32;
        }
    }
    // Retail's splashes and the blasts, which ring and stir the device's
    // water as the tracer's.
    feed_splashes(&mut feed, inputs);
    // The struck crystals' sparks, whose arcs and light the sky's world
    // pass lays (vk_sky.comp).
    feed.sparks = inputs.sparks.clone();
    feed
}

/// How far from the eye the traced water reaches: the water within sight,
/// at the game's own wave heights, to the fog's end (of the view's `fade`);
/// the tracer takes the plane beyond.
fn water_reach(fade: [i32; 2]) -> i32 {
    fade[1].clamp(1024, 12800)
}

/// The model number the guns' shots' lights gather under, which no model
/// has.
pub(crate) const SHOT_GLOWS: u16 = u16::MAX;

/// The share of a world's fall that comes now, 0 to 1: only under the
/// modern sky, except a fall that waits for the night (Pelagia's rain),
/// which comes over the painted sky too, as thick as night has fallen, and
/// only where `night_weather` lets it (Pelagia's is the rework's).
pub(crate) fn precip_share(
    precip: &crate::modern_tuning::PrecipTuning,
    modern_sky: bool,
    nightfall: f32,
    night_weather: bool,
) -> f32 {
    if !modern_sky && !precip.with_night {
        return 0.0;
    }
    match (precip.with_night, night_weather) {
        (false, _) => 1.0,
        (true, true) => nightfall.clamp(0.0, 1.0),
        (true, false) => 0.0,
    }
}

/// The colour the craft's spray is drawn in (0 to 1): white water in the
/// world's light, retail's tint (`005474E0`), a little under it.
fn spray_colour(presentation: &crate::world_backend::WorldPresentationState) -> [f32; 3] {
    let [r, g, b, _] = presentation.tint_word.to_le_bytes();
    [r, g, b].map(|channel| 0.9 * f32::from(channel) / 255.0)
}

/// The player's craft as it stirs puddles: the unit, its middle from the
/// eye, and how hard it stirs them (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
struct PlayerWake {
    unit: crate::object_runtime::ObjectId,
    centre: [f32; 3],
    strength: f32,
}

/// How far above the ground a wheel's lowest point may ride and still
/// touch it (the suspension's give).
const WHEEL_TOUCH: f32 = 24.0;

/// How far above the water's plane the craft's middle may ride and still
/// stir it (a hovercraft's cushion), and how far under it (a buggy wading
/// with its body through the surface).
const HULL_OVER_WATER: f32 = 160.0;
const HULL_UNDER_WATER: f32 = 64.0;

/// Where the player's craft stirs the puddles and the water from, once its
/// draws have laid its wheels in `feed`. Puddles: each wheel that touches
/// the ground, at the ground under its hub (any of the
/// wheels in contact with the puddle, 1 to 4), or its
/// middle where it drew no wheels (the hovercraft, the skimmer); the tracer
/// stirs a puddle only from those that stand in it. The water (both the
/// ACAM body and the wheels make surface ripples): each wheel through its surface and the hull riding
/// in or just over it, at the plane, with the strength negated so the
/// puddles skip them (`pt_splash.glsl`'s `splashed` takes them).
fn craft_wakes(
    feed: &crate::scene_feed::SceneFeed,
    wake: Option<PlayerWake>,
    terrain: &crate::terrain_runtime::TerrainRuntime,
    eye: [i32; 3],
) -> Vec<crate::scene_feed::SceneWake> {
    let Some(wake) = wake else {
        return Vec::new();
    };
    // Worlds without water hold the plane at -4096 or below.
    let plane = terrain.plane_base() >> terrain.plane_shift();
    let plane = (plane > -4096).then(|| f32::from(plane as i16) - eye[1] as f32);
    let ground_at = |at: [f32; 3]| {
        let word = |axis: usize| (eye[axis] as f32 + at[axis]).round() as i32 as u16;
        f32::from(terrain.height_at([word(0), word(2)])) - eye[1] as f32
    };
    let on_water = |at: [f32; 3], wet: bool| {
        plane
            .filter(|&plane| wet && ground_at(at) < plane)
            .map(|plane| crate::scene_feed::SceneWake {
                position: [at[0], plane, at[2]],
                strength: -wake.strength,
            })
    };
    let drawn: Vec<u32> = feed
        .objects
        .iter()
        .enumerate()
        .filter(|(_, object)| object.id == wake.unit)
        .map(|(index, _)| index as u32)
        .collect();
    let wheels: Vec<_> = feed
        .wheels
        .iter()
        .filter(|wheel| drawn.contains(&wheel.object))
        .collect();
    let hull = on_water(
        wake.centre,
        plane.is_some_and(|plane| {
            (-HULL_UNDER_WATER..=HULL_OVER_WATER).contains(&(wake.centre[1] - plane))
        }),
    );
    if wheels.is_empty() {
        return std::iter::once(crate::scene_feed::SceneWake {
            position: wake.centre,
            strength: wake.strength,
        })
        .chain(hull)
        .collect();
    }
    wheels
        .into_iter()
        .filter_map(|wheel| {
            let (low, high) = (wheel.hub[1] - wheel.radius, wheel.hub[1] + wheel.radius);
            if let Some(water) = on_water(
                wheel.hub,
                plane.is_some_and(|plane| low <= plane && plane <= high),
            ) {
                return Some(water);
            }
            let ground = ground_at(wheel.hub);
            (low - ground <= WHEEL_TOUCH).then_some(crate::scene_feed::SceneWake {
                position: [wheel.hub[0], ground, wheel.hub[2]],
                strength: wake.strength,
            })
        })
        .chain(hull)
        .collect()
}

/// Where the player's craft is (16-bit world words) while the camera
/// follows it, which the clouds keep in sight: none in first person, while
/// the player steers a remote unit (the Cranial Implanter's missile, whose
/// nose the camera rides), or without one. It changes nothing.
fn followed_craft(world: &crate::world_backend::WorldSession) -> Option<[u16; 3]> {
    let arena = &world.arena;
    let (vehicle, _) = arena.first_player_drive()?;
    if arena.first_person_craft().is_some() || arena.first_player_steers_remote() {
        return None;
    }
    arena.object(vehicle).ok()?.base.position_words
}

/// The player's craft from `eye` and how hard it stirs the puddles it
/// drives through: by its speed, from a walk (40) to a run (300); none when
/// it stands still or flies.
fn player_wake(world: &crate::world_backend::WorldSession, eye: [i32; 3]) -> Option<PlayerWake> {
    let player = world.arena.first_player()?;
    let state = world.arena.world_player(player).ok()?;
    let handles = world.arena.actor_handles()?;
    let unit = handles
        .peek(state.handle_d8)
        .or_else(|| handles.peek(state.handle_d4))?;
    let actor = world.arena.actor(unit).ok()?;
    let speed = actor
        .words_8e_92
        .map(|word| f32::from(word as i16))
        .iter()
        .map(|axis| axis * axis)
        .sum::<f32>()
        .sqrt();
    // Faint at a crawl, full by a brisk drive (small
    // ripples also show when driving over a puddle).
    let strength = ((speed - 10.0) / 160.0).clamp(0.0, 1.0);
    let [x, y, z] = world.arena.object(unit).ok()?.base.position_words?;
    // Over water, from its surface, not the bed (a hovercraft on a lake).
    let ground = i32::from(world.terrain.height_at([x, z]))
        .max(world.terrain.plane_base() >> world.terrain.plane_shift());
    if strength <= 0.0 || i32::from(y as i16) - ground > 160 {
        return None;
    }
    let from = |word: u16, axis: usize| f32::from(i32::from(word).wrapping_sub(eye[axis]) as i16);
    Some(PlayerWake {
        unit,
        centre: [from(x, 0), from(y, 1), from(z, 2)],
        strength,
    })
}

/// How thick the rain falls on `world` now, 0 to 1, as the tracer draws
/// it (none where the world's fall is not rain), for the ambience.
pub(crate) fn rain_now(
    world: &crate::world_backend::WorldSession,
    residency: &crate::ResidentRuntime,
    rework: bool,
) -> f32 {
    let index = world.mission.current_world_a0;
    let tuning = crate::modern_tuning::current();
    let sky = tuning.sky(index);
    if sky.precip.kind != crate::modern_tuning::PrecipKind::Rain {
        return 0.0;
    }
    sky.precip.density.clamp(0.0, 1.0) * rain_shown(world, residency, rework) * world.weather.fall()
}

/// The share of world's rain that falls where it falls at all, 0 to 1,
/// before the weather's own fade ([`precip_share`]): Pelagia's only as
/// night falls, and only with the rework on; none under a painted sky that
/// draws no fall. The ground soaks only from rain that falls
/// ([`crate::world_weather::WorldWeather::tick`]).
pub(crate) fn rain_shown(
    world: &crate::world_backend::WorldSession,
    residency: &crate::ResidentRuntime,
    rework: bool,
) -> f32 {
    let index = world.mission.current_world_a0;
    let tuning = crate::modern_tuning::current();
    let sky = tuning.sky(index);
    let modern_sky = sky.style == crate::modern_tuning::SkyStyle::Modern;
    let fallen = crate::world_tint::tint_now(world, residency).map_or(0.0, nightfall);
    let night_weather = index != crate::pelagia::WORLD || rework;
    precip_share(&sky.precip, modern_sky, fallen, night_weather)
}

/// The modern sky's lightning on `world`, flashes a second: none under
/// the painted sky, as the tracer draws it.
pub(crate) fn lightning_now(world: u32) -> f32 {
    let tuning = crate::modern_tuning::current();
    let sky = tuning.sky(world);
    if sky.style == crate::modern_tuning::SkyStyle::Modern {
        sky.lightning.rate.clamp(0.0, 2.0)
    } else {
        0.0
    }
}

/// How far night has fallen, 0 by day to 1 by night, from the tint a
/// world's schedule gives it now: world 1's day (186, 133, 105) and sunset
/// (220, 117, 89) read 0, its night (32, 28, 57) 1, so what waits for the
/// night (Pelagia's lights, its rain) comes on as the sunset fades.
pub(crate) fn nightfall(tint: [u8; 3]) -> f32 {
    let [r, g, b] = tint.map(f32::from);
    let brightness = 0.3 * r + 0.59 * g + 0.11 * b;
    ((120.0 - brightness) / 70.0).clamp(0.0, 1.0)
}

/// What the tracer takes of retail's light grid once a view's draws have
/// left it complete.
pub(crate) struct WorldLightFeed<'a> {
    /// The grid's cell spacing in world units.
    pub spacing: u16,
    /// Settings +80: the grid lights the ground too.
    pub ground: bool,
    pub terrain: &'a crate::terrain_runtime::TerrainRuntime,
    pub eye: [i32; 3],
    pub executable: inf_formats::FrontendExecutable<'a>,
}

/// The vehicles that light the most lamps (network games).
const LAMP_VEHICLES: usize = 4;

/// The buggy's and truck's rear lamps in model units, the right of each
/// pair (the `1A` billboards of `wheelmode1a` and `wheelmode2a`), 2 behind
/// them so their light starts clear of the hull: the red pair and the white.
const REAR_RED: [f32; 3] = [17.0, -6.0, -83.0];
const REAR_WHITE: [f32; 3] = [10.0, -6.0, -83.0];

/// The lamps of `vehicles`, eye-relative across the 16-bit wrap, the
/// nearest vehicles to `eye` first. Each pair stands at a model position
/// and its mirror across the nose: with the headlights on, the headlamps at
/// the tuning's, aimed down the nose turned down by its pitch; with the rear
/// lamps lit, the lit pair, aimed straight back turned down by theirs.
pub(crate) fn place_lamps(
    vehicles: &[crate::object_runtime::VehicleLamp],
    eye: [i32; 3],
    look: &crate::modern_tuning::HeadlightTuning,
) -> Vec<crate::scene_feed::SceneLamp> {
    fn unit(v: [f32; 3]) -> [f32; 3] {
        let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1.0e-6);
        v.map(|c| c / length)
    }
    let mut placed: Vec<(f32, Vec<crate::scene_feed::SceneLamp>)> = vehicles
        .iter()
        .map(|vehicle| {
            let row = |first: usize| -> [f32; 3] {
                unit(std::array::from_fn(|axis| {
                    f32::from_bits(vehicle.basis[first + axis])
                }))
            };
            let [across, up, nose] = [row(1), row(4), row(7)];
            let hull: [f32; 3] =
                std::array::from_fn(|c| f32::from(vehicle.hull[c].wrapping_sub(eye[c]) as i16));
            let pair = |[x, y, z]: [f32; 3], back: bool, pitch: f32, color: [f32; 3]| {
                let ahead = nose.map(|c| if back { -c } else { c });
                let (down, level) = pitch.to_radians().sin_cos();
                let axis = unit(std::array::from_fn(|c| ahead[c] * level - up[c] * down));
                let lamp_up = unit(std::array::from_fn(|c| up[c] * level + ahead[c] * down));
                [1.0_f32, -1.0].map(|side| crate::scene_feed::SceneLamp {
                    position: std::array::from_fn(|c| {
                        hull[c] + across[c] * x * side + up[c] * y + nose[c] * z
                    }),
                    axis,
                    across,
                    up: lamp_up,
                    color,
                    rear: back,
                })
            };
            let mut lamps = Vec::new();
            if vehicle.head {
                lamps.extend(pair(look.position, false, look.pitch, look.color));
            }
            let rear = &look.rear;
            match vehicle.rear {
                Some(crate::object_runtime::RearLamps::Red) => {
                    lamps.extend(pair(REAR_RED, true, rear.pitch, rear.red))
                }
                Some(crate::object_runtime::RearLamps::White) => {
                    lamps.extend(pair(REAR_WHITE, true, rear.pitch, rear.white))
                }
                None => {}
            }
            let away = (hull[0] * hull[0] + hull[1] * hull[1] + hull[2] * hull[2]).sqrt();
            (away, lamps)
        })
        .collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    placed
        .into_iter()
        .take(LAMP_VEHICLES)
        .flat_map(|(_, lamps)| lamps)
        .collect()
}

/// Give the scene feed `scene` is collecting the lamps of `vehicles` as
/// soon as the light pass has kept them, so the furniture's crystals catch
/// them ([`place_lamps`]): each headlamp with its lens, a small unlit white
/// disc a unit behind where its light starts, so it never shadows it, seen
/// from in front, in the water and in metal (the rear lamps show retail's
/// billboards).
pub(crate) fn feed_lamps(
    scene: &mut SoftwareScene<'_, '_>,
    vehicles: &[crate::object_runtime::VehicleLamp],
    eye: [i32; 3],
) {
    let Some(builder) = scene.scene_feed_mut() else {
        return;
    };
    let tuning = crate::modern_tuning::current();
    builder.feed.lamps = place_lamps(vehicles, eye, tuning.headlight());
    let lens = tuning.headlight().lens;
    if lens > 0.0 {
        for lamp in builder
            .feed
            .lamps
            .clone()
            .into_iter()
            .filter(|lamp| !lamp.rear)
        {
            let corners: Vec<crate::scene_feed::SceneVertex> = (0..8)
                .map(|step| {
                    let (sin, cos) = (step as f32 * std::f32::consts::TAU / 8.0).sin_cos();
                    crate::scene_feed::SceneVertex {
                        position: std::array::from_fn(|c| {
                            lamp.position[c] - lamp.axis[c]
                                + (lamp.across[c] * cos + lamp.up[c] * sin) * lens
                        }),
                        normal: lamp.axis,
                        color: [255; 4],
                        ..Default::default()
                    }
                })
                .collect();
            builder.feed.polygon(
                &corners,
                None,
                crate::scene_feed::SceneSurface::Shown,
                None,
                None,
                false,
            );
        }
    }
}

/// A lamp's light at eye-relative `point` before the surface's facing and
/// its shadow, in full lights: the CPU's twin of `pt_frame.glsl`'s
/// `lamp_beam`, for what the raster draws (the crystals).
pub(crate) fn lamp_beam(
    lamp: &crate::scene_feed::SceneLamp,
    look: &crate::modern_tuning::LampLook,
    point: [f32; 3],
) -> f32 {
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let offset: [f32; 3] = std::array::from_fn(|c| point[c] - lamp.position[c]);
    let distance = dot(offset, offset).sqrt();
    let local = [
        dot(offset, lamp.across),
        dot(offset, lamp.up),
        dot(offset, lamp.axis),
    ];
    if local[2] <= 0.0 || distance >= look.reach {
        return 0.0;
    }
    let smooth = |from: f32, to: f32, x: f32| {
        let t = ((x - from) / (to - from)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    let side = local[0].atan2(local[2]).abs();
    let rise = local[1].atan2(local[2]);
    let mut shape = 1.0 - smooth(look.spread * look.inner, look.spread, side);
    shape *= 1.0 - smooth(look.cutoff, look.cutoff + look.soft, rise);
    let below = (-rise).max(0.0) / look.hot;
    shape *= (-below * below).exp();
    let window = 1.0 - (distance / look.reach).powi(4);
    let falloff = 1.0e6 / (distance * distance + look.near * look.near);
    look.intensity * shape * falloff * window * window
}

/// The glare of the headlamps among `lamps` that face the eye, as stars
/// over the traced world: as strong as the eye stands within each lamp's
/// beam, its size the lens's (the plan's subtle glare; none at `glare` 0).
pub(crate) fn lamp_glare(
    camera: &crate::scene_feed::SceneCamera,
    lamps: &[crate::scene_feed::SceneLamp],
    look: &crate::modern_tuning::HeadlightTuning,
) -> Vec<Vec<crate::d3d_viewport::D3dTlVertex>> {
    let mut fans = Vec::new();
    if look.glare <= 0.0 {
        return fans;
    }
    let edge = look.spread.clamp(1.0, 89.0).to_radians().cos();
    for lamp in lamps.iter().filter(|lamp| !lamp.rear) {
        let away = lamp.position.map(|c| -c);
        let length = (away[0] * away[0] + away[1] * away[1] + away[2] * away[2]).sqrt();
        let facing = (0..3).map(|c| lamp.axis[c] * away[c]).sum::<f32>() / length.max(1.0e-3);
        if facing <= edge {
            continue;
        }
        let Some([x, y, depth]) = crate::scene_gate::project(camera, lamp.position) else {
            continue;
        };
        let within = ((facing - edge) / (1.0 - edge)).clamp(0.0, 1.0);
        let radius = (look.lens * 8.0 * camera.focal[0] / depth).max(2.0);
        crate::scene_gate::star(
            camera,
            [x, y],
            depth,
            radius,
            look.color,
            look.glare * within * within,
            0.0,
            &mut fans,
        );
    }
    fans
}

/// Give the scene feed `scene` is collecting the light grid `light_grid`:
/// its cells, and each light deposited this frame, standing a little above
/// the ground under it, with where it shines from (above the ground there
/// too), eye-relative across the 16-bit wrap.
pub(crate) fn feed_light_grid(
    scene: &mut SoftwareScene<'_, '_>,
    light_grid: &crate::world_lighting::WorldLightGrid,
    inputs: WorldLightFeed<'_>,
) {
    let outside = inputs
        .executable
        .mapped(0x0052_6068, 3)
        .map(|mapped| std::array::from_fn(|channel| mapped.bytes[channel] as i8))
        .unwrap_or_default();
    let Some(builder) = scene.scene_feed_mut() else {
        return;
    };
    let eye = inputs.eye;
    builder.feed.light_grid = Some(crate::scene_feed::SceneLightGrid {
        anchors: light_grid.anchors(),
        spacing: inputs.spacing,
        cells: light_grid
            .cells()
            .chunks_exact(3)
            .map(|rgb| [rgb[0] as i8, rgb[1] as i8, rgb[2] as i8])
            .collect(),
        outside,
        ground: inputs.ground,
        sources: light_grid
            .sources()
            .iter()
            .map(|source| {
                let above = |[x, y, z]: [i32; 3]| {
                    let ground = inputs.terrain.height_at([x as u16, z as u16]);
                    [x, y.max(i32::from(ground) + LIGHT_SOURCE_HEIGHT), z]
                };
                let [x, z] = source.world_xz;
                let position = above([x, i32::MIN, z]);
                let emitter = source.emitter.map_or(position, above);
                let relative = |point: [i32; 3]| {
                    std::array::from_fn(
                        |axis| f32::from(point[axis].wrapping_sub(eye[axis]) as i16),
                    )
                };
                crate::scene_feed::SceneLightSource {
                    position: relative(position),
                    radius: source.radius as f32,
                    rgb: source.rgb.map(|channel| f32::from(channel) / 128.0),
                    particle: source.particle,
                    emitter: relative(emitter),
                    // Only what burns warms: blasts, hits, shots and muzzle
                    // flashes, not lamps (the ACAM's own would heat it).
                    heat: if source.warms && source.particle {
                        source
                            .own
                            .iter()
                            .map(|&channel| f32::from(channel))
                            .fold(0.0, f32::max)
                            / 127.0
                    } else {
                        0.0
                    },
                }
            })
            .collect(),
    });
}

/// Port option, the full path tracing plan's stage 8: the tracer takes the
/// models outside its view that reach it ([`reaching_view`]). Off holds
/// them (the first-person hull is traced either way).
const OUTSIDE_VIEW_MODELS: bool = true;

/// How near the eye (world units, past its own extent) a model outside the
/// view stays in the traced scene whatever the sun: the bounce light, the
/// metal's reflections and the lamps' shadows it gives the view fall off
/// with its distance.
const OUTSIDE_VIEW_NEAR: f32 = 1024.0;

/// The least step (world units) of a model's shadow toward the ground.
const SHADOW_STEP: f32 = 256.0;

#[cfg(test)]
thread_local! {
    /// Tests: the traced draw takes nothing from outside its view.
    static NOTHING_OUTSIDE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Of `culled` (the object pass's), the objects outside the view that can
/// reach it, in its order: within [`OUTSIDE_VIEW_NEAR`] of the eye, mirrored
/// into the view by the water's plane `water` (eye-relative), or casting
/// their shadow into it: their bounds (a sphere their extent round) swept
/// along the sun's light (`sun`, toward the sun) until wholly under the
/// `ground` (its height under an eye-relative point, eye-relative), against
/// the view's frustum. The sweep ends at the ground under its own line, so
/// a shadow thrown down past a cliff's foot well beyond its extent is
/// missed.
fn reaching_view(
    culled: &[crate::object_runtime::CulledObject],
    camera: &crate::scene_feed::SceneCamera,
    sun: [f32; 3],
    water: Option<f32>,
    ground: impl Fn([f32; 3]) -> f32,
) -> Vec<crate::object_runtime::ObjectId> {
    let frustum = ViewFrustum::of(camera);
    let reaches = |object: &crate::object_runtime::CulledObject| {
        let centre = object.offset.map(|axis| axis as f32);
        let radius = object.extent.max(0) as f32;
        if centre.iter().map(|axis| axis * axis).sum::<f32>().sqrt() - radius < OUTSIDE_VIEW_NEAR {
            return true;
        }
        if let Some(plane) = water.filter(|&plane| centre[1] > plane) {
            let mirrored = [centre[0], 2.0 * plane - centre[1], centre[2]];
            if frustum.meets(mirrored, mirrored, radius) {
                return true;
            }
        }
        if sun[1] <= 0.0 {
            return false;
        }
        let step = sun.map(|axis| -axis * radius.max(SHADOW_STEP));
        let mut end = centre;
        let mut travelled = 0.0;
        while travelled < camera.far && end[1] + radius > ground(end) {
            end = std::array::from_fn(|axis| end[axis] + step[axis]);
            travelled += radius.max(SHADOW_STEP);
        }
        frustum.meets(centre, end, radius)
    };
    culled
        .iter()
        .filter(|object| reaches(object))
        .map(|object| object.id)
        .collect()
}

/// A view's frustum: the planes through the eye at its four edges (unit
/// outward normals, eye-relative) and its depths along `forward`.
struct ViewFrustum {
    sides: [[f32; 3]; 4],
    forward: [f32; 3],
    depth: [f32; 2],
}

impl ViewFrustum {
    fn of(camera: &crate::scene_feed::SceneCamera) -> Self {
        let [right, up, forward] = camera.basis;
        let [width, height] = camera.native.map(|size| size as f32);
        let [cx, cy] = camera.centre;
        let [fx, fy] = camera.focal;
        // An edge's normal in view terms (across, up, depth): a pixel's x is
        // cx + fx * across / depth, its y cy - fy * up / depth.
        let side = |normal: [f32; 3]| {
            let world: [f32; 3] = std::array::from_fn(|axis| {
                normal[0] * right[axis] + normal[1] * up[axis] + normal[2] * forward[axis]
            });
            let length = world.iter().map(|axis| axis * axis).sum::<f32>().sqrt();
            world.map(|axis| axis / length.max(f32::MIN_POSITIVE))
        };
        Self {
            sides: [
                side([-fx, 0.0, -cx]),
                side([fx, 0.0, cx - width]),
                side([0.0, fy, -cy]),
                side([0.0, -fy, cy - height]),
            ],
            forward,
            depth: [camera.near, camera.far],
        }
    }

    /// Whether the capsule from `a` to `b`, `radius` round (eye-relative),
    /// may meet the frustum: no plane has both its ends wholly outside.
    fn meets(&self, a: [f32; 3], b: [f32; 3], radius: f32) -> bool {
        let dot = |normal: &[f32; 3], point: [f32; 3]| {
            normal[0] * point[0] + normal[1] * point[1] + normal[2] * point[2]
        };
        let outside = |normal: &[f32; 3], by: f32| dot(normal, a) > by && dot(normal, b) > by;
        let back = self.forward.map(|axis| -axis);
        !(self.sides.iter().any(|normal| outside(normal, radius))
            || outside(&back, radius - self.depth[0])
            || outside(&self.forward, radius + self.depth[1]))
    }
}

/// The scene feed's camera: the view the world draw published and its
/// projection, with the D3D consumers' near plane at 64.
pub(crate) fn scene_camera(
    view: SceneView,
    setup: crate::frontend_scene::SceneSetup,
) -> crate::scene_feed::SceneCamera {
    let row = |axis: usize| {
        std::array::from_fn(|column| f32::from_bits(view.basis.words[1 + axis * 3 + column]))
    };
    crate::scene_feed::SceneCamera {
        eye: view.eye.map(f64::from),
        basis: [row(0), row(1), row(2)],
        focal: setup.focal.map(|value| value as f32),
        centre: setup.center.map(|value| value as f32),
        native: setup.viewport.map(|value| value.max(1) as u32),
        near: 64.0,
        far: setup.fade[1].max(64) as f32,
    }
}

#[cfg(test)]
#[path = "world_scene_tests.rs"]
mod tests;
