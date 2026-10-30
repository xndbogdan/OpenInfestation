use super::*;
use crate::terrain_runtime::{TerrainHeightScale, TerrainSampling, TerrainSpacing};
use inf_formats::{ByteSpan, TerrainBucket, TerrainCell, TERRAIN_CELL_COUNT};

mod replacement;

fn terrain(spacing: TerrainSpacing, cell: u16) -> TerrainRuntime {
    TerrainRuntime {
        sampling: TerrainSampling {
            spacing,
            height_scale: TerrainHeightScale::Units32,
        },
        cells: vec![TerrainCell::from_raw(cell); TERRAIN_CELL_COUNT].into_boxed_slice(),
        buckets: (0..TERRAIN_GRID_SIDE)
            .map(|z| TerrainBucket {
                y: z as u16,
                byte_size: 0,
                records_span: ByteSpan::new(0, 0),
                records: Vec::new(),
            })
            .collect(),
        plane_base: 0,
        slope_factors: [0; 2],
        light_direction_raw: [0; 3],
        ultra: None,
        ultra_footings: Vec::new(),
        puddle_carve: None,
    }
}

fn record(terrain: &mut TerrainRuntime, xz: [u16; 2], lookup: u8) -> TerrainSparseRecordId {
    let [x, z] = terrain.effect_grid_coordinates(xz);
    let cell = usize::from(x) * TERRAIN_GRID_SIDE + usize::from(z);
    terrain.cells[cell] = TerrainCell::from_raw(terrain.cells[cell].raw() | 0x400);
    let records = &mut terrain.buckets[usize::from(z)].records;
    let index = records
        .binary_search_by_key(&x, |record| record.x)
        .unwrap_err();
    records.insert(
        index,
        TerrainRecord {
            x,
            state: 0xa5,
            resource_lookup: TerrainResourceLookup::from_raw(lookup),
        },
    );
    TerrainSparseRecordId { bucket: z, index }
}

struct Host {
    block: RadiusEffectBlock,
    position: [u16; 3],
    model_radius: Option<u16>,
    null_lookup: Option<u8>,
    lookups: Vec<u8>,
    damages: Vec<FurnitureDamageRequest>,
    fail_damage: bool,
    mutate_after_damage: bool,
}

impl Default for Host {
    fn default() -> Self {
        Self {
            block: RadiusEffectBlock {
                strength: 1000,
                channel: 9,
                byte_03: None,
                inner_radius: 0,
                outer_radius: 128,
                impulse: 256,
            },
            position: [0; 3],
            model_radius: Some(32),
            null_lookup: None,
            lookups: Vec::new(),
            damages: Vec::new(),
            fail_damage: false,
            mutate_after_damage: false,
        }
    }
}

impl TerrainFurnitureEffectHost for Host {
    type Error = &'static str;
    type Source = u32;
    fn source_block(&mut self, source: u32) -> Result<RadiusEffectBlock, Self::Error> {
        assert_eq!(source, 7);
        Ok(self.block)
    }
    fn source_position_words(&mut self, source: u32) -> Result<[u16; 3], Self::Error> {
        assert_eq!(source, 7);
        Ok(self.position)
    }
    fn furniture_model_radius(
        &mut self,
        lookup: TerrainResourceLookup,
    ) -> Result<Option<u16>, Self::Error> {
        self.lookups.push(lookup.raw());
        Ok(if self.null_lookup == Some(lookup.raw()) {
            None
        } else {
            self.model_radius
        })
    }
    fn apply_furniture_damage(
        &mut self,
        _: &mut TerrainRuntime,
        request: FurnitureDamageRequest,
        _: &mut u32,
    ) -> Result<bool, Self::Error> {
        self.damages.push(request);
        if self.fail_damage {
            return Err("required 00436020");
        }
        if self.mutate_after_damage {
            self.block.outer_radius = 65535;
            self.position[0] = 1024;
        }
        Ok(true)
    }
}

fn nine_records(terrain: &mut TerrainRuntime) {
    let mut lookup = 1;
    for x in [-128_i16, 0, 128] {
        for z in [-128_i16, 0, 128] {
            record(terrain, [x as u16, z as u16], lookup);
            lookup += 1;
        }
    }
}

#[test]
fn furniture_walk_is_x_outer_and_uses_strict_falloff_with_required_damage() {
    let mut terrain = terrain(TerrainSpacing::Units128, 0);
    nine_records(&mut terrain);
    let mut host = Host::default();
    let mut rng = 0x1234;
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut rng),
        Ok(1)
    );
    assert_eq!(host.lookups, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
    assert_eq!(host.damages.len(), 1);
    assert_eq!(host.damages[0].xz, [0, 0]);
    assert_eq!(host.damages[0].effect.strength, 1000);
    assert_eq!(host.damages[0].effect.byte_03, None);
    assert_eq!(rng, 0x1234); // Traversal/falloff alone consume no RNG.
}

#[test]
fn callbacks_reread_source_position_and_block_but_keep_initial_walk_radius() {
    let mut terrain = terrain(TerrainSpacing::Units128, 0);
    nine_records(&mut terrain);
    for (xz, lookup) in [
        ([1024, 128], 10),
        ([1152, 65408], 11),
        ([1152, 0], 12),
        ([1152, 128], 13),
    ] {
        record(&mut terrain, xz, lookup);
    }
    let mut host = Host {
        mutate_after_damage: true,
        ..Host::default()
    };
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut 0),
        Ok(5)
    );
    assert_eq!(host.lookups, [1, 2, 3, 4, 5, 10, 11, 12, 13]);
    assert_eq!(
        host.damages
            .iter()
            .map(|request| request.xz)
            .collect::<Vec<_>>(),
        [[0, 0], [1024, 128], [1152, 65408], [1152, 0], [1152, 128]]
    );
}

#[test]
fn null_model_clears_only_sparse_payload_before_later_host_failure() {
    let mut terrain = terrain(TerrainSpacing::Units128, 0xf01f);
    // Flat height 31*32 = 992; source Y matches it.
    nine_records(&mut terrain);
    let mut host = Host {
        position: [0, 992, 0],
        null_lookup: Some(1),
        fail_damage: true,
        ..Host::default()
    };
    let before_cell = terrain.cell_at([65408, 65408]);
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut 0),
        Err(TerrainFurnitureEffectError::Host("required 00436020"))
    );
    let (_, cleared) = terrain.sparse_record_at([65408, 65408]).unwrap();
    assert_eq!(
        (cleared.x, cleared.state, cleared.resource_lookup.raw()),
        (511, 0, 0)
    );
    assert_eq!(terrain.cell_at([65408, 65408]), before_cell);
    assert_eq!(host.lookups, [1, 2, 3, 4, 5]);
    assert_eq!(host.damages.len(), 1);
}

#[test]
fn model_radius_addition_wraps_as_signed_word_before_y_comparisons() {
    let mut terrain = terrain(TerrainSpacing::Units128, 0);
    record(&mut terrain, [0, 0], 1);
    let mut host = Host {
        position: [0, 20, 0],
        model_radius: Some(0xffff),
        ..Host::default()
    };
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut 0),
        Ok(1)
    );
    assert_eq!(host.damages[0].effect.strength, 835); // Y becomes -1; distance21.
}

#[test]
fn signed_negative_radius_skips_and_minimum_word_visits_once() {
    let mut terrain = terrain(TerrainSpacing::Units128, 0);
    let mut host = Host::default();
    host.block.outer_radius = 0xffff;
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut 0),
        Ok(0)
    );
    assert!(host.lookups.is_empty());
    host.block.outer_radius = 0x8000;
    record(&mut terrain, [0x8000, 0x8000], 1);
    assert_eq!(
        terrain.apply_furniture_radius_effect(7, &mut host, &mut 0),
        Ok(0)
    );
    assert_eq!(host.lookups, [1]);
}

#[test]
fn scatter_two_is_three_shared_rng_draws_and_keeps_height_and_low_flags() {
    let mut terrain = terrain(TerrainSpacing::Units256, 0xab80);
    let mut rng = 0;
    let outcome = terrain.scatter_cell_flags(
        TerrainFlagScatterRequest {
            position_words: [0, (-4096_i16) as u16, 0],
            count_minus_one: 2,
        },
        &mut rng,
    );
    assert_eq!(
        outcome,
        TerrainFlagScatterOutcome {
            height_gate_passed: true,
            operations: 3
        }
    );
    assert_eq!(rng, 158_245_525);
    assert_eq!(terrain.cell_at([65280, 65280]).raw(), 0x0b80);
    assert_eq!(terrain.cell_at([0, 0]).raw(), 0x0b80);
    assert_eq!(terrain.cells[1].raw(), 0xab80); // Coarse spacing never selects odd cells.
}

#[test]
fn scatter_height_gate_is_inclusive_and_nonpositive_counts_still_execute_once() {
    for count in [-1, 0] {
        let mut terrain = terrain(TerrainSpacing::Units128, 0xf000);
        let mut rng = 0;
        let outcome = terrain.scatter_cell_flags(
            TerrainFlagScatterRequest {
                position_words: [0, 128, 0],
                count_minus_one: count,
            },
            &mut rng,
        );
        assert_eq!(outcome.operations, 1);
        assert_eq!(rng, 1_299_827);
    }
    let mut terrain = terrain(TerrainSpacing::Units128, 0xf000);
    let mut rng = 0x1234;
    assert_eq!(
        terrain.scatter_cell_flags(
            TerrainFlagScatterRequest {
                position_words: [0, 129, 0],
                count_minus_one: 2,
            },
            &mut rng
        ),
        TerrainFlagScatterOutcome {
            height_gate_passed: false,
            operations: 0
        }
    );
    assert_eq!(rng, 0x1234);
    assert!(terrain.cells.iter().all(|cell| cell.raw() == 0xf000));
}
