//! The barrier against check_selector69.py's receipt of the unchanged
//! executable: commands 00411100, the ground fit 00411150, the update
//! 00411380, the light 00411C50, the draw 00411570 and the +98 00411CE0.

use super::*;
use crate::frontend_scene::SceneView;
use crate::menu::MenuBasis;
use crate::model_context::{ModelContext, ModelDispatch, ModelProjection};
use crate::model_primitives::TextureSortIdentity;
use crate::model_projection::ModelProjectionBackend;
use crate::model_vertices::{ModelVertexInput, ModelVertexProjection, ModelVertexProjector};
use crate::object_runtime::{
    ribbon_sort_word, ActorConstructionRequest, ActorConstructorNull, ActorRegistryConfig,
    ActorTerrainInputs,
};
use inf_formats::{ByteSpan, FrontendExecutable, InstanceField};
use serde::Deserialize;

const SINE: [u16; 4096] = [0; 4096];
/// check_selector69.py's barrier handle +84 and current NULL 0054E894.
const NATIVE_HANDLE: u32 = 0x1234_abcd;
const NATIVE_NULL: u32 = 0x1234_5678;
/// check_particle_draws.py's one resident sprite descriptor.
const SPRITE: u32 = 0x0075_0000;

#[derive(Deserialize)]
struct Receipt {
    cases: Vec<Case>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    kind: String,
    position: [i32; 3],
    basis: [f32; 9],
    #[serde(default)]
    bolts: Option<i32>,
    #[serde(default)]
    age: Option<u32>,
    #[serde(default)]
    length: Option<i32>,
    #[serde(default)]
    half: Option<i32>,
    #[serde(default)]
    links: Option<i32>,
    #[serde(default)]
    ends: Option<[i32; 2]>,
    #[serde(default)]
    posts: Option<[[i32; 3]; 2]>,
    #[serde(default)]
    grant: u16,
    #[serde(default)]
    player: u32,
    #[serde(default)]
    code: i64,
    #[serde(default)]
    value: i64,
    #[serde(default)]
    delta: u32,
    #[serde(default)]
    actors: Vec<NativeActor>,
    eye: [i32; 3],
    context_position: [i32; 3],
    view_basis: [f32; 9],
    focal: [i32; 2],
    viewport: [i32; 2],
    center: [i32; 2],
    fog_factor: i32,
    fade: [i32; 2],
    coordinate_shift: u8,
    far_color: u32,
    sorting: bool,
    selector: u8,
    texture_flags: u16,
    effect_color: u32,
    outcome: serde_json::Value,
    events: Vec<serde_json::Value>,
    packets: Vec<Packet>,
    fields: Fields,
}

#[derive(Deserialize)]
struct NativeActor {
    flags: u32,
    position: [i32; 3],
    radius: u16,
    velocity: [i32; 3],
    state: u32,
}

#[derive(Deserialize)]
struct Packet {
    key: u32,
    callback: u32,
    blocks: usize,
    payload: Vec<Option<u8>>,
}

#[derive(Deserialize)]
struct Fields {
    half: i32,
    radius: u16,
    ends: [i32; 2],
    position: [u16; 3],
    posts: [[u16; 3]; 2],
    bolts: i32,
    age: u32,
    length: i32,
    links: i32,
    word_74: u16,
}

fn receipt() -> Receipt {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector69_native.json"
    ))
    .unwrap()
}

fn words(values: [i32; 3]) -> [u16; 3] {
    values.map(|value| value as u16)
}

fn arena() -> ObjectArena {
    ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 17,
        initial_capacity: 40,
        null_handle: ActorHandle::from_raw(0),
    })
    .unwrap()
}

fn request() -> ActorConstructionRequest<'static> {
    ActorConstructionRequest {
        constructor_null: ActorConstructorNull::RegistrySnapshotApproximation,
        allocation_position: None,
        owner: 1,
        terrain: ActorTerrainInputs {
            terrain: None,
            coordinate_shift: 8,
            clock: 0,
            world_flags: None,
            sine_quarter: &SINE,
        },
    }
}

/// A barrier holding the case's supplied fields, with flags that keep it out
/// of its own update.
fn barrier(arena: &mut ObjectArena, case: &Case) -> ObjectId {
    let object = arena.allocate_selector69(request()).unwrap();
    let base = &mut arena.owned_mut(object).unwrap().state.base;
    base.position_words = Some(words(case.position));
    base.flags = 0;
    let actor = arena.actor_mut(object).unwrap();
    actor.rotating.basis.words[1..].copy_from_slice(&case.basis.map(f32::to_bits));
    actor.word_7a = case.grant;
    let state = arena.selector69_mut(object).unwrap();
    if let Some(bolts) = case.bolts {
        state.bolts_144 = bolts;
    }
    if let Some(age) = case.age {
        state.age_148 = age;
    }
    if let Some(length) = case.length {
        state.length_14c = length;
    }
    state.half_150 = case.half;
    if let Some(links) = case.links {
        state.links_15c = links;
    }
    if let Some(ends) = case.ends {
        state.ends_154 = ends;
    }
    state.posts_138 = case.posts.map(|posts| posts.map(words));
    object
}

fn events<'a>(case: &'a Case, kind: &str) -> Vec<&'a serde_json::Value> {
    case.events
        .iter()
        .filter(|event| event["kind"] == kind)
        .collect()
}

fn word_of(value: &serde_json::Value) -> u16 {
    value.as_u64().unwrap() as u16
}

/// Codes 16 and 17 and the refusals above them against 00411100. Codes up
/// to 15 go to the shared 0047ADA0, which the receipt records and the port's
/// shared command tests own.
#[test]
fn barrier_commands_match_00411100() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let runtime = crate::ResidentRuntime::new(crate::Installation::open(&root).unwrap()).unwrap();
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "command") {
        let code = case.code as i32;
        if code <= 15 {
            assert_eq!(events(case, "shared_command").len(), 1, "{}", case.name);
            continue;
        }
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        let field = InstanceField {
            span: ByteSpan { start: 0, end: 4 },
            value: case.value as i32,
        };
        let view = InstanceCommandView {
            target: InstanceCommandTarget::Constructor(69),
            command_offset: 0,
            code: InstanceField {
                span: ByteSpan { start: 0, end: 4 },
                value: code,
            },
            proven_method: Some(0x0041_1100),
            payload_consumer: Some(0x0041_1100),
            payload: match code {
                16 => InstanceCommandPayload::Selector69DoubledDword14c(field),
                17 => InstanceCommandPayload::Selector69Dword144(field),
                _ => InstanceCommandPayload::Opaque,
            },
        };
        let accepted = arena
            .apply_selector69_command(
                object,
                &view,
                ActorCommandContext {
                    runtime: &runtime,
                    first_player_grants: None,
                    terrain: None,
                },
            )
            .unwrap();
        assert_eq!(
            u64::from(accepted),
            case.outcome["result"].as_u64().unwrap(),
            "{}",
            case.name
        );
        let state = arena.selector69(object).unwrap();
        assert_eq!(state.length_14c, case.fields.length, "{} length", case.name);
        assert_eq!(state.bolts_144, case.fields.bolts, "{} bolts", case.name);
        checked += 1;
    }
    assert_eq!(checked, 7);
}

/// Every ground fit: the samples in order, the half length and radius, the
/// ends, the height, the posts, the untouched +74, and disposal.
#[test]
fn barrier_ground_fits_match_00411150() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "fit") {
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        arena.actor_mut(object).unwrap().word_74 = 0x5a5a;
        let heights = events(case, "height");
        let mut sample = 0;
        let fit = arena
            .fit_selector69::<()>(object, |[x, z]| {
                let native = heights[sample];
                assert_eq!(
                    [x, z],
                    [word_of(&native["x"]), word_of(&native["z"])],
                    "{} sample {sample}",
                    case.name
                );
                sample += 1;
                Ok(word_of(&native["height"]) as i16)
            })
            .unwrap();
        assert_eq!(sample, heights.len(), "{} samples", case.name);
        let disposed = !events(case, "dispose").is_empty();
        assert_eq!(fit == Selector69Fit::Disposed, disposed, "{}", case.name);
        let state = arena.selector69(object).unwrap();
        assert_eq!(state.half_150, Some(case.fields.half), "{} half", case.name);
        assert_eq!(
            arena.object(object).unwrap().base.word_28,
            case.fields.radius,
            "{} +28",
            case.name
        );
        assert_eq!(
            arena.actor(object).unwrap().word_74,
            case.fields.word_74,
            "{} +74",
            case.name
        );
        if disposed {
            assert_eq!(state.posts_138, None, "{}", case.name);
        } else {
            assert_eq!(state.ends_154, case.fields.ends, "{} ends", case.name);
            assert_eq!(
                state.posts_138,
                Some(case.fields.posts),
                "{} posts",
                case.name
            );
            assert_eq!(
                arena.object(object).unwrap().base.position_words,
                Some(case.fields.position),
                "{} position",
                case.name
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 13);
}

#[derive(Default)]
struct Kills {
    order: Vec<(
        &'static str,
        Option<ObjectId>,
        Option<ActorHandle>,
        Option<[u16; 3]>,
    )>,
}

impl Selector69Host for Kills {
    type Error = ();
    fn kill(
        &mut self,
        _: &mut ObjectArena,
        victim: ObjectId,
        source: ActorHandle,
    ) -> Result<(), ()> {
        self.order.push(("kill", Some(victim), Some(source), None));
        Ok(())
    }
    fn trace(&mut self, _: &mut ObjectArena, position: [u16; 3]) -> Result<(), ()> {
        self.order.push(("trace", None, None, Some(position)));
        Ok(())
    }
}

/// Every update: the age, and each kill and trace in order, over actors
/// that cross, pass above, miss the length, carry other flags, are not
/// alive, or move fast across the line.
#[test]
fn barrier_updates_match_00411380() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "update") {
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        let mut actors = Vec::new();
        for native in &case.actors {
            let actor = arena.allocate_actor(request()).unwrap();
            let base = &mut arena.owned_mut(actor).unwrap().state.base;
            base.flags = native.flags;
            base.position_words = Some(words(native.position));
            let state = arena.actor_mut(actor).unwrap();
            state.rotating.visual.radius_word = native.radius;
            state.words_8e_92 = words(native.velocity);
            state.word_c4 = native.state;
            actors.push(actor);
        }
        let mut host = Kills::default();
        arena
            .tick_selector69(object, case.delta, &mut host)
            .unwrap();
        assert_eq!(
            arena.selector69(object).unwrap().age_148,
            case.fields.age,
            "{} age",
            case.name
        );
        let native: Vec<_> = case
            .events
            .iter()
            .filter(|event| matches!(event["kind"].as_str(), Some("kill" | "trace")))
            .collect();
        assert_eq!(host.order.len(), native.len(), "{} events", case.name);
        let handle = arena.actor(object).unwrap().handle;
        for (index, (ours, theirs)) in host.order.iter().zip(&native).enumerate() {
            let label = format!("{} event {index}", case.name);
            assert_eq!(ours.0, theirs["kind"].as_str().unwrap(), "{label}");
            if ours.0 == "kill" {
                let victim = theirs["victim"].as_u64().unwrap() as usize;
                assert_eq!(ours.1, Some(actors[victim]), "{label} victim");
                assert_eq!(ours.2, Some(handle), "{label} source");
                let args: Vec<u64> = serde_json::from_value(theirs["args"].clone()).unwrap();
                assert_eq!(args, [0, u64::from(NATIVE_HANDLE), 1], "{label} 0047D670");
            } else {
                assert_eq!(
                    theirs["resource"].as_u64().unwrap() as u32,
                    BARRIER_TRACE_RESOURCE,
                    "{label}"
                );
                assert_eq!(theirs["edx"].as_u64().unwrap(), 0, "{label}");
                let args: Vec<u64> = serde_json::from_value(theirs["args"].clone()).unwrap();
                assert_eq!(
                    args,
                    [0, 0xffff_ffff, 0, u64::from(NATIVE_NULL)],
                    "{label} 00460310"
                );
                let position: Vec<u16> =
                    serde_json::from_value(theirs["position"].clone()).unwrap();
                assert_eq!(ours.3.unwrap().to_vec(), position, "{label} position");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 9);
}

/// Every light: both pulses' X/Z, radius and packed colour.
#[test]
fn barrier_lights_match_00411c50() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "light") {
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        let lights = arena.selector69_lights(object).unwrap();
        let native = events(case, "light");
        assert_eq!(native.len(), 2, "{}", case.name);
        for (light, native) in lights.iter().zip(native) {
            let xz: Vec<i32> = serde_json::from_value(native["world_xz"].clone()).unwrap();
            assert_eq!(light.world_xz.to_vec(), xz, "{}", case.name);
            assert_eq!(
                i64::from(light.radius),
                native["radius"].as_i64().unwrap(),
                "{}",
                case.name
            );
            let color = native["color"].as_u64().unwrap() as u32;
            let rgb = light.rgb.map(|channel| channel as u8);
            assert_eq!(
                u32::from_le_bytes([rgb[0], rgb[1], rgb[2], 0]),
                color,
                "{} colour",
                case.name
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 11);
}

fn basis(values: [f32; 9]) -> MenuBasis {
    let mut result = MenuBasis::INITIAL;
    result.words[1..].copy_from_slice(&values.map(f32::to_bits));
    result
}

/// Every draw: the ground samples in order; then, in native order, each
/// spark and each beam packet's kind, sort key and payload byte, over front,
/// fogged, close, behind, inside, yawed, wide and sorted views.
#[test]
fn barrier_draws_match_00411570() {
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .expect("required original retail executable");
    let executable = FrontendExecutable::parse(&raw).unwrap();
    let mode = executable.word(BARRIER_MODE_0052F628).unwrap();
    assert_eq!(mode, 4);
    let mut checked = 0;
    let mut packets = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "draw") {
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        let heights = events(case, "height");
        let mut sample = 0;
        let bolts = arena
            .selector69_geometry(object)
            .unwrap()
            .bolts::<()>(|[x, z]| {
                let native = heights[sample];
                assert_eq!(
                    [x, z],
                    [word_of(&native["x"]), word_of(&native["z"])],
                    "{}",
                    case.name
                );
                sample += 1;
                Ok(word_of(&native["height"]) as i16)
            })
            .unwrap();
        assert_eq!(sample, heights.len(), "{} samples", case.name);
        let view = SceneView {
            eye: case.eye,
            basis: basis(case.view_basis),
        };
        let origin = case.context_position.map(|value| value as u16);
        let mut projector = ModelProjectionBackend {
            transform: ModelContext::new(
                origin.map(|value| i32::from(value as i16)),
                MenuBasis::INITIAL,
            )
            .publish(view, case.coordinate_shift),
            view,
            projection: ModelProjection {
                focal: case.focal,
                viewport: case.viewport,
                center: case.center,
                fade: case.fade,
            },
            dispatch: ModelDispatch::Far,
            coordinate_shift: case.coordinate_shift,
            fog_factor: case.fog_factor,
        };
        let mut drawn: Vec<Result<[u16; 3], Selector69Quad>> = Vec::new();
        let posts = arena.selector69_posts(object).unwrap();
        let spark_list = std::cell::RefCell::new(&mut drawn);
        selector69_draw::<()>(
            posts,
            &bolts,
            case.focal[0],
            |point| {
                let local = std::array::from_fn(|axis| {
                    i32::from(point[axis].wrapping_sub(origin[axis]) as i16)
                });
                let mut output = ModelVertexProjection::default();
                let depth = match projector.project(ModelVertexInput::Local(local), &mut output) {
                    Ok(depth) => depth as i32,
                    Err(error) => match error {},
                };
                Ok(Selector69Projected {
                    xy: output.read_u32(0).unwrap(),
                    depth,
                    clip: output.bytes[8].unwrap(),
                    fog: output.bytes[9],
                })
            },
            |point| {
                spark_list.borrow_mut().push(Ok(point));
                Ok(())
            },
            |quad| {
                spark_list.borrow_mut().push(Err(quad));
                Ok(())
            },
        )
        .unwrap();
        let native: Vec<_> = case
            .events
            .iter()
            .filter(|event| matches!(event["kind"].as_str(), Some("spark" | "packet")))
            .collect();
        assert_eq!(drawn.len(), native.len(), "{} events", case.name);
        for (index, (ours, theirs)) in drawn.iter().zip(native).enumerate() {
            let label = format!("{} event {index}", case.name);
            match ours {
                Ok(point) => {
                    assert_eq!(theirs["kind"], "spark", "{label}");
                    let position: Vec<u16> =
                        serde_json::from_value(theirs["position"].clone()).unwrap();
                    assert_eq!(point.to_vec(), position, "{label} spark");
                    assert_eq!(theirs["type"], 0, "{label}");
                    assert_eq!(theirs["velocity"], serde_json::json!([0, 0, 0]), "{label}");
                    assert_eq!(theirs["velocity_pointer"], 0x0054_69fc, "{label}");
                    assert_eq!(
                        (theirs["flags"].clone(), theirs["shadow"].clone()),
                        (0.into(), 0.into()),
                        "{label}"
                    );
                }
                Err(quad) => {
                    assert_eq!(theirs["kind"], "packet", "{label}");
                    let packet = &case.packets[theirs["index"].as_u64().unwrap() as usize];
                    let sort = if case.sorting {
                        ribbon_sort_word::<std::convert::Infallible>(
                            quad.depth,
                            case.selector,
                            |_| Ok(case.texture_flags),
                            || Ok(TextureSortIdentity::NativeEntryAddress(SPRITE)),
                        )
                        .unwrap()
                    } else {
                        quad.depth
                    };
                    let command = quad.command(sort, mode, None, case.effect_color, case.far_color);
                    assert_eq!(
                        command.callback.native_address(),
                        packet.callback,
                        "{label}"
                    );
                    assert_eq!(command.depth, packet.key, "{label} key");
                    assert_eq!(command.payload_blocks, packet.blocks, "{label} blocks");
                    for (offset, expected) in packet.payload.iter().enumerate() {
                        if (0x24..0x28).contains(&offset) {
                            continue;
                        }
                        assert_eq!(
                            command.payload[offset], *expected,
                            "{label} byte {offset:#x}"
                        );
                    }
                    packets += 1;
                }
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 15);
    assert_eq!(packets, 228);
}

/// Every +98: the links, and the release's first-player lookup, grant and
/// disposal.
#[test]
fn barrier_messages_match_00411ce0() {
    let mut checked = 0;
    for case in receipt().cases.iter().filter(|case| case.kind == "message") {
        let mut arena = arena();
        let object = barrier(&mut arena, case);
        let message = arena.selector69_message(object, case.code as i32).unwrap();
        assert_eq!(case.outcome["result"], 0, "{}", case.name);
        assert_eq!(
            arena.selector69(object).unwrap().links_15c,
            case.fields.links,
            "{}",
            case.name
        );
        let kinds: Vec<&str> = case
            .events
            .iter()
            .map(|event| event["kind"].as_str().unwrap())
            .collect();
        let expected: Vec<&str> = match message {
            Selector69Message::Done => vec![],
            Selector69Message::Release { grant } => {
                assert_eq!(grant, case.grant, "{}", case.name);
                let mut kinds = Vec::new();
                if grant != 0 {
                    kinds.push("first_player");
                    if case.player != 0 {
                        kinds.push("grant");
                    }
                }
                kinds.push("dispose");
                kinds
            }
        };
        assert_eq!(kinds, expected, "{}", case.name);
        for grant in events(case, "grant") {
            assert_eq!(
                grant["ecx"].as_u64().unwrap() as u32,
                case.player + 0x9c,
                "{}",
                case.name
            );
            assert_eq!(
                grant["id"].as_u64().unwrap() as u16,
                case.grant,
                "{}",
                case.name
            );
        }
        checked += 1;
    }
    assert_eq!(checked, 13);
}

/// Level 1's barrier (placement 190) on the world's fine terrain, as
/// Infest01 and Infest02 recorded it at its first update (AF439D:F9D,
/// FEBA9:B16): commands 17 and 16 give ten bolts and a length of 3600, the
/// ground fit stands it at 340 between ends 575 and 105 with its posts, and
/// target 189's link (message 7) holds it.
#[test]
fn level1_barrier_fits_its_ground_as_infest01_recorded() {
    let (_resident, _assets, world) =
        crate::test_worlds::offline_world(18, crate::terrain_runtime::TerrainSpacing::Units128);
    let barrier = world.report.placements[190].unwrap();
    let arena = &world.arena;
    let state = arena.selector69(barrier).unwrap();
    assert_eq!(
        (
            state.bolts_144,
            state.length_14c,
            state.half_150,
            state.ends_154,
            state.links_15c
        ),
        (10, 3600, Some(1800), [575, 105], 1)
    );
    assert_eq!(
        state.posts_138,
        Some([[0x5bd4, 0xa8, 0x9251], [0x57e6, 0xc, 0x94dd]])
    );
    let base = &arena.object(barrier).unwrap().base;
    assert_eq!(base.position_words, Some([0x59dd, 0x154, 0x9397]));
    assert_eq!((base.word_28, base.flags), (0x708, 0x4890_000b));
    let actor = arena.actor(barrier).unwrap();
    assert_eq!((actor.word_74, actor.word_7a), (0, 0));
    assert_eq!(
        actor.rotating.basis.words,
        [
            3,
            0x3f56_b1ad,
            0,
            0xbf0b_5b17,
            0,
            0x3f80_0000,
            0,
            0x3f0b_5b17,
            0,
            0x3f56_b1ad
        ]
    );
}
