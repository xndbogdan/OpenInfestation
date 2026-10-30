//! Effect-policy checks with a recording host, then the 2->9->2 sequence
//! through the real SessionController to pin the kernel/policy boundary.

use super::*;
use crate::session::{
    ScreenFactory, ScreenKind, SessionController, SessionHost, SessionPhase, SessionState,
    TickCallback, TransitionEffect,
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    PrepareOffline,
    ClearPrepared,
    FrontendMusic,
    ResetBaselines,
}

#[derive(Default)]
struct RecordingHost {
    calls: Vec<Call>,
    prepare_unavailable: bool,
}

impl SessionEffectsHost for RecordingHost {
    type Error = &'static str;
    fn prepare_offline_game(&mut self, _: &mut SessionState) -> Result<(), Self::Error> {
        self.calls.push(Call::PrepareOffline);
        if self.prepare_unavailable {
            return Err("actual preparation owner unavailable");
        }
        Ok(())
    }
    fn clear_prepared_world(&mut self, _: &SessionState) -> Result<(), Self::Error> {
        self.calls.push(Call::ClearPrepared);
        Ok(())
    }
    fn select_frontend_music(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Call::FrontendMusic);
        Ok(())
    }
    fn reset_input_baselines(&mut self) -> Result<(), Self::Error> {
        self.calls.push(Call::ResetBaselines);
        Ok(())
    }
}

#[test]
fn prepare_phase9_delegates_the_complete_actual_owner() {
    let mut host = RecordingHost::default();
    apply_transition_effect(
        TransitionEffect::PreparePhase9,
        &mut host,
        &mut SessionState::default(),
    )
    .unwrap();
    assert_eq!(host.calls, [Call::PrepareOffline]);
}

#[test]
fn reached_prepare_error_is_preserved() {
    let mut host = RecordingHost {
        prepare_unavailable: true,
        ..RecordingHost::default()
    };
    assert!(matches!(
        apply_transition_effect(
            TransitionEffect::PreparePhase9,
            &mut host,
            &mut SessionState::default()
        ),
        Err(SessionEffectsError::Host(
            "actual preparation owner unavailable"
        ))
    ));
    assert_eq!(host.calls, [Call::PrepareOffline]);
}

#[test]
fn leave_world_clears_prepared_state() {
    let mut host = RecordingHost::default();
    apply_transition_effect(
        TransitionEffect::LeaveWorld,
        &mut host,
        &mut SessionState::default(),
    )
    .unwrap();
    assert_eq!(host.calls, [Call::ClearPrepared]);
}

#[test]
fn frontend_music_and_baseline_reset_forward() {
    let mut host = RecordingHost::default();
    apply_transition_effect(
        TransitionEffect::SelectFrontendMusic,
        &mut host,
        &mut SessionState::default(),
    )
    .unwrap();
    apply_transition_effect(
        TransitionEffect::ResetInputBaselines,
        &mut host,
        &mut SessionState::default(),
    )
    .unwrap();
    assert_eq!(host.calls, [Call::FrontendMusic, Call::ResetBaselines]);
}

#[test]
fn world_and_results_effects_stay_unsupported() {
    for effect in [
        TransitionEffect::PrepareFromFrontend,
        TransitionEffect::LoadWorld,
        TransitionEffect::SelectWorldMusic,
        TransitionEffect::ResumeWorldMusic,
        TransitionEffect::PrepareResults,
        TransitionEffect::SetPlayerTimer {
            microseconds: 500_000,
        },
    ] {
        let mut host = RecordingHost::default();
        assert!(
            matches!(
                apply_transition_effect(effect, &mut host, &mut SessionState::default()),
                Err(SessionEffectsError::Unsupported(found)) if found == effect
            ),
            "{effect:?}"
        );
    }
}

/// The real controller sequences effects; this pins that a 2->9->2 request
/// pair produces PreparePhase9, then LeaveWorld + SelectFrontendMusic, with
/// ResetInputBaselines closing every transition.
#[test]
fn controller_sequences_the_2_9_2_effects() {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Screen {
        Startup,
        Frontend,
        Text,
    }
    struct Host {
        effects: RecordingHost,
        screen: Option<Screen>,
        destroyed: Vec<Screen>,
        created: Vec<ScreenFactory>,
    }
    impl SessionHost for Host {
        type Screen = Screen;
        type ScreenContext = u32;
        type Error = &'static str;
        fn current_screen(&self) -> Option<Self::Screen> {
            self.screen
        }
        fn screen_kind(
            &mut self,
            _: Screen,
            _: &mut SessionState,
        ) -> Result<ScreenKind, Self::Error> {
            Ok(ScreenKind::Frontend)
        }
        fn screen_context(&self, _: Screen) -> Self::ScreenContext {
            0
        }
        fn reuse_screen(
            &mut self,
            _: Screen,
            requested: SessionPhase,
            _: &mut SessionState,
        ) -> Result<bool, Self::Error> {
            Ok(matches!(
                (self.screen, requested),
                (Some(Screen::Frontend), SessionPhase::Frontend)
                    | (Some(Screen::Text), SessionPhase::Phase9)
            ))
        }
        fn destroy_screen(
            &mut self,
            screen: Screen,
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            self.destroyed.push(screen);
            self.screen = None;
            Ok(())
        }
        fn network_session_active(&self) -> bool {
            false
        }
        fn transition_effect(
            &mut self,
            effect: TransitionEffect,
            state: &mut SessionState,
        ) -> Result<(), Self::Error> {
            apply_transition_effect(effect, &mut self.effects, state)
                .map_err(|_| "session effect failed")
        }
        fn create_screen(
            &mut self,
            factory: ScreenFactory,
            _: u32,
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            self.created.push(factory);
            self.screen = Some(match factory {
                ScreenFactory::Startup => Screen::Startup,
                ScreenFactory::Frontend => Screen::Frontend,
                ScreenFactory::TextEntry => Screen::Text,
                _ => return Err("unsupported factory"),
            });
            Ok(())
        }
        fn tick(
            &mut self,
            _: TickCallback,
            _: i32,
            _: i32,
            _: &mut SessionState,
        ) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    let mut host = Host {
        effects: RecordingHost::default(),
        screen: None,
        destroyed: Vec::new(),
        created: Vec::new(),
    };
    let mut controller = SessionController::default();
    controller.create_startup_screen(&mut host, 0).unwrap();
    // Startup -> 2: leaving Title skips LeaveWorld; frontend music and the
    // baseline reset run around the frontend screen construction.
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut host).unwrap();
    assert_eq!(
        host.effects.calls,
        [Call::FrontendMusic, Call::ResetBaselines]
    );
    assert_eq!(
        host.created,
        [ScreenFactory::Startup, ScreenFactory::Frontend]
    );
    assert_eq!(host.destroyed, [Screen::Startup]);
    // 2 -> 9: the frontend screen cannot reuse phase 9 and is destroyed; the
    // text-entry factory constructs the new screen; PreparePhase9 runs.
    host.effects.calls.clear();
    assert!(controller.request_phase(SessionPhase::Phase9));
    controller.transition(&mut host).unwrap();
    assert_eq!(
        host.effects.calls,
        [Call::PrepareOffline, Call::ResetBaselines]
    );
    assert_eq!(host.created.last(), Some(&ScreenFactory::TextEntry));
    assert_eq!(host.destroyed, [Screen::Startup, Screen::Frontend]);
    // 9 -> 2: the text screen is destroyed, LeaveWorld and the frontend music
    // selection run, and the frontend screen is reconstructed.
    host.effects.calls.clear();
    assert!(controller.request_phase(SessionPhase::Frontend));
    controller.transition(&mut host).unwrap();
    assert_eq!(
        host.effects.calls,
        [
            Call::ClearPrepared,
            Call::FrontendMusic,
            Call::ResetBaselines
        ]
    );
    assert_eq!(host.created.last(), Some(&ScreenFactory::Frontend));
    assert_eq!(
        host.destroyed,
        [Screen::Startup, Screen::Frontend, Screen::Text]
    );
    assert_eq!(controller.state().current_phase(), SessionPhase::Frontend);
    assert_eq!(controller.state().tick_callback(), TickCallback::Idle);
}
