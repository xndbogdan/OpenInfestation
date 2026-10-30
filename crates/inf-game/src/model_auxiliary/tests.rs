//! Original H8 programs exercised with explicitly supplied process publishers.
//! These are intrinsic/custody regressions, not native contact acceptance.
use super::*;
use crate::{
    frontend_assets::FrontendAssets,
    frontend_scene::{SceneSetup, SceneView},
    model_process::ModelProcessOwners,
    model_scratch::{ModelScratchDrawState, RegisteredMarkUnwindApproximation},
    object_runtime::{resident_model_resource, ResidentModelBinding},
    world_furniture_effect::{
        AuxiliaryRawSource, FurnitureQueryTableAlias, LiteralRetExitRegistrationApproximation,
    },
    Installation, ResidencyRequest, ResidentRuntime,
};
use std::sync::Arc;

mod actor_pair;
mod child;
mod combat_direct;
mod sphere_box;
mod terrain;

fn assets() -> FrontendAssets {
    FrontendAssets::open(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"))
        .unwrap()
}
fn resident() -> ResidentRuntime {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let mut runtime = ResidentRuntime::new(Installation::open(&root).unwrap()).unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 32 })
        .unwrap();
    runtime
}
fn owners(selected: bool) -> (ModelProcessOwners, ProcessFurnitureQueryState) {
    let assets = assets();
    let exe = inf_formats::FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let owners = ModelProcessOwners::native_startup(exe).unwrap();
    owners.selected_renderer_constructed().unwrap();
    owners
        .graphics
        .publish_view(SceneView {
            eye: [0, 0, -1024],
            basis: MenuBasis::INITIAL,
        })
        .unwrap();
    owners.graphics.publish_shift(0).unwrap();
    owners
        .graphics
        .publish_setup(SceneSetup {
            focal: [320, 240],
            viewport: [640, 480],
            center: [320, 240],
            fade: [128, 4096],
            fade_factor: 0x10000,
        })
        .unwrap();
    if selected {
        owners
            .graphics
            .select_projection(
                ModelSurfaceTableOrigin::TemplateDefault,
                crate::model_context::ModelDispatch::Near,
            )
            .unwrap();
    }
    let mut query = ProcessFurnitureQueryState::native_startup(exe).unwrap();
    query.initialize_query(LiteralRetExitRegistrationApproximation);
    query.publish_pair_basis(MenuBasis::INITIAL);
    (owners, query)
}
fn qualification(binding: &ResidentModelBinding<'_>) -> AuxiliaryRawSource {
    AuxiliaryRawSource {
        registration: binding.registration.clone(),
        allocation: Arc::downgrade(binding.allocation),
        vertices_span: binding.model.vertices_span,
    }
}
fn pair<'a>(
    actor: &'a ModelEntry,
    a: &'a AuxiliaryRawSource,
    counterpart: &'a ModelEntry,
    b: &'a AuxiliaryRawSource,
    origin: [i32; 3],
) -> AuxiliaryPairRequest<'a> {
    AuxiliaryPairRequest {
        residency: None,
        actor_origin: origin,
        actor_basis: MenuBasis::INITIAL,
        actor_model: AuxiliaryModelSource {
            model: actor,
            qualification: a,
        },
        counterpart: AuxiliaryModelSource {
            model: counterpart,
            qualification: b,
        },
    }
}
fn source(runtime: &ResidentRuntime, slot: u16) -> ResidentModelBinding<'_> {
    resident_model_resource(runtime, slot)
        .unwrap()
        .expect("actual original H8 registration")
}

#[test]
fn original_223_51_pair_returns_detail_contact_and_retains_retired_b_custody() {
    let runtime = resident();
    let actor = source(&runtime, 223);
    let counterpart = source(&runtime, 51);
    let a = qualification(&actor);
    let b = qualification(&counterpart);
    let (owners, mut query) = owners(true);
    let mut contact = CollisionContact::unwritten();
    query_auxiliary_pair(
        pair(actor.model, &a, counterpart.model, &b, [0; 3]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap();
    // Original detail centers Y60 / Y-7 and scalars48 /51 yield99-67.
    assert_eq!(contact.flag(), Some(1));
    assert_eq!(contact.displacement(), Some(32));
    assert_eq!(
        contact.normal_bits(),
        [
            Some(0.0f32.to_bits()),
            Some(1.0f32.to_bits()),
            Some(0.0f32.to_bits())
        ]
    );
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
    assert_eq!(query.pair_origin(), [0; 3]);
    assert_eq!(
        query.pair_table_alias(),
        FurnitureQueryTableAlias::Table(ModelSurfaceTableOrigin::TemplateDefault)
    );
    assert_eq!(query.auxiliary_raw_source(), Some(&b));
    let retired = query.auxiliary_allocation().unwrap();
    assert!(matches!(
        owners.scratch.read_vertex::<24>(retired, 0, 0),
        Err(ModelScratchError::RetiredAllocation(_))
    ));
    // Normal child return republishes A, never the child's retained B allocation.
    assert_eq!(owners.graphics.world().unwrap().basis, MenuBasis::INITIAL);
    assert_eq!(owners.graphics.world().unwrap().position, [0; 3]);
    assert!(owners.scratch.read_pool::<24>(0).unwrap()[..12]
        .iter()
        .all(Option::is_some));
}

#[test]
fn actual_492_392_programs_use_the_same_generic_operations_and_return_normally() {
    let runtime = resident();
    let actor = source(&runtime, 492);
    let counterpart = source(&runtime, 392);
    let a = qualification(&actor);
    let b = qualification(&counterpart);
    assert_eq!(actor.model.header.raw_0e, 260);
    assert_eq!(counterpart.model.header.raw_0e, 374);
    for model in [actor.model, counterpart.model] {
        let program = model.auxiliary_program.as_ref().unwrap();
        assert!(program
            .instructions
            .iter()
            .all(|i| matches!(i.opcode, 7 | 0x8e | 0x95)));
        assert!(program
            .instructions
            .iter()
            .any(|i| i.skipped_bytes.contains(&1)));
    }
    // Explicit intrinsic domain, unrelated to an observed actor or cell pose.
    let (owners, mut query) = owners(true);
    let mut contact = CollisionContact::unwritten();
    query_auxiliary_pair(
        pair(actor.model, &a, counterpart.model, &b, [0; 3]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap();
    // The last actor detail sphere (raw4/scalar78) versus counterpart raw2/
    // scalar154 wins with232-round(sqrt(44^2+20^2))=184. This pins the real
    // stream's selected detail result, not merely either possible hit flag.
    let a_center = actor.model.vertices[4];
    let b_center = counterpart.model.vertices[2];
    let mut expected = CollisionContact::unwritten();
    apply_contact_geometry(
        &mut expected,
        ContactGeometryRequest::SpherePair {
            request: ContactSphere {
                center: [
                    i32::from(a_center.x),
                    i32::from(a_center.y),
                    i32::from(a_center.z),
                ],
                radius: 78,
            },
            candidate: ContactSphere {
                center: [
                    i32::from(b_center.x),
                    i32::from(b_center.y),
                    i32::from(b_center.z),
                ],
                radius: 154,
            },
        },
    );
    assert_eq!(expected.displacement(), Some(184));
    assert_eq!(contact, expected);
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Ready
    );
    assert_eq!(query.auxiliary_raw_source(), Some(&b));
}

#[test]
fn rejected_coarse_pair_branches_before_detail_and_clears_only_flag_at_return() {
    let runtime = resident();
    let actor = source(&runtime, 223);
    let counterpart = source(&runtime, 51);
    let a = qualification(&actor);
    let b = qualification(&counterpart);
    let (owners, mut query) = owners(true);
    let normals = [Some(0x11), Some(0x22), Some(0x33)];
    let mut contact = CollisionContact::from_native_words([
        Some(7),
        normals[0],
        normals[1],
        normals[2],
        Some(44),
    ]);
    query_auxiliary_pair(
        pair(actor.model, &a, counterpart.model, &b, [4096, 0, 0]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap();
    assert_eq!(contact.flag(), Some(0));
    assert_eq!(contact.displacement(), Some(-1));
    assert_eq!(contact.normal_bits(), normals);
    let parent_detail = owners.scratch.read_pool::<12>(4 * 24).unwrap();
    assert_eq!(
        parent_detail,
        [Some(0); 12],
        "real loader bytes: branch skips detail world stores"
    );
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
}

#[test]
fn header_zero_gate_preserves_b_and_every_other_contact_dword_without_allocation() {
    let runtime = resident();
    let binding = source(&runtime, 223);
    let a = qualification(&binding);
    let mut disabled = binding.model.clone();
    disabled.header.raw_0e = 0;
    disabled.auxiliary_program = None;
    let (owners, mut query) = owners(false);
    let before = format!("{query:?}");
    let world = owners.graphics.world().unwrap();
    let mut contact =
        CollisionContact::from_native_words([Some(9), Some(1), None, Some(3), Some(4)]);
    query_auxiliary_pair(
        pair(&disabled, &a, binding.model, &a, [9, 8, 7]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap();
    assert_eq!(
        contact.native_words(),
        [Some(0), Some(1), None, Some(3), Some(4)]
    );
    assert_eq!(format!("{query:?}"), before);
    assert_eq!(owners.graphics.world().unwrap(), world);
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
}

#[test]
fn native_null_selection_stops_after_world_store_and_keeps_registered_scope() {
    let runtime = resident();
    let binding = source(&runtime, 223);
    let a = qualification(&binding);
    let (owners, mut query) = owners(false);
    let mut contact = CollisionContact::unwritten();
    let mut supplied = binding.model.clone();
    supplied.vertices[0] = inf_formats::ModelVectorI16 {
        x: 12,
        y: -34,
        z: 56,
    };
    let error = query_auxiliary_pair(
        pair(&supplied, &a, binding.model, &a, [0; 3]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        AuxiliaryModelError::VertexExecution(ModelVertexError::Projection(
            ProcessModelGraphicsError::NativeNullProjection {
                table: ModelSurfaceTableOrigin::TemplateDefault,
                slot: crate::process_model_graphics::SelectedProjectionSlot::Local1c
            }
        ))
    ));
    assert_eq!(contact.flag(), Some(0));
    assert_eq!(contact.displacement(), None);
    let ModelScratchDrawState::Stopped(scope) = owners.scratch.state().unwrap() else {
        panic!("strict stop")
    };
    assert_eq!(
        owners.scratch.cursor().unwrap(),
        usize::from(binding.model.header.vertex_slot_count) * 24 + 8
    );
    let bytes = owners.scratch.read_pool::<24>(0).unwrap();
    let expected = [12_i32, -34, 56].map(i32::to_le_bytes).concat();
    assert_eq!(
        &bytes[..12],
        expected
            .iter()
            .copied()
            .map(Some)
            .collect::<Vec<_>>()
            .as_slice()
    );
    assert_eq!(&bytes[12..], &[Some(0); 12]); //No projection writes reached beyond the NULL callback.
    assert!(
        query.auxiliary_allocation().is_none(),
        "root failed before any B child entry"
    );
    owners
        .scratch
        .recover_stopped_draw(&scope, RegisteredMarkUnwindApproximation)
        .unwrap();
    assert_eq!(owners.scratch.read_pool::<24>(0).unwrap(), bytes);
    assert_eq!(owners.scratch.cursor().unwrap(), 0);
}

#[test]
fn required_program_is_after_allocation_and_actual_model_publication() {
    let runtime = resident();
    let binding = source(&runtime, 223);
    let a = qualification(&binding);
    let mut supplied = binding.model.clone();
    supplied.auxiliary_program = None;
    let (owners, mut query) = owners(false);
    let mut contact = CollisionContact::unwritten();
    assert!(
        matches!(query_auxiliary_pair(pair(&supplied, &a, binding.model, &a, [7, 8, 9]),
        &mut query, &owners, &mut contact), Err(AuxiliaryModelError::RequiredProgram { model })
            if model == supplied.span.start)
    );
    assert_eq!(owners.graphics.world().unwrap().position, [0; 3]);
    assert!(matches!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    assert!(owners.scratch.cursor().unwrap() > 0);
    assert_eq!(contact.flag(), Some(0));
}

#[test]
fn unknown_operation_is_typed_at_reach_without_a_normal_retirement() {
    let runtime = resident();
    let binding = source(&runtime, 223);
    let a = qualification(&binding);
    let mut supplied = binding.model.clone();
    let first = &mut supplied.auxiliary_program.as_mut().unwrap().instructions[0];
    first.opcode = 0x80;
    let address = first.span.start;
    let (owners, mut query) = owners(true);
    let mut contact = CollisionContact::unwritten();
    assert!(
        matches!(query_auxiliary_pair(pair(&supplied, &a, binding.model, &a, [0; 3]),
        &mut query, &owners, &mut contact), Err(AuxiliaryModelError::RequiredOperation { opcode: 0x80, address: a }) if a == address)
    );
    assert!(matches!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    assert_eq!(owners.graphics.world().unwrap().position, [0; 3]);
    assert_eq!(contact.flag(), Some(0));
    assert_eq!(contact.displacement(), None);
}

#[test]
fn preexisting_running_scope_is_hard_before_contact_context_or_pool_changes() {
    let runtime = resident();
    let binding = source(&runtime, 223);
    let a = qualification(&binding);
    let (owners, mut query) = owners(true);
    let scope = owners.scratch.begin_draw().unwrap();
    let before = format!("{query:?}");
    let world = owners.graphics.world().unwrap();
    let mut contact = CollisionContact::from_native_words([Some(9); 5]);
    let error = query_auxiliary_pair(
        pair(binding.model, &a, binding.model, &a, [0; 3]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap_err();
    assert!(matches!(
        error,
        AuxiliaryModelError::Scratch(ModelScratchError::DrawState { .. })
    ));
    assert!(!error.is_required_feature_boundary());
    assert_eq!(contact.native_words(), [Some(9); 5]);
    assert_eq!(format!("{query:?}"), before);
    assert_eq!(owners.graphics.world().unwrap(), world);
    assert_eq!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Running(scope.clone())
    );
    owners
        .scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}

#[test]
fn callback_minus_one_inherits_previous_normals_and_branch95_suppresses_best() {
    let (owners, mut query) = owners(true);
    let scope = owners.scratch.begin_draw().unwrap();
    let run = Run {
        graphics: &owners.graphics,
        scratch: &owners.scratch,
        scope: scope.clone(),
        process: &mut query,
        basis_math: &owners.basis_math,
        residency: None,
        terrain: None,
        counterpart: None,
        ray: None,
    };
    let mut previous =
        CollisionContact::from_native_words([Some(1), Some(10), None, Some(30), Some(5)]);
    let mut best = CollisionContact::from_native_words([None, None, None, None, Some(u32::MAX)]);
    let mut output = CollisionContact::from_native_words([
        Some(1),
        Some(40),
        Some(50),
        Some(60),
        Some(u32::MAX),
    ]);
    run.retain_callback_result(&mut output, &mut previous, &mut best, true)
        .unwrap();
    assert_eq!(
        output.native_words(),
        [Some(1), Some(10), None, Some(30), Some(5)]
    );
    assert_eq!(best.displacement(), Some(-1));
    run.retain_callback_result(&mut output, &mut previous, &mut best, false)
        .unwrap();
    assert_eq!(best, output);
    let tied =
        CollisionContact::from_native_words([Some(1), Some(99), Some(99), Some(99), Some(5)]);
    output = tied;
    run.retain_callback_result(&mut output, &mut previous, &mut best, false)
        .unwrap();
    assert_ne!(
        best, tied,
        "strict signed greater retains the first equal score"
    );
    owners
        .scratch
        .finish_draw(&scope, ModelScratchDrawExit::NormalReturn)
        .unwrap();
}

#[test]
fn reached_backward95_is_named_required_after_real_child_return_without_a_fake_budget() {
    let runtime = resident();
    let actor = source(&runtime, 223);
    let counterpart = source(&runtime, 51);
    let a = qualification(&actor);
    let b = qualification(&counterpart);
    let mut supplied = actor.model.clone();
    let program = supplied.auxiliary_program.as_mut().unwrap();
    let target = program.instructions[0].span.start;
    let branch = program
        .instructions
        .iter_mut()
        .find(|i| i.opcode == 0x95)
        .unwrap();
    let address = branch.span.start;
    let Op::BranchUnlessHit {
        target: destination,
        ..
    } = &mut branch.kind
    else {
        panic!("original95")
    };
    *destination = target;
    let (owners, mut query) = owners(true);
    let mut contact = CollisionContact::unwritten();
    let error = query_auxiliary_pair(
        pair(&supplied, &a, counterpart.model, &b, [4096, 0, 0]),
        &mut query,
        &owners,
        &mut contact,
    )
    .unwrap_err();
    assert!(
        matches!(error,AuxiliaryModelError::RequiredBackwardBranch { address: a,target: t } if a==address&&t==target)
    );
    assert!(error.is_required_feature_boundary());
    assert!(matches!(
        owners.scratch.state().unwrap(),
        ModelScratchDrawState::Stopped(_)
    ));
    assert_eq!(query.auxiliary_raw_source(), Some(&b));
    assert_eq!(contact.flag(), Some(0));
    let allocation = query.auxiliary_allocation().unwrap();
    assert!(matches!(
        owners.scratch.read_vertex::<24>(allocation, 0, 0),
        Err(ModelScratchError::RetiredAllocation(_))
    ));
    assert_eq!(
        owners.graphics.world().unwrap().basis,
        MenuBasis::INITIAL,
        "actual normal child republish reached"
    );
}

mod native_guards;
