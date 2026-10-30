//! Tests: a recorded vehicle's bytes `+000..+5FF` (`vehicle-falls.js`)
//! written over the port's vehicle, for the session's receipt tests.

use super::*;
use crate::object_runtime::PlayerId;

impl ObjectArena {
    /// Writes `bytes`, the vehicle's `+000..+5FF`, into every field the
    /// world contact pass, the response `0047D2F0`, the damage `004212F0`
    /// and the model's values read: flags, position, model words, basis,
    /// motion, health and state, the form and its morph, and the drive's
    /// fields, with the update's held handle `+598` as recorded. The hold's
    /// handles, the lists, the sounds and the behavior table stay the live
    /// vehicle's.
    pub(crate) fn inject_vehicle_snapshot_for_tests(&mut self, vehicle: ObjectId, bytes: &[u8]) {
        assert_eq!(bytes.len(), 0x600);
        let dword =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let low = |offset: usize| dword(offset) as u16;
        let high = |offset: usize| (dword(offset) >> 16) as u16;
        assert_eq!(self.object(vehicle).unwrap().base.word_28, low(0x28));
        {
            let base = &mut self.owned_mut(vehicle).unwrap().state.base;
            base.flags = dword(0x2c);
            base.position_words = Some([high(0x30), low(0x34), high(0x34)]);
        }
        {
            let actor = self.actor_mut(vehicle).unwrap();
            actor.rotating.visual.radius_word = low(0x40);
            actor.rotating.visual.model_word = high(0x40);
            for index in 0..10 {
                actor.rotating.basis.words[index] = dword(0x44 + index * 4);
            }
            actor.word_8c = low(0x8c);
            actor.words_8e_92 = [high(0x8c), low(0x90), high(0x90)];
            actor.words_94_9c = [dword(0x94), dword(0x98), dword(0x9c)];
            actor.health = low(0xa0);
            actor.word_c4 = dword(0xc4);
            actor.word_c8 = low(0xc8);
            actor.word_cc = low(0xcc);
            actor.word_ce = high(0xcc);
            actor.byte_d0 = bytes[0xd0];
        }
        self.derived_actor_mut(vehicle).unwrap().byte_160 = bytes[0x160];
        let state = self.selector2_actor_mut(vehicle).unwrap();
        state.word_184 = Some(dword(0x184));
        state.words_188_194 = [0x188, 0x18c, 0x190, 0x194].map(dword);
        state.drive.normal_198 = Some([0x198, 0x19c, 0x1a0].map(dword));
        state.byte_1c0 = bytes[0x1c0];
        state.words_1c2_1c8 = [high(0x1c0), low(0x1c4), high(0x1c4), low(0x1c8)];
        state.dwords_1e8_1f4 = [0x1e8, 0x1ec, 0x1f0, 0x1f4].map(dword);
        state.drive.dword_1fc = Some(dword(0x1fc));
        state.dwords_200_204 = [dword(0x200), dword(0x204)];
        state.byte_208 = bytes[0x208];
        state.byte_209 = bytes[0x209];
        state.bytes_20a_20c = [bytes[0x20a], bytes[0x20b], bytes[0x20c]];
        state.word_20e = high(0x20c);
        state.dwords_210_218 = [dword(0x210), dword(0x214), dword(0x218)];
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
        state.dwords_250_25c = [0x250, 0x254, 0x258, 0x25c].map(dword);
        state.dword_260 = Some(dword(0x260));
        assert_eq!(dword(0x264), 0, "a native wake ribbon has no port object");
        state.ribbon_264 = None;
        state.dwords_52c_530 = [dword(0x52c), dword(0x530)];
        state.bytes_534_535 = [bytes[0x534], bytes[0x535]];
        for index in 0..8 {
            state.dwords_538_554[index] = dword(0x538 + index * 4);
        }
        state.dword_558 = dword(0x558);
        state.drive.dword_55c = Some(dword(0x55c));
        state.drive.dword_560 = Some(dword(0x560));
        state.dword_564 = dword(0x564);
        state.dword_568 = dword(0x568);
        state.words_56c_570 = [low(0x56c), high(0x56c), low(0x570)];
        state.drive.dword_574 = Some(dword(0x574));
        state.drive.dword_578 = Some(dword(0x578));
        state.drive.dword_57c = Some(dword(0x57c));
        state.dwords_580_584 = [dword(0x580), dword(0x584)];
        state.drive.word_58c = Some(low(0x58c));
        state.dword_590 = dword(0x590);
        state.update.byte_594 = Some(bytes[0x594]);
        state.update.handle_598 = Some(ActorHandle::from_raw(dword(0x598)));
        state.dwords_59c_5a0 = [dword(0x59c), dword(0x5a0)];
        state.dwords_5ac_5b0 = [dword(0x5ac), dword(0x5b0)];
    }

    /// Tests: the unit the player holds, `+D8`, which the player lookup
    /// `0046B090` matches before the vehicle `+D4`.
    pub(crate) fn set_player_held_unit_for_tests(&mut self, player: PlayerId, unit: ActorHandle) {
        self.world_player_mut(player).unwrap().handle_d8 = unit;
    }

    /// Tests: the vehicle's target `+148` (what `00402790` answers) on
    /// `target`, as a lock leaves it.
    pub(crate) fn lock_vehicle_for_tests(&mut self, vehicle: ObjectId, target: ActorHandle) {
        let reference = self
            .follow_target_resource(crate::object_runtime::FollowTargetRequest::resolve_actor(
                target,
            ))
            .unwrap();
        self.derived_actor_mut(vehicle).unwrap().target_148 = reference;
    }

    /// Tests: the player's Shielding level, byte `+142`.
    pub(crate) fn set_player_shielding_for_tests(&mut self, player: PlayerId, level: u8) {
        self.world_player_mut(player).unwrap().bytes_13c_143[6] = level;
    }

    /// Tests: an upgrade the player owns, its bit in `+100`.
    pub(crate) fn grant_player_upgrade_for_tests(&mut self, player: PlayerId, id: u32) {
        self.world_player_mut(player).unwrap().bits_100[(id >> 5) as usize] |= 1 << (id & 31);
    }

    /// Tests: a recorded behavior `+140`'s `+00..+3F` (`head`, sixteen
    /// DWORDs) and its entries (`+18`) written into the vehicle's: the
    /// spread and jitter (`+04`, `+08`), the selections (`+10`, `+11`) and
    /// the bytes beside them, the lead, sound and timers (`+20`, `+2C..+38`)
    /// and each entry's eight DWORDs. The fire chain, table, groups and
    /// sound record stay the live behavior's.
    pub(crate) fn inject_vehicle_behavior_for_tests(
        &mut self,
        vehicle: ObjectId,
        head: &[u32],
        entries: &[u32],
    ) {
        assert_eq!(head.len(), 16);
        let behavior = self
            .derived_actor_mut(vehicle)
            .unwrap()
            .behavior_140
            .as_mut()
            .unwrap();
        let [byte_0c, byte_0d, byte_0e, byte_0f] = head[3].to_le_bytes();
        assert_eq!((behavior.byte_0e, behavior.byte_0f), (byte_0e, byte_0f));
        assert_eq!(entries.len(), behavior.entries_18.len() * 8);
        behavior.word_04 = head[1];
        behavior.word_08 = head[2];
        behavior.byte_0c = byte_0c;
        behavior.byte_0d = byte_0d;
        let [byte_10, byte_11, _, _] = head[4].to_le_bytes();
        behavior.byte_10 = byte_10;
        behavior.byte_11 = byte_11;
        behavior.word_20 = head[8];
        behavior.word_2c = head[11];
        behavior.word_30 = Some(head[12]);
        behavior.word_34 = Some(head[13]);
        behavior.word_38 = head[14];
        for (index, entry) in behavior.entries_18.iter_mut().enumerate() {
            entry.copy_from_slice(&entries[index * 8..index * 8 + 8]);
        }
    }

    /// Tests: a recorded player's bytes `+000..+19F` written into the
    /// fields the vehicle's update and damage read: the message byte
    /// (`+88`), the grants (`+A0`), the upgrades (`+100`), the turbo's
    /// charges and timer (`+12C`, `+F8`), the commands (`+130`), the levels
    /// (`+13C`), and the counters and limits beside them. The control block
    /// and the handles stay the live player's.
    pub(crate) fn inject_player_snapshot_for_tests(&mut self, player: PlayerId, bytes: &[u8]) {
        assert_eq!(bytes.len(), 0x1a0);
        let dword =
            |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
        let state = self.world_player_mut(player).unwrap();
        state.set_message_byte_00(bytes[0x88]);
        for (index, word) in state.grants_9c.iter_mut().enumerate() {
            *word = dword(0xa0 + index * 4);
        }
        for (index, word) in state.bits_100.iter_mut().enumerate() {
            *word = dword(0x100 + index * 4);
        }
        state.dword_118 = dword(0x118);
        state.word_11e = (dword(0x11c) >> 16) as u16;
        for (index, word) in state.dwords_120_128.iter_mut().enumerate() {
            *word = dword(0x120 + index * 4);
        }
        state.dword_12c = dword(0x12c);
        for (index, word) in state.command_mask_130.iter_mut().enumerate() {
            *word = dword(0x130 + index * 4);
        }
        state.bytes_13c_143.copy_from_slice(&bytes[0x13c..0x144]);
        state.dword_144 = dword(0x144);
        state.dword_148 = dword(0x148);
        state.dword_14c = dword(0x14c);
        state.dword_f4 = dword(0xf4) as i32;
        state.dword_f8 = dword(0xf8) as i32;
        state.byte_fc = bytes[0xfc];
        for (index, value) in state.values_150.iter_mut().enumerate() {
            *value = dword(0x150 + index * 4) as i32;
        }
    }
}

/// The bytes a snapshot covers, laid out as native `+000..+5FF`.
struct Image {
    bytes: Vec<u8>,
    covered: Vec<bool>,
}

impl Image {
    fn new() -> Self {
        Self {
            bytes: vec![0; 0x600],
            covered: vec![false; 0x600],
        }
    }

    fn put(&mut self, offset: usize, value: &[u8]) {
        self.bytes[offset..offset + value.len()].copy_from_slice(value);
        self.covered[offset..offset + value.len()].fill(true);
    }

    fn dword(&mut self, offset: usize, value: u32) {
        self.put(offset, &value.to_le_bytes());
    }

    fn word(&mut self, offset: usize, value: u16) {
        self.put(offset, &value.to_le_bytes());
    }

    fn byte(&mut self, offset: usize, value: u8) {
        self.put(offset, &[value]);
    }
}

impl ObjectArena {
    /// Tests: every field [`Self::inject_vehicle_snapshot_for_tests`]
    /// writes that differs from `bytes`, as `(offset, port, recorded)` over
    /// the covered bytes of each DWORD.
    pub(crate) fn vehicle_snapshot_differences_for_tests(
        &self,
        vehicle: ObjectId,
        bytes: &[u8],
    ) -> Vec<(usize, u32, u32)> {
        let mut image = Image::new();
        let base = &self.object(vehicle).unwrap().base;
        image.dword(0x2c, base.flags);
        let [x, y, z] = base.position_words.unwrap();
        image.word(0x32, x);
        image.word(0x34, y);
        image.word(0x36, z);
        let actor = self.actor(vehicle).unwrap();
        image.word(0x40, actor.rotating.visual.radius_word);
        image.word(0x42, actor.rotating.visual.model_word);
        for (index, word) in actor.rotating.basis.words.iter().enumerate() {
            image.dword(0x44 + index * 4, *word);
        }
        image.word(0x8c, actor.word_8c);
        for (index, word) in actor.words_8e_92.iter().enumerate() {
            image.word(0x8e + index * 2, *word);
        }
        for (index, word) in actor.words_94_9c.iter().enumerate() {
            image.dword(0x94 + index * 4, *word);
        }
        image.word(0xa0, actor.health);
        image.dword(0xc4, actor.word_c4);
        image.word(0xc8, actor.word_c8);
        image.word(0xcc, actor.word_cc);
        image.word(0xce, actor.word_ce);
        image.byte(0xd0, actor.byte_d0);
        image.byte(0x160, self.derived_actor(vehicle).unwrap().byte_160);
        let state = self.selector2_actor(vehicle).unwrap();
        if let Some(word) = state.word_184 {
            image.dword(0x184, word);
        }
        for (index, word) in state.words_188_194.iter().enumerate() {
            image.dword(0x188 + index * 4, *word);
        }
        if let Some(normal) = state.drive.normal_198 {
            for (index, word) in normal.iter().enumerate() {
                image.dword(0x198 + index * 4, *word);
            }
        }
        image.byte(0x1c0, state.byte_1c0);
        for (index, word) in state.words_1c2_1c8.iter().enumerate() {
            image.word(0x1c2 + index * 2, *word);
        }
        for (index, word) in state.dwords_1e8_1f4.iter().enumerate() {
            image.dword(0x1e8 + index * 4, *word);
        }
        if let Some(word) = state.drive.dword_1fc {
            image.dword(0x1fc, word);
        }
        image.dword(0x200, state.dwords_200_204[0]);
        image.dword(0x204, state.dwords_200_204[1]);
        image.byte(0x208, state.byte_208);
        image.byte(0x209, state.byte_209);
        image.put(0x20a, &state.bytes_20a_20c);
        image.word(0x20e, state.word_20e);
        for (index, word) in state.dwords_210_218.iter().enumerate() {
            image.dword(0x210 + index * 4, *word);
        }
        if let Some(word) = state.drive.dword_21c {
            image.dword(0x21c, word);
        }
        image.dword(0x220, state.dwords_220_224[0]);
        image.dword(0x224, state.dwords_220_224[1]);
        if let Some(word) = state.dword_228 {
            image.dword(0x228, word);
        }
        if let Some(words) = state.drive.words_22c_236 {
            for (index, word) in words.iter().enumerate() {
                image.word(0x22c + index * 2, *word);
            }
        }
        image.dword(0x238, state.dword_238);
        if let Some(word) = state.drive.dword_23c {
            image.dword(0x23c, word);
        }
        if let Some(word) = state.drive.dword_240 {
            image.dword(0x240, word);
        }
        image.dword(0x244, state.dword_244);
        image.word(0x24c, state.words_24c_24e[0]);
        image.word(0x24e, state.words_24c_24e[1]);
        for (index, word) in state.dwords_250_25c.iter().enumerate() {
            image.dword(0x250 + index * 4, *word);
        }
        if let Some(word) = state.dword_260 {
            image.dword(0x260, word);
        }
        image.dword(0x264, u32::from(state.ribbon_264.is_some()));
        image.dword(0x52c, state.dwords_52c_530[0]);
        image.dword(0x530, state.dwords_52c_530[1]);
        image.put(0x534, &state.bytes_534_535);
        for (index, word) in state.dwords_538_554.iter().enumerate() {
            // +548 is the skid sound's record, not a value.
            if index != 4 {
                image.dword(0x538 + index * 4, *word);
            }
        }
        image.dword(0x558, state.dword_558);
        if let Some(word) = state.drive.dword_55c {
            image.dword(0x55c, word);
        }
        if let Some(word) = state.drive.dword_560 {
            image.dword(0x560, word);
        }
        image.dword(0x564, state.dword_564);
        image.dword(0x568, state.dword_568);
        for (index, word) in state.words_56c_570.iter().enumerate() {
            image.word(0x56c + index * 2, *word);
        }
        for (offset, word) in [
            (0x574, state.drive.dword_574),
            (0x578, state.drive.dword_578),
            (0x57c, state.drive.dword_57c),
        ] {
            if let Some(word) = word {
                image.dword(offset, word);
            }
        }
        image.dword(0x580, state.dwords_580_584[0]);
        image.dword(0x584, state.dwords_580_584[1]);
        if let Some(word) = state.drive.word_58c {
            image.word(0x58c, word);
        }
        image.dword(0x590, state.dword_590);
        if let Some(byte) = state.update.byte_594 {
            image.byte(0x594, byte);
        }
        if let Some(handle) = state.update.handle_598 {
            image.dword(0x598, handle.raw());
        }
        image.dword(0x59c, state.dwords_59c_5a0[0]);
        image.dword(0x5a0, state.dwords_59c_5a0[1]);
        image.dword(0x5ac, state.dwords_5ac_5b0[0]);
        image.dword(0x5b0, state.dwords_5ac_5b0[1]);
        let mut out = Vec::new();
        for offset in (0..0x600).step_by(4) {
            let mask = (0..4).fold(0u32, |mask, byte| {
                if image.covered[offset + byte] {
                    mask | 0xff << (byte * 8)
                } else {
                    mask
                }
            });
            if mask == 0 {
                continue;
            }
            let port = u32::from_le_bytes(image.bytes[offset..offset + 4].try_into().unwrap());
            let native = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            if port & mask != native & mask {
                out.push((offset, port & mask, native & mask));
            }
        }
        out
    }
}
