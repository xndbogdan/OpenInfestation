//! The remastered camera (2026-10-27): the Video menu's Camera Style row (settings `+6C`)
//! gains Remastered beside retail's Normal and Cinematic, on every renderer.
//! Retail's camera code takes the cinematic tables for exactly 1, so 2 runs
//! as Normal, on which the remaster builds:
//!
//! - It rises over what stays between it and the craft it chases (a
//!   building backed into, a tank close behind, a rock), as V2000's own chase
//!   camera does (the steepest slope that clears what lies behind the
//!   craft), instead of showing the inside of the model. Retail's
//!   camera keeps above the ground and the water alone (`00410BF0`).
//! - It sees wider: 100 degrees across a 4:3 view where retail's sees 90
//!   ([`focal`]), the rest of the view widening alike, and wider still with
//!   speed: up to 106 at full speed and 110 while a turbo boost runs
//!   ([`speed_kick`]).
//! - First person rolls half as far as the game's view, itself half the
//!   craft's roll (first person is remastered too); its heading and pitch stay the game's.
//! - A turbo boost kicks the camera with a short shake, and the skimmer
//!   pushed on (Thrust or Climb held) rumbles it a little; the buggy rumbles
//!   it as it gains speed and while its boost runs, and shakes 1.4 times as
//!   hard, the skimmer 1.12 times.
//! - It follows the craft under the water (it sees under the water instead of
//!   remaining above it), where the renderer gives the view its underwater look: the
//!   world draw's own fog and wobble, or Vulkan's raster's murk with the
//!   surface seen from below (`world_scene::UnderwaterLook`).
//! - At speed it chases (in high speed and turning, as the skimmer
//!   circling Oolbryte's pit shows, it behaves like a chase camera rather
//!   than a lazy one): retail's slew caps its acceleration, so a craft turning
//!   hard at speed leaves its eye and look behind. The remaster holds them on
//!   a leash to where the game wants them ([`leashed`]), and never lets the
//!   craft stray far from the middle of the view ([`framed`]).
//!
//! The camera actor, its cached position and all the game reads of it stay
//! retail's; only the view drawn changes. Retail runs some logic inside the
//! world draw for what is in view (particle births, random draws, shots
//! posed), so the view the game's draw takes is the game's: retail's on
//! software, OpenGL and in every network game, where this view is drawn
//! apart from the game (`world_scene::draw_world_views`), and this one in
//! Vulkan single player, the remaster's game. The
//! session's game is fixed when its window opens and its replay records it
//! ([`crate::presentation::GameView`]). The tests keep
//! their own copy of the furniture queries' process state, and the shake
//! its own noise, never the game's random draws. A view is clear when
//! retail's own sight test (`0043AD90`, the aim's) finds nothing on the ray
//! from just above the craft to the eye, against the actors' and the
//! furniture's models and the ground. A view blocked for [`BLOCKED_SECONDS`]
//! raises the eye about the craft, at the same distance and bearing, to the
//! lowest elevation that is clear, and turns to keep looking where the
//! game's view did; what only passes between them (a pylon's leg, a tree)
//! never moves it. Every move eases in and out within its speed (a lift that
//! leapt or a kick that flickered would snap back and jitter): the lift holds a while once clear and settles slowly,
//! and the eased values start afresh when the game cuts the view.

use inf_formats::native_basis::{orient_from_forward, NativeBasis};

use crate::{
    frontend_camera::AnimationClockSource,
    model_process::ModelProcessOwners,
    object_runtime::{CameraDrawRequest, CameraParameterState, ObjectArena, ObjectId},
    terrain_furniture::draw::FurnitureDrawState,
    terrain_runtime::TerrainRuntime,
    world_actor_terrain::WorldTerrainSource,
    world_furniture_effect::ProcessFurnitureQueryState,
    world_sight::{SightActors, SightHit, WorldSightQuery},
    ResidentRuntime,
};

/// Settings `+6C`, the Camera Style row: retail's 0 Normal and 1 Cinematic,
/// and the port's 2 Remastered, its highest value.
pub const CAMERA_STYLE_CELL: usize = 0x6c / 4;
pub const CAMERA_STYLE_REMASTERED: u32 = 2;

/// The camera the Camera Style row draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CameraStyle {
    /// Retail's camera, Normal or Cinematic.
    #[default]
    Original,
    /// The remastered camera: it rises over what would block its view of the
    /// craft, sees wider and follows the craft under the water.
    Remastered,
}

impl CameraStyle {
    /// The style settings word `+6C` selects.
    pub fn from_word(word: u32) -> Self {
        if word == CAMERA_STYLE_REMASTERED {
            Self::Remastered
        } else {
            Self::Original
        }
    }
}

/// The view's focal lengths for `style`: retail's, or the remastered
/// camera's wider view, 100 degrees across 4:3 for retail's 90
/// (`tan 45 / tan 50`). Special views (the scope's zoom) keep retail's.
pub fn focal(style: CameraStyle, special_view: bool, focal: [i32; 2]) -> [i32; 2] {
    if style == CameraStyle::Original || special_view {
        return focal;
    }
    widened(focal, REMASTERED_DEGREES)
}

/// How far above the craft's position the tests' rays start (world units):
/// above its hull, so they leave it upward.
const ANCHOR_RISE: i32 = 48;
/// Below this distance from the craft the camera is not chasing it (first
/// person, a close special view): the view stays the game's.
const CHASE_DISTANCE: f32 = 160.0;
/// The lift's search: the elevation rises in these steps until a view is
/// clear, up to the highest, then halves its last step this many times.
const STEP: f32 = 6.0 * std::f32::consts::PI / 180.0;
const HIGHEST: f32 = 60.0 * std::f32::consts::PI / 180.0;
const REFINE: usize = 3;
/// The lift answers a view blocked this long (seconds).
const BLOCKED_SECONDS: f32 = 0.2;
/// How the lift moves: about how long it takes to rise and to settle
/// (seconds), its fastest (radians a second), and how long it holds its
/// height once the view no longer needs it (seconds).
const RISE_SECONDS: f32 = 0.25;
const SETTLE_SECONDS: f32 = 0.7;
const LIFT_SPEED: f32 = 70.0 * std::f32::consts::PI / 180.0;
const HOLD_SECONDS: f32 = 0.75;
/// A move of the game's eye beyond this in one frame (world units, on any
/// axis), more [`CUT_SPEED`] times the frame's seconds, is a cut, after which
/// the eased values start afresh; a long frame at speed is not.
const CUT_UNITS: i32 = 256;
/// Faster than the game's eye moves (units a second; it reached 7500 round
/// Oolbryte's pit).
const CUT_SPEED: f32 = 8192.0;
/// The remastered view's width across a 4:3 view (degrees), retail's 90 plus
/// ten.
const REMASTERED_DEGREES: f32 = 100.0;
/// First person: the share of the game's roll drawn.
const FIRST_PERSON_ROLL_SHARE: f32 = 0.5;
/// The speed kick: degrees across the view added as the craft's speed along
/// its hull rises between the two speeds (world units a second, about), and
/// while a boost runs; about how long it takes to widen and to narrow
/// (seconds), and its fastest (degrees a second).
const KICK_SPEEDS: [f32; 2] = [300.0, 1000.0];
const KICK_SPEED_DEGREES: f32 = 6.0;
const KICK_BOOST_DEGREES: f32 = 4.0;
const KICK_WIDEN_SECONDS: f32 = 0.35;
const KICK_NARROW_SECONDS: f32 = 0.6;
const KICK_SPEED: f32 = 12.0;
/// The shake: a boost's burst (seconds) and how fast it trembles (a
/// second); the rumble, as a share of the full shake: the skimmer's while
/// pushed on, the buggy's while pushed on as it gains speed (none at the
/// first gain, its share from the second, world units a second each
/// second, about), and at least its boost share while a boost runs; how
/// fast the rumble trembles and how quickly it comes and goes; the buggy's
/// and the skimmer's whole shake (the buggy's while
/// boosting and accelerating made stronger, the skimmer's 10-15%
/// stronger); the reach at 1, world units in chase and degrees in first
/// person.
const BURST_SECONDS: f32 = 0.4;
const BURST_HERTZ: f32 = 12.0;
const SKIM_SHARE: f32 = 0.3;
const BUGGY_GAINS: [f32; 2] = [200.0, 1500.0];
const BUGGY_GAIN_SHARE: f32 = 0.6;
const BUGGY_BOOST_SHARE: f32 = 0.3;
const RUMBLE_HERTZ: f32 = 7.0;
const RUMBLE_SECONDS: f32 = 0.25;
const BUGGY_STRENGTH: f32 = 1.4;
const SKIMMER_STRENGTH: f32 = 1.12;
const SHAKE_UNITS: f32 = 5.0;
const SHAKE_DEGREES: f32 = 0.6;
/// Under water: how far below the waves and above the ground the eye keeps
/// (the near plane, 64, never cuts the waterline, nor the seabed under a
/// steep look down), the shares of its distance from the craft it may come
/// in to for room, and how it follows the craft down and back (about how
/// long it takes, seconds, and its fastest, units a second).
const SURFACE_BAND: i32 = 64;
const SEABED_GAP: i32 = 128;
const DIVE_REACH: [f32; 5] = [1.0, 0.8, 0.65, 0.5, 0.4];
const DIVE_SECONDS: f32 = 0.3;
const DIVE_SPEED: f32 = 800.0;
/// The chase leash: how far the drawn eye and look may lag where the game
/// wants them (world units), slack at the first speed and tight from the
/// second (the craft's speed, world units a second, about); past it the lag
/// eases out to [`LEASH_SLACK`] times as far, and the leash's pull moves in
/// about [`LEASH_SECONDS`], never faster than [`LEASH_RATE`] (units a second).
const LEASH_SPEEDS: [f32; 2] = [1500.0, 2500.0];
const LEASH_EYE: [f32; 2] = [3000.0, 200.0];
const LEASH_LOOK: [f32; 2] = [3000.0, 200.0];
const LEASH_SLACK: f32 = 1.5;
const LEASH_SECONDS: f32 = 0.15;
const LEASH_RATE: f32 = 20_000.0;
/// Retail's most clearance of its eye above the ground and the water
/// (`0041063A`: the eye and look heights' difference plus 32, at most 80).
const EYE_CLEARANCE: i32 = 80;
/// How far from the view's middle the chased craft may stray (degrees): no
/// nearer than the first, easing toward the second; the turn that keeps it
/// there moves in about [`FRAME_SECONDS`], never faster than [`FRAME_RATE`]
/// (radians a second).
const FRAME_DEGREES: [f32; 2] = [28.0, 36.0];
const FRAME_SECONDS: f32 = 0.15;
const FRAME_RATE: f32 = 4.0;

/// What the remastered camera keeps between frames.
#[derive(Debug, Default)]
pub struct CameraClearance {
    /// The lift drawn last (radians above the game's own elevation) and its
    /// speed, the lift it heads for, the seconds it holds that once no
    /// longer needed, and the seconds the game's view has been blocked.
    lift: f32,
    lift_speed: f32,
    lift_target: f32,
    hold: f32,
    blocked: f32,
    /// The speed kick drawn last (degrees across the view), its speed, and
    /// the animation clock it last stepped at (microseconds).
    kick: f32,
    kick_speed: f32,
    kick_clock: Option<u64>,
    /// A boost ran last frame; the burst's seconds left; the rumble (0 to
    /// its share); the craft's speed along its hull last frame; the shake's
    /// own clock (seconds).
    boosting: bool,
    burst: f32,
    rumble: f32,
    forward: Option<f32>,
    shake_time: f32,
    /// How far the eye has gone from the game's after the craft under the
    /// water (world units), and how fast (units a second).
    dive: [f32; 3],
    dive_speed: [f32; 3],
    /// The leash's pull on the game's eye and on its look (world units),
    /// how fast each moves (units a second), and the reach each holds while
    /// the game's lag stays past it (none: 0).
    leash: [[f32; 3]; 2],
    leash_speed: [[f32; 3]; 2],
    leash_held: [f32; 2],
    /// The turn toward the craft that keeps it in the view (radians), and
    /// how fast it moves.
    frame_turn: f32,
    frame_turn_speed: f32,
    /// The craft followed and the game's eye last frame: a new craft or a
    /// cut starts afresh.
    vehicle: Option<ObjectId>,
    game_eye: Option<[i32; 3]>,
    /// The animation clock last frame (microseconds), for the easing.
    clock: Option<u64>,
    /// A failed test, logged once.
    logged: bool,
}

/// The owners the tests borrow beside the world: the model process, the
/// borrowed models' residency, the camera's own furniture query state and
/// what it keeps between frames.
pub struct CameraClearanceLoan<'a> {
    pub model_process: &'a ModelProcessOwners,
    pub model_fallback: Option<&'a ResidentRuntime>,
    pub query: &'a mut ProcessFurnitureQueryState,
    pub state: &'a mut CameraClearance,
}

/// The world the tests read.
pub(crate) struct CameraClearanceWorld<'a> {
    pub arena: &'a ObjectArena,
    pub camera: ObjectId,
    pub residency: &'a ResidentRuntime,
    pub source: &'a WorldTerrainSource,
    pub terrain: &'a TerrainRuntime,
    pub furniture: &'a FurnitureDrawState,
    pub animation: &'a dyn AnimationClockSource,
    pub executable: inf_formats::FrontendExecutable<'a>,
    /// Retail's own camera shake runs (process DWORD `005483B0`): its pose
    /// is drawn as the game makes it.
    pub shaking: bool,
    /// The water's live waves: the animation clock the drawn surface takes,
    /// the world's flags (`0x40` waves) and the sine table.
    pub wave_clock: u32,
    pub world_flags: u32,
    pub sine: &'a [u16; 4096],
}

/// The water's live surface at `xz`, as the world draws it (`00410F60` at
/// the animation clock; retail's camera clamp reads the waves at clock 0);
/// none in a world without water (its plane at -4096 or below).
pub(crate) fn water_surface(
    terrain: &TerrainRuntime,
    xz: [u16; 2],
    clock: u32,
    world_flags: u32,
    sine: &[u16; 4096],
) -> Option<i32> {
    if terrain.plane_base() >> terrain.plane_shift() <= -4096 {
        return None;
    }
    let request = crate::terrain_runtime::TerrainWaterSurfaceRequest {
        xz,
        wave_clock: clock,
        world_flags,
        terrain_height: terrain.height_at(xz),
    };
    Some(i32::from(terrain.water_surface_height(request, sine)))
}

/// The view to draw: the game's, lifted over what blocks it (in first
/// person, rolled less), and shaken while a boost kicks, the skimmer
/// pushes on or the buggy gains speed or boosts.
pub(crate) fn view(
    loan: CameraClearanceLoan<'_>,
    world: CameraClearanceWorld<'_>,
    view: CameraDrawRequest,
) -> CameraDrawRequest {
    let state = loan.state;
    let now = world.animation.animation_micros();
    let seconds = state
        .clock
        .map_or(0.0, |then| now.saturating_sub(then) as f32 / 1.0e6)
        .min(0.25);
    state.clock = Some(now);
    // Chase (mode 3) and first person (2) follow the craft. The view of a
    // lost craft (0) and the dock's (1) let the remaster go, its lift and
    // dive easing back onto the game's camera (a
    // craft dying under water zooms out above water, then continues on the
    // regular death camera); the special
    // views (4) start afresh.
    let mode = world
        .arena
        .selector16(world.camera)
        .map_or(0, |camera| camera.mode_188);
    let following = matches!(mode, 2 | 3);
    let chased = chase_anchor(world.arena, world.camera);
    let cut = mode == 4
        || state
            .game_eye
            .is_some_and(|last| is_cut(last, view.eye, seconds));
    state.game_eye = Some(view.eye);
    if chased.map(|(_, vehicle)| vehicle) != state.vehicle || cut {
        *state = CameraClearance {
            vehicle: chased.map(|(_, vehicle)| vehicle),
            game_eye: state.game_eye,
            clock: state.clock,
            kick: state.kick,
            kick_speed: state.kick_speed,
            kick_clock: state.kick_clock,
            logged: state.logged,
            ..CameraClearance::default()
        };
    }
    let Some((anchor, vehicle)) = chased else {
        return view;
    };
    let trembling = shake(
        state,
        drive(world.arena),
        world.shaking || !following,
        seconds,
    );
    let view = leashed(state, &world, view, vehicle, mode, seconds);
    let view = submerged(state, &world, view, anchor, following, seconds);
    let first_person = mode == 2;
    let eye = view.eye.map(|value| value as u16);
    let offset: [f32; 3] =
        std::array::from_fn(|axis| f32::from(eye[axis].wrapping_sub(anchor[axis]) as i16));
    let distance = length(offset);
    if first_person || distance < CHASE_DISTANCE || offset[0].hypot(offset[2]) < 1.0 {
        settle_lift(state);
        if first_person {
            return first_person_view(view, trembling);
        }
        return view;
    }
    let basis = view.basis;
    let flat = offset[0].hypot(offset[2]);
    let bearing = [offset[0] / flat, offset[2] / flat];
    let elevation = offset[1].atan2(flat);

    // The lift.
    let needed = if following {
        let mut sight = WorldSightQuery {
            residency: world.residency,
            model_fallback: loan.model_fallback,
            source: world.source,
            terrain: world.terrain,
            model_process: loan.model_process,
            process: &mut *loan.query,
            furniture: world.furniture,
        };
        let actors = SightActors {
            arena: world.arena,
            animation: world.animation,
            executable: world.executable,
        };
        let mut blocked = |raise: f32| -> Result<bool, String> {
            let target = point(anchor, bearing, elevation + raise, distance);
            let found = sight
                .sight(actors, anchor, target)
                .map_err(|error| format!("sight test: {error}"))?;
            Ok(match found.hit {
                None => false,
                // The craft itself, should a ray start inside its model.
                Some(SightHit::Actor { actor, .. }) if actor == vehicle => false,
                Some(_) => f32::from(found.length) < distance,
            })
        };
        match clear_raise(elevation, &mut blocked) {
            Ok(needed) => needed,
            Err(error) => {
                if !state.logged {
                    state.logged = true;
                    eprintln!(
                        "Remastered camera: the {error} stopped; the camera keeps retail's view"
                    );
                }
                return view;
            }
        }
    } else {
        // Letting go settles at once, without the hold.
        state.hold = 0.0;
        0.0
    };
    step_lift(state, needed, seconds);
    if state.lift < 1.0e-3 {
        return shaken(framed(state, view, anchor, mode, seconds), trembling);
    }
    let lifted = elevation + state.lift;
    let raised = point(anchor, bearing, lifted, distance);
    // Keep looking where the view looks: the point as far along it as the
    // craft lies from its eye. All relative to the craft.
    let forward = row(basis, 2);
    let along = -(offset[0] * forward[0] + offset[1] * forward[1] + offset[2] * forward[2]);
    let flat = distance * lifted.cos();
    let raised_offset = [
        bearing[0] * flat,
        distance * lifted.sin(),
        bearing[1] * flat,
    ];
    let look: [f32; 3] =
        std::array::from_fn(|axis| offset[axis] + forward[axis] * along - raised_offset[axis]);
    let mut aimed = basis;
    orient_from_forward(&mut aimed, normalized(look), row(basis, 1), false);
    aimed.words[0] = view.basis.words[0];
    let lifted = CameraDrawRequest {
        eye: raised.map(|word| i32::from(word as i16)),
        basis: aimed,
        coordinate_shift: view.coordinate_shift,
    };
    shaken(framed(state, lifted, anchor, mode, seconds), trembling)
}

/// The chase leash. Retail's slew (`0040F1C0`) moves the eye and the look
/// toward where the game wants them (behind the craft, and 400 ahead of it)
/// at most 5000 to 12000 units a second faster each second, so a craft
/// turning hard at speed (the skimmer round Oolbryte's pit at 3300 units a
/// second, turning 190 degrees a second) leaves both further behind each
/// frame until the view looks away from the craft. In the chase the drawn
/// eye and look are pulled back to [`LEASH_EYE`] and [`LEASH_LOOK`] (set by
/// the speed, [`LEASH_SPEEDS`]) of where the game wants them, at most
/// [`LEASH_SLACK`] times as far; within the leash the view stays the game's
/// own. Slowing down keeps the leash until the game's lag closes within it
/// ([`leash_pull`]). The pull moves as a spring of its own, as the game's targets jump (a crash shortens the eye's stand-off, a
/// target lock turns the look), and lets go the same way when the chase
/// ends (the death camera, the dock). Kept above the ground and the water as
/// retail's eye is.
fn leashed(
    state: &mut CameraClearance,
    world: &CameraClearanceWorld<'_>,
    view: CameraDrawRequest,
    vehicle: ObjectId,
    mode: u32,
    seconds: f32,
) -> CameraDrawRequest {
    if mode == 2 {
        // First person is its own view; entering it is a cut.
        state.leash = [[0.0; 3]; 2];
        state.leash_speed = [[0.0; 3]; 2];
        state.leash_held = [0.0; 2];
        state.frame_turn = 0.0;
        state.frame_turn_speed = 0.0;
        return view;
    }
    let camera = world.arena.selector16(world.camera).ok();
    let speed = world
        .arena
        .actor(vehicle)
        .ok()
        .map(|actor| length(actor.words_8e_92.map(|word| f32::from(word as i16))));
    let mut held = state.leash_held;
    let target = camera
        .zip(speed)
        .filter(|_| mode == 3)
        .and_then(|(camera, speed)| {
            let share = ramp(speed, LEASH_SPEEDS);
            let pull = |slewed: &CameraParameterState, radii: [f32; 2], held: &mut f32| {
                let (current, desired) = (slewed.current_position?, slewed.desired_position?);
                let lag = std::array::from_fn(|axis| {
                    f32::from(current[axis].wrapping_sub(desired[axis]) as i16)
                });
                Some(leash_pull(
                    lag,
                    radii[0] + (radii[1] - radii[0]) * share,
                    held,
                ))
            };
            let [eye_held, look_held] = &mut held;
            Some([
                pull(&camera.eye_138, LEASH_EYE, eye_held)?,
                pull(&camera.look_15c, LEASH_LOOK, look_held)?,
            ])
        });
    state.leash_held = if target.is_some() { held } else { [0.0; 2] };
    let target = target.unwrap_or_default();
    for (index, goal) in target.into_iter().enumerate() {
        for axis in 0..3 {
            state.leash[index][axis] = smooth_damp(
                state.leash[index][axis],
                goal[axis],
                &mut state.leash_speed[index][axis],
                LEASH_SECONDS,
                LEASH_RATE,
                seconds,
            );
        }
    }
    let [eye_pull, look_pull] = state.leash;
    if length(eye_pull) + length(look_pull) < 0.5 {
        return view;
    }
    let floor = |eye: [i32; 3]| {
        let xz = [eye[0] as u16, eye[2] as u16];
        let ground = i32::from(world.terrain.height_at(xz));
        water_surface(world.terrain, xz, 0, world.world_flags, world.sine)
            .map_or(ground, |water| water.max(ground))
    };
    let mut eye: [i32; 3] = std::array::from_fn(|axis| {
        i32::from(view.eye[axis].wrapping_add(eye_pull[axis].round() as i32) as i16)
    });
    // No nearer the ground than the game's own eye, up to retail's clearance
    // (retail clamps its eye at the ground under the desired eye, so where it
    // lags it may stand below the ground under itself, and is left there).
    let clearance = (view.eye[1] - floor(view.eye)).min(EYE_CLEARANCE);
    eye[1] = eye[1].max(floor(eye) + clearance);
    let moved: [f32; 3] = std::array::from_fn(|axis| {
        f32::from((eye[axis] as u16).wrapping_sub(view.eye[axis] as u16) as i16)
    });
    // The game's look point lies along its view (`00490370` turns the eye to
    // it), as far as the slewed look lies from its eye.
    let forward = row(view.basis, 2);
    let reach = camera
        .and_then(|camera| camera.look_15c.current_position)
        .map(|look| {
            length(std::array::from_fn(|axis| {
                f32::from(look[axis].wrapping_sub(view.eye[axis] as u16) as i16)
            }))
        })
        .filter(|&reach| reach >= 1.0)
        .unwrap_or(CHASE_DISTANCE);
    let look: [f32; 3] =
        std::array::from_fn(|axis| forward[axis] * reach + look_pull[axis] - moved[axis]);
    let mut basis = view.basis;
    orient_from_forward(&mut basis, normalized(look), row(view.basis, 1), false);
    basis.words[0] = view.basis.words[0];
    CameraDrawRequest { eye, basis, ..view }
}

/// The move that keeps `lag` on the leash: none within its reach; past it
/// the lag eases out toward [`LEASH_SLACK`] times as far, and never further.
/// The reach is `reach` (the speed's), or the shorter one `held` while the
/// lag has stayed past it: slowing down (a crash at speed) never lets the
/// view swing back out to a lag the game has not yet closed, and the leash
/// lets go as the game's lag comes within it.
fn leash_pull(lag: [f32; 3], reach: f32, held: &mut f32) -> [f32; 3] {
    let radius = if *held > 0.0 { reach.min(*held) } else { reach };
    let far = length(lag);
    if far <= radius {
        *held = 0.0;
        return [0.0; 3];
    }
    *held = radius;
    let slack = radius * (LEASH_SLACK - 1.0);
    let kept = radius + slack * ((far - radius) / slack).tanh();
    lag.map(|value| value * (kept / far - 1.0))
}

/// The chase view (mode 3) turned toward the craft at `anchor` as far as
/// keeps it within [`FRAME_DEGREES`] of the view's middle, so whatever the
/// game's camera does the craft stays in sight. Retail's own chase keeps it
/// within 25 degrees below 3000 units a second in a run of
/// 2026-10-28, where this never acted. The turn moves as a spring: where the
/// game hands first person to the chase under water, retail's chase starts
/// looking 50 degrees past the craft, and the view swings onto it. The
/// death camera and the dock let it go the same way.
fn framed(
    state: &mut CameraClearance,
    view: CameraDrawRequest,
    anchor: [u16; 3],
    mode: u32,
    seconds: f32,
) -> CameraDrawRequest {
    let forward = row(view.basis, 2);
    let toward = normalized(std::array::from_fn(|axis| {
        f32::from(anchor[axis].wrapping_sub(view.eye[axis] as u16) as i16)
    }));
    let angle = dot(forward, toward).clamp(-1.0, 1.0).acos();
    let [near, most] = FRAME_DEGREES.map(f32::to_radians);
    let needed = if mode == 3 && angle > near {
        angle - (near + (most - near) * ((angle - near) / (most - near)).tanh())
    } else {
        0.0
    };
    state.frame_turn = smooth_damp(
        state.frame_turn,
        needed,
        &mut state.frame_turn_speed,
        FRAME_SECONDS,
        FRAME_RATE,
        seconds,
    );
    let turn = state.frame_turn.min(angle);
    // Straight behind, no way round is nearer than another.
    if turn < 1.0e-4 || angle > std::f32::consts::PI - 1.0e-3 {
        return view;
    }
    let (sin, cos) = turn.sin_cos();
    let across = normalized(std::array::from_fn(|axis| {
        toward[axis] - forward[axis] * angle.cos()
    }));
    let turned = std::array::from_fn(|axis| forward[axis] * cos + across[axis] * sin);
    let mut basis = view.basis;
    orient_from_forward(&mut basis, turned, row(view.basis, 1), false);
    basis.words[0] = view.basis.words[0];
    CameraDrawRequest { basis, ..view }
}

/// Whether the game's eye moved from `last` to `eye` in one jump over a frame
/// of `seconds`: a cut (a new view, a respawn), not the camera's own motion.
fn is_cut(last: [i32; 3], eye: [i32; 3], seconds: f32) -> bool {
    let most = CUT_UNITS + (CUT_SPEED * seconds) as i32;
    (0..3).any(|axis| i32::from((eye[axis] as i16).wrapping_sub(last[axis] as i16)).abs() > most)
}

/// One frame of the lift toward `needed` (radians): it answers only a view
/// blocked for [`BLOCKED_SECONDS`], holds its height [`HOLD_SECONDS`] once
/// that is no longer needed, and moves within [`LIFT_SPEED`].
fn step_lift(state: &mut CameraClearance, needed: f32, seconds: f32) {
    state.blocked = if needed > 0.0 {
        state.blocked + seconds
    } else {
        0.0
    };
    let wanted = if state.blocked >= BLOCKED_SECONDS {
        needed
    } else {
        0.0
    };
    if wanted >= state.lift_target {
        state.lift_target = wanted;
        state.hold = HOLD_SECONDS;
    } else {
        state.hold -= seconds;
        if state.hold <= 0.0 {
            state.lift_target = wanted;
        }
    }
    let smooth = if state.lift_target > state.lift {
        RISE_SECONDS
    } else {
        SETTLE_SECONDS
    };
    state.lift = smooth_damp(
        state.lift,
        state.lift_target,
        &mut state.lift_speed,
        smooth,
        LIFT_SPEED,
        seconds,
    );
}

/// No lift outside the chase.
fn settle_lift(state: &mut CameraClearance) {
    state.lift = 0.0;
    state.lift_speed = 0.0;
    state.lift_target = 0.0;
    state.hold = 0.0;
    state.blocked = 0.0;
}

/// Under water: retail's clamp (`0041063A..0041071D`) keeps the eye above
/// the ground and the water, so a sinking craft leaves the camera at the
/// surface. While the craft it `follow`s lies below the live surface, the
/// eye goes down to the height the game wants (`eye_138`'s desired eye,
/// clipped to the ground but not to the water), kept [`SEABED_GAP`] above
/// the ground and [`SURFACE_BAND`] below the waves; where that leaves no
/// room it comes in toward the craft, where a trench runs deeper
/// ([`DIVE_REACH`]), and where none has room the game's eye stands. It
/// passes the band round the surface in one step either way, and keeps
/// looking where the game's view did.
fn submerged(
    state: &mut CameraClearance,
    world: &CameraClearanceWorld<'_>,
    view: CameraDrawRequest,
    anchor: [u16; 3],
    follow: bool,
    seconds: f32,
) -> CameraDrawRequest {
    let surface = |xz: [u16; 2]| {
        water_surface(
            world.terrain,
            xz,
            world.wave_clock,
            world.world_flags,
            world.sine,
        )
    };
    let game = view.eye;
    let craft_y = i32::from(anchor[1] as i16);
    let target = (|| {
        if !follow || craft_y >= surface([anchor[0], anchor[2]])? - SURFACE_BAND {
            return None;
        }
        let desired = world
            .arena
            .selector16(world.camera)
            .ok()?
            .eye_138
            .desired_position?;
        let rise = i32::from(desired[1] as i16) - craft_y;
        let away: [f32; 2] =
            [0, 2].map(|axis| f32::from((game[axis] as u16).wrapping_sub(anchor[axis]) as i16));
        DIVE_REACH.iter().find_map(|&share| {
            let xz = [0, 1].map(|index| {
                anchor[index * 2].wrapping_add((away[index] * share).round() as i32 as u16)
            });
            let floor = i32::from(world.terrain.height_at(xz)) + SEABED_GAP;
            let ceiling = surface(xz)? - SURFACE_BAND;
            if floor > ceiling {
                return None;
            }
            let y = (craft_y + (rise as f32 * share).round() as i32).clamp(floor, ceiling);
            Some({
                [
                    xz[0].wrapping_sub(game[0] as u16) as i16,
                    (y - game[1]) as i16,
                    xz[1].wrapping_sub(game[2] as u16) as i16,
                ]
                .map(f32::from)
            })
        })
    })()
    // Only ever down: the game's own eye stands above.
    .filter(|offset| offset[1] < 0.0)
    .unwrap_or([0.0; 3]);
    let downward = target[1] < state.dive[1];
    for axis in 0..3 {
        state.dive[axis] = smooth_damp(
            state.dive[axis],
            target[axis],
            &mut state.dive_speed[axis],
            DIVE_SECONDS,
            DIVE_SPEED,
            seconds,
        );
    }
    let mut eye: [i32; 3] = std::array::from_fn(|axis| {
        i32::from((game[axis] as u16).wrapping_add(state.dive[axis].round() as i32 as u16) as i16)
    });
    if state.dive[1] < -0.5 {
        if let Some(here) = surface([eye[0] as u16, eye[2] as u16]) {
            if (eye[1] - here).abs() < SURFACE_BAND {
                if downward {
                    eye[1] = here - SURFACE_BAND;
                    state.dive[1] = (eye[1] - game[1]) as f32;
                } else if game[1] < here + SURFACE_BAND {
                    state.dive = [0.0; 3];
                    state.dive_speed = [0.0; 3];
                    return view;
                } else {
                    // Up past the band, short of a game's eye above it (the
                    // view of a lost craft rising away): the rest eases.
                    eye[1] = here + SURFACE_BAND;
                    state.dive[1] = (eye[1] - game[1]) as f32;
                }
            }
        }
    }
    if eye == game {
        return view;
    }
    // Keep looking where the game's view did, at the craft's depth along
    // it (first person, at the craft itself, keeps its own aim).
    let forward = row(view.basis, 2);
    let to_craft: [f32; 3] =
        std::array::from_fn(|axis| f32::from(anchor[axis].wrapping_sub(game[axis] as u16) as i16));
    let along = dot(to_craft, forward);
    let mut basis = view.basis;
    if length(to_craft) >= CHASE_DISTANCE && along > 0.0 {
        let look: [f32; 3] = std::array::from_fn(|axis| {
            let moved = f32::from((eye[axis] as u16).wrapping_sub(game[axis] as u16) as i16);
            forward[axis] * along - moved
        });
        orient_from_forward(&mut basis, normalized(look), row(view.basis, 1), false);
        basis.words[0] = view.basis.words[0];
    }
    CameraDrawRequest { eye, basis, ..view }
}

/// The chase view moved by the shake, a few world units at its strongest.
fn shaken(view: CameraDrawRequest, trembling: [f32; 3]) -> CameraDrawRequest {
    if trembling == [0.0; 3] {
        return view;
    }
    CameraDrawRequest {
        eye: std::array::from_fn(|axis| {
            let moved = (trembling[axis] * SHAKE_UNITS).round() as i32;
            i32::from(view.eye[axis].wrapping_add(moved) as i16)
        }),
        ..view
    }
}

/// First person: the game's view rolled [`FIRST_PERSON_ROLL_SHARE`] as far,
/// its heading and pitch kept, turned a fraction of a degree by the shake.
fn first_person_view(view: CameraDrawRequest, trembling: [f32; 3]) -> CameraDrawRequest {
    let forward = row(view.basis, 2);
    let Some((right, up)) = level_frame(forward) else {
        return view;
    };
    let given_up = row(view.basis, 1);
    let roll = dot(given_up, right).atan2(dot(given_up, up)) * FIRST_PERSON_ROLL_SHARE;
    let jitter = trembling.map(|value| (value * SHAKE_DEGREES).to_radians());
    let heading = forward[0].atan2(forward[2]) + jitter[0];
    let pitch = forward[1].clamp(-1.0, 1.0).asin() + jitter[1];
    let turned = [
        pitch.cos() * heading.sin(),
        pitch.sin(),
        pitch.cos() * heading.cos(),
    ];
    let Some((right, up)) = level_frame(turned) else {
        return view;
    };
    let (sin, cos) = roll.sin_cos();
    let rolled: [f32; 3] = std::array::from_fn(|axis| up[axis] * cos + right[axis] * sin);
    let mut basis = view.basis;
    orient_from_forward(&mut basis, turned, rolled, false);
    basis.words[0] = view.basis.words[0];
    CameraDrawRequest { basis, ..view }
}

/// The level right and up beside `forward` (right = up x forward, as
/// [`orient_from_forward`] builds them); none looking straight up or down.
fn level_frame(forward: [f32; 3]) -> Option<([f32; 3], [f32; 3])> {
    let flat = forward[0].hypot(forward[2]);
    if flat < 1.0e-4 {
        return None;
    }
    let right = [forward[2] / flat, 0.0, -forward[0] / flat];
    let up = [
        forward[1] * right[2] - forward[2] * right[1],
        forward[2] * right[0] - forward[0] * right[2],
        forward[0] * right[1] - forward[1] * right[0],
    ];
    Some((right, up))
}

/// The remastered view's speed kick: the scale on the game's slewed focals
/// (`world_scene::view_focals`) that widens the view drawn with the first
/// player's speed and while a boost runs, easing in and out; 1 in special
/// views. It applies past the slew, whose `0x50` step answers any drop in
/// its source at all.
pub fn speed_kick(
    state: &mut CameraClearance,
    arena: &ObjectArena,
    now: u64,
    special_view: bool,
) -> f32 {
    let seconds = state
        .kick_clock
        .map_or(0.0, |then| now.saturating_sub(then) as f32 / 1.0e6)
        .min(0.25);
    state.kick_clock = Some(now);
    if special_view {
        state.kick = 0.0;
        state.kick_speed = 0.0;
        return 1.0;
    }
    kick_scale(state, drive(arena).map_or(0.0, kick_target), seconds)
}

/// The kick eased toward `target` (degrees across the view) over `seconds`,
/// as the scale on the remastered view's focals.
fn kick_scale(state: &mut CameraClearance, target: f32, seconds: f32) -> f32 {
    let smooth = if target > state.kick {
        KICK_WIDEN_SECONDS
    } else {
        KICK_NARROW_SECONDS
    };
    state.kick = smooth_damp(
        state.kick,
        target,
        &mut state.kick_speed,
        smooth,
        KICK_SPEED,
        seconds,
    );
    let half = |degrees: f32| (degrees / 2.0).to_radians().tan();
    half(REMASTERED_DEGREES) / half(REMASTERED_DEGREES + state.kick)
}

/// The kick `drive` asks for, degrees across the view.
fn kick_target(drive: Drive) -> f32 {
    KICK_SPEED_DEGREES * ramp(drive.forward, KICK_SPEEDS)
        + if drive.boost { KICK_BOOST_DEGREES } else { 0.0 }
}

/// `focal` (retail's 90 degrees across 4:3) seeing `degrees` across instead.
fn widened(focal: [i32; 2], degrees: f32) -> [i32; 2] {
    let scale = 45f32.to_radians().tan() / (degrees / 2.0).to_radians().tan();
    focal.map(|value| (value as f32 * scale).round() as i32)
}

/// What the camera reads of the first player's craft.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Drive {
    /// Speed along the hull's forward axis (world units a second, about:
    /// `+8E..+92` move `v * delta / 2^20` a tick).
    forward: f32,
    /// A turbo boost runs (the vehicle's `+224` timer), not a flip.
    boost: bool,
    /// Pushed on: Thrust (Space) or Climb (Up, the wheeled forms' throttle)
    /// held.
    pushed: bool,
    /// The craft's form (`+1E8`): 1 the buggy, 2 the truck, 3 the hover, 4
    /// the VTOL, 5 the skimmer.
    form: u32,
}

fn drive(arena: &ObjectArena) -> Option<Drive> {
    let (vehicle, controls) = arena.first_player_drive()?;
    let actor = arena.actor(vehicle).ok()?;
    let craft = arena.selector2_actor(vehicle).ok()?;
    let velocity = actor.words_8e_92.map(|word| f32::from(word as i16));
    Some(Drive {
        forward: dot(velocity, row(actor.rotating.basis, 2)),
        boost: craft.dwords_220_224[1] != 0 && craft.bytes_20a_20c[2] == 0,
        pushed: controls.thrust_1c > 0 || controls.pitch_18 < 0 || controls.analog_forward(),
        form: craft.dwords_1e8_1f4[0],
    })
}

/// This frame's shake, each axis within -1..1 times the form's strength
/// (none while retail's own shake runs): a burst as a boost begins, fading
/// over [`BURST_SECONDS`], over a slower rumble while the skimmer is pushed
/// on, and while the buggy gains speed or boosts.
fn shake(
    state: &mut CameraClearance,
    drive: Option<Drive>,
    shaking: bool,
    seconds: f32,
) -> [f32; 3] {
    let boosting = drive.is_some_and(|drive| drive.boost);
    if boosting && !state.boosting {
        state.burst = BURST_SECONDS;
    }
    state.boosting = boosting;
    state.burst = (state.burst - seconds).max(0.0);
    let forward = drive.map(|drive| drive.forward);
    let gain = match (state.forward, forward) {
        (Some(last), Some(now)) if seconds > 0.0 => (now - last) / seconds,
        _ => 0.0,
    };
    state.forward = forward;
    let pushed = drive.is_some_and(|drive| drive.pushed);
    let (rumble, strength) = match drive.map(|drive| drive.form) {
        Some(1) => {
            let gaining = if pushed { ramp(gain, BUGGY_GAINS) } else { 0.0 };
            let boost = if boosting { BUGGY_BOOST_SHARE } else { 0.0 };
            ((BUGGY_GAIN_SHARE * gaining).max(boost), BUGGY_STRENGTH)
        }
        Some(5) => (if pushed { SKIM_SHARE } else { 0.0 }, SKIMMER_STRENGTH),
        _ => (0.0, 1.0),
    };
    state.rumble = ease(state.rumble, rumble, seconds, RUMBLE_SECONDS);
    state.shake_time = (state.shake_time + seconds) % 4096.0;
    let burst = (state.burst / BURST_SECONDS).powi(2);
    if shaking || burst + state.rumble < 1.0e-3 {
        return [0.0; 3];
    }
    let time = state.shake_time;
    std::array::from_fn(|axis| {
        let axis = axis as u32;
        let rumble = state.rumble * noise(axis + 3, time * RUMBLE_HERTZ);
        strength * (burst * noise(axis, time * BURST_HERTZ) + rumble).clamp(-1.0, 1.0)
    })
}

/// Smooth value noise in -1..1 along `time`, one stream per `seed`.
fn noise(seed: u32, time: f32) -> f32 {
    let step = time.floor();
    let along = time - step;
    let smooth = along * along * (3.0 - 2.0 * along);
    let at = |index: f32| {
        let mut word =
            (index as i32 as u32).wrapping_mul(0x9e37_79b1) ^ seed.wrapping_mul(0x85eb_ca6b);
        word ^= word >> 15;
        word = word.wrapping_mul(0x2c1b_3c6d);
        word ^= word >> 12;
        word as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    at(step) + (at(step + 1.0) - at(step)) * smooth
}

/// How far `value` lies between `range`'s two ends, 0 to 1.
fn ramp(value: f32, range: [f32; 2]) -> f32 {
    ((value - range[0]) / (range[1] - range[0])).clamp(0.0, 1.0)
}

/// `current` eased toward `target` over `seconds`, closing about two thirds
/// of the gap each `seconds_to`.
fn ease(current: f32, target: f32, seconds: f32, seconds_to: f32) -> f32 {
    current + (target - current) * (1.0 - (-seconds / seconds_to).exp())
}

/// `current` moved toward `target` over `seconds` as a critically damped
/// spring reaching it in about `smooth` seconds, never faster than
/// `max_speed` a second and never past it; `speed` carries between frames
/// (Game Programming Gems 4, 1.10), so a move starts and ends gently.
fn smooth_damp(
    current: f32,
    target: f32,
    speed: &mut f32,
    smooth: f32,
    max_speed: f32,
    seconds: f32,
) -> f32 {
    if seconds <= 0.0 {
        return current;
    }
    let omega = 2.0 / smooth;
    let x = omega * seconds;
    let decay = 1.0 / (1.0 + x + 0.48 * x * x + 0.235 * x * x * x);
    let reach = max_speed * smooth;
    let change = (current - target).clamp(-reach, reach);
    let goal = current - change;
    let pull = (*speed + omega * change) * seconds;
    *speed = (*speed - omega * pull) * decay;
    let next = goal + (change + pull) * decay;
    if (target - current > 0.0) == (next > target) {
        *speed = 0.0;
        return target;
    }
    next
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The lowest raise of the elevation, from `elevation`, whose view `blocked`
/// passes: none when the game's own is clear, else the first clear step,
/// halved toward the last blocked one; the highest when nothing clears.
fn clear_raise<E>(
    elevation: f32,
    blocked: &mut impl FnMut(f32) -> Result<bool, E>,
) -> Result<f32, E> {
    if !blocked(0.0)? {
        return Ok(0.0);
    }
    let highest = (HIGHEST - elevation).max(0.0);
    let mut low = 0.0;
    let mut high = None;
    let mut raise = STEP;
    while raise < highest + STEP {
        let raise_here = raise.min(highest);
        if !blocked(raise_here)? {
            high = Some(raise_here);
            break;
        }
        low = raise_here;
        raise += STEP;
    }
    let Some(mut high) = high else {
        return Ok(highest);
    };
    for _ in 0..REFINE {
        let middle = 0.5 * (low + high);
        if blocked(middle)? {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(high)
}

/// The followed craft and the point above it the rays start from, when the
/// camera chases a vehicle (`004218E0`'s craft); `None` otherwise.
fn chase_anchor(arena: &ObjectArena, camera: ObjectId) -> Option<([u16; 3], ObjectId)> {
    let reference = arena.selector16(camera).ok()?.target_18c?;
    let handle = arena.actor_follow_target(reference).ok()?.handle_04;
    let vehicle = arena.actor_list().iter().copied().find(|&actor| {
        arena.actor(actor).is_ok_and(|state| state.handle == handle)
            && arena.selector2_actor(actor).is_ok()
    })?;
    let position = arena.object(vehicle).ok()?.base.position_words?;
    Some((
        [
            position[0],
            position[1].wrapping_add(ANCHOR_RISE as u16),
            position[2],
        ],
        vehicle,
    ))
}

/// `anchor` plus `distance` along `bearing` (horizontal, unit) raised by
/// `elevation`, in world WORDs.
fn point(anchor: [u16; 3], bearing: [f32; 2], elevation: f32, distance: f32) -> [u16; 3] {
    let flat = distance * elevation.cos();
    let offset = [
        bearing[0] * flat,
        distance * elevation.sin(),
        bearing[1] * flat,
    ];
    std::array::from_fn(|axis| anchor[axis].wrapping_add(offset[axis].round() as i32 as u16))
}

/// The basis's `index` row (0 X, 1 Y, 2 Z: right, up, forward).
fn row(basis: NativeBasis, index: usize) -> [f32; 3] {
    std::array::from_fn(|axis| f32::from_bits(basis.words[1 + index * 3 + axis]))
}

fn length(vector: [f32; 3]) -> f32 {
    (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt()
}

fn normalized(vector: [f32; 3]) -> [f32; 3] {
    let length = length(vector).max(1.0e-6);
    vector.map(|value| value / length)
}

#[cfg(test)]
#[path = "camera_clearance_tests.rs"]
mod tests;
