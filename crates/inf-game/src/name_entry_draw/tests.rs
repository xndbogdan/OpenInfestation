use std::cell::RefCell;

use inf_formats::{ByteSpan, FontMode, GlyphMetrics};

use super::*;

struct Resources {
    font: Font,
    missing: Option<&'static str>,
    calls: RefCell<Vec<(&'static str, i32)>>,
}

impl Resources {
    fn new(missing: Option<&'static str>) -> Self {
        Self {
            font: Font {
                local_index: 0,
                header_span: ByteSpan::new(0, 64),
                raw_header: [0; 16],
                mode: FontMode::Direct,
                coordinate_divisor: 100,
                line_step_terms: [1000, 200],
                maximum_byte: 255,
                fallback_byte: b'?',
                metric_span: ByteSpan::new(64, 2112),
                sprite_span: ByteSpan::new(2112, 3136),
                metrics: vec![
                    GlyphMetrics {
                        advance: 600,
                        spacing: -100,
                        offset: [0, 0],
                    };
                    256
                ],
                sprite_words: (1..=256).collect(),
            },
            missing,
            calls: RefCell::default(),
        }
    }
}

impl MenuTextResources for Resources {
    type Error = &'static str;

    fn string(&self, argument: i32) -> Result<&[u8], Self::Error> {
        self.calls.borrow_mut().push(("string", argument));
        Ok(b"Text")
    }

    fn font(&self, argument: i32) -> Result<&Font, Self::Error> {
        self.calls.borrow_mut().push(("font", argument));
        if self.missing == Some("font") {
            Err("font")
        } else {
            Ok(&self.font)
        }
    }

    fn style(&self, argument: i32) -> Result<u32, Self::Error> {
        self.calls.borrow_mut().push(("style", argument));
        if self.missing == Some("style") {
            Err("style")
        } else {
            Ok(0x0011_2233)
        }
    }

    fn sprite_dimensions(&self, _: i32) -> Result<[u16; 2], Self::Error> {
        panic!("the layout host owns its text and sprite consumers")
    }
}

#[derive(Default)]
struct Host {
    stars: usize,
    star_attempts: usize,
    flushes: usize,
    text: usize,
    highlights: usize,
    fail_flush: bool,
}

impl FrontendSceneHost for Host {
    type Error = &'static str;

    fn submit(&mut self, command: SceneCommand) -> Result<(), Self::Error> {
        assert!(matches!(command, SceneCommand::FlushBatch));
        self.flushes += 1;
        if self.fail_flush {
            Err("flush")
        } else {
            Ok(())
        }
    }

    fn footer_style_word(&mut self) -> Result<u32, Self::Error> {
        panic!("overlay does not fetch frontend footer style")
    }

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        panic!("surface was read by the outer frame")
    }

    fn sprite_dimensions(&mut self, _: u16) -> Result<[u16; 2], Self::Error> {
        panic!("ring highlight uses text metrics, not sprite dimensions")
    }
}

impl NameEntryDrawHost for Host {
    fn star(&mut self, _: StarQuad) -> Result<(), Self::Error> {
        self.star_attempts += 1;
        self.stars += 1;
        Ok(())
    }

    fn highlight(&mut self, _: FooterQuad) -> Result<(), Self::Error> {
        self.highlights += 1;
        Ok(())
    }

    fn line_text(&mut self, _: &mut LineTextRequest<'_>) -> Result<(), Self::Error> {
        self.text += 1;
        Ok(())
    }

    fn raw_text(&mut self, _: &mut RawMenuTextRequest<'_>) -> Result<(), Self::Error> {
        self.text += 1;
        Ok(())
    }
}

fn frame() -> NameEntryDrawFrame<'static> {
    static SINE: [u16; 4096] = [0; 4096];
    NameEntryDrawFrame {
        state: NameEntryDrawState {
            name: b"Pilot",
            selection: 0,
            row_anim: 0,
            completion_delay: 0,
            feedback: 0,
            error: false,
        },
        age_millis: 9000,
        surface_pixels: [640, 480],
        focal: [320, 320],
        context_origin_pixels: [320, 240],
        base_color: MenuColor::from_word(0x0080_8080),
        sine_quarter: &SINE,
    }
}

#[test]
fn backdrop_flush_failure_retains_stars_and_prevents_every_text_layer() {
    let resources = Resources::new(None);
    let mut host = Host {
        fail_flush: true,
        ..Host::default()
    };
    let mut request = frame();
    request.state.feedback = 500;
    request.state.error = true;
    assert!(matches!(
        draw_name_entry_overlay(request, &resources, &mut host),
        Err(NameEntryDrawError::Host("flush"))
    ));
    assert_eq!((host.stars, host.flushes), (1000, 1));
    assert_eq!((host.text, host.highlights), (0, 0));
    assert_eq!(&*resources.calls.borrow(), &[("font", 0), ("style", 2)]);
}

#[test]
fn initial_resource_failures_happen_before_pixels_or_later_lookups() {
    for missing in ["font", "style"] {
        let resources = Resources::new(Some(missing));
        let mut host = Host::default();
        assert!(matches!(
            draw_name_entry_overlay(frame(), &resources, &mut host),
            Err(NameEntryDrawError::Resource(value)) if value == missing
        ));
        assert_eq!(
            (host.star_attempts, host.flushes, host.text, host.highlights),
            (0, 0, 0, 0)
        );
        let expected = if missing == "font" {
            vec![("font", 0)]
        } else {
            vec![("font", 0), ("style", 2)]
        };
        assert_eq!(*resources.calls.borrow(), expected);
    }
}

#[test]
fn zero_focal_divisor_fails_at_rings_but_suppressed_rings_do_not_divide() {
    let resources = Resources::new(None);
    let mut host = Host::default();
    let mut request = frame();
    request.focal[0] = 0;
    assert!(matches!(
        draw_name_entry_overlay(request, &resources, &mut host),
        Err(NameEntryDrawError::Layout(MenuTextLayoutError::Division {
            divisor: 0,
            ..
        }))
    ));
    assert_eq!(
        (host.stars, host.flushes, host.text, host.highlights),
        (1000, 1, 0, 0)
    );

    request.state.completion_delay = 1;
    let mut host = Host::default();
    draw_name_entry_overlay(request, &resources, &mut host).unwrap();
    assert_eq!((host.stars, host.flushes, host.highlights), (1000, 1, 0));
    assert!(host.text > 0);
}
