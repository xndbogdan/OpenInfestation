//! Complete raw 004EBF00 phase used by auxiliary07. Shared exact vertex pool
//! writers/projector error commits; direct-table world_stream is not used.
use super::*;
impl ModelVertices {
    pub(crate) fn execute_auxiliary_raw<P: ModelVertexProjector>(
        &mut self,
        model: &ModelEntry,
        context: &ModelContext,
        command: ModelVertexCommand,
        destination: i16,
        projector: &mut P,
    ) -> Result<(), ModelVertexError<Infallible, P::Error>> {
        self.ensure_running()?;
        let result = (|| {
            let ModelVertexCommand::AuxiliaryOpcode(7) = command else {
                return Err(ModelVertexFault::InvalidInstruction { opcode: 7 }.into());
            };
            //Native unsignedBYTE7<<8, unlike signed renderer WORD commands.
            let mut modes = 7_u32;
            let index = i32::from(destination) >> 1;
            let raw = usize::try_from(index)
                .ok()
                .and_then(|i| model.vertices.get(i))
                .ok_or(ModelVertexFault::InvalidSlot {
                    index,
                    count: model.vertices.len(),
                })?;
            //All signed source WORDs are read even before a zero-mode test.
            let mut local = [i32::from(raw.x), i32::from(raw.y), i32::from(raw.z)];
            let mut pass = 0;
            while modes != 0 {
                if modes & 3 != 0 {
                    let to = self.checked_index(i32::from(destination) + pass)?;
                    if modes & 1 != 0 {
                        self.write_world(to, transform_world(context, local))?;
                    }
                    if modes & 2 != 0 {
                        self.project(to, ModelVertexInput::Local(local), projector)?;
                    }
                }
                modes >>= 2;
                pass += 1;
                local[0] = local[0].wrapping_neg();
            }
            Ok(())
        })();
        self.finish(result)
    }
}
