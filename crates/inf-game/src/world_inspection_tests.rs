use super::*;
use crate::Installation;
use inf_formats::{constructor_dispatch, ConstructorModelBinding, ConstructorPayloadView};
use std::path::Path;

#[test]
fn sound_inputs_skip_zero_and_negatives_and_do_not_activate_missing_resources() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    let inspection = Inspection {
        runtime: &runtime,
        constructors: HashMap::new(),
    };
    let before = runtime.summary();
    for (signed_slot, expected) in [
        (0, ConstructorSoundReference::DisabledZero),
        (
            -1,
            ConstructorSoundReference::NegativeArgument { signed_slot: -1 },
        ),
        (
            i16::MIN,
            ConstructorSoundReference::NegativeArgument {
                signed_slot: i16::MIN,
            },
        ),
        (1, ConstructorSoundReference::NotResident { global_slot: 1 }),
    ] {
        let binding = ConstructorSoundBinding {
            raw: inf_formats::ConstructorField {
                span: ByteSpan::new(80, 82),
                value: signed_slot as u16,
            },
            signed_slot,
            lookup_slot: (signed_slot > 0).then_some(signed_slot as u16),
            object_offset: 0x17c,
            consumer: 0x0042_94a0,
        };
        assert_eq!(inspection.sound(binding).unwrap(), expected);
    }
    assert_eq!(runtime.summary(), before);
}

#[test]
fn absent_unsupported_and_negative_bindings_remain_distinct() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    let mut inspection = Inspection {
        runtime: &runtime,
        constructors: HashMap::new(),
    };
    assert_eq!(
        inspection.constructor(-2).unwrap(),
        ConstructorReference::NegativeArgument { signed_slot: -2 }
    );
    assert_eq!(
        inspection.constructor(0).unwrap(),
        ConstructorReference::NotResident { global_slot: 0 }
    );
    let mut asset = ConstructorAsset {
        local_index: 0,
        dispatch: None,
        model_binding: None,
        payload: ConstructorPayloadView::Opaque,
    };
    assert_eq!(
        inspection.model(&asset).unwrap(),
        InitialModelReference::BindingUnproven
    );
    asset.dispatch = constructor_dispatch(1); // proven base context; no model consumer
    assert_eq!(
        inspection.model(&asset).unwrap(),
        InitialModelReference::NotConsumed
    );
    asset.dispatch = constructor_dispatch(14);
    for (argument, expected) in [
        (-1, InitialModelReference::Disabled),
        (
            -2,
            InitialModelReference::NegativeArgument { signed_slot: -2 },
        ),
        (0, InitialModelReference::NotResident { global_slot: 0 }),
    ] {
        asset.model_binding = Some(ConstructorModelBinding {
            source_span: ByteSpan::new(2, 4),
            raw_word: argument as u16,
            signed_slot: argument,
            consumer: 0x0047_a050,
        });
        assert_eq!(inspection.model(&asset).unwrap(), expected);
    }
}
