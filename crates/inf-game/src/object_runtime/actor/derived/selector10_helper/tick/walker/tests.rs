use super::super::super::super::selector10_tail::tests::{executable, fixture, resident, sound};
use super::super::super::{
    Selector10Helper, Selector10HelperChild, Selector10HelperChildClass, Selector10HelperClass,
};
use super::*;
use serde_json::Value;

fn number(value: &Value) -> u64 {
    value.as_u64().unwrap()
}

fn signed(value: &Value) -> i64 {
    value.as_i64().unwrap()
}

/// The fixture's 0x74 child bytes, every one known.
fn extended_child(bytes: &[u8]) -> Selector10HelperChild {
    let known = |range: std::ops::Range<usize>| bytes[range].iter().map(|b| Some(*b));
    let mut child = Selector10HelperChild {
        vtable_00: 0x0052_4eb8,
        unknown_04_13: [None; 16],
        word_14: u32::from_le_bytes(bytes[0x14..0x18].try_into().unwrap()),
        unknown_18_35: [None; 30],
        word_36: u16::from_le_bytes([bytes[0x36], bytes[0x37]]),
        unknown_38_4f: [None; 24],
        class: Selector10HelperChildClass::Extended446bc0 {
            unknown_50_73: [None; 36],
        },
    };
    for (slot, byte) in child.unknown_04_13.iter_mut().zip(known(0x04..0x14)) {
        *slot = byte;
    }
    for (slot, byte) in child.unknown_18_35.iter_mut().zip(known(0x18..0x36)) {
        *slot = byte;
    }
    for (slot, byte) in child.unknown_38_4f.iter_mut().zip(known(0x38..0x50)) {
        *slot = byte;
    }
    if let Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } = &mut child.class {
        for (slot, byte) in unknown_50_73.iter_mut().zip(known(0x50..0x74)) {
            *slot = byte;
        }
    }
    child
}

/// The child's bytes as the port holds them; None where still unknown.
fn child_bytes(child: &Selector10HelperChild) -> Vec<Option<u8>> {
    let mut bytes = vec![None; 0x74];
    bytes[0..4].copy_from_slice(&child.vtable_00.to_le_bytes().map(Some));
    bytes[0x04..0x14].copy_from_slice(&child.unknown_04_13);
    bytes[0x14..0x18].copy_from_slice(&child.word_14.to_le_bytes().map(Some));
    bytes[0x18..0x36].copy_from_slice(&child.unknown_18_35);
    bytes[0x36..0x38].copy_from_slice(&child.word_36.to_le_bytes().map(Some));
    bytes[0x38..0x50].copy_from_slice(&child.unknown_38_4f);
    if let Selector10HelperChildClass::Extended446bc0 { unknown_50_73 } = &child.class {
        bytes[0x50..0x74].copy_from_slice(unknown_50_73);
    }
    bytes
}

/// 004476D0 run natively over 103 cases (`check_selector10_walker.py`):
/// both gait tables, every gait, the phase's edges, the 0x7FFF arm of each
/// step and twelve-tick strides, each with 00445CD0 at gait 0. The port
/// matches every return, helper field, child byte, RNG state and footstep.
#[test]
fn the_walker_tick_matches_the_native_receipt() {
    let fixture_json: Value = serde_json::from_slice(include_bytes!(
        "../../../../../../../tests/fixtures/selector10_walker_native.json"
    ))
    .unwrap();
    let cases = fixture_json["walker"].as_array().unwrap();
    assert_eq!(cases.len(), 103);
    let residency = resident();
    let mut ticks = 0;
    let mut footsteps = 0;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let (mut arena, actor) = fixture();
        let helper = &case["helper"];
        let children: Vec<Selector10HelperChild> = helper["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|bytes| {
                let bytes: Vec<u8> = bytes
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|b| number(b) as u8)
                    .collect();
                extended_child(&bytes)
            })
            .collect();
        let mut slots: [Option<Box<Selector10HelperChild>>; 8] = Default::default();
        for (slot, child) in slots.iter_mut().zip(children) {
            *slot = Some(Box::new(child));
        }
        arena.selector10_actor_mut(actor).unwrap().word_268 = Some(4);
        arena.selector10_actor_mut(actor).unwrap().helper_26c = Some(Box::new(Selector10Helper {
            vtable_00: 0x0052_4ec8,
            word_04: Some(number(&helper["word_04"]) as u32),
            word_08: number(&helper["word_08"]) as u32,
            byte_0c: number(&helper["byte_0c"]) as u8,
            byte_0d: number(&helper["byte_0d"]) as u8,
            word_0e: Some(number(&helper["word_0e"]) as u16),
            table_10: number(&helper["table_10"]) as u32,
            children_14: slots,
            class: Selector10HelperClass::Family446bc0 {
                phase_34: Some(number(&helper["phase"]) as u16),
            },
        }));
        let actor_json = &case["actor"];
        arena.owned_mut(actor).unwrap().state.base.word_28 = number(&actor_json["scale"]) as u16;
        arena.owned_mut(actor).unwrap().state.base.position_words =
            Some(std::array::from_fn(|i| {
                signed(&actor_json["position"][i]) as u16
            }));
        let basis: Vec<f32> = actor_json["basis"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        {
            let words = &mut arena.actor_mut(actor).unwrap().rotating.basis.words;
            words[0] = 0;
            for (i, value) in basis.iter().enumerate() {
                words[1 + i] = value.to_bits();
            }
        }
        let mut rng = number(&case["rng"]) as u32;
        let mut sounds = sound();
        for tick in case["results"].as_array().unwrap() {
            arena.actor_mut(actor).unwrap().words_8e_92 =
                std::array::from_fn(|i| signed(&tick["velocity"][i]) as u16);
            let before = sounds.records().constructed().len();
            let result = arena
                .tick_selector10_helper(
                    actor,
                    Selector10HelperTickRequest {
                        microseconds: number(&tick["microseconds"]) as u32,
                        executable: executable(),
                        random_state: &mut rng,
                        residency: &residency,
                        sounds: &mut sounds,
                    },
                )
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert_eq!(u64::from(result), number(&tick["result"]), "{name}");
            assert_eq!(u64::from(rng), number(&tick["rng_after"]), "{name}");
            let after: Vec<u8> = tick["helper_after"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| number(b) as u8)
                .collect();
            let helper = arena
                .selector10_actor(actor)
                .unwrap()
                .helper_26c
                .as_deref()
                .unwrap();
            let dword =
                |offset: usize| u32::from_le_bytes(after[offset..offset + 4].try_into().unwrap());
            assert_eq!(helper.word_04, Some(dword(4)), "{name} +04");
            assert_eq!(helper.word_08, dword(8), "{name} +08");
            assert_eq!(
                [helper.byte_0c, helper.byte_0d],
                [after[0xc], after[0xd]],
                "{name}"
            );
            assert_eq!(
                helper.class,
                Selector10HelperClass::Family446bc0 {
                    phase_34: Some(u16::from_le_bytes([after[0x34], after[0x35]]))
                },
                "{name} +34"
            );
            for (index, expected) in tick["children_after"]
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
            {
                let held = child_bytes(helper.children_14[index].as_deref().unwrap());
                for (offset, (held, native)) in
                    held.iter().zip(expected.as_array().unwrap()).enumerate()
                {
                    assert_eq!(
                        *held,
                        Some(number(native) as u8),
                        "{name} child{index} +{offset:X}"
                    );
                }
            }
            let native_sounds: Vec<&Value> = tick["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] == "sound")
                .collect();
            let constructed = sounds.records().constructed();
            assert_eq!(
                constructed.len() - before,
                native_sounds.len(),
                "{name} sounds"
            );
            for ((slot, record), event) in constructed[before..].iter().zip(native_sounds) {
                assert_eq!(
                    slot.map(i64::from),
                    Some(signed(&event["resource"])),
                    "{name}"
                );
                let position: [u16; 3] =
                    std::array::from_fn(|i| signed(&event["position"][i]) as u16);
                assert_eq!(record.position, Some(position), "{name}");
                assert_eq!(
                    record.volume.map(u64::from),
                    Some(number(&event["volume"])),
                    "{name}"
                );
                assert_eq!(
                    record.pitch.map(u64::from),
                    Some(number(&event["pitch"])),
                    "{name}"
                );
                // 00487EC0 births a one-shot: flags 2, plus 4 for a nonzero
                // control byte.
                let control = if number(&event["flag"]) as u8 != 0 {
                    4
                } else {
                    0
                };
                assert_eq!(record.flags, Some(2 | control), "{name}");
                footsteps += usize::from(event["resource"] == 0x18);
            }
            ticks += 1;
        }
    }
    assert_eq!(ticks, 226);
    assert!(footsteps > 0);
}

/// 0048F080 against direct arithmetic: the halving, the root plus one, and
/// the saturation by sign, which only a wrapped sum of squares reaches.
#[test]
fn unit_vectors_follow_0048f080() {
    assert_eq!(unit_48f080([0, 0, 0]), [0, 0, 0]);
    // |3, 4, 0| = 5: root 5, divisor 6.
    assert_eq!(
        unit_48f080([3, 4, 0]),
        [((3_i64 << 31) / 6) as i32, ((4_i64 << 31) / 6) as i32, 0]
    );
    assert_eq!(unit_48f080([-1, 0, 0]), [((-1_i64 << 31) / 2) as i32, 0, 0]);
    // -0xD105 halves to -0x6883 while the magnitudes' OR stops at 0x6882:
    // three such squares wrap negative, the divisor is 1 and all saturate.
    assert_eq!(unit_48f080([-0xd105; 3]), [i32::MIN; 3]);
    // 0x10000 halves to 0x4000; root 0x4000, divisor 0x4001.
    assert_eq!(
        unit_48f080([0x10000, 0, 0]),
        [((0x4000_i64 << 31) / 0x4001) as i32, 0, 0]
    );
    for squared in 1..200_000_i32 {
        let root = root_48f0ed(squared);
        assert!(root * root <= squared && (root + 1) * (root + 1) > squared);
    }
}

/// 00447B30 reads each leg's four keyframes at the stride phase and
/// discards the value; its divisor, the key span masked to 0x7FFF, is
/// never zero in either table at any phase, so the call has no effect.
#[test]
fn no_keyframe_read_divides_by_zero() {
    let exe = executable();
    for table in [TWO_LEG_GAITS, OTHER_GAITS] {
        let legs = signed_byte(exe, table).unwrap();
        let gaits = signed_byte(exe, table + 1).unwrap();
        for gait in 0..gaits as u32 {
            for leg in 0..legs as u32 {
                let keys = table + 2 + GAIT_BYTES * gait + 4 + LEG_BYTES * leg + 8;
                let key = |i: u32| word(exe, keys + 4 * i).unwrap() as i16;
                for phase in 0..0x8000_i32 {
                    let mut found = 0;
                    while found < 4 && phase > i32::from(key(found)) {
                        found += 1;
                    }
                    let (high, low) = if found == 0 || found == 4 {
                        (0, 3)
                    } else {
                        (found, found - 1)
                    };
                    let span = (i32::from(key(high) as u16) - i32::from(key(low))) & 0x7fff;
                    assert_ne!(
                        span, 0,
                        "{table:08X} gait {gait} leg {leg} phase {phase:#x}"
                    );
                }
            }
        }
    }
}

fn walker_receipt() -> Value {
    serde_json::from_slice(include_bytes!(
        "../../../../../../../tests/fixtures/selector10_walker_native.json"
    ))
    .unwrap()
}

/// 00447AE0 and 00447B00 run natively (`check_selector10_walker.py`'s
/// `blend`): differences at and around 0x4000 and 0x8000, with random
/// upper bits on both registers and on t, whose low WORD alone counts.
#[test]
fn blends_match_00447ae0_and_00447b00() {
    let receipt = walker_receipt();
    let cases = receipt["blend"].as_array().unwrap();
    assert_eq!(cases.len(), 136);
    for case in cases {
        let (a, b) = (number(&case["a"]) as u16, number(&case["b"]) as u16);
        let t = number(&case["t"]) as u32 as i32;
        assert_eq!(
            u64::from(lerp_447ae0(a, b, t)),
            number(&case["lerp"]),
            "{case}"
        );
        assert_eq!(
            u64::from(turn_447b00(a, b, t)),
            number(&case["turn"]),
            "{case}"
        );
    }
}

/// 0048F080 run natively (`unit`): magnitudes either side of 0x6882 after
/// halving, negatives whose halving rounds away, the wrapped sum that
/// saturates, zero and random vectors.
#[test]
fn unit_vectors_match_0048f080() {
    let receipt = walker_receipt();
    let cases = receipt["unit"].as_array().unwrap();
    assert_eq!(cases.len(), 76);
    for case in cases {
        let vector: [i32; 3] = std::array::from_fn(|i| signed(&case["vector"][i]) as i32);
        let unit: [i32; 3] = std::array::from_fn(|i| signed(&case["unit"][i]) as i32);
        assert_eq!(unit_48f080(vector), unit, "{vector:?}");
    }
}
