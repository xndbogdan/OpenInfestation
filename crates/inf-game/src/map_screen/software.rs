//! The map screen over the software scene.

use thiserror::Error;

use super::*;
use crate::{
    briefing_map::map_tiles,
    briefing_map_annotations::{
        briefing_map_annotation, BriefingAnnotationDraw, BriefingAnnotationError,
        BriefingAnnotationInputs, BriefingMapAnnotation,
    },
    briefing_markers::{
        BriefingMapActor, BriefingMarkerError, BriefingMarkerInputs, BriefingMarkerPass,
    },
    draw_queue::DrawInvocation,
    frontend_renderer::{FrontendRendererError, FrontendRendererState},
    menu::MenuColor,
    menu_text::{
        draw_line_text, draw_raw_text, measure_text_width, LineTextRequest, MenuGlyphQuad,
        MenuTextEmission, MenuTextError, MenuTextHost, MenuTextLayoutError, MenuTextResources,
        MenuTextReveal, RawMenuTextRequest, INITIAL_GLYPH_BASE_WORD,
    },
    object_runtime::{ObjectArena, ObjectError, PlayerId},
    pixel_backend::PixelBackend,
    render_resources::{
        RenderResourceData, RenderResourceError, RenderResourceKind, RenderResourceRequest,
        RenderResources,
    },
    scene_resources::{ResolvedSceneCommand, SceneResourceBackend},
    screen_renderer_preparation::{prepare_screen_renderer, ScenePreparation},
    software_menu::{glyph_command, SoftwareGlyphError},
    software_scene::{SoftwareScene, SoftwareSceneError},
    world_map::WorldMapState,
};

/// The live world the map reads: the actor list for its markers, the
/// annotations, the map's jitter, the process RNG the jitter draws from,
/// and `00488AA0`'s player.
pub(crate) struct MapWorld<'a> {
    pub arena: &'a ObjectArena,
    pub map_state: &'a WorldMapState,
    pub network_active: bool,
    pub rng: &'a mut u32,
    pub player: Option<PlayerId>,
}

#[derive(Debug, Error)]
pub(crate) enum MapDrawError {
    #[error(transparent)]
    Prepare(#[from] FrontendRendererError),
    #[error(transparent)]
    Scene(#[from] SoftwareSceneError),
    #[error(transparent)]
    Resource(#[from] RenderResourceError),
    #[error(transparent)]
    Object(#[from] ObjectError),
    #[error(transparent)]
    Markers(#[from] BriefingMarkerError<RenderResourceError>),
    #[error(transparent)]
    Annotation(#[from] BriefingAnnotationError<RenderResourceError>),
    #[error(transparent)]
    Text(#[from] MenuTextError<RenderResourceError, SoftwareGlyphError>),
    #[error(transparent)]
    Layout(#[from] MenuTextLayoutError),
    #[error("0046BDC0 reads 00488AA0's player, and the world has none")]
    NoPlayer,
    #[error(transparent)]
    Player(#[from] crate::object_runtime::PlayerAttachError),
    #[error(transparent)]
    Grant(#[from] inf_formats::GrantClosureError),
}

pub(crate) struct SoftwareMapFrame<'a, 'f, 'r> {
    pub scene: &'a mut SoftwareScene<'f, 'r>,
    pub resources: RenderResources<'r>,
    pub state: &'a mut FrontendRendererState,
    pub world: MapWorld<'a>,
}

impl<'f, 'r> SoftwareMapFrame<'_, 'f, 'r> {
    pub fn draw(&mut self, screen: &MapState, mission_seconds: u32) -> Result<(), MapDrawError> {
        draw_map_screen(screen, mission_seconds, self)
    }

    fn font(&self) -> Result<&inf_formats::Font, RenderResourceError> {
        self.resources.font(0)
    }

    fn glyphs(&mut self) -> GlyphHost<'_, 'f, 'r> {
        GlyphHost {
            scene: &mut *self.scene,
            resources: self.resources,
        }
    }

    /// One `00472D30` pass: the actors' markers in `G+48` order, then the
    /// annotations, which resolve H4/0 and H6/0 even when there are none.
    fn tile_markers(&mut self, inputs: BriefingMarkerInputs) -> Result<(), MapDrawError> {
        let resources = self.resources;
        let arena = self.world.arena;
        let layout = self.scene.color_layout();
        let mut pass = BriefingMarkerPass::new(inputs);
        for &id in arena.actor_list() {
            // Retained constructor prefixes remain in the actor FIFO, but
            // cannot submit a map marker or consume its jitter RNG.
            if arena.construction_is_stopped(id)? {
                continue;
            }
            let actor = BriefingMapActor::from_arena(arena, id)?;
            let material = |lookup: crate::briefing_markers::BriefingMarkerMaterialLookup| {
                let material = |slot| {
                    let resource = resources.resolve(RenderResourceRequest {
                        kind: RenderResourceKind::Material,
                        argument: slot,
                    })?;
                    let RenderResourceData::Material(material) = resource.data else {
                        unreachable!("typed H7 material")
                    };
                    Ok::<_, RenderResourceError>(layout.convert_material(material))
                };
                let mut word = material(lookup.initial_slot)?;
                if let Some(slot) = lookup.replacement_slot {
                    word = material(slot)?;
                }
                Ok(word)
            };
            for quad in pass.actor(actor, &mut *self.world.rng, material, |slot| {
                resources.style(slot)
            })? {
                self.scene.draw_sprite_direct(quad)?;
            }
        }
        let font = resources.font(0)?;
        resources.style(0)?;
        let annotation_inputs = BriefingAnnotationInputs {
            tile: inputs.tile,
            scan_age: inputs.scan_age,
            radius_high_byte: 0,
        };
        for &id in arena.map_annotation_list() {
            let annotation = BriefingMapAnnotation::from_arena(arena, id)?;
            match briefing_map_annotation(annotation, annotation_inputs, || resources.style(0x24))?
            {
                BriefingAnnotationDraw::Radius(quad) => self.scene.draw_sprite_direct(quad)?,
                BriefingAnnotationDraw::Label {
                    label,
                    position_pixels,
                    color_word,
                } => {
                    // EXE0052F1D0 supplies "%d"; no localization/H0 lookup.
                    let text = label.to_string();
                    let mut request = LineTextRequest {
                        text: text.as_bytes(),
                        font,
                        position_pixels,
                        color: MenuColor::from_word(color_word),
                        base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
                        context_origin_pixels: [0, 0],
                    };
                    draw_line_text(&mut request, &resources, &mut self.glyphs())?;
                }
            }
        }
        Ok(())
    }
}

impl MapDrawHost for SoftwareMapFrame<'_, '_, '_> {
    type Error = MapDrawError;

    fn prepare(&mut self) -> Result<[i32; 2], Self::Error> {
        prepare_screen_renderer(&mut ScenePreparation {
            scene: &mut *self.scene,
            state: &mut *self.state,
        })?;
        let frame = &self.scene.pixels.frame;
        Ok([frame.width() as i32, frame.height() as i32])
    }

    fn begin_batch(&mut self) -> Result<(), Self::Error> {
        self.scene.submit(ResolvedSceneCommand::BeginBatch)?;
        Ok(())
    }

    fn clip(&mut self, rect: [i16; 4]) -> Result<(), Self::Error> {
        self.scene.pixels.pipeline.viewport = rect;
        Ok(())
    }

    fn resource_scanner(&mut self) -> Result<bool, Self::Error> {
        let player = self.world.player.ok_or(MapDrawError::NoPlayer)?;
        let bits = &self.world.arena.world_player(player)?.bits_100;
        Ok(inf_formats::grant_is_owned(bits, 0x13)?)
    }

    fn map(&mut self, request: MapTilesRequest) -> Result<(), Self::Error> {
        let style = self.resources.style(0x3b)?;
        let tiles = map_tiles(request.center, request.size, request.camera_words, style);
        let shifts = self.scene.pixels.renderer.material_shifts;
        for tile in tiles {
            self.scene.draw_sprite_direct(tile.backdrop)?;
            self.tile_markers(BriefingMarkerInputs {
                tile: tile.markers,
                scan_age: request.scan_age,
                jitter_radius: self.world.map_state.jitter_radius(),
                initial_jitter_x: None,
                initial_jitter_y: i32::from(request.resource_scanner),
                network_active: self.world.network_active,
                material_shifts: shifts,
                session_phase: 7,
            })?;
        }
        Ok(())
    }

    fn quad(&mut self, quad: DirectSpriteQuad) -> Result<(), Self::Error> {
        Ok(self.scene.draw_sprite_direct(quad)?)
    }

    fn style(&mut self, slot: i32) -> Result<u32, Self::Error> {
        Ok(self.resources.style(slot)?)
    }

    fn material(&mut self, slot: i32) -> Result<u32, Self::Error> {
        let resource = self.resources.resolve(RenderResourceRequest {
            kind: RenderResourceKind::Material,
            argument: slot,
        })?;
        let RenderResourceData::Material(material) = resource.data else {
            unreachable!("typed H7 material")
        };
        Ok(self.scene.color_layout().convert_material(material))
    }

    fn material_shifts(&mut self) -> [u32; 3] {
        self.scene.pixels.renderer.material_shifts
    }

    fn string(&mut self, id: i32) -> Result<Vec<u8>, Self::Error> {
        Ok(self.resources.string(id)?.to_vec())
    }

    fn text_width(&mut self, text: &[u8]) -> Result<i16, Self::Error> {
        Ok(measure_text_width(self.font()?, text)? as i16)
    }

    fn raw_text(
        &mut self,
        text: &[u8],
        position: &mut [i32; 2],
        width: i32,
        color_word: u32,
        age_millis: i32,
    ) -> Result<(), Self::Error> {
        let resources = self.resources;
        let mut request = RawMenuTextRequest {
            text,
            font: resources.font(0)?,
            position_pixels: *position,
            width_pixels: width,
            color: MenuColor::from_word(color_word),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: [0, 0],
            emission: MenuTextEmission::Draw,
            reveal: MenuTextReveal::Timed { age_millis },
        };
        draw_raw_text(&mut request, &resources, &mut self.glyphs())?;
        *position = request.position_pixels;
        Ok(())
    }

    fn line_text(
        &mut self,
        text: &[u8],
        position: [i32; 2],
        color_word: u32,
    ) -> Result<(), Self::Error> {
        let resources = self.resources;
        let mut request = LineTextRequest {
            text,
            font: resources.font(0)?,
            position_pixels: position,
            color: MenuColor::from_word(color_word),
            base_color: MenuColor::from_word(INITIAL_GLYPH_BASE_WORD),
            context_origin_pixels: [0, 0],
        };
        draw_line_text(&mut request, &resources, &mut self.glyphs())?;
        Ok(())
    }
}

/// The font's `+A4` glyph passes, drawn at once.
struct GlyphHost<'a, 'f, 'r> {
    scene: &'a mut SoftwareScene<'f, 'r>,
    resources: RenderResources<'r>,
}

impl MenuTextHost for GlyphHost<'_, '_, '_> {
    type Error = SoftwareGlyphError;
    fn submit_glyph(&mut self, glyph: MenuGlyphQuad) -> Result<(), Self::Error> {
        let mut command = glyph_command(self.resources, glyph)?;
        self.scene.pixels.draw(
            DrawInvocation {
                command_offset: 0,
                receiver_offset: 0,
            },
            &mut command,
        )?;
        Ok(())
    }
}
