use serde::Deserialize;
use serde_json::{json, Value};

use super::*;
use crate::frontend_assets::FrontendAssets;

#[derive(Deserialize)]
struct Receipt {
    segment: Vec<SegmentCase>,
    init: Vec<InitCase>,
    knee: Vec<KneeCase>,
    custom: Vec<CustomCase>,
    extended_init: Vec<ExtendedInitCase>,
    extended_knee: Vec<ExtendedKneeCase>,
    extended_custom: Vec<CustomCase>,
}

#[derive(Clone, Copy, Deserialize)]
struct Ground {
    base: i32,
    sx: i32,
    sz: i32,
    water: i32,
    flags: u32,
}

#[derive(Deserialize)]
struct SegmentCase {
    name: String,
    input: [i16; 3],
    direction: [f32; 3],
    length: i32,
    water: u32,
    iterations: u32,
    ground: Ground,
    output: [i16; 3],
    result: u8,
    events: Vec<Value>,
}

#[derive(Deserialize)]
struct ChildBytes {
    bytes: Vec<u8>,
}

#[derive(Deserialize)]
struct InitCase {
    name: String,
    hip: [i16; 3],
    foot: [i16; 3],
    knee_row: usize,
    rows: std::collections::BTreeMap<String, [i32; 3]>,
    child: ChildBytes,
    after: Vec<u8>,
}

#[derive(Deserialize)]
struct KneeCase {
    name: String,
    hip: [i16; 3],
    basis: [f32; 9],
    mode: u32,
    child: ChildBytes,
    after: Vec<u8>,
}

#[derive(Deserialize)]
struct ExtendedInitCase {
    name: String,
    hip: [i16; 3],
    foot: [i16; 3],
    knee_row: usize,
    ankle_row: usize,
    front: u32,
    rows: std::collections::BTreeMap<String, [i32; 3]>,
    child: ChildBytes,
    after: Vec<u8>,
}

#[derive(Deserialize)]
struct ExtendedKneeCase {
    name: String,
    hip: [i16; 3],
    basis: [f32; 9],
    mode: u32,
    sense: u32,
    child: ChildBytes,
    after: Vec<u8>,
}

#[derive(Deserialize)]
struct Actor {
    basis: [f32; 9],
    lifecycle: i32,
    word_188: i64,
    word_240: u32,
    table_244: u32,
}

#[derive(Deserialize)]
struct Helper {
    word_08: u32,
    byte_0c: u8,
    byte_0d: u8,
    table_10: u32,
    children: Vec<ChildBytes>,
}

#[derive(Deserialize)]
struct Call {
    selector: i32,
    extras: [u16; 2],
    #[serde(default)]
    patch_flags: std::collections::BTreeMap<String, u32>,
    output: [Option<i32>; 3],
    events: Vec<Value>,
}

#[derive(Deserialize)]
struct CustomCase {
    name: String,
    rows: std::collections::BTreeMap<String, [i32; 3]>,
    rng: u32,
    clock: u32,
    particle_block: u8,
    ground: Ground,
    actor: Actor,
    helper: Option<Helper>,
    results: Vec<Call>,
    children_after: Vec<Vec<u8>>,
    rng_after: u32,
}

/// The receipt's stubs: terrain base + (sx*x>>8) + (sz*z>>8) over the signed
/// low WORDs, a constant water level, the real 0048F060 RNG and NULL births.
struct Recording {
    ground: Ground,
    clock: u32,
    rng: u32,
    byte: u8,
    events: Vec<Value>,
}

impl Selector10LegGround for Recording {
    fn height(&mut self, x: u16, z: u16, clock: u32) -> i16 {
        let (x, z) = (i32::from(x as i16), i32::from(z as i16));
        let height = self
            .ground
            .base
            .wrapping_add((self.ground.sx * x) >> 8)
            .wrapping_add((self.ground.sz * z) >> 8) as i16;
        self.events
            .push(json!({"kind": "terrain", "x": x, "z": z, "clock": clock, "height": height}));
        height
    }

    fn water(&mut self, x: u16, z: u16, clock: u32, flags: u32, terrain: i16) -> i16 {
        let height = self.ground.water as i16;
        self.events.push(
            json!({"kind": "water", "x": x as i16, "z": z as i16, "clock": clock,
            "flag": flags, "terrain": terrain, "height": height}),
        );
        height
    }
}

impl Selector10CustomServices for Recording {
    fn clock(&self) -> u32 {
        self.clock
    }

    fn world_flags(&self) -> u32 {
        self.ground.flags
    }

    fn update_byte(&self) -> u8 {
        self.byte
    }

    fn random(&mut self) -> u32 {
        let value = crate::name_entry::name_entry_rng_next(&mut self.rng);
        self.events
            .push(json!({"kind": "random", "value": value & 0xffff, "state": self.rng}));
        value
    }

    fn birth_trail(&mut self, trail: FurnitureCustomTrail) -> Result<(), String> {
        self.events.push(json!({"kind": "trail", "particle_type": trail.particle_type,
            "position": trail.position.map(|w| w as i16), "velocity": trail.velocity.map(|w| w as i16),
            "flags": trail.flags}));
        Ok(())
    }
}

fn recording(ground: Ground, clock: u32, rng: u32, byte: u8) -> Recording {
    Recording {
        ground,
        clock,
        rng,
        byte,
        events: Vec::new(),
    }
}

fn child(bytes: &[u8]) -> Selector10HelperChild {
    let known =
        |range: std::ops::Range<usize>| bytes[range].iter().map(|b| Some(*b)).collect::<Vec<_>>();
    // The harness installs the base child table over the random bytes.
    Selector10HelperChild {
        vtable_00: 0x0052_4e88,
        unknown_04_13: known(0x04..0x14).try_into().unwrap(),
        word_14: u32::from_le_bytes(bytes[0x14..0x18].try_into().unwrap()),
        unknown_18_35: known(0x18..0x36).try_into().unwrap(),
        word_36: u16::from_le_bytes(bytes[0x36..0x38].try_into().unwrap()),
        unknown_38_4f: known(0x38..0x50).try_into().unwrap(),
        class: Selector10HelperChildClass::Base445a70,
    }
}

/// The harness installs the extended child table over 0x74 random bytes.
fn extended_child(bytes: &[u8]) -> Selector10HelperChild {
    let mut owned = child(bytes);
    owned.vtable_00 = 0x0052_4eb8;
    owned.class = Selector10HelperChildClass::Extended446bc0 {
        unknown_50_73: bytes[0x50..0x74]
            .iter()
            .map(|b| Some(*b))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap(),
    };
    owned
}

fn bytes(child: &Selector10HelperChild) -> Vec<u8> {
    let mut out = child.vtable_00.to_le_bytes().to_vec();
    out.extend(child.unknown_04_13.iter().map(|b| b.unwrap()));
    out.extend(child.word_14.to_le_bytes());
    out.extend(child.unknown_18_35.iter().map(|b| b.unwrap()));
    out.extend(child.word_36.to_le_bytes());
    out.extend(child.unknown_38_4f.iter().map(|b| b.unwrap()));
    if let Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } = &child.class {
        out.extend(unknown_50_73.iter().map(|b| b.unwrap()));
    }
    out
}

fn words(vector: [i16; 3]) -> [u16; 3] {
    vector.map(|word| word as u16)
}

fn row(rows: &std::collections::BTreeMap<String, [i32; 3]>, index: usize) -> [u16; 3] {
    rows[&index.to_string()].map(|axis| axis as u16)
}

fn receipt() -> Receipt {
    serde_json::from_str(include_str!(
        "../../../../../../tests/fixtures/selector10_legs_native.json"
    ))
    .unwrap()
}

/// 0043A380 under every direction sign, doubled-length wrap, water byte and
/// refinement count: output, AL and every ground sample in order.
#[test]
fn segment_probe_matches_original_0043a380() {
    let receipt = receipt();
    for case in &receipt.segment {
        let mut ground = recording(case.ground, 0x12345, 0, 0);
        let (output, result) = segment_probe_43a380(
            &mut ground,
            SegmentProbe {
                input: words(case.input),
                direction: case.direction,
                length: case.length,
                water: case.water,
                iterations: case.iterations,
                clock: 0x12345,
                world_flags: case.ground.flags,
            },
        )
        .unwrap();
        assert_eq!(output.map(|w| w as i16), case.output, "{}", case.name);
        assert_eq!(u8::from(result), case.result, "{}", case.name);
        assert_eq!(ground.events, case.events, "{}", case.name);
    }
    assert_eq!(receipt.segment.len(), 75);
}

/// 00445F70: rest lengths, reach and stride thresholds from three rows.
#[test]
fn leg_setup_matches_original_00445f70() {
    let receipt = receipt();
    for case in &receipt.init {
        let mut owned = child(&case.child.bytes);
        let mut leg = Child {
            child: &mut owned,
            index: 0,
        };
        leg_setup_445f70(
            &mut leg,
            words(case.hip),
            words(case.foot),
            row(&case.rows, case.knee_row),
        );
        assert_eq!(bytes(&owned), case.after, "{}", case.name);
    }
    assert_eq!(receipt.init.len(), 30);
}

/// 004460B0 over all six bend modes, zero-length legs and unreachable feet.
#[test]
fn knee_matches_original_004460b0() {
    let receipt = receipt();
    for case in &receipt.knee {
        let mut owned = child(&case.child.bytes);
        let mut leg = Child {
            child: &mut owned,
            index: 0,
        };
        knee_4460b0(&mut leg, words(case.hip), case.basis, case.mode).unwrap();
        assert_eq!(bytes(&owned), case.after, "{}", case.name);
    }
    assert_eq!(receipt.knee.len(), 90);
}

/// 00425870 -> 004463E0 frame sequences: setup, first frames, standing and
/// stepping legs, water, both clamps, every trail arm and the 6/4/2-leg
/// descriptors. Outputs, ground samples, draws, trails and final children.
#[test]
fn custom_vertex_matches_original_00425870() {
    let receipt = receipt();
    assert_eq!(
        (receipt.custom.len(), run_custom(&receipt.custom, false)),
        (78, 25)
    );
}

/// The same frames on mode 4's helper: two extended legs set up by
/// 00446EE0 and solved by 004470C0, the ankle +56..+5A as selector 5, first
/// frames, water, the helper off, standing and stepping legs.
#[test]
fn extended_custom_vertex_matches_original_00425870() {
    let receipt = receipt();
    run_custom(&receipt.extended_custom, true);
    assert_eq!(receipt.extended_custom.len(), 20);
}

/// 00446EE0 over random rows, hips, feet and both leg pairs.
#[test]
fn extended_leg_setup_matches_original_00446ee0() {
    let receipt = receipt();
    for case in &receipt.extended_init {
        let mut owned = extended_child(&case.child.bytes);
        let mut leg = Child {
            child: &mut owned,
            index: 0,
        };
        leg_setup_446ee0(
            &mut leg,
            words(case.hip),
            words(case.foot),
            row(&case.rows, case.knee_row),
            row(&case.rows, case.ankle_row),
            case.front & 0xff != 0,
        );
        assert_eq!(bytes(&owned), case.after, "{}", case.name);
    }
    assert_eq!(receipt.extended_init.len(), 30);
}

/// 004470C0 over all six bend modes, both senses, legs within, at and
/// beyond their reach, nearly folded and at the hip, and second stages whose
/// knee-foot distance lands on shin + toe or one below; the mode's and the
/// sense's high WORDs are ignored.
#[test]
fn extended_knee_matches_original_004470c0() {
    let receipt = receipt();
    for case in &receipt.extended_knee {
        let mut owned = extended_child(&case.child.bytes);
        let mut leg = Child {
            child: &mut owned,
            index: 0,
        };
        knee_4470c0(
            &mut leg,
            words(case.hip),
            case.basis,
            case.mode,
            case.sense as u16,
        )
        .unwrap();
        assert_eq!(bytes(&owned), case.after, "{}", case.name);
    }
    assert_eq!(receipt.extended_knee.len(), 166);
}

/// Runs custom-vertex frame cases; returns the trail count.
fn run_custom(cases: &[CustomCase], extended: bool) -> usize {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = FrontendAssets::open(&root).unwrap();
    let executable = FrontendExecutable::parse(assets.executable_bytes()).unwrap();
    let sine = &assets.sine_table().values;
    let mut trails = 0;
    for case in cases {
        let mut rows = ModelVertices::new_invocation(256);
        for (index, row) in &case.rows {
            rows.seed_world(index.parse().unwrap(), *row);
        }
        let mut helper = case.helper.as_ref().map(|helper| {
            let mut owned = Selector10Helper::for_selector(if extended { 4 } else { 1 }).unwrap();
            owned.word_08 = helper.word_08;
            owned.byte_0c = helper.byte_0c;
            owned.byte_0d = helper.byte_0d;
            owned.table_10 = helper.table_10;
            owned.children_14 = std::array::from_fn(|index| {
                helper.children.get(index).map(|bytes| {
                    Box::new(if extended {
                        extended_child(&bytes.bytes)
                    } else {
                        child(&bytes.bytes)
                    })
                })
            });
            owned
        });
        let mut services = recording(case.ground, case.clock, case.rng, case.particle_block);
        for call in &case.results {
            if let Some(helper) = helper.as_mut() {
                for (index, flags) in &call.patch_flags {
                    let index: usize = index.parse().unwrap();
                    helper.children_14[index].as_mut().unwrap().word_14 = *flags;
                }
            }
            services.events.clear();
            let inputs = Selector10CustomInputs {
                selector: call.selector,
                extras: &call.extras,
                rows: &rows,
                basis: case.actor.basis,
                lifecycle: case.actor.lifecycle,
                word_188: Some(case.actor.word_188 as u32),
                word_240: case.actor.word_240,
                table_244: case.actor.table_244,
                executable,
                sine,
            };
            let output =
                selector10_custom_425870(helper.as_deref_mut(), &inputs, &mut services).unwrap();
            let expected = match call.output {
                [Some(x), Some(y), Some(z)] => Some([x, y, z]),
                [None, None, None] => None,
                other => panic!("{}: partial output {other:?}", case.name),
            };
            assert_eq!(output, expected, "{} {:x}", case.name, call.selector);
            assert_eq!(
                services.events, call.events,
                "{} {:x}",
                case.name, call.selector
            );
            trails += call
                .events
                .iter()
                .filter(|event| event["kind"] == "trail")
                .count();
        }
        let after: Vec<Vec<u8>> = helper
            .iter()
            .flat_map(|helper| {
                helper
                    .children_14
                    .iter()
                    .flatten()
                    .map(|child| bytes(child))
            })
            .collect();
        assert_eq!(after, case.children_after, "{}", case.name);
        assert_eq!(services.rng, case.rng_after, "{}", case.name);
    }
    trails
}
