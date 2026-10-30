//! Source and required-backing tests; no whole-eye or retail-pixel acceptance.
use super::*;

fn subject77(arena: &mut ObjectArena, behavior: bool) -> ObjectId {
    let source = arena
        .allocate_selector77_actor(allocation(), &mut 17)
        .unwrap();
    assert_eq!(arena.selector77_actor(source).unwrap().position_192, None);
    if behavior {
        arena.derived_actor_mut(source).unwrap().byte_13d = Some(6);
        assert!(arena.admit_derived_actor(source).unwrap());
    }
    source
}

#[test]
fn selector77_unknown_constructor_fields_are_not_eager_callback_defaults() {
    let mut arena = arena();
    let source = subject77(&mut arena, false);
    let state = arena.selector77_actor(source).unwrap();
    assert_eq!(
        (
            state.position_192,
            state.word_19c,
            state.word_19e,
            state.word_1a0,
            state.word_1ac
        ),
        (None, None, None, None, None)
    );
    let before = format!("{arena:?}");
    let mut output = Some([7, 8, 9]);
    for (extras, error) in [
        (
            vec![],
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: 2,
                index: 0,
            },
        ),
        (
            vec![0xffff],
            ObjectCustomVertexError::Vertex(ModelVertexFault::InvalidSlot {
                index: 65535,
                count: 4,
            }),
        ),
        (
            vec![2],
            ObjectCustomVertexError::Vertex(ModelVertexFault::UnknownWorld { index: 2 }),
        ),
    ] {
        assert_eq!(
            compute(
                &mut arena,
                source,
                2,
                &extras,
                &vertices([1, 2, 3]),
                &mut output
            ),
            Err(error)
        );
        assert_eq!(output, Some([7, 8, 9]));
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn selector77_lowbyte2_aliases_capture_each_reach_and_publish_wrapped74() {
    let mut arena = arena();
    let source = subject77(&mut arena, false);
    arena.copy_actor_position(source, [9, 0x7ffc, 11]).unwrap();
    for selector in [2, 0x102, -254, i32::MIN + 2] {
        let mut output = None;
        compute(
            &mut arena,
            source,
            selector,
            &[0],
            &vertices([0x1_8001, -0x1_8000, 0x1_ffff]),
            &mut output,
        )
        .unwrap();
        assert_eq!(
            arena.selector77_actor(source).unwrap().position_192,
            Some([0x8001, 0x8000, 0xffff])
        );
        assert_eq!(output, Some([-32767, -32768, -1]));
        assert_eq!(arena.actor(source).unwrap().word_74, 4);
    }
    let mut output = None;
    compute(
        &mut arena,
        source,
        2,
        &[0],
        &vertices([17, 23, 31]),
        &mut output,
    )
    .unwrap();
    assert_eq!(
        arena.selector77_actor(source).unwrap().position_192,
        Some([17, 23, 31])
    );
    assert_eq!(output, Some([17, 23, 31]));
    assert_eq!(
        arena.actor(source).unwrap().word_74,
        23_u16.wrapping_sub(0x7ffc)
    );
}

#[test]
fn selector77_lowbyte10_is_unsigned_row_only_without_actor_or_behavior_reads() {
    let mut arena = arena();
    let source = subject77(&mut arena, true);
    arena.actor_mut(source).unwrap().word_74 = 0xbeef;
    let before = format!("{arena:?}");
    for selector in [10, 0x10a, i32::from(0xff0a_u16 as i16)] {
        let mut output = None;
        compute(
            &mut arena,
            source,
            selector,
            &[0],
            &vertices([0x18001, 0x18000, 0x1ffff]),
            &mut output,
        )
        .unwrap();
        assert_eq!(output, Some([-32767, -32768, -1]));
        assert_eq!(format!("{arena:?}"), before);
    }
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            10,
            &[0x8000],
            &ModelVertices::new_invocation(u16::MAX),
            &mut output
        ),
        Err(ObjectCustomVertexError::Vertex(
            ModelVertexFault::UnknownWorld { index: 32768 }
        ))
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(format!("{arena:?}"), before);
}

#[test]
fn selector77_capture_precedes_optional_behavior_extra_and_hard_group_backing() {
    let mut arena = arena();
    let source = subject77(&mut arena, true);
    arena.actor_mut(source).unwrap().word_74 = 0xbeef;
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredExtra {
            object: source,
            selector: 2,
            index: 1
        })
    );
    assert_eq!(
        arena.selector77_actor(source).unwrap().position_192,
        Some([1, 2, 3])
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(arena.actor(source).unwrap().word_74, 0xbeef);
    let behavior = arena
        .derived_actor_mut(source)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap();
    behavior.byte_0d = 1;
    behavior.groups_1c.clear();
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0, 0],
            &vertices([4, 5, 6]),
            &mut output
        ),
        Err(ObjectCustomVertexError::BehaviorGroup {
            object: source,
            index: 0,
            groups: 0
        })
    );
    assert_eq!(
        arena.selector77_actor(source).unwrap().position_192,
        Some([4, 5, 6])
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(arena.actor(source).unwrap().word_74, 0xbeef);
}

#[test]
fn selector77_real48cf70_prefix_precedes_output_and74_on_unknown_actor_x() {
    let mut arena = arena();
    let source = subject77(&mut arena, true);
    arena.actor_mut(source).unwrap().word_74 = 0xbeef;
    let mut output = Some([7, 8, 9]);
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0, 0],
            &vertices([0x18001, 0x18000, 0x1ffff]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x32
        })
    );
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [None, None, None, Some(0x8001), Some(0x8000), Some(0xffff)]
    );
    assert_eq!(
        group.word_0c, 0,
        "world stores then bit4 clear survive the first actor read"
    );
    assert_eq!(
        arena.selector77_actor(source).unwrap().position_192,
        Some([0x8001, 0x8000, 0xffff])
    );
    assert_eq!(output, Some([7, 8, 9]));
    assert_eq!(arena.actor(source).unwrap().word_74, 0xbeef);
}

#[test]
fn selector77_signed_group_bypass_still_publishes_output_before_required_actor_y() {
    let mut arena = arena();
    let source = subject77(&mut arena, true);
    arena.actor_mut(source).unwrap().word_74 = 0xbeef;
    arena
        .derived_actor_mut(source)
        .unwrap()
        .behavior_140
        .as_mut()
        .unwrap()
        .byte_0d = 0x80;
    let mut output = None;
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0, 0xffff],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x34
        })
    );
    assert_eq!(output, Some([1, 2, 3]));
    assert_eq!(
        arena.selector77_actor(source).unwrap().position_192,
        Some([1, 2, 3])
    );
    assert_eq!(arena.actor(source).unwrap().word_74, 0xbeef);
    let group = &arena
        .derived_actor(source)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(group.coordinates_00_0a, [None; 6]);
    assert_eq!(group.word_0c, 4);
}

#[test]
fn selector77_output_before_y_error_and_real_behavior_normal_completion() {
    let mut arena = arena();
    let source = subject77(&mut arena, false);
    arena.actor_mut(source).unwrap().word_74 = 0xbeef;
    let mut output = None;
    assert_eq!(
        compute(
            &mut arena,
            source,
            2,
            &[0],
            &vertices([1, 2, 3]),
            &mut output
        ),
        Err(ObjectCustomVertexError::RequiredField {
            object: source,
            offset: 0x34
        })
    );
    assert_eq!(output, Some([1, 2, 3]));
    assert_eq!(arena.actor(source).unwrap().word_74, 0xbeef);
    let with_behavior = subject77(&mut arena, true);
    arena.copy_actor_position(with_behavior, [1, 2, 3]).unwrap();
    compute(
        &mut arena,
        with_behavior,
        2,
        &[0, 0],
        &vertices([4, 8, 12]),
        &mut output,
    )
    .unwrap();
    let group = &arena
        .derived_actor(with_behavior)
        .unwrap()
        .behavior_140
        .as_ref()
        .unwrap()
        .groups_1c[0];
    assert_eq!(
        group.coordinates_00_0a,
        [Some(3), Some(6), Some(9), Some(4), Some(8), Some(12)]
    );
    assert_eq!(group.word_0c, 3);
    assert_eq!(
        arena.selector77_actor(with_behavior).unwrap().position_192,
        Some([4, 8, 12])
    );
    assert_eq!(output, Some([4, 8, 12]));
    assert_eq!(arena.actor(with_behavior).unwrap().word_74, 6);
}

#[test]
fn selector77_other_lowbytes_leave_the_default_terrain_arm_explicitly_unowned() {
    let mut arena = arena();
    let source = subject77(&mut arena, false);
    let before = format!("{arena:?}");
    for selector in [0, 3, 0x109, -1] {
        let mut output = Some([7, 8, 9]);
        assert_eq!(
            compute(
                &mut arena,
                source,
                selector,
                &[],
                &ModelVertices::new_invocation(0),
                &mut output
            ),
            Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector
            })
        );
        assert_eq!(output, Some([7, 8, 9]));
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn selector77_complete_original_callback_constructor_and_table_guards() {
    use inf_formats::FrontendExecutable;
    use sha2::{Digest, Sha256};
    let raw = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/Infest.exe"),
    )
    .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    let executable = FrontendExecutable::parse(&raw).unwrap();
    //The supported original PE .text maps raw offset=RVA. mapped() remains
    //file-backed .data/.rdata only; text is guarded through original raw bytes.
    assert_eq!(&raw[0x1e0..0x1e8], b".text\0\0\0");
    assert_eq!(
        u32::from_le_bytes(raw[0x1ec..0x1f0].try_into().unwrap()),
        0x1000
    );
    assert_eq!(
        u32::from_le_bytes(raw[0x1f4..0x1f8].try_into().unwrap()),
        0x1000
    );
    for (address, length, expected) in [
        (
            0x407410_usize,
            121,
            "de7cb9f1b1d315c9b03dbd4c9ebe61dca2f3fc2379d5f2261099479b707342b2",
        ),
        (
            0x4074a0,
            38,
            "4c750f41382932362490f890f29aa9c7bfdf99756e5af4c26e52b36f3f708470",
        ),
        (
            0x4074e0,
            273,
            "bad5368a465558beb8c0c9814eb8b0682c1fb59838448abffa0b766725e8793b",
        ),
        (
            0x408090,
            38,
            "37935d19f16ddf4d51b43e5ef83036efc70ddf8f73dfeaafb3e95f8a56c2f934",
        ),
        (
            0x439e10,
            37,
            "e802424855ab95d69bc935e1e0b89bc44221cfde57a4efa68ae7cb28326c60fd",
        ),
        (
            0x48cf70,
            144,
            "8c857c0def30cfe9c95082bdf424646674abb1b441c85772db581a5fd65f123a",
        ),
    ] {
        let at = address - 0x400000;
        assert_eq!(
            format!("{:x}", Sha256::digest(&raw[at..at + length])),
            expected,
            "body{address:08X}"
        );
    }
    let table = executable.mapped(0x51fbb0, 0x50).unwrap().bytes;
    let entry = |offset| u32::from_le_bytes(table[offset..offset + 4].try_into().unwrap());
    assert_eq!(entry(0x10), 0x4074e0);
    assert_eq!(entry(0x14), 0x4074a0);
    assert_eq!(entry(0x1c), 0x408090);
    assert_eq!(entry(0x2c), 0x47d910);
    assert_eq!(entry(0x48), 0x4026b0);
}
