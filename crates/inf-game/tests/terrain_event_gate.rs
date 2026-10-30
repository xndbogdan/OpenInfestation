//! Static custody audit for the independent005483B0 event writer. Placement
//! presence is not proof that its damage/lifecycle callback executes live.

use std::path::{Path, PathBuf};

use inf_formats::FrontendExecutable;
use inf_game::world_inspection::{
    inspect_world_assets, ConstructorReference, InitialModelReference,
};
use inf_game::{Installation, ResidencyRequest, ResidentRuntime, ResourceSelection};

fn data_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[test]
fn independent_terrain_event_writer_is_authored_only_in_three_later_worlds() {
    let root = data_root();
    let bytes = std::fs::read(root.join("Infest.exe")).unwrap();
    let executable = FrontendExecutable::parse(&bytes).unwrap();
    // Selector49 final vtable: postinitialize, update, state1 and damage.
    for (slot, method) in [
        (0x1c, 0x0047_6470),
        (0x24, 0x0047_65a0),
        (0xac, 0x0047_6520),
        (0xb4, 0x0047_6720),
    ] {
        assert_eq!(executable.word(0x0052_7908 + slot).unwrap(), method);
    }

    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 0,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    let mut reached = Vec::new();
    for level in 11..=43 {
        runtime.apply(ResidencyRequest::Activate { level }).unwrap();
        let before = runtime.summary();
        let snapshot = inspect_world_assets(&runtime, 0).unwrap();
        assert_eq!(runtime.summary(), before, "inspection is read-only");
        for link in &snapshot.placement_links {
            let ConstructorReference::Resolved(constructor) = &link.constructor else {
                panic!(
                    "missing H13: leaf {level}, placement {}",
                    link.placement_index
                );
            };
            if constructor.resource.constructor_selector != 49 {
                continue;
            }
            assert_eq!(constructor.source.overlay.level, 1);
            assert_eq!(constructor.source.registration.global_index, 28);
            let dispatch = constructor.asset.dispatch.as_ref().unwrap();
            assert_eq!(dispatch.constructor, 0x0047_6380);
            assert_eq!(dispatch.vtable, 0x0052_7908);
            let InitialModelReference::Resolved(model) = &constructor.initial_model else {
                panic!("selector49 initial model must be resident");
            };
            assert_eq!(model.source.registration.global_index, 279);
            assert_eq!(model.name_ascii.as_deref(), Some("f04base"));
            let codes = link
                .command_views
                .as_ref()
                .unwrap()
                .iter()
                .map(|view| view.code.value)
                .collect::<Vec<_>>();
            // Command24 supplies the linked placement reference consumed by
            // postinitialize. Leaf27 omits it; constructor+16C is unwritten.
            let expected = if level == 27 {
                vec![9, 23, 21, 22]
            } else {
                vec![9, 23, 21, 22, 24]
            };
            assert_eq!(codes, expected);
            reached.push((level, link.placement_index));
        }
    }
    assert_eq!(reached, [(17, 35), (23, 3), (27, 50)]);
    assert!(reached.iter().all(|(level, _)| *level != 32));
}
