//! Presentation of a validated, bounded healthsmall draw observation.
//!
//! Captured XY is already in native screen coordinates. It is only translated
//! and uniformly magnified here; quantization and coincident points survive.
//! The optional orbit geometry instead orients authored projection inputs by
//! the observed nested basis. Neither representation reads the unwritten world
//! XYZ in projection-only slots, and neither claims native face visibility,
//! material appearance, shading, or rasterization.

use std::fmt;

use inf_formats::native_basis::NativeBasis;
use inf_formats::observed_model::{HealthSmallObservation, ObservedProjectionSlot};
use inf_formats::{
    ByteSpan, ModelDiagnosticCompleteness, ModelDiagnosticFaceFlags, ModelDiagnosticFaceLighting,
    ModelDiagnosticFacePresentation, ModelDiagnosticGeometry, ModelDiagnosticVertex, ModelEntry,
    ModelInstructionKind, ModelNestedKind, ModelSlotRef,
};

use crate::render::ProjectedModelVertex;

/// Authored triangle provenance; spans refer to the retained model-entry bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapturedTriangleSource {
    pub command_span: ByteSpan,
    pub fan_triangle_index: u16,
}

#[derive(Clone, Debug)]
pub struct CapturedModelPresentation {
    /// Indices address both `projections` and `source_geometry.vertices`.
    /// All authored triangles are shown; native culling is not replayed.
    pub triangles: Vec<[usize; 3]>,
    pub triangle_sources: Vec<CapturedTriangleSource>,
    pub projections: Vec<ObservedProjectionSlot>,
    /// Authored inputs oriented by the observed nested basis, with native
    /// integer point rounding. Translation is deliberately excluded for orbit
    /// framing. These positions are computed offsets, never observed world XYZ.
    /// This reuses the renderer's topology shape, not its static-analysis path.
    pub source_geometry: ModelDiagnosticGeometry,
    /// Native signed screen coordinates: minimum X/Y, maximum X/Y.
    pub screen_bounds: [i32; 4],
}

#[derive(Clone, Debug)]
pub struct CapturedScreenProjection {
    /// Viewport-local display pixels, retaining the captured screen-Y direction.
    /// Depth is retained as metadata; this wireframe does not depth-test faces.
    pub vertices: Vec<ProjectedModelVertex>,
    pub pixels_per_captured_pixel: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapturedModelError {
    UnsupportedSource(&'static str),
    ProjectionSlots,
    InvalidProjection { slot: u16 },
    UninitializedFaceSlot { slot: u16 },
    NonFiniteBasis,
}

impl fmt::Display for CapturedModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSource(reason) => write!(f, "unsupported captured model: {reason}"),
            Self::ProjectionSlots => {
                f.write_str("captured model requires all 84 ordered even slots 2..168")
            }
            Self::InvalidProjection { slot } => write!(
                f,
                "captured slot {slot} has inconsistent or unprojected fields"
            ),
            Self::UninitializedFaceSlot { slot } => write!(
                f,
                "authored face references uninitialized captured slot {slot}"
            ),
            Self::NonFiniteBasis => {
                f.write_str("captured nested basis contains a non-finite component")
            }
        }
    }
}

impl std::error::Error for CapturedModelError {}

impl CapturedModelPresentation {
    /// The importer owns event correlation and source-byte fingerprint checks.
    /// Its parsed `source_model` comes from those same retained entry bytes;
    /// there is no catalog lookup, static fallback, or inferred callback here.
    pub fn build(observation: &HealthSmallObservation) -> Result<Self, CapturedModelError> {
        if observation.near_far != 1 {
            return Err(CapturedModelError::UnsupportedSource(
                "only the observed far path is supported",
            ));
        }
        build_presentation(
            &observation.source_model,
            &observation.projection_slots,
            observation.nested.basis,
        )
    }

    /// Fit the captured pixel rectangle without reprojection, reflection, or
    /// rounding of native XY. A zero-size viewport produces a zero scale.
    pub fn fit_screen(&self, viewport_size: [u32; 2]) -> CapturedScreenProjection {
        let [min_x, min_y, max_x, max_y] = self.screen_bounds;
        let width = (max_x - min_x).max(1) as f32;
        let height = (max_y - min_y).max(1) as f32;
        let scale = (viewport_size[0] as f32 / width).min(viewport_size[1] as f32 / height) * 0.78;
        let center = [
            (min_x as f32 + max_x as f32) * 0.5,
            (min_y as f32 + max_y as f32) * 0.5,
        ];
        let vertices = self
            .projections
            .iter()
            .map(|slot| {
                let [x, y] = captured_xy(slot);
                ProjectedModelVertex {
                    x: (x as f32 - center[0]) * scale + viewport_size[0] as f32 * 0.5,
                    y: (y as f32 - center[1]) * scale + viewport_size[1] as f32 * 0.5,
                    depth: slot.depth_word as i32 as f32,
                }
            })
            .collect();
        CapturedScreenProjection {
            vertices,
            pixels_per_captured_pixel: scale,
        }
    }
}

fn captured_xy(slot: &ObservedProjectionSlot) -> [i32; 2] {
    [
        i32::from(slot.packed_xy as u16 as i16),
        i32::from((slot.packed_xy >> 16) as u16 as i16),
    ]
}

fn build_presentation(
    entry: &ModelEntry,
    projections: &[ObservedProjectionSlot],
    basis: NativeBasis,
) -> Result<CapturedModelPresentation, CapturedModelError> {
    if entry.identity.global_index != 168
        || entry.header.vertex_slot_count != 170
        || entry.header.raw_vertex_count != 85
        || entry.vertices.len() != 85
    {
        return Err(CapturedModelError::UnsupportedSource(
            "expected the canonical healthsmall allocation",
        ));
    }
    if basis.words[1..]
        .iter()
        .any(|word| !f32::from_bits(*word).is_finite())
    {
        return Err(CapturedModelError::NonFiniteBasis);
    }
    if projections.len() != 84
        || projections
            .iter()
            .enumerate()
            .any(|(index, slot)| usize::from(slot.slot) != (index + 1) * 2)
    {
        return Err(CapturedModelError::ProjectionSlots);
    }
    let mut vertices = Vec::with_capacity(projections.len());
    let mut screen_bounds = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for slot in projections {
        // The far callback does not freshly write fog on its depth<64 path.
        // Requiring the actual projected path keeps all ten claimed bytes valid.
        if (slot.depth_word as i32) < 64
            || slot.packed_xy != u32::from_le_bytes(slot.raw[12..16].try_into().unwrap())
            || slot.depth_word != u32::from_le_bytes(slot.raw[16..20].try_into().unwrap())
            || slot.clip != slot.raw[20]
            || slot.fog != slot.raw[21]
        {
            return Err(CapturedModelError::InvalidProjection { slot: slot.slot });
        }
        let [x, y] = captured_xy(slot);
        screen_bounds[0] = screen_bounds[0].min(x);
        screen_bounds[1] = screen_bounds[1].min(y);
        screen_bounds[2] = screen_bounds[2].max(x);
        screen_bounds[3] = screen_bounds[3].max(y);
        let raw = entry.vertices[usize::from(slot.slot) / 2];
        vertices.push(ModelDiagnosticVertex {
            slot: ModelSlotRef::from_raw(slot.slot),
            position: basis.transform_point([i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)]),
        });
    }

    let instructions = &entry.program.instructions;
    let Some(first) = instructions.first() else {
        return Err(CapturedModelError::UnsupportedSource("missing F5 operand"));
    };
    if first.raw_command != 0x00f5
        || !matches!(&first.kind, ModelInstructionKind::RegisterOperation(command) if command.arguments == [1, 0x0080, 0x400a])
    {
        return Err(CapturedModelError::UnsupportedSource(
            "unexpected F5 operand",
        ));
    }
    let Some(second) = instructions.get(1) else {
        return Err(CapturedModelError::UnsupportedSource(
            "missing nested projection producer",
        ));
    };
    if second.raw_command != 0x0005
        || !matches!(&second.kind, ModelInstructionKind::NestedProgram(nested)
            if nested.kind == ModelNestedKind::Transform
                && nested.arguments == [0, 0x00c1, 0, 0xc000, 2, 170, 170, 170, 170, 170, 170, 170]
                && nested.program.instructions.is_empty())
    {
        return Err(CapturedModelError::UnsupportedSource(
            "unexpected nested projection producer",
        ));
    }
    let mut triangles = Vec::new();
    let mut triangle_sources = Vec::new();
    let mut face_presentation = Vec::new();
    for instruction in &instructions[2..] {
        let ModelInstructionKind::TriangleFan(fan) = &instruction.kind else {
            return Err(CapturedModelError::UnsupportedSource(
                "non-fan command after the observed producer",
            ));
        };
        if instruction.raw_command != 1
            || fan.flags.emits_mirror_instance()
            || fan.vertices.len() < 3
        {
            return Err(CapturedModelError::UnsupportedSource(
                "unsupported fan topology",
            ));
        }
        for (triangle_index, pair) in fan.vertices[1..].windows(2).enumerate() {
            let source_vertices = [&fan.vertices[0], &pair[0], &pair[1]];
            let mut triangle = [0; 3];
            for (index, vertex) in source_vertices.iter().enumerate() {
                let slot = vertex.vertex.raw();
                if !(2..170).contains(&slot) || slot & 1 != 0 {
                    return Err(CapturedModelError::UninitializedFaceSlot { slot });
                }
                triangle[index] = usize::from(slot) / 2 - 1;
            }
            let triangle_index = u16::try_from(triangle_index).map_err(|_| {
                CapturedModelError::UnsupportedSource("fan triangle count exceeds native range")
            })?;
            triangles.push(triangle);
            triangle_sources.push(CapturedTriangleSource {
                command_span: instruction.span,
                fan_triangle_index: triangle_index,
            });
            face_presentation.push(ModelDiagnosticFacePresentation {
                source_opcode: 1,
                material: fan.material,
                source_flags: ModelDiagnosticFaceFlags::Fan(fan.flags),
                source_lighting: ModelDiagnosticFaceLighting::PerVertex(
                    source_vertices
                        .iter()
                        .map(|vertex| vertex.lighting_ref)
                        .collect(),
                ),
                fan_vertex_attributes: Some(
                    source_vertices
                        .iter()
                        .map(|vertex| vertex.attributes.clone())
                        .collect(),
                ),
                fan_triangle_index: Some(triangle_index),
                mirrored_instance: false,
            });
        }
    }
    if triangles.len() != 120 {
        return Err(CapturedModelError::UnsupportedSource(
            "expected all 120 authored triangles",
        ));
    }
    let faces = triangles.iter().map(|triangle| triangle.to_vec()).collect();
    Ok(CapturedModelPresentation {
        triangles,
        triangle_sources,
        projections: projections.to_vec(),
        source_geometry: ModelDiagnosticGeometry {
            view_plane_decisions: Vec::new(),
            vertices,
            faces,
            face_presentation,
            ignored_edge_commands: 0,
            completeness: ModelDiagnosticCompleteness::CompleteMesh,
            unresolved_dependencies: Vec::new(),
            omitted_effects: Vec::new(),
        },
        screen_bounds,
    })
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::OnceLock;

    use inf_formats::{
        parse_model_bank, HandlerFraming, HandlerId, HandlerRegion, HandlerRequest, ModelSubBlock,
        OverlayKey, ResourceSegment,
    };

    use super::*;

    fn healthsmall() -> &'static ModelEntry {
        static ENTRY: OnceLock<ModelEntry> = OnceLock::new();
        ENTRY.get_or_init(|| {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
            let overlay = std::fs::read(root.join("Overlay/1.ovl")).unwrap();
            let bytes = &overlay[0x20eb8..0x22110];
            let span = ByteSpan::new(0, bytes.len());
            // This frames the existing entry bytes; no OVL bytes are rewritten.
            parse_model_bank(
                bytes,
                &HandlerRegion {
                    handler: HandlerId::MODELS,
                    requested_count: 1,
                    span,
                    framing: HandlerFraming::ModelSubBlocks {
                        total_entries: 1,
                        blocks: vec![ModelSubBlock {
                            span,
                            entry_count: 1,
                            payload: span,
                        }],
                    },
                },
                &HandlerRequest {
                    count: 1,
                    segments: vec![ResourceSegment {
                        local_start: 0,
                        global_delta: 168,
                    }],
                },
                &OverlayKey::parse_filename("1.ovl").unwrap(),
            )
            .unwrap()
            .entries
            .remove(0)
        })
    }

    fn projections() -> Vec<ObservedProjectionSlot> {
        (0..84)
            .map(|index| {
                let x = (index % 7 - 3) as i16;
                let y = (index / 7 - 6) as i16;
                let packed_xy = u32::from(x as u16) | (u32::from(y as u16) << 16);
                let depth_word = 1000 + index as u32;
                let mut raw = [0xa5; 24];
                raw[12..16].copy_from_slice(&packed_xy.to_le_bytes());
                raw[16..20].copy_from_slice(&depth_word.to_le_bytes());
                raw[20] = 0x12;
                raw[21] = 0x42;
                ObservedProjectionSlot {
                    slot: ((index + 1) * 2) as u16,
                    packed_xy,
                    depth_word,
                    clip: 0x12,
                    fog: 0x42,
                    raw,
                }
            })
            .collect()
    }

    #[test]
    fn canonical_topology_keeps_all_index_domains_and_fan_provenance_aligned() {
        let presentation =
            build_presentation(healthsmall(), &projections(), NativeBasis::INITIAL).unwrap();
        assert_eq!(presentation.projections.len(), 84);
        assert_eq!(presentation.source_geometry.vertices.len(), 84);
        assert_eq!(presentation.triangles.len(), 120);
        assert_eq!(presentation.triangle_sources.len(), 120);
        assert_eq!(presentation.source_geometry.face_presentation.len(), 120);
        let mut expected = Vec::new();
        for instruction in &healthsmall().program.instructions[2..] {
            let ModelInstructionKind::TriangleFan(fan) = &instruction.kind else {
                panic!("retail fan");
            };
            for pair in fan.vertices[1..].windows(2) {
                expected.push([
                    fan.vertices[0].vertex.raw(),
                    pair[0].vertex.raw(),
                    pair[1].vertex.raw(),
                ]);
            }
        }
        for (index, triangle) in presentation.triangles.iter().enumerate() {
            assert_eq!(
                triangle.map(|vertex| presentation.projections[vertex].slot),
                expected[index]
            );
            assert_eq!(presentation.source_geometry.faces[index], triangle.to_vec());
            assert!(presentation.triangle_sources[index].command_span.start >= 0x43a);
        }
        for (index, vertex) in presentation.source_geometry.vertices.iter().enumerate() {
            assert_eq!(vertex.slot.raw(), presentation.projections[index].slot);
            let raw = healthsmall().vertices[index + 1];
            assert_eq!(
                vertex.position,
                [i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)]
            );
        }
    }

    #[test]
    fn oriented_source_geometry_never_reads_unwritten_world_xyz_or_padding() {
        let basis = NativeBasis {
            words: [17, 0, 0x3f800000, 0, 0xbf800000, 0, 0, 0, 0, 0x3f800000],
        };
        let mut slots = projections();
        let before = build_presentation(healthsmall(), &slots, basis).unwrap();
        for slot in &mut slots {
            slot.raw[..12].fill(0xff);
            slot.raw[22..].fill(0x17);
        }
        let after = build_presentation(healthsmall(), &slots, basis).unwrap();
        assert_eq!(before.source_geometry, after.source_geometry);
        assert_eq!(
            before.fit_screen([640, 480]).vertices,
            after.fit_screen([640, 480]).vertices
        );
        for (index, vertex) in after.source_geometry.vertices.iter().enumerate() {
            let raw = healthsmall().vertices[index + 1];
            assert_eq!(
                vertex.position,
                [-i32::from(raw.y), i32::from(raw.x), i32::from(raw.z)]
            );
        }
    }

    #[test]
    fn screen_fit_keeps_signed_xy_direction_quantization_and_one_uniform_scale() {
        let mut slots = projections();
        slots[1].packed_xy = slots[0].packed_xy;
        let xy = slots[0].raw[12..16].to_vec();
        slots[1].raw[12..16].copy_from_slice(&xy);
        let presentation = build_presentation(healthsmall(), &slots, NativeBasis::INITIAL).unwrap();
        assert_eq!(presentation.screen_bounds, [-3, -6, 3, 5]);
        let fit = presentation.fit_screen([600, 440]);
        assert!((fit.pixels_per_captured_pixel - 31.2).abs() < 0.0001);
        assert_eq!(fit.vertices[0].x, fit.vertices[1].x);
        assert_eq!(fit.vertices[0].y, fit.vertices[1].y);
        assert!(
            (fit.vertices[3].x - fit.vertices[2].x - fit.pixels_per_captured_pixel).abs() < 0.0001
        );
        assert!(
            (fit.vertices[7].y - fit.vertices[0].y - fit.pixels_per_captured_pixel).abs() < 0.0001
        );
        assert_eq!(fit.vertices[0].depth, 1000.0);
    }

    #[test]
    fn missing_reordered_and_duplicate_slots_fail_closed() {
        let mut slots = projections();
        slots.pop();
        assert!(matches!(
            build_presentation(healthsmall(), &slots, NativeBasis::INITIAL),
            Err(CapturedModelError::ProjectionSlots)
        ));
        let mut slots = projections();
        slots.swap(0, 1);
        assert!(matches!(
            build_presentation(healthsmall(), &slots, NativeBasis::INITIAL),
            Err(CapturedModelError::ProjectionSlots)
        ));
        slots[0] = slots[1].clone();
        assert!(matches!(
            build_presentation(healthsmall(), &slots, NativeBasis::INITIAL),
            Err(CapturedModelError::ProjectionSlots)
        ));
    }

    #[test]
    fn depth_gate_and_inconsistent_typed_fields_are_rejected() {
        for depth in [63_u32, u32::MAX] {
            let mut slots = projections();
            slots[0].depth_word = depth;
            slots[0].raw[16..20].copy_from_slice(&depth.to_le_bytes());
            assert!(matches!(
                build_presentation(healthsmall(), &slots, NativeBasis::INITIAL),
                Err(CapturedModelError::InvalidProjection { slot: 2 })
            ));
        }
        let mut slots = projections();
        slots[0].packed_xy ^= 1;
        assert!(matches!(
            build_presentation(healthsmall(), &slots, NativeBasis::INITIAL),
            Err(CapturedModelError::InvalidProjection { slot: 2 })
        ));
    }

    #[test]
    fn uninitialized_face_reference_and_changed_producer_are_rejected() {
        let mut entry = healthsmall().clone();
        let ModelInstructionKind::TriangleFan(fan) = &mut entry.program.instructions[2].kind else {
            panic!("retail fan");
        };
        fan.vertices[0].vertex = ModelSlotRef::from_raw(3);
        assert!(matches!(
            build_presentation(&entry, &projections(), NativeBasis::INITIAL),
            Err(CapturedModelError::UninitializedFaceSlot { slot: 3 })
        ));
        let mut entry = healthsmall().clone();
        let ModelInstructionKind::NestedProgram(nested) = &mut entry.program.instructions[1].kind
        else {
            panic!("retail producer");
        };
        nested.arguments[4] = 0;
        assert!(matches!(
            build_presentation(&entry, &projections(), NativeBasis::INITIAL),
            Err(CapturedModelError::UnsupportedSource(_))
        ));
    }

    #[test]
    fn nonfinite_basis_and_zero_viewport_have_explicit_outcomes() {
        let mut basis = NativeBasis::INITIAL;
        basis.words[1] = f32::NAN.to_bits();
        assert!(matches!(
            build_presentation(healthsmall(), &projections(), basis),
            Err(CapturedModelError::NonFiniteBasis)
        ));
        let presentation =
            build_presentation(healthsmall(), &projections(), NativeBasis::INITIAL).unwrap();
        let fit = presentation.fit_screen([0, 0]);
        assert_eq!(fit.pixels_per_captured_pixel, 0.0);
        assert!(fit
            .vertices
            .iter()
            .all(|vertex| vertex.x == 0.0 && vertex.y == 0.0));
    }
}
