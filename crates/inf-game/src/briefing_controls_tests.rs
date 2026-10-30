use super::*;
use inf_formats::{parse_briefing_program, parse_overlay, parse_world_bank, Preload};
use std::{fs, path::Path};

#[test]
fn authored_detail_panes_keep_the_native_tick_owner_across_menu_replacement() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).expect("retail Infestation data required");
    let basis = MenuBasisMath::new(assets.sine_table().values);
    let preload = Preload::parse(&fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();

    // These are H14 OVL leaf IDs, not campaign selection IDs. Header inputs
    // are deliberately synthetic DWORDs: the native tick class must remain
    // independent of both the active leaf and the root's title argument.
    for leaf in [28, 29, 30] {
        let bytes = fs::read(root.join("Overlay").join(format!("{leaf}.ovl"))).unwrap();
        let layout = parse_overlay(&bytes, &preload.leaf(leaf).unwrap().base).unwrap();
        let region = layout
            .regions
            .iter()
            .find(|region| region.handler.get() == 14)
            .unwrap();
        let bank = parse_world_bank(&bytes, region).unwrap();
        let program = parse_briefing_program(&bytes, &bank, &bank.entries[0]).unwrap();
        assert!(!program.records.is_empty(), "authored leaf {leaf}");
        let mut runtime = BriefingRuntime::new(program);
        let fixture_selection = 0xffff_ff80_u32.wrapping_add(u32::from(leaf));
        let mut controls = BriefingControls::new(
            BriefingMenuTemplates::parse(&assets).unwrap(),
            fixture_selection,
        );
        assert_eq!(
            controls.root.current().unwrap().data().records().items[0].draw_arguments[0],
            fixture_selection.wrapping_add(0xe0),
        );

        controls
            .text
            .current()
            .unwrap()
            .data()
            .records_mut()
            .layouts[0]
            .scroll_animation = 0x1234;
        controls.tick_text(16000, &basis).unwrap();
        assert_eq!(controls.text.current().unwrap().age_millis(), 0);
        assert_eq!(
            controls.text.current().unwrap().data().records().layouts[0].scroll_animation,
            0x1234
        );
        controls.text.mark_drawn();
        controls.tick_text(16000, &basis).unwrap();
        let detail = controls.text.current().unwrap();
        assert_eq!(detail.age_millis(), 32);
        assert_eq!(detail.data().records().items[0].age, 16);
        assert_eq!(detail.data().records().layouts[0].scroll_animation, 0);
        let mut expected_basis = MenuBasis::INITIAL;
        basis.rotate(&mut expected_basis, 160);
        assert_eq!(
            detail.basis.words, expected_basis.words,
            "one base rotation in leaf {leaf}"
        );

        controls
            .command(MenuCommand::Next, &mut runtime, &basis)
            .unwrap();
        controls
            .command(MenuCommand::Activate, &mut runtime, &basis)
            .unwrap();
        assert!(controls.list_active);
        assert_eq!(runtime.mode(), crate::briefing_runtime::BriefingMode::List);
        controls
            .text
            .current()
            .unwrap()
            .data()
            .records_mut()
            .layouts[0]
            .scroll_animation = 200;
        controls.text.mark_drawn();
        controls.tick_text(16000, &basis).unwrap();
        let list = controls.text.current().unwrap();
        assert_eq!(list.age_millis(), 16, "list keeps 00452470");
        assert_eq!(list.data().records().items[0].age, 16);
        assert_eq!(list.data().records().layouts[0].scroll_animation, 185);

        controls
            .command(MenuCommand::Previous, &mut runtime, &basis)
            .unwrap();
        controls
            .command(MenuCommand::Activate, &mut runtime, &basis)
            .unwrap();
        assert!(!controls.list_active);
        assert_eq!(
            runtime.mode(),
            crate::briefing_runtime::BriefingMode::Detail
        );
        controls
            .text
            .current()
            .unwrap()
            .data()
            .records_mut()
            .layouts[0]
            .scroll_animation = -0x2345;
        controls.text.mark_drawn();
        controls.tick_text(16000, &basis).unwrap();
        let replay = controls.text.current().unwrap();
        assert_eq!(replay.age_millis(), 32, "replacement retains 0040D5F0");
        assert_eq!(replay.data().records().items[0].age, 16);
        assert_eq!(replay.data().records().layouts[0].scroll_animation, 0);
    }
}
