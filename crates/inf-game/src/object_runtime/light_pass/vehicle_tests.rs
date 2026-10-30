use std::collections::HashMap;

use serde::Deserialize;

use super::*;
use crate::terrain_runtime::TerrainSpacing;

#[derive(Deserialize)]
struct Fixture {
    receipts: Vec<Receipt>,
}

#[derive(Deserialize)]
struct Receipt {
    kind: String,
    at: String,
    world: u32,
    position: [u16; 3],
    basis: [u32; 10],
    gear: u32,
    deposit: Option<Deposit>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct Deposit {
    x: u16,
    z: u16,
    radius: i32,
    rgb: [u8; 3],
}

/// Infest03's vehicle lights (`00420080`), each arm run from its recorded
/// hull: 40 headlamp rays over Tabuchi's coarse terrain (world 9), the
/// headlights on, and 40 rear glows on Artoria and Altair Prime (red in
/// gear 1, none in gear 2), with 20 amber ones in gear 0 from Infest01's
/// Level 1. Every
/// deposit `0045B1A0` the port plans matches retail's X, Z, radius and
/// colour, and so does every ray that found no ground and every glow in the
/// boost's gear.
#[test]
fn the_vehicle_lights_match_every_recorded_receipt() {
    let fixture: Fixture = serde_json::from_str(include_str!(
        "../../../tests/fixtures/vehicle_lights_infest03.json"
    ))
    .unwrap();
    let mut worlds = HashMap::new();
    let mut failures = Vec::new();
    let (mut headlamps, mut rears) = (0, 0);
    for receipt in &fixture.receipts {
        let planned = match receipt.kind.as_str() {
            "headlamp" => {
                headlamps += 1;
                let (_, _, world) = worlds.entry(receipt.world).or_insert_with(|| {
                    crate::test_worlds::offline_world_retained(
                        receipt.world,
                        TerrainSpacing::Units256,
                    )
                });
                headlamp_light(receipt.position, &receipt.basis, |xz| {
                    world.terrain.height_at(xz)
                })
            }
            "rear" => {
                rears += 1;
                rear_glow(receipt.position, &receipt.basis, receipt.gear)
            }
            other => panic!("unknown arm {other}"),
        };
        let planned = planned.map(|light| Deposit {
            x: light.world_xz[0] as u16,
            z: light.world_xz[1] as u16,
            radius: light.radius,
            rgb: light.rgb.map(|channel| channel as u8),
        });
        if planned != receipt.deposit {
            failures.push(format!(
                "{} {}: port {planned:?} native {:?}",
                receipt.kind, receipt.at, receipt.deposit
            ));
        }
    }
    assert_eq!((headlamps, rears), (40, 60));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// `004201C4..004201CE`: a hit level with the first step moves its Z on by
/// one before the divide, and a ray that neither climbs nor falls lights
/// nothing.
#[test]
fn a_level_ray_lights_nothing_and_a_hit_at_the_first_z_moves_on() {
    let flat = |words: [f32; 9]| {
        let mut basis = [0u32; 10];
        for (slot, value) in basis[1..].iter_mut().zip(words) {
            *slot = value.to_bits();
        }
        basis
    };
    // Nose straight ahead along X, up row straight up: the step is
    // (512, -128, 0), so the ray falls, and every sample shares Z.
    let basis = flat([0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0]);
    let light = headlamp_light([1000, 1000, 2000], &basis, |_| 2000).unwrap();
    // Step 2 hits: (2024, 744, 2000), Z on to 2001, the fraction
    // (2000 - 2001) / 1 = -1 moves X back by (512 * -1) >> 15 = -1, Y by
    // (-128 * -1) >> 15 = 0 and Z by (1 * -1) >> 15 = -1: (2025, 744, 2002).
    assert_eq!(light.world_xz, [2025, 2002]);
    // Its distance from the first step (1512, 872, 2000): (-513, 128, -2),
    // 528 long: radius 396 + 512, grey 96 * 500 / 528.
    assert_eq!(light.radius, 528 * 3 / 4 + 0x200);
    assert_eq!(light.rgb, [90, 90, 90]);
    // Nose and up row both level: no rise or fall, no light.
    let level = flat([0.0, 0.0, -1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    assert!(headlamp_light([1000, 1000, 2000], &level, |_| 2000).is_none());
}
