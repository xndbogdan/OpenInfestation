use super::*;

/// Every listed selector's final table names the destructor at +00, whose
/// code is the scalar deleting shape around a call to the body:
/// push esi; mov esi, ecx; call body; test byte ptr [esp+8], 1; ...
#[test]
fn class_destructors_match_every_final_table() {
    use inf_formats::{constructor_dispatch, FrontendExecutable};
    let exe = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&exe).unwrap();
    let code = |address: u32, length: usize| &exe[(address - 0x0040_0000) as usize..][..length];
    let mut covered = std::collections::BTreeSet::new();
    for destructor in &CLASS_DESTRUCTORS {
        for &selector in destructor.selectors {
            let dispatch = constructor_dispatch(selector).unwrap();
            assert_eq!(
                executable.word(dispatch.vtable).unwrap(),
                destructor.deleting,
                "selector {selector}"
            );
            assert!(covered.insert(selector), "selector {selector} twice");
        }
        let head = code(destructor.deleting, 12);
        assert_eq!(
            head[..4],
            [0x56, 0x8b, 0xf1, 0xe8],
            "{:#x}",
            destructor.deleting
        );
        let call = destructor.deleting + 3;
        let displacement = u32::from_le_bytes(head[4..8].try_into().unwrap());
        assert_eq!((call + 5).wrapping_add(displacement), destructor.body);
        assert_eq!(
            head[8..12],
            [0xf6, 0x44, 0x24, 0x08],
            "{:#x}",
            destructor.deleting
        );
    }
    // Every selector, the multiplayer arenas' included.
    assert_eq!(covered.len(), 94);
}
