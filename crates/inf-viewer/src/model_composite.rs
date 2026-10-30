//! Conservative model-instance composition for the asset workbench.
//!
//! Handler-8 opcode `0x0E` can attach recursively rendered child models to a
//! parent slot. Packed callback values are runtime state, so this module only
//! assembles a deliberately labelled neutral pose: callback-backed joint inputs
//! are substituted with zero, including register expressions derived from
//! those inputs. Each emitted attachment slot and fixed/saved base orientation
//! must be statically proven; guarded instances, unknown register expressions,
//! and incomplete instance chains remain explicit omissions in the shared
//! diagnostic geometry.

use std::ops::Range;

use inf_formats::{
    ModelDiagnosticCompleteness, ModelDiagnosticGeometry, ModelDiagnosticOmissionKind,
    ModelDiagnosticProjection, ModelDiagnosticUnavailableKind, ModelDynamicDependency,
    ModelInstructionKind, ModelOrientation,
};

use crate::catalog::{AssetCatalog, ModelVariant};
use inf_formats::native_basis::{NativeBasis, NativeBasisMath, RotationPlane};

#[derive(Clone, Debug)]
pub(crate) struct NeutralModelComposite {
    pub geometry: ModelDiagnosticGeometry,
    pub child_instances: usize,
}

struct NeutralCompositeBuilder<'a> {
    catalog: &'a AssetCatalog,
    source: &'a str,
    composite: NeutralModelComposite,
    stack: Vec<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ModelTransform {
    origin: [i32; 3],
    basis: NativeBasis,
}

impl ModelTransform {
    const IDENTITY: Self = Self {
        origin: [0; 3],
        basis: NativeBasis::INITIAL,
    };

    fn apply(self, point: [i32; 3]) -> [i32; 3] {
        let rotated = self.basis.transform_point(point);
        std::array::from_fn(|axis| self.origin[axis].wrapping_add(rotated[axis]))
    }
}

/// Each invocation starts with the passed current basis and fresh saved identity.
/// Unknown vectors and unknown normalization counters are deliberately separate:
/// a later 03 can recover vectors without resetting the saved counter.
struct NeutralBasisState {
    current: Option<NativeBasis>,
    saved: Option<NativeBasis>,
    saved_counter: Option<u32>,
}

impl NeutralBasisState {
    fn new(current: NativeBasis) -> Self {
        Self {
            current: Some(current),
            saved: Some(NativeBasis::INITIAL),
            saved_counter: Some(0),
        }
    }

    fn child_basis(&self, orientation: ModelOrientation) -> Option<NativeBasis> {
        if orientation.uses_saved_basis() {
            self.saved
        } else {
            let mut result = NativeBasis::INITIAL;
            result.copy_oriented_from(self.current?, orientation.raw());
            Some(result)
        }
    }

    fn apply(
        &mut self,
        instruction: &inf_formats::ModelInstruction,
        registers: &NeutralRegisterFile,
        math: Option<&NativeBasisMath>,
    ) {
        let opcode = instruction.raw_command as u8;
        if opcode == 0x04 {
            // 004A0AC0/00490370 construct saved vectors from world-slot deltas
            // and clear the destination counter. That construction is unmodeled.
            self.saved = None;
            self.saved_counter = Some(0);
            return;
        }
        if opcode == 0x0f {
            self.saved = None;
            return;
        } // camera vectors, counter retained
        if opcode == 0x10 {
            self.current = self.saved;
            return;
        }
        let ModelInstructionKind::Opaque(command) = &instruction.kind else {
            return;
        };
        let (axis, packed) = match (opcode, command.arguments.as_slice()) {
            (0x03, [orientation, axis, packed, ..]) => {
                self.saved = self
                    .current
                    .zip(self.saved_counter)
                    .map(|(current, counter)| {
                        let mut saved = NativeBasis::INITIAL;
                        saved.words[0] = counter;
                        saved.copy_oriented_from(current, *orientation);
                        saved
                    });
                // 004A0AFA checks raw bytes before packed decoding or axis selection.
                if *packed == 0 {
                    return;
                }
                (*axis, *packed)
            }
            (0x1c, [axis, packed, ..]) => (*axis, *packed),
            _ => return,
        };
        let next_counter = self.saved_counter.map(|counter| {
            let incremented = counter.wrapping_add(1);
            if incremented > 100 {
                0
            } else {
                incremented
            }
        });
        self.saved = self.saved.zip(registers.resolve(packed)).zip(math).map(
            |((mut saved, angle), math)| {
                let plane = match axis as i16 {
                    1 => RotationPlane::Xz,
                    2 => RotationPlane::Xy,
                    _ => RotationPlane::Yz,
                };
                math.rotate_in_plane(&mut saved, i32::from(angle.raw as i16), plane);
                saved
            },
        );
        self.saved_counter = next_counter;
    }
}

#[derive(Clone, Debug, Default)]
struct NeutralRegisterFile([Option<NeutralValue>; 22]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NeutralValue {
    raw: u16,
    callback_backed: bool,
}

impl NeutralRegisterFile {
    /// 004A0ED0 copies parent context WORDs +70/+72/+74/+76 at
    /// 004A10B9..004A10FD. All later registers remain unknown in the child,
    /// and no child register is copied back to its parent on return.
    fn for_child(&self) -> Self {
        let mut child = Self::default();
        child.0[..4].copy_from_slice(&self.0[..4]);
        child
    }

    fn apply(&mut self, instruction: &inf_formats::ModelInstruction) {
        let ModelInstructionKind::RegisterOperation(command) = &instruction.kind else {
            return;
        };
        let Some((&destination, operands)) = command.arguments.split_first() else {
            return;
        };
        let value = match (instruction.raw_command as u8, operands) {
            (0xf1, [left, right]) => {
                self.resolve(*left)
                    .zip(self.resolve(*right))
                    .map(|(left, right)| NeutralValue {
                        raw: left.raw.wrapping_sub(right.raw),
                        callback_backed: left.callback_backed || right.callback_backed,
                    })
            }
            (0xf5, [value, shift]) => {
                self.resolve(*value)
                    .zip(self.resolve(*shift))
                    .map(|(value, shift)| NeutralValue {
                        raw: (u32::from(value.raw) << (u32::from(shift.raw) & 0x1f)) as u16,
                        callback_backed: value.callback_backed || shift.callback_backed,
                    })
            }
            _ => None,
        };
        if let Some(register) = self.0.get_mut(usize::from(destination)) {
            *register = value;
        }
    }

    fn resolve(&self, packed: u16) -> Option<NeutralValue> {
        if packed & 0x0080 == 0 {
            let shift = u32::from((packed as u8 >> 2) & 0x0f);
            let mut value = packed & 0xff03;
            if shift != 0 {
                value = value.rotate_left(shift);
            }
            return Some(NeutralValue {
                raw: if packed & 0x0040 == 0 { value } else { !value },
                callback_backed: false,
            });
        }
        if packed & 0x0040 != 0 {
            return self.0.get(usize::from(packed & 0x003f)).copied()?;
        }

        // FUN_004ED4B0 delegates this encoding to the packed-value callback.
        // Substituting zero is the viewer's explicit neutral-pose policy, not
        // a claim that the original callback always returned zero.
        Some(NeutralValue {
            raw: 0,
            callback_backed: true,
        })
    }
}

/// Add every child instance whose neutral joint, attachment slot, and native
/// orientation are statically proven, retaining unresolved siblings as
/// diagnostic omissions.
pub(crate) fn build_neutral_model_composite(
    catalog: &AssetCatalog,
    root: &ModelVariant,
    source: &str,
) -> Option<NeutralModelComposite> {
    let root_geometry = neutral_seed_geometry(root)?;
    let root_omissions = 0..root_geometry.omitted_effects.len();
    let mut builder = NeutralCompositeBuilder {
        catalog,
        source,
        composite: NeutralModelComposite {
            geometry: root_geometry,
            child_instances: 0,
        },
        stack: vec![root.entry.identity.global_index],
    };
    let all_instances_proven = append_neutral_children(
        &mut builder,
        root,
        ModelTransform::IDENTITY,
        NeutralRegisterFile::default(),
        0,
        root_omissions,
    );
    let mut composite = builder.composite;
    if composite.child_instances == 0 || composite.geometry.faces.is_empty() {
        return None;
    }
    composite
        .geometry
        .omitted_effects
        .retain(|omission| omission.count != 0);
    if all_instances_proven
        && !composite
            .geometry
            .omitted_effects
            .iter()
            .any(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
    {
        composite
            .geometry
            .unresolved_dependencies
            .retain(|dependency| {
                !matches!(
                    dependency,
                    ModelDynamicDependency::ExternalModelCatalog
                        | ModelDynamicDependency::SavedTransformBasis
                )
            });
    }
    Some(composite)
}

fn neutral_seed_geometry(variant: &ModelVariant) -> Option<ModelDiagnosticGeometry> {
    match &variant.diagnostic {
        ModelDiagnosticProjection::Ready(geometry) => Some(geometry.clone()),
        ModelDiagnosticProjection::Unavailable(unavailable)
            if matches!(
                unavailable.kind,
                ModelDiagnosticUnavailableKind::ModelInstance
                    | ModelDiagnosticUnavailableKind::RuntimeDependentGeometry
            ) && unavailable
                .omitted_effects
                .iter()
                .any(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances) =>
        {
            Some(ModelDiagnosticGeometry {
                view_plane_decisions: Vec::new(),
                vertices: Vec::new(),
                faces: Vec::new(),
                face_presentation: Vec::new(),
                ignored_edge_commands: 0,
                completeness: ModelDiagnosticCompleteness::PartialMesh,
                unresolved_dependencies: unavailable.unresolved_dependencies.clone(),
                omitted_effects: unavailable.omitted_effects.clone(),
            })
        }
        ModelDiagnosticProjection::Unavailable(_) => None,
    }
}

fn resolve_instance_omissions_in_range(
    geometry: &mut ModelDiagnosticGeometry,
    range: Range<usize>,
    mut resolved: usize,
) {
    for omission in &mut geometry.omitted_effects[range] {
        if omission.kind != ModelDiagnosticOmissionKind::ModelInstances || resolved == 0 {
            continue;
        }
        let consumed = omission.count.min(resolved);
        omission.count -= consumed;
        resolved -= consumed;
    }
}

fn append_neutral_children(
    builder: &mut NeutralCompositeBuilder<'_>,
    parent: &ModelVariant,
    parent_transform: ModelTransform,
    mut registers: NeutralRegisterFile,
    depth: usize,
    parent_omissions: Range<usize>,
) -> bool {
    const MAX_INSTANCE_DEPTH: usize = 16;
    let has_model_instances = parent
        .entry
        .program
        .instructions
        .iter()
        .any(|instruction| matches!(instruction.kind, ModelInstructionKind::ModelInstance(_)));
    if !has_model_instances {
        return true;
    }
    if depth >= MAX_INSTANCE_DEPTH || program_has_control_flow(parent) {
        return false;
    }

    let mut written_slots = Vec::new();
    // Root registers begin unknown. Each child receives only the four proven
    // inherited WORDs, including their callback-backed neutral provenance.
    let mut bases = NeutralBasisState::new(parent_transform.basis);
    let mut attachment_context_unknown = false;
    let mut complete = true;
    let mut resolved_here = 0;
    let catalog = builder.catalog;
    let source = builder.source;
    for instruction in &parent.entry.program.instructions {
        let ModelInstructionKind::ModelInstance(instance) = &instruction.kind else {
            if let ModelInstructionKind::RegisterOperation(command) = &instruction.kind {
                if command
                    .arguments
                    .first()
                    .is_some_and(|destination| *destination >= 22)
                {
                    // 22/23 alias the parent pointer; 24+ alias saved-basis words.
                    // Those writes are not an independent diagnostic register file.
                    attachment_context_unknown = true;
                    bases.saved = None;
                    bases.saved_counter = None;
                }
            }
            if matches!(instruction.kind, ModelInstructionKind::NestedProgram(_)) {
                // 05/09 restore current basis/origin, but saved basis, registers and
                // world-slot writes may escape. We do not execute nested programs.
                attachment_context_unknown = true;
                bases.saved = None;
                bases.saved_counter = None;
                registers = NeutralRegisterFile::default();
            }
            record_possible_vertex_writes(instruction, &mut written_slots);
            bases.apply(instruction, &registers, catalog.native_basis_math());
            registers.apply(instruction);
            continue;
        };
        let Ok(child_id) = u16::try_from(instance.model.raw()) else {
            complete = false;
            continue;
        };
        if builder.stack.contains(&child_id) {
            complete = false;
            continue;
        }
        let Some(&attachment_slot) = instance.state_words.first() else {
            complete = false;
            continue;
        };
        if attachment_context_unknown || written_slots.contains(&attachment_slot) {
            complete = false;
            continue;
        }
        let Some(attachment) = static_model_slot(parent, attachment_slot) else {
            complete = false;
            continue;
        };
        let Some(basis) = bases.child_basis(instance.orientation) else {
            complete = false;
            continue;
        };
        // Root world slots were initialized at invocation entry. A later 10
        // changes current basis, not those already computed attachment points.
        let child_transform = ModelTransform {
            origin: parent_transform.apply(attachment),
            basis,
        };
        let Some(child) = catalog.active_model_variant_in_source(child_id, source) else {
            complete = false;
            continue;
        };
        let Some(child_geometry) = neutral_seed_geometry(child) else {
            complete = false;
            continue;
        };
        let child_omission_start = builder.composite.geometry.omitted_effects.len();
        if !append_transformed_geometry(
            &mut builder.composite.geometry,
            &child_geometry,
            child_transform,
        ) {
            complete = false;
            continue;
        }
        let child_omissions =
            child_omission_start..builder.composite.geometry.omitted_effects.len();

        builder.composite.child_instances += 1;
        resolved_here += 1;
        builder.stack.push(child_id);
        if !append_neutral_children(
            builder,
            child,
            child_transform,
            registers.for_child(),
            depth + 1,
            child_omissions,
        ) {
            complete = false;
        }
        builder.stack.pop();
    }
    resolve_instance_omissions_in_range(
        &mut builder.composite.geometry,
        parent_omissions,
        resolved_here,
    );
    complete
}

fn program_has_control_flow(variant: &ModelVariant) -> bool {
    variant
        .entry
        .program
        .instructions
        .iter()
        .any(|instruction| match instruction.raw_command as u8 {
            0x0b | 0x13 => true,
            // 004A0C90 returns args+jump or args+3. When jump is three,
            // unknown operands cannot select another instruction boundary.
            // The diagnostic retains their callback/register dependencies.
            0x16 => !matches!(
                &instruction.kind,
                ModelInstructionKind::UnresolvedDynamic(command)
                    if command.arguments.first() == Some(&3)
            ),
            _ => false,
        })
}

fn record_possible_vertex_writes(
    instruction: &inf_formats::ModelInstruction,
    written_slots: &mut Vec<u16>,
) {
    if !(0x20..=0x2d).contains(&(instruction.raw_command as u8)) {
        return;
    }
    let destination = match &instruction.kind {
        ModelInstructionKind::Opaque(command) => command.arguments.first().copied(),
        ModelInstructionKind::UnresolvedDynamic(command) => command.arguments.first().copied(),
        _ => None,
    };
    let Some(destination) = destination else {
        return;
    };
    let opcode = instruction.raw_command as u8;
    // 004A0212 sign-extends the command WORD before dispatch; 004EBF25 /
    // 004EC5E9 use SAR 8, then the loop consumes pairs with unsigned SHR 2.
    let mut mode = (i32::from(instruction.raw_command as i16) >> 8) as u32;
    if opcode != 0x20 && opcode != 0x2c {
        // 004EBF00 / 004EC5C0 retain the shifted DWORD; other generated
        // vertex handlers mask four mode bits. 2D's upper nibble is length.
        mode &= 15;
    }
    let mut offset = 0;
    while mode != 0 {
        let writes_world = if opcode == 0x2a {
            // 004ED350 copies an entire parent record for either bit of a
            // pair. A null-parent mirror-world request first writes original
            // origin XYZ. Preserve the union, since parent custody is unknown.
            mode & 3 != 0 || (offset == 0 && mode & 4 != 0)
        } else {
            mode & 1 != 0
        };
        let slot = writes_world
            .then(|| destination.checked_add(offset))
            .flatten();
        mode >>= 2;
        offset += 1;
        let Some(slot) = slot else { continue };
        if !written_slots.contains(&slot) {
            written_slots.push(slot);
        }
    }
}

fn static_model_slot(variant: &ModelVariant, slot: u16) -> Option<[i32; 3]> {
    // FUN_004EC0A0 receives the first three header boundaries. The fourth
    // word is not part of root vertex initialization.
    let [static_start, mirrored_start, static_end, _reserved_end] =
        variant.entry.header.vertex_ranges;
    if slot < static_start || slot >= static_end || (slot < mirrored_start && slot % 2 != 0) {
        return None;
    }
    let raw = variant.entry.vertices.get(usize::from(slot / 2))?;
    let x = i32::from(raw.x);
    Some([
        if slot >= mirrored_start && slot % 2 != 0 {
            -x
        } else {
            x
        },
        i32::from(raw.y),
        i32::from(raw.z),
    ])
}

fn append_transformed_geometry(
    target: &mut ModelDiagnosticGeometry,
    source: &ModelDiagnosticGeometry,
    transform: ModelTransform,
) -> bool {
    let vertices: Vec<_> = source
        .vertices
        .iter()
        .map(|vertex| {
            let mut vertex = *vertex;
            vertex.position = transform.apply(vertex.position);
            vertex
        })
        .collect();
    let vertex_offset = target.vertices.len();
    let Some(faces) = source
        .faces
        .iter()
        .map(|face| {
            face.iter()
                .map(|index| index.checked_add(vertex_offset))
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()
    else {
        return false;
    };

    target.vertices.extend(vertices);
    target.faces.extend(faces);
    target
        .face_presentation
        .extend(source.face_presentation.iter().cloned());
    target.ignored_edge_commands = target
        .ignored_edge_commands
        .saturating_add(source.ignored_edge_commands);
    target.completeness = ModelDiagnosticCompleteness::PartialMesh;
    for dependency in &source.unresolved_dependencies {
        if !target.unresolved_dependencies.contains(dependency) {
            target.unresolved_dependencies.push(*dependency);
        }
    }
    target
        .omitted_effects
        .extend(source.omitted_effects.iter().cloned());
    true
}

pub(crate) fn immediate_model_children(variant: &ModelVariant) -> Vec<u16> {
    let mut children = Vec::new();
    for child in variant.child_model_ids() {
        if !children.contains(&child) {
            children.push(child);
        }
    }
    children
}

pub(crate) fn model_name(catalog: &AssetCatalog, global_id: u16) -> Option<&str> {
    catalog
        .models
        .iter()
        .find(|asset| asset.locator.global_id == global_id)
        .map(|asset| asset.locator.name.as_str())
}

#[cfg(test)]
#[path = "model_composite_inheritance_tests.rs"]
mod inheritance_tests;

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn retail_catalog() -> AssetCatalog {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        AssetCatalog::load(&root, None).unwrap()
    }

    fn basis_instruction(opcode: u16, arguments: &[u16]) -> inf_formats::ModelInstruction {
        inf_formats::ModelInstruction {
            span: inf_formats::ByteSpan::new(0, 0),
            raw_command: opcode,
            kind: ModelInstructionKind::Opaque(inf_formats::ModelRawCommand {
                arguments: arguments.to_vec(),
            }),
        }
    }

    #[test]
    fn world_write_barriers_follow_native_mode_pairs_and_parent_copy_exception() {
        for opcode in 0x20..=0x2d {
            for mode in 0..16 {
                let mut written = Vec::new();
                record_possible_vertex_writes(
                    &basis_instruction(opcode | mode << 8, &[10]),
                    &mut written,
                );
                let mut expected = Vec::new();
                if if opcode == 0x2a {
                    mode & 7 != 0
                } else {
                    mode & 1 != 0
                } {
                    expected.push(10);
                }
                if if opcode == 0x2a {
                    mode & 12 != 0
                } else {
                    mode & 4 != 0
                } {
                    expected.push(11);
                }
                assert_eq!(written, expected, "opcode {opcode:02x}, mode {mode:x}");
            }
        }
        for opcode in [0x20, 0x2c] {
            let mut written = Vec::new();
            record_possible_vertex_writes(&basis_instruction(0x5100 | opcode, &[10]), &mut written);
            assert_eq!(written, [10, 12, 13]);
            written.clear();
            record_possible_vertex_writes(&basis_instruction(0x8000 | opcode, &[10]), &mut written);
            assert_eq!(
                written,
                (14..26).collect::<Vec<_>>(),
                "signed command extends upper mode pairs"
            );
        }
        let mut written = Vec::new();
        record_possible_vertex_writes(&basis_instruction(0xf02d, &[10]), &mut written);
        assert!(
            written.is_empty(),
            "2D high nibble is callback argument count"
        );
    }

    #[test]
    fn projection_only_writes_preserve_proven_world_attachments() {
        let catalog = retail_catalog();
        let parent = catalog
            .active_model_variant_in_source(707, "29.ovl")
            .unwrap();
        let baseline = build_neutral_model_composite(&catalog, parent, "29.ovl").unwrap();
        // This is the declared-output diagnostic policy, not a proof that
        // every installed actor callback has no non-geometric side effects.
        for opcode in [0x20, 0x23, 0x2b, 0x2c, 0x2d] {
            let mut modified = parent.clone();
            modified
                .entry
                .program
                .instructions
                .insert(0, basis_instruction(0xa00 | opcode, &[0, 0, 0]));
            let composite = build_neutral_model_composite(&catalog, &modified, "29.ovl").unwrap();
            assert_eq!(composite.geometry, baseline.geometry);
            assert_eq!(composite.child_instances, baseline.child_instances);
        }
        for command in [0x0120, 0x022a, 0x042a] {
            let mut modified = parent.clone();
            modified
                .entry
                .program
                .instructions
                .insert(0, basis_instruction(command, &[0, 0, 0]));
            assert!(
                build_neutral_model_composite(&catalog, &modified, "29.ovl").is_none(),
                "world/whole-parent writes stay unproven"
            );
        }
    }

    #[test]
    fn equal_target_branches_preserve_children_but_real_guards_still_block() {
        let catalog = retail_catalog();
        let parent = catalog
            .active_model_variant_in_source(707, "29.ovl")
            .unwrap();
        let baseline = build_neutral_model_composite(&catalog, parent, "29.ovl").unwrap();
        for sense in [0, 1, 0xff] {
            for jump in [3, 4, u16::MAX] {
                let mut modified = parent.clone();
                let mut branch = basis_instruction(0x16 | sense << 8, &[]);
                let next = modified.entry.program.instructions[0].span.start;
                branch.span = inf_formats::ByteSpan::new(next - 8, next);
                branch.kind =
                    ModelInstructionKind::UnresolvedDynamic(inf_formats::ModelUnresolvedCommand {
                        arguments: vec![jump, 0xc0, 0x80],
                        dependencies: vec![
                            ModelDynamicDependency::RegisterFile,
                            ModelDynamicDependency::PackedValueCallback,
                        ],
                    });
                modified.entry.program.instructions.insert(0, branch);
                if jump == 3 {
                    modified.diagnostic = inf_formats::analyze_model_diagnostic(
                        inf_formats::ModelDiagnosticRequest {
                            entry: &modified.entry,
                            view: inf_formats::ModelDiagnosticView::Unconditioned,
                        },
                    );
                }
                let composite = build_neutral_model_composite(&catalog, &modified, "29.ovl");
                if jump == 3 {
                    let composite = composite.unwrap();
                    assert_eq!(composite.geometry.vertices, baseline.geometry.vertices);
                    assert_eq!(composite.geometry.faces, baseline.geometry.faces);
                    assert_eq!(
                        composite.geometry.face_presentation,
                        baseline.geometry.face_presentation
                    );
                    for dependency in [
                        ModelDynamicDependency::RegisterFile,
                        ModelDynamicDependency::PackedValueCallback,
                    ] {
                        assert!(composite
                            .geometry
                            .unresolved_dependencies
                            .contains(&dependency));
                    }
                } else {
                    assert!(composite.is_none());
                }
            }
        }
        for guard in [0x0b, 0x13] {
            let mut modified = parent.clone();
            modified
                .entry
                .program
                .instructions
                .insert(0, basis_instruction(guard, &[4, 0, 0]));
            assert!(build_neutral_model_composite(&catalog, &modified, "29.ovl").is_none());
        }
    }

    #[test]
    fn authored_rotation_and_encoded_zero_follow_the_native_counter_path() {
        let catalog = retail_catalog();
        let math = catalog.native_basis_math().unwrap();
        let registers = NeutralRegisterFile::default();
        let mut state = NeutralBasisState::new(NativeBasis::INITIAL);
        state.apply(
            &basis_instruction(3, &[0, 1, 0x4000]),
            &registers,
            Some(math),
        );
        assert_eq!(
            state.saved.unwrap().transform_point([11, 22, 33]),
            [33, 22, -11]
        );
        assert_eq!(state.saved_counter, Some(1));
        // Raw zero skips axis/decode/rotation and preserves the saved counter.
        state.apply(&basis_instruction(3, &[6, u16::MAX, 0]), &registers, None);
        assert_eq!(
            state.saved.unwrap().transform_point([11, 22, 33]),
            [11, 22, 33]
        );
        assert_eq!(state.saved_counter, Some(1));
        // Encoded literal zero still rotates; the 101st call normalizes.
        for _ in 0..100 {
            state.apply(&basis_instruction(3, &[0, 1, 4]), &registers, Some(math));
        }
        assert_eq!(state.saved_counter, Some(0));
        assert_eq!(state.saved.unwrap().words[0], 0);
        state.apply(&basis_instruction(3, &[0, 1, 4]), &registers, None);
        assert!(
            state.saved.is_none(),
            "missing original table cannot silently skip a rotation"
        );
    }

    #[test]
    fn saved_rotation_axis_fallback_and_child_copy_are_intentionally_asymmetric() {
        let catalog = retail_catalog();
        let math = catalog.native_basis_math();
        let registers = NeutralRegisterFile::default();
        for axis in [0, 3, u16::MAX] {
            let mut state = NeutralBasisState::new(NativeBasis::INITIAL);
            state.apply(&basis_instruction(3, &[0, axis, 0x4000]), &registers, math);
            let saved = state.child_basis(ModelOrientation::from_raw(0x3e)).unwrap();
            assert_eq!(saved.words[0], 1);
            assert_eq!(saved.transform_point([11, 22, 33]), [11, -33, 22]);
            let fixed = state.child_basis(ModelOrientation::from_raw(0x38)).unwrap();
            assert_eq!(fixed.words[0], 0);
            assert_eq!(fixed.transform_point([11, 22, 33]), [-11, -22, -33]);
        }
    }

    #[test]
    fn saved_mutations_keep_unknowns_and_native_reset_boundaries() {
        let catalog = retail_catalog();
        let math = catalog.native_basis_math();
        let registers = NeutralRegisterFile::default();
        let mut state = NeutralBasisState::new(NativeBasis::INITIAL);
        state.apply(&basis_instruction(0x1c, &[2, 0x4000]), &registers, math);
        assert_eq!(
            state.saved.unwrap().transform_point([11, 22, 33]),
            [-22, 11, 33]
        );
        state.apply(&basis_instruction(0x10, &[]), &registers, math);
        assert_eq!(state.current, state.saved);
        state.apply(&basis_instruction(0x0f, &[]), &registers, math);
        assert!(state.child_basis(ModelOrientation::from_raw(6)).is_none());
        assert_eq!(state.saved_counter, Some(1));
        state.apply(&basis_instruction(3, &[0, 1, 0]), &registers, math);
        assert_eq!(state.saved, state.current);
        state.apply(&basis_instruction(4, &[0, 0, 0]), &registers, math);
        assert!(state.saved.is_none());
        assert!(state.child_basis(ModelOrientation::from_raw(6)).is_none());
        assert_eq!(state.saved_counter, Some(0));
        state.apply(&basis_instruction(3, &[0, 1, 0]), &registers, math);
        assert_eq!(state.saved.unwrap().words[0], 0);
        assert_eq!(
            &state.saved.unwrap().words[1..],
            &state.current.unwrap().words[1..]
        );
        state.apply(&basis_instruction(3, &[0, 1, 0xc2]), &registers, math);
        assert!(state.saved.is_none());
        assert_eq!(state.saved_counter, Some(1));
        assert!(
            registers.resolve(0xd8).is_none(),
            "register 24 aliases basis; it is not a neutral callback"
        );
    }

    #[test]
    fn current_basis_changes_do_not_move_preinitialized_attachment_slots() {
        let entry = ModelTransform {
            origin: [i32::MAX, 100, 200],
            basis: NativeBasis::INITIAL,
        };
        assert_eq!(entry.apply([1, 2, 3]), [i32::MIN, 102, 203]);
        let catalog = retail_catalog();
        let mut state = NeutralBasisState::new(entry.basis);
        state.apply(
            &basis_instruction(3, &[0, 1, 0x4000]),
            &NeutralRegisterFile::default(),
            catalog.native_basis_math(),
        );
        state.apply(
            &basis_instruction(0x10, &[]),
            &NeutralRegisterFile::default(),
            catalog.native_basis_math(),
        );
        assert_ne!(state.current, Some(entry.basis));
        assert_eq!(entry.apply([1, 2, 3]), [i32::MIN, 102, 203]);
    }

    #[test]
    fn missing_original_table_keeps_literal_zero_and_fixed_children_available() {
        let mut catalog = AssetCatalog::default();
        catalog.models = retail_catalog().models;
        let parent = catalog
            .active_model_variant_in_source(264, "1.ovl")
            .unwrap();
        let composite = build_neutral_model_composite(&catalog, parent, "1.ovl").unwrap();
        assert_eq!(
            composite.child_instances, 1,
            "only raw-zero TURBOGROUP child needs no table"
        );
        assert_eq!(composite.geometry.faces.len(), 70);
        let mut state = NeutralBasisState::new(NativeBasis::INITIAL);
        state.apply(
            &basis_instruction(3, &[0, 1, 0x4000]),
            &NeutralRegisterFile::default(),
            None,
        );
        assert!(state.child_basis(ModelOrientation::from_raw(6)).is_none());
        assert!(state.child_basis(ModelOrientation::from_raw(1)).is_some());
    }

    #[test]
    fn alias_register_writes_are_not_treated_as_independent_static_angles() {
        let catalog = retail_catalog();
        let mut parent = catalog
            .active_model_variant_in_source(264, "1.ovl")
            .unwrap()
            .clone();
        let mut instruction = basis_instruction(0xf1, &[]);
        instruction.kind =
            ModelInstructionKind::RegisterOperation(inf_formats::ModelUnresolvedCommand {
                arguments: vec![24, 0, 0],
                dependencies: vec![ModelDynamicDependency::RegisterFile],
            });
        parent.entry.program.instructions.insert(0, instruction);
        assert!(build_neutral_model_composite(&catalog, &parent, "1.ovl").is_none());
    }

    #[test]
    fn authored_basis_corpus_census_preserves_partial_mesh_status() {
        let catalog = retail_catalog();
        let mut counts = [0; 3];
        for variant in catalog.models.iter().flat_map(|asset| &asset.variants) {
            let mut available = false;
            for occurrence in &variant.occurrences {
                if let Some(composite) =
                    build_neutral_model_composite(&catalog, variant, &occurrence.source)
                {
                    counts[1] += 1;
                    available = true;
                    assert_eq!(
                        composite.geometry.completeness,
                        ModelDiagnosticCompleteness::PartialMesh
                    );
                    assert_eq!(
                        composite.geometry.faces.len(),
                        composite.geometry.face_presentation.len()
                    );
                }
            }
            counts[0] += usize::from(available);
            counts[2] += usize::from(
                available
                    && matches!(
                        variant.diagnostic,
                        ModelDiagnosticProjection::Unavailable(_)
                    ),
            );
        }
        assert_eq!(counts, [105, 697, 25]);
    }

    #[test]
    fn neutral_registers_decode_packed_values_and_preserve_nonzero_rotations() {
        let mut registers = NeutralRegisterFile::default();
        assert_eq!(
            registers.resolve(0x8007),
            Some(NeutralValue {
                raw: 7,
                callback_backed: false,
            })
        );
        assert_eq!(registers.resolve(0x00c2), None);
        assert_eq!(
            registers.resolve(0x4000),
            Some(NeutralValue {
                raw: 0x4000,
                callback_backed: false,
            })
        );

        registers.0[2] = Some(NeutralValue {
            raw: 0x1234,
            callback_backed: true,
        });
        assert_eq!(
            registers.resolve(0x00c2),
            Some(NeutralValue {
                raw: 0x1234,
                callback_backed: true,
            })
        );
    }

    #[test]
    fn retail_literal_zero_saved_basis_exposes_child_geometry_and_provenance() {
        let catalog = retail_catalog();
        let parent = catalog
            .active_model_variant_in_source(707, "29.ovl")
            .expect("retail NEURALPOSITION");
        let child = catalog
            .active_model_variant_in_source(708, "29.ovl")
            .expect("retail NEURAL child");
        assert!(matches!(
            parent.diagnostic,
            ModelDiagnosticProjection::Unavailable(_)
        ));
        let ModelDiagnosticProjection::Ready(child_geometry) = &child.diagnostic else {
            panic!("NEURAL has a proven local diagnostic")
        };
        let composite = build_neutral_model_composite(&catalog, parent, "29.ovl")
            .expect("authored zero copies the fixed base into the saved basis");

        assert_eq!(composite.child_instances, 1);
        assert_eq!(composite.geometry.vertices.len(), 12);
        assert_eq!(composite.geometry.faces.len(), 10);
        let origin = static_model_slot(parent, 0).expect("authored attachment slot");
        for (actual, source) in composite
            .geometry
            .vertices
            .iter()
            .zip(&child_geometry.vertices)
        {
            assert_eq!(actual.slot, source.slot);
            assert_eq!(
                actual.position,
                std::array::from_fn(|axis| source.position[axis] + origin[axis])
            );
        }
        assert_eq!(composite.geometry.faces, child_geometry.faces);
        assert_eq!(
            composite.geometry.face_presentation,
            child_geometry.face_presentation
        );
        assert_eq!(
            composite.geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
        let child_omissions = |geometry: &ModelDiagnosticGeometry| {
            geometry
                .omitted_effects
                .iter()
                .filter(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
                .map(|omission| omission.count)
                .sum::<usize>()
        };
        // This proves the parent attachment, not every attachment inside the
        // child's program. Preserve those deeper omissions and dependencies.
        assert_eq!(
            child_omissions(&composite.geometry),
            child_omissions(child_geometry)
        );
        for dependency in &child_geometry.unresolved_dependencies {
            assert!(composite
                .geometry
                .unresolved_dependencies
                .contains(dependency));
        }
    }

    #[test]
    fn retail_authored_rotations_assemble_all_turbogroup_siblings() {
        let catalog = retail_catalog();
        let parent = catalog
            .active_model_variant_in_source(264, "1.ovl")
            .expect("retail TURBOGROUP");
        let composite = build_neutral_model_composite(&catalog, parent, "1.ovl")
            .expect("the first of three children has a literal-zero saved basis");
        assert_eq!(composite.child_instances, 3);
        assert_eq!(composite.geometry.vertices.len(), 114);
        assert_eq!(composite.geometry.faces.len(), 210);
        assert_eq!(
            composite
                .geometry
                .omitted_effects
                .iter()
                .filter(|omission| omission.kind == ModelDiagnosticOmissionKind::ModelInstances)
                .map(|omission| omission.count)
                .sum::<usize>(),
            0
        );
        for dependency in [
            ModelDynamicDependency::ExternalModelCatalog,
            ModelDynamicDependency::SavedTransformBasis,
        ] {
            assert!(!composite
                .geometry
                .unresolved_dependencies
                .contains(&dependency));
        }
    }

    #[test]
    fn resolved_children_reduce_only_their_parents_instance_omissions() {
        let omission = |kind, count| inf_formats::ModelDiagnosticOmission {
            kind,
            count,
            opcodes: vec![0x0e],
            reason: "test".to_owned(),
        };
        let mut geometry = ModelDiagnosticGeometry {
            view_plane_decisions: Vec::new(),
            vertices: Vec::new(),
            faces: Vec::new(),
            face_presentation: Vec::new(),
            ignored_edge_commands: 0,
            completeness: ModelDiagnosticCompleteness::PartialMesh,
            unresolved_dependencies: Vec::new(),
            omitted_effects: vec![
                omission(ModelDiagnosticOmissionKind::ModelInstances, 3),
                omission(ModelDiagnosticOmissionKind::ConditionalFaces, 2),
                omission(ModelDiagnosticOmissionKind::ModelInstances, 1),
            ],
        };

        resolve_instance_omissions_in_range(&mut geometry, 2..3, 1);
        assert_eq!(geometry.omitted_effects[0].count, 3);
        assert_eq!(geometry.omitted_effects[2].count, 0);

        resolve_instance_omissions_in_range(&mut geometry, 0..2, 2);
        assert_eq!(geometry.omitted_effects[0].count, 1);
        assert_eq!(
            geometry.omitted_effects[1].kind,
            ModelDiagnosticOmissionKind::ConditionalFaces
        );
        assert_eq!(geometry.omitted_effects[1].count, 2);
    }

    #[test]
    fn unresolved_topology_is_never_used_as_a_composite_seed() {
        let catalog = retail_catalog();
        let mut variant = catalog
            .models
            .iter()
            .flat_map(|asset| &asset.variants)
            .find(|variant| {
                matches!(&variant.diagnostic,
                ModelDiagnosticProjection::Unavailable(gap)
                    if gap.kind == ModelDiagnosticUnavailableKind::RuntimeDependentGeometry)
            })
            .expect("retail callback-driven nested transform")
            .clone();
        // The equal-target branch correction removes the old retail example
        // of an unresolved program with children. Exercise that safety policy
        // explicitly, rather than requiring a known diagnostic bug to persist.
        for kind in [
            ModelDiagnosticUnavailableKind::NestedProgram,
            ModelDiagnosticUnavailableKind::UnresolvedCommandEffects,
        ] {
            let ModelDiagnosticProjection::Unavailable(gap) = &mut variant.diagnostic else {
                unreachable!()
            };
            gap.kind = kind;
            gap.omitted_effects
                .push(inf_formats::ModelDiagnosticOmission {
                    kind: ModelDiagnosticOmissionKind::ModelInstances,
                    count: 1,
                    opcodes: vec![0x0e],
                    reason: "synthetic child inside unresolved topology".to_owned(),
                });
            assert!(neutral_seed_geometry(&variant).is_none());
        }
    }

    #[test]
    fn static_model_slots_use_the_first_three_root_boundaries() {
        let catalog = retail_catalog();
        let beacon = catalog
            .models
            .iter()
            .find(|asset| asset.locator.global_id == 2)
            .and_then(|asset| asset.default_variant())
            .expect("retail BEACON");
        assert_eq!(beacon.entry.header.vertex_ranges, [2, 2, 20, 20]);

        assert_eq!(static_model_slot(beacon, 0), None);
        assert_eq!(static_model_slot(beacon, 2), Some([48, 132, 0]));
        assert_eq!(static_model_slot(beacon, 3), Some([-48, 132, 0]));
        assert_eq!(static_model_slot(beacon, 20), None);
    }

    #[test]
    fn retail_radar_register_chain_keeps_runtime_dish_attachment_unflattened() {
        let catalog = retail_catalog();
        let radar = catalog
            .models
            .iter()
            .find(|asset| asset.locator.global_id == 60)
            .and_then(|asset| asset.default_variant())
            .expect("retail COMRADAR");

        assert_eq!(immediate_model_children(radar), [61]);
        assert_eq!(catalog.model_parents(61), [60, 579]);

        let ModelDiagnosticProjection::Ready(geometry) = &radar.diagnostic else {
            panic!("COMRADAR has proven raw-projected shell geometry")
        };
        assert_eq!(
            geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
        assert!(!geometry.faces.is_empty());
        assert!(geometry
            .omitted_effects
            .iter()
            .any(|o| o.kind == ModelDiagnosticOmissionKind::ModelInstances));

        let mut registers = NeutralRegisterFile::default();
        let instance_index = radar
            .entry
            .program
            .instructions
            .iter()
            .position(|instruction| {
                matches!(instruction.kind, ModelInstructionKind::ModelInstance(_))
            })
            .expect("COMRADAR model instance");
        for instruction in &radar.entry.program.instructions[..instance_index - 1] {
            registers.apply(instruction);
        }
        assert_eq!(
            registers.resolve(0x00c2),
            Some(NeutralValue {
                raw: 0,
                callback_backed: true,
            })
        );
        let mut bases = NeutralBasisState::new(NativeBasis::INITIAL);
        bases.apply(
            &radar.entry.program.instructions[instance_index - 1],
            &registers,
            catalog.native_basis_math(),
        );
        assert_eq!(bases.saved.unwrap().words[0], 1);
        assert_eq!(
            bases.saved.unwrap().transform_point([11, 22, 33]),
            [11, 22, 33]
        );
    }

    #[test]
    fn guarded_instance_programs_are_not_flattened() {
        let catalog = retail_catalog();
        let guarded = catalog
            .models
            .iter()
            .flat_map(|asset| &asset.variants)
            .find(|variant| {
                program_has_control_flow(variant)
                    && !immediate_model_children(variant).is_empty()
                    && matches!(variant.diagnostic, ModelDiagnosticProjection::Ready(_))
            })
            .expect("retail guarded model-instance program");
        let ModelDiagnosticProjection::Ready(_) = &guarded.diagnostic else {
            unreachable!()
        };
        let source = guarded
            .occurrences
            .first()
            .map(|occurrence| occurrence.source.as_str())
            .expect("guarded model source");

        assert!(build_neutral_model_composite(&catalog, guarded, source).is_none());
    }

    #[test]
    fn retail_compositor_exposes_proven_children_without_fabricating_the_rest() {
        let catalog = retail_catalog();
        let (root, composite) = catalog
            .models
            .iter()
            .flat_map(|asset| &asset.variants)
            .filter(|variant| matches!(&variant.diagnostic, ModelDiagnosticProjection::Ready(_)))
            .find_map(|variant| {
                variant.occurrences.iter().find_map(|occurrence| {
                    build_neutral_model_composite(&catalog, variant, &occurrence.source)
                        .map(|composite| (variant, composite))
                })
            })
            .expect("retail corpus has a statically provable neutral composite");

        assert!(root.entry.program.instructions.iter().any(|instruction| {
            matches!(instruction.kind, ModelInstructionKind::ModelInstance(_))
        }));
        assert!(composite.child_instances > 0);
        assert!(!composite.geometry.faces.is_empty());
        assert_eq!(
            composite.geometry.faces.len(),
            composite.geometry.face_presentation.len()
        );
        assert_eq!(
            composite.geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
    }

    #[test]
    fn runtime_local_shell_can_still_expose_proven_child_geometry() {
        let catalog = retail_catalog();
        let composite = catalog
            .models
            .iter()
            .flat_map(|asset| &asset.variants)
            .filter(|variant| {
                matches!(
                    &variant.diagnostic,
                    ModelDiagnosticProjection::Unavailable(unavailable)
                        if unavailable.omitted_effects.iter().any(|omission| {
                            omission.kind == ModelDiagnosticOmissionKind::ModelInstances
                        })
                )
            })
            .find_map(|variant| {
                variant.occurrences.iter().find_map(|occurrence| {
                    build_neutral_model_composite(&catalog, variant, &occurrence.source)
                })
            })
            .expect("retail corpus has a runtime root with proven child geometry");

        assert!(composite.child_instances > 0);
        assert!(!composite.geometry.faces.is_empty());
        assert_eq!(
            composite.geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
        assert!(!composite.geometry.omitted_effects.is_empty());
    }

    #[test]
    fn retail_turret_catalog_exposes_runtime_instance_chain_without_flattening() {
        let catalog = retail_catalog();
        let gunbase = catalog
            .models
            .iter()
            .find(|asset| asset.locator.global_id == 42)
            .and_then(|asset| asset.default_variant())
            .expect("retail GUNBASE");
        let gunstrut = catalog
            .models
            .iter()
            .find(|asset| asset.locator.global_id == 43)
            .and_then(|asset| asset.default_variant())
            .expect("retail GUNSTRUT01");

        assert_eq!(immediate_model_children(gunbase), [43]);
        assert_eq!(immediate_model_children(gunstrut), [44]);
        assert_eq!(catalog.model_parents(43), [42]);
        assert_eq!(catalog.model_parents(44), [43]);

        let ModelDiagnosticProjection::Ready(gunbase_geometry) = &gunbase.diagnostic else {
            panic!("GUNBASE has a proven shell alongside its runtime child")
        };
        assert_eq!(
            gunbase_geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
        assert!(!gunbase_geometry.faces.is_empty());
        assert!(gunbase_geometry
            .omitted_effects
            .iter()
            .any(|omission| omission.kind
                == inf_formats::ModelDiagnosticOmissionKind::ModelInstances));

        let ModelDiagnosticProjection::Ready(gunstrut_geometry) = &gunstrut.diagnostic else {
            panic!("GUNSTRUT01 has a proven shell alongside its runtime child")
        };
        assert_eq!(
            gunstrut_geometry.completeness,
            ModelDiagnosticCompleteness::PartialMesh
        );
        assert!(!gunstrut_geometry.faces.is_empty());
        assert!(gunstrut_geometry
            .omitted_effects
            .iter()
            .any(|omission| omission.kind
                == inf_formats::ModelDiagnosticOmissionKind::ModelInstances));
    }
}
