//! The mining rig's update over the world's owners: its calls to the mines,
//! `0047DD20(radius, 0xF, 0)`, reach every actor within the radius through
//! the world's message delivery, and the answers add up.

use crate::object_runtime::{
    ActorMessageWalk, ObjectArena, ObjectId, Selector52Host, SELECTOR52_MINE_CALL,
};

use super::{broadcast_world_actor_message, WorldFamilyUpdateHost};

pub(crate) struct WorldRigHost<'a, 'exe> {
    pub family: WorldFamilyUpdateHost<'a, 'exe>,
}

impl Selector52Host for WorldRigHost<'_, '_> {
    type Error = String;

    fn call_mines(
        &mut self,
        arena: &mut ObjectArena,
        rig: ObjectId,
        radius: u32,
    ) -> Result<u32, String> {
        let family = &mut self.family;
        broadcast_world_actor_message(
            arena,
            rig,
            ActorMessageWalk::Radius { radius },
            SELECTOR52_MINE_CALL,
            0,
            &mut family.owners,
            None,
            &mut *family.rng,
        )
    }
}
