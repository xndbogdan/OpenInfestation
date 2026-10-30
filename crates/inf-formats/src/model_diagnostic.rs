//! Conservative Handler-8 geometry projection shared by diagnostic tools.
//!
//! The structural decoder intentionally preserves commands whose presentation
//! needs live game state.  This module performs the narrower analysis needed
//! by an asset workbench: it follows proven forward branches, evaluates static
//! vertex generators, and omits only polygon faces whose control flow or input
//! slots remain dynamic.  Unrelated animation registers, depth-order records,
//! edges, and beam/sprite effects do not make an otherwise static mesh vanish.

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

mod view_plane;
pub use view_plane::{
    ModelDiagnosticRequest, ModelDiagnosticView, ModelDiagnosticViewPlaneDecision,
};

use crate::{
    ModelEntry, ModelFaceFlags, ModelFanFlags, ModelFanVertexAttributes, ModelInstruction,
    ModelInstructionKind, ModelMaterialRef, ModelNestedKind, ModelNestedProgram, ModelProgram,
    ModelQuadLighting, ModelSlotRef, ModelTriangleLighting,
};

pub const MODEL_DIAGNOSTIC_BASIS: &str = "proven_static_command_projection";

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ModelDiagnosticProjection {
    Ready(ModelDiagnosticGeometry),
    Unavailable(ModelDiagnosticUnavailable),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiagnosticCompleteness {
    CompleteMesh,
    PartialMesh,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiagnosticOmissionKind {
    ConditionalFaces,
    UnresolvedFaceSlots,
    ModelInstances,
    NonMeshEffects,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDiagnosticOmission {
    pub kind: ModelDiagnosticOmissionKind,
    pub count: usize,
    pub opcodes: Vec<u8>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDiagnosticGeometry {
    /// Vertices referenced by the emitted diagnostic faces. A runtime slot can
    /// occur more than once when the command stream overwrites it between
    /// draws; `slot` preserves that provenance.
    pub vertices: Vec<ModelDiagnosticVertex>,
    /// Zero-based indices into `vertices`, preserving primitive winding.
    pub faces: Vec<Vec<usize>>,
    /// Presentation provenance parallel to `faces`. Values come from the
    /// archive command except for the proven `slot ^ 1` substitutions on an
    /// emitted mirror instance; resources and lighting remain unresolved.
    pub face_presentation: Vec<ModelDiagnosticFacePresentation>,
    /// Typed edge commands are not polygon faces and remain intentionally
    /// absent from this topology projection.
    pub ignored_edge_commands: usize,
    /// Complete means every polygon face in the analyzed root program was
    /// proven. This does not claim that non-mesh effects or edges are rendered.
    pub completeness: ModelDiagnosticCompleteness,
    /// Runtime inputs left unresolved by this analysis. Emitted local faces
    /// never depend on these values; even a branch with identical destinations
    /// retains its unknown comparison inputs. Complete polygon topology does
    /// not establish complete runtime execution of the asset.
    pub unresolved_dependencies: Vec<crate::ModelDynamicDependency>,
    pub omitted_effects: Vec<ModelDiagnosticOmission>,
    /// Executed diagnostic 0B decisions under an explicitly supplied eye.
    /// Empty for an unconditioned analysis; these are not captured retail events.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub view_plane_decisions: Vec<ModelDiagnosticViewPlaneDecision>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDiagnosticFacePresentation {
    pub source_opcode: u8,
    pub material: ModelMaterialRef,
    pub source_flags: ModelDiagnosticFaceFlags,
    /// Lighting slots used by this emitted face. Mirror instances contain the
    /// original handler's `slot ^ 1` substitutions.
    pub source_lighting: ModelDiagnosticFaceLighting,
    /// Fan records carry additional per-vertex words, including the recovered
    /// unsigned Q0.16 UV values for textured fans. Fixed triangle/quad commands
    /// leave this empty because their backend supplies implicit full-image UVs.
    pub fan_vertex_attributes: Option<Vec<ModelFanVertexAttributes>>,
    /// Zero-based triangle index within the source fan. This preserves the
    /// original bit-1 rule that gates the complete fan from its first
    /// triangle, even though the diagnostic topology stores fan triangles as
    /// independent faces.
    pub fan_triangle_index: Option<u16>,
    /// True for the additional face emitted by a source command's proven
    /// mirror-instance bit. Geometry and lighting references use the original
    /// handler's `slot ^ 1` substitution; material, flags, command order, and
    /// fan UV roles remain unchanged.
    pub mirrored_instance: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ModelDiagnosticFaceFlags {
    Face(ModelFaceFlags),
    Fan(ModelFanFlags),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", content = "refs", rename_all = "snake_case")]
pub enum ModelDiagnosticFaceLighting {
    Shared(ModelSlotRef),
    PerVertex(Vec<ModelSlotRef>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ModelDiagnosticVertex {
    pub slot: ModelSlotRef,
    /// Proven pre-projection geometry in root-model coordinates. This is not
    /// the native slot's world XYZ field, which can remain uninitialized.
    pub position: [i32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelDiagnosticUnavailableKind {
    /// The archive entry contains no data beyond an empty runtime allocation
    /// and the required command terminator.
    Empty,
    NoPolygonGeometry,
    RuntimeDependentGeometry,
    /// All omitted geometry consists of faces inside unprojectable nested scopes.
    /// Runtime slot writes that outlive such a scope do not put later root faces
    /// in this category.
    NestedProgram,
    ModelInstance,
    UnresolvedCommandEffects,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModelDiagnosticUnavailable {
    pub kind: ModelDiagnosticUnavailableKind,
    pub reason: String,
    pub referenced_face_slots: Vec<ModelSlotRef>,
    pub unresolved_dependencies: Vec<crate::ModelDynamicDependency>,
    pub omitted_effects: Vec<ModelDiagnosticOmission>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbstractValue {
    Known(u16),
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct StaticTransform {
    origin: [i32; 3],
    /// Local X/Y/Z axes expressed in the root model's coordinate space.
    basis: [[f32; 3]; 3],
}

impl StaticTransform {
    const IDENTITY: Self = Self {
        origin: [0; 3],
        basis: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
    };

    fn transform_vertex(self, vertex: [i32; 3]) -> [i32; 3] {
        std::array::from_fn(|axis| {
            let transformed = self.basis[0][axis] * vertex[0] as f32
                + self.basis[1][axis] * vertex[1] as f32
                + self.basis[2][axis] * vertex[2] as f32;
            self.origin[axis] + transformed.round_ties_even() as i32
        })
    }

    fn rotate_handler_05(&mut self, rotations: [u16; 3]) {
        // FUN_004ED9C0 applies FUN_00491530, FUN_00491290, then
        // FUN_004917D0. Each helper rotates a pair of the current basis
        // vectors in place; this is deliberately not an Euler shortcut.
        rotate_basis_pair(&mut self.basis, 0, 2, rotations[0]);
        rotate_basis_pair(&mut self.basis, 2, 1, rotations[1]);
        rotate_basis_pair(&mut self.basis, 1, 0, rotations[2]);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedFace {
    vertices: Vec<ModelDiagnosticVertex>,
    presentation: ModelDiagnosticFacePresentation,
}

#[derive(Default)]
struct OmissionAccumulator {
    entries: BTreeMap<ModelDiagnosticOmissionKind, (usize, BTreeSet<u8>)>,
}

impl OmissionAccumulator {
    fn add(&mut self, kind: ModelDiagnosticOmissionKind, opcode: u8, count: usize) {
        let entry = self.entries.entry(kind).or_default();
        entry.0 += count;
        entry.1.insert(opcode);
    }

    fn count(&self, kind: ModelDiagnosticOmissionKind) -> usize {
        self.entries.get(&kind).map_or(0, |entry| entry.0)
    }

    fn finish(self) -> Vec<ModelDiagnosticOmission> {
        self.entries
            .into_iter()
            .map(|(kind, (count, opcodes))| ModelDiagnosticOmission {
                kind,
                count,
                opcodes: opcodes.into_iter().collect(),
                reason: match kind {
                    ModelDiagnosticOmissionKind::ConditionalFaces => {
                        "polygon faces are guarded by unresolved runtime branches"
                    }
                    ModelDiagnosticOmissionKind::UnresolvedFaceSlots => {
                        "polygon faces reference runtime-dependent vertex slots"
                    }
                    ModelDiagnosticOmissionKind::ModelInstances => {
                        "child model instances require runtime transform state"
                    }
                    ModelDiagnosticOmissionKind::NonMeshEffects => {
                        "beam/sprite effects are outside the polygon topology diagnostic"
                    }
                }
                .to_owned(),
            })
            .collect()
    }
}

struct DiagnosticInterpreter<'a> {
    entry: &'a ModelEntry,
    view: ModelDiagnosticView<'a>,
    root_view_supported: bool,
    view_plane_decisions: Vec<ModelDiagnosticViewPlaneDecision>,
    /// Native slot +00/+04/+08. These may be unknown while a face is drawable.
    slots: Vec<Option<[i32; 3]>>,
    /// Proven 3D inputs to native slot +0C projection, in root-model space.
    /// These are geometry sources, not pixel coordinates or initialized world
    /// fields. World-only writes leave them unchanged.
    projection_sources: Vec<Option<[i32; 3]>>,
    registers: [AbstractValue; 64],
    resolved_faces: Vec<ResolvedFace>,
    conditional_face_candidates: Vec<ResolvedFace>,
    referenced_face_slots: BTreeSet<u16>,
    ignored_edge_commands: usize,
    omissions: OmissionAccumulator,
    unresolved_dependencies: BTreeSet<crate::ModelDynamicDependency>,
    unresolved_opcodes: BTreeSet<u8>,
    unresolved_nested_faces: usize,
    transform: StaticTransform,
}

pub fn analyze_model_diagnostic(request: ModelDiagnosticRequest<'_>) -> ModelDiagnosticProjection {
    let entry = request.entry;
    if is_terminator_only_empty(entry) {
        return unavailable(
            ModelDiagnosticUnavailableKind::Empty,
            "archive entry is intentionally empty (terminator-only command stream)",
            BTreeSet::new(),
            BTreeSet::new(),
            Vec::new(),
        );
    }

    let mut interpreter = DiagnosticInterpreter::new(request);
    interpreter.run();
    interpreter.finish()
}

impl<'a> DiagnosticInterpreter<'a> {
    fn new(request: ModelDiagnosticRequest<'a>) -> Self {
        let entry = request.entry;
        Self {
            entry,
            view: request.view,
            root_view_supported: view_plane::supports_root_identity(entry),
            view_plane_decisions: Vec::new(),
            slots: initial_slots(entry),
            projection_sources: initial_projection_sources(entry),
            registers: [AbstractValue::Unknown; 64],
            resolved_faces: Vec::new(),
            conditional_face_candidates: Vec::new(),
            referenced_face_slots: BTreeSet::new(),
            ignored_edge_commands: 0,
            omissions: OmissionAccumulator::default(),
            unresolved_dependencies: BTreeSet::new(),
            unresolved_opcodes: BTreeSet::new(),
            unresolved_nested_faces: 0,
            transform: StaticTransform::IDENTITY,
        }
    }

    fn run(&mut self) {
        let program = self.entry.program.clone();
        self.run_program(&program, false, false);
    }

    fn run_program(
        &mut self,
        program: &ModelProgram,
        inherited_conditional: bool,
        projection_blocked: bool,
    ) {
        let offsets = program
            .instructions
            .iter()
            .enumerate()
            .map(|(index, instruction)| (instruction.span.start, index))
            .chain(std::iter::once((
                program.terminator.start,
                program.instructions.len(),
            )))
            .collect::<BTreeMap<_, _>>();
        let mut conditional_until = Vec::new();
        let mut index = 0usize;
        while index < program.instructions.len() {
            let instruction = &program.instructions[index];
            conditional_until.retain(|target| *target > instruction.span.start);
            let conditional = inherited_conditional || !conditional_until.is_empty();
            let opcode = instruction.raw_command as u8;

            if matches!(opcode, 0x0b | 0x13 | 0x16) {
                let branch = self.branch_decision(instruction, conditional);
                let Some((target, target_index)) = branch_target(instruction, &offsets) else {
                    self.unresolved_opcodes.insert(opcode);
                    index += 1;
                    continue;
                };
                // 004A0C90 returns args + jump when the comparison selects the
                // branch, otherwise args + 3. A jump of three words therefore
                // reaches the next instruction for either comparison result.
                // branch_decision still records unknown input provenance;
                // this does not resolve callbacks or any enclosing condition.
                if opcode == 0x16 && target == instruction.span.end {
                    index += 1;
                    continue;
                }
                match branch {
                    BranchDecision::Jump if !conditional => {
                        index = target_index;
                        continue;
                    }
                    BranchDecision::FallThrough if !conditional => {}
                    BranchDecision::Unknown
                    | BranchDecision::Jump
                    | BranchDecision::FallThrough => {
                        if target > instruction.span.end {
                            conditional_until.push(target);
                        } else {
                            self.unresolved_opcodes.insert(opcode);
                        }
                    }
                }
                index += 1;
                continue;
            }

            match &instruction.kind {
                ModelInstructionKind::Triangle(triangle) => {
                    let vertices = triangle.vertices.map(ModelSlotRef::raw).to_vec();
                    let presentation = ModelDiagnosticFacePresentation {
                        source_opcode: opcode,
                        material: triangle.material,
                        source_flags: ModelDiagnosticFaceFlags::Face(triangle.flags),
                        source_lighting: triangle_lighting(&triangle.lighting),
                        fan_vertex_attributes: None,
                        fan_triangle_index: None,
                        mirrored_instance: false,
                    };
                    self.emit_face(
                        &vertices,
                        presentation.clone(),
                        conditional,
                        projection_blocked,
                    );
                    if triangle.flags.emits_mirror_instance() {
                        self.emit_face(
                            &mirrored_face(&vertices),
                            mirrored_presentation(presentation),
                            conditional,
                            projection_blocked,
                        );
                    }
                }
                ModelInstructionKind::Quad(quad) => {
                    let vertices = quad.vertices.map(ModelSlotRef::raw).to_vec();
                    let presentation = ModelDiagnosticFacePresentation {
                        source_opcode: opcode,
                        material: quad.material,
                        source_flags: ModelDiagnosticFaceFlags::Face(quad.flags),
                        source_lighting: quad_lighting(&quad.lighting),
                        fan_vertex_attributes: None,
                        fan_triangle_index: None,
                        mirrored_instance: false,
                    };
                    self.emit_face(
                        &vertices,
                        presentation.clone(),
                        conditional,
                        projection_blocked,
                    );
                    if quad.flags.emits_mirror_instance() {
                        self.emit_face(
                            &mirrored_face(&vertices),
                            mirrored_presentation(presentation),
                            conditional,
                            projection_blocked,
                        );
                    }
                }
                ModelInstructionKind::TriangleFan(fan) => {
                    for (triangle_index, pair) in fan.vertices[1..].windows(2).enumerate() {
                        let source_vertices = [&fan.vertices[0], &pair[0], &pair[1]];
                        let triangle = source_vertices.map(|vertex| vertex.vertex.raw()).to_vec();
                        let presentation = ModelDiagnosticFacePresentation {
                            source_opcode: opcode,
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
                            fan_triangle_index: u16::try_from(triangle_index).ok(),
                            mirrored_instance: false,
                        };
                        self.emit_face(
                            &triangle,
                            presentation.clone(),
                            conditional,
                            projection_blocked,
                        );
                        if fan.flags.emits_mirror_instance() {
                            self.emit_face(
                                &mirrored_face(&triangle),
                                mirrored_presentation(presentation),
                                conditional,
                                projection_blocked,
                            );
                        }
                    }
                }
                ModelInstructionKind::Edge(_) => self.ignored_edge_commands += 1,
                ModelInstructionKind::RegisterOperation(command) => {
                    self.execute_register(opcode, &command.arguments, conditional)
                }
                ModelInstructionKind::UnresolvedDynamic(command) => {
                    self.execute_vertex_operation(
                        instruction.raw_command,
                        &command.arguments,
                        conditional || projection_blocked,
                    );
                    if !is_handled_dynamic_opcode(opcode) {
                        self.unresolved_dependencies
                            .extend(command.dependencies.iter().copied());
                    }
                }
                ModelInstructionKind::Opaque(command) => {
                    if (0x18..=0x1b).contains(&opcode) {
                        self.omissions
                            .add(ModelDiagnosticOmissionKind::NonMeshEffects, opcode, 1);
                    } else if is_vertex_opcode(opcode) {
                        self.execute_vertex_operation(
                            instruction.raw_command,
                            &command.arguments,
                            conditional || projection_blocked,
                        );
                    } else if !is_topology_neutral_opcode(opcode) {
                        self.unresolved_opcodes.insert(opcode);
                    }
                }
                ModelInstructionKind::SentinelList(_) => {
                    if opcode == 0x06 {
                        self.unresolved_opcodes.insert(opcode);
                    }
                }
                ModelInstructionKind::VariableWords(_) => {}
                ModelInstructionKind::NestedProgram(nested) => {
                    self.execute_nested_program(opcode, nested, conditional, projection_blocked);
                }
                ModelInstructionKind::ModelInstance(instance) => {
                    // Opcode 0x0E recursively renders a child model. Its
                    // orientation can use the saved basis assembled by opcode
                    // 0x03, so inventing a static child pose would be
                    // misleading. Keep the independently proven parent faces
                    // instead of discarding the whole entry, and record the
                    // missing child instance explicitly.
                    self.omissions
                        .add(ModelDiagnosticOmissionKind::ModelInstances, opcode, 1);
                    self.unresolved_dependencies
                        .extend(instance.dependencies.iter().copied());
                }
            }
            index += 1;
        }
    }

    fn branch_decision(
        &mut self,
        instruction: &ModelInstruction,
        conditional: bool,
    ) -> BranchDecision {
        let opcode = instruction.raw_command as u8;
        let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
            return BranchDecision::Unknown;
        };
        if !conditional && opcode == 0x0b && self.root_view_supported {
            if let Some(decision) =
                view_plane::evaluate(self.entry, instruction, self.view, &self.slots)
            {
                let branch = if decision.jump_taken {
                    BranchDecision::Jump
                } else {
                    BranchDecision::FallThrough
                };
                self.view_plane_decisions.push(decision);
                return branch;
            }
        }
        if conditional || opcode != 0x16 {
            self.unresolved_dependencies
                .extend(command.dependencies.iter().copied());
            return BranchDecision::Unknown;
        }
        let [_, left, right] = command.arguments.as_slice() else {
            return BranchDecision::Unknown;
        };
        match (self.packed_value(*left), self.packed_value(*right)) {
            (AbstractValue::Known(left), AbstractValue::Known(right)) => {
                let comparison = u16::from(left != right);
                if comparison != instruction.raw_command >> 8 {
                    BranchDecision::Jump
                } else {
                    BranchDecision::FallThrough
                }
            }
            _ => {
                self.unresolved_dependencies
                    .extend(command.dependencies.iter().copied());
                BranchDecision::Unknown
            }
        }
    }

    fn emit_face(
        &mut self,
        slots: &[u16],
        presentation: ModelDiagnosticFacePresentation,
        conditional: bool,
        projection_blocked: bool,
    ) {
        self.referenced_face_slots.extend(slots.iter().copied());
        if projection_blocked {
            self.unresolved_nested_faces += 1;
            self.omissions.add(
                ModelDiagnosticOmissionKind::UnresolvedFaceSlots,
                presentation.source_opcode,
                1,
            );
            return;
        }
        if conditional {
            self.omissions.add(
                ModelDiagnosticOmissionKind::ConditionalFaces,
                presentation.source_opcode,
                1,
            );
            if let Some(vertices) = self.resolve_face_vertices(slots) {
                self.conditional_face_candidates.push(ResolvedFace {
                    vertices,
                    presentation,
                });
            }
            return;
        }
        if let Some(vertices) = self.resolve_face_vertices(slots) {
            self.resolved_faces.push(ResolvedFace {
                vertices,
                presentation,
            });
        } else {
            self.omissions.add(
                ModelDiagnosticOmissionKind::UnresolvedFaceSlots,
                presentation.source_opcode,
                1,
            );
        }
    }

    fn resolve_face_vertices(&self, slots: &[u16]) -> Option<Vec<ModelDiagnosticVertex>> {
        slots
            .iter()
            .map(|slot| {
                self.projection_sources
                    .get(usize::from(*slot))
                    .and_then(|position| *position)
                    .map(|position| ModelDiagnosticVertex {
                        slot: ModelSlotRef::from_raw(*slot),
                        position,
                    })
            })
            .collect()
    }

    fn execute_nested_program(
        &mut self,
        opcode: u8,
        nested: &ModelNestedProgram,
        conditional: bool,
        projection_blocked: bool,
    ) {
        if opcode != 0x05 || nested.kind != ModelNestedKind::Transform {
            // FUN_004EDB10 constructs an oriented basis from two live slot
            // vectors through FUN_00490370. Its handedness/mirror policy is
            // retained structurally, but is not yet static diagnostic math.
            self.unresolved_dependencies
                .insert(crate::ModelDynamicDependency::TransformState);
            self.invalidate_nested_range(&nested.arguments, 4);
            self.run_program(&nested.program, conditional, true);
            return;
        }

        let Ok(arguments) = <&[u16; 12]>::try_from(nested.arguments.as_slice()) else {
            self.unresolved_opcodes.insert(opcode);
            self.invalidate_all_slots();
            self.run_program(&nested.program, conditional, true);
            return;
        };
        let [origin_slot, rotate_15, rotate_12, rotate_17, projection_start, static_start, mirrored_start, static_end, projection_end, _, _, _] =
            arguments;

        let range = valid_static_range(
            *static_start,
            *mirrored_start,
            *static_end,
            self.slots.len(),
        );
        let range_has_raw_vertices =
            range.is_some_and(|range| usize::from(range.end / 2) <= self.entry.vertices.len());
        // Render handler 004A1150 calls 004EBFE0 with args[4/6/8],
        // independently of the world initializer's args[5/6/7].
        let projection_range = valid_static_range(
            *projection_start,
            *mirrored_start,
            *projection_end,
            self.projection_sources.len(),
        );
        let projection_has_raw_vertices = projection_range
            .is_some_and(|range| usize::from(range.end / 2) <= self.entry.vertices.len());
        let origin = self
            .slots
            .get(usize::from(*origin_slot))
            .and_then(|position| *position);
        let rotations = [
            self.packed_value(*rotate_15),
            self.packed_value(*rotate_12),
            self.packed_value(*rotate_17),
        ];
        let known_rotations = rotations.map(|value| match value {
            AbstractValue::Known(value) => Some(value),
            AbstractValue::Unknown => None,
        });

        let can_project = !conditional
            && !projection_blocked
            && origin.is_some()
            && known_rotations.iter().all(Option::is_some)
            && range.is_some()
            && range_has_raw_vertices
            && projection_range.is_some()
            && projection_has_raw_vertices;
        if !can_project {
            if origin.is_none()
                || range.is_none()
                || !range_has_raw_vertices
                || projection_range.is_none()
                || !projection_has_raw_vertices
                || projection_blocked
            {
                self.unresolved_dependencies
                    .insert(crate::ModelDynamicDependency::TransformState);
            }
            for (&raw, resolved) in [*rotate_15, *rotate_12, *rotate_17]
                .iter()
                .zip(known_rotations)
            {
                if resolved.is_none() {
                    self.record_packed_value_dependency(raw);
                }
            }
            if let Some(range) = range {
                self.invalidate_slot_range(range);
            } else {
                // A signed/out-of-bounds range prevents us from bounding the
                // runtime writes. Invalidating every slot is conservative and
                // avoids carrying stale parent coordinates through the scope.
                self.invalidate_all_slots();
            }
            if let Some(range) = projection_range {
                for slot in range.start..range.end {
                    self.projection_sources[usize::from(slot)] = None;
                }
            } else {
                self.projection_sources.fill(None);
            }
            self.run_program(&nested.program, conditional, true);
            return;
        }

        let saved_transform = self.transform;
        self.transform.origin = origin.expect("can_project requires a static origin");
        self.transform.rotate_handler_05([
            known_rotations[0].expect("can_project requires static rotation"),
            known_rotations[1].expect("can_project requires static rotation"),
            known_rotations[2].expect("can_project requires static rotation"),
        ]);
        self.initialize_static_range(range.expect("can_project requires a valid range"));
        initialize_geometry_range(
            self.entry,
            self.transform,
            projection_range.expect("can_project requires a valid projection range"),
            &mut self.projection_sources,
        );
        self.run_program(&nested.program, false, false);
        // FUN_004ED9C0 restores only the origin and 3x3 basis. Vertex slots
        // and packed-value registers written by the nested stream persist.
        self.transform = saved_transform;
    }

    fn initialize_static_range(&mut self, range: StaticVertexRange) {
        initialize_geometry_range(self.entry, self.transform, range, &mut self.slots);
    }

    fn invalidate_nested_range(&mut self, arguments: &[u16], range_start: usize) {
        // Unrecovered 09 projection writes must not leave stale geometry after
        // its scope. Its world range can still be bounded independently.
        self.projection_sources.fill(None);
        let range = arguments
            .get(range_start..range_start + 3)
            .and_then(|range| valid_static_range(range[0], range[1], range[2], self.slots.len()));
        if let Some(range) = range {
            self.invalidate_slot_range(range);
        } else {
            self.invalidate_all_slots();
        }
    }

    fn invalidate_slot_range(&mut self, range: StaticVertexRange) {
        for slot in range.start..range.end {
            self.slots[usize::from(slot)] = None;
        }
    }

    fn invalidate_all_slots(&mut self) {
        self.slots.fill(None);
        self.projection_sources.fill(None);
    }

    fn record_packed_value_dependency(&mut self, raw: u16) {
        let dependency = if raw & 0x80 == 0 {
            return;
        } else if raw & 0x40 != 0 {
            crate::ModelDynamicDependency::RegisterFile
        } else {
            crate::ModelDynamicDependency::PackedValueCallback
        };
        self.unresolved_dependencies.insert(dependency);
    }

    fn execute_register(&mut self, opcode: u8, arguments: &[u16], conditional: bool) {
        let [destination, left, right] = arguments else {
            self.unresolved_opcodes.insert(opcode);
            return;
        };
        let destination = usize::from(*destination);
        if destination >= self.registers.len() {
            self.unresolved_opcodes.insert(opcode);
            return;
        }
        if conditional {
            self.registers[destination] = AbstractValue::Unknown;
            return;
        }
        let left = self.packed_value(*left);
        let right = self.packed_value(*right);
        let result = evaluate_register_operation(opcode, left, right);
        self.registers[destination] = result;
        if matches!(result, AbstractValue::Unknown) {
            self.unresolved_dependencies
                .extend(program_dependencies_for_instruction(opcode));
        }
    }

    fn packed_value(&self, raw: u16) -> AbstractValue {
        if raw & 0x80 == 0 {
            let rotation = u32::from((raw >> 2) & 0xf);
            let mut value = (raw & 0xff03).rotate_left(rotation);
            if raw & 0x40 != 0 {
                value = !value;
            }
            AbstractValue::Known(value)
        } else if raw & 0x40 != 0 {
            self.registers[usize::from(raw & 0x3f)]
        } else {
            AbstractValue::Unknown
        }
    }

    fn execute_vertex_operation(&mut self, raw_command: u16, arguments: &[u16], conditional: bool) {
        let opcode = raw_command as u8;
        let Some(&destination) = arguments.first() else {
            self.unresolved_opcodes.insert(opcode);
            return;
        };
        if opcode == 0x2a {
            self.invalidate_parent_import(raw_command, destination);
            return;
        }
        let mut mode = (raw_command >> 8) & 0xf;
        for parity in 0..2u16 {
            let pair = mode & 3;
            mode >>= 2;
            if pair == 0 {
                continue;
            }
            let Some(target) = destination.checked_add(parity) else {
                self.unresolved_opcodes.insert(opcode);
                continue;
            };
            let value = if conditional {
                None
            } else {
                self.evaluate_vertex(opcode, arguments, parity != 0)
            };
            let target = usize::from(target);
            if target >= self.slots.len() {
                self.unresolved_opcodes.insert(opcode);
                continue;
            }
            if pair & 1 != 0 {
                self.slots[target] = value;
            }
            if pair & 2 != 0 {
                self.projection_sources[target] = value;
            }
        }
    }

    fn invalidate_parent_import(&mut self, command: u16, destination: u16) {
        // 004ED350 is not an ordinary generated-coordinate writer. With a
        // parent, either bit in a mode pair copies the entire 24-byte slot.
        // Without a parent, mirror-only flags first produce the corresponding
        // original origin field and then copy it. Parent custody is unknown in
        // this isolated analysis, so invalidate the union of both write sets.
        let mode = (command >> 8) & 15;
        if mode == 0 {
            return;
        }
        self.unresolved_dependencies
            .insert(crate::ModelDynamicDependency::ParentVertexSlots);
        let target = usize::from(destination);
        if target >= self.slots.len() {
            self.unresolved_opcodes.insert(0x2a);
            return;
        }
        if mode & 3 != 0 || mode & 5 != 0 {
            self.slots[target] = None;
        }
        if mode & 3 != 0 || mode & 10 != 0 {
            self.projection_sources[target] = None;
        }
        if mode & 12 != 0 {
            if target + 1 < self.slots.len() {
                self.slots[target + 1] = None;
                self.projection_sources[target + 1] = None;
            } else {
                self.unresolved_opcodes.insert(0x2a);
            }
        }
    }

    fn evaluate_vertex(
        &mut self,
        opcode: u8,
        arguments: &[u16],
        mirrored: bool,
    ) -> Option<[i32; 3]> {
        let source = |this: &Self, raw: u16| {
            let raw = if mirrored { raw ^ 1 } else { raw };
            this.slots.get(usize::from(raw)).and_then(|value| *value)
        };
        match opcode {
            0x20 => raw_model_vertex(self.entry, arguments[0], mirrored)
                .map(|vertex| self.transform.transform_vertex(vertex)),
            0x21 => source(self, arguments[1]).map(|point| {
                std::array::from_fn(|axis| {
                    self.transform.origin[axis]
                        .wrapping_mul(2)
                        .wrapping_sub(point[axis])
                })
            }),
            0x23 => {
                let left = source(self, arguments[1])?;
                let right = source(self, arguments[2])?;
                Some(std::array::from_fn(|axis| {
                    left[axis].wrapping_add(right[axis]) / 2
                }))
            }
            0x24 => {
                let a = source(self, arguments[1])?;
                let b = source(self, arguments[2])?;
                let c = source(self, arguments[3])?;
                Some(std::array::from_fn(|axis| {
                    a[axis].wrapping_sub(c[axis]).wrapping_add(b[axis])
                }))
            }
            0x25 => {
                let left = source(self, arguments[1])?;
                let right = source(self, arguments[2])?;
                Some(std::array::from_fn(|axis| {
                    left[axis]
                        .wrapping_add(right[axis])
                        .wrapping_sub(self.transform.origin[axis])
                }))
            }
            0x26 => {
                let start = source(self, arguments[1])?;
                let end = source(self, arguments[2])?;
                let AbstractValue::Known(factor) = self.packed_value(arguments[3]) else {
                    self.record_packed_value_dependency(arguments[3]);
                    return None;
                };
                Some(std::array::from_fn(|axis| {
                    // 004EC950 retains even steps after signed IMUL/SHL/RCL,
                    // matching the native runtime vertex owner.
                    let delta = end[axis].wrapping_sub(start[axis]);
                    let product = i64::from(delta) * i64::from(i32::from(factor) << 14);
                    start[axis].wrapping_add(((product >> 31) as i32).wrapping_mul(2))
                }))
            }
            0x22 => {
                self.unresolved_dependencies
                    .insert(crate::ModelDynamicDependency::RandomSource);
                None
            }
            0x2b | 0x2c => {
                self.unresolved_dependencies
                    .insert(crate::ModelDynamicDependency::RendererProjection);
                None
            }
            0x2d => {
                self.unresolved_dependencies
                    .insert(crate::ModelDynamicDependency::CustomVertexCallback);
                None
            }
            0x27..=0x29 => {
                self.unresolved_dependencies
                    .insert(crate::ModelDynamicDependency::PackedValueCallback);
                None
            }
            _ => {
                self.unresolved_opcodes.insert(opcode);
                None
            }
        }
    }

    fn finish(mut self) -> ModelDiagnosticProjection {
        // A runtime branch may repeat the exact same statically positioned
        // source face in multiple alternatives. Retain one representative in
        // that narrow case; unique alternatives remain omitted, so the
        // diagnostic never unions mutually exclusive animation geometry.
        let mut repeated_representatives = Vec::new();
        for (index, candidate) in self.conditional_face_candidates.iter().enumerate() {
            let repeated = self.conditional_face_candidates[index + 1..]
                .iter()
                .any(|other| other == candidate);
            if repeated && !repeated_representatives.contains(candidate) {
                repeated_representatives.push(candidate.clone());
            }
        }
        self.resolved_faces.extend(repeated_representatives);
        let omitted_face_count = self
            .omissions
            .count(ModelDiagnosticOmissionKind::ConditionalFaces)
            + self
                .omissions
                .count(ModelDiagnosticOmissionKind::UnresolvedFaceSlots);
        let omitted_instance_count = self
            .omissions
            .count(ModelDiagnosticOmissionKind::ModelInstances);
        let omitted_geometry_count = omitted_face_count + omitted_instance_count;
        let omissions = self.omissions.finish();
        if !self.unresolved_opcodes.is_empty() {
            return unavailable(
                ModelDiagnosticUnavailableKind::UnresolvedCommandEffects,
                format!(
                    "program includes commands with unresolved topology effects: {}",
                    self.unresolved_opcodes
                        .iter()
                        .map(|opcode| format!("0x{opcode:02X}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                self.referenced_face_slots,
                self.unresolved_dependencies,
                omissions,
            );
        }
        if self.resolved_faces.is_empty() {
            let kind = if omitted_instance_count != 0 && omitted_face_count == 0 {
                ModelDiagnosticUnavailableKind::ModelInstance
            } else if self.unresolved_nested_faces != 0
                && self.unresolved_nested_faces == omitted_geometry_count
            {
                ModelDiagnosticUnavailableKind::NestedProgram
            } else if omitted_geometry_count == 0 {
                ModelDiagnosticUnavailableKind::NoPolygonGeometry
            } else {
                ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
            };
            let reason = if kind == ModelDiagnosticUnavailableKind::ModelInstance {
                "entry contains only child model geometry requiring the external catalog and runtime transform"
            } else if kind == ModelDiagnosticUnavailableKind::NestedProgram {
                "entry contains only nested polygon geometry whose transform is not statically proven"
            } else if omitted_geometry_count == 0 {
                "entry contains no typed triangle, quad, or fan primitives"
            } else {
                "entry has no polygon faces whose control flow and vertex slots are statically proven"
            };
            return unavailable(
                kind,
                reason,
                self.referenced_face_slots,
                self.unresolved_dependencies,
                omissions,
            );
        }

        let mut vertices = self
            .resolved_faces
            .iter()
            .flat_map(|face| face.vertices.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        vertices.sort_by_key(|vertex| (vertex.slot.raw(), vertex.position));
        let indices = vertices
            .iter()
            .enumerate()
            .map(|(index, vertex)| (*vertex, index))
            .collect::<BTreeMap<_, _>>();
        let (faces, face_presentation) = self
            .resolved_faces
            .into_iter()
            .map(|face| {
                let indices = face
                    .vertices
                    .into_iter()
                    .map(|vertex| indices[&vertex])
                    .collect();
                (indices, face.presentation)
            })
            .unzip();
        ModelDiagnosticProjection::Ready(ModelDiagnosticGeometry {
            vertices,
            faces,
            face_presentation,
            ignored_edge_commands: self.ignored_edge_commands,
            completeness: if omitted_geometry_count == 0 {
                ModelDiagnosticCompleteness::CompleteMesh
            } else {
                ModelDiagnosticCompleteness::PartialMesh
            },
            unresolved_dependencies: self.unresolved_dependencies.into_iter().collect(),
            omitted_effects: omissions,
            view_plane_decisions: self.view_plane_decisions,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BranchDecision {
    Jump,
    FallThrough,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct StaticVertexRange {
    start: u16,
    mirrored_start: u16,
    end: u16,
}

fn valid_static_range(
    start: u16,
    mirrored_start: u16,
    end: u16,
    slot_count: usize,
) -> Option<StaticVertexRange> {
    let in_bounds = usize::from(end) <= slot_count;
    let ordered = start <= mirrored_start && mirrored_start <= end;
    if !in_bounds || !ordered {
        return None;
    }
    let aligned = (mirrored_start - start) % 2 == 0 && (end - mirrored_start) % 2 == 0;
    (in_bounds && ordered && aligned).then_some(StaticVertexRange {
        start,
        mirrored_start,
        end,
    })
}

fn rotate_basis_pair(basis: &mut [[f32; 3]; 3], left: usize, right: usize, angle: u16) {
    let sine = retail_sine(angle);
    let cosine = retail_sine(angle.wrapping_add(0x4000));
    let old_left = basis[left];
    let old_right = basis[right];
    basis[left] = std::array::from_fn(|axis| cosine * old_left[axis] - sine * old_right[axis]);
    basis[right] = std::array::from_fn(|axis| cosine * old_right[axis] + sine * old_left[axis]);
}

fn retail_sine(angle: u16) -> f32 {
    // Diagnostic approximation of the quadrant lookup in 00491290/00491530/
    // 004917D0. Production must consume the original 0053DF00 table and scale
    // by f32 bits 0x38000100 (~1/32767), preserving x87 intermediates. This
    // reconstructed /32768 curve is not the native basis math; see the concrete
    // inf-game menu_basis owner and FRONTEND_RUNTIME.md before reusing it.
    let mut quarter_angle = angle & 0x3ffc;
    if angle & 0x4000 != 0 {
        quarter_angle = 0x3ffc - quarter_angle;
    }
    let radians = f64::from(quarter_angle) * std::f64::consts::PI / 32768.0;
    let magnitude = (radians.sin() * 32768.0).floor() as i32;
    let signed = if angle & 0x8000 != 0 {
        -magnitude
    } else {
        magnitude
    };
    signed as f32 / 32768.0
}

fn branch_target(
    instruction: &ModelInstruction,
    offsets: &BTreeMap<usize, usize>,
) -> Option<(usize, usize)> {
    let ModelInstructionKind::UnresolvedDynamic(command) = &instruction.kind else {
        return None;
    };
    // 004A0D78 sign-extends the WORD displacement from the argument pointer.
    // This bounded analyzer follows only forward control flow, never loops.
    let jump_words = isize::from(*command.arguments.first()? as i16);
    let jump_bytes = jump_words.checked_mul(2)?.checked_add(2)?;
    let target = instruction.span.start.checked_add_signed(jump_bytes)?;
    if target <= instruction.span.start {
        return None;
    }
    offsets.get(&target).copied().map(|index| (target, index))
}

fn evaluate_register_operation(
    opcode: u8,
    left: AbstractValue,
    right: AbstractValue,
) -> AbstractValue {
    let (AbstractValue::Known(left), AbstractValue::Known(right)) = (left, right) else {
        return AbstractValue::Unknown;
    };
    let value = match opcode {
        0xf0 => left.wrapping_add(right),
        0xf1 => left.wrapping_sub(right),
        0xf2 => left.wrapping_mul(right),
        0xf3 => {
            if right == 0 {
                left
            } else {
                left / right
            }
        }
        0xf4 => ((u32::from(left) >> (right & 0x1f)) & 0xffff) as u16,
        // 004A03A0 shifts a zero-extended DWORD with the x86 five-bit
        // count, then stores AX. A u16 shift would mask by fifteen again.
        0xf5 => u32::from(left).wrapping_shl(u32::from(right & 0x1f)) as u16,
        0xf6 => left.max(right),
        0xf7 => {
            if left < right {
                left
            } else {
                right.wrapping_sub(1)
            }
        }
        0xf8 => {
            if right == 0 {
                left
            } else {
                ((u32::from(left) << 16) / u32::from(right)) as u16
            }
        }
        0xf9 => ((i32::from(left as i16) >> (right & 0x1f)) as i16) as u16,
        0xfb => {
            if right <= left {
                left
            } else {
                0
            }
        }
        0xfc => {
            if left <= right {
                left
            } else {
                0
            }
        }
        0xff => left & right,
        0xfa | 0xfd | 0xfe => return AbstractValue::Unknown,
        _ => return AbstractValue::Unknown,
    };
    AbstractValue::Known(value)
}

fn initial_slots(entry: &ModelEntry) -> Vec<Option<[i32; 3]>> {
    (0..entry.header.vertex_slot_count)
        .map(|slot| resolve_initial_slot(entry, slot).ok())
        .collect()
}

fn initial_projection_sources(entry: &ModelEntry) -> Vec<Option<[i32; 3]>> {
    let mut sources = vec![None; usize::from(entry.header.vertex_slot_count)];
    let [_, mirrored_start, _, end] = entry.header.vertex_ranges;
    // 004A00E0 -> 004EBFE0 uses header +10/+14/+18. Unlike the
    // +12/+14/+16 world range, this starts at reserved_10 (zero in retail).
    if let Some(range) =
        valid_static_range(entry.header.reserved_10, mirrored_start, end, sources.len())
    {
        initialize_geometry_range(entry, StaticTransform::IDENTITY, range, &mut sources);
    }
    sources
}

fn initialize_geometry_range(
    entry: &ModelEntry,
    transform: StaticTransform,
    range: StaticVertexRange,
    slots: &mut [Option<[i32; 3]>],
) {
    for slot in (range.start..range.end).step_by(2) {
        slots[usize::from(slot)] =
            raw_model_vertex(entry, slot, false).map(|vertex| transform.transform_vertex(vertex));
        if slot >= range.mirrored_start {
            slots[usize::from(slot + 1)] = raw_model_vertex(entry, slot, true)
                .map(|vertex| transform.transform_vertex(vertex));
        }
    }
}

fn resolve_initial_slot(entry: &ModelEntry, slot: u16) -> Result<[i32; 3], String> {
    // FUN_004EC0A0 consumes raw vertex `slot / 2`, initializes even slots in
    // [vertex_ranges[0], vertex_ranges[1]), then initializes mirrored pairs in
    // [vertex_ranges[1], vertex_ranges[2]). The fourth header word is not an
    // argument to this initialization call. Slots outside the first three
    // boundaries remain runtime data.
    let [static_start, mirrored_start, static_end, _reserved_end] = entry.header.vertex_ranges;
    if slot < static_start || slot >= static_end {
        return Err(format!(
            "slot is outside the proven static range [{static_start}, {static_end})"
        ));
    }
    if slot < mirrored_start && slot % 2 != 0 {
        return Err(format!(
            "odd slot is not initialized before mirrored range {mirrored_start}"
        ));
    }
    raw_model_vertex(entry, slot, slot >= mirrored_start && slot % 2 != 0)
        .ok_or_else(|| "raw vertex is outside the entry pool".to_owned())
}

fn raw_model_vertex(entry: &ModelEntry, slot: u16, mirrored: bool) -> Option<[i32; 3]> {
    let raw = entry.vertices.get(usize::from(slot / 2)).copied()?;
    let x = i32::from(raw.x);
    Some([
        if mirrored { -x } else { x },
        i32::from(raw.y),
        i32::from(raw.z),
    ])
}

fn mirrored_face(face: &[u16]) -> Vec<u16> {
    // The original face handlers keep command order and substitute slot^1;
    // their mirror-pass cull predicate accounts for the reflected winding.
    face.iter()
        .map(|slot| ModelSlotRef::from_raw(*slot).mirrored().raw())
        .collect()
}

fn mirrored_presentation(
    mut presentation: ModelDiagnosticFacePresentation,
) -> ModelDiagnosticFacePresentation {
    match &mut presentation.source_lighting {
        ModelDiagnosticFaceLighting::Shared(slot) => *slot = slot.mirrored(),
        ModelDiagnosticFaceLighting::PerVertex(slots) => {
            for slot in slots {
                *slot = slot.mirrored();
            }
        }
    }
    presentation.mirrored_instance = true;
    presentation
}

fn triangle_lighting(lighting: &ModelTriangleLighting) -> ModelDiagnosticFaceLighting {
    match lighting {
        ModelTriangleLighting::Shared(slot) => ModelDiagnosticFaceLighting::Shared(*slot),
        ModelTriangleLighting::PerVertex(slots) => {
            ModelDiagnosticFaceLighting::PerVertex(slots.to_vec())
        }
    }
}

fn quad_lighting(lighting: &ModelQuadLighting) -> ModelDiagnosticFaceLighting {
    match lighting {
        ModelQuadLighting::Shared(slot) => ModelDiagnosticFaceLighting::Shared(*slot),
        ModelQuadLighting::PerVertex(slots) => {
            ModelDiagnosticFaceLighting::PerVertex(slots.to_vec())
        }
    }
}

fn is_vertex_opcode(opcode: u8) -> bool {
    (0x20..=0x2d).contains(&opcode)
}

fn is_handled_dynamic_opcode(opcode: u8) -> bool {
    matches!(opcode, 0x16 | 0x22 | 0x2a..=0x2d)
}

fn is_topology_neutral_opcode(opcode: u8) -> bool {
    matches!(
        opcode,
        0x02..=0x04 | 0x08 | 0x0f | 0x10 | 0x15 | 0x1c | 0xea..=0xef
    )
}

fn is_terminator_only_empty(entry: &ModelEntry) -> bool {
    entry.header.raw_vertex_count == 0
        && entry.header.vertex_slot_count == 0
        && entry.auxiliary.is_empty()
        && entry.program.instructions.is_empty()
}

fn program_dependencies_for_instruction(
    opcode: u8,
) -> impl Iterator<Item = crate::ModelDynamicDependency> {
    let mut dependencies = Vec::new();
    if (0xf0..=0xff).contains(&opcode) {
        dependencies.push(crate::ModelDynamicDependency::RegisterFile);
        dependencies.push(crate::ModelDynamicDependency::PackedValueCallback);
    }
    dependencies.into_iter()
}

fn unavailable(
    kind: ModelDiagnosticUnavailableKind,
    reason: impl Into<String>,
    slots: BTreeSet<u16>,
    dependencies: BTreeSet<crate::ModelDynamicDependency>,
    omitted_effects: Vec<ModelDiagnosticOmission>,
) -> ModelDiagnosticProjection {
    ModelDiagnosticProjection::Unavailable(ModelDiagnosticUnavailable {
        kind,
        reason: reason.into(),
        referenced_face_slots: slots.into_iter().map(ModelSlotRef::from_raw).collect(),
        unresolved_dependencies: dependencies.into_iter().collect(),
        omitted_effects,
    })
}
