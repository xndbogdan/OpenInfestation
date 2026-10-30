//! Produces portable stopped/normal miss-stage provenance; no native execution.
//! Both runs use the actual linked scheduler, title/profile input route and draw.

use super::*;
use crate::object_runtime::{ObjectArena, ObjectList};
use crate::session_platform::intro::required_miss_observer::{
    self as observer, MissPacket, MissStage,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

// The saved normal release650 input file has these exact zero-based events.
// No actor/controller/mission/timer/view or model field is overridden here.
const INPUTS: &[(u64, &str, bool)] = &[
    (5, "Return", true),
    (6, "Return", false),
    (100, "Return", true),
    (101, "Return", false),
    (200, "Return", true),
    (201, "Return", false),
    (230, "Backspace", true),
    (231, "Backspace", false),
    (240, "A", true),
    (241, "A", false),
    (260, "Tab", true),
    (261, "Tab", false),
];
const ITERATIONS: u64 = 650;

// Fresh test sessions have distinct portable arena/heap/sound namespaces. Normalize
// only those Debug labels for comparison; keep slots, generations, native
// handles, numeric actor fields and reference aliases exactly as produced.
fn canonical_namespaces(text: &str) -> String {
    let mut out = String::new();
    let mut namespaces = HashMap::new();
    let mut rest = text;
    while !rest.is_empty() {
        if let Some(label) = ["arena: ", "heap: ", "sound_process: ", "sound_voice_pool: "]
            .into_iter()
            .find(|label| rest.starts_with(*label))
        {
            out.push_str(label);
            rest = &rest[label.len()..];
            let length = rest.bytes().take_while(u8::is_ascii_digit).count();
            if length != 0 {
                // Distinct namespaces remain distinct, including foreign/stale
                // reference relationships. Only their fresh process numbers vary.
                let next = namespaces.len();
                let ordinal = *namespaces.entry((label, &rest[..length])).or_insert(next);
                out.push_str(&format!("<process:{ordinal}>"));
                rest = &rest[length..];
            }
        } else {
            let character = rest.chars().next().unwrap();
            out.push(character);
            rest = &rest[character.len_utf8()..];
        }
    }
    out
}

fn actor_custody(arena: &ObjectArena) -> String {
    let mut seen = HashSet::new();
    let mut snapshot = String::new();
    for list in [
        ObjectList::Flag0Primary,
        ObjectList::Flag0Secondary,
        ObjectList::Flag1Primary,
        ObjectList::Flag1Secondary,
        ObjectList::Parking,
        ObjectList::Disposal,
    ] {
        snapshot.push_str(&format!("{list:?}: {:?}\n", arena.list(list)));
        for node in arena.list(list) {
            if seen.insert(node.object) {
                snapshot.push_str(&format!(
                    "{:?}: {:?}\n",
                    node.object,
                    arena.object(node.object).unwrap()
                ));
            }
        }
    }
    assert_eq!(
        seen.len(),
        arena.len(),
        "all live objects must retain a real list node"
    );
    canonical_namespaces(&snapshot)
}

struct Replay {
    actors: String,
    errors: String,
    world: String,
    rng: u32,
    presented: Vec<Presented>,
    platform_effects: Vec<Effect>,
    platform_rng: u32,
    packets: Vec<MissPacket>,
    phase: SessionPhase,
    frames: u64,
    renderer_epochs: u64,
}

fn replay(armed: bool) -> Replay {
    assert!(!observer::is_armed());
    let (mut ctx, tables, startup) = setup_inner_with_profiles(
        "required-miss-provenance",
        true,
        None,
        InitialProfileMode::EmptyPortable,
    );
    assert_eq!(ctx.session.platform().profiles.cached_count(), 0);
    // Residency changes retire only the scene's resource loan. Preserve the
    // actual renderer/process state and present the already-admitted iteration
    // on its new epoch, just as frontend_app's outer renderer loop does.
    enum RendererCarry {
        Fresh(crate::software_scene::SoftwareSceneConfig),
        Retained(crate::software_scene::SoftwareSceneState),
    }
    let mut frame = SoftwareFrame::new(640, 480, startup.clear_rgba);
    let mut carry = RendererCarry::Fresh(startup.scene);
    let mut present_pending = false;
    let mut iteration = 0_u64;
    let mut renderer_epochs = 0_u64;
    let observation = armed.then(observer::arm);
    let mut packets = Vec::new();
    'epochs: loop {
        let snapshot = ctx.session.residency().clone();
        let epoch = (ctx.session.residency_generation(), snapshot.revision());
        let resources = RenderResources::new(&snapshot);
        let mut scene = match carry {
            RendererCarry::Fresh(config) => SoftwareScene::new(SoftwareSceneRequest {
                model_process: ctx.session.model_process().clone(),
                resources,
                tables: &tables,
                frame: &mut frame,
                config,
            })
            .expect("initial observer renderer"),
            RendererCarry::Retained(state) => {
                let identity = state.id();
                let scene = SoftwareScene::resume(
                    state,
                    crate::software_scene::SoftwareSceneResume {
                        resources,
                        tables: &tables,
                        frame: &mut frame,
                    },
                )
                .expect("observer renderer epoch resume");
                assert_eq!(
                    scene.id(),
                    identity,
                    "resource rebind must retain the renderer"
                );
                scene
            }
        };
        renderer_epochs += 1;
        scene.set_terrain(ctx.session.world().map(|world| world.terrain.clone()));
        loop {
            macro_rules! frame {
                ($render:expr) => {
                    CpuFrame {
                        session: &mut ctx.session,
                        basis: &ctx.basis,
                        notifications: &mut ctx.notifications,
                        now_micros: ctx.now,
                        completions: &mut ctx.completions,
                        render: $render,
                    }
                };
            }
            if !present_pending {
                for &(_, key, pressed) in INPUTS.iter().filter(|(index, _, _)| *index == iteration)
                {
                    ctx.notifications.push(InputNotification::Key {
                        scancode: Scancode::from_name(key).expect("authored replay scancode"),
                        pressed,
                    });
                }
                ctx.now += 125_000;
                if observer::is_armed() {
                    // Gameplay actor callbacks run inside first_pass's session
                    // tick. Publish this iteration before that callback; a
                    // transition that changes its resource loan is rejected
                    // by the unchanged post-step custody check below.
                    observer::frame_context(observer::FrameContext {
                        iteration: ctx.frames,
                        now_micros: ctx.now,
                        generation: ctx.session.residency_generation(),
                        revision: ctx.session.residency().revision(),
                        process_radius_word: ctx.session.furniture_radius.word(),
                    });
                }
                assert!(ctx
                    .driver
                    .first_pass(ctx.now, &mut frame!(None))
                    .expect("observer actual linked first pass")
                    .is_some());
                ctx.driver
                    .linked_update(&mut frame!(None))
                    .expect("observer actual linked update");
                ctx.epoch = (
                    ctx.session.residency_generation(),
                    ctx.session.residency().revision(),
                );
                if ctx.epoch != epoch {
                    present_pending = true;
                    break;
                }
            }
            present_pending = false;
            let render = CpuRender {
                scene: &mut scene,
                resources,
                frontend_scene: &ctx.frontend_scene,
                sine: &ctx.sine,
                template: ctx.template,
                frontend_state: &mut ctx.frontend_state,
                name_state: &mut ctx.name_state,
                presented: &mut ctx.presented,
            };
            ctx.driver
                .linked_present(&mut frame!(Some(render)))
                .expect("observer actual linked present after current-epoch handoff");
            ctx.frames += 1;
            if let Some(scope) = &observation {
                let fresh = scope.take();
                if !fresh.is_empty() {
                    // These actual owner reads happen only in the armed run, after
                    // the unchanged update and presentation. A changed epoch/radius
                    // rejects later attribution; it never changes the original stop.
                    let after = (
                        ctx.session.residency_generation(),
                        ctx.session.residency().revision(),
                        ctx.session.furniture_radius.word(),
                    );
                    for mut packet in fresh {
                        let before = &packet.packet["frame"];
                        let unchanged = before["generation"].as_u64() == Some(after.0)
                            && before["revision"].as_u64() == Some(after.1)
                            && before["process_radius_word"].as_u64() == Some(u64::from(after.2));
                        packet.packet["post_step"] = json!({ "iterations_completed": ctx.frames,
                            "generation": after.0, "revision": after.1, "process_radius_word": after.2 });
                        packet.packet["request_join_custody_unchanged"] = json!(unchanged);
                        if let Some(world) = ctx.session.world() {
                            packet.packet["post_step_cache"] =
                                observer::post_step_cache(&world.arena, &packet)
                                    .unwrap_or_else(|error| json!({"observation_error": error}));
                        }
                        packets.push(packet);
                    }
                }
            }
            iteration += 1;
            assert_eq!(
                ctx.frames, iteration,
                "each admitted input iteration presents once"
            );
            if iteration == ITERATIONS {
                break 'epochs;
            }
        }
        carry = RendererCarry::Retained(scene.into_state());
    }
    drop(observation);
    assert!(!observer::is_armed());
    let world = ctx
        .session
        .world()
        .expect("the authored New Game route retains its world");
    // Stored typed stops must remain the original callback failure, with its
    // original actor/ordinal/XZ. This check does not call the actor again.
    for packet in &packets {
        // Never use an old-world actor/cache snapshot as a current-world join.
        // The raw rejected packet still goes to the receipt and the regression
        // below fails closed; no production error or state is replaced.
        if packet.packet["request_join_custody_unchanged"] != true
            || packet.stage == MissStage::CollisionPrefixReturned
        {
            continue;
        }
        let stop = world
            .actor_updates
            .family_prefix_stops
            .iter()
            .find(|stop| stop.actor == packet.actor)
            .expect("original stop remains retained");
        let crate::world_backend::FamilyPrefixStopReason::Selector12TailFailed(error) =
            &stop.reason
        else {
            panic!("observation must not replace the actual typed stop")
        };
        let (actor, ordinal, xz, _) = observer::miss(error).expect("same required-miss variant");
        assert_eq!(
            (actor, ordinal, xz),
            (
                packet.actor,
                packet.ordinal.expect("stopped packet has actual ordinal"),
                packet.position_words
            )
        );
        assert_eq!(Some(error.to_string()), packet.error_display);
        assert_eq!(
            json!(world
                .arena
                .family429e20_actor(actor)
                .unwrap()
                .probe_234
                .as_ref()
                .unwrap()
                .decoded_cache()
                .words()),
            packet.packet["cache_prefix"]["words"]
        );
    }
    Replay {
        actors: actor_custody(&world.arena),
        errors: canonical_namespaces(&format!("{:?}", world.actor_updates)),
        world: canonical_namespaces(&format!("mission={:?}; flags={:?}; intro={:?}; fade={:?}; presentation={:?}; camera={:?}; retirement={:?}",
            world.mission, world.world_flags, world.intro_runtime, world.fade,
            world.presentation, world.native_camera, world.owner_retirement)),
        rng: world.rng, presented: ctx.presented.clone(),
        platform_effects: ctx.session.platform().effects.clone(),
        platform_rng: ctx.session.platform().random_state, packets,
        phase: ctx.driver.state().current_phase(), frames: ctx.frames,
        renderer_epochs,
    }
}

#[test]
#[ignore = "renders the authored 650-frame route twice; run explicitly in release mode"]
fn actual_linked_first_miss_stages_publish_source_identity_without_mutating_the_run() {
    let unarmed = replay(false);
    assert!(unarmed.packets.is_empty());
    let armed = replay(true);
    let path = root().join(format!(
        "../.tmp/inf-required-miss-observer-{}-{}.json",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let packets: Vec<Value> = armed
        .packets
        .iter()
        .map(|packet| json!({"original_error": packet.error_display, "snapshot": packet.packet}))
        .collect();
    let producer_source_sha256 = std::collections::BTreeMap::from([
        (
            "object_runtime/actor/derived/selector12_descriptor_changes.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector12_descriptor_changes.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector12_point_update.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector12_point_update.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector10_tail.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector10_tail.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector10_motion.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector10_motion.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector10_audio.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector10_audio.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector10_helper.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector10_helper.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector10_helper/tick.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector10_helper/tick.rs"
                ))
            ),
        ),
        (
            "linked_tests/required_miss_observer.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("required_miss_observer.rs"))
            ),
        ),
        (
            "intro/required_miss_observer.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../intro/required_miss_observer.rs"))
            ),
        ),
        (
            "linked_tests.rs",
            format!("{:x}", Sha256::digest(include_bytes!("../linked_tests.rs"))),
        ),
        (
            "intro/runtime.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../intro/runtime.rs"))
            ),
        ),
        (
            "intro.rs",
            format!("{:x}", Sha256::digest(include_bytes!("../intro.rs"))),
        ),
        (
            "world_actor_collision.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../world_actor_collision.rs"))
            ),
        ),
        (
            "model_process.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../model_process.rs"))
            ),
        ),
        (
            "process_model_graphics.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../process_model_graphics.rs"))
            ),
        ),
        (
            "model_process_publication.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../model_process_publication.rs"))
            ),
        ),
        (
            "model_auxiliary.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../model_auxiliary.rs"))
            ),
        ),
        (
            "model_projection/process.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../model_projection/process.rs"))
            ),
        ),
        (
            "session_platform.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../session_platform.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../object_runtime/actor/derived.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived/contact.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/contact.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector36_motion.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector36_motion.rs"
                ))
            ),
        ),
        (
            "world_actor_contact.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../world_actor_contact.rs"))
            ),
        ),
        (
            "process_sound.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../process_sound.rs"))
            ),
        ),
        (
            "process_sound/records.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../process_sound/records.rs"))
            ),
        ),
        (
            "process_sound/voices.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../process_sound/voices.rs"))
            ),
        ),
        (
            "platform_audio.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../platform_audio.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived/selector36_audio.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector36_audio.rs"
                ))
            ),
        ),
        (
            "object_runtime/sound_destruction.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../object_runtime/sound_destruction.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived/descriptor_motion.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/descriptor_motion.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/descriptor_motion/fc.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/descriptor_motion/fc.rs"
                ))
            ),
        ),
        (
            "process_particles/factories.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../process_particles/factories.rs"))
            ),
        ),
        (
            "object_runtime/actor/behavior/damage_cache/particle_adapter.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/behavior/damage_cache/particle_adapter.rs"
                ))
            ),
        ),
        (
            "session_platform/world_actor_behavior.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../world_actor_behavior.rs"))
            ),
        ),
        (
            "session_platform/world_actor_behavior/particles.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../world_actor_behavior/particles.rs"))
            ),
        ),
        (
            "object_runtime/actor/allocation.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../object_runtime/actor/allocation.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived/family4236d0.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/family4236d0.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/family428650.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/family428650.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector12.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector12.rs"
                ))
            ),
        ),
        (
            "object_runtime/actor/derived/selector77.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/selector77.rs"
                ))
            ),
        ),
        (
            "world_backend/owners.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../world_backend/owners.rs"))
            ),
        ),
        (
            "fresh_heap.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("../../fresh_heap.rs"))
            ),
        ),
        (
            "object_runtime/actor/derived/behavior_dispatch.rs",
            format!(
                "{:x}",
                Sha256::digest(include_bytes!(
                    "../../object_runtime/actor/derived/behavior_dispatch.rs"
                ))
            ),
        ),
    ]);
    let mut receipt = json!({
        "status": "portable_first_miss_stage_provenance",
        "current_callback_parent_head": "c06e1aba6cb6b8b45efabf920c459cdef743a96c",
        "historical_point_class10_callback_parent_head": "a1ba088d4b8c23ee5684221e2e454807d59af279",
        "historical_actor_services_callback_parent_head": "3b32db23e67a6dd49dede2bec77e4e22286297ac",
        "historical_auxiliary_callback_parent_head": "f28165179bbd9f8f58ac15f4ca71c1f5ef15ced5",
        "historical_voice_callback_parent_head": "65a2c8f500efe7049f8a4ce71add27d83a1f4610",
        "historical_record_callback_parent_head": "eee74064e290a7fb1f9f5fd505a7e68cb2060169",
        "historical_motion_callback_parent_head": "a01f93e739ba31c773ef42914e04f2d23746409e",
        "historical_collision_prefix_parent_head": "728ca2e06edef5b476997b58bc418092729b3117",
        "historical_production_baseline_head": "81bda7f9d86f6c5832921a65821d4f4aae673ba3",
        "historical_observer_parent_head": "94e0aab9119d2f868b14a4caaacd4a95486b69af",
        "producer_source_sha256": producer_source_sha256,
        "original_executable_sha256": format!("{:x}", Sha256::digest(FrontendAssets::open(&root()).unwrap().executable_bytes())),
        "source_input_receipt_sha256": "10f21947c9f5beefc05c3f4df37cfe5002a217b4c840cab17923c8e5a5d5d241",
        "iterations": ITERATIONS, "delta_micros": 125000, "inputs": INPUTS,
        "renderer_epochs": { "armed": armed.renderer_epochs, "unarmed": unarmed.renderer_epochs },
        "empty_sidecar_mode": "explicit portable profile input; no retail bytes overwritten",
        "regression_assertions_complete": false,
        "armed_unarmed_equal": {"actors_and_lists": armed.actors == unarmed.actors,
            "rng": armed.rng == unarmed.rng && armed.platform_rng == unarmed.platform_rng,
            "retained_typed_errors": armed.errors == unarmed.errors,
            "world_state": armed.world == unarmed.world,
            "presentations": armed.presented == unarmed.presented,
            "platform_effects": armed.platform_effects == unarmed.platform_effects},
        "packet_stage_contract": "StoppedCallback retains the actual obstacle-request ordinal/XZ/error. CollisionPrefixReturned records actual snapped temporary collision XZ with no invented ordinal/error; post-step cache is a separate live read.",
        "packets": packets,
        "bounds": "Original assets are immutable; explicit actual portable stopped or normal-prefix-return stage. Normal snapshot precedes429EE0 restoration/cache completion; post-step bytes are separate. Census remains read-only. No native47C1D0/auxiliary/contact/CRT allocation/full-intro acceptance."
    });
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    eprintln!("INF_REQUIRED_MISS_OBSERVER_RECEIPT={}", path.display());
    assert!(armed.packets.iter().all(|packet| packet.packet["request_join_custody_unchanged"] == true),
        "a miss-stage snapshot crossed its actual residency/process-radius epoch; raw receipt retained");
    assert!(
        armed.packets.iter().all(|packet| {
            packet.packet["frame"]["iteration"]
                .as_u64()
                .and_then(|n| n.checked_add(1))
                == packet.packet["post_step"]["iterations_completed"].as_u64()
        }),
        "a miss-stage snapshot used a stale iteration; raw receipt retained"
    );
    assert!(
        armed.renderer_epochs > 1,
        "the authored route must retire its title resource epoch"
    );
    assert_eq!(armed.renderer_epochs, unarmed.renderer_epochs);
    assert_eq!(armed.frames, ITERATIONS);
    assert_eq!(armed.frames, unarmed.frames);
    assert_eq!(armed.phase, unarmed.phase);
    assert_eq!(armed.actors, unarmed.actors);
    assert_eq!(armed.errors, unarmed.errors);
    assert_eq!(armed.world, unarmed.world);
    assert_eq!(armed.rng, unarmed.rng);
    assert_eq!(armed.presented, unarmed.presented);
    assert_eq!(armed.platform_effects, unarmed.platform_effects);
    assert_eq!(armed.platform_rng, unarmed.platform_rng);
    assert!(
        !armed.presented.is_empty(),
        "the observer runs through actual linked draws"
    );
    // These slots identify the earlier PORT replay's two requests only.
    // Every asset/class/model relationship is independently sampled at the miss;
    // neither the hook nor the census selects an actor by these slot numbers.
    for slot in [47, 82] {
        let label = format!("slot: {slot},");
        let matches = armed
            .packets
            .iter()
            .filter(|packet| {
                packet.stage == MissStage::CollisionPrefixReturned
                    && format!("{:?}", packet.actor).contains(&label)
            })
            .collect::<Vec<_>>();
        assert_eq!(
            matches.len(),
            1,
            "exact first actual normal-prefix return for portable witness {label}"
        );
        let packet = &matches[0].packet;
        assert!(packet.get("observation_error").is_none(), "{packet:#}");
        assert_eq!(packet["actual_update_class"], "Selector12");
        assert_eq!(packet["caller_actor"], packet["actual_actor"]);
        assert!(matches[0].ordinal.is_none());
        assert!(matches[0].error_display.is_none());
        assert_eq!(packet["stage"], "collision_prefix_returned_normally");
        assert_eq!(
            packet["query_return"]["outcome"],
            "actual_prefix_normal_return"
        );
        let entry_mask = packet["query_entry"]["mask"].as_u64().unwrap();
        assert_eq!(
            packet["query_return"]["mask"].as_u64(),
            Some(entry_mask | 1)
        );
        assert_eq!(
            packet["query_entry"]["record"],
            packet["query_return"]["record"]
        );
        assert_eq!(
            packet["query_entry"]["alias_is_546b48"],
            packet["query_return"]["alias_is_546b48"]
        );
        assert_eq!(
            packet["query_entry"]["fallback"],
            packet["query_return"]["fallback"]
        );
        assert_ne!(
            packet["query_return"]["guard_word22e"].as_u64().unwrap() & 0x8000,
            0,
            "normal collision return is observed before caller restores/clears guard"
        );
        assert_eq!(
            packet["query_return"]["temporary_xyz"],
            packet["base"]["position_words"]
        );
        assert!(packet["post_step_cache"].get("observation_error").is_none());
        assert_eq!(
            packet["cache_prefix"]["allocation_address"],
            packet["post_step_cache"]["allocation_address"]
        );
        assert_eq!(
            packet["post_step_cache"]["relative_in_window"], true,
            "this fixture must retain the actual observed point through the rest of its step"
        );
        assert_eq!(
            packet["post_step_cache"]["snapshot_xz"],
            packet["snapshot_xz"]
        );
        assert_eq!(
            packet["coordinate_source"],
            "actual450BD0_temporary_collision_xz"
        );
        assert!(
            packet.get("miss_xz").is_none(),
            "snapped collision coordinates must not masquerade as the original cache request"
        );
        assert!(
            packet["post_step_cache"]["encoded_nibble_at_snapshot_xz"]
                .as_u64()
                .is_some(),
            "read the actual nibble without attributing it as the original request completion"
        );
        assert_eq!(packet["h13"]["status"], "owned");
        assert_eq!(
            packet["h13"]["registration"]["global_index"],
            packet["base"]["resource_word"]
        );
        assert_eq!(packet["actor_root_h8"]["status"], "owned");
        assert_eq!(
            packet["actor_root_h8"]["model_word"],
            packet["current_model_word_42"]
        );
        assert_eq!(packet["terrain"]["allocation_authenticated"], true);
        assert!(packet["frame"]["process_radius_word"].as_u64().is_some());
        assert!(packet["actor_root_h8"]["header_bytes"]["sha256"]
            .as_str()
            .is_some());
        assert!(packet["actor_root_h8"]["auxiliary_bytes"]["sha256"]
            .as_str()
            .is_some());
        let cells = packet["census"]["cells"].as_array().unwrap();
        assert!(cells.len() <= 128);
        // No later cell may be resolved after the first required boundary.
        for cell in cells.iter().take(cells.len().saturating_sub(1)) {
            let boundary = cell["boundary"].as_str().unwrap();
            assert!(
                boundary == "zero_lookup_skips_h11_and_counterpart"
                    || boundary == "native_negative_above_null_or_zero_counterpart_skip"
            );
        }
    }
    receipt["regression_assertions_complete"] = json!(true);
    std::fs::write(&path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
}

#[test]
fn observer_contract_is_guarded_by_complete_original_collision_and_auxiliary_bodies() {
    let assets = FrontendAssets::open(&root()).expect("retail data required");
    let bytes = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(bytes).unwrap();
    // Strict original PE profile above establishes .text raw=RVA here.
    for (start, end, hash) in [
        (
            0x0047_c1d0_u32,
            0x0047_c6cc_u32,
            "70c744e0ca1fc009d760d2516289a54013cf3a078dc4a023f3825de0451071ad",
        ),
        (
            0x004f_2be0,
            0x004f_2c3e,
            "d2bedc011bd9bcaf69b6b376c070a7b375ff81ef07dd53a4af82d0e7784a6ca4",
        ),
        (
            0x004f_2c50,
            0x004f_3549,
            "3fd86d1ee3f192734fd1d35176eedd2f674c16ddac81b7981ce259ba544f0e15",
        ),
    ] {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(
                    &bytes[(start - 0x0040_0000) as usize..(end - 0x0040_0000) as usize]
                )
            ),
            hash
        );
    }
    for (address, instruction) in [
        (0x0047_c2ae_u32, &[0x66, 0x8b, 0x41, 0x0e][..]),
        (0x0047_c2b2, &[0x66, 0x85, 0xc0][..]),
        (0x0047_c2bb, &[0x85, 0xf6][..]),
        (0x0047_c3a8, &[0xf6, 0x44, 0x4b, 0x15, 0x04][..]),
        (0x0047_c3bc, &[0xbf, 0xc4, 0x83, 0x54, 0x00][..]),
    ] {
        let offset = (address - 0x0040_0000) as usize;
        assert_eq!(&bytes[offset..offset + instruction.len()], instruction);
    }
    let table = executable.mapped(0x0054_4c68, 20).unwrap().bytes;
    assert_eq!(
        format!("{:x}", Sha256::digest(table)),
        "64a2e21742e9d01273f98d75ba870a7e95b4a67d8815bdcd406f2b83a2a60660"
    );
}

#[test]
fn sound_namespace_normalization_preserves_owner_serial_aliases_and_numeric_process_state() {
    let assets = FrontendAssets::open(&root()).expect("retail data required");
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let mut a = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let mut b = crate::process_sound::ProcessSoundRuntime::new(executable).unwrap();
    let mut resident =
        crate::ResidentRuntime::new(crate::Installation::open(&root()).unwrap()).unwrap();
    resident.apply(crate::ResidencyRequest::Boot).unwrap();
    let a1 = a
        .create_persistent(&resident, 64, 1, 2, 0, || Ok::<_, &'static str>([0; 3]))
        .unwrap();
    let a2 = a
        .create_persistent(&resident, 64, 1, 2, 0, || Ok::<_, &'static str>([0; 3]))
        .unwrap();
    let b1 = b
        .create_persistent(&resident, 64, 1, 2, 0, || Ok::<_, &'static str>([0; 3]))
        .unwrap();
    let canonical = canonical_namespaces(&format!(
        "{a1:?}|{a2:?}|{a1:?}|{b1:?}; process: 987; phase: 4"
    ));
    let references = canonical.split('|').collect::<Vec<_>>();
    assert_eq!(references[0], references[2]);
    assert_ne!(references[0], references[1]);
    assert_ne!(references[0], references[3].split(';').next().unwrap());
    assert!(references[0].contains("sound_process: <process:0>"));
    assert!(references[1].contains("sound_process: <process:0>"));
    assert!(references[1].contains("serial: 2"));
    assert!(references[3].contains("sound_process: <process:1>"));
    assert!(canonical.ends_with("process: 987; phase: 4"));
}

#[test]
fn sound_voice_namespace_normalization_preserves_alias_serial_and_numeric_state() {
    let a=canonical_namespaces("sound_voice_pool: 200, serial: 7; sound_voice_pool: 200, serial: 8; sound_voice_pool: 201, serial: 7; process: 55");
    let b=canonical_namespaces("sound_voice_pool: 900, serial: 7; sound_voice_pool: 900, serial: 8; sound_voice_pool: 901, serial: 7; process: 55");
    assert_eq!(a, b);
    assert!(a.contains("serial: 8"));
    assert!(a.contains("process: 55"));
    assert_ne!(a,canonical_namespaces("sound_voice_pool: 900, serial: 7; sound_voice_pool: 901, serial: 8; sound_voice_pool: 901, serial: 7; process: 55"));
    assert_ne!(a,canonical_namespaces("sound_voice_pool: 900, serial: 7; sound_voice_pool: 900, serial: 8; sound_voice_pool: 901, serial: 7; process: 56"));
}
