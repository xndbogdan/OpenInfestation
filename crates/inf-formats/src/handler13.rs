//! Handler-13/14 relocation-framed resources: census groundwork.
//!
//! The shared loader `FUN_004480B0` reads each region through `FUN_004eddb0`
//! (main allocation, relocation table, then one entry-pointer offset per
//! requested resource) and registers every entry with `FUN_004EFC00`. The
//! canonical parsers preserve those exact framed bytes, request segments, and
//! local-to-global mappings. `world_assets` now decodes the common constructor
//! prefix and world placement graph; class-specific suffixes remain opaque.
//!
//! This module stages that consumer search. It answers "which scenes request
//! Handler-13/14 resources in which load phases" from `PreLoad.dat` alone, so
//! the first consumer hunt starts from the five Handler-13 regions and their
//! 162 resources instead of a corpus-wide guess. Consumer provenance belongs
//! to `world_assets` and the decompiled WORLD_ASSETS documentation.

use serde::Serialize;

use crate::preload::{Preload, RequestKind};

pub const HANDLER_13: u8 = 13;
pub const HANDLER_14: u8 = 14;

/// Per-phase requested counts for Handlers 13 and 14 in one scene, in
/// `FUN_004EE680` dispatch order: base, difficulty, language, combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct RelocationSceneCensus {
    pub level: u8,
    pub handler_13: [i16; 4],
    pub handler_14: [i16; 4],
}

impl RelocationSceneCensus {
    pub const fn total_13(self) -> i32 {
        self.handler_13[0] as i32
            + self.handler_13[1] as i32
            + self.handler_13[2] as i32
            + self.handler_13[3] as i32
    }

    pub const fn total_14(self) -> i32 {
        self.handler_14[0] as i32
            + self.handler_14[1] as i32
            + self.handler_14[2] as i32
            + self.handler_14[3] as i32
    }
}

/// Count one handler's requests in a scene's four load phases.
fn phase_counts(preload: &Preload, level: u8, handler: u8) -> Option<[i16; 4]> {
    let leaf = preload.leaf(level)?;
    let tables = [
        &leaf.base,
        &leaf.difficulty,
        &leaf.language,
        &leaf.difficulty_language,
    ];
    let index = usize::from(handler);
    let mut counts = [0; 4];
    for (slot, table) in counts.iter_mut().zip(tables) {
        *slot = table.get(index)?.count;
    }
    Some(counts)
}

/// Census one scene's Handler-13/14 requests. Returns `None` for levels absent
/// from the scene tree (retail IDs 44-46).
pub fn scene_relocation_census(preload: &Preload, level: u8) -> Option<RelocationSceneCensus> {
    Some(RelocationSceneCensus {
        level,
        handler_13: phase_counts(preload, level, HANDLER_13)?,
        handler_14: phase_counts(preload, level, HANDLER_14)?,
    })
}

/// Total requested Handler-13/14 resources across every scene and phase.
pub fn global_relocation_requested(preload: &Preload) -> (i32, i32) {
    let mut total_13 = 0;
    let mut total_14 = 0;
    for leaf in preload.leaves() {
        if let Some(census) = scene_relocation_census(preload, leaf.overlay_id) {
            total_13 += census.total_13();
            total_14 += census.total_14();
        }
    }
    (total_13, total_14)
}

/// Dispatch order shared with [`SceneContext`](crate::SceneContext) phases.
pub const RELOCATION_PHASE_ORDER: [RequestKind; 4] = [
    RequestKind::Base,
    RequestKind::Difficulty,
    RequestKind::Language,
    RequestKind::DifficultyLanguage,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn retail_preload() -> Preload {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let bytes = std::fs::read(root.join("PreLoad.dat")).expect("retail PreLoad.dat");
        Preload::parse(&bytes).expect("retail PreLoad.dat parses")
    }

    #[test]
    fn retail_handler_13_is_five_base_regions_and_162_resources() {
        let preload = retail_preload();
        let (total_13, _) = global_relocation_requested(&preload);
        assert_eq!(total_13, 162);

        // (level, base count): 1.ovl holds 131, the rest are small banks.
        // Every non-base phase requests zero, so no later phase overwrites them.
        let expected = [(1, 131), (6, 1), (29, 12), (30, 12), (32, 6)];
        let mut found = Vec::new();
        for leaf in preload.leaves() {
            let census = scene_relocation_census(&preload, leaf.overlay_id)
                .expect("leaf resolves its own level");
            assert_eq!(census.handler_13[1..], [0, 0, 0]);
            if census.handler_13[0] != 0 {
                found.push((leaf.overlay_id, census.handler_13[0]));
            }
        }
        found.sort();
        assert_eq!(found, expected);
    }

    #[test]
    fn retail_handler_14_is_one_base_resource_per_level_11_through_43() {
        let preload = retail_preload();
        let (_, total_14) = global_relocation_requested(&preload);
        assert_eq!(total_14, 33);

        for level in 11..=43 {
            let census = scene_relocation_census(&preload, level).expect("level has a scene leaf");
            assert_eq!(census.handler_14, [1, 0, 0, 0], "level {level}");
        }
        for leaf in preload.leaves() {
            let census = scene_relocation_census(&preload, leaf.overlay_id)
                .expect("leaf resolves its own level");
            if !(11..=43).contains(&leaf.overlay_id) {
                assert_eq!(census.handler_14, [0, 0, 0, 0]);
            }
        }
    }

    #[test]
    fn level_1_census_is_base_only_relocation() {
        let preload = retail_preload();
        let census = scene_relocation_census(&preload, 1).expect("level 1 exists");
        assert_eq!(census.handler_13, [131, 0, 0, 0]);
        assert_eq!(census.handler_14, [0, 0, 0, 0]);
        assert_eq!(census.total_13(), 131);
    }

    #[test]
    fn unknown_levels_have_no_relocation_census() {
        let preload = retail_preload();
        assert!(scene_relocation_census(&preload, 44).is_none());
    }
}
