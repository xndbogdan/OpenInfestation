//! Supplied typed constructors/live retained bytes; no production+D8 or movement claim.
use super::*;
use crate::object_runtime::ObjectClassKind;

fn scored(kind: FamilyDescriptorScoreKind, bias: i32) -> FamilyDescriptorScoreRequest {
    FamilyDescriptorScoreRequest {
        kind,
        current: false,
        bias,
    }
}
fn retained_fixture(
    profile: FamilyBehaviorProfile,
) -> (ObjectArena, ObjectId, Arc<RuntimeAllocation>) {
    let (mut arena, actor) = fixture(profile);
    let owner = allocation(vec![0; 32]);
    retain(&mut arena, actor, profile, &owner, 4);
    (arena, actor, owner)
}
fn coefficient(owner: &RuntimeAllocation, kind: FamilyDescriptorScoreKind, value: u16) {
    owner
        .write_word(4 + kind.coefficient_offset().unwrap(), value)
        .unwrap();
}

#[test]
fn constants_bypass_receiver_retained_and_bias() {
    let mut arena = arena();
    let base = arena.allocate(ObjectClassKind::Base);
    let foreign = self::arena().allocate(ObjectClassKind::Base);
    for actor in [base, foreign] {
        for bias in [i32::MIN, -1, 0, i32::MAX] {
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(FamilyDescriptorScoreKind::ConstantOne, bias),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(1)
            );
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(FamilyDescriptorScoreKind::ConstantZero, bias),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(0)
            );
        }
    }
    assert_eq!(
        FamilyDescriptorScoreKind::ConstantOne.coefficient_offset(),
        None
    );
    assert_eq!(
        FamilyDescriptorScoreKind::ConstantZero.coefficient_offset(),
        None
    );
}

#[test]
fn actual_descriptor_tables_select_all_exact_score_wrappers() {
    let mut callbacks = std::collections::BTreeSet::new();
    for profile in [
        FamilyBehaviorProfile::Family4236d0 { mode: 0 },
        FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        FamilyBehaviorProfile::Family4236d0 { mode: 2 },
        FamilyBehaviorProfile::Selector35,
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: true,
        },
    ] {
        for index in 0..profile.descriptor_table().unwrap().1 {
            let descriptor = profile.descriptor(index).unwrap();
            let request =
                FamilyDescriptorScoreRequest::from_descriptor(descriptor, false, -7).unwrap();
            assert_eq!(request.kind.address(), descriptor.score_address);
            assert_eq!(request.bias, -7);
            callbacks.insert(request.kind.address());
        }
    }
    assert_eq!(callbacks.len(), 19);
    let mut unknown = FamilyBehaviorProfile::Selector35.descriptor(0).unwrap();
    unknown.score_address = 0x1234_5678;
    assert_eq!(
        FamilyDescriptorScoreRequest::from_descriptor(unknown, false, 0),
        None
    );
}

#[test]
fn required_sources_read_coefficient_before_reached_typed_boundary() {
    use FamilyDescriptorScoreKind as K;
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor, owner) = retained_fixture(profile);
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    arena.family428650_actor_mut(actor).unwrap().position_222 = None;
    for (kind, virtual_slot, direct) in [
        (K::VirtualFc, Some(0xfc), None),
        (K::Virtual134Selector12, Some(0x134), None),
        (K::Virtual140Selector12, Some(0x140), None),
        (K::Source43e360, None, Some(0x0043_e360)),
        (K::Source428990, None, Some(0x0042_8990)),
        (K::Source43d730, None, Some(0x0043_d730)),
        (K::Source43d8f0, None, Some(0x0043_d8f0)),
    ] {
        coefficient(&owner, kind, 0);
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                scored(kind, i32::MIN),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0)
        );
        coefficient(&owner, kind, 1);
        let before = format!("{arena:?}");
        let mut required = scored(kind, -1);
        //Common FC reads constructor-known byte260=1 after shared NULL148.
        //The fresh native target query fails before any mutation or RNG.
        required.current = kind == K::VirtualFc;
        // Effective multiplier0 cannot bypass the nonzero coefficient's call.
        let expected = if kind == K::VirtualFc {
            FamilyDescriptorScoreError::MissingCurrentTarget {
                actor,
                address: 0x0042_4f86,
            }
        } else if let Some(offset) = virtual_slot {
            FamilyDescriptorScoreError::RequiredActorVirtual {
                actor,
                callback: kind.address(),
                offset,
            }
        } else if kind == K::Source428990 {
            // 00428990's first read is process NULL, which this caller omits.
            FamilyDescriptorScoreError::MissingProcessNull {
                callback: kind.address(),
                address: 0x0042_8990,
            }
        } else {
            FamilyDescriptorScoreError::RequiredNativeSource {
                actor,
                callback: kind.address(),
                address: direct.unwrap(),
            }
        };
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                required,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Err(expected)
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn zero_still_requires_its_actual_retained_word() {
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor) = fixture(profile);
    let kind = FamilyDescriptorScoreKind::Source43d8f0;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Derived(
            DerivedActorError::MissingFamily4236d0Payload
        ))
    );
    let owner = allocation(vec![0; 0x12]);
    retain(&mut arena, actor, profile, &owner, 0);
    assert!(matches!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Derived(
            DerivedActorError::Allocation(RuntimeAllocationError::Range { offset: 0x12, .. })
        ))
    ));
}

#[test]
fn f0_unsigned_max_uses_distinct_coefficients_and_live_shared_bytes() {
    use FamilyDescriptorScoreKind as K;
    for profile in [
        FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        FamilyBehaviorProfile::Selector35,
    ] {
        let (mut arena, actor, owner) = retained_fixture(profile);
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.words_1d8_200[0] = 0x8000_0000;
        family.words_1d8_200[4] = 0x7fff_ffff;
        let released =
            arena.import_fixture_target(crate::object_runtime::NativeTarget::FixedPosition(
                crate::object_runtime::FixedPositionTarget {
                    handle_04: crate::object_runtime::ActorHandle::from_raw(0),
                    word_10: 8,
                    focus_08: None,
                },
            ));
        arena.release_actor_service(released).unwrap();
        arena
            .family428650_actor_mut(actor)
            .unwrap()
            .targets_1d4_1fc
            .fill(Some(released));
        for kind in [K::Max1d8And1f8Common, K::Max1d8And1f8Selector12] {
            coefficient(&owner, kind, 0);
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(kind, 17),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(0)
            );
            coefficient(&owner, kind, 0x8000);
            let before = format!("{arena:?}");
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(kind, -32767),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(i32::MIN)
            );
            assert_eq!(format!("{arena:?}"), before);
            coefficient(&owner, kind, 0xffff);
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(kind, 1),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(0)
            );
        }
    }
    let mut arena = arena();
    let actor = arena.allocate_selector34_actor(request(), &mut 0).unwrap();
    let owner = allocation(vec![0; 4]);
    arena.set_selector34_retained(actor, &owner, 0).unwrap();
    owner.write_word(0, 1).unwrap();
    arena.family428650_actor_mut(actor).unwrap().words_1d8_200[4] = 9;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(K::Max1d8And1f8Common, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(9)
    );
}

#[test]
fn selector12_virtual128_b4_gate_and_byte_shift_are_exact() {
    let profile = FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    };
    let (mut arena, actor, owner) = retained_fixture(profile);
    let kind = FamilyDescriptorScoreKind::Selector12Score270;
    coefficient(&owner, kind, 1);
    arena.selector12_actor_mut(actor).unwrap().word_270 = 0x4000_0001;
    arena.selector12_actor_mut(actor).unwrap().byte_29d = 0;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(0x4000_0001)
    );
    arena.selector12_actor_mut(actor).unwrap().byte_29d = 0x80;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(4)
    );
    arena.actor_mut(actor).unwrap().word_b4 = 18;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, i32::MAX),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(0)
    );
    arena.actor_mut(actor).unwrap().word_b4 = 0x0001_0012;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(4)
    );
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(4)
    );
    assert_eq!(format!("{arena:?}"), before);
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor, owner) = retained_fixture(profile);
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(0)
    );
    coefficient(&owner, kind, 1);
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::RequiredActorVirtual {
            actor,
            callback: 0x0046_9130,
            offset: 0x128
        })
    );
}

#[test]
fn home_wrappers_preserve_word_wrap_bit31_repair_and_signed_radius_clamp() {
    use FamilyDescriptorScoreKind as K;
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor, owner) = retained_fixture(profile);
    for kind in [K::HomeDistanceCommon, K::HomeDistanceSelector12] {
        coefficient(&owner, kind, 1);
        for (position, home, radius, expected) in [
            ([0, 0, 0], [0xffff, 0, 0], 10, 1),
            ([3, 4, 0], [0, 0, 0], 6, 5),
            ([3, 4, 0], [0, 0, 0], 5, 2),
            ([0, 0, 0], [0, 0, 0], -5, -2),
            ([0x8000; 3], [0; 3], 60_000, 56_754),
            ([0x8000; 3], [0; 3], 50_000, 25_000),
        ] {
            arena.owned_mut(actor).unwrap().state.base.position_words = Some(position);
            arena.family428650_actor_mut(actor).unwrap().position_222 = Some(home);
            arena.derived_actor_mut(actor).unwrap().word_164 = radius;
            let before = format!("{arena:?}");
            assert_eq!(
                arena.score_family_descriptor(
                    actor,
                    scored(kind, 0),
                    FamilyDescriptorScoreRng::RequiredAtReach
                ),
                Ok(expected)
            );
            assert_eq!(format!("{arena:?}"), before);
        }
    }
}

#[test]
fn home_missing_fields_stop_only_after_nonzero_coefficient() {
    let profile = FamilyBehaviorProfile::Family4236d0 { mode: 1 };
    let (mut arena, actor, owner) = retained_fixture(profile);
    let kind = FamilyDescriptorScoreKind::HomeDistanceCommon;
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(0)
    );
    coefficient(&owner, kind, 1);
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Object(
            ObjectError::ActorPositionUnknown(actor)
        ))
    );
    arena.owned_mut(actor).unwrap().state.base.position_words = Some([0; 3]);
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 0),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Uninitialized {
            actor,
            offset: 0x224
        })
    );
}

#[test]
fn complete_native_score_wrappers_sources_and_final_virtual_bindings_are_guarded() {
    use sha2::{Digest, Sha256};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
    let assets = crate::frontend_assets::FrontendAssets::open(&root).unwrap();
    let exe = assets.executable_bytes();
    assert_eq!(
        format!("{:x}", Sha256::digest(exe)),
        "bf09f66e3685d4990d95da13d4be1ca7e293fb31cddccc90bd575882921e4956"
    );
    inf_formats::FrontendExecutable::parse(exe).unwrap();
    for (start, end, expected) in NATIVE_BODIES {
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(&exe[*start - 0x0040_0000..*end - 0x0040_0000])
            ),
            *expected
        );
    }
    for table in [
        0x0052_0cc8,
        0x0052_7ee0,
        0x0052_7058,
        0x0052_273c,
        0x0052_1eac,
    ] {
        let offset = table - 0x0040_0000 + 0xf0;
        assert_eq!(
            u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
            0x0040_5730
        );
    }
    let offset = 0x0052_7058 - 0x0040_0000 + 0x140;
    assert_eq!(
        u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
        0x0046_7840
    );
    let offset = 0x0052_7058 - 0x0040_0000 + 0x134;
    assert_eq!(
        u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
        0x0046_1c20
    );
    //Constructor467070 stores zero at+278; shared publisher427220 stores
    //the new unsigned score at pair+4 before any target replacement/allocation.
    assert_eq!(
        &exe[0x670c9..0x670cf],
        &[0x89, 0x9e, 0x78, 0x02, 0x00, 0x00]
    );
    assert_eq!(&exe[0x0002_723e..0x0002_7241], &[0x89, 0x47, 0x04]);
    for (table, method) in [
        (0x0052_0b84, 0x0042_4f50_u32),
        (0x0052_0cc8, 0x0042_4f50),
        (0x0052_7ee0, 0x0042_4f50),
        (0x0052_7058, 0x0042_7320),
        (0x0052_273c, 0x0042_7320),
    ] {
        let offset = table - 0x0040_0000 + 0xfc;
        assert_eq!(
            u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
            method
        );
    }
    let offset = 0x0052_7058 - 0x0040_0000 + 0x128;
    assert_eq!(
        u32::from_le_bytes(exe[offset..offset + 4].try_into().unwrap()),
        0x0046_88f0
    );
}

const NATIVE_BODIES: &[(usize, usize, &str)] = &[
    (
        0x467840,
        0x467892,
        "c92f997ad95ddf8bfed351c005e22a850fea23c29f62ddb83f00c47352b0adcb",
    ),
    (
        0x424f50,
        0x4250be,
        "58b25928c067de5e93e4f10375589ac1f37debefda13affa05fdbdc0e702c4d4",
    ),
    (
        0x427320,
        0x427383,
        "0fff10fa843a8d43098f478007bf825cf08429055d8b4f17c61b5222646b7246",
    ),
    (
        0x4294a0,
        0x429895,
        "189764878265d1529a6045efcfb635239d380c1edb3a695f4aa9d641090b7233",
    ),
    (
        0x461c20,
        0x461c29,
        "95924f4ddfaa6de4264d2302a389d28c7eff9a2c3902fcc684ae46d46dda6cb7",
    ),
    (
        0x467070,
        0x4671bd,
        "a1c8a13da94b34067915fe51de92f71967b79db495b6a93775f1d526cbb7b370",
    ),
    (
        0x427220,
        0x427319,
        "8bcad8a38edd3a1e95a7a48b4687d43aad1674984533b74c65dfcaff01b52776",
    ),
    (
        0x425df0,
        0x425df8,
        "a6bd7b63e4c12708d7f7a7e7d4ba72c1eb5381f5e270fdd13f4a86ae356bac84",
    ),
    (
        0x469370,
        0x469375,
        "531db4852a682340f10d91199822e1a1aed030bae016ff1e2c9f32d13b9cc9a1",
    ),
    (
        0x405730,
        0x405745,
        "aff84648c7a689c67e2cdd21c49a5d75f33832ecab23e8d091c86828e86b6e0c",
    ),
    (
        0x4688f0,
        0x468914,
        "9145b868c96f1637de156ef175d6f4766fa743135a0280dd3a31311fa63a83b6",
    ),
    (
        0x428320,
        0x42838e,
        "5202f748d2bcccc67db9a3e595629dd7c20859e5669be920fba8bb6fff701f0b",
    ),
    (
        0x43fac0,
        0x43fafe,
        "29b7f96a1e098b9df0fac21cec12d219befda4e6e555904398232dd3c2a2927f",
    ),
    (
        0x469330,
        0x469370,
        "cf5bdaa0d1555852bd4834bc44921f24cb65048602b96fdbf633be7e6113d780",
    ),
    (
        0x469130,
        0x469170,
        "3d876ba61ed700e2f80fbe150daeea0abf07e32b747f82373f13ae1e6822da72",
    ),
    (
        0x425db0,
        0x425ded,
        "1b0946f3119f384daa7798ea50aee4fa6efd1b544c2978d62ad05bdec150a338",
    ),
    (
        0x469270,
        0x4692ad,
        "f4c8221a3e149b075565194e03c92fbfe575f65165b89ed2c653c5e1f6bc4276",
    ),
    (
        0x43fa40,
        0x43fa80,
        "571b68f95fb9d70a793ed6f018a3557c404327e6288e024ea6c297a746cb0d57",
    ),
    (
        0x425d30,
        0x425d6e,
        "7dadfeda303d444506f1aba3d45c366c51220efcff06a87d5aa470b8709690f1",
    ),
    (
        0x469190,
        0x4691d0,
        "cb2d60f99530945a0925a2a81fb53ea8157d66d8874761c37bd6c8aefcbbacc3",
    ),
    (
        0x4692b0,
        0x4692ee,
        "bf2b658724ea71a3119b917d7d160d5ab511e5f5a0aa977d8c37d48994279457",
    ),
    (
        0x43fc60,
        0x43fc9d,
        "b18ff2f80826cc20d9791c84b143c91d46ec896b965b9fa390e86b44f9722ac6",
    ),
    (
        0x43fce0,
        0x43fd1d,
        "21126b3d8683aab5ffb8840ee9c297e015308a1a08e07640322f7b0869b2132e",
    ),
    (
        0x43fd40,
        0x43fd7d,
        "615858db20c0f16f1b24b6232d331d805a7ddc8202dfbba620b705fd0a94aab3",
    ),
    (
        0x43fdc0,
        0x43fdfd,
        "a4b1a85b65f53dc4afa9b56ab90cd9444236b43995ebbbcc8e1f1f5a9047e296",
    ),
];

#[test]
fn selector12_virtual134_reads_live_published_score_without_target_or_weapon_reads() {
    use crate::object_runtime::{
        FamilyTargetSlot, WeightedTargetConstructorNull, WeightedTargetReferent,
        WeightedTargetRequest,
    };
    let profile = FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    };
    let (mut arena, actor, owner) = retained_fixture(profile);
    let kind = FamilyDescriptorScoreKind::Virtual134Selector12;
    assert_eq!(arena.selector12_actor(actor).unwrap().word_278, 0);
    coefficient(&owner, kind, 3);
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 5),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(0)
    );

    //The real shared publisher writes this exact+274/+278 pair. Native Object
    //publication is427220; no decoded target snapshot substitutes for+278.
    let referent = arena.allocate(ObjectClassKind::Base);
    arena.owned_mut(referent).unwrap().state.base.position_words = Some([9, 8, 7]);
    assert!(arena
        .publish_family_target(
            actor,
            FamilyTargetSlot::Target274,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Object(Some(referent)),
                score: 0x8000_0003,
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            },
        )
        .unwrap());
    assert_eq!(arena.selector12_actor(actor).unwrap().word_278, 0x8000_0003);
    let target = arena
        .family_weighted_target(actor, FamilyTargetSlot::Target274)
        .unwrap()
        .target
        .unwrap();
    arena.release_actor_service(target).unwrap();
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    //The retained raw target pointer is now Released; pure461C20 never reads it.
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            scored(kind, 5),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Ok(3i32.wrapping_add(5).wrapping_mul(0x8000_0003u32 as i32))
    );
    assert_eq!(format!("{arena:?}"), before);

    for (factor, bias, source) in [
        (1u16, 0, u32::MAX),
        (0xffff, i32::MAX, 0x8000_0000),
        (1, -1, 0xdead_beef),
        (0, i32::MIN, 0x8000_0001),
    ] {
        coefficient(&owner, kind, factor);
        arena.selector12_actor_mut(actor).unwrap().word_278 = source;
        let before = format!("{arena:?}");
        let expected = if factor == 0 {
            0
        } else {
            i32::from(factor)
                .wrapping_add(bias)
                .wrapping_mul(source as i32)
        };
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                scored(kind, bias),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(expected)
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn false_current_fc_reads_actual_score200_without_target_or_rng_work() {
    let kind = FamilyDescriptorScoreKind::VirtualFc;
    for profile in [
        FamilyBehaviorProfile::Family4236d0 { mode: 1 },
        FamilyBehaviorProfile::Selector12 {
            behavior_present: false,
        },
        FamilyBehaviorProfile::Selector35,
    ] {
        let (mut arena, actor, owner) = retained_fixture(profile);
        arena.owned_mut(actor).unwrap().state.base.position_words = None;
        assert_eq!(arena.derived_actor(actor).unwrap().word_14c, None);
        for source in [0, 1, 0x8000_0000, u32::MAX] {
            arena.family428650_actor_mut(actor).unwrap().words_1d8_200[5] = source;
            for factor in [0, 1, 1000, u16::MAX] {
                coefficient(&owner, kind, factor);
                for bias in [i32::MIN, -1, 0, i32::MAX] {
                    let before = format!("{arena:?}");
                    let expected = if factor == 0 {
                        0
                    } else {
                        i32::from(factor)
                            .wrapping_add(bias)
                            .wrapping_mul(source as i32)
                    };
                    assert_eq!(
                        arena.score_family_descriptor(
                            actor,
                            scored(kind, bias),
                            FamilyDescriptorScoreRng::RequiredAtReach
                        ),
                        Ok(expected)
                    );
                    assert_eq!(format!("{arena:?}"), before);
                }
            }
        }
        let mut current = scored(kind, -1);
        current.current = true;
        coefficient(&owner, kind, 0);
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0)
        );
        coefficient(&owner, kind, 1);
        let before = format!("{arena:?}");
        let expected = if matches!(profile, FamilyBehaviorProfile::Family4236d0 { .. }) {
            Err(FamilyDescriptorScoreError::MissingCurrentTarget {
                actor,
                address: 0x0042_4f86,
            })
        } else {
            //Final12/35+FC is427320 only: NULL148 returns1, then(1-1)*1=0.
            Ok(0)
        };
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current,
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            expected
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn selector12_virtual140_new_candidate_uses_unsigned_scores_without_unknown_carry_reads() {
    let profile = FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    };
    let (mut arena, actor, owner) = retained_fixture(profile);
    let kind = FamilyDescriptorScoreKind::Virtual140Selector12;
    assert_eq!(
        arena.family428650_actor(actor).unwrap().words_188_19c[2],
        None
    );
    arena.owned_mut(actor).unwrap().state.base.position_words = None;
    for (left, right, factor, bias) in [
        (0u32, 0u32, 1u16, 0i32),
        (0x8000_0001, 7, 3, -7),
        (1, u32::MAX, 0xffff, i32::MAX),
        (u32::MAX, 0x8000_0000, 1, -1),
    ] {
        let state = arena.selector12_actor_mut(actor).unwrap();
        state.word_280 = left;
        state.word_288 = right;
        coefficient(&owner, kind, factor);
        let before = format!("{arena:?}");
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                scored(kind, bias),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(i32::from(factor)
                .wrapping_add(bias)
                .wrapping_mul(left.max(right) as i32))
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[test]
fn selector12_virtual140_current_reads_unknown190_only_after_all_rawnull_gates() {
    use crate::object_runtime::{
        FamilyTargetSlot, WeightedTargetConstructorNull, WeightedTargetReferent,
        WeightedTargetRequest,
    };
    let profile = FamilyBehaviorProfile::Selector12 {
        behavior_present: false,
    };
    let (mut arena, actor, owner) = retained_fixture(profile);
    let kind = FamilyDescriptorScoreKind::Virtual140Selector12;
    let current = |bias| FamilyDescriptorScoreRequest {
        current: true,
        ..scored(kind, bias)
    };
    //Zero coefficient bypasses the actual getter. A zero final multiplier or
    //zero source does not bypass the native getter's reached unknown190 read.
    assert_eq!(
        arena.score_family_descriptor(actor, current(7), FamilyDescriptorScoreRng::RequiredAtReach),
        Ok(0)
    );
    coefficient(&owner, kind, 1);
    let before = format!("{arena:?}");
    assert_eq!(
        arena.score_family_descriptor(
            actor,
            current(-1),
            FamilyDescriptorScoreRng::RequiredAtReach
        ),
        Err(FamilyDescriptorScoreError::Uninitialized {
            actor,
            offset: 0x190
        })
    );
    assert_eq!(format!("{arena:?}"), before);

    let referent = arena.allocate(ObjectClassKind::Base);
    arena.owned_mut(referent).unwrap().state.base.position_words = Some([1, 2, 3]);
    arena
        .publish_family_target(
            actor,
            FamilyTargetSlot::Target1d4,
            WeightedTargetRequest {
                referent: WeightedTargetReferent::Object(Some(referent)),
                score: 1,
                constructor_null: WeightedTargetConstructorNull::ActorRegistry,
            },
        )
        .unwrap();
    let target = arena.family428650_actor(actor).unwrap().targets_1d4_1fc[0].unwrap();
    arena.release_actor_service(target).unwrap();
    arena.selector12_actor_mut(actor).unwrap().word_280 = 0x4000_0001;
    arena.selector12_actor_mut(actor).unwrap().word_288 = 7;
    //Explicit raw-pointer fixture placements keep the sameReleased ref; only
    //one field is present per case, and this getter never clones/frees it.
    for index in [0usize, 1, 4, 3] {
        let family = arena.family428650_actor_mut(actor).unwrap();
        family.targets_1d4_1fc = [None; 6];
        family.targets_1d4_1fc[index] = Some(target);
        assert_eq!(family.words_188_19c[2], None);
        let before = format!("{arena:?}");
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(0),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0x4000_0001)
        );
        assert_eq!(format!("{arena:?}"), before);
    }
    arena.family428650_actor_mut(actor).unwrap().targets_1d4_1fc = [None; 6];
    coefficient(&owner, kind, 0xffff);
    for carry in [0u32, 1, u32::MAX] {
        arena.family428650_actor_mut(actor).unwrap().words_188_19c[2] = Some(carry);
        let source = if carry == 0 { 0x4000_0001u32 } else { 4 };
        let before = format!("{arena:?}");
        assert_eq!(
            arena.score_family_descriptor(
                actor,
                current(i32::MAX),
                FamilyDescriptorScoreRng::RequiredAtReach
            ),
            Ok(0xffffi32.wrapping_add(i32::MAX).wrapping_mul(source as i32))
        );
        assert_eq!(format!("{arena:?}"), before);
    }
}

#[path = "fc_tests.rs"]
mod fc_tests;

#[path = "fc_native_guards.rs"]
mod fc_native_guards;

/// 0043FC60 over Selector35's 0043E360, against the original bodies: byte
/// +26C, the current flag (else +164 halves toward zero), the coefficient
/// WORD and the bias, with DWORD wrapping.
#[test]
fn selector35_radius_score_matches_the_native_cases() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/selector35_landing_native.json"
    )))
    .unwrap();
    let kind = FamilyDescriptorScoreKind::Source43e360;
    let mut checked = 0;
    for case in fixture["cases"].as_array().unwrap() {
        let i = &case["input"];
        if i["mode"] != "score" {
            continue;
        }
        let (mut arena, actor, owner) = retained_fixture(FamilyBehaviorProfile::Selector35);
        coefficient(&owner, kind, i["coefficient"].as_u64().unwrap() as u16);
        arena.derived_actor_mut(actor).unwrap().word_164 = i["radius"].as_u64().unwrap() as i32;
        arena.selector35_actor_mut(actor).unwrap().byte_26c = i["byte26c"].as_u64().unwrap() as u8;
        let request = FamilyDescriptorScoreRequest {
            kind,
            current: i["entering"].as_bool().unwrap(),
            bias: i["bias"].as_i64().unwrap() as i32,
        };
        let score = arena
            .score_family_descriptor(actor, request, FamilyDescriptorScoreRng::RequiredAtReach)
            .unwrap();
        assert_eq!(
            score as u32 as u64,
            case["output"]["eax"].as_u64().unwrap(),
            "{i}"
        );
        checked += 1;
    }
    assert_eq!(checked, 18);
}
