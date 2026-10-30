//! A vehicle's recorded drive state: every field the mode drives read, as
//! the mode receipts write it into the port's vehicle and the scenario lab
//! into retail's (`lab/snapshot.py`'s `VEHICLE_FIELDS`), from the native
//! vehicle's DWORDs.

use crate::object_runtime::{ObjectArena, ObjectId};

impl ObjectArena {
    /// The `+2C` flags `0x60000000`, the position `+32`, the basis `+44`,
    /// the velocity `+8E`, `+94..+9C` and the drive's own words, each read
    /// through `dword` (the native DWORD at an offset). Pointer and sound
    /// fields stay the port's own.
    pub fn inject_vehicle_drive_state(
        &mut self,
        vehicle: ObjectId,
        dword: impl Fn(u32) -> u32,
    ) -> Result<(), String> {
        let low = |offset| dword(offset) as u16;
        let high = |offset| (dword(offset) >> 16) as u16;
        {
            let base = &mut self
                .owned_mut(vehicle)
                .map_err(|error| error.to_string())?
                .state
                .base;
            base.flags = (base.flags & !0x6000_0000) | (dword(0x2c) & 0x6000_0000);
            base.position_words = Some([high(0x30), low(0x34), high(0x34)]);
        }
        {
            let actor = self.actor_mut(vehicle).map_err(|error| error.to_string())?;
            for index in 0..10 {
                actor.rotating.basis.words[index] = dword(0x44 + index as u32 * 4);
            }
            actor.words_8e_92 = [high(0x8c), low(0x90), high(0x90)];
            actor.words_94_9c = [dword(0x94), dword(0x98), dword(0x9c)];
        }
        self.derived_actor_mut(vehicle)
            .map_err(|error| error.to_string())?
            .byte_160 = dword(0x160) as u8;
        let state = self
            .selector2_actor_mut(vehicle)
            .map_err(|error| error.to_string())?;
        state.words_188_194 = [0x188, 0x18c, 0x190, 0x194].map(&dword);
        state.drive.normal_198 = Some([0x198, 0x19c, 0x1a0].map(&dword));
        state.byte_1c0 = dword(0x1c0) as u8;
        state.dwords_1e8_1f4 = [0x1e8, 0x1ec, 0x1f0, 0x1f4].map(&dword);
        state.drive.dword_1fc = Some(dword(0x1fc));
        state.bytes_20a_20c[2] = dword(0x20c) as u8;
        state.drive.dword_21c = Some(dword(0x21c));
        state.dwords_220_224 = [dword(0x220), dword(0x224)];
        state.dword_228 = Some(dword(0x228));
        state.drive.words_22c_236 = Some([
            low(0x22c),
            high(0x22c),
            low(0x230),
            high(0x230),
            low(0x234),
            high(0x234),
        ]);
        state.dword_238 = dword(0x238);
        state.drive.dword_23c = Some(dword(0x23c));
        state.drive.dword_240 = Some(dword(0x240));
        state.dword_244 = dword(0x244);
        state.words_24c_24e = [low(0x24c), high(0x24c)];
        state.dwords_250_25c = [0x250, 0x254, 0x258, 0x25c].map(&dword);
        state.dword_260 = Some(dword(0x260));
        state.dwords_538_554[6] = dword(0x550);
        state.drive.dword_55c = Some(dword(0x55c));
        state.drive.dword_560 = Some(dword(0x560));
        state.dword_564 = dword(0x564);
        state.dword_568 = dword(0x568);
        state.drive.dword_574 = Some(dword(0x574));
        state.drive.dword_578 = Some(dword(0x578));
        state.drive.dword_57c = Some(dword(0x57c));
        state.drive.word_58c = Some(low(0x58c));
        state.dwords_5ac_5b0[0] = dword(0x5ac);
        Ok(())
    }
}
