use super::*;
use crate::{object_runtime::TerrainSphereGrid, terrain_runtime::TerrainSpacing};
use inf_formats::{TerrainCell, TERRAIN_GRID_SIDE};

/// Intro Selector35 model 474: 07/8E bounding sphere, branch 95, then eight
/// 07/8E hull spheres. Flat cells keep every plane normal exactly +Y.
#[test]
fn intro_ship_474_terrain_query_tracks_its_deepest_flat_contact() {
    let runtime = resident();
    let model = source(&runtime, 474);
    let qualification = qualification(&model);
    // Signed height byte 1, shifted by 5: ground at 32 everywhere.
    let cells = vec![TerrainCell::from_raw(1); TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE];
    let query = |y: i32| {
        let (owners, mut process) = owners(true);
        let mut contact = CollisionContact::from_native_words([Some(9); 5]);
        query_auxiliary_terrain(
            AuxiliaryTerrainRequest {
                model: AuxiliaryModelSource {
                    model: model.model,
                    qualification: &qualification,
                },
                basis: MenuBasis::INITIAL,
                origin: [0x4000, y, 0x4000],
                terrain: AuxiliaryTerrain {
                    cells: &cells,
                    grid: TerrainSphereGrid {
                        spacing: TerrainSpacing::Units256,
                        mask: TerrainSpacing::Units256,
                        height_shift: 5,
                    },
                },
                values: AuxiliaryValueSource::Required,
                residency: Some(&runtime),
            },
            &mut process,
            &owners,
            &mut contact,
        )
        .unwrap();
        contact
    };
    let mut previous = None;
    let mut contacts = 0;
    for y in (-200..=1500).rev().step_by(10) {
        let contact = query(y);
        match contact.flag() {
            // High above, the bounding sphere misses and 95 ends the program.
            Some(0) => assert_eq!(previous, None, "contact lost at Y {y}"),
            Some(1) => {
                assert_eq!(
                    contact.normal_bits(),
                    [Some(0), Some(1f32.to_bits()), Some(0)],
                    "Y {y}"
                );
                let displacement = contact.displacement().unwrap();
                // A uniform drop keeps the same deepest hull sphere.
                if let Some(previous) = previous {
                    assert_eq!(displacement, previous + 10, "Y {y}");
                }
                previous = Some(displacement);
                contacts += 1;
            }
            flag => panic!("unexpected flag {flag:?} at Y {y}"),
        }
    }
    assert!(contacts > 20, "{contacts} contacts");
}
