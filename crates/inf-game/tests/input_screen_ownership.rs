//! Cross-owner selection and destruction, without platform or screen stubs.

use inf_game::input::{
    ActionDescriptor, DeviceButtons, DigitalBinding, InputCallback, InputHost, InputRuntime,
};
use inf_game::screen_registry::{RegistryError, ScreenAttachment, ScreenRegistry};

#[derive(Default)]
struct CallbackLog(Vec<InputCallback>);

impl InputHost for CallbackLog {
    type Error = std::convert::Infallible;

    fn clock_micros(&mut self) -> Result<u64, Self::Error> {
        Ok(100)
    }

    fn callback(&mut self, event: InputCallback, _: &mut InputRuntime) -> Result<(), Self::Error> {
        self.0.push(event);
        Ok(())
    }
}

#[test]
fn deselecting_an_old_screen_clears_bindings_but_retains_the_selected_screen() {
    let mut input = InputRuntime::default();
    let mut registry = ScreenRegistry::new();
    let session = registry.register_session();
    let owner = registry.register_context("display generation 1");
    let attach = ScreenAttachment {
        session,
        context_owner: owner,
    };
    let old = registry.attach_screen(attach).unwrap();
    let old_context = input.create_context().unwrap();
    registry
        .bind_input(old, old_context, |context| input.select_context(context))
        .unwrap();

    let current = registry.attach_screen(attach).unwrap();
    let current_context = input.create_context().unwrap();
    registry
        .bind_input(current, current_context, |context| {
            input.select_context(context)
        })
        .unwrap();
    assert_eq!(input.selected_context(), Some(current_context));

    registry
        .deselect_input(old, |context| input.select_context(context))
        .unwrap();
    assert_eq!(registry.selected_screen(), Some(current));
    assert_eq!(input.selected_context(), None);

    registry
        .select_input(current, |context| input.select_context(context))
        .unwrap();
    assert_eq!(input.selected_context(), Some(current_context));
    assert_eq!(
        registry
            .replace_backend(owner, "display generation 2")
            .unwrap(),
        "display generation 1"
    );
    assert_eq!(
        registry.screen(current).unwrap().attachment.context_owner,
        owner
    );
    assert_eq!(registry.linked_screen(owner).unwrap(), Some(current));
}

#[test]
fn screen_context_destruction_preserves_shared_binding_sets_and_queued_callbacks() {
    let mut input = InputRuntime::default();
    let action = input
        .acquire_consumer(
            ActionDescriptor {
                identity: 0x530000,
                callback_address: Some(0x4850c0),
                flags: 2,
            },
            7,
        )
        .unwrap();
    let device = input.create_device(DeviceButtons::default()).unwrap();
    let context = input.create_context().unwrap();
    let binding_set = input
        .create_binding_set(
            [Some(device), None],
            &[DigitalBinding {
                consumer: action,
                required: [[1, 0, 0, 0], [0; 4]],
                forbidden: [[0; 4]; 2],
            }],
        )
        .unwrap();
    input.attach_binding_set(context, binding_set).unwrap();
    let mut registry = ScreenRegistry::new();
    let session = registry.register_session();
    let owner = registry.register_context(());
    let screen = registry
        .attach_screen(ScreenAttachment {
            session,
            context_owner: owner,
        })
        .unwrap();
    registry
        .bind_input(screen, context, |binding| input.select_context(binding))
        .unwrap();

    let mut host = CallbackLog::default();
    let mut pressed = DeviceButtons::default();
    pressed.set(1, true);
    input
        .update_device(
            device,
            inf_game::input::DeviceSnapshot {
                buttons: pressed,
                axes: [0; 8],
            },
            &mut host,
        )
        .unwrap();
    assert_eq!(input.queued_len(), 1);
    assert!(!input.held(action).unwrap());

    // 00438080: embedded input context precedes scheduler attachment removal.
    // Binding sets and external consumer references survive this context.
    input.destroy_context(context).unwrap();
    registry.remove_screen(screen).unwrap();
    assert_eq!(input.selected_context(), None);
    assert_eq!(registry.selected_screen(), None);
    assert!(matches!(
        registry.linked_screen(owner),
        Err(RegistryError::StaleLinkedScreen { .. })
    ));
    assert_eq!(input.consumer_state(action).unwrap().references, 2);

    input.poll(&mut host).unwrap();
    assert_eq!(host.0.len(), 1);
    assert_eq!(host.0[0].value, 1);
    assert_eq!(host.0[0].context, 7);
    assert!(
        input.held(action).unwrap(),
        "context destruction must not invalidate a surviving consumer's queued target"
    );
    input.destroy_binding_set(binding_set).unwrap();
    assert_eq!(input.consumer_state(action).unwrap().references, 1);
    input.release_consumer(action).unwrap();
}
