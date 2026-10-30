//! Native descriptor arithmetic/target receipts with explicitly supplied
//! allocator and terrain boundaries. These do not supply the common +19C carry.
use super::*;
use crate::object_runtime::{
    ActorConstructionRequest, ActorConstructorNull, ActorHandle, ActorRegistryConfig,
    ActorServiceResourceKind, ActorTerrainInputs, DerivedActorConstructionRequest,
    FixedPositionTarget, FixtureActorServiceAllocation, ObjectClassKind, ObjectPositionTarget,
};
use crate::terrain_runtime::TerrainRuntime;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const SINE: [u16; 4096] = [0; 4096];
const FIELDS: [u16; 12] = [
    0x14c, 0x150, 0x15c, 0x18c, 0x190, 0x194, 0x208, 0x228, 0x230, 0x234, 0x260, 0x264,
];
/// No grant owners: these descriptors grant nothing.
fn no_grants() -> crate::object_runtime::LiveGrantContext<'static> {
    crate::object_runtime::LiveGrantContext {
        process: None,
        executable: None,
        resident: None,
        text_data: None,
    }
}
fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}
fn words(v: &Value) -> [u16; 3] {
    std::array::from_fn(|n| u(&v[n]) as u16)
}
fn fixture() -> (ObjectArena, ObjectId) {
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap();
    let actor = arena
        .allocate_selector35_actor(
            DerivedActorConstructionRequest {
                actor: ActorConstructionRequest {
                    constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
                    allocation_position: Some([0; 3]),
                    owner: 1,
                    terrain: ActorTerrainInputs {
                        terrain: None,
                        coordinate_shift: 8,
                        clock: 0,
                        world_flags: None,
                        sine_quarter: &SINE,
                    },
                },
                world_mask_limits: [3; 2],
            },
            &mut 0,
        )
        .unwrap();
    (arena, actor)
}
fn set_word(arena: &mut ObjectArena, actor: ObjectId, at: u16, value: u32) {
    match at {
        0x14c => arena.derived_actor_mut(actor).unwrap().word_14c = Some(value),
        0x150 => arena.derived_actor_mut(actor).unwrap().word_150 = Some(value),
        0x15c => arena.derived_actor_mut(actor).unwrap().word_15c = Some(value),
        0x18c..=0x194 => {
            arena.family428650_actor_mut(actor).unwrap().words_188_19c
                [((at - 0x188) / 4) as usize] = Some(value)
        }
        0x208 => arena.family428650_actor_mut(actor).unwrap().words_204_208[1] = Some(value),
        0x228 => arena.family428650_actor_mut(actor).unwrap().word_228 = value,
        0x230 => arena.selector35_actor_mut(actor).unwrap().word_230 = value,
        0x234 => arena.selector35_actor_mut(actor).unwrap().word_234 = value,
        0x260 => arena.selector35_actor_mut(actor).unwrap().word_260 = Some(value),
        0x264 => arena.selector35_actor_mut(actor).unwrap().word_264 = value,
        _ => panic!("unexpected field"),
    }
}
fn get_word(arena: &ObjectArena, actor: ObjectId, at: u16) -> u32 {
    match at {
        0x14c => arena.derived_actor(actor).unwrap().word_14c.unwrap(),
        0x150 => arena.derived_actor(actor).unwrap().word_150.unwrap(),
        0x15c => arena.derived_actor(actor).unwrap().word_15c.unwrap(),
        0x18c..=0x194 => arena.family428650_actor(actor).unwrap().words_188_19c
            [((at - 0x188) / 4) as usize]
            .unwrap(),
        0x208 => arena.family428650_actor(actor).unwrap().words_204_208[1].unwrap(),
        0x228 => arena.family428650_actor(actor).unwrap().word_228,
        0x230 => arena.selector35_actor(actor).unwrap().word_230,
        0x234 => arena.selector35_actor(actor).unwrap().word_234,
        0x260 => arena.selector35_actor(actor).unwrap().word_260.unwrap(),
        0x264 => arena.selector35_actor(actor).unwrap().word_264,
        _ => panic!("unexpected field"),
    }
}
fn target(
    arena: &mut ObjectArena,
    handle: u32,
    point: [u16; 3],
    route: Option<ObjectId>,
) -> NativeHeapRef {
    arena.import_fixture_target(match route {
        Some(route) => NativeTarget::ObjectPosition(ObjectPositionTarget::from_position(
            ActorHandle::from_raw(handle),
            point,
            route,
        )),
        None => NativeTarget::FixedPosition(FixedPositionTarget::from_point(
            ActorHandle::from_raw(handle),
            Some(point),
        )),
    })
}
fn target_value(arena: &ObjectArena, reference: Option<NativeHeapRef>) -> Value {
    reference.map_or(Value::Null, |r| {
        let t = arena.native_target(r).unwrap();
        json!({"handle":t.handle_04().raw(),"focus":t.raw_focus_08_words().unwrap(),"kind":t.kind_word()})
    })
}
struct TerrainHost {
    terrain: TerrainRuntime,
    calls: usize,
}
impl ActorTerrainHost for TerrainHost {
    type Error = &'static str;
    fn terrain(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
    ) -> Result<&TerrainRuntime, Self::Error> {
        self.calls += 1;
        Ok(&self.terrain)
    }
}
fn terrain(height: i8) -> TerrainHost {
    use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
    use inf_formats::{
        parse_terrain_bank, ByteSpan, HandlerFraming, HandlerId, HandlerRegion, TERRAIN_CELL_COUNT,
        TERRAIN_ENTRY_FIXED_BYTES, TERRAIN_PREFIX_BYTES,
    };
    let mut bytes = vec![0; TERRAIN_ENTRY_FIXED_BYTES];
    for index in 0..TERRAIN_CELL_COUNT {
        bytes[TERRAIN_PREFIX_BYTES + index * 2] = height as u8;
    }
    let span = ByteSpan::new(0, bytes.len());
    let bank = parse_terrain_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(12).unwrap(),
            requested_count: 1,
            span,
            framing: HandlerFraming::Standard { payload: span },
        },
    )
    .unwrap();
    TerrainHost {
        terrain: TerrainRuntime::new(
            &bank.entries[0],
            TerrainSampling {
                spacing: TerrainSpacing::Units128,
                height_scale: TerrainHeightScale::Units32,
            },
        ),
        calls: 0,
    }
}

#[test]
fn selector35_descriptors_match_65_complete_native_cases() {
    let fixture_json: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_descriptors_native.json"
    )))
    .unwrap();
    for (index, case) in fixture_json["cases"].as_array().unwrap().iter().enumerate() {
        let i = &case["input"];
        let out = &case["output"];
        let mode = i["mode"].as_str().unwrap();
        let (mut arena, actor) = fixture();
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&i["position"]));
        arena.owned_mut(actor).unwrap().state.base.word_28 = u(&i["size"]) as u16;
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&i["velocity"]);
        for at in FIELDS {
            set_word(&mut arena, actor, at, u(&i["words"][format!("0x{at:x}")]));
        }
        arena.family428650_actor_mut(actor).unwrap().word_22e = u(&i["flags22e"]) as u16;
        arena.family428650_actor_mut(actor).unwrap().byte_22c = u(&i["byte22c"]) as u8;
        arena.selector35_actor_mut(actor).unwrap().byte_238 = u(&i["byte238"]) as u8;
        arena.selector35_actor_mut(actor).unwrap().byte_250 = u(&i["byte250"]) as u8;
        let route = arena.allocate(ObjectClassKind::RouteNode);
        arena.owned_mut(route).unwrap().state.base.position_words = Some(words(&i["source_focus"]));
        arena.route_mut(route).unwrap().distance_squared = u(&i["route_distance"]);
        let route_target = (mode.starts_with("route") || mode == "set_route").then_some(route);
        let source = target(
            &mut arena,
            u(&i["source_handle"]),
            words(&i["source_focus"]),
            route_target,
        );
        let preferred = target(
            &mut arena,
            u(&i["preferred_handle"]),
            words(&i["preferred_focus"]),
            None,
        );
        let current = target(
            &mut arena,
            u(&i["current_handle"]),
            words(&i["current_focus"]),
            route_target,
        );
        arena.derived_actor_mut(actor).unwrap().target_148 =
            i["current"].as_bool().unwrap().then_some(current);
        arena.derived_actor_mut(actor).unwrap().target_154 =
            i["secondary"].as_bool().unwrap().then_some(preferred);
        let slots = if mode == "evade" {
            [1, 3]
        } else if route_target.is_some() {
            [5, 4]
        } else {
            [0, 4]
        };
        for (slot, key, r, score) in [
            (slots[0], "source", source, "source_score"),
            (slots[1], "preferred", preferred, "preferred_score"),
        ] {
            let family = arena.family428650_actor_mut(actor).unwrap();
            family.targets_1d4_1fc[slot] = i[key].as_bool().unwrap().then_some(r);
            family.words_1d8_200[slot] = u(&i[score]);
        }
        let initial_attempts = arena.actor_service_heap().attempts().len();
        let mut rng = u(&i["rng"]);
        let mut process = Selector35RouteProcessState {
            flags_547638: u(&i["global_flags"]) as u8,
            threshold_54762c: i["global_threshold"].as_i64().unwrap() as i32,
        };
        let mut context = Selector35DescriptorContext {
            rng: &mut rng,
            process: &mut process,
            constructor_null: WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(
                u(&i["null"]),
            ))),
            settings_7c: 0,
            live_grants: no_grants(),
        };
        let mut terrain = terrain((i["terrain_height"].as_i64().unwrap() / 32) as i8);
        let delta = u(&i["delta"]);
        let entering = i["entering"].as_bool().unwrap();
        let request = match mode {
            "attack_change" => Selector35DescriptorRequest::ChangeAttack { entering },
            "attack_update" => Selector35DescriptorRequest::UpdateAttack {
                microseconds: delta,
            },
            "evade" => Selector35DescriptorRequest::UpdateEvade {
                microseconds: delta,
            },
            "route_update" => Selector35DescriptorRequest::UpdateObject {
                microseconds: delta,
            },
            "route_change" => Selector35DescriptorRequest::ChangeObject { entering },
            "set_route" => Selector35DescriptorRequest::SetRoute { route },
            _ => panic!("unexpected request"),
        };
        arena
            .run_selector35_descriptor(actor, request, &mut context, &mut terrain)
            .unwrap_or_else(|e| panic!("case{index}/{mode}: {e:?}"));
        let actual_words: serde_json::Map<String, Value> = FIELDS
            .into_iter()
            .map(|at| (format!("0x{at:x}"), json!(get_word(&arena, actor, at))))
            .collect();
        let family = arena.family428650_actor(actor).unwrap();
        let class = arena.selector35_actor(actor).unwrap();
        let derived = arena.derived_actor(actor).unwrap();
        let actual = json!({"words":actual_words,"flags22e":family.word_22e,"byte22c":family.byte_22c,"byte238":class.byte_238,"rng":rng,"global_flags":process.flags_547638,"global_threshold":process.threshold_54762c as u32,"current":target_value(&arena,derived.target_148),"secondary":target_value(&arena,derived.target_154)});
        assert_eq!(&actual, out, "case{index}/{mode}");
        let events = case["events"].as_array().unwrap();
        let allocated: Vec<_> = arena.actor_service_heap().attempts()[initial_attempts..]
            .iter()
            .map(|kind| match kind {
                ActorServiceResourceKind::Target(class) => class.allocation_bytes() as usize,
                _ => panic!("wrong allocation"),
            })
            .collect();
        let expected: Vec<_> = events
            .iter()
            .filter(|e| e["kind"] == "allocate")
            .map(|e| u(&e["size"]) as usize)
            .collect();
        assert_eq!(allocated, expected, "case{index} allocations");
        assert_eq!(
            arena.actor_service_heap().fixture_release_attempts().len(),
            events.iter().filter(|e| e["kind"] == "free").count(),
            "case{index} releases"
        );
        assert_eq!(
            terrain.calls,
            events.iter().filter(|e| e["kind"] == "terrain").count(),
            "case{index} terrain"
        );
        assert_eq!(
            family.words_188_19c[5], None,
            "descriptor must not invent19C"
        );
    }
}

#[test]
fn selector35_descriptor_whole_bodies_and_process_startup_are_guarded() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_descriptors_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["provenance"]["exe_sha256"].as_str().unwrap()
    );
    let image = inf_formats::FrontendExecutable::parse(bytes).unwrap();
    image.loader_zero_data(0x0054_7638, 1).unwrap();
    image.loader_zero_data(0x0054_762c, 4).unwrap();
    assert_eq!(Selector35RouteProcessState::default().flags_547638, 0);
    assert_eq!(Selector35RouteProcessState::default().threshold_54762c, 0);
    for guard in fixture["guards"].as_array().unwrap() {
        //The authenticated BF09 profile's file-backed .text raw offset equals RVA.
        let at = (u(&guard["address"]) - 0x0040_0000) as usize;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + u(&guard["size"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
}

#[test]
fn selector35_null_clone_and_missing_source_retain_native_prefixes() {
    let (mut arena, actor) = fixture();
    let source = target(&mut arena, 7, [1, 2, 3], None);
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc[0] = Some(source);
    arena
        .actor_service_heap_mut()
        .script([FixtureActorServiceAllocation::Null]);
    let mut rng = 1;
    let mut process = Selector35RouteProcessState::default();
    let mut context = Selector35DescriptorContext {
        rng: &mut rng,
        process: &mut process,
        constructor_null: WeightedTargetConstructorNull::Supplied(None),
        settings_7c: 0,
        live_grants: no_grants(),
    };
    let error = arena
        .run_selector35_descriptor(
            actor,
            Selector35DescriptorRequest::ChangeAttack { entering: true },
            &mut context,
            &mut terrain(0),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        Selector35DescriptorError::RequiredField { offset: 0x148, .. }
    ));
    assert!(arena.derived_actor(actor).unwrap().target_148.is_none());
    assert!(arena.family428650_actor(actor).unwrap().words_188_19c[1].unwrap() >= 2_000_000);
    let expected_rng = rng;
    let error = arena
        .run_selector35_descriptor(
            actor,
            Selector35DescriptorRequest::ChangeAttack { entering: true },
            &mut Selector35DescriptorContext {
                rng: &mut rng,
                process: &mut process,
                constructor_null: WeightedTargetConstructorNull::Supplied(None),
                settings_7c: 0,
                live_grants: no_grants(),
            },
            &mut terrain(0),
        )
        .unwrap_err();
    assert!(matches!(
        error,
        Selector35DescriptorError::Target(NativeTargetError::MissingProcessNull)
    ));
    assert_ne!(rng, expected_rng);
    assert!(arena.derived_actor(actor).unwrap().target_148.is_none());
}

fn landing_site(arena: &mut ObjectArena, family: bool) -> ObjectId {
    let request = ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: Some([0; 3]),
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    };
    if family {
        arena.allocate_selector37(request).unwrap()
    } else {
        arena
            .allocate_selector35_actor(
                DerivedActorConstructionRequest {
                    actor: request,
                    world_mask_limits: [3; 2],
                },
                &mut 0,
            )
            .unwrap()
    }
}

/// 0043E660, and 0043E390 through its site search 0043E270: runs of the
/// original bodies with the allocator, 0047AD90 and 00450A30 supplied. The
/// sites are bridges (or, for a +54 that answers NULL, ships) in the actor
/// list. A live site's handle always resolves in the port, so the one case
/// whose supplied 0047AD90 refuses it stays native-only.
#[test]
fn selector35_landing_matches_the_native_cases() {
    let fixture_json: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_landing_native.json"
    )))
    .unwrap();
    const LANDING: [u16; 9] = [
        0x18c, 0x190, 0x194, 0x208, 0x228, 0x230, 0x234, 0x260, 0x268,
    ];
    let mut checked = 0;
    for (index, case) in fixture_json["cases"].as_array().unwrap().iter().enumerate() {
        let i = &case["input"];
        let out = &case["output"];
        if i["unresolved"].as_bool().unwrap() || i["mode"] == "score" {
            continue;
        }
        let (mut arena, actor) = fixture();
        arena.owned_mut(actor).unwrap().state.base.position_words = Some(words(&i["position"]));
        arena.owned_mut(actor).unwrap().state.base.word_28 = u(&i["size"]) as u16;
        arena.actor_mut(actor).unwrap().words_8e_92 = words(&i["velocity"]);
        for at in LANDING {
            let value = u(&i["words"][format!("0x{at:x}")]);
            match at {
                0x268 => {
                    arena.selector35_actor_mut(actor).unwrap().handle_268 =
                        ActorHandle::from_raw(value)
                }
                _ => set_word(&mut arena, actor, at, value),
            }
        }
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.word_22e = u(&i["flags22e"]) as u16;
        family.byte_22c = u(&i["byte22c"]) as u8;
        arena.selector35_actor_mut(actor).unwrap().byte_26c = u(&i["byte26c"]) as u8;
        let route = arena.allocate(ObjectClassKind::RouteNode);
        let object = (u(&i["current_kind"]) == 16).then_some(route);
        let current = target(
            &mut arena,
            u(&i["current_handle"]),
            words(&i["current_focus"]),
            object,
        );
        let helper = target(&mut arena, 3, [0; 3], None);
        arena.derived_actor_mut(actor).unwrap().target_148 =
            i["current"].as_bool().unwrap().then_some(current);
        arena.derived_actor_mut(actor).unwrap().target_154 =
            i["secondary"].as_bool().unwrap().then_some(helper);
        let mut sites = Vec::new();
        for (n, s) in i["sites"].as_array().unwrap().iter().enumerate() {
            let site = landing_site(&mut arena, u(&s["table"]) == 0x5289c0);
            let base = &mut arena.owned_mut(site).unwrap().state.base;
            base.flags = u(&s["flags"]);
            base.position_words = Some(words(&s["position"]));
            base.word_28 = u(&s["size"]) as u16;
            if let Ok(state) = arena.family40d800_mut(site) {
                state.byte_138 = u(&s["byte138"]) as u8;
            }
            let forward = s["forward"].as_array().unwrap();
            let basis = &mut arena.actor_mut(site).unwrap().rotating.basis;
            basis.words[7] = (forward[0].as_f64().unwrap() as f32).to_bits();
            basis.words[9] = (forward[1].as_f64().unwrap() as f32).to_bits();
            sites.push((0x100 + n as u32, arena.actor(site).unwrap().handle));
        }
        if !i["alloc"].as_bool().unwrap() {
            arena
                .actor_service_heap_mut()
                .script([FixtureActorServiceAllocation::Null]);
        }
        let initial_attempts = arena.actor_service_heap().attempts().len();
        let mut rng = u(&i["rng"]);
        let mut process = Selector35RouteProcessState::default();
        let mut context = Selector35DescriptorContext {
            rng: &mut rng,
            process: &mut process,
            constructor_null: WeightedTargetConstructorNull::Supplied(Some(ActorHandle::from_raw(
                u(&i["null"]),
            ))),
            settings_7c: 0,
            live_grants: no_grants(),
        };
        let mut terrain = terrain((i["terrain_height"].as_i64().unwrap() / 32) as i8);
        let request = match i["mode"].as_str().unwrap() {
            "change" => Selector35DescriptorRequest::ChangeLanding {
                entering: i["entering"].as_bool().unwrap(),
            },
            "update" => Selector35DescriptorRequest::UpdateLanding {
                microseconds: u(&i["delta"]),
            },
            mode => panic!("unexpected {mode}"),
        };
        arena
            .run_selector35_descriptor(actor, request, &mut context, &mut terrain)
            .unwrap_or_else(|e| panic!("case{index}: {e:?}"));
        let class = arena.selector35_actor(actor).unwrap();
        let picked = sites
            .iter()
            .find(|(_, handle)| *handle == class.handle_268)
            .map_or(class.handle_268.raw(), |(native, _)| *native);
        let actual_words: serde_json::Map<String, Value> = LANDING
            .into_iter()
            .map(|at| {
                let value = if at == 0x268 {
                    picked
                } else {
                    get_word(&arena, actor, at)
                };
                (format!("0x{at:x}"), json!(value))
            })
            .collect();
        let family = arena.family428650_actor(actor).unwrap();
        let derived = arena.derived_actor(actor).unwrap();
        let actual = json!({"words":actual_words,"flags22e":family.word_22e,"byte22c":family.byte_22c,"byte26c":class.byte_26c,"rng":rng,"eax":Value::Null,"current":target_value(&arena,derived.target_148),"current_is_original":derived.target_148 == Some(current),"secondary":target_value(&arena,derived.target_154)});
        assert_eq!(&actual, out, "case{index}");
        let events = case["events"].as_array().unwrap();
        assert_eq!(
            arena.actor_service_heap().attempts().len() - initial_attempts,
            events.iter().filter(|e| e["kind"] == "allocate").count(),
            "case{index} allocations"
        );
        assert_eq!(
            terrain.calls,
            events.iter().filter(|e| e["kind"] == "terrain").count(),
            "case{index} terrain"
        );
        checked += 1;
    }
    assert_eq!(checked, 110);
}

#[test]
fn selector35_landing_bodies_are_guarded() {
    let fixture: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_landing_native.json"
    )))
    .unwrap();
    let assets = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap();
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        fixture["provenance"]["exe_sha256"].as_str().unwrap()
    );
    for guard in fixture["guards"].as_array().unwrap() {
        let at = (u(&guard["address"]) - 0x0040_0000) as usize;
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&bytes[at..at + u(&guard["size"]) as usize])
            ),
            guard["sha256"].as_str().unwrap()
        );
    }
}
