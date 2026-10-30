use super::*;

fn fixture() -> ProcessFurnitureQueryState {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    ProcessFurnitureQueryState::native_startup(
        FrontendExecutable::parse(assets.executable_bytes()).unwrap(),
    )
    .unwrap()
}

#[test]
fn reached_init_preserves_other_mask_bits_and_fallback_but_constructs_all_five_bases() {
    let mut process = fixture();
    // Explicit supplied prior bytes, distinct from native startup evidence.
    process.query_init_mask = 0x80;
    process.fallback = [0x11, 0x22, 0x33, 0x44];
    let record = process.current_record.clone();
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    assert_eq!(process.query_init_mask, 0x81);
    assert_eq!(process.query_bases, [MenuBasis::INITIAL; 5]);
    assert_eq!(process.fallback, [0x11, 0x22, 0x33, 0x44]);
    assert_eq!(process.current_record, record);
    assert!(!process.pair_alias_is_546b48);
    assert!(process.literal_ret_registration_attempted);
}

#[test]
fn already_initialized_visit_preserves_actual_basis_record_and_pointer_alias_history() {
    let mut process = fixture();
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    let mut basis = MenuBasis::INITIAL;
    basis.words[0] = 0x1234;
    process.publish_pair_basis(basis);
    process.publish_pair_alias_546b48();
    process.publish_record(FurnitureQueryRecord::Fallback);
    process.write_fallback_state(0x27);
    let prior_bases = process.query_bases;
    process.initialize_query(LiteralRetExitRegistrationApproximation);
    assert_eq!(process.query_bases, prior_bases);
    assert_eq!(process.current_record, FurnitureQueryRecord::Fallback);
    assert!(process.pair_alias_is_546b48);
    assert_eq!(process.fallback_payload().state, 0x27);
    assert_eq!(process.fallback_payload().resource_lookup.raw(), 0);
    assert_eq!(process.query_init_mask, 1);
}
