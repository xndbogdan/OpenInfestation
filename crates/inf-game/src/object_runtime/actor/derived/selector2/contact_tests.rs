//! The vehicle's furniture response: its virtual `+A8` is the shared
//! `0047BFD0`, which Infest02 reaches five times.

use serde_json::Value;

use crate::object_runtime::{
    ActorDamagePacket, ActorDamageRequest, ActorHandle, CollisionContact, FurnitureContactResponse,
    FurnitureResponseHost, NetworkImpactGate, ObjectArena, ObjectId,
};
use crate::terrain_runtime::TerrainSpacing;

use super::test_support::loaded;

#[derive(Debug, PartialEq)]
enum Call {
    Damage(ActorDamageRequest),
    Furniture([u16; 2], ActorDamagePacket),
}

#[derive(Default)]
struct Host {
    calls: Vec<Call>,
}

impl FurnitureResponseHost for Host {
    type Error = &'static str;
    fn network_impact_gate(&self) -> NetworkImpactGate {
        NetworkImpactGate::Offline
    }
    fn process_null_handle(&self) -> Option<ActorHandle> {
        Some(ActorHandle::from_raw(0))
    }
    fn actor_damage(
        &mut self,
        _: &mut ObjectArena,
        _: ObjectId,
        request: ActorDamageRequest,
    ) -> Result<(), Self::Error> {
        self.calls.push(Call::Damage(request));
        Ok(())
    }
    fn furniture_damage(
        &mut self,
        xz: [u16; 2],
        packet: ActorDamagePacket,
    ) -> Result<(), Self::Error> {
        self.calls.push(Call::Furniture(xz, packet));
        Ok(())
    }
}

fn words<const N: usize>(value: &Value) -> [u16; N] {
    std::array::from_fn(|index| value[index].as_u64().unwrap() as u16)
}

/// Each response pushes the vehicle out along the contact's normal, rebounds
/// its velocity, and past 150 damages it and the furniture's cell by its
/// velocity change times its mass (`+8C`, 200).
#[test]
fn the_vehicle_responds_to_furniture_as_retail_does() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../../tests/fixtures/vehicle_furniture_response_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    let (_resident, _assets, mut world) = loaded(18, TerrainSpacing::Units128);
    let arena = &mut world.arena;
    let vehicle = world.respawn.as_ref().unwrap().vehicle.unwrap();
    let mut damaged = 0;
    for (index, case) in cases.iter().enumerate() {
        {
            let base = &mut arena.owned_mut(vehicle).unwrap().state.base;
            base.position_words = Some(words(&case["position"]));
            base.flags = case["flags"].as_u64().unwrap() as u32;
        }
        {
            let actor = arena.actor_mut(vehicle).unwrap();
            actor.word_8c = case["mass"].as_u64().unwrap() as u16;
            actor.words_8e_92 = words(&case["velocity"]);
        }
        let contact: Vec<u32> = case["contact"]
            .as_array()
            .unwrap()
            .iter()
            .map(|word| word.as_u64().unwrap() as u32)
            .collect();
        let cell: [u16; 3] = words(&case["cell"]);
        let mut host = Host::default();
        let al = arena
            .respond_furniture_contact(
                vehicle,
                FurnitureContactResponse {
                    contact: CollisionContact::from_native_words(std::array::from_fn(|i| {
                        Some(contact[i])
                    })),
                    kind: case["kind"].as_u64().unwrap() as u8,
                    cell_xz: [cell[0], cell[2]],
                },
                &mut host,
            )
            .unwrap();
        assert_eq!(al, 0);
        let after = &case["after"];
        assert_eq!(
            arena.object(vehicle).unwrap().base.position_words,
            Some(words(&after["position"])),
            "case {index} position"
        );
        assert_eq!(
            arena.actor(vehicle).unwrap().words_8e_92,
            words::<3>(&after["velocity"]),
            "case {index} velocity"
        );
        let expected = match case["damage"].as_object() {
            None => Vec::new(),
            Some(damage) => {
                damaged += 1;
                let packet = ActorDamagePacket {
                    strength: damage["strength"].as_u64().unwrap() as i16,
                    channel: damage["channel"].as_u64().unwrap() as u8,
                    byte_03: None,
                };
                vec![
                    Call::Damage(ActorDamageRequest {
                        packet,
                        source: arena.actor(vehicle).unwrap().handle,
                    }),
                    Call::Furniture(words(&case["furniture"]), packet),
                ]
            }
        };
        assert_eq!(host.calls, expected, "case {index} calls");
    }
    assert_eq!(damaged, 2);
}
