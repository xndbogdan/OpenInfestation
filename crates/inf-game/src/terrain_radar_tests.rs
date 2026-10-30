use super::*;
use crate::name_entry::name_entry_rng_next;

/// `check_terrain_radar.py`'s input stream.
struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        self.0
    }

    fn below(&mut self, n: u32) -> u32 {
        (self.next() >> 8) % n
    }
}

struct Inputs {
    materials: std::collections::HashMap<i32, u32>,
    samples: Vec<[i16; 8]>,
    masks: Vec<i8>,
    overlay: Vec<i8>,
    map: Vec<u8>,
    pixels: Vec<u16>,
}

/// The oracle's draws, in its order.
fn inputs(case: &serde_json::Value) -> Inputs {
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    let mut gen = Lcg(int(&case["seed"]) as u32);
    let [width, height] = [0, 1].map(|axis| int(&case["size"][axis]) as usize);
    let slots = [0x0f, 0x1f, 0x20, 0x21, 0x22, 0x3c]
        .into_iter()
        .chain(OVERLAY_SLOTS)
        .chain(TERRAIN_MATERIAL_SLOTS);
    let mut materials: std::collections::HashMap<i32, u32> =
        slots.map(|slot| (slot, gen.next())).collect();
    if let Some(mask) = case["mask"].as_u64() {
        materials.insert(MASK_SLOT, mask as u32);
    }
    let words: Vec<i16> = (0..width * height * 8)
        .map(|_| (gen.next() >> 16) as u16 as i16)
        .collect();
    let samples = words
        .chunks_exact(8)
        .map(|cell| cell.try_into().unwrap())
        .collect();
    let masks = (0..width * height)
        .map(|_| match gen.below(32) {
            0..=11 => 0,
            12..=15 => 15,
            _ => (gen.next() >> 24) as u8 as i8,
        })
        .collect();
    let overlay = (0..4 * width * height)
        .map(|_| gen.below(16) as i8)
        .collect();
    let map = (0..0x10000).map(|_| gen.below(43) as u8).collect();
    let [fb_width, fb_height] = [0, 1].map(|axis| int(&case["surface"][axis]) as usize);
    let pixels = (0..fb_width * fb_height)
        .map(|_| (gen.next() >> 16) as u16)
        .collect();
    Inputs {
        materials,
        samples,
        masks,
        overlay,
        map,
        pixels,
    }
}

/// Every `00470170` case of `terrain_radar_native.json` (the original
/// routine under Unicorn, `check_terrain_radar.py`): the pixels it changed,
/// the RNG it left and whether its contents walk followed, over all four
/// reflections, the mask, rim and noise branches (and a draw equal to the
/// jitter, which does not jam), the clip rectangle's four edges, a zero
/// heading, the width-reflected bottom rows, and RGB565.
#[test]
fn passes_match_the_original_routine() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/terrain_radar_native.json");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 25);
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    let mut drawn_rim = false;
    let mut drawn_noise = false;
    for case in cases {
        let name = case["name"].as_str().unwrap();
        let inputs = inputs(case);
        let material = |slot: i32| inputs.materials[&slot];
        let shifts = [0, 1, 2].map(|i| int(&case["shifts"][i]) as u32);
        let colours = TerrainRadarColours {
            terrain: terrain_colours(
                TERRAIN_MATERIAL_SLOTS.map(material),
                TERRAIN_EXTRA_SLOTS.map(material),
                shifts,
            ),
            overlay: std::array::from_fn(|i| material(OVERLAY_SLOTS.start + i as i32)),
            noise: NOISE_SLOTS.map(material),
            mask: material(MASK_SLOT),
        };
        let axis = [0, 1].map(|i| f32::from_bits(int(&case["axis"][i]) as u32));
        let eye = [0, 1].map(|i| int(&case["eye"][i]) as u32);
        let pass = TerrainRadarPass {
            grids: TerrainRadarGrids {
                size: [0, 1].map(|i| int(&case["size"][i]) as i32),
                samples: &inputs.samples,
                masks: &inputs.masks,
                overlay: &inputs.overlay,
            },
            colours,
            map: &inputs.map,
            centre: [0, 1].map(|i| int(&case["centre"][i]) as i32),
            viewport: [0, 1, 2, 3].map(|i| int(&case["viewport"][i]) as i16),
            eye_words: eye.map(|word| word as u16),
            rotation: terrain_heading_rotation([axis[0], 0.0, axis[1]]),
            jitter: int(&case["jitter"]) as i32,
        };
        let mut rng = int(&case["rng"]) as u32;
        let radar = terrain_radar(&pass, &mut rng, name_entry_rng_next).unwrap();
        assert_eq!(rng, int(&case["rng_after"]) as u32, "{name}");
        let fb_width = int(&case["surface"][0]) as i32;
        let mut pixels = inputs.pixels.clone();
        if let Some(radar) = &radar {
            for &([x, y], pixel) in &radar.pixels {
                assert!((0..fb_width).contains(&x), "{name}: column {x}");
                let at = (y * fb_width + x) as usize;
                pixels[at] = match pixel {
                    TerrainRadarPixel::Word(word) => word,
                    TerrainRadarPixel::Rim(terms) => {
                        drawn_rim = true;
                        radar.rim(terms, pixels[at])
                    }
                };
            }
            drawn_noise |= pass.jitter > 0;
        }
        let changes: Vec<[i64; 3]> = pixels
            .iter()
            .zip(&inputs.pixels)
            .enumerate()
            .filter(|(_, (after, before))| after != before)
            .map(|(at, (after, _))| {
                [
                    at as i64 % i64::from(fb_width),
                    at as i64 / i64::from(fb_width),
                    i64::from(*after),
                ]
            })
            .collect();
        let expected: Vec<[i64; 3]> = case["changes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|change| [0, 1, 2].map(|i| int(&change[i])))
            .collect();
        assert_eq!(changes, expected, "{name}");
        let walk = radar.filter(|radar| radar.contents).map(|radar| {
            (
                radar.origin.map(|value| i64::from(value as u16)),
                eye.map(i64::from),
            )
        });
        let expected = (!case["walk"].is_null()).then(|| {
            assert_eq!(case["walk"]["resource"], case["resource"], "{name}");
            (
                [0, 1].map(|i| int(&case["walk"]["origin"][i])),
                [0, 1].map(|i| int(&case["walk"]["eye"][i])),
            )
        });
        assert_eq!(walk, expected, "{name}");
    }
    assert!(drawn_rim && drawn_noise);
}

/// Both headings, from the original prefixes of `00470170` (to `004702F5`)
/// and `004723F0` (to `004724BF`): where a float-stored quotient truncates
/// differently, the terrain radar's sine and cosine differ from the
/// contents' by one.
#[test]
fn headings_match_both_original_roundings() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/terrain_radar_native.json");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    let rotations = fixture["rotations"].as_array().unwrap();
    let mut differing = 0;
    for rotation in rotations {
        let [x, z] = [0, 1].map(|i| f32::from_bits(int(&rotation["axis"][i]) as u32));
        let expect = |key: &str| [0, 1].map(|i| int(&rotation[key][i]) as i32);
        assert_eq!(
            terrain_heading_rotation([x, 0.0, z]),
            expect("terrain"),
            "{x} {z}"
        );
        assert_eq!(
            crate::radar_blips::heading_rotation([x, 0.0, z]),
            expect("contents"),
            "{x} {z}"
        );
        differing += usize::from(expect("terrain") != expect("contents"));
    }
    assert!(differing > 0 && rotations.len() > differing);
}

#[test]
fn colours_scale_each_channel_by_the_shade() {
    // White in RGB555 ([6, 1, 4] material shifts): 0xF8 * shade / 8, held
    // to 0x7F, keeps its top five bits.
    let table = terrain_colours([0x7fff; 5], [1, 2, 3], [6, 1, 4]);
    let grey = |five: u32| five << 10 | five << 5 | five;
    assert_eq!(table[0], grey(0x5d >> 3));
    assert_eq!(table[5], grey(0x7c >> 3));
    assert_eq!(table[10], grey(0x7f >> 3));
    assert_eq!(table[39], grey(0x7f >> 3));
    assert_eq!(table[40..], [1, 2, 3]);
}

#[test]
fn a_zero_heading_reads_as_due_north() {
    assert_eq!(terrain_heading_rotation([0.0; 3]), [32768, 0]);
    assert_eq!(terrain_heading_rotation([1.0, 0.0, 0.0]), [32768, 0]);
    assert_eq!(terrain_heading_rotation([0.0, 0.0, 1.0]), [0, -32768]);
}

fn hex_bytes(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|at| u8::from_str_radix(&text[at..at + 2], 16).unwrap())
        .collect()
}

fn retail_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail")
}

/// The process map 0054ED78 as 00471AE0 built it for the recorded world:
/// the player's own terrain (`retail/`) and the recorded RNG word (only its
/// low 17 bits reach the map), checked against the recorded map's SHA-256.
fn recorded_map(record: &serde_json::Value) -> Vec<u8> {
    use crate::{
        terrain_map::{TerrainMap, TerrainMapPlane},
        terrain_runtime::TerrainSpacing,
        world_placement::WorldPlacements,
        HandlerResources, Installation, ResidencyRequest, ResidentRuntime, ResourceSelection,
    };
    use sha2::{Digest, Sha256};
    let mut runtime = ResidentRuntime::new(Installation::open(&retail_root()).unwrap()).unwrap();
    runtime
        .apply(ResidencyRequest::Select {
            selection: ResourceSelection {
                difficulty: 2,
                language: 0,
            },
        })
        .unwrap();
    runtime.apply(ResidencyRequest::Boot).unwrap();
    runtime
        .apply(ResidencyRequest::Activate { level: 48 })
        .unwrap();
    let level = record["map_level"].as_u64().unwrap() as u8;
    runtime.apply(ResidencyRequest::Activate { level }).unwrap();
    let placements = WorldPlacements::new(&runtime, TerrainSpacing::Units256).unwrap();
    let source = placements.terrain_source();
    let HandlerResources::Terrain(bank) =
        &source.overlay.handlers()[source.registration.region_index]
    else {
        panic!("registered terrain");
    };
    let raw = &bank.entries[usize::from(source.registration.local_index)];
    let plane =
        TerrainMapPlane::from_world_flags(placements.world().flags, raw.prefix.plane_height_fixed);
    let mut rng = record["map_seed"].as_u64().unwrap() as u32;
    let map = TerrainMap::new(placements.terrain(), plane, &mut rng)
        .indices()
        .to_vec();
    let digest: String = Sha256::digest(&map)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(digest, record["map_sha256"].as_str().unwrap());
    map
}

/// H10 global slots 0..2 of the repository's `Overlay/<name>`, through the
/// PreLoad registration the port's residency resolves them by.
fn overlay_grids(name: &str) -> [inf_formats::SampleGrid; 3] {
    use inf_formats::SampleGridRole::{EightWordSamples, FourSampleMask, MaterialIndices};
    let root = retail_root();
    let preload =
        inf_formats::Preload::parse(&std::fs::read(root.join("PreLoad.dat")).unwrap()).unwrap();
    let key = inf_formats::OverlayKey::parse_filename(name).unwrap();
    let requests = preload
        .leaf(key.level)
        .unwrap()
        .requests(key.request_kind());
    let bytes = std::fs::read(root.join("Overlay").join(name)).unwrap();
    let layout = inf_formats::parse_overlay(&bytes, requests).unwrap();
    let region = layout
        .regions
        .iter()
        .find(|region| region.handler.get() == inf_formats::HANDLER_10)
        .unwrap();
    let bank = inf_formats::parse_handler10_bank(&bytes, region).unwrap();
    let request = requests.get(inf_formats::HANDLER_10 as usize).unwrap();
    let grid = |slot: u16, role| {
        let record = bank
            .records
            .iter()
            .find(|record| request.global_index(record.local_index as u16) == Some(slot))
            .unwrap();
        inf_formats::decode_sample_grid(&bytes, bank.payload, record, role).unwrap()
    };
    [
        grid(0, EightWordSamples),
        grid(1, FourSampleMask),
        grid(2, MaterialIndices),
    ]
}

/// Infest03's terrain radars (`terrain-radar.js`): retail's inputs, its own
/// colours (an RGB565 display) and process map, with the grids of the
/// overlay it ran read from the repository. Every WORD the routine stored,
/// in order and with its position, the destination WORDs its rim pixels
/// read, and the RNG it left must match.
#[test]
fn infest03_radars_match_retail() {
    use inf_formats::SampleGridValues::{SignedBytes, SignedWords8};
    if !retail_root().join("PreLoad.dat").is_file() {
        eprintln!("retail game data missing; copy the game into retail/ to run this test");
        return;
    }
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/terrain_radar_infest03.json");
    let records: Vec<serde_json::Value> =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let int = |value: &serde_json::Value| value.as_i64().unwrap();
    let mut checked = 0;
    for record in &records {
        let [samples, masks, overlay] = overlay_grids(record["overlay"].as_str().unwrap());
        let (SignedWords8(samples_values), SignedBytes(mask_values), SignedBytes(overlay_values)) =
            (&samples.samples, &masks.samples, &overlay.samples)
        else {
            panic!("H10 slots 0..2 hold other layouts");
        };
        let map = recorded_map(record);
        for call in record["calls"].as_array().unwrap() {
            let at = call["at"].as_str().unwrap();
            let h7: std::collections::HashMap<i32, u32> = call["h7"]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| (int(&pair[0]) as i32, int(&pair[1]) as u32))
                .collect();
            let material = |slot: i32| h7[&slot];
            let colours = TerrainRadarColours {
                terrain: terrain_colours(
                    TERRAIN_MATERIAL_SLOTS.map(material),
                    TERRAIN_EXTRA_SLOTS.map(material),
                    [0, 1, 2].map(|i| int(&call["shifts"][i]) as u32),
                ),
                overlay: std::array::from_fn(|i| material(OVERLAY_SLOTS.start + i as i32)),
                noise: NOISE_SLOTS.map(material),
                mask: material(MASK_SLOT),
            };
            let axis = [0, 1].map(|i| f32::from_bits(int(&call["axis"][i]) as u32));
            let pass = TerrainRadarPass {
                grids: TerrainRadarGrids {
                    size: [0, 1].map(|i| int(&call["size"][i]) as i32),
                    samples: samples_values,
                    masks: mask_values,
                    overlay: overlay_values,
                },
                colours,
                map: &map,
                centre: [0, 1].map(|i| int(&call["centre"][i]) as i32),
                viewport: [0, 1, 2, 3].map(|i| int(&call["viewport"][i]) as i16),
                eye_words: [0, 1].map(|i| int(&call["eye"][i]) as u16),
                rotation: terrain_heading_rotation([axis[0], 0.0, axis[1]]),
                jitter: int(&call["jitter"]) as i32,
            };
            assert_eq!(
                [samples.width, samples.height].map(|value| value as i64),
                [0, 1].map(|i| int(&call["size"][i])),
                "{at}"
            );
            let mut rng = int(&call["rng"]) as u32;
            let radar = terrain_radar(&pass, &mut rng, name_entry_rng_next).unwrap();
            assert_eq!(rng, int(&call["rng_after"]) as u32, "{at}");
            let Some(radar) = radar else {
                assert_eq!(call["stopped_at"], "41e3c8", "{at}");
                continue;
            };
            assert_eq!(call["stopped_at"], "471989", "{at}");
            assert_eq!(radar.contents, pass.jitter < 0x4000);
            let reads: Vec<([i32; 2], u16)> = call["reads"]
                .as_array()
                .unwrap()
                .iter()
                .map(|read| {
                    (
                        [int(&read[0]) as i32, int(&read[1]) as i32],
                        int(&read[2]) as u16,
                    )
                })
                .collect();
            let rims: Vec<[i32; 2]> = radar
                .pixels
                .iter()
                .filter(|(_, pixel)| matches!(pixel, TerrainRadarPixel::Rim(_)))
                .map(|&(position, _)| position)
                .collect();
            assert_eq!(
                rims,
                reads
                    .iter()
                    .map(|&(position, _)| position)
                    .collect::<Vec<_>>(),
                "{at}"
            );
            let destinations: std::collections::HashMap<[i32; 2], u16> =
                reads.into_iter().collect();
            let stored: Vec<u8> = radar
                .pixels
                .iter()
                .flat_map(|&(position, pixel)| {
                    let value = match pixel {
                        TerrainRadarPixel::Word(word) => word,
                        TerrainRadarPixel::Rim(terms) => radar.rim(terms, destinations[&position]),
                    };
                    [position[0] as u16, position[1] as u16, value]
                        .into_iter()
                        .flat_map(u16::to_le_bytes)
                })
                .collect();
            let expected = hex_bytes(call["writes"].as_str().unwrap());
            assert_eq!(stored.len(), expected.len(), "{at}");
            assert!(stored == expected, "{at}: the stored WORDs differ");
            checked += 1;
        }
    }
    assert!(checked > 0);
}
