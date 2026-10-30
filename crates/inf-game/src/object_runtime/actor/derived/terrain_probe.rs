//! Packed cache 00454B30/00454ED0, destroyed by 00454B90. Actual terrain and
//! collision sampling 00429EE0 remains a synchronous required miss service.

mod access;
mod allocation;
pub use access::TerrainProbeError;
pub use allocation::{FamilyTerrainProbeError, TerrainProbeAllocation};

/// 00454B30 owns one packed terrain-probe allocation. The header's unwritten
/// bytes +08/+0B remain unknown; they are not portable zero-initialized state.
/// Private storage retains native DWORD shift and encoded sample policies.
/// Dropping this owner releases the inner buffer,
/// matching 00454B90's single free of the pointer at +00.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainProbeCache {
    words_00: Box<[u32]>,
    word_04: u16,
    word_06: u16,
    byte_08: Option<u8>,
    dimension_09: u8,
    stride_words_0a: u8,
    byte_0b: Option<u8>,
}

impl TerrainProbeCache {
    /// The retail argument is a BYTE. Only 8 and 16 survive normalization;
    /// every other value uses 8. Each DWORD holds eight four-bit samples, so
    /// doubled dimensions 16/32 require strides of 2/4 DWORDs and allocations
    /// of 128/512 bytes. The constructor calls 00454C20 with BYTE 1, clearing
    /// every DWORD without touching the header's remaining unknown bytes.
    pub fn new(input: u8) -> Self {
        let half_dimension = match input {
            8 | 16 => input,
            _ => 8,
        };
        let dimension = half_dimension * 2;
        let stride_words = dimension / 8;
        Self {
            words_00: vec![0; usize::from(dimension) * usize::from(stride_words)]
                .into_boxed_slice(),
            word_04: 0,
            word_06: 0,
            byte_08: None,
            dimension_09: dimension,
            stride_words_0a: stride_words,
            byte_0b: None,
        }
    }

    pub fn word_04(&self) -> u16 {
        self.word_04
    }

    pub fn word_06(&self) -> u16 {
        self.word_06
    }

    pub fn byte_08(&self) -> Option<u8> {
        self.byte_08
    }

    pub fn dimension(&self) -> u8 {
        self.dimension_09
    }

    pub fn stride_words(&self) -> u8 {
        self.stride_words_0a
    }

    pub fn byte_0b(&self) -> Option<u8> {
        self.byte_0b
    }

    pub fn words(&self) -> &[u32] {
        &self.words_00
    }
}

#[cfg(test)]
mod tests {
    use super::TerrainProbeCache;
    use serde_json::{json, Value};

    #[test]
    fn constructor_matches_native_sizes_normalization_and_cleared_storage() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/terrain_probe_native.json"
        )))
        .unwrap();
        let mut tested_inputs = vec![];
        for case in fixture["cases"].as_array().unwrap() {
            let input: u8 = case["input"].as_u64().unwrap().try_into().unwrap();
            let cache = TerrainProbeCache::new(input);
            let snapshot = json!({
                "word_04": cache.word_04(),
                "word_06": cache.word_06(),
                "byte_08": cache.byte_08(),
                "byte_09": cache.dimension(),
                "byte_0a": cache.stride_words(),
                "byte_0b": cache.byte_0b(),
                "words_00": cache.words(),
            });
            assert_eq!(snapshot, case["state"], "terrain probe input {input}");
            // The fixture observes native allocation/free call boundaries;
            // it supplies the buffer between them without executing either
            // allocator. Both observed extents must match our owned storage.
            let storage_bytes = std::mem::size_of_val(cache.words()) as u64;
            for boundary in case["heap_boundaries"].as_array().unwrap() {
                assert_eq!(boundary["owner"], "words_00");
                assert_eq!(boundary["size"].as_u64().unwrap(), storage_bytes);
            }
            tested_inputs.push(input);
        }
        assert_eq!(tested_inputs, [0, 8, 16, 255]);
    }
}
