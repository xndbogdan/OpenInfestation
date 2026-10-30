use std::path::Path;

use inf_formats::{
    parse_font_bank, parse_handler10_bank, parse_handler5_bank, parse_material_bank,
    parse_mission_parameters, parse_mission_placements, parse_model_bank, parse_sound_bank,
    parse_sprite_bank, parse_string_bank, parse_terrain_bank, parse_word_resource_bank, FontBank,
    Handler10Bank, Handler5Bank, HandlerRegion, MaterialBank, MissionParameterBank,
    MissionPlacementBank, ModelBank, OverlayKey, RequestTable, SoundBank, SpriteBank, StringBank,
    TerrainBank, WordResourceBank,
};

use crate::{Error, Result};

/// Parsed resource storage for one handler region. Byte spans always refer to
/// the owning [`crate::LoadedOverlay`], never to relocated retail pointers.
/// Parsed fields alone do not imply that a runtime consumer is implemented.
#[derive(Debug)]
pub enum HandlerResources {
    Strings(StringBank),
    Words(WordResourceBank),
    Sprites(SpriteBank),
    Fonts(FontBank),
    Records5(Handler5Bank),
    Materials(MaterialBank),
    Models(ModelBank),
    Sounds(SoundBank),
    Records10(Handler10Bank),
    MissionPlacements(MissionPlacementBank),
    Terrain(TerrainBank),
    MissionParameters(MissionParameterBank),
    /// Handlers 13/14 retain exact framing and source bytes. Relocation
    /// tables are evidence, not an instruction to manufacture host pointers.
    Opaque,
}

impl HandlerResources {
    pub fn is_opaque(&self) -> bool {
        matches!(self, Self::Opaque)
    }

    pub(crate) fn parse(
        bytes: &[u8],
        region: &HandlerRegion,
        requests: &RequestTable,
        key: &OverlayKey,
        path: &Path,
    ) -> Result<Self> {
        // One dispatch, one canonical parser per proven handler family.
        // The error retains the file and handler even when source errors have
        // different types. Nothing is downgraded to a diagnostic warning.
        let handler = region.handler.get();
        let wrap = |source| Error::Handler {
            path: path.to_owned(),
            handler,
            source,
        };
        let parsed = match handler {
            0 => Self::Strings(parse_string_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            1 | 2 | 6 => {
                Self::Words(parse_word_resource_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?)
            }
            3 => Self::Sprites(parse_sprite_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            4 => Self::Fonts(parse_font_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            13 | 14 => Self::Opaque,
            5 => Self::Records5(parse_handler5_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            7 => {
                Self::Materials(parse_material_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?)
            }
            8 => Self::Models(
                parse_model_bank(bytes, region, &requests.0[8], key)
                    .map_err(|e| wrap(Box::new(e)))?,
            ),
            9 => Self::Sounds(parse_sound_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            10 => {
                Self::Records10(parse_handler10_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?)
            }
            11 => Self::MissionPlacements(
                parse_mission_placements(bytes, region).map_err(|e| wrap(Box::new(e)))?,
            ),
            12 => Self::Terrain(parse_terrain_bank(bytes, region).map_err(|e| wrap(Box::new(e)))?),
            15 => Self::MissionParameters(
                parse_mission_parameters(bytes, region).map_err(|e| wrap(Box::new(e)))?,
            ),
            _ => unreachable!("HandlerId permits only the 16 dispatch handlers"),
        };
        Ok(parsed)
    }
}
