use std::{fs, path::Path};

use inf_game::frontend_assets::{FrontendAssetError, FrontendAssets, FrontendMenuCatalog};
use inf_game::menu::{
    MenuActionRequest, MenuBasis, MenuControllerSpec, MenuHost, MenuStack, MenuState,
};
use inf_game::menu_basis::MenuBasisMath;

fn data_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

#[test]
fn retail_frontend_definitions_keep_the_original_bytes_and_callback_link() {
    let assets = FrontendAssets::open(&data_root()).unwrap();
    assert_eq!(
        assets.executable_bytes(),
        fs::read(data_root().join("Infest.exe")).unwrap()
    );
    assert_eq!((assets.position_scale(), assets.motion_scale()), (10, 10));
    assert_eq!(assets.scene_models().len(), 9);
    assert_eq!(assets.camera_presets().len(), 10);
    let sine = assets.sine_table();
    assert_eq!(sine.virtual_address, 0x53df00);
    assert_eq!(
        [
            sine.values[0],
            sine.values[1],
            sine.values[1024],
            sine.values[2048],
            sine.values[3072],
            sine.values[4095]
        ],
        [0, 12, 12539, 23170, 30273, 32767]
    );
    let options = assets.root_menu().items[1].action_arguments[1];
    let definition = assets.menu_definition(options).unwrap();
    assert_eq!(definition.virtual_address, 0x539378);
    assert!(!definition.items.is_empty());
    // A decoded instance can mutate without modifying immutable source bytes.
    let mut snapshot = assets.menu_definition(options).unwrap();
    snapshot.items[0].age = 37;
    assert_ne!(assets.menu_definition(options).unwrap().items[0].age, 37);
    assert!(assets.menu_definition(0).is_err());
    assert!(assets.menu_definition(u32::MAX).is_err());
}

#[test]
fn unsupported_builds_and_truncated_frontend_records_fail_before_publication() {
    let original = fs::read(data_root().join("Infest.exe")).unwrap();
    let mut bytes = original.clone();
    let pe = u32::from_le_bytes(bytes[0x3c..0x40].try_into().unwrap()) as usize;
    bytes[pe + 8] ^= 1;
    assert!(matches!(
        FrontendAssets::from_executable(bytes),
        Err(FrontendAssetError::Executable(_))
    ));
    let mut bytes = original;
    // Supported image's .data RVA and file offset coincide; this is a local
    // test mutation only, never an edit to the original executable on disk.
    bytes[0x13bfb0..0x13bfb4].copy_from_slice(&(-1_i32).to_le_bytes());
    assert!(matches!(
        FrontendAssets::from_executable(bytes),
        Err(FrontendAssetError::NegativeModelCount(-1))
    ));
}

#[test]
fn retail_catalog_reopens_shared_tables_without_discarding_renderer_heights() {
    let mut catalog = FrontendMenuCatalog::new(FrontendAssets::open(&data_root()).unwrap());
    let root = catalog.menu_data(0x53c088).unwrap();
    let options = root.records().items[1].action_arguments[1];
    // Proven generic reachable definitions, including Quit and Options children.
    for address in [options, 0x538e68, 0x539088, 0x539188, 0x539458, 0x53c130] {
        catalog.menu_data(address).unwrap();
    }
    let first = MenuState::new(root.clone(), MenuBasis::INITIAL);
    first.data().records_mut().items[0].row_height = 43;
    first.data().records_mut().items[0].age = 123;
    first.data().records_mut().layouts[0].scroll_animation = -17;
    drop(first);
    let reopened = MenuState::new(catalog.menu_data(0x53c088).unwrap(), MenuBasis::INITIAL);
    assert_eq!(reopened.data().records().items[0].row_height, 43);
    assert_eq!(root.records().items[0].row_height, 43);
    assert_eq!(root.records().items[0].age, 0);
    assert_eq!(root.records().layouts[0].scroll_animation, 0);
    assert_eq!(catalog.assets().root_menu().items[0].row_height, 0);
}

#[test]
fn cross_definition_aliases_do_not_create_divergent_live_tables() {
    let original = fs::read(data_root().join("Infest.exe")).unwrap();
    // Local fixture only: a second definition now refers to the root arrays,
    // whole, as the network host menus 00538650 and 00538668 share theirs.
    let mut bytes = original.clone();
    bytes.copy_within(0x13c088..0x13c09c, 0x139378);
    let mut catalog = FrontendMenuCatalog::new(FrontendAssets::from_executable(bytes).unwrap());
    let root = catalog.menu_data(0x53c088).unwrap();
    root.records_mut().items[0].row_height = 71;
    let alias = catalog.menu_data(0x539378).unwrap();
    assert_eq!(alias.records().items[0].row_height, 71);
    alias.records_mut().items[1].row_height = 9;
    assert_eq!(root.records().items[1].row_height, 9);

    // An item array that only overlaps another stays refused.
    let mut bytes = original;
    bytes.copy_within(0x13c088..0x13c09c, 0x139378);
    bytes[0x139378..0x13937c].copy_from_slice(&0x0053_c028_u32.to_le_bytes());
    bytes[0x13937c..0x139380].copy_from_slice(&2_u32.to_le_bytes());
    let mut catalog = FrontendMenuCatalog::new(FrontendAssets::from_executable(bytes).unwrap());
    let root = catalog.menu_data(0x53c088).unwrap();
    root.records_mut().items[0].row_height = 71;
    assert!(matches!(
        catalog.menu_data(0x539378),
        Err(FrontendAssetError::SharedMenuRecords {
            address: 0x539378,
            existing: 0x53c088
        })
    ));
    assert_eq!(
        catalog.menu_data(0x53c088).unwrap().records().items[0].row_height,
        71
    );
}

#[test]
fn authored_menu_ticks_execute_original_table_rotation_after_draw_acknowledgement() {
    struct BasisHost(MenuBasisMath);
    impl MenuHost for BasisHost {
        type Error = &'static str;
        fn action(&mut self, _: MenuActionRequest, _: &mut MenuStack) -> Result<(), Self::Error> {
            Err("unexpected action in animation fixture")
        }
        fn rotate_menu_basis(
            &mut self,
            basis: &mut MenuBasis,
            angle: i32,
        ) -> Result<(), Self::Error> {
            self.0.rotate(basis, angle);
            Ok(())
        }
    }
    let mut catalog = FrontendMenuCatalog::new(FrontendAssets::open(&data_root()).unwrap());
    let mut host = BasisHost(MenuBasisMath::new(catalog.assets().sine_table().values));
    let mut stack = MenuStack::new(MenuControllerSpec {
        fade_rate: 65535,
        viewport: [0, 0, 1024, 1024],
    });
    stack.push(MenuState::new(
        catalog.menu_data(0x53c088).unwrap(),
        MenuBasis::INITIAL,
    ));
    stack.tick(16_000, &mut host).unwrap();
    assert_eq!(stack.current().unwrap().basis, MenuBasis::INITIAL);
    // Test acknowledgement of the external draw boundary, not a real renderer.
    stack.mark_drawn();
    for _ in 0..100 {
        stack.tick(16_000, &mut host).unwrap();
    }
    assert_eq!(stack.current().unwrap().basis.words[0], 100);
    assert_ne!(
        stack.current().unwrap().basis.words[1..],
        MenuBasis::INITIAL.words[1..]
    );
    stack.tick(16_000, &mut host).unwrap();
    let menu = stack.current().unwrap();
    assert_eq!(menu.basis.words[0], 0);
    assert_eq!(menu.age_millis(), 1616);
    assert!(menu
        .data()
        .records()
        .items
        .iter()
        .all(|row| row.age == 1616));
    for first in [1, 4, 7] {
        let length_squared: f32 = menu.basis.words[first..first + 3]
            .iter()
            .map(|&bits| f32::from_bits(bits).powi(2))
            .sum();
        assert!((length_squared - 1.0).abs() < 0.00001);
    }
}
