//! Explicit level/difficulty/language load context for one retail scene.
//!
//! `FUN_004EE680` loads the four OVL file types for the current level in a
//! fixed order: base (`N.ovl`), difficulty (`NdM.ovl`), language (`NlM.ovl`),
//! then combined (`NdMlM.ovl`). Each phase registers its requested resources
//! through `FUN_004EFC00`, which overwrites the mapped global slot, so a later
//! phase replaces an earlier one. This module names that context and order so
//! scene-active tables stop being guessed from the corpus-wide registration
//! graph. Coexisting leaves are governed by the authored Group/Select tree
//! (see `docs/re/RUNTIME_FOUNDATION.md`). This single-leaf context does not
//! execute that lifetime policy or imply a complete resident resource table.
//!
//! Observed retail ranges on disk are difficulties 0-4 and languages 0-5 (six
//! language names are registered by `0.ovl` Handler 0). The constructor does
//! not enforce them: request tables belong to the leaf, independent of these
//! selectors, so an unsupported selector can name an absent required file.

use serde::{Deserialize, Serialize};

use crate::overlay::{OverlayKey, OverlayVariant};
use crate::preload::{Preload, RequestKind, RequestTable, HANDLER_COUNT};

/// Level/difficulty/language selection identifying one retail scene load.
///
/// Difficulty mirrors `DAT_0065E72C` and language mirrors `DAT_0065E72D`;
/// `PreLoad.dat` stores their startup values in its first two header bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SceneContext {
    pub level: u8,
    pub difficulty: u8,
    pub language: u8,
}

impl SceneContext {
    pub const fn new(level: u8, difficulty: u8, language: u8) -> Self {
        Self {
            level,
            difficulty,
            language,
        }
    }

    /// The four OVL keys in `FUN_004EE680` dispatch order.
    pub const fn overlay_keys(self) -> [OverlayKey; 4] {
        [
            OverlayKey {
                level: self.level,
                variant: OverlayVariant::Base,
            },
            OverlayKey {
                level: self.level,
                variant: OverlayVariant::Difficulty {
                    value: self.difficulty,
                },
            },
            OverlayKey {
                level: self.level,
                variant: OverlayVariant::Language {
                    value: self.language,
                },
            },
            OverlayKey {
                level: self.level,
                variant: OverlayVariant::DifficultyLanguage {
                    difficulty: self.difficulty,
                    language: self.language,
                },
            },
        ]
    }

    /// The four OVL filenames in `FUN_004EE680` dispatch order.
    ///
    /// Every emitted name round-trips through
    /// [`OverlayKey::parse_filename`] to the matching [`RequestKind`].
    pub fn overlay_filenames(self) -> [String; 4] {
        [
            format!("{}.ovl", self.level),
            format!("{}d{}.ovl", self.level, self.difficulty),
            format!("{}l{}.ovl", self.level, self.language),
            format!("{}d{}l{}.ovl", self.level, self.difficulty, self.language),
        ]
    }

    /// The four `PreLoad.dat` request tables for this scene in dispatch order:
    /// base, difficulty, language, then combined.
    ///
    /// Returns `None` when the level is absent from the scene tree (retail
    /// defines IDs 0-43, 47, and 48; IDs 44-46 are absent).
    pub fn request_tables(self, preload: &Preload) -> Option<[&RequestTable; 4]> {
        let leaf = preload.leaf(self.level)?;
        Some([
            &leaf.base,
            &leaf.difficulty,
            &leaf.language,
            &leaf.difficulty_language,
        ])
    }
}

/// Phases of [`SceneContext::request_tables`] that request at least one
/// resource from `handler`, in `FUN_004EE680` dispatch order.
///
/// A zero count means the handler is skipped for that phase and consumes no
/// file bytes; it is not a silent empty registration.
pub fn active_phases_for_handler(requests: [&RequestTable; 4], handler: u8) -> Vec<RequestKind> {
    const KINDS: [RequestKind; 4] = [
        RequestKind::Base,
        RequestKind::Difficulty,
        RequestKind::Language,
        RequestKind::DifficultyLanguage,
    ];
    let index = usize::from(handler);
    if index >= HANDLER_COUNT {
        return Vec::new();
    }
    requests
        .iter()
        .zip(KINDS)
        .filter_map(|(table, kind)| {
            table
                .get(index)
                .filter(|request| request.count != 0)
                .map(|_| kind)
        })
        .collect()
}

/// Resolve the scene-active value for one global slot from its per-phase
/// candidates in dispatch order.
///
/// Later phases overwrite earlier ones through `FUN_004EFC00`, so the last
/// present candidate wins. `None` means no phase of this scene registers the
/// slot.
pub fn resolve_scene_winner<T: Clone>(phases: [Option<T>; 4]) -> Option<T> {
    phases.into_iter().flatten().last()
}

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
    fn filenames_follow_dispatch_order_and_parse_back() {
        let context = SceneContext::new(17, 4, 0);
        assert_eq!(
            context.overlay_filenames(),
            [
                "17.ovl".to_owned(),
                "17d4.ovl".to_owned(),
                "17l0.ovl".to_owned(),
                "17d4l0.ovl".to_owned(),
            ]
        );

        let keys = context.overlay_keys();
        let filenames = context.overlay_filenames();
        assert_eq!(
            keys.clone().map(|key| key.request_kind()),
            [
                RequestKind::Base,
                RequestKind::Difficulty,
                RequestKind::Language,
                RequestKind::DifficultyLanguage,
            ]
        );
        for (filename, key) in filenames.iter().zip(keys) {
            assert_eq!(OverlayKey::parse_filename(filename).unwrap(), key);
        }
    }

    #[test]
    fn later_phases_overwrite_earlier_ones() {
        assert_eq!(
            resolve_scene_winner([Some("base"), None, Some("lang"), None]),
            Some("lang")
        );
        assert_eq!(
            resolve_scene_winner([Some("base"), Some("diff"), None, None]),
            Some("diff")
        );
        assert_eq!(resolve_scene_winner::<&str>([None, None, None, None]), None);
    }

    #[test]
    fn unknown_levels_have_no_request_tables() {
        let preload = retail_preload();
        // IDs 44-46 are absent from the retail scene tree.
        assert!(SceneContext::new(44, 0, 0)
            .request_tables(&preload)
            .is_none());
        assert!(SceneContext::new(45, 0, 0)
            .request_tables(&preload)
            .is_none());
        assert!(SceneContext::new(46, 0, 0)
            .request_tables(&preload)
            .is_none());
    }

    #[test]
    fn retail_handler_8_is_base_only_in_every_scene() {
        let preload = retail_preload();
        // 36 OVL regions, 457 sub-blocks, 5,142 entries: every Handler-8
        // registration comes from a level's base OVL. No difficulty, language,
        // or combined phase requests Handler 8, so no later phase can
        // overwrite the base scene-active model table.
        let mut base_levels = 0;
        for leaf in preload.leaves() {
            let context = SceneContext::new(leaf.overlay_id, 0, 0);
            let requests = context
                .request_tables(&preload)
                .expect("leaf resolves its own level");
            let active = active_phases_for_handler(requests, 8);
            let base_count = requests[0].get(8).expect("16-handler table").count;
            if base_count != 0 {
                base_levels += 1;
                assert_eq!(active, [RequestKind::Base]);
                assert!(base_count > 0);
            } else {
                assert!(active.is_empty());
            }
            for table in &requests[1..] {
                assert_eq!(table.get(8).expect("16-handler table").count, 0);
            }
        }
        assert_eq!(base_levels, 36);
    }

    #[test]
    fn retail_level_1_base_requests_match_the_pinned_census() {
        let preload = retail_preload();
        let context = SceneContext::new(1, 0, 0);
        let requests = context.request_tables(&preload).expect("level 1 exists");
        // Base: 278 Handler-8 models, 47 Handler-9 sounds, 131 Handler-13
        // resources; difficulty adds Handler-3/10, language adds strings.
        assert_eq!(requests[0].get(8).unwrap().count, 278);
        assert_eq!(requests[0].get(9).unwrap().count, 47);
        assert_eq!(requests[0].get(13).unwrap().count, 131);
        // Level 1 base registers no sprites; its Handler-3 pool arrives via
        // the difficulty phase alone.
        assert_eq!(
            active_phases_for_handler(requests, 3),
            [RequestKind::Difficulty]
        );
    }
}
