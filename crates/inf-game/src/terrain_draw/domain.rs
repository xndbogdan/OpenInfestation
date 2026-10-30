//! 0044B2E0 scan rows and 0044AA10's fourteen-cell strip partition.

use super::{TerrainDrawFault, TerrainGrid};

/// The exact six-byte native row: signed grid Z, inclusive left vertex,
/// inclusive right vertex. Right-left is the number of tile intervals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainRow {
    pub z: i16,
    pub left: i16,
    pub right: i16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainDomain {
    pub rows: Vec<TerrainRow>,
    pub minimum_x: i32,
    pub maximum_x: i32,
}

struct Edge {
    index: usize,
    slope: i32,
}

impl Edge {
    /// 0044B4C0 decrements;0044B510 increments. Flat edges are skipped.
    fn advance(
        &mut self,
        corners: [[i16; 2]; 4],
        increment: usize,
    ) -> Result<i32, TerrainDrawFault> {
        for _ in 0..4 {
            let previous = corners[self.index];
            self.index = (self.index + increment) & 3;
            let next = corners[self.index];
            let height = i32::from(previous[1]) - i32::from(next[1]);
            if height != 0 {
                self.slope = ((i32::from(next[0]) - i32::from(previous[0])) << 8) / height;
                return Ok(i32::from(previous[0]));
            }
        }
        // Retail loops forever for this degenerate input.
        Err(TerrainDrawFault::FlatFootprint)
    }
}

/// Rows 0044B2E0's buffer holds: every row past the last overwrites it.
pub const RETAIL_TERRAIN_ROWS: usize = 176;

/// Native arithmetic is signed DWORD until each final WORD store. With
/// `capacity` (retail's [`RETAIL_TERRAIN_ROWS`]) the cap overwrites the last
/// row repeatedly; extrema still include every reached row. Without it every
/// row is kept.
pub fn terrain_row_domain(
    corners: [[i16; 2]; 4],
    grid: TerrainGrid,
    capacity: Option<usize>,
) -> Result<TerrainDomain, TerrainDrawFault> {
    grid.validate()?;
    let (mut highest, mut lowest, mut highest_index) = (-65536, 65536, 0);
    for (index, corner) in corners.iter().enumerate() {
        let z = i32::from(corner[1]);
        if z > highest {
            highest = z;
            highest_index = index;
        }
        lowest = lowest.min(z);
    }
    let mut left = Edge {
        index: highest_index,
        slope: 0,
    };
    let mut right = Edge {
        index: highest_index,
        slope: 0,
    };
    let mut left_x = left.advance(corners, 3)?;
    let mut right_x = right.advance(corners, 1)?;
    let spacing = i32::from(grid.spacing);
    let mut domain = TerrainDomain {
        rows: Vec::new(),
        minimum_x: 128,
        maximum_x: -128,
    };
    let mut z = highest;
    while z >= lowest {
        if z - spacing < i32::from(corners[left.index][1]) {
            left_x = left.advance(corners, 3)?;
        }
        // The ELSE belongs only to the right-edge test. Advancing the right
        // edge suppresses BOTH additions, including the left addition.
        if z - spacing < i32::from(corners[right.index][1]) {
            right_x = right.advance(corners, 1)?;
        } else {
            left_x = left_x.wrapping_add(spacing.wrapping_mul(left.slope) >> 8);
            right_x = right_x.wrapping_add(spacing.wrapping_mul(right.slope) >> 8);
        }
        let row = TerrainRow {
            z: ((z & !i32::from(grid.mask)) / spacing) as i16,
            left: (left_x / spacing) as i16,
            right: (right_x / spacing) as i16,
        };
        domain.minimum_x = domain.minimum_x.min(i32::from(row.left));
        domain.maximum_x = domain.maximum_x.max(i32::from(row.right));
        match capacity {
            Some(capacity) if domain.rows.len() == capacity => domain.rows[capacity - 1] = row,
            _ => domain.rows.push(row),
        }
        z -= spacing;
    }
    Ok(domain)
}

impl TerrainDomain {
    /// 0044ADFB/0044B201 copies every row, then clamps the two X words.
    /// It does not discard empty or inverted rows.
    pub fn strips(&self) -> Vec<Vec<TerrainRow>> {
        let mut strips = Vec::new();
        let mut x = self.minimum_x;
        while x < self.maximum_x {
            strips.push(
                self.rows
                    .iter()
                    .map(|row| TerrainRow {
                        z: row.z,
                        left: row.left.max(x as i16),
                        right: row.right.min((x + 14) as i16),
                    })
                    .collect(),
            );
            x += 14;
        }
        strips
    }
}
