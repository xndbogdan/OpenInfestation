//! Screen lifecycle and input authority at the linked draw boundary.
//!
//! Nested under text_entry_screen_tests to share its real factory/input fixture.

use super::*;
use crate::frontend_camera::AnimationClock;
use crate::name_entry_frame::{NameEntryFrameError, NameEntryFrameHost, NameEntryFrameInput};
use crate::screen_renderer_preparation::ScreenRendererPreparationHost;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DrawCapture {
    name: Vec<u8>,
    selection: i32,
    row_anim: i32,
    completion_delay: i32,
    feedback: i32,
    error: bool,
    age_millis: i32,
    animation: AnimationClock,
    selector_one: u16,
}

impl From<NameEntryFrameInput<'_>> for DrawCapture {
    fn from(input: NameEntryFrameInput<'_>) -> Self {
        Self {
            name: input.state.name.to_vec(),
            selection: input.state.selection,
            row_anim: input.state.row_anim,
            completion_delay: input.state.completion_delay,
            feedback: input.state.feedback,
            error: input.state.error,
            age_millis: input.age_millis,
            animation: input.animation,
            selector_one: input.selector_one,
        }
    }
}

/// Owner-level probe; renderer identity/order is covered by name_entry_frame.
#[derive(Default)]
struct DrawHost {
    events: Vec<&'static str>,
    captures: Vec<DrawCapture>,
    fail_overlay: bool,
}

impl ScreenRendererPreparationHost<u32> for DrawHost {
    type Renderer = ();

    fn renderer(&mut self) -> Result<(), u32> {
        self.events.push("renderer");
        Ok(())
    }
    fn replace_command_arena(&mut self, _: (), _: usize) -> Result<(), u32> {
        self.events.push("arena");
        Ok(())
    }
    fn begin_batch(&mut self, _: ()) -> Result<(), u32> {
        self.events.push("begin");
        Ok(())
    }
    fn publish_current_renderer(&mut self, _: ()) -> Result<(), u32> {
        self.events.push("publish");
        Ok(())
    }
}

impl NameEntryFrameHost for DrawHost {
    type Error = u32;
    type Context = ();

    fn surface_pixels(&mut self) -> Result<[i32; 2], Self::Error> {
        self.events.push("size");
        Ok([640, 480])
    }
    fn capture_caller_context(&mut self) -> Result<(), Self::Error> {
        self.events.push("context");
        Ok(())
    }
    fn reset_renderer(&mut self, _: ()) -> Result<(), Self::Error> {
        self.events.push("reset");
        Ok(())
    }
    fn validate_surface(&mut self) -> Result<(), Self::Error> {
        self.events.push("validate");
        Ok(())
    }
    fn draw_backdrop(&mut self, _: (), input: NameEntryFrameInput<'_>) -> Result<(), Self::Error> {
        self.events.push("backdrop");
        self.captures.push(input.into());
        Ok(())
    }
    fn draw_overlay(
        &mut self,
        _: (),
        input: NameEntryFrameInput<'_>,
        _: [i32; 2],
    ) -> Result<(), Self::Error> {
        self.events.push("overlay");
        self.captures.push(input.into());
        if self.fail_overlay {
            Err(0x301)
        } else {
            Ok(())
        }
    }
    fn finish_surface_draw(&mut self) -> Result<(), Self::Error> {
        self.events.push("finish");
        Ok(())
    }
}

fn draw_input(fixture: &Fixture) -> TextEntryScreenDraw {
    TextEntryScreenDraw {
        animation: fixture.world.process.animation(),
        selector_one: 0xbeef,
    }
}

#[test]
fn new_and_destroyed_screens_reject_draw_before_host_effects() {
    let mut fixture = Fixture::new();
    let mut host = DrawHost::default();
    let input = draw_input(&fixture);
    assert!(matches!(
        fixture.screen.draw_frame(input, &mut host),
        Err(NameEntryFrameError::Lifecycle(ScreenLifecycle::New))
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::New);
    assert!(host.events.is_empty());

    fixture.initialize();
    fixture
        .screen
        .destroy(&mut fixture.world.registry, &mut fixture.world.input)
        .unwrap();
    assert!(matches!(
        fixture.screen.draw_frame(input, &mut host),
        Err(NameEntryFrameError::Lifecycle(ScreenLifecycle::Destroyed))
    ));
    assert!(host.events.is_empty());
}

#[test]
fn ready_draw_reads_live_entry_and_uncapped_age_without_advancing_them() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    fixture.world.keys(&[0x10]);
    fixture.tick_split(40_000, 300_000, 256_000).unwrap();
    fixture.callback(0x0044_5220, 1).unwrap();
    fixture.callback(0x0044_5220, 0).unwrap();
    fixture.callback(0x0044_5260, 1).unwrap();
    fixture.callback(0x0044_5260, 0).unwrap();
    fixture.callback(CONFIRM_NAME_CALLBACK, 1).unwrap();
    fixture.screen.entry.as_mut().unwrap().set_error(true);
    // This process update is independent of the screen age and prior tick.
    fixture.world.process.advance_animation(80_000);
    let input = draw_input(&fixture);
    let before_rng = fixture.world.rng;
    let before_effects = fixture.world.effects.events.clone();
    let expected = DrawCapture {
        name: b"PilotQ".to_vec(),
        selection: 50,
        row_anim: -2520,
        completion_delay: 1000,
        feedback: 0,
        error: true,
        age_millis: 300,
        animation: input.animation,
        selector_one: 0xbeef,
    };
    let mut host = DrawHost::default();
    // Repeated draws must not consume a tick, key, RNG value or completion.
    fixture.screen.draw_frame(input, &mut host).unwrap();
    fixture.screen.draw_frame(input, &mut host).unwrap();
    assert_eq!(host.captures, vec![expected; 4]);
    assert_eq!(
        host.events
            .iter()
            .filter(|&&event| event == "finish")
            .count(),
        2
    );
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Ready);
    assert_eq!(fixture.screen.screen_input().age_millis(), 300);
    assert_eq!(fixture.screen.entry().unwrap().name(), b"PilotQ");
    assert_eq!(fixture.screen.entry().unwrap().completion_delay(), 1000);
    assert_eq!(fixture.world.process.animation(), input.animation);
    assert_eq!(fixture.world.rng, before_rng);
    assert_eq!(fixture.world.effects.events, before_effects);
}

#[test]
fn native_301_draw_failure_stops_screen_without_finish_or_retry() {
    let mut fixture = Fixture::new();
    fixture.initialize();
    let input = draw_input(&fixture);
    let mut host = DrawHost {
        fail_overlay: true,
        ..DrawHost::default()
    };
    assert!(matches!(
        fixture.screen.draw_frame(input, &mut host),
        Err(NameEntryFrameError::Host(0x301))
    ));
    assert_eq!(fixture.screen.lifecycle(), ScreenLifecycle::Stopped);
    assert_eq!(host.events.last(), Some(&"overlay"));
    assert!(!host.events.contains(&"finish"));
    assert_eq!(host.captures.len(), 2);
    let completed_events = host.events.clone();
    host.fail_overlay = false;
    assert!(matches!(
        fixture.screen.draw_frame(input, &mut host),
        Err(NameEntryFrameError::Lifecycle(ScreenLifecycle::Stopped))
    ));
    assert_eq!(host.events, completed_events);
    assert_eq!(fixture.screen.entry().unwrap().name(), b"Pilot");
    assert!(fixture.world.effects.events.is_empty());
}
