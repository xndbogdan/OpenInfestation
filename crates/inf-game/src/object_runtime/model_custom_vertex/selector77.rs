//! Selector77's actual virtual+10 at 004074E0, bounded LOWBYTE2/10.
//! Default terrain output remains a separate required source phase.

use super::*;

impl ObjectArena {
    pub(super) fn capture_selector77_model_vertex(
        &mut self,
        request: ObjectCustomVertexRequest<'_>,
    ) -> Result<(), ObjectCustomVertexError> {
        let source = request.source;
        let selector = request.compute.selector & 0xff;
        if !matches!(selector, 2 | 10) {
            return Err(ObjectCustomVertexError::Unowned {
                object: source,
                selector: request.compute.selector,
            });
        }
        let index = request.compute.extras.first().copied().ok_or(
            ObjectCustomVertexError::RequiredExtra {
                object: source,
                selector: request.compute.selector,
                index: 0,
            },
        )?;
        let world = request.vertices.row_words_439e10(index)?;
        if selector == 10 {
            //40754E..407586 uses a local row and publishes only signed XYZ.
            //It reads neither actor position nor behavior, and never stores74.
            *request.output = Some(world.map(|axis| i32::from(axis as i16)));
            return Ok(());
        }
        //40758F/40759F lends actual192/194/196 to439E10 before the
        //fresh140 test and optional extra1. Complete-known row custody is the
        //existing port domain; arbitrary partial native pointer faults stay open.
        let DerivedActorClass::Selector77(state) = &mut self.derived_actor_mut(source)?.class
        else {
            unreachable!("actual source callback checked");
        };
        state.position_192 = Some(world);
        if self.derived_actor(source)?.behavior_140.is_some() {
            let group = request.compute.extras.get(1).copied().ok_or(
                ObjectCustomVertexError::RequiredExtra {
                    object: source,
                    selector: request.compute.selector,
                    index: 1,
                },
            )?;
            //The existing complete48CF70 owner keeps signed group-count,
            //six-WORD backing, world/relative prefixes and real actor basis.
            self.capture_model_behavior_group(source, group, world)?;
        }
        //4075BC..4075D7 rereads the retained capture after48CF70, then
        //4075DA..4075E5 reads actorY and writes74 after all output stores.
        //The complete helper cannot replace or clear selector77's192 capture.
        let DerivedActorClass::Selector77(state) = &self.derived_actor(source)?.class else {
            unreachable!("actual source callback checked");
        };
        let world = state.position_192.expect("reached439E10 capture");
        *request.output = Some(world.map(|axis| i32::from(axis as i16)));
        let actor_y = self.object(source)?.base.position_words.ok_or(
            ObjectCustomVertexError::RequiredField {
                object: source,
                offset: 0x34,
            },
        )?[1];
        self.actor_mut(source)?.word_74 = world[1].wrapping_sub(actor_y);
        Ok(())
    }
}
