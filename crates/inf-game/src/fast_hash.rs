//! A fast hasher for the port's hot in-memory maps (rustc's "Fx" hash):
//! one rotate, xor and multiply a word. Not resistant to chosen keys, which
//! none of these maps take from outside; std's SipHash cost about 4% of a
//! traced frame (docs/PERFORMANCE.md). Iteration order is fixed for a given
//! set of insertions, where SipHash's random keys varied it run to run.

use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasherDefault, Hasher},
};

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

#[derive(Clone, Copy, Debug, Default)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            self.add(u64::from_le_bytes(chunk.try_into().expect("eight bytes")));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }
    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.add(u64::from(value));
    }
    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.add(u64::from(value));
    }
    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(u64::from(value));
    }
    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }
    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
    #[inline]
    fn write_i16(&mut self, value: i16) {
        self.add(value as u16 as u64);
    }
    #[inline]
    fn write_i32(&mut self, value: i32) {
        self.add(value as u32 as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type FastMap<K, V> = HashMap<K, V, FxBuild>;
pub type FastSet<K> = HashSet<K, FxBuild>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn hash(value: impl Hash) -> u64 {
        let mut hasher = FxHasher::default();
        value.hash(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn equal_keys_hash_alike_and_near_keys_apart() {
        assert_eq!(hash((3_u16, 7_i16)), hash((3_u16, 7_i16)));
        let spread: FastSet<u64> = (0_u32..10_000).map(hash).collect();
        assert_eq!(spread.len(), 10_000);
        let mut map: FastMap<[u32; 3], u32> = FastMap::default();
        map.insert([1, 2, 3], 4);
        assert_eq!(map.get(&[1, 2, 3]), Some(&4));
    }
}
