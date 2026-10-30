use super::*;
use crate::terrain_runtime::TerrainSpacing;

fn screen_state(phase: SessionPhase) -> SessionState {
    let mut state = SessionState::default();
    assert!(state.request_phase(phase));
    state
}

#[test]
fn terrain_policy_real_intro_screens_and_reload_keep_native_selection_order() {
    for detail in [0, 1, 2] {
        let (mut session, _) = fixture();
        session.platform.services.terrain_detail = detail;
        let policy_address = std::ptr::addr_of!(session.terrain_policy);
        session
            .prepare_offline_player(&mut SessionState::default())
            .unwrap();
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut SessionState::default())
            .unwrap();
        let intro = session.world.as_ref().unwrap();
        assert_eq!(intro.world_flags, 0x20);
        assert_eq!(intro.terrain.sampling().spacing, TerrainSpacing::Units256);
        assert_eq!(session.platform.services.terrain_detail_reads, 0);
        let mut briefing = screen_state(SessionPhase::Phase3);
        session
            .create_screen(ScreenFactory::Phase3, session.attachment, &mut briefing)
            .unwrap();
        assert_eq!(
            session.world.as_ref().unwrap().terrain.sampling().spacing,
            TerrainSpacing::Units256
        );
        session
            .destroy_screen(PortScreen::Briefing, &mut briefing)
            .unwrap();
        let before: Vec<_> = session
            .world
            .as_ref()
            .unwrap()
            .terrain
            .cells()
            .iter()
            .map(|cell| cell.raw() & !0x800)
            .collect();
        let mut gameplay = screen_state(SessionPhase::Gameplay);
        session
            .create_screen(ScreenFactory::Gameplay, session.attachment, &mut gameplay)
            .unwrap();
        let selected = if detail == 0 {
            TerrainSpacing::Units256
        } else {
            TerrainSpacing::Units128
        };
        assert_eq!(session.terrain_policy.spacing(), selected);
        assert_eq!(session.platform.services.terrain_detail_reads, 1);
        assert_eq!(
            session.world.as_ref().unwrap().terrain.sampling().spacing,
            selected
        );
        assert_eq!(
            session
                .world
                .as_ref()
                .unwrap()
                .terrain
                .cells()
                .iter()
                .map(|cell| cell.raw() & !0x800)
                .collect::<Vec<_>>(),
            before
        );
        session
            .destroy_screen(PortScreen::Gameplay, &mut gameplay)
            .unwrap();
        session
            .world
            .as_mut()
            .unwrap()
            .mission
            .save
            .mission_selection = 18;
        session
            .transition_effect(TransitionEffect::LoadWorld, &mut SessionState::default())
            .unwrap();
        assert_eq!(std::ptr::addr_of!(session.terrain_policy), policy_address);
        assert_eq!(
            session.world.as_ref().unwrap().terrain.sampling().spacing,
            selected
        );
        let mut briefing = screen_state(SessionPhase::Phase3);
        session
            .create_screen(ScreenFactory::Phase3, session.attachment, &mut briefing)
            .unwrap();
        assert_eq!(
            session.world.as_ref().unwrap().terrain.sampling().spacing,
            TerrainSpacing::Units256
        );
        assert_eq!(session.terrain_policy.spacing(), TerrainSpacing::Units256);
        session
            .destroy_screen(PortScreen::Briefing, &mut briefing)
            .unwrap();
        let mut gameplay = screen_state(SessionPhase::Gameplay);
        session
            .create_screen(ScreenFactory::Gameplay, session.attachment, &mut gameplay)
            .unwrap();
        assert_eq!(
            session.world.as_ref().unwrap().terrain.sampling().spacing,
            selected
        );
        assert_eq!(session.platform.services.terrain_detail_reads, 2);
    }
}

#[test]
fn terrain_policy_gameplay_late_failures_retain_input_and_policy_prefixes() {
    for missing_setting in [false, true] {
        let (mut session, _) = fixture();
        session.platform.services.terrain_detail = 1;
        session.platform.services.terrain_detail_missing = missing_setting;
        let mut state = screen_state(SessionPhase::Gameplay);
        let result = session.create_screen(ScreenFactory::Gameplay, session.attachment, &mut state);
        assert!(matches!(
            result,
            Err(PortSessionError::Gameplay(GameplayScreenError::Terrain(_)))
        ));
        assert_eq!(session.platform.services.terrain_detail_reads, 1);
        assert_eq!(session.platform.frontend.devices_created, 1);
        let screen = session.gameplay.as_ref().unwrap();
        assert_eq!(
            screen.lifecycle(),
            crate::screen_registry::ScreenLifecycle::Stopped
        );
        assert!(screen.id().is_some());
        assert_eq!(
            session.terrain_policy.spacing(),
            if missing_setting {
                TerrainSpacing::Units256
            } else {
                TerrainSpacing::Units128
            }
        );
        // The failed screen remains owned; the factory cannot replay its prefix.
        assert!(matches!(
            session.create_screen(ScreenFactory::Gameplay, session.attachment, &mut state),
            Err(PortSessionError::ScreenAlreadyPresent(PortScreen::Gameplay))
        ));
        assert_eq!(session.platform.services.terrain_detail_reads, 1);
    }
}
