use super::*;

/// 0047B610 runs the same 004F2BE0 composite table with B on its own stack.
/// With identity bases and B's origin zero, it must match the process-B pair
/// query over the same original models and relative origin.
#[test]
fn original_223_51_stack_counterpart_matches_the_process_b_pair_query() {
    let runtime = resident();
    let actor = source(&runtime, 223);
    let counterpart = source(&runtime, 51);
    let a = qualification(&actor);
    let b = qualification(&counterpart);
    for origin in [[0, 0, 0], [0, 40, 0], [12, -30, 7], [0, 400, 0]] {
        let (process_owners, mut query) = owners(true);
        let mut process_contact = CollisionContact::unwritten();
        query_auxiliary_pair(
            pair(actor.model, &a, counterpart.model, &b, origin),
            &mut query,
            &process_owners,
            &mut process_contact,
        )
        .unwrap();
        let (stack_owners, mut query) = owners(true);
        let mut stack_contact = CollisionContact::unwritten();
        query_auxiliary_actor_pair(
            AuxiliaryActorPairRequest {
                actor_origin: origin,
                actor_basis: MenuBasis::INITIAL,
                actor_model: AuxiliaryModelSource {
                    model: actor.model,
                    qualification: &a,
                },
                actor_values: AuxiliaryValueSource::Required,
                counterpart_basis: MenuBasis::INITIAL,
                counterpart_model: AuxiliaryModelSource {
                    model: counterpart.model,
                    qualification: &b,
                },
                counterpart_values: AuxiliaryValueSource::Required,
                residency: None,
            },
            &mut query,
            &stack_owners,
            &mut stack_contact,
        )
        .unwrap();
        assert_eq!(stack_contact, process_contact, "origin {origin:?}");
        // The stack counterpart publishes no process-B custody.
        assert_eq!(query.auxiliary_raw_source(), None);
    }
}

/// Helniazine's crystal extractors (mining rigs with H8 430) are boxes
/// (8F). As the pair's A, the composite's +08 (004F2690) runs the vehicle's
/// program (H8 117) with the box table, whose spheres meet the box through
/// 004F15F0 (00544C50+00). The port stopped there, and the vehicle
/// drove through the extractor (2026-10-18). Overlapping, they touch;
/// 20000 apart, they do not.
#[test]
fn helniazines_extractor_box_meets_the_vehicle() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 16 })
        .unwrap();
    let extractor = source(&runtime, 430);
    let vehicle = source(&runtime, 117);
    let (a, b) = (qualification(&extractor), qualification(&vehicle));
    for (origin, touches) in [([0, 0, 0], true), ([0, 0, 20000], false)] {
        let (owners, mut query) = owners(true);
        let mut contact = CollisionContact::unwritten();
        query_auxiliary_actor_pair(
            AuxiliaryActorPairRequest {
                actor_origin: origin,
                actor_basis: MenuBasis::INITIAL,
                actor_model: AuxiliaryModelSource {
                    model: extractor.model,
                    qualification: &a,
                },
                actor_values: AuxiliaryValueSource::Required,
                counterpart_basis: MenuBasis::INITIAL,
                counterpart_model: AuxiliaryModelSource {
                    model: vehicle.model,
                    qualification: &b,
                },
                counterpart_values: AuxiliaryValueSource::Required,
                residency: None,
            },
            &mut query,
            &owners,
            &mut contact,
        )
        .unwrap();
        assert_eq!(
            contact.native_words()[0] == Some(1),
            touches,
            "origin {origin:?}: {contact:?}"
        );
    }
}
