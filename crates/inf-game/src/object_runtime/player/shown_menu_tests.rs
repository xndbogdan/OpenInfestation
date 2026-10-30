/// Each owner's menu table, and the controller's: `+00`, the scalar deleting
/// destructor (push esi; mov esi, ecx; call body; test byte ptr [esp+8], 1),
/// calls the body whose effects the port's destruction applies.
const MENU_TABLES: [(&str, u32, u32, u32); 6] = [
    ("Shop", 0x0052_1a14, 0x0043_15c0, 0x0043_15e0),
    ("InWorld", 0x0052_3440, 0x0044_33e0, 0x0044_3400),
    ("Plain", 0x0052_5608, 0x0045_2400, 0x0045_2420),
    ("Database", 0x0052_07f4, 0x0041_4460, 0x0041_4480),
    ("HintList", 0x0052_0808, 0x0041_47b0, 0x0041_47d0),
    ("controller", 0x0052_07e0, 0x0041_4110, 0x0041_4130),
];

#[test]
fn shown_menus_destroy_through_their_tables() {
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let code = |address: u32, length: usize| &exe[(address - 0x0040_0000) as usize..][..length];
    let word = |address: u32| u32::from_le_bytes(code(address, 4).try_into().unwrap());
    for (name, table, deleting, body) in MENU_TABLES {
        assert_eq!(word(table), deleting, "{name}");
        assert_eq!(code(deleting, 4), [0x56, 0x8b, 0xf1, 0xe8], "{name}");
        assert_eq!(
            (deleting + 8).wrapping_add(word(deleting + 4)),
            body,
            "{name}"
        );
        assert_eq!(code(deleting + 8, 4), [0xf6, 0x44, 0x24, 0x08], "{name}");
    }
    // The controller's body is the base controller's destructor 00453400.
    assert_eq!(code(0x0041_4130, 1), [0xe9]);
    assert_eq!(0x0041_4135_u32.wrapping_add(word(0x0041_4131)), 0x0045_3400);
    // A plain menu's restores its table, then unlinks +30/+34 and returns.
    assert_eq!(code(0x0045_2420, 6), [0xc7, 0x01, 0x08, 0x56, 0x52, 0x00]);
    assert_eq!(code(0x0045_2437, 1), [0xc3]);
}
