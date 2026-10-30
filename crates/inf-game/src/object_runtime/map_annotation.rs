//! Selector 87: 0048C710 / 0048C7A0 / 0048C780. The separate +40
//! node belongs to the process map-annotation list, not the actor list.

use inf_formats::{InstanceCommandPayload, InstanceCommandView};

use super::{ObjectArena, ObjectClass, ObjectConstructionError, ObjectError, ObjectId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MapAnnotationState {
    /// Unsigned command WORD zero-extended into native DWORD +4C. Zero
    /// selects the radius sprite; nonzero is formatted with native "%d".
    pub label: u32,
    /// Unsigned WORD +50, independently scaled by the current map tile.
    pub radius: u16,
    /// Raw unsigned RGB bytes +52..+54.
    pub rgb: [u8; 3],
}

impl MapAnnotationState {
    pub(super) fn new() -> Self {
        Self {
            label: 0,
            radius: 10_000,
            rgb: [0; 3],
        }
    }
}

impl ObjectArena {
    /// Native 0048C6F0 traversal begins at head.next and follows next links.
    pub fn map_annotation_list(&self) -> &[ObjectId] {
        &self.map_annotations
    }

    pub fn map_annotation(&self, object: ObjectId) -> Result<&MapAnnotationState, ObjectError> {
        match &self.object(object)?.class {
            ObjectClass::MapAnnotation(state) => Ok(state),
            _ => Err(ObjectError::NotMapAnnotation(object)),
        }
    }

    fn map_annotation_mut(
        &mut self,
        object: ObjectId,
    ) -> Result<&mut MapAnnotationState, ObjectError> {
        match &mut self.owned_mut(object)?.state.class {
            ObjectClass::MapAnnotation(state) => Ok(state),
            _ => Err(ObjectError::NotMapAnnotation(object)),
        }
    }

    /// Exact generalized virtual +04 consumer. The canonical decoder bounds
    /// only the reached WORD/byte; rejected and base commands read no payload.
    pub fn apply_map_annotation_command(
        &mut self,
        object: ObjectId,
        command: &InstanceCommandView,
    ) -> Result<bool, ObjectConstructionError> {
        self.map_annotation(object)?;
        use InstanceCommandPayload::*;
        match (command.code.value, &command.payload) {
            (0 | 1, _) => Ok(true),
            (2, MapAnnotationLabel(value)) => {
                self.map_annotation_mut(object)?.label = u32::from(value.value);
                // 0048C7C8 writes +4C before unlinking then appending +40.
                // Repeated and zero-label calls still move the node.
                let index = self
                    .map_annotations
                    .iter()
                    .position(|id| *id == object)
                    .expect("annotation node");
                self.map_annotations.remove(index);
                self.map_annotations.push(object);
                Ok(true)
            }
            (code @ 3..=5, MapAnnotationColor { channel, value })
                if i32::from(*channel) == code - 3 =>
            {
                self.map_annotation_mut(object)?.rgb[usize::from(*channel)] = value.value;
                Ok(true)
            }
            (6, MapAnnotationRadius(value)) => {
                self.map_annotation_mut(object)?.radius = value.value;
                Ok(true)
            }
            (2..=6, _) => Err(super::ActorCommandError::Payload(command.code.value).into()),
            _ => Ok(false),
        }
    }
}

#[cfg(test)]
mod tests;
