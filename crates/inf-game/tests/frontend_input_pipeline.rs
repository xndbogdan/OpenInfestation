//! Original tables -> owned bindings -> publication -> screen repeat.
//! Device IDs are explicit logical fixtures, not an OS enumeration claim.

use inf_game::frontend_assets::FrontendAssets;
use inf_game::frontend_bindings::{FrontendBindingPhase, FrontendBindings, FrontendDevice};
use inf_game::input::{DeviceButtons, DeviceSnapshot, InputCallback, InputHost, InputRuntime};
use inf_game::scheduler::SchedulerTick;
use inf_game::screen_input::{ScreenDispatchPolicy, ScreenInput, ScreenInputAction};
use inf_game::screen_registry::{ScreenAttachment, ScreenRegistry};
use std::path::Path;

struct Host {
    screen: ScreenInput,
    actions: Vec<ScreenInputAction>,
}

impl InputHost for Host {
    type Error = &'static str;
    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(16_000)
    }
    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        self.screen
            .digital_callback(event, &mut |screen, action| {
                self.actions.push(action);
                if action == ScreenInputAction::Back {
                    screen.clear_held();
                }
                Ok::<_, &'static str>(())
            })
            .map_err(|_| "unexpected system callback in screen fixture")
    }
}

#[test]
fn retail_bindings_publish_before_screen_tick_and_back_clears_the_live_repeat_mask() {
    let assets =
        FrontendAssets::open(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")).unwrap();
    let mut input = InputRuntime::default();
    let primary = input.create_device(DeviceButtons::default()).unwrap();
    let axes = input.create_device(DeviceButtons::default()).unwrap();
    let mut registry = ScreenRegistry::new();
    let session = registry.register_session();
    let context_owner = registry.register_context(());
    let screen_id = registry
        .attach_screen(ScreenAttachment {
            session,
            context_owner,
        })
        .unwrap();
    let mut bindings = FrontendBindings::new(77, assets.screen_analog_descriptors());
    bindings.initialize(&mut input).unwrap();
    bindings
        .install(
            &mut input,
            assets.frontend_input_tables(),
            &[
                FrontendDevice {
                    native_type: 2,
                    source: axes,
                },
                FrontendDevice {
                    native_type: 0,
                    source: primary,
                },
            ],
        )
        .unwrap();
    assert_eq!(input.selected_context(), None);
    registry
        .bind_input(screen_id, bindings.context().unwrap(), |context| {
            input.select_context(context)
        })
        .unwrap();
    let mut host = Host {
        screen: ScreenInput::new(ScreenDispatchPolicy::Frontend, 77),
        actions: vec![],
    };
    // Type-2 authored Y gain is -1/8; default logical device calibration
    // preserves this in-range signed value. The result is 3001, above 2500.
    input
        .update_device(
            axes,
            DeviceSnapshot {
                axes: [0, -24_008, 0, 0, 0, 0, 0, 0],
                ..DeviceSnapshot::default()
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(bindings.analog_values(&input).unwrap().consumer_d0, 0);
    assert!(host.actions.is_empty());
    input.poll(&mut host).unwrap();
    assert_eq!(bindings.analog_values(&input).unwrap().consumer_d0, 3001);
    let tick = SchedulerTick {
        delta_micros: 16_000,
        elapsed_micros: 16_000,
    };
    host.screen
        .tick(
            tick,
            bindings.analog_values(&input).unwrap(),
            &mut |_, action| {
                host.actions.push(action);
                Ok::<_, ()>(())
            },
        )
        .unwrap();
    assert_eq!(host.actions, [ScreenInputAction::Previous]);
    assert_eq!(host.screen.repeat_millis(), 234);

    let back = assets.frontend_input_tables().digital[0]
        .as_ref()
        .unwrap()
        .records
        .iter()
        .find(|record| record.descriptor.callback_address == 0x4451e0)
        .unwrap();
    let mut buttons = DeviceButtons::default();
    for code in back.required[0].into_iter().take_while(|code| *code != 0) {
        buttons.set(code, true);
    }
    input
        .update_device(
            primary,
            DeviceSnapshot {
                buttons,
                axes: [0; 8],
            },
            &mut host,
        )
        .unwrap();
    input.poll(&mut host).unwrap();
    // Newly pressed Back dispatches the already-held Previous first, then the
    // Back action clears the live mask; the post-callback repeat write remains.
    assert_eq!(
        host.actions,
        [
            ScreenInputAction::Previous,
            ScreenInputAction::Previous,
            ScreenInputAction::Back
        ]
    );
    assert_eq!(host.screen.held_mask(), 0);
    assert_eq!(host.screen.repeat_millis(), 250);

    let cc = bindings.consumer_cc().unwrap();
    let d0 = bindings.consumer_d0().unwrap();
    bindings.release_sets(&mut input).unwrap();
    assert_eq!(input.analog_consumer_state(cc).unwrap().references, 1);
    assert_eq!(input.analog_consumer_state(d0).unwrap().references, 1);
    bindings.release_base(&mut input).unwrap();
    registry.remove_screen(screen_id).unwrap();
    assert_eq!(bindings.phase(), FrontendBindingPhase::Released);
    assert_eq!(input.selected_context(), None);
    assert_eq!(registry.selected_screen(), None);
    assert!(input.analog_value(cc).is_err());
    assert!(input.analog_value(d0).is_err());
    assert!(
        input.device_buttons(axes).is_ok(),
        "device adapters outlive screen teardown"
    );
}
