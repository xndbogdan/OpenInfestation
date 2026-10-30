use super::*;
use inf_formats::{
    BehaviorResourcePayload as P, BehaviorResourcePayloadTail as T, ByteSpan, ConstructorField,
};

fn field<V>(value: V) -> ConstructorField<V> {
    ConstructorField {
        span: ByteSpan::new(0, 0),
        value,
    }
}

fn packet(tail: T) -> P {
    P::Packet44 {
        bytes_44: field([0x8c, 0x0a, 4, 0, 1, 2, 3, 4, 0xc8, 0]),
        tail,
    }
}

fn object() -> ObjectId {
    let mut arena = ObjectArena::new();
    arena.allocate(crate::object_runtime::ObjectClassKind::Base)
}

fn family(state: &BehaviorResourceState) -> &Family455560State {
    match &state.class {
        BehaviorResourceClass::Family455560(state) => state,
        BehaviorResourceClass::Family485f50(_) => panic!("00455560's family"),
    }
}

#[test]
fn constructors_store_each_class_default() {
    let null = ActorHandle::from_raw(0x1234);
    for selector in [24_u16, 30, 33, 76] {
        assert!(
            BehaviorResourceState::new(selector, null).is_none(),
            "{selector}"
        );
    }
    let s32 = BehaviorResourceState::new(32, null).unwrap();
    assert_eq!(
        s32.class,
        BehaviorResourceClass::Family485f50(Family485f50State {
            words_138: None,
            dword_140: 600_000,
            word_144: 0,
            word_146: None,
            byte_148: 6,
            dword_14c: 1,
            selector31: None,
        })
    );
    let s31 = BehaviorResourceState::new(31, null).unwrap();
    assert!(matches!(
        s31.class,
        BehaviorResourceClass::Family485f50(Family485f50State {
            byte_148: 8,
            selector31: Some((0, 0)),
            ..
        })
    ));
    // 00455560(0x7D0), then the tails.
    let s28 = BehaviorResourceState::new(28, null).unwrap();
    assert_eq!(
        (
            family(&s28).byte_138,
            family(&s28).word_13a,
            family(&s28).dword_148
        ),
        (1, 0x7d0, 0x36)
    );
    let s25 = BehaviorResourceState::new(25, null).unwrap();
    assert_eq!(family(&s25).word_13a, 10_000);
    let s38 = BehaviorResourceState::new(38, null).unwrap();
    assert_eq!((family(&s38).byte_138, family(&s38).word_13a), (0, 0x7d0));
    // 00457FE0: disarmed, its fuse length +14C copied from +13A.
    let s29 = BehaviorResourceState::new(29, null).unwrap();
    assert_eq!((family(&s29).byte_138, family(&s29).word_13a), (0, 0x7d0));
    assert_eq!(
        family(&s29).tail,
        Family455560Tail::Selector29 { dword_14c: 0x7d0 }
    );
    let s27 = BehaviorResourceState::new(27, null).unwrap();
    assert!(matches!(
        family(&s27).tail,
        Family455560Tail::Flags14c {
            handle_154: Some(handle),
            offset_158: None,
            dword_150: 5_000_000,
            ..
        } if handle == null
    ));
    assert!(matches!(
        family(&BehaviorResourceState::new(26, null).unwrap()).tail,
        Family455560Tail::Flags14c {
            handle_154: None,
            ..
        }
    ));
}

#[test]
fn initializers_copy_the_packet_and_apply_their_reads() {
    let null = ActorHandle::from_raw(0);
    let id = object();
    // 00485440: WORD +44 only; the packet is +148 then +146.
    let mut s32 = BehaviorResourceState::new(32, null).unwrap();
    assert_eq!(s32.packet_strength_146(), Some([None, None]));
    s32.initialize(
        id,
        Some(&P::Strength44 {
            word_44: field(0xc8),
        }),
    )
    .unwrap();
    assert_eq!(s32.packet_channel_148(), Some(Some(6)));
    assert_eq!(s32.packet_strength_146(), Some([Some(0xc8), Some(0)]));
    assert_eq!(s32.packet_13e(), None);
    assert!(s32.component_6c() && !s32.component_68());
    // 004556F0: the ten bytes, the packet DWORD is the first four.
    let mut s28 = BehaviorResourceState::new(28, null).unwrap();
    assert_eq!(s28.packet_13e(), Some([None; 4]));
    s28.initialize(id, Some(&packet(T::None))).unwrap();
    assert_eq!(s28.packet_13e(), Some([0x8c, 0x0a, 4, 0].map(Some)));
    assert!(s28.component_68() && !s28.component_6c());
    // A payload of another initializer is refused.
    assert!(BehaviorResourceState::new(28, null)
        .unwrap()
        .initialize(id, Some(&P::Strength44 { word_44: field(1) }))
        .is_err());
    assert!(BehaviorResourceState::new(26, null)
        .unwrap()
        .initialize(id, Some(&packet(T::None)))
        .is_err());
    assert!(BehaviorResourceState::new(32, null)
        .unwrap()
        .initialize(id, None)
        .is_err());
}

#[test]
fn flag_byte_54_adjusts_lifetime_and_class_in_native_order() {
    let null = ActorHandle::from_raw(0);
    let id = object();
    for (byte_54, word_13a, dword_148) in [
        (0_u8, 0x7d0_u16, 0x36_u32),
        (1, 0x7d0 + 0x1f40, 0x36),
        (4, 0x7d0, 0x37),
        // Bit 1 is tested after bit 2 and wins.
        (6, 0x7d0, 0x39),
        (7, 0x7d0 + 0x1f40, 0x39),
    ] {
        let mut s26 = BehaviorResourceState::new(26, null).unwrap();
        s26.initialize(
            id,
            Some(&packet(T::Bytes50And54 {
                byte_50: field(0x55),
                byte_54: field(byte_54),
            })),
        )
        .unwrap();
        let state = family(&s26);
        assert_eq!(
            (state.word_13a, state.dword_148),
            (word_13a, dword_148),
            "{byte_54}"
        );
        assert!(matches!(
            state.tail,
            Family455560Tail::Flags14c {
                byte_14c: Some(b14c),
                byte_14d: Some(0x55),
                ..
            } if b14c == byte_54
        ));
    }
    // Selector 38: a zero DWORD +58 forces +148 = 0x37 after the bits.
    for (dword_58, expected) in [(0_u32, 0x37_u32), (9, 0x39)] {
        let mut s38 = BehaviorResourceState::new(38, null).unwrap();
        s38.initialize(
            id,
            Some(&packet(T::Bytes50And54Dwords58And5c {
                byte_50: field(0),
                byte_54: field(2),
                dword_58: field(dword_58),
                dword_5c: field(0xdead_beef),
            })),
        )
        .unwrap();
        let state = family(&s38);
        assert_eq!(state.dword_148, expected);
        assert!(matches!(
            state.tail,
            Family455560Tail::Selector38 {
                byte_154: Some(present),
                dword_158: Some(0xdead_beef),
                ..
            } if present == u8::from(dword_58 != 0)
        ));
    }
}

#[test]
fn selector25_compares_word_50_signed() {
    let null = ActorHandle::from_raw(0);
    let id = object();
    for (word_50, byte_15e) in [(599_u16, 1_u8), (600, 0), (0x8000, 1), (0xffff, 1)] {
        let mut s25 = BehaviorResourceState::new(25, null).unwrap();
        s25.initialize(
            id,
            Some(&packet(T::Word50Bytes52And53 {
                word_50: field(word_50),
                byte_52: field(0),
                byte_53: field(7),
            })),
        )
        .unwrap();
        let Family455560Tail::Selector25 {
            word_15a,
            byte_15c,
            byte_15d,
            byte_15e: actual,
            ..
        } = family(&s25).tail
        else {
            unreachable!()
        };
        assert_eq!((word_15a, byte_15c, byte_15d), (word_50, 0, 1));
        assert_eq!(actual, byte_15e, "{word_50:#x}");
    }
}

#[test]
fn model_value_14_reads_byte_138_for_selector_one() {
    let null = ActorHandle::from_raw(0);
    let s28 = BehaviorResourceState::new(28, null).unwrap();
    assert_eq!(s28.model_value_14(1), 1);
    assert_eq!(s28.model_value_14(0), 0);
    assert_eq!(s28.model_value_14(2), 0);
    let s38 = BehaviorResourceState::new(38, null).unwrap();
    assert_eq!(s38.model_value_14(1), 0);
    let s32 = BehaviorResourceState::new(32, null).unwrap();
    assert_eq!(s32.model_value_14(1), 0);
}
