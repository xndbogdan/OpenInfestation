//! Original-bytecode receipts supply inputs, not behavioral defaults.

use super::tests::allocation;
use super::*;
use crate::object_runtime::{ActorExtension, ActorRegistryConfig, CameraSettings, ObjectClassKind};
use inf_formats::native_basis::NativeBasis;
use serde_json::{json, Value};

fn words(case: &Value, key: &str) -> [u16; 3] {
    serde_json::from_value(case[key].clone()).unwrap()
}

fn follow_snapshot(target: Option<ActorFollowTarget>) -> Value {
    target.map_or(Value::Null, |target| {
        json!({
            "handle_04": target.handle_04.raw(), "focus_08": target.focus_08,
            "word_10": target.word_10, "focus_14": target.focus_14,
            "velocity_1a": target.velocity_1a,
        })
    })
}

fn object_snapshot(
    target: Option<ObjectPositionTarget>,
    first: ObjectId,
    other: ObjectId,
) -> Value {
    target.map_or(Value::Null, |target| {
        json!({
            "handle_04": target.handle_04.raw(), "focus_08": target.focus_08,
            "word_10": target.word_10,
            "object_14": if target.object_14 == first { "first" } else {
                assert_eq!(target.object_14, other); "other"
            },
        })
    })
}

#[test]
fn owned_target_methods_and_live_callers_match_original_bytecode() {
    let receipt: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/target_resources_native.json"
    ))
    .unwrap();
    let mut compared = 0;
    let mut native_only = 0;
    for case in receipt["cases"].as_array().unwrap() {
        let operation = case["operation"].as_str().unwrap();
        let kind = case["kind"].as_str().unwrap();
        if case["allocation_fails"] == true
            || operation == "destroy"
            || (kind == "object"
                && matches!(
                    operation,
                    "refresh" | "basis_query" | "velocity_query" | "actor_query"
                ))
        {
            // Native heap outcomes and currently unreached object getter
            // slots remain evidence, without inventing portable APIs.
            native_only += 1;
            continue;
        }
        let mut arena =
            ObjectArena::with_actor_registry(ActorRegistryConfig {
                owner_count: 2,
                initial_capacity: 8,
                null_handle: ActorHandle::from_raw(
                    case["process_null_handle"].as_u64().unwrap() as u32
                ),
            })
            .unwrap();
        let handle = ActorHandle::from_raw(case["handle_04"].as_u64().unwrap() as u32);
        let first = if case["referent_domain"] == "route" {
            arena.allocate(ObjectClassKind::RouteNode)
        } else {
            let actor = arena
                .allocate_actor(allocation(Some(words(case, "position"))))
                .unwrap();
            let allocated_handle = arena.actor(actor).unwrap().handle;
            assert!(arena
                .actor_handles
                .as_mut()
                .unwrap()
                .release(allocated_handle));
            arena
                .actor_handles
                .as_mut()
                .unwrap()
                .bind(handle, actor)
                .unwrap();
            arena.actor_mut(actor).unwrap().handle = handle;
            let state = arena.actor_mut(actor).unwrap();
            state.word_74 = case["raise_word"].as_u64().unwrap() as u16;
            state.words_8e_92 = words(case, "velocity");
            state.word_c4 = case["state"].as_u64().unwrap() as u32;
            state.rotating.basis = NativeBasis {
                words: serde_json::from_value(case["basis_words"].clone()).unwrap(),
            };
            if case["resolves"] == false {
                assert!(arena.actor_handles.as_mut().unwrap().release(handle));
            }
            actor
        };
        arena.owned_mut(first).unwrap().state.base.position_words = Some(words(case, "position"));
        arena.owned_mut(first).unwrap().state.base.flags = case["flags"].as_u64().unwrap() as u32;
        let other = arena.allocate(ObjectClassKind::RouteNode);
        arena.owned_mut(other).unwrap().state.base.position_words =
            Some(words(case, "other_position"));
        let first_before = arena.object(first).unwrap().clone();
        let other_before = arena.object(other).unwrap().clone();
        let mut follow = ActorFollowTarget {
            handle_04: handle,
            focus_08: Some(words(case, "focus_08")),
            word_10: 2,
            focus_14: Some(words(case, "focus_14")),
            velocity_1a: Some(words(case, "target_velocity")),
        };
        let mut object = ObjectPositionTarget {
            handle_04: arena.actor_handles().unwrap().null_handle(),
            focus_08: words(case, "focus_08"),
            word_10: 16,
            object_14: first,
        };
        let mut returned = Value::Null;
        let mut query = Value::Null;
        let target = match (kind, operation) {
            ("follow", "construct_null") => follow_snapshot(Some(
                arena
                    .follow_target(FollowTargetRequest::resolve_actor(handle))
                    .unwrap(),
            )),
            ("follow", "construct_explicit") => follow_snapshot(Some(
                arena
                    .follow_target(FollowTargetRequest {
                        handle,
                        point: FollowTargetPoint::ExplicitPoint(Some(words(
                            case,
                            "explicit_point",
                        ))),
                    })
                    .unwrap(),
            )),
            ("follow", "refresh") => {
                returned = json!(arena
                    .refresh_follow_target(
                        &mut follow,
                        case["settings_7c"].as_u64().unwrap() as u32
                    )
                    .unwrap());
                follow_snapshot(Some(follow))
            }
            ("follow", "clone") => follow_snapshot(Some(follow.clone_native())),
            ("follow", "basis_query") => {
                query = json!({"kind":"basis", "words":arena.follow_target_basis(&follow).unwrap().words});
                follow_snapshot(Some(follow))
            }
            ("follow", "actor_query") => {
                returned = json!(u32::from(
                    arena.resolve_actor_handle(handle).unwrap().is_some()
                ));
                follow_snapshot(Some(follow))
            }
            ("follow", "velocity_query") => {
                query = json!({"kind":"velocity", "words":follow.velocity_1a.unwrap()});
                follow_snapshot(Some(follow))
            }
            ("object", "clone") => object_snapshot(
                Some(arena.clone_object_position_target(&object).unwrap()),
                first,
                other,
            ),
            ("object", "identity_replace") => {
                returned = json!(u32::from(
                    arena
                        .replace_object_position_target(
                            &mut object,
                            if case["same_identity"] == true {
                                first
                            } else {
                                other
                            }
                        )
                        .unwrap()
                ));
                object_snapshot(Some(object), first, other)
            }
            ("object", "route_replace") | ("follow", "follow_replace") => {
                let helper = arena.allocate_selector90(allocation(None)).unwrap();
                if case["has_old"] == true {
                    let old = arena.import_fixture_target(if kind == "object" {
                        NativeTarget::ObjectPosition(object)
                    } else {
                        NativeTarget::ActorFollow(follow)
                    });
                    let ActorExtension::Selector90(state) =
                        &mut arena.actor_mut(helper).unwrap().extension
                    else {
                        unreachable!()
                    };
                    if kind == "object" {
                        state.route_150 = Some(old);
                    } else {
                        state.target_154 = Some(old);
                    }
                }
                if kind == "object" {
                    arena
                        .replace_selector90_route(
                            helper,
                            (case["has_new"] == true).then_some(other),
                            WeightedTargetConstructorNull::ActorRegistry,
                        )
                        .unwrap();
                    object_snapshot(
                        arena
                            .selector90(helper)
                            .unwrap()
                            .route_150
                            .map(|reference| arena.object_position_target_data(reference).unwrap()),
                        first,
                        other,
                    )
                } else {
                    let new = (case["has_new"] == true)
                        .then(|| arena.import_fixture_target(NativeTarget::ActorFollow(follow)));
                    arena.replace_selector90_target(helper, new).unwrap();
                    follow_snapshot(
                        arena
                            .selector90(helper)
                            .unwrap()
                            .target_154
                            .map(|reference| arena.actor_follow_target(reference).unwrap()),
                    )
                }
            }
            ("follow", "secondary_replace") | ("follow", "camera_replace") => {
                let settings = CameraSettings {
                    first_person: case["first_person"].as_u64().unwrap() as u32,
                    camera_style: 0,
                };
                let camera = arena
                    .allocate_selector16(allocation(None), settings)
                    .unwrap();
                let old = (case["has_old"] == true)
                    .then(|| arena.import_fixture_target(NativeTarget::ActorFollow(follow)));
                let new = (case["has_new"] == true)
                    .then(|| arena.import_fixture_target(NativeTarget::ActorFollow(follow)));
                if let ActorExtension::Selector16(state) =
                    &mut arena.actor_mut(camera).unwrap().extension
                {
                    state.mode_188 = 7;
                    if operation == "camera_replace" {
                        state.target_18c = old;
                    } else {
                        state.target_190 = old;
                    }
                }
                if operation == "camera_replace" {
                    arena.replace_camera_target(camera, new, settings).unwrap();
                    follow_snapshot(
                        arena
                            .selector16(camera)
                            .unwrap()
                            .target_18c
                            .map(|reference| arena.actor_follow_target(reference).unwrap()),
                    )
                } else {
                    arena.replace_camera_secondary_target(camera, new).unwrap();
                    follow_snapshot(
                        arena
                            .selector16(camera)
                            .unwrap()
                            .target_190
                            .map(|reference| arena.actor_follow_target(reference).unwrap()),
                    )
                }
            }
            _ => panic!("unclassified receipt {}", case["name"]),
        };
        assert_eq!(target, case["target"], "{}", case["name"]);
        assert_eq!(returned, case["returned"], "{}", case["name"]);
        assert_eq!(query, case["query"], "{}", case["name"]);
        assert_eq!(case["referents_unchanged"], true);
        assert_eq!(
            arena.object(first).unwrap(),
            &first_before,
            "{}",
            case["name"]
        );
        assert_eq!(
            arena.object(other).unwrap(),
            &other_before,
            "{}",
            case["name"]
        );
        compared += 1;
    }
    assert_eq!((compared, native_only), (34, 9));
}
