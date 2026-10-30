//! Terrain preparation and height queries used by world loader 0048DC30.
//!
//! H12 bytes remain immutable format evidence. This owner copies their cells,
//! prepares the runtime triangle bit with 004506A0, and samples those triangles
//! with 00450A30. Spacing is an explicit process policy; world flag 0x80 selects
//! the independent vertical scale. No water-plane clamp, terrain rendering,
//! deformation, furniture construction or object initialization is implied.
//! The effects child separately owns mutable retirement flag/sparse operations.

mod effects;
mod furniture_radius;
mod policy;
mod probe;
pub mod ultra;
pub use effects::{
    FurnitureDamageRequest, TerrainFlagScatterOutcome, TerrainFlagScatterRequest,
    TerrainFurnitureEffectError, TerrainFurnitureEffectHost, TerrainSparsePayload,
    TerrainSparseRecordId, TerrainSparseWriteError,
};
pub use furniture_radius::{
    produce_furniture_radius, produce_resident_furniture_radius, ProcessFurnitureRadius,
    TerrainFurnitureRadiusError, TerrainFurnitureRadiusRequest,
};
pub use policy::{ProcessTerrainPolicy, TerrainDetailContext, TerrainPolicyError};
#[cfg(test)]
pub(crate) use probe::tests::fixture_terrain as native_probe_fixture_terrain;
pub use probe::{
    TerrainProbePlanePolicy, TerrainProbePosition, TerrainSlopeProbeRequest,
    TerrainSlopeProbeResult,
};

use inf_formats::{
    native_basis::{q31_divide, spacing_sample},
    terrain_blend_table, TerrainBucket, TerrainCell, TerrainEntry, TERRAIN_GRID_SIDE,
};

/// 0044FDD0's H12 direction publication to 00548380/84/88. The same
/// stored vector supplies lighting 004501A0 and the slope-ratio phase below.
pub fn normalize_terrain_light(light: [i32; 3]) -> [f32; 3] {
    // 44FE17..3E keeps X/raw-Z in x87 but spills Y and the Z multiplier.
    // FSQRT is then spilled to f32 before all three divisions 44FE4F..6F.
    let x = f64::from(light[0]);
    let y = f64::from(light[1] as f32);
    let z = f64::from(light[2] as f32);
    let length = ((f64::from(light[2]) * z + y * y) + x * x).sqrt() as f32;
    if length == 0.0 {
        [1.0, 0.0, 0.0]
    } else {
        [x, y, z].map(|v| (v / f64::from(length)) as f32)
    }
}

/// Load-time 00548390/8C: scale the normalized stored floats by 16384, truncate,
/// then take saturated Q31 ratios with the native sign fixups. Cache state
/// 00450230 and 00548394 belong to their separate owners.
pub fn compute_slope_factors(light: [i32; 3]) -> [i32; 2] {
    let stored = normalize_terrain_light(light);
    // 0x511BA8 semantics: chop toward zero, low DWORD kept.
    let scaled = stored.map(|v| (f64::from(v) * 16384.0) as i32);
    let signs = [
        (scaled[1] as u32) ^ (scaled[0] as u32),
        (scaled[2] as u32) ^ (scaled[1] as u32),
    ];
    let magnitude = scaled.map(|v| v.wrapping_abs() as u32);
    let mut factors = [0x7fff_ffffu32, 0x7fff_ffffu32];
    if magnitude[0] < magnitude[1] {
        factors[0] = q31_divide(magnitude[0], magnitude[1]);
    }
    if magnitude[2] < magnitude[1] {
        factors[1] = q31_divide(magnitude[2], magnitude[1]);
    }
    [
        if (signs[0] as i32) >= 0 {
            (factors[0] as i32).wrapping_neg()
        } else {
            factors[0] as i32
        },
        if (signs[1] as i32) >= 0 {
            (factors[1] as i32).wrapping_neg()
        } else {
            factors[1] as i32
        },
    ]
}

/// The two supported spacing/mask pairs at 005372F0/005372F4. Coordinates
/// always wrap in a 16-bit world; the underlying H12 grid is always 512 square.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainSpacing {
    Units128,
    Units256,
}

impl TerrainSpacing {
    /// 0048DC30 calls 004502C0(0) when H14 flag 0x2000 forces coarse terrain.
    /// Otherwise it retains the process's current policy. This is not a
    /// default for the separately owned settings +70 or temporary detail swaps.
    pub const fn for_world(self, flags: u32) -> Self {
        if flags & 0x2000 != 0 {
            Self::Units256
        } else {
            self
        }
    }

    pub const fn units(self) -> u16 {
        match self {
            Self::Units128 => 128,
            Self::Units256 => 256,
        }
    }

    const fn shift(self) -> u32 {
        match self {
            Self::Units128 => 7,
            Self::Units256 => 8,
        }
    }
}

/// `(F0, F4)` coordinate scale/mask for a spacing, from the
/// `004502C0` static table: coarse `(0x100, 0xFF)`, fine
/// `(0x80, 0x7F)`. Proven live for mission 18's fine frame
/// (bind-oracle odd-bucket binds); the EXE settings default is
/// Detail 0, so default loads stay coarse.
pub const fn spacing_pair(spacing: TerrainSpacing) -> (u16, u16) {
    match spacing {
        TerrainSpacing::Units128 => (0x80, 0x7f),
        TerrainSpacing::Units256 => (0x100, 0xff),
    }
}

/// Signed H12 height-byte scale set at 0048DD75..0048DDA5. This is separate
/// from horizontal detail and from the H12 plane-height fixed-point shift.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerrainHeightScale {
    Units16,
    Units32,
}

impl TerrainHeightScale {
    pub const fn from_world_flags(flags: u32) -> Self {
        if flags & 0x80 != 0 {
            Self::Units16
        } else {
            Self::Units32
        }
    }

    pub(crate) const fn shift(self) -> u32 {
        match self {
            Self::Units16 => 4,
            Self::Units32 => 5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainSampling {
    pub spacing: TerrainSpacing,
    pub height_scale: TerrainHeightScale,
}

/// `0044FFA0` projector failure: which unowned input a path needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Project44FFA0Error {
    /// A reached enabled wave below the plane needs the original sine table.
    MissingTable,
}

/// Actual per-call globals and retained renderer origin for 0044FFA0.
/// Native4500A1 captures the clock once; both4500E8/450177 water calls
/// receive five explicit arguments, including their signed terrain height.
/// This owned-input domain excludes resource/table alias mutation mid-call.
pub struct Project44FFA0Inputs<'a> {
    /// H14 entry-0 +4 bit 0x40 gate for the spacing-sample sine paths.
    pub world_flags: u32,
    pub wave_clock: u32,
    /// Copy source at model-context+58 -> renderer+04/+08/+0C.
    pub renderer_xyz: [i32; 3],
    /// Original sine table; read only if bit40 and full plane>signed height.
    pub table: Option<&'a [u16; 4096]>,
}

/// Provably unread table after 00410F60's disabled/above-plane bypass.
static DEAD_TABLE: [u16; 4096] = [0; 4096];

/// Explicit stack inputs to 00410F60. The H12 plane datum and process
/// 005483BC shift remain owned by TerrainRuntime. Camera 004106D1 supplies
/// a literal zero clock; other consumers must pass their own reached clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainWaterSurfaceRequest {
    /// Only the low unsigned WORD of native X/Z stack arguments is consumed.
    pub xz: [u16; 2],
    /// Third stack argument, shifted/multiplied with wrapping DWORD arithmetic.
    pub wave_clock: u32,
    /// H14 world entry +4, read at 004106BE; bit 0x40 enables spatial waves.
    pub world_flags: u32,
    /// Fifth stack argument: signed interpolated terrain low AX from 00450A30.
    pub terrain_height: i16,
}

/// Factor-scaled vertex adjustment shared by the slope and tail paths:
/// `record + (((delta * factor) high DWORD << 1) | sign)`, all wrapping.
/// The sign reads the low DWORD exactly like the original cast.
fn slope_adjust(delta: i64, factor: i64) -> i32 {
    let product = delta.wrapping_mul(factor);
    (((product as u64) >> 32) as i32).wrapping_shl(1) | ((product as i32) < 0) as i32
}

/// Prepared cells for one H12 allocation and one explicit sampling policy.
/// Preparation preserves cell metadata outside bit 11, including signed
/// heights. The effects child owns later cell-flag and sparse-payload writes;
/// sparse X words, record ordering and loader-fixed table shape remain fixed.
#[derive(Clone, Debug)]
pub struct TerrainRuntime {
    sampling: TerrainSampling,
    cells: Box<[TerrainCell]>,
    buckets: Vec<TerrainBucket>,
    /// Signed H12 plane datum (`plane_height_fixed`), the `0044FFA0`
    /// projector's shifted height base. Read from the source entry; a
    /// water bridge's rise (`0040A146`) rewrites it.
    plane_base: i32,
    /// Slope pair from `0044FDD0`, computed from the source light
    /// vector at construction. Load-time constants per world.
    slope_factors: [i32; 2],
    /// The entry's raw light words +4/+8/+C, which the course gate's draw
    /// (00440640) reads again.
    light_direction_raw: [i32; 3],
    /// Port setting, not retail: Ultra ground detail's fine grid
    /// ([`ultra`]), which the ground's height takes on High.
    ultra: Option<std::sync::Arc<ultra::UltraGround>>,
    /// The buildings' footings Ultra ground raises.
    ultra_footings: Vec<ultra::Footing>,
    /// Port setting, not retail (Vulkan's remaster, single player): the
    /// puddles' carve the wheels sink into (`wheel_height_at`), or none.
    puddle_carve: Option<crate::scene_ground_detail::CarveLook>,
}

/// Message 22's view of the furniture buckets: a bucket is the one at that
/// `y`, a record's value its `x`, its lookup byte `+3`.
impl crate::network::furniture::FurnitureBuckets for TerrainRuntime {
    fn count(&self, bucket: usize) -> usize {
        self.furniture_bucket(bucket).map_or(0, |b| b.records.len())
    }
    fn record(
        &self,
        bucket: usize,
        index: usize,
    ) -> Option<crate::network::furniture::FurnitureRecord> {
        let r = self.furniture_bucket(bucket)?.records.get(index)?;
        Some(crate::network::furniture::FurnitureRecord {
            value: r.x,
            state: r.state,
            lookup: r.resource_lookup.raw(),
        })
    }
    fn set_state(&mut self, bucket: usize, index: usize, state: u8) -> bool {
        self.furniture_record_mut(bucket, index)
            .map(|r| r.state = state)
            .is_some()
    }
    fn set_lookup(&mut self, bucket: usize, index: usize, lookup: u8) -> bool {
        self.furniture_record_mut(bucket, index)
            .map(|r| r.resource_lookup = inf_formats::TerrainResourceLookup::from_raw(lookup))
            .is_some()
    }
}

impl TerrainRuntime {
    fn furniture_bucket(&self, y: usize) -> Option<&TerrainBucket> {
        self.buckets
            .iter()
            .find(|bucket| usize::from(bucket.y) == y)
    }

    fn furniture_record_mut(
        &mut self,
        y: usize,
        index: usize,
    ) -> Option<&mut inf_formats::TerrainRecord> {
        self.buckets
            .iter_mut()
            .find(|bucket| usize::from(bucket.y) == y)?
            .records
            .get_mut(index)
    }

    /// The 0044B590 preparation phase: build the blend table (0044B5C0), then
    /// apply 004506A0 with its zero/default step. Coarse detail only writes
    /// even/even cell origins; unvisited cells retain their original bits.
    pub fn new(source: &TerrainEntry, sampling: TerrainSampling) -> Self {
        let mut terrain = Self {
            sampling,
            cells: source.cells().into(),
            buckets: source.buckets().into(),
            plane_base: source.prefix.plane_height_fixed,
            slope_factors: compute_slope_factors(source.prefix.light_direction_raw),
            light_direction_raw: source.prefix.light_direction_raw,
            ultra: None,
            ultra_footings: Vec::new(),
            puddle_carve: None,
        };
        terrain.prepare_diagonals();
        terrain
    }

    /// Port setting, not retail: Ultra ground detail shaped as `shape`
    /// says ([`ultra`]), with the footings it has, or none for retail's
    /// ground. The same shape keeps the grid it has.
    pub fn set_ultra(&mut self, shape: Option<ultra::UltraShape>) {
        if self.ultra.as_ref().map(|ultra| ultra.shape()) == shape {
            return;
        }
        self.ultra = shape.map(|shape| {
            std::sync::Arc::new(ultra::UltraGround::build(
                &self.cells,
                self.height_shift(),
                shape,
                self.ultra_footings.clone(),
            ))
        });
    }

    /// Port setting, not retail: the buildings' footings Ultra ground
    /// raises ([`ultra::Footing`]), now and whenever it is built again.
    pub fn set_ultra_footings(&mut self, footings: Vec<ultra::Footing>) {
        if let Some(ultra) = self.ultra.as_mut() {
            std::sync::Arc::make_mut(ultra).set_footings(footings.clone());
        }
        self.ultra_footings = footings;
    }

    /// Port setting, not retail: the puddles' carve the wheeled craft sink
    /// into (`wheel_height_at`), or none for retail's ground. The session
    /// sets it each tick from the weather.
    pub fn set_puddle_carve(&mut self, look: Option<crate::scene_ground_detail::CarveLook>) {
        self.puddle_carve = look;
    }

    /// The ground under a wheeled craft's hull sample at `xz`: retail's
    /// height, less the puddles' bed where they are carved (the wheel
    /// goes in), as the tracer's
    /// detail patch draws it (`scene_ground_detail::bed_carve`).
    pub fn wheel_height_at(&self, xz: [u16; 2]) -> i16 {
        let height = self.height_at(xz);
        let Some(look) = self.puddle_carve.as_ref().filter(|_| self.ultra.is_none()) else {
            return height;
        };
        let world = xz.map(f32::from);
        let carve = crate::scene_ground_detail::bed_carve(self, world, look);
        height.saturating_sub(carve.round() as i16)
    }

    /// Ultra ground detail's fine grid, where the ground's height takes it:
    /// on High.
    pub fn ultra(&self) -> Option<&ultra::UltraGround> {
        self.ultra
            .as_deref()
            .filter(|_| self.sampling.spacing == TerrainSpacing::Units128)
    }

    /// The H12 entry's raw light words +4/+8/+C.
    pub fn light_direction_raw(&self) -> [i32; 3] {
        self.light_direction_raw
    }

    pub fn sampling(&self) -> TerrainSampling {
        self.sampling
    }

    /// Signed H12 plane datum, the entry's first DWORD.
    pub fn plane_base(&self) -> i32 {
        self.plane_base
    }

    /// `0040A146`: a water bridge writes the entry's first DWORD. The
    /// plane's readers during play (draws, contacts, actor updates) read it
    /// here each time.
    pub fn set_plane_base(&mut self, plane_base: i32) {
        self.plane_base = plane_base;
    }

    /// Port option: what a world draw may write of the ground, its
    /// furniture's records and the plane, for [`Self::put_back_drawn`] to
    /// put back (the view shown's draw, apart from the game's).
    pub(crate) fn drawn(&self) -> (Vec<TerrainBucket>, i32) {
        (self.buckets.clone(), self.plane_base)
    }

    pub(crate) fn put_back_drawn(&mut self, (buckets, plane_base): (Vec<TerrainBucket>, i32)) {
        self.buckets = buckets;
        self.plane_base = plane_base;
    }

    /// Test terrain from supplied raw cells (native `x*512+z` order), with
    /// no preparation.
    #[cfg(test)]
    pub(crate) fn from_raw_cells(
        sampling: TerrainSampling,
        plane_base: i32,
        cells: impl IntoIterator<Item = u16>,
    ) -> Self {
        let cells: Box<[TerrainCell]> = cells.into_iter().map(TerrainCell::from_raw).collect();
        assert_eq!(cells.len(), TERRAIN_GRID_SIDE * TERRAIN_GRID_SIDE);
        Self {
            sampling,
            cells,
            buckets: Vec::new(),
            plane_base,
            slope_factors: [0; 2],
            light_direction_raw: [0; 3],
            ultra: None,
            ultra_footings: Vec::new(),
            puddle_carve: None,
        }
    }

    /// Load-time `005483BC` policy observed in the world loader: 8 without
    /// the `0x80` vertical-scale flag, 9 with it (the loader sets
    /// `005372F8` to 5/4 on the same flag). Paired derivation, not two
    /// independent policies; the exact loader computation is a follow-up.
    /// Before any world, the CRT's initializer 00450680 leaves 13 minus the
    /// static scale 5, the same 8; 0048DC30 rewrites it on every load.
    pub fn plane_shift(&self) -> u32 {
        match self.sampling.height_scale {
            TerrainHeightScale::Units16 => 9,
            TerrainHeightScale::Units32 => 8,
        }
    }

    /// 00410F60's signed low AX result, using the original sine table and
    /// this H12 entry's signed fixed-point DWORD zero. Handler-table entry12
    /// at 0053AA44 identifies 0055EE38/0055EE3C; 004106D5 passes that entry
    /// as ECX. The wave gate is separately read from the H14 world resource.
    pub fn water_surface_height(
        &self,
        request: TerrainWaterSurfaceRequest,
        sine_quarter: &[u16; 4096],
    ) -> i16 {
        spacing_sample(
            self.plane_base as u32,
            u32::from(request.xz[0]),
            u32::from(request.xz[1]),
            request.wave_clock,
            request.world_flags & 0x40,
            i32::from(request.terrain_height) as u32,
            self.plane_shift(),
            sine_quarter,
        ) as u16 as i16
    }

    /// Load-time slope pair (`00548390/8C`) from `0044FDD0`, retained for
    /// the `0044FFA0` projector. Computed from the source light vector
    /// at construction; the light vector has no later mutation owner.
    pub fn slope_factors(&self) -> [i32; 2] {
        self.slope_factors
    }

    /// Prepared grid in native x*512+z order, independently owned from H12.
    pub fn cells(&self) -> &[TerrainCell] {
        &self.cells
    }
    /// Sparse buckets in native y order, retained verbatim from H12 for
    /// the furniture walk. Record counts are the decoded record vecs
    /// (the loader's `byte_size >> 2` fixup); the on-disk byte sizes
    /// stay on the format side.
    pub fn buckets(&self) -> &[TerrainBucket] {
        &self.buckets
    }

    /// Port option, not retail: furniture of the port's own (Pelagia's
    /// sunken ruins) at the 128-unit cell holding world X/Z `at`, in its
    /// row's bucket by X order with its cell marked, as the loader keeps
    /// every authored record. False, changing nothing, where the cell
    /// already holds a record or the row has no bucket.
    pub(crate) fn add_furniture(
        &mut self,
        at: [u16; 2],
        state: u8,
        lookup: inf_formats::TerrainResourceLookup,
    ) -> bool {
        let [x, z] = at.map(|word| word >> 7);
        let index = usize::from(x) * TERRAIN_GRID_SIDE + usize::from(z);
        let Some(cell) = self.cells.get(index).copied() else {
            return false;
        };
        if cell.has_sparse_record() {
            return false;
        }
        let Some(bucket) = self.buckets.iter_mut().find(|bucket| bucket.y == z) else {
            return false;
        };
        let place = bucket.records.partition_point(|record| record.x < x);
        bucket.records.insert(
            place,
            inf_formats::TerrainRecord {
                x,
                state,
                resource_lookup: lookup,
            },
        );
        bucket.byte_size = bucket.byte_size.wrapping_add(4);
        self.cells[index] = TerrainCell::from_raw(cell.raw() | 0x0400);
        true
    }
    /// Port option, not retail: takes out the furniture record at the
    /// 128-unit cell holding world X/Z `at` (Pelagia's dead gate, which
    /// trades places with its working one), unmarking its cell, as
    /// [`Self::add_furniture`] puts one in. None, changing nothing, where
    /// the cell holds no record.
    pub(crate) fn remove_furniture(&mut self, at: [u16; 2]) -> Option<inf_formats::TerrainRecord> {
        let [x, z] = at.map(|word| word >> 7);
        let index = usize::from(x) * TERRAIN_GRID_SIDE + usize::from(z);
        let cell = self.cells.get(index).copied()?;
        if !cell.has_sparse_record() {
            return None;
        }
        let bucket = self.buckets.iter_mut().find(|bucket| bucket.y == z)?;
        let place = bucket.records.iter().position(|record| record.x == x)?;
        let record = bucket.records.remove(place);
        bucket.byte_size = bucket.byte_size.wrapping_sub(4);
        self.cells[index] = TerrainCell::from_raw(cell.raw() & !0x0400);
        Some(record)
    }
    /// 00437AF0: the stored cell at the aligned low-WORD world coordinates.
    /// This reads one cell and does not interpolate its height or inspect its
    /// sparse records. 0047F600 consumes the signed low byte of this same cell.
    pub fn cell_at(&self, [x, z]: [u16; 2]) -> TerrainCell {
        let grid_mask = !((self.sampling.spacing.units() - 1) >> 7);
        self.cell_at_grid([(x >> 7) & grid_mask, (z >> 7) & grid_mask])
    }

    ///Validated canonical grid indices; caller owns its reached live5372F4
    ///conversion. This never substitutes the terrain's stored sampling policy.
    pub(crate) fn cell_at_grid(&self, [x, z]: [u16; 2]) -> TerrainCell {
        self.cell(usize::from(x), usize::from(z))
    }

    /// 00450890: store `height` arithmetically shifted down by the height
    /// scale (005372F8) as the low byte of the cell at the aligned X/Z,
    /// keeping its other byte. Nothing is prepared again: the diagonal bits
    /// stay as they were.
    pub fn write_raw_height(&mut self, [x, z]: [u16; 2], height: i32) {
        let grid_mask = !((self.sampling.spacing.units() - 1) >> 7);
        self.write_raw_height_cell([(x >> 7) & grid_mask, (z >> 7) & grid_mask], height);
    }

    /// 00450890 under 00450350's 128-unit addressing (`005372F0` = 0x80,
    /// `005372F4` = 0x7F) whatever the terrain detail, as selector 83's
    /// patch writes its cells.
    pub fn write_raw_height_128(&mut self, [x, z]: [u16; 2], height: i32) {
        self.write_raw_height_cell([x >> 7, z >> 7], height);
    }

    fn write_raw_height_cell(&mut self, [x, z]: [u16; 2], height: i32) {
        let index = usize::from(x) * TERRAIN_GRID_SIDE + usize::from(z);
        let cell = self.cells[index].raw();
        let low = (height >> self.sampling.height_scale.shift()) as u8;
        self.cells[index] = TerrainCell::from_raw((cell & 0xff00) | u16::from(low));
        let shift = self.sampling.height_scale.shift();
        if let Some(ultra) = self.ultra.as_mut() {
            std::sync::Arc::make_mut(ultra).update(&self.cells, shift, [x, z].map(usize::from));
        }
    }

    /// Port option, not retail: a 128-unit cell's height, material and baked
    /// shade written whole (Pelagia's sculpted ground), its sparse-record bit
    /// kept. Nothing is prepared again until [`Self::refresh_sculpted`].
    pub(crate) fn write_sculpted_cell(
        &mut self,
        [x, z]: [u16; 2],
        height: i32,
        material: u8,
        shade: u8,
    ) {
        let index = usize::from(x >> 7) * TERRAIN_GRID_SIDE + usize::from(z >> 7);
        let low = (height >> self.sampling.height_scale.shift()).clamp(-128, 127) as i8 as u8;
        let keep = self.cells[index].raw() & 0x0c00;
        self.cells[index] = TerrainCell::from_raw(
            u16::from(shade & 0x0f) << 12 | keep | u16::from(material & 3) << 8 | u16::from(low),
        );
    }

    /// Port option, not retail: after [`Self::write_sculpted_cell`], the
    /// diagonals from the new materials (as the load prepares them) and
    /// Ultra ground built again over the new heights.
    pub(crate) fn refresh_sculpted(&mut self) {
        self.prepare_diagonals();
        if let Some(shape) = self.ultra.as_ref().map(|ultra| ultra.shape()) {
            self.ultra = None;
            self.set_ultra(Some(shape));
        }
    }

    /// `005372F8`, the shift from a cell's height byte to world units.
    pub fn height_shift(&self) -> u32 {
        self.sampling.height_scale.shift()
    }

    /// Signed raw cell height in world WORD units, without interpolation.
    /// 0043689B..004368A1 sign-extends H12's low byte, shifts by005372F8,
    /// then retains the low WORD for the furniture executor's stack position.
    pub fn raw_height_at(&self, xz: [u16; 2]) -> i16 {
        (i32::from(self.cell_at(xz).height_i8()) << self.sampling.height_scale.shift()) as i16
    }

    /// 00450A30 returns signed low AX. X/Z use only the caller's low WORDs;
    /// callers with packed DWORD arguments must extract those words explicitly.
    /// Its third stack argument is unused and has no portable counterpart.
    pub fn height_at(&self, [x, z]: [u16; 2]) -> i16 {
        if let Some(ultra) = self.ultra() {
            let alternate = self.cell_at([x, z]).stored_alternate_diagonal();
            return ultra.height_at([x, z], alternate).round() as i16;
        }
        let spacing = self.sampling.spacing;
        let units = spacing.units();
        let mask = units - 1;
        let step = usize::from(units >> 7);
        let x0 = usize::from((x >> 7) & !(mask >> 7));
        let z0 = usize::from((z >> 7) & !(mask >> 7));
        let x1 = (x0 + step) & (TERRAIN_GRID_SIDE - 1);
        let z1 = (z0 + step) & (TERRAIN_GRID_SIDE - 1);
        let height =
            |x, z| i32::from(self.cell(x, z).height_i8()) << self.sampling.height_scale.shift();
        let [h00, h10, h11, h01] = [
            height(x0, z0),
            height(x1, z0),
            height(x1, z1),
            height(x0, z1),
        ];
        let dx = i32::from(x & mask);
        let dz = i32::from(z & mask);
        // Each product is rounded separately by SAR; combining the products
        // before shifting changes negative slopes and diagonal-edge heights.
        let slope = |difference: i32, fraction: i32| (difference * fraction) >> spacing.shift();
        let height = if self.cell(x0, z0).stored_alternate_diagonal() {
            let remaining_z = i32::from(units) - dz;
            if dx < remaining_z {
                h00 + slope(h01 - h00, dz) + slope(h10 - h00, dx)
            } else {
                h01 + slope(h10 - h11, remaining_z) + slope(h11 - h01, dx)
            }
        } else if dx < dz {
            h00 + slope(h11 - h01, dx) + slope(h01 - h00, dz)
        } else {
            h00 + slope(h11 - h10, dz) + slope(h10 - h00, dx)
        };
        height as i16
    }

    /// `0044FFA0`: terrain-coupled vertex projector behind opcode 0x2C.
    /// `record` is the source vertex XYZ in world coordinates. The datum
    /// `h0` is the plane base shifted arithmetically; heights sample low
    /// words exactly like `00450A30` (high halves never consumed).
    /// Branch A serves below/at-datum vertices (slope with the
    /// load-time factors, else a context copy); the tail serves the
    /// rest (water sample, then factor slopes). Only reached water-wave
    /// gates require a sine table. No H12 lazy loads: residency holds.
    pub fn project_44ffa0(
        &self,
        record: [i32; 3],
        inputs: &Project44FFA0Inputs,
    ) -> Result<[i32; 3], Project44FFA0Error> {
        let h0full = self.plane_base >> (self.plane_shift() & 31);
        let h0datum = (h0full as i16) as i32;
        if record[1].wrapping_sub(10) <= h0datum {
            let height = self.height_at([record[0] as u16, record[2] as u16]) as i32;
            if height <= h0datum {
                if record[1].wrapping_add(10) < h0datum {
                    // Slope path: factor-scaled adjustment plus a height
                    // resample over the adjusted XZ.
                    let [factor_90, factor_8c] = self.slope_factors;
                    let delta = (record[1] as i32).wrapping_sub(height) as i64;
                    let out_x = record[0].wrapping_add(slope_adjust(delta, factor_90 as i64));
                    let out_z = record[2].wrapping_add(slope_adjust(delta, factor_8c as i64));
                    let out_y = self.height_at([out_x as u16, out_z as u16]) as i32;
                    return Ok([out_x, out_y, out_z]);
                }
                // Copy path for near-datum vertices.
                return Ok(inputs.renderer_xyz);
            }
        }
        let height = self.height_at([record[0] as u16, record[2] as u16]) as i32;
        //4500C3 pushes the fifth signed height before the H14 lookup;
        //4500E3..E5 supply captured clock and source low-WORD X/Z.
        let sample =
            self.project_water_sample([record[0] as u16, record[2] as u16], height as i16, inputs)?;
        // Both sides truncate to short before the signed maximum.
        let projected = if sample < (height as i16) {
            height
        } else {
            i32::from(sample)
        };
        let [factor_90, factor_8c] = self.slope_factors;
        let out_x = record[0].wrapping_add(slope_adjust(
            (record[1] as i32).wrapping_sub(projected) as i64,
            factor_90 as i64,
        ));
        let out_z = record[2].wrapping_add(slope_adjust(
            (record[1] as i32).wrapping_sub(projected) as i64,
            factor_8c as i64,
        ));
        let out_y = self.height_at([out_x as u16, out_z as u16]) as i32;
        //45012E/450131 retain adjusted X/Z low WORDs;45016D supplies
        //the new height as the fifth argument. Output Y is still unwritten.
        let spaced = i32::from(self.project_water_sample(
            [out_x as u16, out_z as u16],
            out_y as i16,
            inputs,
        )?);
        Ok([out_x, if spaced < out_y { out_y } else { spaced }, out_z])
    }

    fn project_water_sample(
        &self,
        xz: [u16; 2],
        terrain_height: i16,
        inputs: &Project44FFA0Inputs<'_>,
    ) -> Result<i16, Project44FFA0Error> {
        let plane = self.plane_base >> (self.plane_shift() & 31);
        let table = if inputs.world_flags & 0x40 != 0 && i32::from(terrain_height) < plane {
            inputs.table.ok_or(Project44FFA0Error::MissingTable)?
        } else {
            &DEAD_TABLE
        };
        Ok(self.water_surface_height(
            TerrainWaterSurfaceRequest {
                xz,
                wave_clock: inputs.wave_clock,
                world_flags: inputs.world_flags,
                terrain_height,
            },
            table,
        ))
    }

    fn cell(&self, x: usize, z: usize) -> TerrainCell {
        self.cells[x * TERRAIN_GRID_SIDE + z]
    }

    fn prepare_diagonals(&mut self) {
        let blends = terrain_blend_table();
        let step = usize::from(self.sampling.spacing.units() >> 7);
        // 004506A0 walks Z outside X. Material bits are never mutated, so
        // earlier triangle writes do not change the later blend lookups.
        for z0 in (0..TERRAIN_GRID_SIDE).step_by(step) {
            let z1 = (z0 + step) & (TERRAIN_GRID_SIDE - 1);
            for x0 in (0..TERRAIN_GRID_SIDE).step_by(step) {
                let x1 = (x0 + step) & (TERRAIN_GRID_SIDE - 1);
                let combo = self.cell(x0, z0).material()
                    | self.cell(x0, z1).material() << 2
                    | self.cell(x1, z0).material() << 4
                    | self.cell(x1, z1).material() << 6;
                let diagonal =
                    u16::from(blends[usize::from(combo)].uses_alternate_diagonal()) << 11;
                let index = x0 * TERRAIN_GRID_SIDE + z0;
                self.cells[index] =
                    TerrainCell::from_raw((self.cells[index].raw() & !0x800) | diagonal);
            }
        }
    }
}

#[cfg(test)]
mod project_44ffa0_tests;
#[cfg(test)]
mod tests;

/// The ground's grid of heights as the tracer's ground mesh holds it
/// (`scene_terrain`), for the puddles' carve.
impl crate::scene_ground_detail::GridHeights for TerrainRuntime {
    fn points(&self) -> usize {
        TERRAIN_GRID_SIDE / usize::from(self.sampling.spacing.units() >> 7) + 1
    }
    fn spacing(&self) -> f32 {
        f32::from(self.sampling.spacing.units())
    }
    fn height(&self, i: usize, j: usize) -> f32 {
        let step = usize::from(self.sampling.spacing.units() >> 7);
        let side = TERRAIN_GRID_SIDE / step;
        let cell = self.cell((i % side) * step, (j % side) * step);
        f32::from(((i32::from(cell.height_i8())) << self.height_shift()) as i16)
    }
}
