//! Phase-3 briefing screen: factory `0040A2E0`, constructor `0040A420`.
//!
//! Native lifecycle (see `BRIEFING_RUNTIME.md`):
//! - Factory allocates `0x32C` bytes, constructs over base `00437F90`.
//! - Constructor publishes vtable `00520338`, enumerates devices, attaches
//!   slots, selects context, publishes H6 styles, binds briefing inputs
//!   (world `+0x2C` records to screen `+0x40`, live player to `+0x3B`),
//!   builds menus, with a conditional early return gated on gameplay bytes.
//! - Tick `0040C6A0` finishes through `0040D510` (request phase 4) when
//!   record/flag gates fail at once; the recording's first briefing completes
//!   after a single tick.
//! - Draw `0040B0F0` presents briefing records; full semantics stay open.
//!
//! Authored records bind before ticking. The native gate/timeline owner keeps
//! visible briefings resident while records and menus run; Start arms the
//! shared two-second timer. Renderer submission remains a separate owner.

use inf_formats::BriefingProgram;
use thiserror::Error;

use crate::briefing_camera::{BriefingCameraInputs, BriefingCameraWorld};
use crate::briefing_controls::{BriefingControlError, BriefingControls, BriefingMenuTemplates};
use crate::briefing_runtime::{BriefingGates, BriefingRuntime, BriefingTickOutcome};
use crate::frontend_assets::FrontendAssets;
use crate::frontend_bindings::{FrontendBindingError, FrontendBindings, FrontendDevice};
use crate::input::{InputCallback, InputContextId, InputRuntime, InputStateError};
use crate::menu::{MenuCommand, MenuDrawColors, MenuError};
use crate::menu_basis::MenuBasisMath;
use crate::scheduler::SchedulerTick;
use crate::screen_input::{
    ScreenButton, ScreenDispatchPolicy, ScreenInput, ScreenInputAction, ScreenInputError,
};
use crate::screen_registry::{
    RegistryCallbackError, RegistryError, ScreenAttachment, ScreenId, ScreenLifecycle,
    ScreenMembership, ScreenRegistry,
};
use crate::session::{SessionPhase, SessionState};

/// `0040AA10` reuse rule: the briefing screen serves only requested phase 3.
pub fn briefing_reuses_phase(requested: SessionPhase) -> bool {
    requested == SessionPhase::Phase3
}

/// Opaque callback identity for the briefing screen, distinct from the
/// frontend/text-entry/title contexts.
pub const BRIEFING_CALLBACK_CONTEXT: u32 = 4;

#[derive(Clone, Copy, Debug)]
pub struct BriefingScreenConfig {
    pub attachment: ScreenAttachment,
    pub callback_context: u32,
}

#[derive(Debug, Error)]
pub enum BriefingScreenError<E> {
    #[error("briefing screen requires {expected:?}, currently {actual:?}")]
    Lifecycle {
        expected: ScreenLifecycle,
        actual: ScreenLifecycle,
    },
    #[error("0040A2E0 serves requested phase 3 (Briefing), not {0:?}")]
    RequestedPhase(SessionPhase),
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Bindings(#[from] FrontendBindingError),
    #[error(transparent)]
    Input(#[from] InputStateError),
    #[error("briefing requires the current world's authored record program")]
    MissingProgram,
    #[error("briefing requires both native menu controllers")]
    MissingControls,
    #[error("required briefing Handler-6 color slot {0} is unavailable")]
    ColorResource(u16),
    #[error(transparent)]
    Controls(#[from] MenuError<BriefingControlError>),
    #[error(transparent)]
    ScreenInput(#[from] ScreenInputError<MenuError<BriefingControlError>>),
    #[error(transparent)]
    Terrain(#[from] crate::terrain_runtime::TerrainPolicyError),
    #[error("required briefing effect failed")]
    Host(E),
}

/// Factory effects with no successful defaults.
pub trait BriefingFactoryHost {
    type Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error>;
}

/// Any frontend factory host already owns the shared keyboard adapter the
/// briefing constructor enumerates (`00439B30`). Reuse that enumeration so
/// the briefing screen needs no separate platform trait.
impl<T: crate::frontend_screen::FrontendFactoryHost> BriefingFactoryHost for T {
    type Error = T::Error;
    fn enumerate_calibrated_devices(
        &mut self,
        input: &mut InputRuntime,
    ) -> Result<Vec<FrontendDevice>, Self::Error> {
        <T as crate::frontend_screen::FrontendFactoryHost>::enumerate_calibrated_devices(
            self, input,
        )
    }
}

/// Tick effects: phase requests reach `00438380` through the session state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BriefingAudioRequest {
    ResumeMusic,
    Cue { slot: u32 },
}

pub trait BriefingTickHost {
    type Error;
    /// Cues dispatched by base input precede C770's process RNG draws.
    /// Aliased cue 5 consumes the shared RNG even with fixed pitch.
    fn audio(&mut self, request: BriefingAudioRequest, rng: &mut u32) -> Result<(), Self::Error>;
    fn request_phase(
        &mut self,
        previous: SessionPhase,
        requested: SessionPhase,
    ) -> Result<(), Self::Error>;
}

/// Distinct H14 header inputs for the currently loaded briefing world.
#[derive(Clone, Debug)]
pub struct BriefingWorldBinding {
    pub program: BriefingProgram,
    pub mission_selection: u32,
    /// Header +24, consumed by progress sprites; not record count +28.
    pub milestone_count: u32,
    /// When the list closes, for a port briefing with fewer than retail's
    /// four objectives; None keeps retail's 5000 ms.
    pub list_close_millis: Option<u32>,
}

/// The registry-bound briefing owner.
#[derive(Debug)]
pub struct BriefingScreen {
    config: BriefingScreenConfig,
    lifecycle: ScreenLifecycle,
    id: Option<ScreenId>,
    bindings: FrontendBindings,
    ticks: u64,
    finished: bool,
    runtime: Option<BriefingRuntime>,
    milestone_count: Option<u32>,
    controls: Option<BriefingControls>,
    base_input: ScreenInput,
    basis: MenuBasisMath,
    draw_skips_logged: bool,
}

impl BriefingScreen {
    pub fn new(config: BriefingScreenConfig, assets: &FrontendAssets) -> Self {
        Self {
            bindings: FrontendBindings::new(
                config.callback_context,
                assets.screen_analog_descriptors(),
            ),
            config,
            lifecycle: ScreenLifecycle::New,
            id: None,
            ticks: 0,
            finished: false,
            runtime: None,
            milestone_count: None,
            controls: None,
            base_input: ScreenInput::new(ScreenDispatchPolicy::Base, config.callback_context),
            basis: MenuBasisMath::new(assets.sine_table().values),
            draw_skips_logged: false,
        }
    }

    pub fn lifecycle(&self) -> ScreenLifecycle {
        self.lifecycle
    }

    pub fn id(&self) -> Option<ScreenId> {
        self.id
    }

    pub fn ticks(&self) -> u64 {
        self.ticks
    }

    /// Bind the H14 +28/+2C program belonging to the loaded current world.
    pub fn bind_world(&mut self, world: BriefingWorldBinding, templates: BriefingMenuTemplates) {
        let runtime = BriefingRuntime::new(world.program);
        self.runtime = Some(match world.list_close_millis {
            Some(millis) => runtime.with_list_close(millis),
            None => runtime,
        });
        self.milestone_count = Some(world.milestone_count);
        self.controls = Some(BriefingControls::new(templates, world.mission_selection));
        self.finished = false;
        self.draw_skips_logged = false;
    }

    pub fn runtime(&self) -> Option<&BriefingRuntime> {
        self.runtime.as_ref()
    }

    pub fn milestone_count(&self) -> Option<u32> {
        self.milestone_count
    }

    pub fn runtime_mut(&mut self) -> Option<&mut BriefingRuntime> {
        self.runtime.as_mut()
    }

    pub fn controls(&self) -> Option<&BriefingControls> {
        self.controls.as_ref()
    }
    pub fn controls_mut(&mut self) -> Option<&mut BriefingControls> {
        self.controls.as_mut()
    }

    pub fn mark_draw_skips_logged(&mut self) -> bool {
        let first = !self.draw_skips_logged;
        self.draw_skips_logged = true;
        first
    }

    pub fn digital_callback(
        &mut self,
        event: InputCallback,
    ) -> Result<Vec<u32>, BriefingCallbackError> {
        if event.context != self.config.callback_context {
            return Err(BriefingCallbackError::Context {
                expected: self.config.callback_context,
                actual: event.context,
            });
        }
        if self.lifecycle != ScreenLifecycle::Ready {
            return Err(BriefingCallbackError::NotReady);
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(BriefingCallbackError::MissingProgram)?;
        let controls = self
            .controls
            .as_mut()
            .ok_or(BriefingCallbackError::MissingControls)?;
        let mut cues = Vec::new();
        match event.address {
            0x0040_D660 => {
                if event.value != 0 {
                    runtime.arm_start();
                }
            }
            0x0040_D670 | 0x0040_D680 | 0x0040_D690 => {
                if event.value != 0 {
                    let action = match event.address {
                        0x0040_D670 => ScreenInputAction::Confirm,
                        0x0040_D680 => ScreenInputAction::Previous,
                        _ => ScreenInputAction::Next,
                    };
                    dispatch_action(action, controls, runtime, &self.basis, &mut cues)?;
                }
            }
            address if ScreenButton::from_callback(address).is_some() => {
                self.base_input.digital_callback(event, &mut |_, action| {
                    dispatch_action(action, controls, runtime, &self.basis, &mut cues)
                })?;
            }
            address => return Err(BriefingCallbackError::UnknownCallback(address)),
        }
        Ok(cues)
    }

    /// Constructor `0040A420` plus factory video/selection, in native order:
    /// scheduler membership, base input, link, device enumeration, authored
    /// phase-3 tables, context selection. Only requested phase 3 reaches
    /// retail construction. The host binds the retained world program and
    /// native menu templates through `bind_world` before ticking the screen.
    pub fn initialize<H: BriefingFactoryHost>(
        &mut self,
        services: &mut BriefingFactoryServices<'_>,
        host: &mut H,
        state: &mut SessionState,
    ) -> Result<(), BriefingScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::New)?;
        if state.requested_phase() != SessionPhase::Phase3 {
            return Err(BriefingScreenError::RequestedPhase(state.requested_phase()));
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        let id = services
            .registry
            .attach_scheduler_member(self.config.attachment)?;
        self.id = Some(id);
        self.bindings.initialize(services.input)?;
        services.registry.link_screen_membership(id)?;
        let devices = host
            .enumerate_calibrated_devices(services.input)
            .map_err(BriefingScreenError::Host)?;
        self.bindings.install(
            services.input,
            services.assets.phase3_input_tables(),
            &devices,
        )?;
        let context = self.bindings.context().expect("installed briefing context");
        services
            .registry
            .bind_input(id, context, |context| {
                services.input.select_context(context)
            })
            .map_err(|error| match error {
                RegistryCallbackError::Registry(error) => BriefingScreenError::Registry(error),
                RegistryCallbackError::Callback(error) => BriefingScreenError::Input(error),
            })?;
        // 0040A420 / 00453E10: selected start, ordinary, selected end.
        // The literal slot-4 lookup is inlined in the native constructor.
        let color = |slot| {
            crate::frontend_screen::resident_menu_color(services.residency, slot)
                .ok_or(BriefingScreenError::ColorResource(slot))
        };
        let ordinary = color(5)?;
        let selected_start = color(4)?;
        let selected_end = color(6)?;
        services.process.publish_colors(MenuDrawColors {
            selected_start,
            ordinary,
            selected_end,
        });
        //40A987..98E force coarse AFTER the world/control/color prefix,
        //before439530 resets the shared animation clock.
        services.terrain.configure(0)?;
        services.process.reset_animation();
        self.lifecycle = ScreenLifecycle::Ready;
        Ok(())
    }

    /// Teardown `0040AA40` (scalar deleting `0040AA20`): reset vtable, clear
    /// player word, release binding sets, destroy menu controller, destroy
    /// base. No video handle to close.
    pub fn destroy<D, H>(
        &mut self,
        registry: &mut ScreenRegistry<D, InputContextId>,
        input: &mut InputRuntime,
    ) -> Result<(), BriefingScreenError<H>> {
        if self.lifecycle == ScreenLifecycle::Destroyed {
            return Ok(());
        }
        self.lifecycle = ScreenLifecycle::Stopped;
        self.bindings.release_sets(input)?;
        if let Some(id) = self.id {
            if registry.screen(id)?.membership == ScreenMembership::Linked {
                registry.unlink_screen_membership(id)?;
            }
        }
        self.bindings.release_base(input)?;
        if let Some(id) = self.id {
            registry.remove_screen(id)?;
            self.id = None;
        }
        self.lifecycle = ScreenLifecycle::Destroyed;
        Ok(())
    }

    /// Native tick gates and authored record timing. The host executes
    /// returned presentation/audio/controller effects; completion still
    /// reaches the session's first-request-wins phase request.
    pub fn tick<H: BriefingTickHost>(
        &mut self,
        tick: SchedulerTick,
        gates: BriefingGates,
        rng: &mut u32,
        input: &InputRuntime,
        camera_world: Option<BriefingCameraWorld<'_>>,
        host: &mut H,
    ) -> Result<BriefingTickOutcome, BriefingScreenError<H::Error>> {
        self.require_lifecycle(ScreenLifecycle::Ready)?;
        if self.finished {
            return Ok(BriefingTickOutcome::default());
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(BriefingScreenError::MissingProgram)?;
        let controls = self
            .controls
            .as_mut()
            .ok_or(BriefingScreenError::MissingControls)?;
        self.ticks += 1;
        let mut cues = Vec::new();
        if runtime.tick_finish_reason(gates).is_none() {
            if runtime.music_resume_pending() {
                host.audio(BriefingAudioRequest::ResumeMusic, rng)
                    .map_err(BriefingScreenError::Host)?;
                runtime.acknowledge_music_resume();
            }
            let analog = self.bindings.analog_values(input)?;
            self.base_input.tick(tick, analog, &mut |_, action| {
                dispatch_action(action, controls, runtime, &self.basis, &mut cues)
            })?;
        }
        for cue in cues {
            host.audio(BriefingAudioRequest::Cue { slot: cue }, rng)
                .map_err(BriefingScreenError::Host)?;
        }
        let outcome = runtime.tick(
            tick,
            gates,
            rng,
            camera_world.map(|world| BriefingCameraInputs {
                world,
                math: &self.basis,
            }),
        );
        for effect in &outcome.effects {
            controls.apply_effect(effect, runtime, &self.basis)?;
        }
        if outcome.finish_reason.is_some() {
            host.request_phase(SessionPhase::Phase3, SessionPhase::Gameplay)
                .map_err(BriefingScreenError::Host)?;
            self.finished = true;
        }
        Ok(outcome)
    }

    /// Validate the linked `0040B0F0` draw lifecycle. The software backend
    /// submits the supported panels from this screen's runtime and controls.
    pub fn draw(&self) -> Result<(), BriefingDrawError> {
        self.require_lifecycle_ready()?;
        Ok(())
    }

    fn require_lifecycle<E>(
        &self,
        expected: ScreenLifecycle,
    ) -> Result<(), BriefingScreenError<E>> {
        if self.lifecycle == expected {
            Ok(())
        } else {
            Err(BriefingScreenError::Lifecycle {
                expected,
                actual: self.lifecycle,
            })
        }
    }

    fn require_lifecycle_ready(&self) -> Result<(), BriefingDrawError> {
        if self.lifecycle == ScreenLifecycle::Ready {
            Ok(())
        } else {
            Err(BriefingDrawError::NotReady)
        }
    }
}

#[derive(Debug, Error)]
pub enum BriefingDrawError {
    #[error("briefing draw requires a Ready screen")]
    NotReady,
}

#[derive(Debug, Error)]
pub enum BriefingCallbackError {
    #[error("briefing callback context mismatch: expected {expected}, received {actual}")]
    Context { expected: u32, actual: u32 },
    #[error("briefing callback requires a Ready screen")]
    NotReady,
    #[error("briefing callback requires the current world's authored program")]
    MissingProgram,
    #[error("briefing callback requires both native menu controllers")]
    MissingControls,
    #[error("briefing callback {0:#010x} has no native owner")]
    UnknownCallback(u32),
    #[error(transparent)]
    Controls(#[from] MenuError<BriefingControlError>),
    #[error(transparent)]
    ScreenInput(#[from] ScreenInputError<MenuError<BriefingControlError>>),
}

fn dispatch_action(
    action: ScreenInputAction,
    controls: &mut BriefingControls,
    runtime: &mut BriefingRuntime,
    basis: &MenuBasisMath,
    cues: &mut Vec<u32>,
) -> Result<(), MenuError<BriefingControlError>> {
    // Held slots +24 (0040D4D0), +28 (0040D4B0) and +2C (0040D4F0): the
    // command to the player's message (00413950), then the cue (004391D0).
    let (command, cue) = match action {
        ScreenInputAction::Confirm => (MenuCommand::Activate, 5),
        ScreenInputAction::Previous => (MenuCommand::Previous, 2),
        ScreenInputAction::Next => (MenuCommand::Next, 2),
        _ => return Ok(()), // Briefing virtual +30/+34/+38 are empty tails.
    };
    // 00413950 emits its cue before menu dispatch; each briefing slot emits
    // the same cue once more after that command returns.
    cues.push(cue);
    controls.command(command, runtime, basis)?;
    cues.push(cue);
    Ok(())
}

pub struct BriefingFactoryServices<'a> {
    pub terrain: crate::terrain_runtime::TerrainDetailContext<'a>,
    pub registry:
        &'a mut ScreenRegistry<crate::frontend_renderer::FrontendRenderContext, InputContextId>,
    pub input: &'a mut InputRuntime,
    pub assets: &'a FrontendAssets,
    pub residency: &'a crate::ResidentRuntime,
    pub process: &'a crate::frontend_screen::FrontendProcessState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend_assets::FrontendAssets;
    use crate::frontend_bindings::FrontendDevice;
    use crate::input::InputRuntime;
    use crate::screen_registry::{ScreenAttachment, ScreenRegistry};
    use crate::session::SessionState;
    use std::path::Path;

    struct TestHost {
        keyboard: Option<crate::platform_input::SdlInput>,
    }

    impl BriefingFactoryHost for TestHost {
        type Error = std::convert::Infallible;
        fn enumerate_calibrated_devices(
            &mut self,
            input: &mut InputRuntime,
        ) -> Result<Vec<FrontendDevice>, Self::Error> {
            if self.keyboard.is_none() {
                self.keyboard =
                    Some(crate::platform_input::SdlInput::attach(input).expect("attach keyboard"));
            }
            Ok(vec![self
                .keyboard
                .as_ref()
                .expect("attached keyboard")
                .frontend_device()])
        }
    }

    struct TickHost {
        requests: Vec<(SessionPhase, SessionPhase)>,
        cues: Vec<u32>,
    }

    impl BriefingTickHost for TickHost {
        type Error = std::convert::Infallible;
        fn audio(
            &mut self,
            request: BriefingAudioRequest,
            _rng: &mut u32,
        ) -> Result<(), Self::Error> {
            if let BriefingAudioRequest::Cue { slot } = request {
                self.cues.push(slot);
            }
            Ok(())
        }
        fn request_phase(
            &mut self,
            previous: SessionPhase,
            requested: SessionPhase,
        ) -> Result<(), Self::Error> {
            self.requests.push((previous, requested));
            Ok(())
        }
    }

    fn assets() -> FrontendAssets {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        FrontendAssets::open(&root).expect("retail Infest.exe")
    }

    #[test]
    fn briefing_reuse_serves_only_phase3() {
        assert!(briefing_reuses_phase(SessionPhase::Phase3));
        for phase in SessionPhase::ALL
            .iter()
            .copied()
            .filter(|phase| *phase != SessionPhase::Phase3)
        {
            assert!(!briefing_reuses_phase(phase), "{phase:?}");
        }
    }

    #[test]
    fn zero_record_intro_briefing_requests_gameplay_once() {
        let assets = assets();
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let installation = crate::Installation::open(&root).unwrap();
        let mut residency = crate::ResidentRuntime::new(installation).unwrap();
        residency.apply(crate::ResidencyRequest::Boot).unwrap();
        let process = crate::frontend_screen::FrontendProcessState::default();
        let mut registry = ScreenRegistry::new();
        let attachment = ScreenAttachment {
            session: registry.register_session(),
            context_owner: registry.register_context(
                crate::frontend_renderer::FrontendRenderContext {
                    projection: crate::frontend_scene::SceneProjection {
                        focal: [320, 240],
                        viewport: [640, 480],
                        fade: [0, 0],
                    },
                },
            ),
        };
        let mut input = InputRuntime::new(0);
        let mut screen = BriefingScreen::new(
            BriefingScreenConfig {
                attachment,
                callback_context: BRIEFING_CALLBACK_CONTEXT,
            },
            &assets,
        );
        let mut state = SessionState::default();
        state.request_phase(SessionPhase::Phase3);
        let mut host = TestHost { keyboard: None };
        screen
            .initialize(
                &mut BriefingFactoryServices {
                    terrain: crate::terrain_runtime::TerrainDetailContext {
                        process: &mut crate::terrain_runtime::ProcessTerrainPolicy::supplied(
                            crate::terrain_runtime::TerrainSpacing::Units256,
                        ),
                        world_flags: Some(0x20),
                        terrain: Some(&mut crate::terrain_runtime::native_probe_fixture_terrain(
                            &serde_json::json!({
                                "spacing":256,"height_shift":5,"plane_base":0,"plane_shift":8,
                                "xz":[0,0],"heights":[0,0,0,0],"alternate":false
                            }),
                        )),
                    },
                    registry: &mut registry,
                    input: &mut input,
                    assets: &assets,
                    residency: &residency,
                    process: &process,
                },
                &mut host,
                &mut state,
            )
            .unwrap();
        assert_eq!(
            process.colors(),
            Some(MenuDrawColors {
                selected_start: crate::frontend_screen::resident_menu_color(&residency, 4).unwrap(),
                ordinary: crate::frontend_screen::resident_menu_color(&residency, 5).unwrap(),
                selected_end: crate::frontend_screen::resident_menu_color(&residency, 6).unwrap(),
            })
        );
        let mut tick_host = TickHost {
            requests: Vec::new(),
            cues: Vec::new(),
        };
        let tick = SchedulerTick {
            delta_micros: 8_000,
            elapsed_micros: 8_000,
        };
        let mut rng = 0;
        assert!(matches!(
            screen.tick(
                tick,
                BriefingGates::default(),
                &mut rng,
                &input,
                None,
                &mut tick_host
            ),
            Err(BriefingScreenError::MissingProgram)
        ));
        screen.bind_world(
            BriefingWorldBinding {
                program: BriefingProgram {
                    record_span: inf_formats::ByteSpan::new(0, 0),
                    records: vec![],
                },
                mission_selection: 22,
                milestone_count: 0,
                list_close_millis: None,
            },
            BriefingMenuTemplates::parse(&assets).unwrap(),
        );
        assert_eq!(screen.milestone_count(), Some(0));
        assert_eq!(
            screen
                .controls()
                .unwrap()
                .root
                .current()
                .unwrap()
                .data()
                .records()
                .items[0]
                .draw_arguments[0],
            22 + 0xe0
        );
        let event = |address, value, context| InputCallback {
            address,
            value,
            context,
            timestamp_micros: 0,
        };
        assert!(matches!(
            screen.digital_callback(event(0x0040_d660, 1, BRIEFING_CALLBACK_CONTEXT + 1)),
            Err(BriefingCallbackError::Context { .. })
        ));
        assert_eq!(screen.runtime().unwrap().start_remaining_millis(), 0);
        assert!(screen
            .digital_callback(event(0x0040_d660, 0, BRIEFING_CALLBACK_CONTEXT))
            .unwrap()
            .is_empty());
        assert_eq!(screen.runtime().unwrap().start_remaining_millis(), 0);
        assert_eq!(
            screen
                .digital_callback(event(0x0040_d690, 1, BRIEFING_CALLBACK_CONTEXT))
                .unwrap(),
            vec![2, 2]
        );
        assert_eq!(
            screen
                .digital_callback(event(0x0040_d670, 1, BRIEFING_CALLBACK_CONTEXT))
                .unwrap(),
            vec![5, 5]
        );
        assert_eq!(
            screen.runtime().unwrap().mode(),
            crate::briefing_runtime::BriefingMode::List
        );
        assert!(screen
            .digital_callback(event(0x0040_d660, 1, BRIEFING_CALLBACK_CONTEXT))
            .unwrap()
            .is_empty());
        assert_eq!(screen.runtime().unwrap().start_remaining_millis(), 2000);
        assert!(screen
            .tick(
                tick,
                BriefingGates::default(),
                &mut rng,
                &input,
                None,
                &mut tick_host
            )
            .unwrap()
            .finish_reason
            .is_some());
        assert_eq!(
            tick_host.requests,
            vec![(SessionPhase::Phase3, SessionPhase::Gameplay)]
        );
        // Second tick is a no-op after finish.
        assert!(screen
            .tick(
                tick,
                BriefingGates::default(),
                &mut rng,
                &input,
                None,
                &mut tick_host
            )
            .unwrap()
            .effects
            .is_empty());
        assert_eq!(tick_host.requests.len(), 1);
        assert!(screen.draw().is_ok());
    }
}
