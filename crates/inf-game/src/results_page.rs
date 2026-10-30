//! The results page after a world: Draw `0042C2B0` past the intro's exit
//! (`0042C321..0042CFAE`) with the panels' draws `0042B6D0` and `0042BBF0`.
//! Over the renderer the screen prepared, `004A4400` opens a batch for the
//! planet background `004737A0`, model 98 twice and the panels' frames
//! `00455080`; `004A4460` draws it, and the text follows: the exit line, the
//! world's stats, the medal and rank, the save line and the prompt.
//!
//! The final world's page (`+F5`) draws neither the planet nor the panels:
//! model `0x2B3` alone, swaying, then the credits roll (`0042AED0`) and the
//! save line near the top, with no prompt.
//!
//! The draw's state changes (the panels building each other, the score and
//! the medal) are the screen's `draw_effects`; this module only reads that
//! state. The shown profile menu (`+F7`) draws instead of the text
//! (`0042CA85`), through the caller's controller draw.

use inf_formats::Font;
use thiserror::Error;

use crate::{
    briefing_sprites::background_quads,
    frontend_camera::AnimationClock,
    frontend_scene::{
        FrontendSceneHost, ModelCallbackPolicy, ModelFailurePolicy, ModelPrimitiveContext,
        ModelSurfaceTableOrigin, SceneCommand, SceneModelSubmission, SceneProjection, SceneSetup,
        SceneView,
    },
    menu::{MenuBasis, MenuColor},
    menu_basis::{MenuBasisMath, RotationPlane},
    menu_text::{
        measure_text_width, LineTextRequest, MenuTextEmission, MenuTextLayoutError,
        MenuTextResources, MenuTextReveal, RawMenuTextRequest,
    },
    panel_animation::PanelAnimation,
    panel_frame::{draw_panel_frame, PanelFrame},
    results_credits::{roll_credits, CreditText, CreditsError, FinalCredits, PLAYER_LINE_TEXT},
    scene_sprite::DirectSpriteQuad,
    world_mission::{WorldMission, RECORD_0C_BYTES},
};

/// H4 font 0 and H6 style 5: the page's text, and its models' light.
const FONT: i32 = 0;
const STYLE: i32 = 5;
/// 004737A0's own style lookup.
const BACKGROUND_STYLE: i32 = 0x3b;
/// 0042C728: H8 slot 98, drawn twice at depth 1300.
const MODEL: i16 = 98;
const MODEL_DEPTH: i32 = 1300;
/// 0042C4CA: the final world's page draws H8 slot 0x2B3 alone, lit by style
/// 0xE, with both ends of its fade at 0xFFFF.
const FINAL_MODEL: i16 = 0x2b3;
const FINAL_STYLE: i32 = 0xe;
const FINAL_FADE: i32 = 0xffff;
/// 0042B07F: the credits' H6 styles, bold and plain.
const CREDIT_STYLES: [i32; 2] = [4, 5];
/// 0042B190, 0042B350 and 0042B510 store the same frame (+50 = 1, so H8
/// slot 99; +51 = 0; +4C = 0xB000; +54 = 3) at x 900 and y 150, 500, 850.
const PANEL_CENTERS_1024: [[i16; 2]; 3] = [[900, 150], [900, 500], [900, 850]];
const PANEL_EXTENT: u16 = 0xb000;
const PANEL_ORIENTATION: u32 = 3;
/// Mission record +0C offsets the stats read: the four kill counters and the
/// three collateral ones.
pub(crate) const KILL_FIELDS: [usize; 4] = [0xa0, 0xa8, 0xac, 0xb0];
pub(crate) const COLLATERAL_FIELDS: [usize; 3] = [0x8c, 0xb4, 0xc8];
/// EXE formats 00531D7C, 00531D80, 00531D8C and 00531D94.
const FORMAT_TEXT: &[u8] = b"%s";
const FORMAT_TIME: &[u8] = b"%02d:%02d";
const FORMAT_COUNT: &[u8] = b"%s: %d";
const FORMAT_PAIR: &[u8] = b"%s : %s";
/// H0 texts.
const EXIT_TEXT: i32 = 0x162;
const KILLS_TEXT: i32 = 0x323;
const COLLATERAL_TEXT: i32 = 0x324;
const DURATION_TEXT: i32 = 0x30b;
const TOKENS_TEXT: i32 = 0x329;
const NO_TOKENS_TEXT: i32 = 0x32a;
const ABANDONED_TEXT: i32 = 0x31f;
const ALIENS_TEXTS: [i32; 2] = [0x314, 0x315];
const SAVE_MODE_TEXT: i32 = 0x15b;
const COMPLETED_TEXT: i32 = 0x31a;
const STATUS_8_TEXT: i32 = 0x328;
const INCOMPLETE_TEXT: i32 = 0x31b;
const MEDAL_TEXT: i32 = 0x313;
const MEDAL_NAMES: i32 = 0x316;
const RANK_TEXT: i32 = 0x31e;
const RANK_NAMES: i32 = 0x355;
const RANK_LAST: i32 = 11;
const LOST_TEXT: i32 = 0x326;
const SAVING_TEXT: i32 = 0xa6;
const SAVE_FAILED_TEXT: i32 = 0xc7;
const SAVED_TEXT: i32 = 0x327;
const SAVE_OFFER_TEXT: i32 = 0x25;
const NO_PROFILE_TEXTS: [i32; 2] = [0x2e, 0x97];
const CONTINUE_TEXT: i32 = 0x26;

/// The screen's state the page reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResultsPageState {
    /// +C0, the screen's age.
    pub age_millis: i32,
    /// +14C, the screen's capped elapsed time.
    pub elapsed_millis: i32,
    /// +144, the ACAMs lost.
    pub acams_lost: u16,
    /// +13C, once awarded.
    pub medal: Option<u8>,
    /// +EC, the save countdown.
    pub save_countdown_millis: i32,
    /// +F0, +F1 and +FC.
    pub saved: bool,
    pub save_failed: bool,
    pub profile_status: u32,
    /// +148, the exit countdown.
    pub exit_millis: u32,
    /// +F7, the profile menu shown.
    pub menu_shown: bool,
    /// +130, +134 and +138 as the draw found them: a panel the draw builds
    /// gets its frame from the next draw.
    pub panels: [Option<PanelAnimation>; 3],
    /// +F5, the final world's page.
    pub final_page: bool,
}

pub struct ResultsPage<'a> {
    pub state: ResultsPageState,
    /// G+1B0.
    pub mission: &'a WorldMission,
    /// The first player's +11E (00488AA0's player), for the rank.
    pub rank_word: Option<i16>,
    /// G+5B4: the kind-7 pickups the world's retirement counted.
    pub tokens: u32,
    pub surface_pixels: [i32; 2],
    /// The template projection (focal 00546F20/24, default fade) over the
    /// surface.
    pub projection: SceneProjection,
    pub animation: AnimationClock,
    /// Process WORD 00533760, for the models' callback 0043A270.
    pub selector_one: u16,
    /// The shared glyph base 0055F8F8, which descriptor flags without bit 0
    /// select.
    pub glyph_base_color: MenuColor,
    /// What the final world's page rolls.
    pub credits: Option<&'a FinalCredits>,
}

pub trait ResultsPageResources: MenuTextResources {
    /// The first DWORD of an H8 model, its visibility extent.
    fn model_extent(&self, slot: i16) -> Result<u32, Self::Error>;
}

pub trait ResultsPageHost: FrontendSceneHost {
    /// One of 004737A0's direct sprites.
    fn sprite(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error>;
    /// Renderer virtual +14(0), which resets depth.
    fn reset_renderer(&mut self) -> Result<(), Self::Error>;
    /// 004A3B20.
    fn raw_text(&mut self, request: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error>;
    /// 004A3AB0.
    fn line_text(&mut self, request: &mut LineTextRequest<'_>) -> Result<(), Self::Error>;
}

#[derive(Debug, Error)]
pub enum ResultsPageError<R, H> {
    #[error("results page resource: {0}")]
    Resource(R),
    #[error("results page submission: {0}")]
    Backend(H),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error(transparent)]
    Format(#[from] FormatError),
    /// State retail reads without a check, where it would fault.
    #[error("the results page reads {0}, which is missing")]
    State(&'static str),
    #[error(transparent)]
    Credits(#[from] CreditsError),
}

/// A format whose directives the page's arguments do not answer.
#[derive(Debug, Error)]
#[error("results page format {0:?} needs other arguments")]
pub struct FormatError(String);

type PageResult<T, R, H> =
    Result<T, ResultsPageError<<R as MenuTextResources>::Error, <H as FrontendSceneHost>::Error>>;

/// G+1EC[G+250], the current world's status.
pub(crate) fn current_world_status(mission: &WorldMission) -> Option<u32> {
    mission
        .save
        .owner_3c
        .get(mission.current_world_a0 as usize)
        .copied()
}

/// The mission record's kill and collateral totals, as DWORD sums.
pub(crate) fn record_totals(record: &[u8; RECORD_0C_BYTES]) -> [u32; 2] {
    let dword = |offset: usize| {
        u32::from_le_bytes([
            record[offset],
            record[offset + 1],
            record[offset + 2],
            record[offset + 3],
        ])
    };
    let total = |fields: &[usize]| {
        fields
            .iter()
            .fold(0_u32, |sum, &offset| sum.wrapping_add(dword(offset)))
    };
    [total(&KILL_FIELDS), total(&COLLATERAL_FIELDS)]
}

/// Draw the page: the background, the models and the frames in one batch,
/// then the text.
pub fn draw_results_page<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    math: &MenuBasisMath,
    host: &mut H,
) -> PageResult<(), R, H> {
    // 0042C347: 004A4400.
    host.submit(SceneCommand::BeginBatch)
        .map_err(ResultsPageError::Backend)?;
    if page.state.final_page {
        final_model(page, resources, math, host)?;
    } else {
        background(page, resources, math, host)?;
        models(page, resources, math, host)?;
        for (panel, center) in page.state.panels.iter().zip(PANEL_CENTERS_1024) {
            if let Some(panel) = panel {
                draw_panel_frame(
                    PanelFrame {
                        model_slot: PanelFrame::model_for(1),
                        center_1024: center,
                        maximum_extent_word: PANEL_EXTENT,
                        flag: 0,
                        orientation: PANEL_ORIENTATION,
                    },
                    *panel,
                    page.projection,
                    page.animation,
                    math,
                    host,
                )
                .map_err(ResultsPageError::Backend)?;
            }
        }
    }
    // 0042C9D6: 004A4460.
    host.submit(SceneCommand::FlushBatch)
        .map_err(ResultsPageError::Backend)?;
    if page.state.menu_shown {
        return Ok(());
    }
    if page.state.final_page {
        final_text(page, resources, host)
    } else {
        text(page, resources, host)
    }
}

/// 0042C356..0042C3C1: the planet at (3w/4, h/2), 3w/5 across and shrinking
/// with the exit countdown, turned by +14C; then the depth reset.
fn background<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    math: &MenuBasisMath,
    host: &mut H,
) -> PageResult<(), R, H> {
    let [width, height] = page.surface_pixels;
    let mut radius = (width.wrapping_mul(3) / 5) as u32;
    if page.state.exit_millis != 0 {
        radius = page.state.exit_millis.wrapping_mul(radius) / 1000;
    }
    let center = [(width.wrapping_mul(3) / 4) as i16, (height / 2) as i16];
    let style = resources
        .style(BACKGROUND_STYLE)
        .map_err(ResultsPageError::Resource)?;
    for quad in background_quads(
        center,
        radius as u16,
        page.state.elapsed_millis,
        page.projection.focal,
        style,
        math,
    ) {
        host.sprite(quad).map_err(ResultsPageError::Backend)?;
    }
    host.reset_renderer().map_err(ResultsPageError::Backend)
}

/// 0042C3D7..0042C4B5 (0042C51D..0042C5E1 on the final page): the style
/// lights the models from (-1, 1, -1), with a quarter of its colour about.
fn light<R: ResultsPageResources, H: ResultsPageHost>(
    style: i32,
    resources: &R,
    host: &mut H,
) -> PageResult<(), R, H> {
    let rgb = resources
        .style(style)
        .map_err(ResultsPageError::Resource)?
        .to_le_bytes();
    let light = [
        SceneCommand::DirectionalLight {
            direction: [-2365.0 / 4096.0, 2365.0 / 4096.0, -2365.0 / 4096.0],
            rgb: [rgb[0], rgb[1], rgb[2]],
        },
        SceneCommand::AmbientLight([rgb[0], rgb[1], rgb[2]].map(|value| i32::from(value >> 2))),
    ];
    for command in light {
        host.submit(command).map_err(ResultsPageError::Backend)?;
    }
    Ok(())
}

/// 0042C3C4..0042C9CB: style 5 lights model 98, drawn at (60w/256, 65h/256)
/// turning with the age, zooming in over the first second, then again at
/// (60w/256, 190h/256) turned a quarter about Y-Z.
fn models<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    math: &MenuBasisMath,
    host: &mut H,
) -> PageResult<(), R, H> {
    let [width, height] = page.surface_pixels;
    let age = page.state.age_millis;
    light(STYLE, resources, host)?;
    let zoom = if age < 1000 {
        1000_i32.wrapping_sub(age).wrapping_shl(15) / 1000
    } else {
        0
    };
    let extent = resources
        .model_extent(MODEL)
        .map_err(ResultsPageError::Resource)? as i32;
    let near = MODEL_DEPTH.wrapping_sub(extent.wrapping_shl(1));
    let mut far = extent.wrapping_add(MODEL_DEPTH);
    if zoom != 0 {
        far = far.wrapping_sub(i32::from(
            (extent.wrapping_mul(zoom).wrapping_mul(3) >> 15) as i16,
        ));
    }
    let mut setup = SceneSetup::context(SceneProjection {
        fade: [near, far],
        ..page.projection
    });
    setup.center = [width.wrapping_mul(60) >> 8, height.wrapping_mul(65) >> 8];
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, 0x1000, RotationPlane::Xy);
    math.rotate_in_plane(&mut basis, age.wrapping_shl(2), RotationPlane::Xz);
    // 0043B4A0's template copy carries far color 0 and the identity view.
    for command in [
        SceneCommand::Setup(setup),
        SceneCommand::FarColor(0),
        SceneCommand::PublishView(SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        }),
        SceneCommand::Model(model(page, MODEL, basis)),
    ] {
        host.submit(command).map_err(ResultsPageError::Backend)?;
    }
    // 0042C8FA..0042C98E: only the projection is published again.
    setup.center[1] = height.wrapping_mul(190) >> 8;
    for (angle, plane) in [
        (age.wrapping_neg().wrapping_shl(2), RotationPlane::Xz),
        (-0x1000, RotationPlane::Xy),
        (0x4000, RotationPlane::Yz),
        (-0x1000, RotationPlane::Xy),
        (age.wrapping_shl(2), RotationPlane::Xz),
    ] {
        math.rotate_in_plane(&mut basis, angle, plane);
    }
    for command in [
        SceneCommand::Setup(setup),
        SceneCommand::Model(model(page, MODEL, basis)),
    ] {
        host.submit(command).map_err(ResultsPageError::Backend)?;
    }
    Ok(())
}

/// 0042C4C8..0042C723: style 0xE lights model 0x2B3, drawn once at
/// (w/2, h/3) with both ends of its fade at 0xFFFF, half a turn about Y and
/// swaying either way by the sine of four times the age (0x2000 at most, a
/// cycle every 16.4 s).
fn final_model<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    math: &MenuBasisMath,
    host: &mut H,
) -> PageResult<(), R, H> {
    let [width, height] = page.surface_pixels;
    light(FINAL_STYLE, resources, host)?;
    let mut setup = SceneSetup::context(SceneProjection {
        fade: [FINAL_FADE, FINAL_FADE],
        ..page.projection
    });
    setup.center = [width / 2, height / 3];
    let sine = math.sine_q15(page.state.age_millis.wrapping_shl(2));
    let sway = i32::from((sine.wrapping_shl(13) >> 15) as i16);
    let mut basis = MenuBasis::INITIAL;
    math.rotate_in_plane(&mut basis, sway + 0x8000, RotationPlane::Xz);
    for command in [
        SceneCommand::Setup(setup),
        SceneCommand::FarColor(0),
        SceneCommand::PublishView(SceneView {
            eye: [0; 3],
            basis: MenuBasis::INITIAL,
        }),
        SceneCommand::Model(model(page, FINAL_MODEL, basis)),
    ] {
        host.submit(command).map_err(ResultsPageError::Backend)?;
    }
    Ok(())
}

/// The 0043A150 submission 0049FEA0 draws.
fn model(page: &ResultsPage<'_>, slot: i16, basis: MenuBasis) -> SceneModelSubmission {
    SceneModelSubmission {
        model_slot: slot,
        position: [0, 0, MODEL_DEPTH],
        basis,
        animation: page.animation,
        primitive_context: ModelPrimitiveContext::Inherited,
        failure_policy: ModelFailurePolicy::StopScene,
        surface_table: ModelSurfaceTableOrigin::TemplateDefault,
        policy: ModelCallbackPolicy::NameEntry {
            selector_one: page.selector_one,
        },
    }
}

/// The text's shared descriptor: font, colour, glyph base and reveal flag.
struct Lines<'a, 'r, R, H> {
    font: &'r Font,
    color: MenuColor,
    base: MenuColor,
    resources: &'r R,
    host: &'a mut H,
}

impl<'r, R: ResultsPageResources, H: ResultsPageHost> Lines<'_, 'r, R, H> {
    fn string(&self, id: i32) -> PageResult<&'r [u8], R, H> {
        let resources: &'r R = self.resources;
        resources.string(id).map_err(ResultsPageError::Resource)
    }

    /// 004A4240, narrowed to AX and sign-extended as the callers do.
    fn measure(&self, text: &[u8]) -> PageResult<i32, R, H> {
        Ok(i32::from(measure_text_width(self.font, text)? as i16))
    }

    /// 004A3B20 with descriptor flags 2 (`Timed`) or 0 (`Complete`).
    fn draw(
        &mut self,
        text: &[u8],
        position_pixels: [i32; 2],
        width_pixels: i32,
        reveal: MenuTextReveal,
    ) -> PageResult<(), R, H> {
        self.host
            .raw_text(&mut RawMenuTextRequest {
                text,
                font: self.font,
                position_pixels,
                width_pixels,
                color: self.color,
                base_color: self.base,
                context_origin_pixels: [0, 0],
                emission: MenuTextEmission::Draw,
                reveal,
            })
            .map_err(ResultsPageError::Backend)
    }

    /// A line right-aligned at `right`, revealed from `age_millis`.
    fn right(
        &mut self,
        text: &[u8],
        right: i32,
        y: i32,
        width: i32,
        age_millis: i32,
    ) -> PageResult<(), R, H> {
        let x = right.wrapping_sub(self.measure(text)?);
        self.draw(text, [x, y], width, MenuTextReveal::Timed { age_millis })
    }
}

fn text<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    host: &mut H,
) -> PageResult<(), R, H> {
    let state = page.state;
    let [width, height] = page.surface_pixels;
    // 0042C9DB..0042CA36: the line step h/20 and the right edge 91w/100,
    // kept as WORDs.
    let step = i32::from((height.wrapping_mul(5) / 100) as i16);
    let right = i32::from((width.wrapping_mul(91) / 100) as i16);
    let font = resources.font(FONT).map_err(ResultsPageError::Resource)?;
    let color = MenuColor::from_word(resources.style(STYLE).map_err(ResultsPageError::Resource)?);
    let mut lines = Lines {
        font,
        color,
        base: page.glyph_base_color,
        resources,
        host,
    };
    // 0042CA05: the descriptor's flags are 2 until the exit line clears them.
    let mut timed = true;
    if state.exit_millis != 0 {
        // 0042CAC2..0042CB6C: centred, revealed as the countdown runs.
        let text = format(lines.string(EXIT_TEXT)?, &[])?;
        let half = lines.measure(&text)? / 2;
        lines.draw(
            &text,
            [(width / 2).wrapping_sub(half), height / 2],
            width,
            MenuTextReveal::Timed {
                age_millis: 1000_i32.wrapping_sub(state.exit_millis as i32),
            },
        )?;
        timed = false;
    }
    if let Some(first) = state.panels[0] {
        first_panel(page, first, right, step, &mut lines)?;
    }
    if let Some(second) = state.panels[1] {
        if second.current_stage >= 1 {
            second_panel(page, right, step, &mut lines)?;
        }
    }
    if let Some(third) = state.panels[2] {
        if third.current_stage >= 1 {
            third_panel(page, right, step, timed, &mut lines)?;
        }
    }
    Ok(())
}

/// 0042B6D0's text: from h/10, the kills, the collateral losses, the time
/// and the tokens left, each revealed 300 ms after the last. An abandoned
/// world (status bit 4) shows one line instead.
fn first_panel<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    first: PanelAnimation,
    right: i32,
    step: i32,
    lines: &mut Lines<'_, '_, R, H>,
) -> PageResult<(), R, H> {
    if first.current_stage < 1 {
        return Ok(());
    }
    let [width, height] = page.surface_pixels;
    let mut age = page.state.age_millis.wrapping_sub(1000);
    let mut y = height.wrapping_mul(10) / 100;
    let status = current_world_status(page.mission)
        .ok_or(ResultsPageError::State("the current world's status"))?;
    if status & 4 != 0 {
        let text = format(lines.string(ABANDONED_TEXT)?, &[])?;
        return lines.right(&text, right, y, width, age);
    }
    let record = page
        .mission
        .record_0c
        .as_deref()
        .ok_or(ResultsPageError::State("the mission record +0C"))?;
    let [kills, collateral] = record_totals(record);
    let kills = format(
        FORMAT_COUNT,
        &[Arg::Text(lines.string(KILLS_TEXT)?), Arg::Int(kills as i32)],
    )?;
    let collateral = format(
        FORMAT_COUNT,
        &[
            Arg::Text(lines.string(COLLATERAL_TEXT)?),
            Arg::Int(collateral as i32),
        ],
    )?;
    // 0042B9FE..0042BAC5: "Duration: %d secs " with 0, cut at its first
    // '0', then G+1E8's minutes and seconds.
    let mut duration = format(lines.string(DURATION_TEXT)?, &[Arg::Int(0)])?;
    if let Some(cut) = duration.iter().position(|&byte| byte == b'0') {
        duration.truncate(cut);
    }
    let millis = page
        .mission
        .dword_38
        .ok_or(ResultsPageError::State("the mission time G+1E8"))? as i32;
    duration.extend(format(
        FORMAT_TIME,
        &[Arg::Int(millis / 60000), Arg::Int((millis / 1000) % 60)],
    )?);
    let tokens = if page.tokens != 0 {
        format(
            FORMAT_COUNT,
            &[
                Arg::Text(lines.string(TOKENS_TEXT)?),
                Arg::Int(page.tokens as i32),
            ],
        )?
    } else {
        format(FORMAT_TEXT, &[Arg::Text(lines.string(NO_TOKENS_TEXT)?)])?
    };
    for (index, text) in [kills, collateral, duration, tokens].iter().enumerate() {
        if index != 0 {
            y = y.wrapping_add(step);
        }
        age = age.wrapping_sub(300);
        lines.right(text, right, y, width, age)?;
    }
    Ok(())
}

/// 0042BBF0's text: from 38h/100, the aliens cleared (blinking), the world's
/// status (blinking after 4 s), the medal, the rank and the ACAMs lost.
fn second_panel<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    right: i32,
    step: i32,
    lines: &mut Lines<'_, '_, R, H>,
) -> PageResult<(), R, H> {
    let state = page.state;
    let [width, height] = page.surface_pixels;
    let mut age = state.age_millis.wrapping_sub(2000);
    let y0 = height.wrapping_mul(38) / 100;
    let bytes = || -> PageResult<[u8; 3], R, H> {
        page.mission
            .bytes_14_16
            .ok_or(ResultsPageError::State("G+1C4..G+1C6"))
    };
    let mut y = y0.wrapping_add(step.wrapping_mul(2));
    if (state.age_millis / 500) % 2 != 0 && bytes()?[2] != 0 {
        y = y0;
        for (index, id) in ALIENS_TEXTS.into_iter().enumerate() {
            if index != 0 {
                y = y.wrapping_add(step);
            }
            age = age.wrapping_sub(150);
            let text = format(lines.string(id)?, &[])?;
            lines.right(&text, right, y, width, age)?;
        }
        y = y.wrapping_add(step);
    }
    age = age.wrapping_sub(300);
    if !(state.age_millis >= 4000 && (state.age_millis / 500) % 2 == 0) {
        if bytes()?[1] != 0 {
            age = age.wrapping_sub(300);
            let text = format(lines.string(SAVE_MODE_TEXT)?, &[])?;
            lines.right(&text, right, y, width, age)?;
        } else {
            let status = current_world_status(page.mission)
                .ok_or(ResultsPageError::State("the current world's status"))?;
            if status & 4 == 0 {
                let id = if status & 2 != 0 {
                    COMPLETED_TEXT
                } else if status & 8 != 0 {
                    STATUS_8_TEXT
                } else {
                    INCOMPLETE_TEXT
                };
                let text = format(lines.string(id)?, &[])?;
                lines.right(&text, right, y, width, age)?;
            }
        }
    }
    // Each later line steps down first.
    if let Some(medal) = state.medal {
        let text = format(
            FORMAT_PAIR,
            &[
                Arg::Text(lines.string(MEDAL_TEXT)?),
                Arg::Text(lines.string(MEDAL_NAMES + i32::from(medal))?),
            ],
        )?;
        y = y.wrapping_add(step);
        age = age.wrapping_sub(300);
        lines.right(&text, right, y, width, age)?;
    }
    let rank_word = page
        .rank_word
        .ok_or(ResultsPageError::State("the first player"))?;
    let rank = (i32::from(rank_word) / 3).clamp(0, RANK_LAST);
    let text = format(
        FORMAT_PAIR,
        &[
            Arg::Text(lines.string(RANK_TEXT)?),
            Arg::Text(lines.string(RANK_NAMES + rank)?),
        ],
    )?;
    y = y.wrapping_add(step);
    age = age.wrapping_sub(300);
    lines.right(&text, right, y, width, age)?;
    let text = format(
        FORMAT_COUNT,
        &[
            Arg::Text(lines.string(LOST_TEXT)?),
            Arg::Int(i32::from(state.acams_lost)),
        ],
    )?;
    y = y.wrapping_add(step);
    age = age.wrapping_sub(300);
    lines.right(&text, right, y, width, age)
}

/// 0042CC3C..0042CF8E: at 82h/100 the save line (blinking while saving),
/// with a second line above it when there is no profile, then the prompt,
/// no further left than 2w/5.
fn third_panel<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    right: i32,
    step: i32,
    timed: bool,
    lines: &mut Lines<'_, '_, R, H>,
) -> PageResult<(), R, H> {
    let state = page.state;
    let [width, height] = page.surface_pixels;
    let (first, second) = save_texts(&state);
    let mut y = height.wrapping_mul(82) / 100;
    let age = state.age_millis.wrapping_sub(3000);
    let reveal = |age_millis| {
        if timed {
            MenuTextReveal::Timed { age_millis }
        } else {
            MenuTextReveal::Complete
        }
    };
    if let Some(id) = first {
        let text = format(lines.string(id)?, &[])?;
        let wrap = width.wrapping_shl(2) / 5;
        let measured = lines.measure(&text)?;
        let x = right.wrapping_sub(if wrap < measured { wrap } else { measured });
        lines.draw(&text, [x, y], wrap, reveal(age))?;
        if let Some(id) = second {
            let text = format(lines.string(id)?, &[])?;
            let x = right.wrapping_sub(lines.measure(&text)?);
            lines.draw(&text, [x, y.wrapping_sub(step)], wrap, reveal(age))?;
        }
    }
    let text = format(lines.string(CONTINUE_TEXT)?, &[])?;
    let wrap = width.wrapping_mul(5) / 10;
    let measured = lines.measure(&text)?;
    let x = right
        .wrapping_sub(if wrap < measured { wrap } else { measured })
        .max(width.wrapping_shl(1) / 5);
    y = y.wrapping_add(step);
    lines.draw(&text, [x, y], wrap, reveal(age.wrapping_sub(300)))
}

/// 0042CC3C..0042CCAC: the save line's H0 text (blinking while saving),
/// and the second line without a profile.
fn save_texts(state: &ResultsPageState) -> (Option<i32>, Option<i32>) {
    if state.save_countdown_millis != 0 {
        if (state.save_countdown_millis / 100) % 2 != 0 {
            (Some(SAVING_TEXT), None)
        } else {
            (None, None)
        }
    } else if state.save_failed {
        (Some(SAVE_FAILED_TEXT), None)
    } else if state.profile_status == 3 {
        (
            Some(if state.saved {
                SAVED_TEXT
            } else {
                SAVE_OFFER_TEXT
            }),
            None,
        )
    } else {
        (Some(NO_PROFILE_TEXTS[0]), Some(NO_PROFILE_TEXTS[1]))
    }
}

/// The final world's text (0042CC11, then 0042CC3C..0042CE8C): the credits
/// roll (0042AED0) in H6 styles 4 and 5, then at h/10 the save line,
/// centred unless wider than 4w/5 (then from 91w/100 less 4w/5), with the
/// second line a step above it, centred. No prompt follows.
fn final_text<R: ResultsPageResources, H: ResultsPageHost>(
    page: &ResultsPage<'_>,
    resources: &R,
    host: &mut H,
) -> PageResult<(), R, H> {
    let state = page.state;
    let [width, height] = page.surface_pixels;
    let credits = page
        .credits
        .ok_or(ResultsPageError::State("the final world's credits"))?;
    let font = resources.font(FONT).map_err(ResultsPageError::Resource)?;
    // 0042AB00 formats the player's line into 00546A98 once; its H0 text
    // and the name do not change while the page shows.
    let player_line = format(
        resources
            .string(PLAYER_LINE_TEXT)
            .map_err(ResultsPageError::Resource)?,
        &[Arg::Text(&credits.player_name)],
    )?;
    let mut strings: Vec<(i16, &[u8])> = Vec::new();
    for line in &credits.lines {
        if let CreditText::H0(slot) = line.text {
            if !strings.iter().any(|(known, _)| *known == slot) {
                let text = resources
                    .string(i32::from(slot))
                    .map_err(ResultsPageError::Resource)?;
                strings.push((slot, terminated(text)));
            }
        }
    }
    let resolve = |text: CreditText| -> Option<&[u8]> {
        match text {
            CreditText::H0(slot) => strings
                .iter()
                .find(|(known, _)| *known == slot)
                .map(|(_, text)| *text),
            CreditText::Literal(address) => credits.literal(address),
            CreditText::PlayerLine => Some(&player_line),
            CreditText::Blank => None,
        }
    };
    let [bold, plain] = CREDIT_STYLES;
    let styles = [
        resources.style(bold).map_err(ResultsPageError::Resource)?,
        resources.style(plain).map_err(ResultsPageError::Resource)?,
    ];
    let draws = roll_credits(
        &credits.lines,
        resolve,
        |text| -> PageResult<i16, R, H> { Ok(measure_text_width(font, text)? as i16) },
        [width, height],
        state.age_millis as u32,
        styles,
    )?;
    for draw in draws {
        let text = resolve(credits.lines[draw.line].text)
            .ok_or(ResultsPageError::State("a credit line's text"))?;
        host.line_text(&mut LineTextRequest {
            text,
            font,
            position_pixels: [draw.x, draw.y],
            color: MenuColor::from_word(draw.color),
            base_color: page.glyph_base_color,
            context_origin_pixels: [0, 0],
        })
        .map_err(ResultsPageError::Backend)?;
    }
    // 0042C9DB..0042CA36: the step and the right edge, kept as WORDs.
    let step = i32::from((height.wrapping_mul(5) / 100) as i16);
    let right = i32::from((width.wrapping_mul(91) / 100) as i16);
    let color = MenuColor::from_word(resources.style(STYLE).map_err(ResultsPageError::Resource)?);
    let mut lines = Lines {
        font,
        color,
        base: page.glyph_base_color,
        resources,
        host,
    };
    let (first, second) = save_texts(&state);
    let y = height.wrapping_mul(10) / 100;
    let reveal = MenuTextReveal::Timed {
        age_millis: state.age_millis.wrapping_sub(3000),
    };
    let wrap = width.wrapping_shl(2) / 5;
    if let Some(id) = first {
        let text = format(lines.string(id)?, &[])?;
        let measured = lines.measure(&text)?;
        let x = if measured < wrap {
            width.wrapping_sub(measured) / 2
        } else {
            right.wrapping_sub(wrap)
        };
        lines.draw(&text, [x, y], wrap, reveal)?;
        if let Some(id) = second {
            let text = format(lines.string(id)?, &[])?;
            let x = width.wrapping_sub(lines.measure(&text)?) / 2;
            lines.draw(&text, [x, y.wrapping_sub(step)], wrap, reveal)?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum Arg<'a> {
    Int(i32),
    Text(&'a [u8]),
}

fn terminated(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

/// 00511B56 over the directives the page's formats use: `%d` (with an
/// optional zero flag and width), `%s` and `%%`. An H0 text passed as the
/// format with no arguments must hold no other directive.
fn format(format: &[u8], args: &[Arg<'_>]) -> Result<Vec<u8>, FormatError> {
    let format = terminated(format);
    let error = || FormatError(String::from_utf8_lossy(format).into_owned());
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut bytes = format.iter().copied().peekable();
    while let Some(byte) = bytes.next() {
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        let zero = bytes.next_if_eq(&b'0').is_some();
        let mut width = 0_usize;
        while let Some(digit) = bytes.next_if(u8::is_ascii_digit) {
            width = width * 10 + usize::from(digit - b'0');
        }
        match (bytes.next(), args.clone().next()) {
            (Some(b'%'), _) if !zero && width == 0 => out.push(b'%'),
            (Some(b'd'), Some(&Arg::Int(value))) => {
                let digits = value.unsigned_abs().to_string();
                let sign = usize::from(value < 0);
                let pad = width.saturating_sub(digits.len() + sign);
                if zero {
                    if value < 0 {
                        out.push(b'-');
                    }
                    out.extend(std::iter::repeat(b'0').take(pad));
                } else {
                    out.extend(std::iter::repeat(b' ').take(pad));
                    if value < 0 {
                        out.push(b'-');
                    }
                }
                out.extend_from_slice(digits.as_bytes());
                args.next();
            }
            (Some(b's'), Some(&Arg::Text(text))) if !zero && width == 0 => {
                out.extend_from_slice(terminated(text));
                args.next();
            }
            _ => return Err(error()),
        }
    }
    Ok(out)
}

pub(crate) mod software;

#[cfg(test)]
#[path = "results_page_tests.rs"]
mod tests;
