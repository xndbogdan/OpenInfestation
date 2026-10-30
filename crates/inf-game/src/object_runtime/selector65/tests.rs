use super::*;
use crate::object_runtime::{ObjectClassKind, ObjectList};
use serde_json::Value;

fn u(value: &Value) -> u32 {
    value.as_u64().unwrap() as u32
}

/// `check_selector65_update.py`'s cells: an LCG's high WORDs, x * 512 + z.
fn cells(seed: u32) -> Vec<u16> {
    let mut state = seed;
    (0..512 * 512)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 16) as u16
        })
        .collect()
}

struct Host {
    cells: Vec<u16>,
    spacing: u32,
    height_shift: u32,
    plane_shift: u32,
    plane: i32,
    splashes: Vec<[u16; 3]>,
}

impl Selector65Host for Host {
    type Error = std::convert::Infallible;
    fn under_water(&self, [x, z]: [u16; 2]) -> Result<bool, Self::Error> {
        let mask = (!self.spacing as i32 >> 7) as u32;
        let cell = |word: u16| (u32::from(word) >> 7) & mask;
        let index = (cell(x) << 9) + cell(z);
        let height = i32::from(self.cells[index as usize] as u8 as i8) << self.height_shift;
        Ok(height < self.plane >> self.plane_shift)
    }
    fn splash(&mut self, position: [u16; 3], _: &mut u32) -> Result<(), Self::Error> {
        self.splashes.push(position);
        Ok(())
    }
}

/// Every case of `selector65_update_native.json` (the original 0042DC90
/// under Unicorn): the fade it left, its disposal, the splashes it threw
/// and the RNG, over the clamp edges, signed limits, zero and wrapped deltas
/// and three terrain layouts.
#[test]
fn selector65_update_matches_the_original_routine() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/selector65_update_native.json"
    ))
    .unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 36);
    let sine = crate::frontend_assets::FrontendAssets::open(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail"),
    )
    .unwrap()
    .sine_table()
    .values;
    let mut thrown = 0;
    for case in cases {
        let mut arena = ObjectArena::new();
        let object = arena.allocate(ObjectClassKind::Selector65);
        assert_eq!(*arena.selector65(object).unwrap(), Selector65State::new());
        let owned = arena.owned_mut(object).unwrap();
        owned.state.base.position_words = Some([0, 1, 2].map(|i| u(&case["position"][i]) as u16));
        match &mut owned.state.class {
            ObjectClass::Selector65(state) => {
                state.fade_44 = u(&case["fade"]);
                state.limit_48 = u(&case["limit"]);
            }
            _ => unreachable!(),
        }
        let mut host = Host {
            cells: cells(u(&case["seed"])),
            spacing: u(&case["spacing"]),
            height_shift: u(&case["height_shift"]),
            plane_shift: u(&case["plane_shift"]),
            plane: case["plane"].as_i64().unwrap() as i32,
            splashes: Vec::new(),
        };
        let mut rng = u(&case["rng"]);
        arena
            .tick_selector65(
                object,
                u(&case["micros"]),
                &sine,
                &mut rng,
                crate::name_entry::name_entry_rng_next,
                &mut host,
            )
            .unwrap();
        let name = case["name"].as_str().unwrap();
        let disposed = arena
            .list(ObjectList::Disposal)
            .iter()
            .any(|node| node.object == object);
        assert_eq!(disposed, case["disposed"].as_bool().unwrap(), "{name}");
        assert_eq!(
            arena.selector65(object).unwrap().fade_44,
            u(&case["fade_after"]),
            "{name}"
        );
        assert_eq!(rng, u(&case["rng_after"]), "{name}");
        let expected: Vec<[u16; 3]> = case["splashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|splash| [0, 1, 2].map(|i| u(&splash[i]) as u16))
            .collect();
        assert_eq!(host.splashes, expected, "{name}");
        thrown += expected.len();
    }
    assert!(thrown > 20);
}
