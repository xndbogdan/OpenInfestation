//!00436F10 sparse replacement and its437060/4370F0 program adapters.
//!The live H12 and process fallback stay distinct; a missing flagged record
//!returns normally before any random draw, rather than selecting fallback.

use inf_formats::TerrainResourceLookup;

use super::ProcessFurnitureQueryState;
use crate::{
    name_entry::name_entry_rng_next,
    terrain_runtime::{TerrainRuntime, TerrainSparsePayload, TerrainSparseWriteError},
};

#[derive(Clone, Copy, Debug)]
pub(crate) enum FurnitureReplacement {
    ///4370F0 supplies lookupFD without the earlier cell write.
    LookupFd,
    ///437060 clears the cell's high nibble before supplying lookupFF.
    ClearCellAndLookupFf,
}

pub(crate) fn run_furniture_replacement(
    terrain: &mut TerrainRuntime,
    process: &mut ProcessFurnitureQueryState,
    position: [u16; 3],
    replacement: FurnitureReplacement,
    rng: &mut u32,
) -> Result<bool, TerrainSparseWriteError> {
    let xz = [position[0], position[2]];
    let lookup = match replacement {
        FurnitureReplacement::LookupFd => 0xfd,
        FurnitureReplacement::ClearCellAndLookupFf => {
            terrain.copy_aligned_low_flags_to_cell(xz);
            0xff
        }
    };
    //436F63..7C: a flagged cell can have a native NULL lookup. Only an
    //unflagged cell selects the physical process record005483C4.
    let (record, mut payload) = if terrain.cell_at(xz).has_sparse_record() {
        let Some((record, value)) = terrain.sparse_record_at(xz) else {
            return Ok(true);
        };
        (
            Some(record),
            TerrainSparsePayload {
                state: value.state,
                resource_lookup: value.resource_lookup,
            },
        )
    } else {
        (None, process.fallback_payload())
    };
    payload.resource_lookup = TerrainResourceLookup::from_raw(lookup);
    if let Some(record) = record {
        terrain.write_sparse_payload(record, payload)?;
    } else {
        process.write_fallback_lookup(lookup);
    }
    payload.state |= (name_entry_rng_next(rng) as u8).wrapping_shl(4);
    if let Some(record) = record {
        terrain.write_sparse_payload(record, payload)?;
    } else {
        process.write_fallback_state(payload.state);
    }
    let mut x_offset = 0_u16;
    loop {
        //436FC1..437001 reloads the original XYZ each iteration. The current
        //pure RNG owner cannot mutate this stack position or H12 residency.
        terrain.copy_aligned_low_flags_to_cell([xz[0].wrapping_add(x_offset), xz[1]]);
        name_entry_rng_next(rng); //437005 is consumed even though EAX is discarded.
        x_offset = if name_entry_rng_next(rng) & 1 != 0 {
            128
        } else {
            (-128_i16) as u16
        };
        if name_entry_rng_next(rng) & 0xff <= 0xe6 {
            break;
        }
    }
    payload.state = (payload.state & !3) | (name_entry_rng_next(rng) as u8 & 3);
    if let Some(record) = record {
        terrain.write_sparse_payload(record, payload)?;
    } else {
        process.write_fallback_state(payload.state);
    }
    Ok(true)
}
