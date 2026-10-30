use super::*;

#[test]
fn combat_timed_damage_ratio_matches_native_signed_boundaries() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/combat_actor_timed_damage_native.json"
    ))
    .unwrap();
    assert_eq!(fixture["cases"].as_array().unwrap().len(), 20);
    for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let result = timed_modifier::<std::convert::Infallible>(
            input["delta"].as_i64().unwrap() as i32,
            input["period"].as_i64().unwrap() as i32,
        );
        if case["idiv_trap"] == true {
            assert!(
                matches!(result, Err(ActorTimedDamageError::Division { .. })),
                "case{index}"
            );
        } else {
            let expected = case["events"]
                .as_array()
                .unwrap()
                .first()
                .map(|event| event["modifier"].as_u64().unwrap() as i32);
            assert_eq!(result.unwrap(), expected, "case{index}");
        }
    }
}

struct Host {
    callbacks: Vec<ActorDamageCallback>,
    fail: bool,
}
impl ActorTimedDamageHost for Host {
    type Error = &'static str;
    fn virtual_b4(
        &mut self,
        arena: &mut ObjectArena,
        actor: ObjectId,
        callback: ActorDamageCallback,
    ) -> Result<(), Self::Error> {
        self.callbacks.push(callback);
        arena.actor_mut(actor).unwrap().word_ca = Some(55);
        if self.fail {
            Err("required B4")
        } else {
            Ok(())
        }
    }
}

fn fixture() -> (ObjectArena, ObjectId) {
    use crate::object_runtime::{
        ActorConstructionRequest, ActorRegistryConfig, ActorTerrainInputs,
    };
    let mut arena = ObjectArena::with_actor_registry(ActorRegistryConfig {
        owner_count: 2,
        initial_capacity: 8,
        null_handle: ActorHandle::from_raw(u32::MAX),
    })
    .unwrap();
    let actor = arena
        .allocate_actor(ActorConstructionRequest {
            constructor_null:
                crate::object_runtime::ActorConstructorNull::RegistrySnapshotApproximation,
            allocation_position: Some([0; 3]),
            owner: 1,
            terrain: ActorTerrainInputs {
                terrain: None,
                coordinate_shift: 8,
                clock: 0,
                world_flags: None,
                sine_quarter: &[0; 4096],
            },
        })
        .unwrap();
    (arena, actor)
}

#[test]
fn timed_damage_skips_zero_resistance_and_retains_failed_callback_writes() {
    let (mut arena, actor) = fixture();
    let mut host = Host {
        callbacks: Vec::new(),
        fail: true,
    };
    let mut request = ActorTimedDamageRequest {
        packet: ActorDamagePacket {
            strength: 4000,
            channel: 0,
            byte_03: None,
        },
        source: ActorHandle::from_raw(0x12345678),
        microseconds: 1,
        period_microseconds: 0,
    };
    arena
        .apply_actor_timed_damage(actor, request, &mut host)
        .unwrap();
    assert!(host.callbacks.is_empty());
    request.packet.channel = 5;
    assert!(matches!(
        arena.apply_actor_timed_damage(actor, request, &mut host),
        Err(ActorTimedDamageError::Host("required B4"))
    ));
    assert_eq!(host.callbacks.len(), 1);
    assert_eq!(host.callbacks[0].adjusted_damage, 2000);
    assert_eq!(host.callbacks[0].packet.byte_03, None);
    assert_eq!(host.callbacks[0].source, request.source);
    assert_eq!(arena.actor(actor).unwrap().word_ca, Some(55));
}

fn set_response(
    arena: &mut ObjectArena,
    actor: ObjectId,
    channel: u8,
    threshold: u16,
    multiplier: u16,
) {
    use inf_formats::{
        parse_constructor_asset_bank, ByteSpan, ConstructorPayloadView, HandlerFraming, HandlerId,
        HandlerRegion, RelocationTail,
    };
    let mut bytes = vec![0; 4 + 0x50];
    bytes[..2].copy_from_slice(&2_u16.to_le_bytes()); //selector3 shared actor
    let offset = 4 + 0x10 + usize::from(channel) * 4;
    bytes[offset..offset + 2].copy_from_slice(&threshold.to_le_bytes());
    bytes[offset + 2..offset + 4].copy_from_slice(&multiplier.to_le_bytes());
    let main = ByteSpan::new(0, bytes.len());
    let empty = ByteSpan::new(bytes.len(), bytes.len());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let bank = parse_constructor_asset_bank(
        &bytes,
        &HandlerRegion {
            handler: HandlerId::new(13).unwrap(),
            requested_count: 1,
            span: ByteSpan::new(0, bytes.len()),
            framing: HandlerFraming::RelocationTail(RelocationTail {
                main_payload: main,
                relocation_count: 0,
                relocation_offsets: empty,
                entry_offsets: ByteSpan::new(main.end, bytes.len()),
            }),
        },
    )
    .unwrap();
    let ConstructorPayloadView::SharedActor(payload) = &bank.assets[0].payload else {
        unreachable!()
    };
    arena.actor_mut(actor).unwrap().damage_table =
        crate::object_runtime::ActorDamageTable::Payload(std::sync::Arc::new(payload.clone()));
}

#[test]
fn combat_timed_damage_integrated_native_resistance_and_callback_receipts_match() {
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/combat_actor_timed_damage_integrated_native.json"
    ))
    .unwrap();
    assert_eq!(receipt["cases"].as_array().unwrap().len(), 10);
    for (index, case) in receipt["cases"].as_array().unwrap().iter().enumerate() {
        let input = &case["input"];
        let (mut arena, actor) = fixture();
        let channel = input["channel"].as_u64().unwrap() as u8;
        set_response(
            &mut arena,
            actor,
            channel,
            input["threshold"].as_u64().unwrap() as u16,
            input["multiplier"].as_u64().unwrap() as u16,
        );
        let mut host = Host {
            callbacks: Vec::new(),
            fail: input["stop"] == true,
        };
        let request = ActorTimedDamageRequest {
            packet: ActorDamagePacket {
                strength: input["strength"].as_i64().unwrap() as i16,
                channel,
                byte_03: None,
            },
            source: ActorHandle::from_raw(input["source"].as_u64().unwrap() as u32),
            microseconds: input["delta"].as_i64().unwrap() as i32,
            period_microseconds: input["period"].as_i64().unwrap() as i32,
        };
        let result = arena.apply_actor_timed_damage(actor, request, &mut host);
        assert_eq!(
            result.is_ok(),
            case["normal_return"] == true,
            "case{index}: {result:?}"
        );
        let expected = &case["virtual_b4"];
        if expected.is_null() {
            assert!(host.callbacks.is_empty(), "case{index}");
        } else {
            assert_eq!(
                host.callbacks,
                [ActorDamageCallback {
                    adjusted_damage: expected["scaled"].as_u64().unwrap() as i32,
                    packet: request.packet,
                    source: request.source,
                }],
                "case{index}"
            );
        }
    }
}
