//! Port-only notice for a pad connected or removed.
//!
//! Presentation only: it reads no simulation, input or replay state and
//! writes none. The line is the port's own (font 0, H6 style 0x2A, the
//! startup screen's label yellow) over a packed-black box, drawn into the
//! frame's scene just before presentation so every renderer shows it.

use inf_formats::Font;

use crate::{
    draw_queue::DrawInvocation,
    menu::MenuColor,
    menu_text::{
        draw_line_text, measure_text_width, LineTextRequest, MenuGlyphQuad, MenuTextHost,
        MenuTextResources, INITIAL_GLYPH_BASE_WORD,
    },
    pixel_backend::PixelBackend,
    port_text::PortText,
    render_resources::RenderResources,
    scene_quad::DirectFlatQuad,
    software_menu::glyph_command,
    software_scene::SoftwareScene,
};

/// How long a notice stays up, in the frame clock's microseconds.
pub const DURATION_MICROS: u64 = 2_500_000;
/// H6 style 0x2A: RGB 255/255/0, as the startup screen's labels.
const STYLE: i32 = 0x2a;
/// The box's top, its height (one menu row) and horizontal padding. The
/// glyphs sit on a baseline 15 pixels below the top, two below the box's.
const TOP_PIXELS: i32 = 6;
const HEIGHT_PIXELS: i32 = 18;
const PADDING_PIXELS: i32 = 6;
const BASELINE_DROP_PIXELS: i32 = 15;
/// Free space kept between the box and the screen's sides.
const MARGIN_PIXELS: i32 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PadNoticeKind {
    Connected,
    Removed,
}

impl PadNoticeKind {
    fn text(self, language: u8) -> &'static [u8] {
        match self {
            Self::Connected => PortText::ControllerConnected.text(language),
            Self::Removed => PortText::ControllerRemoved.text(language),
        }
    }
}

/// The current notice: its kind, the pad's name and when it started.
#[derive(Default)]
pub struct PadNotice {
    kind: Option<PadNoticeKind>,
    name: String,
    shown_micros: u64,
    failed: bool,
}

impl PadNotice {
    /// Show the notice from `now_micros` for its duration; a newer call
    /// replaces an older one.
    pub fn show(&mut self, kind: PadNoticeKind, name: &str, now_micros: u64) {
        self.kind = Some(kind);
        self.name.clear();
        self.name.push_str(name);
        self.shown_micros = now_micros;
        self.failed = false;
    }

    /// The notice while it is current, and nothing once its time is up.
    fn current(&self, now_micros: u64) -> Option<(PadNoticeKind, &str)> {
        let kind = self.kind?;
        (now_micros >= self.shown_micros
            && now_micros - self.shown_micros < DURATION_MICROS)
            .then_some((kind, self.name.as_str()))
    }

    /// Draw the notice into the outgoing frame while it is current. A draw
    /// that fails is logged once by the caller and skipped until the next
    /// call to [`Self::show`].
    pub(crate) fn draw_current<'f, 'r>(
        &mut self,
        scene: &mut SoftwareScene<'f, 'r>,
        resources: RenderResources<'r>,
        dimensions: [u32; 2],
        language: u8,
        now_micros: u64,
    ) -> Result<(), String> {
        let Some((kind, name)) = self.current(now_micros) else {
            return Ok(());
        };
        if self.failed {
            return Ok(());
        }
        let name = name.to_owned();
        match draw(scene, resources, dimensions, language, kind, &name) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.failed = true;
                Err(error)
            }
        }
    }
}

/// `INF_PAD_NOTICE=connected:<name>` or `removed:<name>`: a visual check,
/// shown once at the first frame, ignored by hidden runs.
pub(crate) fn visual_check_request() -> Option<(PadNoticeKind, String)> {
    let value = std::env::var("INF_PAD_NOTICE").ok()?;
    let (kind, name) = value.split_once(':')?;
    let kind = match kind {
        "connected" => PadNoticeKind::Connected,
        "removed" => PadNoticeKind::Removed,
        _ => return None,
    };
    Some((kind, name.to_owned()))
}

/// Draw one notice for `kind` and `name` into `scene`'s frame.
fn draw<'f, 'r>(
    scene: &mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
    dimensions: [u32; 2],
    language: u8,
    kind: PadNoticeKind,
    name: &str,
) -> Result<(), String> {
    let font = resources.font(0).map_err(|error| error.to_string())?;
    let style = resources.style(STYLE).map_err(|error| error.to_string())?;
    let width = dimensions[0] as i32;
    let text = line(font, width, kind, name, language);
    let measured =
        i32::from(measure_text_width(font, &text).map_err(|error| error.to_string())? as i16);
    let left = (width - measured) / 2;
    let top = TOP_PIXELS;
    let bottom = top + HEIGHT_PIXELS;
    let box_left = left - PADDING_PIXELS;
    let box_right = left + measured + PADDING_PIXELS;
    // Packed black: the converted H7 zero. Depth 0x40 is the gameplay
    // overlay's near depth, so depth-tested frames keep it in front. The
    // renderer's projection and depth policy are left as the frame drew them.
    scene
        .draw_flat_quad_direct(DirectFlatQuad {
            corners: [
                [box_left as i16, top as i16],
                [box_right as i16, top as i16],
                [box_right as i16, bottom as i16],
                [box_left as i16, bottom as i16],
            ],
            vertex_depths: [0x40; 4],
            descriptor_word: 0,
            color_word: 0,
        })
        .map_err(|error| error.to_string())?;
    draw_line_text(
        &mut LineTextRequest {
            text: &text,
            font,
            position_pixels: [left, top + BASELINE_DROP_PIXELS],
            color: MenuColor::from_word(style),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: [0, 0],
        },
        &resources,
        &mut Glyphs { scene, resources },
    )
    .map_err(|error| format!("{error:?}"))
}

/// The notice's line: the generic words alone unless every character of the
/// name is one font 0 draws and the named line fits between the margins.
fn line(font: &Font, width: i32, kind: PadNoticeKind, name: &str, language: u8) -> Vec<u8> {
    let generic = kind.text(language);
    let Some(name) = latin_1_name(font, name.trim()) else {
        return generic.to_vec();
    };
    if name.is_empty() {
        return generic.to_vec();
    }
    let mut named = generic.to_vec();
    named.extend_from_slice(b": ");
    named.extend_from_slice(&name);
    let fits = measure_text_width(font, &named)
        .is_ok_and(|measured| measured + 2 * (PADDING_PIXELS + MARGIN_PIXELS) <= width);
    if fits {
        named
    } else {
        generic.to_vec()
    }
}

/// The name as Latin-1 bytes, or nothing when a byte is not one font 0 has
/// a sprite for. A byte past the font's maximum would draw `!` and a
/// missing sprite its fallback, so either makes the generic line stand.
fn latin_1_name(font: &Font, name: &str) -> Option<Vec<u8>> {
    name.chars()
        .map(|character| {
            let byte = u8::try_from(u32::from(character)).ok()?;
            (byte != 0
                && i32::from(byte) <= font.maximum_byte
                && font
                    .sprite_words
                    .get(usize::from(byte))
                    .copied()
                    .unwrap_or(0)
                    != 0)
                .then_some(byte)
        })
        .collect()
}

/// The line's glyphs, drawn at once through the scene's pixels.
struct Glyphs<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

impl MenuTextHost for Glyphs<'_, '_, '_> {
    type Error = String;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), String> {
        let mut command =
            glyph_command(self.resources, glyph).map_err(|error| error.to_string())?;
        self.scene
            .pixels
            .draw(
                DrawInvocation {
                    command_offset: 0,
                    receiver_offset: 0,
                },
                &mut command,
            )
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inf_formats::{ByteSpan, FontMode, GlyphMetrics};

    fn font(advance: i16, missing: &[u8]) -> Font {
        Font {
            local_index: 0,
            header_span: ByteSpan::new(0, 64),
            raw_header: [0; 16],
            mode: FontMode::Direct,
            coordinate_divisor: 1,
            line_step_terms: [12, 4],
            maximum_byte: 255,
            fallback_byte: b'?',
            metric_span: ByteSpan::new(64, 2112),
            sprite_span: ByteSpan::new(2112, 3136),
            metrics: vec![
                GlyphMetrics {
                    advance,
                    spacing: 0,
                    offset: [0, 0]
                };
                256
            ],
            sprite_words: (0..256)
                .map(|byte| u32::from(!missing.contains(&(byte as u8))))
                .collect(),
        }
    }

    #[test]
    fn the_notice_shows_then_expires_and_a_newer_call_replaces_it() {
        let mut notice = PadNotice::default();
        assert_eq!(notice.current(0), None);
        notice.show(PadNoticeKind::Connected, "Xbox One For Windows", 1_000);
        // A clock that starts again does not revive the old notice.
        assert_eq!(notice.current(999), None);
        assert_eq!(
            notice.current(1_000),
            Some((PadNoticeKind::Connected, "Xbox One For Windows"))
        );
        assert_eq!(
            notice.current(1_000 + DURATION_MICROS - 1),
            Some((PadNoticeKind::Connected, "Xbox One For Windows"))
        );
        assert_eq!(notice.current(1_000 + DURATION_MICROS), None);
        notice.show(PadNoticeKind::Removed, "DualSense", 9_000);
        assert_eq!(
            notice.current(9_000),
            Some((PadNoticeKind::Removed, "DualSense"))
        );
    }

    #[test]
    fn a_name_font_0_cannot_draw_falls_back_to_the_generic_line() {
        let font = font(10, &[0x80]);
        let generic = PortText::ControllerConnected.text(0);
        assert_eq!(
            line(&font, 1000, PadNoticeKind::Connected, "Xbox Pad \u{80}", 0),
            generic
        );
        assert_eq!(
            line(&font, 1000, PadNoticeKind::Connected, "Xbox \u{20ac}", 0),
            generic
        );
        // A name every byte draws keeps its words.
        assert_eq!(
            line(&font, 1000, PadNoticeKind::Connected, "Xbox Pad", 0),
            b"Controller connected: Xbox Pad"
        );
    }

    #[test]
    fn a_name_too_wide_for_the_screen_falls_back_to_the_generic_line() {
        let font = font(10, &[]);
        let generic = PortText::ControllerConnected.text(0);
        assert_eq!(
            line(&font, 200, PadNoticeKind::Connected, "Xbox One", 0),
            generic
        );
        assert_eq!(
            line(&font, 1000, PadNoticeKind::Connected, "Xbox One", 0),
            b"Controller connected: Xbox One"
        );
    }

    #[test]
    fn a_notice_draws_a_black_box_and_text_at_the_top_centre() {
        use crate::{
            frontend_assets::FrontendAssets,
            frontend_display::frontend_startup,
            model_primitives::PrimitiveTables,
            presentation::NativeFrame,
            software_frame::SoftwareFrame,
            software_scene::{SoftwareScene, SoftwareSceneRequest},
            Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let assets = FrontendAssets::open(&root).unwrap();
        let executable = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
        let tables = PrimitiveTables::from_executable(executable).unwrap();
        let mut resident = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
        resident
            .apply(ResidencyRequest::Select {
                selection: ResourceSelection {
                    difficulty: 4,
                    language: 0,
                },
            })
            .unwrap();
        resident.apply(ResidencyRequest::Boot).unwrap();
        let resources = RenderResources::new(&resident);
        let startup = frontend_startup(&resident, NativeFrame::original([1024, 768])).unwrap();
        let untouched = [31, 43, 61, 255];
        let mut pixels = SoftwareFrame::new(1024, 768, untouched);
        let mut scene = SoftwareScene::new(SoftwareSceneRequest {
            model_process: crate::model_process::ModelProcessOwners::supplied_scene_fixture(
                crate::model_scratch::ProcessModelScratch::isolated_unknown_fixture(),
                startup.scene.environment,
            ),
            resources,
            tables: &tables,
            frame: &mut pixels,
            config: startup.scene,
        })
        .unwrap();
        scene.begin_frame().unwrap();
        let mut notice = PadNotice::default();
        notice.show(PadNoticeKind::Connected, "Xbox One For Windows", 1_000);
        notice
            .draw_current(&mut scene, resources, [1024, 768], 0, 1_000)
            .unwrap();

        let font = resources.font(0).unwrap();
        let text = line(
            font,
            1024,
            PadNoticeKind::Connected,
            "Xbox One For Windows",
            0,
        );
        let measured = i32::from(measure_text_width(font, &text).unwrap() as i16);
        let left = (1024 - measured) / 2;
        let box_left = left - PADDING_PIXELS;
        let box_right = left + measured + PADDING_PIXELS;
        assert!((box_left + box_right) / 2 - 512 <= 1);
        let top = TOP_PIXELS;
        let bottom = top + HEIGHT_PIXELS;
        let inside = |x: i32, y: i32| scene.pixels.frame.pixel_rgba(x as u32, y as u32);
        // Black box pixels inside its padding, and untouched pixels beyond it.
        assert_eq!(inside(box_left + 2, top + 4), Some([0, 0, 0, 255]));
        assert_eq!(inside(box_right - 2, top + 4), Some([0, 0, 0, 255]));
        assert_eq!(inside(box_left - 1, top + 4), Some(untouched));
        assert_eq!(inside(box_right + 1, top + 4), Some(untouched));
        assert_eq!(inside((box_left + box_right) / 2, top - 1), Some(untouched));
        assert_eq!(
            inside((box_left + box_right) / 2, bottom + 1),
            Some(untouched)
        );
        // The styled glyphs change pixels within the line's box only.
        let mut text_pixels = 0;
        let mut outside = 0;
        for y in 0..768 {
            for x in 0..1024 {
                let pixel = scene.pixels.frame.pixel_rgba(x, y).unwrap();
                if pixel == [0, 0, 0, 255] || pixel == untouched {
                    continue;
                }
                if (box_left..=box_right).contains(&(x as i32))
                    && (top..=bottom).contains(&(y as i32))
                {
                    text_pixels += 1;
                } else {
                    outside += 1;
                }
            }
        }
        assert!(text_pixels > 20, "text pixels: {text_pixels}");
        assert_eq!(outside, 0);
    }
}
