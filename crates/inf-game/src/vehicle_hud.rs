//! The player vehicle's own HUD, `0041C320`, which the vehicle's virtual +84
//! (`0041ED40`) draws before its player's `0046BE00`. This module plans the
//! draw in native order from the vehicle, player, world and frame inputs;
//! [`crate::player_hud`] executes the plan on the software renderer.
//!
//! Infest01 runs the HUD 717 times over Level 1, always in the third-person
//! view (layout 0) and with the vehicle in the draw list, so the cockpit
//! prefix `0041C4D7..0041C92E` never runs. Its instruction coverage bounds
//! what is planned here:
//!
//! - The reticle (`0041C92E..0041D065`), unless a menu is shown: with the
//!   "HUD" setting and an actor or furniture target +148 (types 2 and 4),
//!   the red box, three trailing corners and a bracket closing on the
//!   target, with an actor's health bar and caption; otherwise the green
//!   three-ring sight along the fire direction `0041FF20`, after a bracket
//!   on a point target. See [`reticle`].
//! - The carried units (`0041D59A..0041D8F5`), each in its hold slot at the
//!   bottom left, spinning, drawn by its own draw `+2C`.
//! - The health gauge (`0047ED20` at 203/256 W, 200/256 H) with the mode icon,
//!   sprite `0x2AC` + mode tinted red to green, blinking under 37.5%.
//! - Both weapon icons: the model (`0049FEA0`) at 1000 in front of an
//!   identity view centred on the icon, a frame gauge and the ammo count
//!   (the primary's only between 0 and 9). The models draw at the flush
//!   `004A4460` that follows them. While a weapon key is held its icon gives
//!   way to the switch panel `0041A740`: the row's eight weapons on a ring
//!   that grows out of the icon towards the screen's centre, with the
//!   selected weapon's name and the owned weapons' counts.
//! - The radar `00473490`: its frame, then its contents `004723F0`: the
//!   process RNG draw, the actors' blips while the player owns the Scanner
//!   (grant 0x11, [`crate::radar_blips`]) and the map annotations' numbered
//!   labels; then the bare frame beside the counters. With the Radar
//!   (grant 0x12) the terrain radar `00470170` ([`crate::terrain_radar`])
//!   replaces the first radar and its frame, and its contents always walk
//!   the actors, unless the map's jitter has reached 0x4000.
//! - The three counter gauges of player +120/+124/+128 (out of 30, eased by
//!   the quarter sine) and the turbo gauge (+12C out of 3), the milestone
//!   counter "+11C/total", the compass ring (`0041BF60`) around the radar,
//!   the mission clock and the deep-water notice (+20A).
//!
//! Not planned, reported as an omission when reached: the first-person
//! layouts (any layout but 0, including the cockpit prefix). The radar's
//! list at `0055EF10` (save chunk 38) has no port owner and is never
//! populated: it starts empty (`004755E0`), and only the network's
//! territory (`00475610`) adds to it; it is empty throughout Infest01.
//!
//! Online (`0055F448`, [`HudNetwork`]) the radar is always the terrain
//! radar, the counter gauges give way to the score line (`0041E7A3`: the
//! team's captures, this slot's score, or the race's laps and place) and
//! the turbo gauge follows it; Powerball (mode 4) leaves out the sight, the
//! health gauge, the carried units, the secondary's count and the primary,
//! and shows the ball (model 0x271) as the secondary of the vehicle that
//! holds it.

use thiserror::Error;

use crate::{
    frontend_scene::SceneView,
    menu::MenuBasis,
    model_context::{ModelContext, ModelDispatch, ModelProjection},
    model_projection::ModelProjectionBackend,
    model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector},
    object_runtime::native_trunc_low32 as ftol,
    radar_blips::{RadarActor, RadarBlipPass},
    scene_sprite::{DirectSpriteMode, DirectSpriteQuad},
    terrain_radar::{terrain_heading_rotation, terrain_radar_origin},
};
use inf_formats::native_basis::{isqrt_48efc0, rotate_axis_vector, sine_q15};

/// `0047ED20`'s sprites: the small frame, the large frame's quarter and the
/// fill disc.
const GAUGE_SMALL_SPRITE: u16 = 0x2ac;
const GAUGE_QUARTER_SPRITE: u16 = 0x2a6;
const GAUGE_FILL_SPRITE: u16 = 0x2ab;
/// The mode icon is sprite 0x2AC plus the mode WORD +1E8.
const MODE_ICON_BASE: u16 = 0x2ac;
/// Every HUD quad vertex carries this depth DWORD.
const QUAD_DEPTH: u32 = 0x4000;
/// `0047F350`'s chevron sprite, and `0047F080`'s frame and bar sprites.
const BRACKET_SPRITE: u16 = 0x22b;
const BAR_FRAME_SPRITE: u16 = 0xe9;
const BAR_SPRITE: u16 = 0x22a;
/// The reticle's quads carry this vertex DWORD instead.
const RETICLE_DEPTH: u32 = 0x40;
/// The box's captions (H0): an actor of another side, and any other.
pub const HOSTILE_CAPTION: u16 = 0x155;
pub const CAPTION: u16 = 0x156;
/// `0041CD41..0041CD45`: `0046C760(0x21, 0)` on the player while the box
/// frames a hostile actor. The command's text, shown the first time only,
/// is "When your weapons have locked on to a target, the target triangle
/// goes red" (H0 0x2D5).
pub const HOSTILE_HINT: u32 = 0x21;
/// ReticleCornerResidueApproximation: the vehicle never writes the
/// reticle's corners +5B4..+5BF before the reticle eases them, so retail's
/// first values are heap residue (Infest01's and Infest03's hold leftover
/// text, Infest02's and Infest04's zeros). The port starts them at the zero
/// a fresh heap holds; only the brackets' first glide depends on it.
pub const RETICLE_CORNER_RESIDUE_APPROXIMATION: [[u16; 2]; 3] = [[0; 2]; 3];
/// The weapon icon models by weapon index, a stack table at `0041D902`.
pub const WEAPON_ICON_MODELS: [i16; 16] = [
    0xaf, 0xad, 0xac, 0xae, 0xb2, 0xb1, 0xb0, 0x40, 0xb4, 0xb3, 0xb6, 0xb7, 0xb9, 0xb8, 0xb5, 0xba,
];
/// `0041A740`'s weapon names (H0), by weapon index: the primary row's
/// Cannons..Cranial Implanter, then the secondary row's Grenade..Napalm.
/// Online, with settings `+10` 0 and `+18` 2, index 2 reads 0x2FF instead.
pub const WEAPON_NAME_STRINGS: [u16; 16] = [
    0x14c, 0x14d, 0x14e, 0x14f, 0x150, 0x151, 0x152, 0x153, 0x144, 0x145, 0x146, 0x147, 0x148,
    0x149, 0x14a, 0x14b,
];
/// The upgrade each weapon needs (`005305B0`); ID 0 is always owned.
pub const WEAPON_UPGRADES: [u16; 16] = [
    0x32, 0x9b, 0x98, 0xa1, 0xa0, 0xa3, 0x00, 0x33, 0x00, 0xa9, 0xa8, 0xaa, 0xab, 0xac, 0xad, 0xae,
];
/// `0041A740`'s model distances: the selected weapon, an owned one and one
/// the player lacks.
const PANEL_SELECTED_DISTANCE: i32 = 500;
const PANEL_OWNED_DISTANCE: i32 = 1000;
const PANEL_LACKING_DISTANCE: i32 = 5000;
/// `005305E8`, "AI": the count sits this string's width below its name.
pub const PANEL_COUNT_SPACER: &[u8] = b"AI";
/// The notices: H0 strings 0x135 (+20A, deep water) and 0x16D (+20B).
pub const NOTICE_STRINGS: [u16; 2] = [0x135, 0x16d];
/// H6 style slots the HUD reads: the frames, the turbo and charge gauges
/// and the three counters.
pub const STYLE_FRAME: i32 = 1;
pub const STYLE_TURBO: i32 = 0x21;
/// The target bar's frame and fill.
pub const STYLE_BAR: i32 = 0;
pub const STYLE_COUNTERS: [i32; 3] = [0x22, 0x23, 0x24];
/// `0041D2BB`: the weapon models' light, (128, 64, -128) / sqrt(36864),
/// white, with a quarter-white ambient.
pub const WEAPON_LIGHT_DIRECTION: [f32; 3] = [128.0 / 192.0, 64.0 / 192.0, -128.0 / 192.0];
pub const WEAPON_LIGHT_RGB: [u8; 3] = [0xff; 3];
pub const WEAPON_AMBIENT: [i32; 3] = [0x3f; 3];
/// `0041DC2A`: the weapon models sit 1000 in front of the eye. X and Y are
/// the zeroes `0043A150` stores when `0041C3E8` constructs the descriptor.
pub const WEAPON_MODEL_POSITION: [i32; 3] = [0, 0, 1000];

/// What the frame supplies.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HudFrame {
    /// Surface +0/+4 (`00495900`/`00495910`), in pixels.
    pub surface: [i32; 2],
    /// `00546F20/24`, which the HUD stores at the view record's +38/+3C.
    pub focal: [i32; 2],
    /// `0041ED40`'s third argument: 0 unless the camera is in its first
    /// person mode (+188 = 2), when it is the "First Person" setting.
    pub layout: u8,
    /// The animation clock `005471C0`.
    pub clock: i32,
    /// The "HUD" setting, settings +60.
    pub hud_setting: u32,
    /// H4 font 0's +14 / +10, divided by four (`0041C1EF..0041C1FF`).
    pub font_quarter_height: i32,
    /// The view record's basis vectors X, Y, Z (+14..+34).
    pub view_basis: [[f32; 3]; 3],
    pub sine_quarter: [u16; 4096],
    pub styles: HudStyles,
    /// H10 global slot 0's dimensions (`00471A80`/`00471AB0`), the radar's
    /// half width and height.
    pub radar_size: [i32; 2],
    /// The renderer's clip rectangle `+48..+4E` (`004A4C40`), which the
    /// terrain radar checks its corner against.
    pub viewport: [i16; 4],
    /// The world draw's published view and projection, which the reticle
    /// projects through.
    pub projection: HudProjection,
}

/// What `0048FB40` and the near local projection `004EB5A0` (the view
/// record's +50, slot +8) read when the reticle projects a point: the view
/// the world draw published and its projection globals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudProjection {
    pub view: SceneView,
    /// `0055F22C..0055F24C`.
    pub projection: ModelProjection,
    /// `0055F250`.
    pub coordinate_shift: u8,
}

/// The vehicle's target +148 as the reticle reads it, after its refresh
/// (virtual +18).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudTarget {
    /// +10: 2 an actor, 4 furniture, 8 a point, 16 an object.
    pub kind: u32,
    /// The point virtual +1C returns (+14 for types 2 and 4, else +08),
    /// which the box projects; None where the target never wrote it.
    pub focus: Option<[u16; 3]>,
    /// Type 2's actor (virtual +10, `0047AD90` of +04), when it resolves and
    /// is not the vehicle itself (`0041CB89`).
    pub actor: Option<HudTargetActor>,
}

/// The fields of a framed actor that the health bar and caption read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudTargetActor {
    pub health: u16,
    /// +A2, which the bar reads only with health left.
    pub maximum_health: Option<u16>,
    /// +B4, its side.
    pub team: u32,
}

/// The H6 style words of [`STYLE_FRAME`], [`STYLE_TURBO`] and
/// [`STYLE_COUNTERS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudStyles {
    pub frame: u32,
    pub turbo: u32,
    /// Style [`STYLE_BAR`].
    pub bar: u32,
    pub counters: [u32; 3],
}

/// The vehicle's fields the HUD reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudVehicle {
    /// +2C bit 1: the vehicle is drawn (the third-person view).
    pub drawn: bool,
    /// +A0 and +A2.
    pub health: u16,
    pub maximum_health: u16,
    /// The low WORD of +1E8.
    pub mode: u16,
    /// The target +148, read only when the reticle reaches it (the HUD
    /// setting on and no menu shown).
    pub target: Option<HudTarget>,
    /// The reticle's three corners +5B4..+5BF, each an X and Y WORD.
    pub corners: [[u16; 2]; 3],
    /// Position WORDs +32..+36, which the sight's rings start from.
    pub position: [u16; 3],
    /// The basis rows X, Y and Z (its +48) as binary32 bits, which turn the
    /// radar in a first-person layout.
    pub basis_bits: [[u32; 3]; 3],
    /// +B4, its side.
    pub team: u32,
    /// `0041FF20`'s fire direction, taken when the sight draws; under the
    /// special view it writes none.
    pub sight_direction: Option<[i16; 3]>,
    /// The hold: each of the three +1A4 slots that resolves to an actor
    /// (`00412780`).
    pub carried: [Option<HudCarriedActor>; 3],
    pub dword_218: u32,
    /// +534/+535: the primary and secondary weapon switch panels.
    pub switching: [u8; 2],
    /// +52C/+530: each panel's timer in milliseconds, which `0041A740`
    /// compares signed.
    pub switch_timers: [u32; 2],
    /// +540/+544: each panel's slide, an angle added to its ring.
    pub switch_slides: [u32; 2],
    pub weapons: HudWeapons,
    /// +20A/+20B.
    pub notices: [u8; 2],
    pub dword_5c0: u32,
}

/// A carried actor in the hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudCarriedActor {
    pub actor: crate::object_runtime::ObjectId,
    /// Its size WORD +28.
    pub size_28: u16,
}

/// The behavior object's (+140) weapon selection and counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudWeapons {
    /// +10 and +11, signed BYTEs.
    pub primary: i8,
    pub secondary: i8,
    /// Entry +0 (`entries_18`) of each selected weapon.
    pub primary_ammo: i32,
    pub secondary_ammo: i32,
    /// `0048D7D0` for the primary: -1 without a definition, else its timer
    /// percent of six seconds.
    pub primary_charge: i32,
    /// Entry +0 of each of the sixteen weapons (+18), which the switch
    /// panels count.
    pub ammo: [i32; 16],
}

/// The player's fields the HUD reads (`0046B090` of the vehicle's +84).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudPlayer {
    /// The message's BYTE+0 at +88 (a menu is shown).
    pub message_shown: u8,
    /// WORD +11C, the milestones reached.
    pub milestones: u16,
    /// +120/+124/+128, each out of 30.
    pub counters: [u32; 3],
    /// +12C, the turbo charges out of 3.
    pub turbo: u32,
    /// Upgrades 0x11 (the Scanner: the radar's actor blips), 0x12 (the
    /// Radar: the terrain radar `00470170`) and 0x13 (the Resource scanner:
    /// the spawners' markers) through `0046BDC0`.
    pub radar_upgrades: [bool; 3],
    /// `0046BDC0` of each weapon's upgrade ([`WEAPON_UPGRADES`]).
    pub weapons_owned: [bool; 16],
}

/// A map annotation (selector 87) in the `0055F170` list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudAnnotation {
    /// Position WORDs X and Z.
    pub position_words: [u16; 2],
    /// DWORD +4C; zero has no radar label.
    pub label: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HudWorld {
    /// The H14 header's +24, the level's milestone total.
    pub milestone_total: u32,
    /// The mission clock G+1E4, in seconds.
    pub seconds: u32,
    /// The view record's eye X and Z WORDs (+4, +C).
    pub eye_words: [u16; 2],
    /// The map annotations in `0055F170` order.
    pub annotations: Vec<HudAnnotation>,
    /// The actor list `G+48`, for the radar's blips.
    pub radar_actors: Vec<RadarActor>,
    /// The map's jitter `+24`, which jams the radar.
    pub radar_jitter: i32,
    /// The network game's words, None offline.
    pub network: Option<HudNetwork>,
}

/// What the HUD reads online (`0055F448` set).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HudNetwork {
    /// Settings `+10`, the game's mode.
    pub mode: u32,
    /// Powerball's ball (`005468D0`) is held by this vehicle (its `+14C`
    /// the vehicle's `+84`).
    pub holds_ball: bool,
    pub score: HudScore,
}

/// `0041E7A3..0041E9DB`: the score line at (20/256 W, 88/100 H) for a
/// vehicle on a team from 2 to 17 (`+B4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudScore {
    /// A team outside 2..17: no line.
    None,
    /// Modes 2 and 4: this slot's score (`0054E79C`), "%02d".
    Own(u16),
    /// Mode 0: the team's words `+0C` and `+0E` (`0055F100`), "%d/%d".
    Team([u16; 2]),
    /// The race: the laps "%d/%d" (the laps to run while not racing) and,
    /// racing, the place (17 less the slot's score) 4% up and left.
    Race {
        laps: u32,
        total: u32,
        place: Option<i32>,
    },
    /// Mode 3 without a race: `0041E867` draws its text buffer unwritten.
    Unwritten,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HudInputs {
    pub frame: HudFrame,
    pub vehicle: HudVehicle,
    pub player: HudPlayer,
    pub world: HudWorld,
}

/// Where `00415650` text sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudTextPosition {
    At([i32; 2]),
    /// X minus half the text's signed WORD width (`004A4240`).
    CentredOn([i32; 2]),
    /// The surface width minus the text's signed WORD width, halved.
    ScreenCentred {
        y: i32,
    },
    /// Centred on X as [`Self::CentredOn`], and lower by the signed WORD
    /// width of `spacer` (`0041AF2D`).
    CentredUnder {
        point: [i32; 2],
        spacer: &'static [u8],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HudText {
    Bytes(Vec<u8>),
    /// An H0 string resource.
    String(u16),
}

/// What the HUD reaches but the port does not draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HudOmission {
    /// Layout 2 (`0041C4BF` forces it for an undrawn vehicle): the cockpit
    /// prefix `0041C4D7..0041C92E` and the cockpit placements. The HUD
    /// stops.
    FirstPerson { layout: u8 },
    /// [`HudScore::Unwritten`]: the line is left out.
    ScoreBuffer,
}

/// `00470170`'s inputs (`0041E39E..0041E3C3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainRadarOp {
    /// The radar's centre WORDs, as the ordinary radar's.
    pub centre: [i32; 2],
    /// The X and Z WORDs it centres on: the view record's eye, or in a
    /// first-person layout the vehicle's position.
    pub eye_words: [u16; 2],
    /// [`terrain_heading_rotation`] of the X axis of the view's basis, or
    /// in a first-person layout of the vehicle's.
    pub rotation: [i32; 2],
    /// The map's jitter `+24`.
    pub jitter: i32,
    pub viewport: [i16; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub enum HudOp {
    /// Renderer virtual +9C or +A4.
    Quad(DirectSpriteQuad),
    /// Renderer virtual +84, a flat quad of one colour.
    Flat(crate::scene_quad::DirectFlatQuad),
    /// `00415650`: H6 style 7 one pixel right and down, then style 0, each
    /// through `004A3AB0` with H4 font 0.
    Text {
        text: HudText,
        position: HudTextPosition,
    },
    /// The weapon models' light and ambient (`0041D2BB..0041D355`).
    WeaponLight,
    /// A carried actor (`0041D5E9..0041D8D5`): an identity view centred on
    /// `centre`, then the actor's own draw `+2C` lent the position (0, 0,
    /// `distance`) and the identity basis turned `basis_angle` in XZ.
    CarriedActor {
        actor: crate::object_runtime::ObjectId,
        centre: [i32; 2],
        distance: u16,
        basis_angle: i32,
    },
    /// A weapon model, queued until the flush: the icon's at 1000, a switch
    /// panel's at 500, 1000 or 5000 (descriptor `+0C`).
    WeaponModel {
        model: i16,
        centre: [i32; 2],
        basis_angle: i32,
        distance: i32,
    },
    /// A switch panel's text packet (`004A42B0`, callback `00423370`, which
    /// reaches renderer `+0C` and `004A3AB0`): one pass in H6 style 0 with
    /// H4 font 0, the text formatted as `sprintf`'s format. Its sort word,
    /// `0xC0000000` from font sprite 50's blended variants (0 without
    /// material sorting), sorts after every model face, so it draws at the
    /// flush after the models; equal words replay in the order queued, as
    /// Infest03's panels show.
    PanelText {
        text: HudText,
        position: HudTextPosition,
    },
    /// `004A4460`.
    Flush,
    /// `004723F0`'s walk: one process RNG draw (`00472409`), then, with the
    /// pass, the actors' blips ([`crate::radar_blips::radar_blips`]), which
    /// draw more while jammed.
    RadarContents(Option<RadarBlipPass>),
    /// The terrain radar's pixels ([`crate::terrain_radar::terrain_radar`]),
    /// written straight into the frame.
    TerrainRadar(TerrainRadarOp),
    /// A radar label: "%d" through `004A3AB0` in one pass, H4 font 0.
    RadarLabel {
        text: Vec<u8>,
        position: [i32; 2],
        color_word: u32,
    },
    /// The reticle countdown +5C0: halved under the box (`0041C9F7`),
    /// risen by 50000 up to 500000 otherwise (`0041CDB5`/`0041CE32`).
    SetReticle5c0(u32),
    /// The corners +5B4..+5BF after the box or the sight moved them.
    SetReticleCorners([[u16; 2]; 3]),
    /// `0046C760(id, 0)` on the vehicle's player.
    PlayerCommand(u32),
    /// A drawn notice clears its flag (`0041EC87`, `0041ED19`).
    ClearNotice(usize),
    Omission(HudOmission),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum VehicleHudError {
    #[error("native signed division {numerator}/{divisor} faults")]
    Division { numerator: i32, divisor: i32 },
    #[error("weapon index {0} is outside the HUD's sixteen-model stack table")]
    WeaponIndex(i8),
    #[error("mode WORD {0:04X} names the negative sprite argument the HUD passes as NULL")]
    ModeSprite(u16),
    /// Under the special view `0041FF20` writes no direction, and the rings
    /// read the stack's residue.
    #[error("the sight's rings read the direction 0041FF20 left unwritten")]
    SightDirection,
    #[error("the target box projects a focus (virtual +1C) the target never wrote")]
    TargetFocus,
    #[error("the health bar reads a maximum health +A2 the framed actor never wrote")]
    TargetMaximumHealth,
}

fn divide(numerator: i32, divisor: i32) -> Result<i32, VehicleHudError> {
    numerator
        .checked_div(divisor)
        .ok_or(VehicleHudError::Division { numerator, divisor })
}

/// `x * numerator / denominator` with the HUD's wrapping multiply and
/// truncating signed division.
fn scale(value: i32, numerator: i32, denominator: i32) -> i32 {
    value.wrapping_mul(numerator) / denominator
}

/// The width the HUD lays out on: the surface's, or for a surface wider than
/// 4:3 (widescreen), its 4:3 centre's. Retail's surfaces are all 4:3, so this
/// is retail's width.
pub fn layout_width([width, height]: [i32; 2]) -> i32 {
    width.min(height.wrapping_mul(4) / 3)
}

/// An X at `numerator / denominator` of the HUD's layout width, moved to its
/// side of a widescreen surface: an element in the left third keeps its 4:3
/// position, one in the right third moves by the surface's extra width, and
/// one in the middle third by half of it. Each cluster keeps retail's
/// spacing and sizes and sits against its own edge; a 4:3 surface keeps
/// retail's X.
pub fn hud_x(surface: [i32; 2], numerator: i32, denominator: i32) -> i32 {
    let layout = layout_width(surface);
    let extra = surface[0] - layout;
    let x = scale(layout, numerator, denominator);
    if numerator.wrapping_mul(3) < denominator {
        x
    } else if numerator.wrapping_mul(3) > denominator.wrapping_mul(2) {
        x + extra
    } else {
        x + extra / 2
    }
}

fn quad(sprite: u16, corners: [[i32; 2]; 4], mode: DirectSpriteMode) -> HudOp {
    HudOp::Quad(DirectSpriteQuad {
        sprite_slot: sprite,
        corners: corners.map(|corner| corner.map(|value| value as i16)),
        vertex_depths: [QUAD_DEPTH; 4],
        descriptor_word: 0,
        mode,
    })
}

/// `0047ED20(x, y, size, fill, colour)`. Below 40 the frame is one quad of
/// sprite 0x2AC at seven eighths of the size; otherwise four mirrored
/// quarters of 0x2A6. The horizontal extent follows the focal ratio. A
/// positive fill (0x8000 = full) adds the 0x2AB disc in `colour` at
/// `fill` of three quarters of the size, at least one pixel.
pub fn gauge(
    ops: &mut Vec<HudOp>,
    focal: [i32; 2],
    [x, y]: [i32; 2],
    size: i32,
    fill: i32,
    colour: u32,
) -> Result<(), VehicleHudError> {
    let half_width = divide(focal[0].wrapping_mul(size), focal[1])?;
    if size < 0x28 {
        let height = size.wrapping_mul(7) / 8;
        let width = half_width.wrapping_mul(7) / 8;
        ops.push(quad(
            GAUGE_SMALL_SPRITE,
            [
                [x - width, y - height],
                [x + width, y - height],
                [x + width, y + height],
                [x - width, y + height],
            ],
            DirectSpriteMode::Textured,
        ));
    } else {
        let (top, bottom) = (y - size, y + size);
        let (left, right) = (x - half_width, x + half_width);
        for corners in [
            [[x, top], [left, top], [left, y], [x, y]],
            [[x, top], [right, top], [right, y], [x, y]],
            [[x, bottom], [left, bottom], [left, y], [x, y]],
            [[x, bottom], [right, bottom], [right, y], [x, y]],
        ] {
            ops.push(quad(
                GAUGE_QUARTER_SPRITE,
                corners,
                DirectSpriteMode::Textured,
            ));
        }
    }
    if fill > 0 {
        // 0047F011 compares the product's low WORD.
        let product = (size.wrapping_mul(6) / 8).wrapping_mul(fill) >> 15;
        let radius = i32::from(product as i16).max(1);
        let half = divide(focal[0].wrapping_mul(radius), focal[1])?;
        ops.push(quad(
            GAUGE_FILL_SPRITE,
            [
                [x - half, y - radius],
                [x + half, y - radius],
                [x + half, y + radius],
                [x - half, y + radius],
            ],
            DirectSpriteMode::Effect { color_word: colour },
        ));
    }
    Ok(())
}

fn reticle_quad(sprite: u16, corners: [[i32; 2]; 4], colour: u32) -> HudOp {
    HudOp::Quad(DirectSpriteQuad {
        sprite_slot: sprite,
        corners: corners.map(|corner| corner.map(|value| value as i16)),
        vertex_depths: [RETICLE_DEPTH; 4],
        descriptor_word: 0,
        mode: DirectSpriteMode::Effect { color_word: colour },
    })
}

/// `0047F350(point, colour, w, h)`: a chevron of three one-pixel quads of
/// sprite 0x22B in `colour`, its line `w` either side of the point at `h`
/// above it and its sides meeting below it, built from ninths of the sizes
/// (each height through the focal ratio). A zero `h` draws a narrow tall
/// one: half width 7 and height `w`, at least 7.
pub fn bracket(
    ops: &mut Vec<HudOp>,
    focal: [i32; 2],
    [x, y]: [i32; 2],
    colour: u32,
    w: i32,
    h: i32,
) -> Result<(), VehicleHudError> {
    let (half, tall) = if h == 0 { (7, w.max(7)) } else { (w, h) };
    let half_y = divide(focal[1].wrapping_mul(half), focal[0])?;
    let tall_y = divide(focal[1].wrapping_mul(tall), focal[0])?;
    let top = y.wrapping_sub(tall_y);
    ops.push(reticle_quad(
        BRACKET_SPRITE,
        [
            [x.wrapping_sub(half), top],
            [x.wrapping_add(half), top],
            [x.wrapping_add(half), top.wrapping_add(1)],
            [x.wrapping_sub(half), top.wrapping_add(1)],
        ],
        colour,
    ));
    let ninth = |value: i32| value / 9;
    let four_w = ninth(half.wrapping_mul(4));
    let five_w = ninth(half.wrapping_mul(5));
    let eight_h = ninth(tall.wrapping_mul(8));
    let four_hy = ninth(tall_y.wrapping_mul(4));
    let eight_wy = ninth(half_y.wrapping_mul(8));
    let upper = y.wrapping_add(four_hy).wrapping_sub(eight_wy);
    let lower = y.wrapping_add(four_hy).wrapping_add(eight_wy);
    for (from, to) in [
        (
            x.wrapping_sub(eight_h).wrapping_sub(four_w),
            x.wrapping_sub(eight_h).wrapping_add(five_w),
        ),
        (
            x.wrapping_add(four_w).wrapping_add(eight_h),
            x.wrapping_sub(five_w).wrapping_add(eight_h),
        ),
    ] {
        ops.push(reticle_quad(
            BRACKET_SPRITE,
            [
                [from, upper],
                [to, lower],
                [to.wrapping_add(1), lower],
                [from.wrapping_add(1), upper],
            ],
            colour,
        ));
    }
    Ok(())
}

/// `0047F080(x, y, filled, full, 0, thick, rest, bar)`, the horizontal bar:
/// a frame of sprite 0xE9 in `bar` one pixel right of and below the bar,
/// then sprite 0x22A from `x` to `x + filled` in `bar` and on to
/// `x + full` in `rest`, each `thick` tall above `y`. (Its other mode, a
/// vertical bar, the box never draws.)
pub fn target_bar(
    ops: &mut Vec<HudOp>,
    [x, y]: [i32; 2],
    filled: i32,
    full: i32,
    thick: i32,
    rest: u32,
    bar: u32,
) {
    let (end, edge) = (x.wrapping_add(full), x.wrapping_add(filled));
    let top = y.wrapping_sub(thick);
    ops.push(reticle_quad(
        BAR_FRAME_SPRITE,
        [
            [end.wrapping_add(1), y.wrapping_add(1)],
            [x.wrapping_add(1), y.wrapping_add(1)],
            [x.wrapping_add(1), top.wrapping_add(1)],
            [end.wrapping_add(1), top.wrapping_add(1)],
        ],
        bar,
    ));
    ops.push(reticle_quad(
        BAR_SPRITE,
        [[x, y], [edge, y], [edge, top], [x, top]],
        bar,
    ));
    ops.push(reticle_quad(
        BAR_SPRITE,
        [[end, y], [edge, y], [edge, top], [end, top]],
        rest,
    ));
}

/// `0047F080(x, y, filled, full, fill, thick, rest, bar)` in either mode: a
/// zero `fill` is the horizontal [`target_bar`]; otherwise the bar stands
/// `full` wide and `thick` tall above `y`: the frame one pixel right of and
/// below it, then sprite 0x22A in `bar` from `y` up `fill` and on to the
/// top in `rest` (its corners written by `0040EF40`). `filled` is unread.
#[allow(clippy::too_many_arguments)]
pub fn hud_bar(
    ops: &mut Vec<HudOp>,
    [x, y]: [i32; 2],
    filled: i32,
    full: i32,
    fill: i32,
    thick: i32,
    rest: u32,
    bar: u32,
) {
    if fill == 0 {
        return target_bar(ops, [x, y], filled, full, thick, rest, bar);
    }
    let (right, top, level) = (
        x.wrapping_add(full),
        y.wrapping_sub(thick),
        y.wrapping_sub(fill),
    );
    ops.push(reticle_quad(
        BAR_FRAME_SPRITE,
        [
            [x.wrapping_add(1), top.wrapping_add(1)],
            [right.wrapping_add(1), top.wrapping_add(1)],
            [right.wrapping_add(1), y.wrapping_add(1)],
            [x.wrapping_add(1), y.wrapping_add(1)],
        ],
        bar,
    ));
    ops.push(reticle_quad(
        BAR_SPRITE,
        [[x, y], [right, y], [right, level], [x, level]],
        bar,
    ));
    ops.push(reticle_quad(
        BAR_SPRITE,
        [[x, top], [right, top], [right, level], [x, level]],
        rest,
    ));
}

/// `0043A150`'s descriptor at `point`, published (`0048FB40`) and projected
/// at its origin through the near local entry (`004EB5A0`): the screen point
/// of a world point, as the reticle aims its brackets.
pub fn project_point(projection: &HudProjection, point: [i32; 3]) -> [i32; 2] {
    let transform = ModelContext::new(point, MenuBasis::INITIAL)
        .publish(projection.view, projection.coordinate_shift);
    let mut backend = ModelProjectionBackend {
        transform,
        view: projection.view,
        projection: projection.projection,
        dispatch: ModelDispatch::Near,
        coordinate_shift: projection.coordinate_shift,
        // The near entry writes no fog.
        fog_factor: 0,
    };
    let mut output = ModelVertexProjection { bytes: [None; 12] };
    let Ok(_) = backend.project(ModelVertexInput::Local([0; 3]), &mut output);
    let words = output
        .read_u32(0)
        .expect("the near projection writes the screen X and Y");
    [
        i32::from(words as u16 as i16),
        i32::from((words >> 16) as u16 as i16),
    ]
}

/// A corner a quarter of the way to `toward` (WORDs, rounding toward zero).
fn ease(corner: [u16; 2], toward: [u16; 2]) -> [u16; 2] {
    std::array::from_fn(|axis| {
        let from = i32::from(corner[axis] as i16);
        let gap = i32::from(toward[axis] as i16).wrapping_sub(from);
        from.wrapping_add(gap / 4) as u16
    })
}

/// `0041C92E..0041D065`, the reticle once no menu is shown. With the HUD
/// setting and a target of type 2 or 4, the box projects the target's focus,
/// halves the countdown +5C0 and draws its three corners in style 0x21,
/// sized `(5 - i) * 3` by `(5 - i) * 2`, each before it moves a quarter of
/// the way to the next (the last to the focus), then a bracket on the focus
/// `+5C0 / 500` tall. A type-2 actor that is not the vehicle gets a health
/// bar under it, `14H / 100` long and `H / 100` thick (H the surface's
/// height), filled by its health (below 1/32 it blinks every 15 clock ticks),
/// and a caption: H0 0x155 (with player command 0x21) for another side
/// than the vehicle's and not side 0 or 1, else H0 0x156. Another target
/// raises the countdown by 50000 up to 500000 and draws a bracket on the
/// first corner; no target only raises it. Without the box and with the HUD
/// setting, the sight's three rings each move a quarter of the way to the
/// vehicle's position plus the fire direction shifted right by `2 - i`,
/// projected, and draw in style 1.
fn reticle(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    vehicle: &HudVehicle,
) -> Result<(), VehicleHudError> {
    let hud = frame.hud_setting != 0;
    let [_, height] = frame.surface;
    let wide = frame.layout as i8 > 0;
    let mut corners = vehicle.corners;
    let mut countdown = vehicle.dword_5c0 as i32;
    let raise = |countdown: i32| countdown.wrapping_add(50_000).min(500_000);
    let word = |point: [i32; 2]| point.map(|value| value as u16);
    let screen = |corner: [u16; 2]| corner.map(|value| i32::from(value as i16));
    let mut sight = true;
    match vehicle.target.filter(|_| hud) {
        Some(target) if matches!(target.kind, 2 | 4) => {
            let focus = target.focus.ok_or(VehicleHudError::TargetFocus)?;
            let point = project_point(
                &frame.projection,
                focus.map(|value| i32::from(value as i16)),
            );
            sight = false;
            countdown /= 2;
            for index in 0..3 {
                let left = 5 - index as i32;
                let (mut w, mut h) = (left * 3, left * 2);
                if wide {
                    w = w * 3 / 2;
                    h = h * 3 / 2;
                }
                bracket(
                    ops,
                    frame.focal,
                    screen(corners[index]),
                    frame.styles.turbo,
                    w,
                    h,
                )?;
                corners[index] = if index == 2 {
                    word(point)
                } else {
                    ease(corners[index], corners[index + 1])
                };
            }
            bracket(
                ops,
                frame.focal,
                point,
                frame.styles.turbo,
                countdown / 500,
                0,
            )?;
            if let (2, Some(actor)) = (target.kind, target.actor) {
                let fill = if actor.health == 0 {
                    0
                } else {
                    let maximum = i32::from(
                        actor
                            .maximum_health
                            .ok_or(VehicleHudError::TargetMaximumHealth)?,
                    );
                    let missing = divide(
                        maximum
                            .wrapping_sub(i32::from(actor.health))
                            .wrapping_shl(16),
                        maximum.wrapping_add(1),
                    )?;
                    let fill = 0x10000 - (missing & 0xffff);
                    if fill < 0x800 {
                        (i32::from((frame.clock / 15) as i8) & 1) << 11
                    } else {
                        fill
                    }
                };
                let full = scale(height, 14, 100);
                target_bar(
                    ops,
                    [point[0], point[1].wrapping_add(scale(height, 2, 100))],
                    full.wrapping_mul(fill) >> 16,
                    full,
                    height / 100,
                    frame.styles.turbo,
                    frame.styles.bar,
                );
                let hostile = actor.team != vehicle.team && actor.team != 1 && actor.team != 0;
                if hostile {
                    ops.push(HudOp::PlayerCommand(HOSTILE_HINT));
                }
                ops.push(HudOp::Text {
                    text: HudText::String(if hostile { HOSTILE_CAPTION } else { CAPTION }),
                    position: HudTextPosition::At(point),
                });
            }
        }
        Some(_) => {
            countdown = raise(countdown);
            bracket(
                ops,
                frame.focal,
                screen(corners[0]),
                frame.styles.turbo,
                countdown / 500,
                0,
            )?;
        }
        None => countdown = raise(countdown),
    }
    ops.push(HudOp::SetReticle5c0(countdown as u32));
    if hud && sight {
        let direction = vehicle
            .sight_direction
            .ok_or(VehicleHudError::SightDirection)?;
        let sizes = if wide {
            [(30, 20), (24, 16), (18, 12)]
        } else {
            [(15, 10), (12, 8), (9, 6)]
        };
        for (index, (w, h)) in sizes.into_iter().enumerate() {
            let shift = 2 - index as u32;
            let point = std::array::from_fn(|axis| {
                i32::from(vehicle.position[axis] as i16)
                    .wrapping_add(i32::from(direction[axis]) >> shift)
            });
            let toward = word(project_point(&frame.projection, point));
            corners[index] = ease(corners[index], toward);
            bracket(
                ops,
                frame.focal,
                screen(corners[index]),
                frame.styles.frame,
                w,
                h,
            )?;
        }
    }
    ops.push(HudOp::SetReticleCorners(corners));
    Ok(())
}

/// The quarter-sine easing of a 0..0x8000 value (`0041E48A..0041E4BD`):
/// half the value as an angle, through the quarter table, signed.
fn eased(value: i32, sine: &[u16; 4096]) -> i32 {
    let capped = if value > 0x8000 { 0x8000 } else { value };
    sine_q15(capped / 2, sine)
}

fn weapon_model(index: i8) -> Result<i16, VehicleHudError> {
    WEAPON_ICON_MODELS
        .get(usize::try_from(index).map_err(|_| VehicleHudError::WeaponIndex(index))?)
        .copied()
        .ok_or(VehicleHudError::WeaponIndex(index))
}

/// `0041BF60(view, centre, radius, 0, 1)`: the ring of 32 marks around the
/// radar, [`hud_ring`]'s compass with both halves.
fn compass(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    centre: [i32; 2],
    radius: i32,
) -> Result<(), VehicleHudError> {
    hud_ring(ops, frame, centre, radius, false, true)
}

/// `0041BF60(view, centre, radius, vertical, both)`: 32 marks, each turned
/// 0x800 from the last (`00491D10`). The compass turns about the world's up
/// axis (`0055F280`) from the view's horizontal forward, Z of the basis (or
/// X turned a quarter while the view looks steeply up or down); its marks
/// 0, 8, 16 and 24 are "n", "w", "s" and "e", the rest dots. The pitch ring
/// turns about +X (`0055F290`) from the forward's pitch and its horizontal
/// length (`0048EFC0`); its marks 0, 8 and 24 are " 00", "+90" and "-90",
/// the rest dots, and it moves only in Y. Marks 0, 8, 16 and 24 sit a
/// quarter of the font height lower. Without `both`, only the marks in
/// front (direction Z above 0) draw, and the compass keeps them on `centre`'s
/// row. Every mark is centred on its width (`00415650`).
pub fn hud_ring(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    [x, y]: [i32; 2],
    radius: i32,
    vertical: bool,
    both: bool,
) -> Result<(), VehicleHudError> {
    let [axis_x, _, axis_z] = frame.view_basis;
    let scaled = |value: f32, factor: f64| ftol(f64::from(value) * factor);
    let (a, b) = if vertical {
        // 0041C08D..0041C0DD: the forward's pitch and horizontal length.
        let forward_x = scaled(axis_z[0], 4096.0);
        let product = scaled(axis_z[2], -4096.0).wrapping_mul(scaled(axis_z[2], 4096.0));
        let length = isqrt_48efc0(forward_x.wrapping_mul(forward_x).wrapping_sub(product));
        (scaled(axis_z[1], 4096.0) as i16, length as i16)
    } else {
        // 0041BF85..0041BF8A: a wrapping absolute value, signed comparison.
        let pitch = scaled(axis_z[1], 4096.0);
        if pitch.wrapping_abs() > 0x800 {
            (
                scaled(axis_x[2], -4096.0) as i16,
                scaled(axis_x[0], 4096.0) as i16,
            )
        } else {
            (
                scaled(axis_z[0], 4096.0) as i16,
                scaled(axis_z[2], 4096.0) as i16,
            )
        }
    };
    let (a, b) = (f64::from(a), f64::from(b));
    // The 53-bit FSQRT is compared unrounded; the divisions use its float.
    let length = (b * b + a * a).sqrt();
    let mut direction = if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        let stored = f64::from(length as f32);
        let (a, b, zero) = (
            (a / stored) as f32,
            (b / stored) as f32,
            (0.0 / stored) as f32,
        );
        if vertical {
            [zero, a, b]
        } else {
            [a, zero, b]
        }
    };
    let radius_f = f64::from(radius);
    let axis = if vertical {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    for step in 0..32 {
        if direction[2] > 0.0 || both {
            let (mut mark_x, mut mark_y) = (x, y);
            if vertical {
                let lift = ftol(radius_f * f64::from(direction[1])).wrapping_mul(frame.focal[1]);
                mark_y = divide(lift, frame.focal[0])?.wrapping_add(y);
            } else {
                if both {
                    let lift =
                        ftol(radius_f * f64::from(direction[2])).wrapping_mul(frame.focal[1]);
                    mark_y = y.wrapping_sub(divide(lift, frame.focal[0])?);
                }
                mark_x = x.wrapping_sub(ftol(radius_f * f64::from(direction[0])));
            }
            let text: &[u8] = match (step, vertical) {
                (0, false) => b"n",
                (0, true) => b" 00",
                (8, false) => b"w",
                (8, true) => b"+90",
                (16, false) => b"s",
                (24, false) => b"e",
                (24, true) => b"-90",
                _ => b".",
            };
            if step % 8 == 0 {
                mark_y = mark_y.wrapping_add(frame.font_quarter_height);
            }
            ops.push(HudOp::Text {
                text: HudText::Bytes(text.to_vec()),
                position: HudTextPosition::CentredOn([mark_x, mark_y]),
            });
        }
        direction = rotate_axis_vector(direction, axis, 0x800, &frame.sine_quarter);
    }
    Ok(())
}

/// `00473490`'s frame around `centre`: four mirrored quarters of sprite
/// 0x2A6 at nine eighths of the radar's half size.
fn radar_frame(ops: &mut Vec<HudOp>, centre: [i32; 2], size: [i32; 2]) {
    quarter_frame(ops, centre, size.map(|value| value.wrapping_mul(9) / 8));
}

/// Four mirrored quarters of sprite 0x2A6 about `centre`, each spanning
/// `half` to one side and up or down: top left, top right, bottom left,
/// bottom right. The corners are narrowed to signed WORDs.
fn quarter_frame(ops: &mut Vec<HudOp>, [x, y]: [i32; 2], [half_width, half_height]: [i32; 2]) {
    let (top, bottom) = (y - half_height, y + half_height);
    let (left, right) = (x - half_width, x + half_width);
    for corners in [
        [[x, top], [left, top], [left, y], [x, y]],
        [[x, top], [right, top], [right, y], [x, y]],
        [[x, bottom], [left, bottom], [left, y], [x, y]],
        [[x, bottom], [right, bottom], [right, y], [x, y]],
    ] {
        ops.push(quad(
            GAUGE_QUARTER_SPRITE,
            corners,
            DirectSpriteMode::Textured,
        ));
    }
}

/// The row `0041A740` draws: the primary weapons 0..7 around behavior
/// `+10`, or the secondary 8..15 around `+11`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SwitchPanel {
    Primary,
    Secondary,
}

/// `0041A740(renderer, view, icon, W, H, descriptor, row, timer, slide,
/// models)`, the switch panel that replaces a weapon's icon while its key
/// is held.
///
/// The panel grows over its first 125 ms and shrinks over its last: from a
/// timer above 875 its scale is `(875 - timer) * 100 / 125 + 100`, below
/// 125 it is `timer * 100 / 125`, otherwise 100. Each of the row's eight
/// weapons sits on a ring at the angle `((i - selected) << 16) / 8 +
/// slide` (0x10000 a turn), the selected one at the top: `W / 4` times the
/// quarter sine across and `(focal_y * W / focal_x) / 4` times the cosine
/// up, each product shifted 15 into a signed WORD. Every offset from the
/// icon toward the surface's centre is scaled by the percentage, so at
/// scale 0 the ring folds into the icon. The models sit at 500 when
/// selected, 1000 when owned (`0046BDC0` of [`WEAPON_UPGRADES`]) and 5000
/// otherwise, where the view's fade dims them.
///
/// An owned weapon gets text packets: the selected one its name
/// ([`WEAPON_NAME_STRINGS`]) seven eighths of the way out, and any whose
/// count is 0 to 9 that count, centred below the name or the model by the
/// width of "AI". Last the frame: four quarters of sprite
/// 0x2A6 about the scaled centre, `scale * W / 250` across and `(focal_y *
/// W / focal_x) * scale / 250` up.
///
/// Port widescreen: sizes take the HUD's 4:3 layout width and the ring
/// moves toward the surface's own centre, so a 4:3 surface keeps retail's
/// arithmetic.
fn weapon_switch_panel(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    vehicle: &HudVehicle,
    player: &HudPlayer,
    panel: SwitchPanel,
    [icon_x, icon_y]: [i32; 2],
) -> Result<(), VehicleHudError> {
    let (index, start, selected): (usize, i32, i8) = match panel {
        SwitchPanel::Primary => (0, 0, vehicle.weapons.primary),
        SwitchPanel::Secondary => (1, 8, vehicle.weapons.secondary),
    };
    let timer = vehicle.switch_timers[index] as i32;
    let slide = vehicle.switch_slides[index] as i32;
    // 0041A86F..0041A8CA.
    let scale = if timer > 0x36b {
        (0x36b - timer).wrapping_mul(100) / 125 + 100
    } else if timer < 0x7d {
        timer.wrapping_mul(100) / 125
    } else {
        100
    };
    let width = layout_width(frame.surface);
    let [surface_width, height] = frame.surface;
    // The icon's WORDs, sign-extended.
    let (icon_x, icon_y) = (i32::from(icon_x as i16), i32::from(icon_y as i16));
    let to_centre = [surface_width / 2 - icon_x, height / 2 - icon_y];
    let across = width / 4;
    let up = divide(frame.focal[1].wrapping_mul(width), frame.focal[0])?;
    let selected_i = i32::from(selected);
    let basis_angle = frame.clock.wrapping_mul(400);
    let toward = |offset: i32, scale: i32| offset.wrapping_mul(scale) / 100;
    for i in start..start + 8 {
        // 0041AA42..0041AAF8.
        let angle = ((i << 16).wrapping_sub(selected_i.wrapping_shl(16)) / 8).wrapping_add(slide);
        let sine =
            i32::from((sine_q15(angle, &frame.sine_quarter).wrapping_mul(across) >> 15) as i16);
        let cosine =
            (up / 4).wrapping_mul(sine_q15(angle.wrapping_add(0x4000), &frame.sine_quarter)) >> 15;
        let cosine = i32::from(cosine as i16);
        let owned = player.weapons_owned[i as usize];
        let distance = if i == selected_i {
            PANEL_SELECTED_DISTANCE
        } else if owned {
            PANEL_OWNED_DISTANCE
        } else {
            PANEL_LACKING_DISTANCE
        };
        let model_offset = [
            toward(sine + to_centre[0], scale),
            toward(to_centre[1] - cosine, scale),
        ];
        ops.push(HudOp::WeaponModel {
            model: WEAPON_ICON_MODELS[i as usize],
            centre: [icon_x + model_offset[0], icon_y + model_offset[1]],
            basis_angle,
            distance,
        });
        if !owned {
            continue;
        }
        // 0041AC47..0041AE12: the name, seven eighths of the way out.
        let mut text_offset = model_offset;
        if i == selected_i {
            text_offset = [
                toward(sine * 7 / 8 + to_centre[0], scale),
                toward(to_centre[1] - cosine * 7 / 8, scale),
            ];
            ops.push(HudOp::PanelText {
                text: HudText::String(WEAPON_NAME_STRINGS[i as usize]),
                position: HudTextPosition::CentredOn([
                    icon_x + text_offset[0],
                    icon_y + text_offset[1],
                ]),
            });
        }
        // 0041AE15..0041AF7A: the count, 0 to 9.
        let count = vehicle.weapons.ammo[i as usize];
        if (0..10).contains(&count) {
            ops.push(HudOp::PanelText {
                text: HudText::Bytes(count.to_string().into_bytes()),
                position: HudTextPosition::CentredUnder {
                    point: [icon_x + text_offset[0], icon_y + text_offset[1]],
                    spacer: PANEL_COUNT_SPACER,
                },
            });
        }
    }
    // 0041AFBA..0041B214: the frame about the scaled centre.
    let centre = [
        i32::from((icon_x + toward(to_centre[0], scale)) as i16),
        i32::from((icon_y + toward(to_centre[1], scale)) as i16),
    ];
    let half = [
        scale.wrapping_mul(width) / 250,
        up.wrapping_mul(scale) / 250,
    ];
    quarter_frame(ops, centre, half);
    Ok(())
}

/// `0041D59A..0041D8F5`: each carried actor (`00412780` of hold slot i)
/// spins in a row at the bottom left, centred at 18/256 W plus i * W / 30
/// across and 89/100 H down (84/100 for slot 1), its size plus 900 in front
/// of an identity view, the basis turned 400 times the clock in XZ.
/// Infest02's and Infest03's draws read these centres at 1024 by 768:
/// (72, 683), (106, 645) and (140, 683). Port widescreen: the row keeps
/// its 4:3 position against the left edge.
fn carried_actors(ops: &mut Vec<HudOp>, frame: &HudFrame, vehicle: &HudVehicle, basis_angle: i32) {
    let [_, height] = frame.surface;
    let width = layout_width(frame.surface);
    for (slot, carried) in vehicle.carried.iter().enumerate() {
        let Some(carried) = carried else { continue };
        let index = slot as i32;
        // 0041D6A8..0041D6DF and 0041D6E2..0041D72F: signed divisions.
        let x = index.wrapping_mul(width) / 30 + width.wrapping_mul(18) / 256;
        let y = if slot == 1 {
            scale(height, 84, 100)
        } else {
            scale(height, 89, 100)
        };
        ops.push(HudOp::CarriedActor {
            actor: carried.actor,
            centre: [x, y],
            // 0041D5E9..0041D5F6: the WORD +28 plus 900.
            distance: carried.size_28.wrapping_add(900),
            basis_angle,
        });
    }
}

/// `0041E286..0041E449`: the radar at (230/256 W, 85/100 H) with its
/// contents `004723F0`, then the bare frame at (26/256 W, 85/100 H). The
/// third-person layout centres it on the eye and turns it by the view; a
/// first-person one (`0041E2CB..0041E33E`) on the vehicle's X/Z WORDs,
/// turned by its basis (`+48`), as Infest04's first-person radar calls
/// read them.
fn radar(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    layout: u8,
    vehicle: &HudVehicle,
    player: &HudPlayer,
    world: &HudWorld,
) -> Result<(), VehicleHudError> {
    let [_, height] = frame.surface;
    let size = frame.radar_size;
    let y = scale(height, 85, 100);
    let [scanner, terrain, resource_scanner] = player.radar_upgrades;
    let centre = [hud_x(frame.surface, 230, 256), y];
    let (eye_words, basis) = if layout == 0 {
        (world.eye_words, frame.view_basis)
    } else {
        (
            [vehicle.position[0], vehicle.position[2]],
            vehicle.basis_bits.map(|row| row.map(f32::from_bits)),
        )
    };
    let rotation = crate::radar_blips::heading_rotation(basis[0]);
    let contents = |walk: bool, origin: [i32; 2]| {
        HudOp::RadarContents(walk.then(|| RadarBlipPass {
            actors: world.radar_actors.clone(),
            eye_words,
            origin,
            size,
            rotation,
            jitter: world.radar_jitter,
            clock: frame.clock,
            resource_scanner,
        }))
    };
    // 0041E343..0041E359: the Radar, or online play, draws the terrain
    // radar instead.
    if terrain || world.network.is_some() {
        ops.push(HudOp::TerrainRadar(TerrainRadarOp {
            centre,
            eye_words,
            rotation: terrain_heading_rotation(basis[0]),
            jitter: world.radar_jitter,
            viewport: frame.viewport,
        }));
        // 00470383..004703A4 and 00471989: a radar left of or above the
        // clip rectangle draws nothing, and a jitter of 0x4000 or more
        // leaves out the contents; otherwise they follow with the walk
        // (its skip byte is 0).
        let origin = terrain_radar_origin(centre, size);
        if origin[0] >= frame.viewport[0]
            && origin[1] >= frame.viewport[1]
            && world.radar_jitter < 0x4000
        {
            let origin = origin.map(i32::from);
            ops.push(contents(true, origin));
            radar_labels(ops, frame, world, eye_words, rotation, origin)?;
        }
    } else {
        radar_frame(ops, centre, size);
        // 0047368E: the frame moves its centre WORDs to the top left corner.
        let origin = [0, 1].map(|axis| i32::from((centre[axis] - size[axis]) as i16));
        // 004736B3: the walk runs only with the Scanner (its argument is the
        // complement of the skip byte).
        ops.push(contents(scanner, origin));
        radar_labels(ops, frame, world, eye_words, rotation, origin)?;
    }
    radar_frame(ops, [hud_x(frame.surface, 26, 256), y], size);
    Ok(())
}

/// `004728DF..00472B02`: each map annotation with a label, placed by its
/// offset from the radar's centre. The offset turns by the radar's heading
/// (its basis's X axis in XZ, `0047242C..004724BC`); a distance of r lands at the radar's half
/// size times the quarter sine of r (0x4000 a quarter turn), pinned at the
/// rim beyond. The label's style 0x24 pulses with the clock.
fn radar_labels(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    world: &HudWorld,
    eye_words: [u16; 2],
    [c, s]: [i32; 2],
    origin: [i32; 2],
) -> Result<(), VehicleHudError> {
    let pulse = (-1i32).wrapping_sub(frame.clock.wrapping_mul(1200)) & 0xffff;
    let style = frame.styles.counters[2];
    let channel = |shift: u32| ((((style >> shift) & 0xff) as i32).wrapping_mul(pulse) >> 15) as u8;
    let color_word = u32::from_le_bytes([channel(0), channel(8), channel(16), (style >> 24) as u8]);
    let [width, height] = frame.radar_size;
    for annotation in &world.annotations {
        if annotation.label == 0 {
            continue;
        }
        let offset = [0, 1].map(|axis| {
            i32::from(annotation.position_words[axis].wrapping_sub(eye_words[axis]) as i16)
        });
        let [dx, dz] = offset;
        // 0048EFC0 returns its root in EAX; 00472643 doubles it.
        let distance =
            isqrt_48efc0((dz.wrapping_mul(dz) >> 2).wrapping_add(dx.wrapping_mul(dx) >> 2)) as i32;
        let turned_x = i32::from((dx.wrapping_mul(c) >> 15) as i16)
            - i32::from((dz.wrapping_mul(s) >> 15) as i16);
        let turned_z = i32::from((dx.wrapping_mul(s) >> 15) as i16)
            + i32::from((dz.wrapping_mul(c) >> 15) as i16);
        let span = distance.wrapping_mul(2);
        let reach: i32 = if span as u32 >= 0x4000 {
            0x7fff_ffff
        } else {
            let sample = sine_q15(span, &frame.sine_quarter) as u32 & 0xffff;
            (sample | sample << 16) as i32
        };
        let position = if span == 0 {
            [origin[0] + width, origin[1] + height]
        } else {
            // 00472A26..00472A33: the product's bits 31..62, divided by span.
            let part =
                |value: i32| divide((i64::from(reach) * i64::from(value) >> 31) as i32, span);
            [
                origin[0] + width + part(turned_x.wrapping_mul(width))?,
                origin[1] - part(turned_z.wrapping_mul(height))? + height,
            ]
        };
        ops.push(HudOp::RadarLabel {
            text: (annotation.label as i32).to_string().into_bytes(),
            position,
            color_word,
        });
    }
    Ok(())
}

/// `0041DCCB`: Powerball's ball as the secondary's model.
const BALL_MODEL: i16 = 0x271;

/// The HUD's draw in native order.
pub fn plan_vehicle_hud(inputs: &HudInputs) -> Result<Vec<HudOp>, VehicleHudError> {
    let HudInputs {
        frame,
        vehicle,
        player,
        world,
    } = inputs;
    let mut ops = Vec::new();
    // 0041C4B9: an undrawn vehicle forces layout 2 and the cockpit prefix.
    // Any other layout draws the third-person HUD, a nonzero one with its
    // first-person sizes and radar.
    let layout = if vehicle.drawn { frame.layout } else { 2 };
    if layout == 2 {
        ops.push(HudOp::Omission(HudOmission::FirstPerson { layout }));
        return Ok(ops);
    }

    // 0041C940, 0041D065, 0041D3FF, 0041DD88 and 0041DEDA: Powerball's
    // leaves.
    let powerball = world.network.is_some_and(|network| network.mode == 4);

    // 0041C92E: the reticle, unless a menu is shown.
    if player.message_shown == 0 && !powerball {
        reticle(&mut ops, frame, vehicle)?;
    }

    // 0041D07D: the health gauge and the mode icon.
    if !powerball {
        health_gauge(&mut ops, frame, vehicle)?;
    }

    // 0041D2BB: the models' light, then the carried actors (0041D59A).
    ops.push(HudOp::WeaponLight);
    let basis_angle = frame.clock.wrapping_mul(400);
    if !powerball {
        carried_actors(&mut ops, frame, vehicle, basis_angle);
    }
    weapons_and_counters(
        &mut ops,
        frame,
        vehicle,
        player,
        world,
        layout,
        basis_angle,
        powerball,
    )?;
    Ok(ops)
}

/// `0041D07D..0041D2BB`: the health gauge and the mode icon.
fn health_gauge(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    vehicle: &HudVehicle,
) -> Result<(), VehicleHudError> {
    let [_, height] = frame.surface;
    let focal = frame.focal;
    let fraction = divide(
        i32::from(vehicle.health) << 15,
        i32::from(vehicle.maximum_health),
    )?;
    let green = (fraction.wrapping_mul(255) >> 15) as u8;
    let red = 0xffu8.wrapping_sub(green);
    let centre = [hud_x(frame.surface, 203, 256), scale(height, 200, 256)];
    gauge(
        ops,
        focal,
        centre,
        scale(height, 5, 100),
        0x8000 - fraction,
        0x0000_2040,
    )?;
    if fraction > 0x3000 || (frame.clock / 15) % 2 != 0 {
        let sprite = vehicle.mode.wrapping_add(MODE_ICON_BASE);
        if (sprite as i16) < 0 {
            return Err(VehicleHudError::ModeSprite(vehicle.mode));
        }
        let half_width = scale(layout_width(frame.surface), 10, 256) / 2;
        let half_height = scale(height, 14, 256) / 2;
        let [x, y] = centre;
        ops.push(quad(
            sprite,
            [
                [x - half_width, y - half_height],
                [x + half_width, y - half_height],
                [x + half_width, y + half_height],
                [x - half_width, y + half_height],
            ],
            // The colour's high BYTE is unwritten stack; near effect quads
            // read only the three RGB BYTEs.
            DirectSpriteMode::Effect {
                color_word: u32::from_le_bytes([red, green, 0, 0]),
            },
        ));
    }
    Ok(())
}

/// `0041D8F5..0041EAAC`: the weapons, the radar, then offline the counters,
/// the turbo and the milestones, online the score line and the turbo.
#[allow(clippy::too_many_arguments)]
fn weapons_and_counters(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    vehicle: &HudVehicle,
    player: &HudPlayer,
    world: &HudWorld,
    layout: u8,
    basis_angle: i32,
    powerball: bool,
) -> Result<(), VehicleHudError> {
    let [_, height] = frame.surface;
    let focal = frame.focal;
    let styles = frame.styles;
    let weapons = vehicle.weapons;
    let small_gauge = scale(height, 4, 100);
    let gauge_y = scale(height, 92, 100);
    let ammo_y = scale(height, 96, 100);

    // 0041DA34: the secondary weapon, with the +218 gauge before it.
    let secondary = [hud_x(frame.surface, 197, 256), scale(height, 90, 100)];
    if vehicle.dword_218 != 0 {
        let value = vehicle.dword_218;
        let percent = if value <= 200_000 {
            0
        } else if value < 1_000_000 {
            (value as i32 - 200_000) / 8000
        } else {
            100
        };
        gauge(
            ops,
            focal,
            [hud_x(frame.surface, 177, 256), gauge_y],
            small_gauge,
            (percent << 15) / 100,
            styles.frame,
        )?;
    }
    if vehicle.switching[1] != 0 {
        weapon_switch_panel(
            ops,
            frame,
            vehicle,
            player,
            SwitchPanel::Secondary,
            secondary,
        )?;
    } else {
        // 0041DC92: Powerball's secondary is the ball (0x271) on the vehicle
        // that holds it, else model 0.
        let model = match world.network {
            Some(network) if network.mode == 4 => {
                if network.holds_ball {
                    BALL_MODEL
                } else {
                    0
                }
            }
            _ => weapon_model(weapons.secondary)?,
        };
        ops.push(HudOp::WeaponModel {
            model,
            centre: secondary,
            basis_angle,
            distance: WEAPON_MODEL_POSITION[2],
        });
        gauge(
            ops,
            focal,
            secondary,
            scale(height, 8, 100),
            0,
            styles.frame,
        )?;
        if !powerball {
            ops.push(HudOp::Text {
                text: HudText::Bytes(weapons.secondary_ammo.to_string().into_bytes()),
                position: HudTextPosition::CentredOn([secondary[0], ammo_y]),
            });
        }
    }

    // 0041DE68: the primary, with its charge gauge (0048D7D0) before it.
    let primary = [hud_x(frame.surface, 59, 256), scale(height, 90, 100)];
    // 0041DEDA: Powerball shows no primary.
    if !powerball && weapons.primary_charge > 3 {
        gauge(
            ops,
            focal,
            [hud_x(frame.surface, 79, 256), gauge_y],
            small_gauge,
            (weapons.primary_charge << 15) / 100,
            styles.turbo,
        )?;
    }
    if powerball {
        // As above.
    } else if vehicle.switching[0] != 0 {
        weapon_switch_panel(ops, frame, vehicle, player, SwitchPanel::Primary, primary)?;
    } else {
        ops.push(HudOp::WeaponModel {
            model: weapon_model(weapons.primary)?,
            centre: primary,
            basis_angle,
            distance: WEAPON_MODEL_POSITION[2],
        });
        gauge(ops, focal, primary, scale(height, 8, 100), 0, styles.frame)?;
        if (0..10).contains(&weapons.primary_ammo) {
            ops.push(HudOp::Text {
                text: HudText::Bytes(weapons.primary_ammo.to_string().into_bytes()),
                position: HudTextPosition::CentredOn([primary[0], ammo_y]),
            });
        }
    }
    ops.push(HudOp::Flush);

    // 0041E286: the radar and its bare twin, then the counters, the turbo
    // and the milestones.
    radar(ops, frame, layout, vehicle, player, world)?;
    let counter_size = scale(height, 3, 100);
    let turbo = ((player.turbo << 15) as i32) / 3;
    let turbo_gauge = |ops: &mut Vec<HudOp>| {
        gauge(
            ops,
            focal,
            [hud_x(frame.surface, 216, 256), scale(height, 188, 256)],
            counter_size,
            if turbo > 0x8000 { 0x8000 } else { turbo },
            styles.turbo,
        )
    };
    if let Some(network) = world.network {
        // 0041E44E: online the counters give way to the score line, and
        // the turbo follows it (0041E9E6).
        score_line(ops, frame, network.score);
        turbo_gauge(ops)?;
        return compass_and_clock(ops, frame, vehicle, world);
    }
    for (index, (&count, [x, y])) in player
        .counters
        .iter()
        .zip([[35, 187], [44, 196], [50, 209]])
        .enumerate()
    {
        let value = ((count << 15) as i32) / 30;
        gauge(
            ops,
            focal,
            [hud_x(frame.surface, x, 256), scale(height, y, 256)],
            counter_size,
            eased(value, &frame.sine_quarter),
            styles.counters[index],
        )?;
    }
    turbo_gauge(ops)?;
    ops.push(HudOp::Text {
        text: HudText::Bytes(
            format!(
                "{}/{}",
                player.milestones as i16, world.milestone_total as i32
            )
            .into_bytes(),
        ),
        position: HudTextPosition::At([hud_x(frame.surface, 20, 256), scale(height, 94, 100)]),
    });
    compass_and_clock(ops, frame, vehicle, world)
}

/// `0041E7A3..0041E9DB`, online: the score line.
fn score_line(ops: &mut Vec<HudOp>, frame: &HudFrame, score: HudScore) {
    let [_, height] = frame.surface;
    let at = [hud_x(frame.surface, 20, 256), scale(height, 88, 100)];
    let text = |text: String, position: [i32; 2]| HudOp::Text {
        text: HudText::Bytes(text.into_bytes()),
        position: HudTextPosition::At(position),
    };
    match score {
        HudScore::None => {}
        HudScore::Own(score) => ops.push(text(format!("{score:02}"), at)),
        HudScore::Team([captures, losses]) => ops.push(text(format!("{captures}/{losses}"), at)),
        HudScore::Race { laps, total, place } => {
            ops.push(text(format!("{}/{}", laps as i32, total as i32), at));
            if let Some(place) = place {
                let back = [
                    scale(layout_width(frame.surface), 4, 100),
                    scale(height, 4, 100),
                ];
                ops.push(text(format!("{place}"), [at[0] - back[0], at[1] - back[1]]));
            }
        }
        HudScore::Unwritten => ops.push(HudOp::Omission(HudOmission::ScoreBuffer)),
    }
}

/// `0041EAAC..`: the compass ring around the radar, the clock, the notices.
fn compass_and_clock(
    ops: &mut Vec<HudOp>,
    frame: &HudFrame,
    vehicle: &HudVehicle,
    world: &HudWorld,
) -> Result<(), VehicleHudError> {
    let [_, height] = frame.surface;

    // 0041EAAC: the compass ring around the radar, the clock, the notices.
    let ring_centre = [hud_x(frame.surface, 230, 256), scale(height, 85, 100)];
    compass(
        ops,
        frame,
        ring_centre,
        scale(layout_width(frame.surface), 8, 100),
    )?;
    let seconds = world.seconds as i32;
    ops.push(HudOp::Text {
        text: HudText::Bytes(
            format!(
                "{:02}:{:02}:{:02}",
                (seconds / 3600) % 100,
                (seconds / 60) % 60,
                seconds % 60
            )
            .into_bytes(),
        ),
        position: HudTextPosition::At([hud_x(frame.surface, 27, 64), scale(height, 31, 32)]),
    });
    if vehicle.notices[0] != 0 && frame.clock & 0x10 != 0 {
        ops.push(HudOp::Text {
            text: HudText::String(NOTICE_STRINGS[0]),
            position: HudTextPosition::ScreenCentred {
                y: scale(height, 50, 100),
            },
        });
        ops.push(HudOp::ClearNotice(0));
    }
    if vehicle.notices[1] != 0 && frame.clock & 0x10 == 0 {
        ops.push(HudOp::Text {
            text: HudText::String(NOTICE_STRINGS[1]),
            position: HudTextPosition::ScreenCentred {
                y: scale(height, 54, 100),
            },
        });
        ops.push(HudOp::ClearNotice(1));
    }
    Ok(())
}

#[cfg(test)]
#[path = "vehicle_hud_tests.rs"]
mod tests;
