use super::*;

/// Every actor selector the factory completes reads its final table's +40:
/// the port's class must name the same function.
#[test]
fn completion_classes_match_every_final_tables_plus_40() {
    use inf_formats::{constructor_dispatch, FrontendExecutable};
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mut seen = std::collections::BTreeMap::new();
    for selector in 1..=94u16 {
        let Some(dispatch) = constructor_dispatch(selector) else {
            continue;
        };
        let completion = executable.word(dispatch.vtable + 0x40).unwrap();
        let expected = match completion {
            0x0047_e2e0 => ActorCompletionClass::Shared,
            0x0040_25b0 => ActorCompletionClass::Selector11,
            0x0041_3120 => ActorCompletionClass::Selector59,
            0x0048_63c0 => ActorCompletionClass::Shot,
            // Not the actor path's completion: 0047E320 (the visual base's
            // dirty clear), 00410F50 (the camera), 004542E0 (selector 43)
            // and the multiplayer arenas' 00403CF0.
            _ => continue,
        };
        assert_eq!(
            actor_completion_class(selector),
            expected,
            "selector {selector}"
        );
        *seen.entry(completion).or_insert(0) += 1;
    }
    assert_eq!(seen.len(), 4, "{seen:x?}");
}
