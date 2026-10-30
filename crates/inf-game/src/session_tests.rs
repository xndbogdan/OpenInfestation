//! Observable callback order and exceptional paths of the retail dispatcher.

use std::cell::RefCell;

use super::*;
use crate::scheduler::{SchedulerActivity, SchedulerTick};

use SessionPhase as P;
use TickCallback as T;
use TransitionEffect as E;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Kind(u32),
    CaptureContext(u32, u32),
    Reuse(u32, P),
    Destroy(u32),
    Effect(E),
    Create(ScreenFactory, u32),
    Tick(T, i32, i32),
    PollInput,
    LinkedTick(SchedulerTick),
    Linked08,
    Linked0c,
    Activity(SchedulerActivity, bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Snapshot {
    current: P,
    requested: P,
    pending: bool,
    tick: T,
}

impl Snapshot {
    fn of(state: &SessionState) -> Self {
        Self {
            current: state.current_phase(),
            requested: state.requested_phase(),
            pending: state.transition_pending(),
            tick: state.tick_callback(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event {
    call: Call,
    state: Option<Snapshot>,
}

struct Host {
    screen: Option<u32>,
    context: u32,
    context_after_kind_query: Option<u32>,
    context_after_reuse: Option<u32>,
    reuse: bool,
    retained_after_destroy: Option<u32>,
    network: bool,
    change_network: Option<(E, bool)>,
    request_in_callbacks: Option<P>,
    input_selected: bool,
    input_request: Option<P>,
    linked_08_request: Option<P>,
    request_results: Vec<(Call, bool)>,
    fail_at: Option<Call>,
    /// Port-only admissions: requested phase and the events recorded before it.
    admissions: Vec<(P, usize)>,
    refuse_admission: Option<P>,
    defer_admission: Option<P>,
    events: RefCell<Vec<Event>>,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            screen: Some(1),
            context: 100,
            context_after_kind_query: None,
            context_after_reuse: None,
            reuse: false,
            retained_after_destroy: None,
            network: false,
            change_network: None,
            request_in_callbacks: None,
            input_selected: true,
            input_request: None,
            linked_08_request: None,
            request_results: Vec::new(),
            fail_at: None,
            admissions: Vec::new(),
            refuse_admission: None,
            defer_admission: None,
            events: RefCell::new(Vec::new()),
        }
    }
}

impl Host {
    fn callback(&mut self, call: Call, state: &mut SessionState) -> Result<(), &'static str> {
        self.events.borrow_mut().push(Event {
            call,
            state: Some(Snapshot::of(state)),
        });
        if let Some(request) = self.request_in_callbacks {
            self.request_results
                .push((call, state.request_phase(request)));
        }
        if self.fail_at == Some(call) {
            return Err("injected callback failure");
        }
        Ok(())
    }

    fn calls(&self) -> Vec<Call> {
        self.events
            .borrow()
            .iter()
            .map(|event| event.call)
            .collect()
    }

    fn effects(&self) -> Vec<(E, Snapshot)> {
        self.events
            .borrow()
            .iter()
            .filter_map(|event| match event.call {
                Call::Effect(effect) => Some((effect, event.state.unwrap())),
                _ => None,
            })
            .collect()
    }
}

impl SessionHost for Host {
    type Screen = u32;
    type ScreenContext = u32;
    type Error = &'static str;

    fn current_screen(&self) -> Option<u32> {
        self.screen
    }

    fn screen_kind(
        &mut self,
        screen: u32,
        state: &mut SessionState,
    ) -> Result<ScreenKind, Self::Error> {
        self.callback(Call::Kind(screen), state)?;
        if let Some(context) = self.context_after_kind_query {
            self.context = context;
        }
        Ok(ScreenKind::Gameplay)
    }

    fn screen_context(&self, screen: u32) -> u32 {
        self.events.borrow_mut().push(Event {
            call: Call::CaptureContext(screen, self.context),
            state: None,
        });
        self.context
    }

    fn reuse_screen(
        &mut self,
        screen: u32,
        requested: P,
        state: &mut SessionState,
    ) -> Result<bool, Self::Error> {
        self.callback(Call::Reuse(screen, requested), state)?;
        if let Some(context) = self.context_after_reuse {
            self.context = context;
        }
        Ok(self.reuse)
    }

    fn destroy_screen(&mut self, screen: u32, state: &mut SessionState) -> Result<(), Self::Error> {
        self.callback(Call::Destroy(screen), state)?;
        self.screen = self.retained_after_destroy;
        Ok(())
    }

    fn network_session_active(&self) -> bool {
        self.network
    }

    fn transition_effect(
        &mut self,
        effect: E,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.callback(Call::Effect(effect), state)?;
        if let Some((trigger, network)) = self.change_network {
            if effect == trigger {
                self.network = network;
            }
        }
        Ok(())
    }

    fn create_screen(
        &mut self,
        factory: ScreenFactory,
        context: u32,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.callback(Call::Create(factory, context), state)?;
        self.screen = Some(2);
        self.context = context;
        Ok(())
    }

    fn tick(
        &mut self,
        callback: T,
        delta: i32,
        elapsed: i32,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.callback(Call::Tick(callback, delta, elapsed), state)
    }
}

impl SessionFrameHost for Host {
    fn input_is_selected(&self) -> bool {
        self.input_selected
    }

    fn poll_input(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        self.callback(Call::PollInput, state)?;
        if let Some(request) = self.input_request.take() {
            self.request_results
                .push((Call::PollInput, state.request_phase(request)));
        }
        Ok(())
    }

    fn admit_transition(
        &mut self,
        state: &SessionState,
    ) -> Result<SessionTransitionAdmission, Self::Error> {
        let requested = state.requested_phase();
        self.admissions
            .push((requested, self.events.borrow().len()));
        if self.refuse_admission == Some(requested) {
            return Err("port refused transition");
        }
        Ok(if self.defer_admission == Some(requested) {
            SessionTransitionAdmission::Deferred
        } else {
            SessionTransitionAdmission::Proceed
        })
    }

    fn linked_tick(
        &mut self,
        tick: SchedulerTick,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.callback(Call::LinkedTick(tick), state)
    }

    fn linked_callback_08(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        self.callback(Call::Linked08, state)?;
        if let Some(request) = self.linked_08_request.take() {
            self.request_results
                .push((Call::Linked08, state.request_phase(request)));
        }
        Ok(())
    }

    fn linked_callback_0c(&mut self, state: &mut SessionState) -> Result<(), Self::Error> {
        self.callback(Call::Linked0c, state)
    }

    fn linked_activity(
        &mut self,
        activity: SchedulerActivity,
        state: &mut SessionState,
    ) -> Result<(), Self::Error> {
        self.callback(Call::Activity(activity, state.suspended()), state)?;
        self.input_selected = activity == SchedulerActivity::Resume;
        Ok(())
    }
}

fn controller(current: P, tick: T) -> SessionController {
    SessionController {
        state: SessionState {
            current,
            requested: current,
            tick_callback: tick,
            ..SessionState::default()
        },
        ..SessionController::default()
    }
}

fn transition(controller: &mut SessionController, host: &mut Host, phase: P) -> PhaseTransition {
    assert!(controller.request_phase(phase));
    controller.transition(host).unwrap().unwrap()
}

#[test]
fn screen_class_identity_is_separate_from_session_phase() {
    let expected = [0, 1, 2, 3, 5, 6, 5, 7, 4, 8, 8, 8, 9];
    for (phase, class) in P::ALL.into_iter().zip(expected) {
        assert_eq!(
            ScreenKind::for_factory(ScreenFactory::for_phase(phase)).id(),
            class
        );
    }
    // The host reports Gameplay class 5 even though current phase starts at 1.
    // Retail discards the class query and uses session+64C for the branch.
    let mut controller = SessionController::default();
    let mut host = Host::default();
    let result = transition(&mut controller, &mut host, P::Frontend);
    assert_eq!(result.previous, P::Title);
    assert!(!host
        .effects()
        .iter()
        .any(|(effect, _)| *effect == E::LeaveWorld));
}

/// Each class's table: `+18` answers its class ID (`xor eax, eax` or
/// `mov eax, id`, then `ret`), and `+00`, the scalar deleting destructor,
/// calls the body each port screen's `destroy` follows. Phase 5's screen is
/// not built, so only its query is checked.
#[test]
fn screen_classes_answer_their_tables() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let code = |address: u32, length: usize| &exe[(address - 0x0040_0000) as usize..][..length];
    let word = |address: u32| u32::from_le_bytes(code(address, 4).try_into().unwrap());
    let tables = [
        (
            ScreenKind::Startup,
            0x0052_1da0,
            0x0040_1300,
            Some((0x0043_2720, 0x0043_2740)),
        ),
        (
            ScreenKind::Title,
            0x0051_f600,
            0x0040_4740,
            Some((0x0040_4760, 0x0040_4780)),
        ),
        (
            ScreenKind::Frontend,
            0x0052_84d8,
            0x0048_3b70,
            Some((0x0048_3bb0, 0x0048_3bd0)),
        ),
        (
            ScreenKind::Phase3,
            0x0052_0338,
            0x0040_aa00,
            Some((0x0040_aa20, 0x0040_aa40)),
        ),
        (
            ScreenKind::Phase8,
            0x0052_1724,
            0x0040_2700,
            Some((0x0042_ae10, 0x0042_ae30)),
        ),
        (
            ScreenKind::Gameplay,
            0x0052_3370,
            0x0044_0b50,
            Some((0x0044_0b80, 0x0044_0be0)),
        ),
        (ScreenKind::Phase5, 0x0052_33d8, 0x0044_3320, None),
        (
            ScreenKind::Phase7,
            0x0052_55c8,
            0x0045_11b0,
            Some((0x0045_11d0, 0x0045_11f0)),
        ),
        (
            ScreenKind::TextEntry,
            0x0052_6028,
            0x0045_9260,
            Some((0x0045_9290, 0x0045_92b0)),
        ),
        (
            ScreenKind::Phase12,
            0x0052_4dc4,
            0x0044_4d50,
            Some((0x0044_4d70, 0x0044_4d90)),
        ),
    ];
    // +1C, the reuse query, of the classes that serve one phase:
    // cmp dword ptr [esp+4], phase; sete al; ret 4.
    for (table, reuse, phase) in [
        (0x0051_f600, 0x0040_4750, 1),
        (0x0052_84d8, 0x0048_3b80, 2),
        (0x0052_0338, 0x0040_aa10, 3),
        (0x0052_55c8, 0x0045_11c0, 7),
        (0x0052_1724, 0x0042_ae00, 8),
        (0x0052_4dc4, 0x0044_4d60, 12),
    ] {
        assert_eq!(word(table + 0x1c), reuse);
        assert_eq!(
            code(reuse, 11),
            [0x83, 0x7c, 0x24, 0x04, phase, 0x0f, 0x94, 0xc0, 0xc2, 0x04, 0x00]
        );
    }
    for (kind, table, query, destructor) in tables {
        assert_eq!(word(table + 0x18), query, "{kind:?}");
        let answer = match kind.id() {
            0 => vec![0x33, 0xc0, 0xc3],
            id => vec![0xb8, id, 0, 0, 0, 0xc3],
        };
        assert_eq!(code(query, answer.len()), answer, "{kind:?}");
        let Some((deleting, body)) = destructor else {
            continue;
        };
        assert_eq!(word(table), deleting, "{kind:?}");
        // push esi; mov esi, ecx; call body; test byte ptr [esp+8], 1
        assert_eq!(code(deleting, 4), [0x56, 0x8b, 0xf1, 0xe8], "{kind:?}");
        assert_eq!((deleting + 8).wrapping_add(word(deleting + 4)), body);
        assert_eq!(code(deleting + 8, 4), [0xf6, 0x44, 0x24, 0x08], "{kind:?}");
    }
}

#[test]
fn all_thirteen_phases_choose_the_retail_factory_and_callback() {
    let factories = [
        (ScreenFactory::Startup, 0x00433810, T::Gameplay),
        (ScreenFactory::Title, 0x00404a10, T::Idle),
        (ScreenFactory::Frontend, 0x00483430, T::Idle),
        (ScreenFactory::Phase3, 0x0040a2e0, T::Idle),
        (ScreenFactory::Gameplay, 0x00440c60, T::Gameplay),
        (ScreenFactory::Phase5, 0x00440d20, T::Player),
        (ScreenFactory::Gameplay, 0x00440c60, T::Player),
        (ScreenFactory::Phase7, 0x00450fc0, T::Player),
        (ScreenFactory::Phase8, 0x0042a970, T::Idle),
        (ScreenFactory::TextEntry, 0x00459000, T::Idle),
        (ScreenFactory::TextEntry, 0x00459000, T::Player),
        (ScreenFactory::TextEntry, 0x00459000, T::Player),
        (ScreenFactory::Phase12, 0x00444ac0, T::Idle),
    ];
    for (id, (factory, address, tick)) in factories.into_iter().enumerate() {
        let phase = P::try_from(id as u32).unwrap();
        let mut controller = controller(P::Title, T::Gameplay);
        let mut host = Host::default();
        let result = transition(&mut controller, &mut host, phase);
        assert_eq!(result.requested, phase);
        assert_eq!(result.effective, phase);
        assert_eq!(result.created, Some(factory));
        assert_eq!(factory.retail_address(), address);
        assert!(host.calls().contains(&Call::Create(factory, 100)));
        assert_eq!(controller.state().current_phase(), phase);
        assert_eq!(controller.state().tick_callback(), tick, "phase {id}");
        assert!(!controller.state().transition_pending());
    }
    assert!(P::try_from(13).is_err());
    assert!(P::try_from(u32::MAX).is_err());
}

#[test]
fn network_phase_five_six_seven_preserve_each_existing_callback() {
    for phase in [P::Phase5, P::Phase6, P::Phase7] {
        for initial in [T::Idle, T::Gameplay, T::Player] {
            let mut controller = controller(P::Gameplay, initial);
            let mut host = Host {
                network: true,
                ..Host::default()
            };
            transition(&mut controller, &mut host, phase);
            assert_eq!(controller.state().tick_callback(), initial);
        }
    }
    for phase in [P::Phase10, P::Phase11] {
        let mut controller = controller(P::Gameplay, T::Gameplay);
        let mut host = Host {
            network: true,
            ..Host::default()
        };
        transition(&mut controller, &mut host, phase);
        assert_eq!(controller.state().tick_callback(), T::Player);
    }
}

#[test]
fn capture_occurs_after_kind_query_before_reuse_and_survives_screen_deletion() {
    let mut controller = SessionController::default();
    let mut host = Host {
        context_after_kind_query: Some(200),
        context_after_reuse: Some(300),
        ..Host::default()
    };
    transition(&mut controller, &mut host, P::Title);
    assert_eq!(
        host.calls(),
        [
            Call::Kind(1),
            Call::CaptureContext(1, 200),
            Call::Reuse(1, P::Title),
            Call::Destroy(1),
            Call::Create(ScreenFactory::Title, 200),
            Call::Effect(E::ResetInputBaselines),
        ]
    );
    assert_eq!(host.context, 200);
}

#[test]
fn phase_nine_reuse_receives_original_request_then_rewrite_precedes_load() {
    for reuse in [false, true] {
        let mut controller = controller(P::Frontend, T::Gameplay);
        let mut host = Host {
            reuse,
            network: true,
            ..Host::default()
        };
        let result = transition(&mut controller, &mut host, P::Phase9);
        assert_eq!(result.requested, P::Phase9);
        assert_eq!(result.effective, P::Phase3);
        assert_eq!(result.reuse_accepted, reuse);
        assert_eq!(
            result.created,
            if reuse {
                None
            } else {
                Some(ScreenFactory::Phase3)
            }
        );
        assert!(host.calls().contains(&Call::Reuse(1, P::Phase9)));
        let effects = host.effects();
        assert_eq!(
            effects
                .iter()
                .map(|(effect, _)| *effect)
                .collect::<Vec<_>>(),
            [E::PreparePhase9, E::LoadWorld, E::ResetInputBaselines,]
        );
        assert_eq!(effects[0].1.requested, P::Phase9);
        assert_eq!(effects[0].1.tick, T::Gameplay);
        assert_eq!(effects[1].1.requested, P::Phase3);
        assert_eq!(effects[1].1.tick, T::Idle);
        for (_, state) in effects {
            assert_eq!(state.current, P::Frontend);
            assert!(state.pending);
        }
        assert_eq!(controller.state().current_phase(), P::Phase3);
        assert_eq!(controller.state().requested_phase(), P::Phase3);
    }
}

#[test]
fn phase_nine_reads_network_state_after_prepare_callback() {
    for initial in [false, true] {
        let mut controller = controller(P::Frontend, T::Gameplay);
        let mut host = Host {
            network: initial,
            change_network: Some((E::PreparePhase9, !initial)),
            ..Host::default()
        };
        let result = transition(&mut controller, &mut host, P::Phase9);
        assert_eq!(
            result.effective,
            if initial { P::Phase9 } else { P::Phase3 }
        );
        assert_eq!(
            host.effects()
                .iter()
                .any(|(effect, _)| *effect == E::LoadWorld),
            !initial
        );
    }
}

#[test]
fn previous_phase_controls_world_effects_and_callback_change_order() {
    for previous in P::ALL {
        for requested in [P::Frontend, P::Phase3, P::Gameplay, P::Phase8, P::Phase9] {
            let mut controller = controller(previous, T::Gameplay);
            let mut host = Host {
                reuse: true,
                ..Host::default()
            };
            transition(&mut controller, &mut host, requested);
            let mut expected = Vec::new();
            match requested {
                P::Frontend => {
                    if previous != P::Title && previous != P::Phase12 {
                        expected.push((E::LeaveWorld, T::Idle));
                    }
                    expected.push((E::SelectFrontendMusic, T::Idle));
                }
                P::Phase3 => {
                    if previous == P::Frontend {
                        expected.push((E::PrepareFromFrontend, T::Gameplay));
                    }
                    if previous == P::Gameplay {
                        expected.push((E::PrepareResults, T::Gameplay));
                    }
                    expected.push((E::LoadWorld, T::Gameplay));
                }
                P::Gameplay => {
                    match previous {
                        P::Phase3 | P::Phase8 => expected.push((E::SelectWorldMusic, T::Gameplay)),
                        P::Phase5 | P::Phase6 => expected.push((E::ResumeWorldMusic, T::Gameplay)),
                        _ => {}
                    }
                    expected.push((
                        E::SetPlayerTimer {
                            microseconds: 500_000,
                        },
                        T::Gameplay,
                    ));
                }
                P::Phase8 => expected.push((E::PrepareResults, T::Gameplay)),
                P::Phase9 => {
                    if previous == P::Frontend {
                        expected.push((E::PreparePhase9, T::Gameplay));
                    }
                }
                _ => unreachable!(),
            }
            expected.push((
                E::ResetInputBaselines,
                if requested == P::Gameplay {
                    T::Gameplay
                } else {
                    T::Idle
                },
            ));
            let observed = host
                .effects()
                .iter()
                .map(|(effect, state)| (*effect, state.tick))
                .collect::<Vec<_>>();
            assert_eq!(observed, expected, "{previous:?} -> {requested:?}");
            assert!(!host
                .calls()
                .iter()
                .any(|call| matches!(call, Call::Create(..))));
        }
    }
    // The timer callback observes the old tick mode, before gameplay is selected.
    let mut controller = controller(P::Phase3, T::Player);
    let mut host = Host::default();
    transition(&mut controller, &mut host, P::Gameplay);
    assert_eq!(
        host.effects()
            .iter()
            .map(|(effect, state)| (*effect, state.tick))
            .collect::<Vec<_>>(),
        [
            (E::SelectWorldMusic, T::Player),
            (
                E::SetPlayerTimer {
                    microseconds: 500_000
                },
                T::Player
            ),
            (E::ResetInputBaselines, T::Gameplay),
        ]
    );
}

#[test]
fn refusing_reuse_still_skips_factory_if_another_screen_remains() {
    let mut controller = SessionController::default();
    let mut host = Host {
        retained_after_destroy: Some(77),
        ..Host::default()
    };
    let result = transition(&mut controller, &mut host, P::Frontend);
    assert!(!result.reuse_accepted);
    assert_eq!(result.created, None);
    assert_eq!(host.screen, Some(77));
    assert!(host.calls().contains(&Call::Destroy(1)));
    assert!(host.calls().contains(&Call::Effect(E::SelectFrontendMusic)));
    assert!(!host
        .calls()
        .iter()
        .any(|call| matches!(call, Call::Create(..))));
}

#[test]
fn pending_request_wins_over_every_reentrant_callback_including_factory() {
    let mut controller = controller(P::Frontend, T::Gameplay);
    let mut host = Host {
        network: true,
        request_in_callbacks: Some(P::Phase12),
        ..Host::default()
    };
    assert!(controller.request_phase(P::Phase9));
    assert!(!controller.request_phase(P::Title));
    controller.transition(&mut host).unwrap().unwrap();
    assert!(host.request_results.iter().all(|(_, accepted)| !accepted));
    assert!(host
        .request_results
        .iter()
        .any(|(call, _)| *call == Call::Create(ScreenFactory::Phase3, 100)));
    let final_event = host.events.borrow().last().copied().unwrap();
    assert_eq!(final_event.call, Call::Effect(E::ResetInputBaselines));
    assert_eq!(
        final_event.state,
        Some(Snapshot {
            current: P::Frontend,
            requested: P::Phase3,
            pending: true,
            tick: T::Idle,
        })
    );
    assert_eq!(controller.state().current_phase(), P::Phase3);
    assert!(!controller.state().transition_pending());
    assert!(
        controller.request_phase(P::Phase3),
        "same-phase requests are accepted after commit"
    );
}

#[test]
fn startup_factory_keeps_constructor_phase_one_and_can_request_that_same_phase() {
    let mut controller = SessionController::default();
    let mut host = Host {
        screen: None,
        request_in_callbacks: Some(P::Title),
        ..Host::default()
    };
    controller.create_startup_screen(&mut host, 91).unwrap();
    assert_eq!(host.calls(), [Call::Create(ScreenFactory::Startup, 91)]);
    assert_eq!(
        host.request_results,
        [(Call::Create(ScreenFactory::Startup, 91), true)]
    );
    assert_eq!(controller.state().current_phase(), P::Title);
    assert_eq!(controller.state().requested_phase(), P::Title);
    assert!(controller.state().transition_pending());
    assert_eq!(controller.state().tick_callback(), T::Idle);
    assert!(matches!(
        controller.create_startup_screen(&mut host, 92),
        Err(SessionError::ScreenAlreadyPresent)
    ));
    assert_eq!(host.calls().len(), 1);
}

#[test]
fn no_pending_transition_does_nothing_and_missing_screen_fails_before_callbacks() {
    let mut controller = SessionController::default();
    let mut host = Host {
        screen: None,
        ..Host::default()
    };
    assert!(controller.transition(&mut host).unwrap().is_none());
    assert!(host.calls().is_empty());
    assert!(controller.request_phase(P::Frontend));
    assert!(matches!(
        controller.transition(&mut host),
        Err(SessionError::MissingScreen)
    ));
    assert!(host.calls().is_empty());
    assert!(matches!(
        controller.transition(&mut host),
        Err(SessionError::Stopped)
    ));
    assert!(!controller.request_phase(P::Title));
}

#[test]
fn callback_failure_stops_without_replaying_partial_effects_or_committing() {
    let failures = [
        Call::Kind(1),
        Call::Reuse(1, P::Phase3),
        Call::Destroy(1),
        Call::Effect(E::PrepareFromFrontend),
        Call::Effect(E::LoadWorld),
        Call::Create(ScreenFactory::Phase3, 100),
        Call::Effect(E::ResetInputBaselines),
    ];
    for failure in failures {
        let mut controller = controller(P::Frontend, T::Gameplay);
        let mut host = Host {
            fail_at: Some(failure),
            ..Host::default()
        };
        assert!(controller.request_phase(P::Phase3));
        assert!(matches!(
            controller.transition(&mut host),
            Err(SessionError::Host("injected callback failure"))
        ));
        assert_eq!(host.calls().last(), Some(&failure));
        assert_eq!(controller.state().current_phase(), P::Frontend);
        assert_eq!(controller.state().requested_phase(), P::Phase3);
        assert!(controller.state().transition_pending());
        let partial_calls = host.calls();
        let partial_screen = host.screen;
        let partial_state = controller.state().clone();
        assert!(matches!(
            controller.transition(&mut host),
            Err(SessionError::Stopped)
        ));
        assert!(matches!(
            controller.tick(&mut host, 1, 2),
            Err(SessionError::Stopped)
        ));
        assert!(matches!(
            controller.create_startup_screen(&mut host, 200),
            Err(SessionError::Stopped)
        ));
        assert!(!controller.request_phase(P::Title));
        assert_eq!(host.calls(), partial_calls);
        assert_eq!(host.screen, partial_screen);
        assert_eq!(controller.state(), &partial_state);
    }
}

#[test]
fn phase_nine_failure_retains_rewrite_and_callback_without_claiming_commit() {
    let mut controller = controller(P::Frontend, T::Gameplay);
    let mut host = Host {
        network: true,
        fail_at: Some(Call::Effect(E::LoadWorld)),
        ..Host::default()
    };
    assert!(controller.request_phase(P::Phase9));
    assert!(matches!(
        controller.transition(&mut host),
        Err(SessionError::Host(_))
    ));
    assert_eq!(controller.state().current_phase(), P::Frontend);
    assert_eq!(controller.state().requested_phase(), P::Phase3);
    assert_eq!(controller.state().tick_callback(), T::Idle);
    assert!(controller.state().transition_pending());
    assert_eq!(host.screen, None, "the completed destructor is not undone");
}

#[test]
fn restoring_mission_seconds_keeps_the_native_scheduler_remainder() {
    let mut state = SessionState::default();
    state.gameplay_time.advance(1_125_000);
    state.restore_mission_elapsed_seconds(73);
    assert_eq!(
        state.gameplay_time(),
        GameplayTime {
            seconds: 73,
            remainder_micros: 125_000,
        }
    );
    state.restore_mission_elapsed_seconds(0);
    assert_eq!(
        state.gameplay_time(),
        GameplayTime {
            seconds: 0,
            remainder_micros: 125_000,
        }
    );
}

#[test]
fn gameplay_time_uses_strict_signed_threshold_and_wrapping_native_words() {
    let mut time = GameplayTime::default();
    time.advance(1_000_000);
    assert_eq!(
        time,
        GameplayTime {
            seconds: 0,
            remainder_micros: 1_000_000
        }
    );
    time.advance(0);
    assert_eq!(time.seconds, 0);
    time.advance(1);
    assert_eq!(
        time,
        GameplayTime {
            seconds: 1,
            remainder_micros: 1
        }
    );
    time.advance(2_000_000);
    assert_eq!(
        time,
        GameplayTime {
            seconds: 3,
            remainder_micros: 1
        }
    );
    time.advance(-2);
    assert_eq!(time.remainder_micros, -1);
    time = GameplayTime {
        seconds: u32::MAX,
        remainder_micros: 1_000_000,
    };
    time.advance(1);
    assert_eq!(
        time,
        GameplayTime {
            seconds: 0,
            remainder_micros: 1
        }
    );
    time.remainder_micros = i32::MAX;
    time.advance(1);
    assert_eq!(
        time,
        GameplayTime {
            seconds: 0,
            remainder_micros: i32::MIN
        }
    );
}

#[test]
fn ticks_forward_both_arguments_but_only_successful_gameplay_advances_time() {
    for callback in [T::Idle, T::Player, T::Gameplay] {
        let mut controller = controller(P::Gameplay, callback);
        let mut host = Host::default();
        controller.tick(&mut host, 1_000_001, 2_000_003).unwrap();
        assert_eq!(controller.state().last_delta_micros(), 1_000_001);
        if callback == T::Idle {
            assert!(host.calls().is_empty());
        } else {
            assert_eq!(host.calls(), [Call::Tick(callback, 1_000_001, 2_000_003)]);
        }
        assert_eq!(
            controller.state().gameplay_time(),
            if callback == T::Gameplay {
                GameplayTime {
                    seconds: 1,
                    remainder_micros: 1,
                }
            } else {
                GameplayTime::default()
            }
        );
    }
    let mut controller = controller(P::Gameplay, T::Gameplay);
    let mut host = Host {
        fail_at: Some(Call::Tick(T::Gameplay, 1_000_001, 2_000_003)),
        ..Host::default()
    };
    assert!(matches!(
        controller.tick(&mut host, 1_000_001, 2_000_003),
        Err(SessionError::Host(_))
    ));
    assert_eq!(controller.state().gameplay_time(), GameplayTime::default());
    assert_eq!(controller.state().last_delta_micros(), 1_000_001);
    assert!(matches!(
        controller.tick(&mut host, 1, 2),
        Err(SessionError::Stopped)
    ));
}

#[test]
fn tick_requests_next_transition_and_phase_changes_retain_gameplay_remainder() {
    let mut controller = controller(P::Gameplay, T::Gameplay);
    let mut host = Host {
        request_in_callbacks: Some(P::Phase5),
        ..Host::default()
    };
    controller.tick(&mut host, 900_000, 950_000).unwrap();
    assert_eq!(
        host.request_results,
        [(Call::Tick(T::Gameplay, 900_000, 950_000), true)]
    );
    assert_eq!(controller.state().requested_phase(), P::Phase5);
    assert!(!controller.request_phase(P::Phase12));
    controller.transition(&mut host).unwrap();
    assert_eq!(controller.state().gameplay_time().remainder_micros, 900_000);
    assert_eq!(controller.state().tick_callback(), T::Player);
    host.request_in_callbacks = None;
    controller.tick(&mut host, 900_000, 900_000).unwrap();
    assert_eq!(controller.state().gameplay_time().remainder_micros, 900_000);
    transition(&mut controller, &mut host, P::Gameplay);
    controller.tick(&mut host, 100_000, 100_000).unwrap();
    assert_eq!(
        controller.state().gameplay_time(),
        GameplayTime {
            seconds: 0,
            remainder_micros: 1_000_000
        }
    );
    controller.tick(&mut host, 1, 1).unwrap();
    assert_eq!(
        controller.state().gameplay_time(),
        GameplayTime {
            seconds: 1,
            remainder_micros: 1
        }
    );
}

#[test]
fn scheduled_input_changes_same_frame_tick_and_late_screen_request_waits_for_next_frame() {
    let host = Host {
        input_request: Some(P::Gameplay),
        ..Host::default()
    };
    let mut session = ScheduledSession::new(host, 0);
    assert!(session.first_pass(7_999).unwrap().is_none());
    session.second_pass().unwrap();
    assert!(session.host().calls().is_empty());

    let tick = SchedulerTick {
        delta_micros: 8_000,
        elapsed_micros: 8_000,
    };
    assert_eq!(session.first_pass(8_000).unwrap(), Some(tick));
    assert_eq!(
        session.host().calls(),
        [
            Call::PollInput,
            Call::Kind(1),
            Call::CaptureContext(1, 100),
            Call::Reuse(1, P::Gameplay),
            Call::Destroy(1),
            Call::Effect(E::SetPlayerTimer {
                microseconds: 500_000
            }),
            Call::Create(ScreenFactory::Gameplay, 100),
            Call::Effect(E::ResetInputBaselines),
            Call::Tick(T::Gameplay, 8_000, 8_000),
        ]
    );
    assert_eq!(session.state().current_phase(), P::Gameplay);
    session.host_mut().linked_08_request = Some(P::Phase5);
    session.host().events.borrow_mut().clear();
    session.second_pass().unwrap();
    assert_eq!(
        session.host().calls(),
        [Call::LinkedTick(tick), Call::Linked08, Call::Linked0c]
    );
    assert_eq!(session.state().current_phase(), P::Gameplay);
    assert_eq!(session.state().requested_phase(), P::Phase5);
    assert!(session.state().transition_pending());

    session.host().events.borrow_mut().clear();
    assert!(session.first_pass(15_999).unwrap().is_none());
    session.second_pass().unwrap();
    assert!(session.host().calls().is_empty());
    assert!(session.state().transition_pending());
    assert_eq!(session.first_pass(16_000).unwrap(), Some(tick));
    assert_eq!(session.state().current_phase(), P::Phase5);
    assert_eq!(
        session.host().calls().last(),
        Some(&Call::Tick(T::Player, 8_000, 8_000))
    );
    assert_eq!(session.state().gameplay_time().remainder_micros, 8_000);
    session.second_pass().unwrap();
    assert_eq!(
        &session.host().calls()[session.host().calls().len() - 3..],
        [Call::LinkedTick(tick), Call::Linked08, Call::Linked0c]
    );
}

#[test]
fn scheduled_activity_updates_flag_before_linked_owner_without_pausing_clock() {
    let mut session = ScheduledSession::new(Host::default(), 0);
    assert!(session.request_phase(P::Gameplay));
    session.first_pass(8_000).unwrap();
    session.second_pass().unwrap();
    session.host().events.borrow_mut().clear();

    session.activity(SchedulerActivity::Suspend).unwrap();
    assert_eq!(
        session.host().calls(),
        [Call::Activity(SchedulerActivity::Suspend, true)]
    );
    assert!(session.state().suspended());
    assert!(!session.host().input_selected);
    session.host().events.borrow_mut().clear();
    let suspended_tick = SchedulerTick {
        delta_micros: 125_000,
        elapsed_micros: 1_000_000,
    };
    assert_eq!(session.first_pass(1_008_000).unwrap(), Some(suspended_tick));
    session.second_pass().unwrap();
    assert_eq!(
        session.host().calls(),
        [
            Call::Tick(T::Gameplay, 125_000, 1_000_000),
            Call::LinkedTick(suspended_tick),
            Call::Linked08,
            Call::Linked0c,
        ]
    );
    assert_eq!(session.state().gameplay_time().remainder_micros, 133_000);

    session.host().events.borrow_mut().clear();
    session.activity(SchedulerActivity::Resume).unwrap();
    assert_eq!(
        session.host().calls(),
        [Call::Activity(SchedulerActivity::Resume, false)]
    );
    assert!(!session.state().suspended());
    assert!(session.host().input_selected);
    session.host().events.borrow_mut().clear();
    assert_eq!(
        session.first_pass(1_016_000).unwrap(),
        Some(SchedulerTick {
            delta_micros: 8_000,
            elapsed_micros: 8_000
        })
    );
    assert_eq!(
        session.host().calls(),
        [Call::PollInput, Call::Tick(T::Gameplay, 8_000, 8_000)]
    );
}

#[test]
fn scheduled_host_failures_stop_all_later_passes_and_activity() {
    for failure in [
        Call::PollInput,
        Call::LinkedTick(SchedulerTick {
            delta_micros: 8_000,
            elapsed_micros: 8_000,
        }),
        Call::Linked08,
        Call::Linked0c,
        Call::Activity(SchedulerActivity::Suspend, true),
    ] {
        let host = Host {
            fail_at: Some(failure),
            ..Host::default()
        };
        let mut session = ScheduledSession::new(host, 0);
        let result = match failure {
            Call::PollInput => session.first_pass(8_000).map(|_| ()),
            Call::Activity(..) => session.activity(SchedulerActivity::Suspend),
            _ => {
                session.first_pass(8_000).unwrap();
                session.second_pass()
            }
        };
        assert!(matches!(
            result,
            Err(SessionError::Host("injected callback failure"))
        ));
        assert_eq!(session.host().calls().last(), Some(&failure));
        let partial_calls = session.host().calls();
        assert!(matches!(
            session.first_pass(16_000),
            Err(SessionError::Stopped)
        ));
        assert!(matches!(session.second_pass(), Err(SessionError::Stopped)));
        assert!(matches!(
            session.activity(SchedulerActivity::Resume),
            Err(SessionError::Stopped)
        ));
        assert!(!session.request_phase(P::Title));
        assert_eq!(session.host().calls(), partial_calls);
    }
}

#[test]
fn port_admission_runs_only_for_pending_requests_before_the_screen_is_touched() {
    let mut session = ScheduledSession::new(Host::default(), 0);
    session.first_pass(8_000).unwrap();
    assert!(session.host().admissions.is_empty());
    session.second_pass().unwrap();

    session.host().events.borrow_mut().clear();
    assert!(session.request_phase(P::Gameplay));
    session.first_pass(16_000).unwrap();
    // PollInput is recorded first; admission precedes the +18 kind query.
    assert_eq!(session.host().admissions, [(P::Gameplay, 1)]);
    assert_eq!(session.host().calls()[1], Call::Kind(1));

    let mut refused = ScheduledSession::new(
        Host {
            refuse_admission: Some(P::Phase3),
            ..Host::default()
        },
        0,
    );
    assert!(refused.request_phase(P::Phase3));
    assert!(matches!(
        refused.first_pass(8_000),
        Err(SessionError::Host("port refused transition"))
    ));
    // The current screen was neither queried, reused nor destroyed.
    assert_eq!(refused.host().calls(), [Call::PollInput]);
    assert_eq!(refused.host().screen, Some(1));
    assert!(refused.state().transition_pending());
    assert!(matches!(
        refused.first_pass(16_000),
        Err(SessionError::Stopped)
    ));
}

#[test]
fn deferred_admission_cancels_only_the_request_and_preserves_the_scheduled_callbacks() {
    let mut session = ScheduledSession::new(
        Host {
            defer_admission: Some(P::Phase9),
            ..Host::default()
        },
        0,
    );
    // No pending request: even a host configured to defer is never consulted.
    session.first_pass(8_000).unwrap();
    session.second_pass().unwrap();
    assert!(session.host().admissions.is_empty());
    session.host().events.borrow_mut().clear();

    assert!(session.request_phase(P::Phase9));
    assert!(
        !session.request_phase(P::Gameplay),
        "the first request still wins"
    );
    session.host_mut().input_request = Some(P::Phase3);
    session.first_pass(16_000).unwrap();
    assert_eq!(
        session.host().request_results.last(),
        Some(&(Call::PollInput, false))
    );
    assert_eq!(session.host().calls(), [Call::PollInput]);
    assert_eq!(session.host().admissions, [(P::Phase9, 1)]);
    assert_eq!(session.host().screen, Some(1));
    assert_eq!(session.host().context, 100);
    assert_eq!(session.state().current_phase(), P::Title);
    assert_eq!(session.state().requested_phase(), P::Title);
    assert_eq!(session.state().tick_callback(), T::Idle);
    assert!(!session.state().transition_pending());
    session.second_pass().unwrap();
    assert_eq!(
        session.host().calls(),
        [
            Call::PollInput,
            Call::LinkedTick(SchedulerTick {
                delta_micros: 8_000,
                elapsed_micros: 8_000
            }),
            Call::Linked08,
            Call::Linked0c,
        ]
    );

    // Cancellation does not stop the controller or pin it to the old phase.
    session.host().events.borrow_mut().clear();
    assert!(session.request_phase(P::Gameplay));
    session.first_pass(24_000).unwrap();
    assert_eq!(session.state().current_phase(), P::Gameplay);
    assert_eq!(session.host().screen, Some(2));
    assert!(session.host().calls().contains(&Call::Destroy(1)));
}

#[test]
fn an_admission_error_never_uses_the_deferred_cancellation_path() {
    let mut session = ScheduledSession::new(
        Host {
            refuse_admission: Some(P::Phase9),
            defer_admission: Some(P::Phase9),
            ..Host::default()
        },
        0,
    );
    assert!(session.request_phase(P::Phase9));
    assert!(matches!(
        session.first_pass(8_000),
        Err(SessionError::Host("port refused transition"))
    ));
    assert!(session.state().transition_pending());
    assert_eq!(session.state().requested_phase(), P::Phase9);
    assert_eq!(session.host().calls(), [Call::PollInput]);
    assert_eq!(session.host().screen, Some(1));
    assert!(matches!(session.second_pass(), Err(SessionError::Stopped)));
}

#[test]
fn direct_frontend_entry_runs_the_title_to_frontend_tail() {
    let mut driver = SessionDriver::new(0);
    let mut host = Host {
        screen: None,
        ..Host::default()
    };
    let transition = driver.enter_frontend_directly(&mut host, 7).unwrap();
    assert_eq!(
        host.calls(),
        [
            Call::Effect(E::SelectFrontendMusic),
            Call::Create(ScreenFactory::Frontend, 7),
            Call::Effect(E::ResetInputBaselines),
        ]
    );
    // The music selection observes the pending title->frontend request.
    assert_eq!(
        host.effects()[0].1,
        Snapshot {
            current: P::Title,
            requested: P::Frontend,
            pending: true,
            tick: T::Idle
        }
    );
    assert_eq!(
        transition,
        PhaseTransition {
            previous: P::Title,
            requested: P::Frontend,
            effective: P::Frontend,
            reuse_accepted: false,
            created: Some(ScreenFactory::Frontend),
        }
    );
    assert_eq!(driver.transitions(), [transition]);
    assert_eq!(driver.state().current_phase(), P::Frontend);
    assert!(!driver.state().transition_pending());
    assert!(matches!(
        driver.enter_frontend_directly(&mut host, 7),
        Err(SessionError::DirectEntry)
    ));

    let mut pending = SessionDriver::new(0);
    assert!(pending.request_phase(P::Frontend));
    assert!(matches!(
        pending.enter_frontend_directly(
            &mut Host {
                screen: None,
                ..Host::default()
            },
            7
        ),
        Err(SessionError::DirectEntry)
    ));
    let mut occupied = SessionDriver::new(0);
    assert!(matches!(
        occupied.enter_frontend_directly(&mut Host::default(), 7),
        Err(SessionError::ScreenAlreadyPresent)
    ));
    // A failed factory stops the controller like any other callback.
    let mut failing = SessionDriver::new(0);
    let mut host = Host {
        screen: None,
        fail_at: Some(Call::Create(ScreenFactory::Frontend, 7)),
        ..Host::default()
    };
    assert!(matches!(
        failing.enter_frontend_directly(&mut host, 7),
        Err(SessionError::Host("injected callback failure"))
    ));
    assert!(failing.transitions().is_empty());
    assert!(matches!(
        failing.first_pass(8_000, &mut host),
        Err(SessionError::Stopped)
    ));
}

#[test]
fn split_linked_pass_matches_second_pass_and_rejects_misordered_calls() {
    let tick = SchedulerTick {
        delta_micros: 8_000,
        elapsed_micros: 8_000,
    };
    let mut driver = SessionDriver::new(0);
    let mut host = Host::default();
    // Before any admitted first pass the split calls are empty.
    driver.linked_update(&mut host).unwrap();
    driver.linked_present(&mut host).unwrap();
    assert!(host.calls().is_empty());

    driver.first_pass(8_000, &mut host).unwrap();
    host.events.borrow_mut().clear();
    assert!(matches!(
        driver.linked_present(&mut host),
        Err(SessionError::LinkedPassOrder)
    ));
    driver.linked_update(&mut host).unwrap();
    assert_eq!(host.calls(), [Call::LinkedTick(tick)]);
    assert!(matches!(
        driver.linked_update(&mut host),
        Err(SessionError::LinkedPassOrder)
    ));
    driver.linked_present(&mut host).unwrap();
    assert_eq!(
        host.calls(),
        [Call::LinkedTick(tick), Call::Linked08, Call::Linked0c]
    );
    assert!(matches!(
        driver.second_pass(&mut host),
        Err(SessionError::LinkedPassOrder)
    ));

    // A gated first pass leaves an empty second pass, not an ordering error.
    host.events.borrow_mut().clear();
    assert_eq!(driver.first_pass(15_999, &mut host).unwrap(), None);
    driver.second_pass(&mut host).unwrap();
    assert!(host.calls().is_empty());
    assert_eq!(driver.first_pass(16_000, &mut host).unwrap(), Some(tick));
    driver.second_pass(&mut host).unwrap();
    assert_eq!(
        &host.calls()[1..],
        [Call::LinkedTick(tick), Call::Linked08, Call::Linked0c]
    );
    // Ordering errors are misuse, not callback failures: the session runs on.
    assert!(driver.first_pass(24_000, &mut host).unwrap().is_some());
}

#[test]
fn a_new_first_pass_cannot_drop_pending_linked_callbacks_or_advance_a_transition() {
    for update_already_ran in [false, true] {
        let mut driver = SessionDriver::new(0);
        let mut host = Host::default();
        driver.first_pass(8_000, &mut host).unwrap();
        if update_already_ran {
            driver.linked_update(&mut host).unwrap();
        }
        assert!(driver.request_phase(P::Gameplay));
        let calls = host.calls();

        // Even a gated attempt must retain this iteration's linked work.
        for clock in [15_999, 16_000] {
            assert!(matches!(
                driver.first_pass(clock, &mut host),
                Err(SessionError::LinkedPassOrder)
            ));
            assert_eq!(host.calls(), calls);
            assert_eq!(driver.state().current_phase(), P::Title);
            assert!(driver.state().transition_pending());
        }

        if !update_already_ran {
            driver.linked_update(&mut host).unwrap();
        }
        driver.linked_present(&mut host).unwrap();
        assert_eq!(
            &host.calls()[host.calls().len() - 3..],
            [
                Call::LinkedTick(SchedulerTick {
                    delta_micros: 8_000,
                    elapsed_micros: 8_000,
                }),
                Call::Linked08,
                Call::Linked0c,
            ]
        );
        // The refused calls neither stopped the session nor consumed time.
        assert_eq!(
            driver.first_pass(16_000, &mut host).unwrap(),
            Some(SchedulerTick {
                delta_micros: 8_000,
                elapsed_micros: 8_000,
            })
        );
        assert_eq!(driver.state().current_phase(), P::Gameplay);
        assert!(!driver.state().transition_pending());
    }
}
