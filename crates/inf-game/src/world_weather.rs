//! Port option, not retail: the weather of a world whose modern sky rains or
//! snows (a warp can randomly rain or not,
//! each phase lasts long enough not to annoy, puddles slowly dry out and
//! slowly build up as it rains).
//! Each visit rolls whether the fall has begun, then alternates long
//! phases of fall and clear; the fall fades in and out over a minute and a
//! half, and the ground's wetness builds while it rains and dries after.
//! Snow wets nothing but the models, faintly. Only what is drawn and heard
//! follows it, its seed no draw of the game's generator, but for one
//! gameplay setting of the remaster: with "wheels sink into puddles" the
//! wheeled craft meet the puddles' carved bed
//! (`TerrainRuntime::wheel_height_at`). A network game keeps retail's
//! weather, none ([`WorldWeather::retail`]).

use crate::modern_tuning::PrecipKind;

/// How long a phase of fall lasts, and of clear sky, in seconds.
const FALLING: std::ops::Range<f32> = 600.0..1080.0;
const CLEAR: std::ops::Range<f32> = 720.0..1320.0;
/// How long the fall takes to come in or die away.
const FADE_SECONDS: f32 = 90.0;
/// How long full rain takes to soak the ground, and dry ground to dry.
const SOAK_SECONDS: f32 = 300.0;
const DRY_SECONDS: f32 = 720.0;
/// How likely a visit starts in the fall.
const FALLING_AT_ARRIVAL: f32 = 0.45;
/// The ground's wetness at which the first puddles stand, and at which they
/// are full (puddles appear a short while,
/// 1-5 minutes, after it starts raining). Under full rain from a dry start,
/// the first come after 60 seconds and all by 285; from a clear sky, the
/// fall's fade-in first, after 105 and 330.
const PUDDLES_FROM: f32 = 0.2;
const PUDDLES_FULL: f32 = 0.95;
/// How wet snow leaves the models, of what rain leaves.
const SNOW_ON_MODELS: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorldWeather {
    /// The world's fall, if it has weather (rain or snow); none keeps the
    /// fall its tuning gives always (ash, embers, none).
    kind: Option<PrecipKind>,
    falling: bool,
    /// Seconds left in this phase.
    phase_left: f32,
    /// The fall now, 0 (clear) to 1 (as the tuning gives it).
    fall: f32,
    /// The ground's wetness, 0 to 1.
    wet: f32,
    seed: u32,
}

impl Default for WorldWeather {
    fn default() -> Self {
        Self::constant()
    }
}

impl WorldWeather {
    /// A world without weather: its fall, if any, always as tuned.
    pub fn constant() -> Self {
        Self {
            kind: None,
            falling: true,
            phase_left: f32::INFINITY,
            fall: 1.0,
            wet: 0.0,
            seed: 0,
        }
    }

    /// The weather for a visit to a world whose fall is `kind`, rolled from
    /// `seed`; `forced` (the start harness) sets the fall and soaks or dries
    /// the ground at once. A visit that arrives in the rain finds the ground
    /// dry, the rain just come: it soaks, and its puddles stand, as the
    /// player watches (a visit that spawns in the rain
    /// can have puddles).
    pub fn arrive(kind: PrecipKind, seed: u32, forced: Option<bool>) -> Self {
        if !matches!(kind, PrecipKind::Rain | PrecipKind::Snow) {
            return Self::constant();
        }
        let mut weather = Self {
            kind: Some(kind),
            falling: false,
            phase_left: 0.0,
            fall: 0.0,
            wet: 0.0,
            seed: seed | 1,
        };
        weather.falling = forced.unwrap_or_else(|| weather.unit() < FALLING_AT_ARRIVAL);
        // The first phase is already under way: anything from a fifth of a
        // phase to the whole of one is left.
        let span = weather.span();
        weather.phase_left = span * (0.2 + 0.8 * weather.unit());
        weather.fall = if weather.falling { 1.0 } else { 0.0 };
        if kind == PrecipKind::Rain && weather.falling && forced.is_some() {
            weather.wet = 1.0;
        }
        weather
    }

    /// A network game's weather: retail's, none (in network mode all weather is
    /// disabled, so the game acts like retail). A raining or snowing world stays clear and dry for
    /// good; another keeps its fall as tuned (ash, embers), which is its look
    /// rather than weather.
    pub fn retail(kind: PrecipKind) -> Self {
        if !matches!(kind, PrecipKind::Rain | PrecipKind::Snow) {
            return Self::constant();
        }
        Self {
            kind: Some(kind),
            falling: false,
            phase_left: f32::INFINITY,
            fall: 0.0,
            wet: 0.0,
            seed: 1,
        }
    }

    /// Advance by `seconds`, the rain falling at `shown` of its fall (0 to
    /// 1: none by day on Pelagia, whose rain waits for the night): the
    /// ground soaks only from rain that falls, so no puddle stands before
    /// it has rained.
    pub fn tick(&mut self, seconds: f32, shown: f32) {
        let Some(kind) = self.kind else {
            return;
        };
        let seconds = seconds.max(0.0);
        self.phase_left -= seconds;
        if self.phase_left <= 0.0 {
            self.falling = !self.falling;
            self.phase_left = self.span();
        }
        let target = if self.falling { 1.0 } else { 0.0 };
        let step = seconds / FADE_SECONDS;
        self.fall = if self.fall < target {
            (self.fall + step).min(target)
        } else {
            (self.fall - step).max(target)
        };
        let raining = self.fall * shown.clamp(0.0, 1.0);
        if kind == PrecipKind::Rain && raining > 0.05 {
            self.wet = (self.wet + seconds * raining / SOAK_SECONDS).min(1.0);
        } else {
            self.wet = (self.wet - seconds / DRY_SECONDS).max(0.0);
        }
    }

    /// The share of the tuned fall that falls now (1 on a world without
    /// weather).
    pub fn fall(&self) -> f32 {
        self.fall
    }

    /// How stormy the sky is: as the fall, for a world with weather; 1 for
    /// one without, whose sky stays as tuned.
    pub fn storm(&self) -> f32 {
        self.fall
    }

    /// How wet the ground is, 0 to 1 (rain only).
    pub fn wet_ground(&self) -> f32 {
        self.wet
    }

    /// How far the puddles have filled, 0 (none) to 1: none until the
    /// ground has soaked a while, so the first rain only darkens it, then
    /// growing from the deepest hollows out; drying, they go first.
    pub fn puddles(&self) -> f32 {
        ((self.wet - PUDDLES_FROM) / (PUDDLES_FULL - PUDDLES_FROM)).clamp(0.0, 1.0)
    }

    /// How wet the models are, 0 to 1: the ground's wetness, or a faint
    /// share of the snow falling.
    pub fn wet_models(&self) -> f32 {
        match self.kind {
            Some(PrecipKind::Snow) => SNOW_ON_MODELS * self.fall,
            _ => self.wet,
        }
    }

    /// Whether the rain may wet this visit's ground: a world whose fall is
    /// rain, its phases turning (not a network game's, which stays dry).
    pub fn may_wet(&self) -> bool {
        self.kind == Some(PrecipKind::Rain) && self.phase_left.is_finite()
    }

    /// Whether this world has weather at all.
    pub fn changes(&self) -> bool {
        self.kind.is_some()
    }

    fn span(&mut self) -> f32 {
        let range = if self.falling { FALLING } else { CLEAR };
        range.start + (range.end - range.start) * self.unit()
    }

    /// The next draw from the weather's own generator, 0 to 1.
    fn unit(&mut self) -> f32 {
        // xorshift32.
        let mut x = self.seed;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.seed = x;
        (x >> 8) as f32 / 16_777_216.0
    }
}

/// The seed of a visit to world `world`: the mission's elapsed seconds at
/// the load mixed with the world, so each visit rolls its own weather and a
/// replayed session the same.
pub fn visit_seed(world: u32, elapsed_seconds: u32) -> u32 {
    let mut h = world.wrapping_mul(0x9e37_79b1) ^ elapsed_seconds.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb_352d);
    h ^= h >> 15;
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worlds_without_rain_or_snow_keep_their_fall() {
        for kind in [PrecipKind::None, PrecipKind::Ash, PrecipKind::Embers] {
            let mut weather = WorldWeather::arrive(kind, 7, None);
            weather.tick(5000.0, 1.0);
            assert_eq!(weather.fall(), 1.0);
            assert!(!weather.changes());
        }
    }

    /// Only a visit's rain wets the ground: not snow, not a network game's.
    #[test]
    fn only_rain_may_wet_the_ground() {
        assert!(WorldWeather::arrive(PrecipKind::Rain, 5, Some(false)).may_wet());
        assert!(!WorldWeather::arrive(PrecipKind::Snow, 5, Some(true)).may_wet());
        assert!(!WorldWeather::retail(PrecipKind::Rain).may_wet());
        assert!(!WorldWeather::constant().may_wet());
    }

    #[test]
    fn phases_last_ten_minutes_or_more_and_fade_over_a_minute_and_a_half() {
        let mut weather = WorldWeather::arrive(PrecipKind::Rain, 12345, Some(true));
        assert_eq!(weather.fall(), 1.0);
        assert_eq!(weather.wet_ground(), 1.0);
        // Walk two hours a second at a time, noting each phase's length.
        let mut lengths = Vec::new();
        let mut was = weather.falling;
        let mut length = 0.0;
        for _ in 0..7200 {
            weather.tick(1.0, 1.0);
            length += 1.0;
            if weather.falling != was {
                lengths.push(length);
                length = 0.0;
                was = weather.falling;
            }
            assert!((0.0..=1.0).contains(&weather.fall()));
            assert!((0.0..=1.0).contains(&weather.wet_ground()));
        }
        // The first phase may be cut short; every later one is long.
        assert!(lengths.len() >= 4, "{lengths:?}");
        for length in &lengths[1..] {
            assert!(*length >= 600.0, "{lengths:?}");
        }
        // The fade: from clear, 45 seconds in, half the fall.
        let mut weather = WorldWeather::arrive(PrecipKind::Rain, 99, Some(false));
        weather.falling = true;
        weather.phase_left = 1000.0;
        weather.tick(45.0, 1.0);
        assert!((weather.fall() - 0.5).abs() < 0.01);
    }

    #[test]
    fn rain_soaks_slowly_and_dries_slower() {
        let mut weather = WorldWeather::arrive(PrecipKind::Rain, 5, Some(false));
        assert_eq!(weather.wet_ground(), 0.0);
        weather.falling = true;
        weather.phase_left = 10_000.0;
        for _ in 0..120 {
            weather.tick(1.0, 1.0);
        }
        // Two minutes in, the fade still rising: barely damp.
        assert!(weather.wet_ground() < 0.3, "{}", weather.wet_ground());
        for _ in 0..600 {
            weather.tick(1.0, 1.0);
        }
        assert_eq!(weather.wet_ground(), 1.0);
        weather.falling = false;
        for _ in 0..360 {
            weather.tick(1.0, 1.0);
        }
        // Six minutes after, still half wet.
        assert!(
            (0.4..0.75).contains(&weather.wet_ground()),
            "{}",
            weather.wet_ground()
        );
    }

    /// Seconds of ticking until the puddles first stand and are full.
    fn puddle_times(weather: &mut WorldWeather) -> (f32, f32) {
        let (mut first, mut full) = (None, None);
        for second in 1..=1200 {
            weather.tick(1.0, 1.0);
            if first.is_none() && weather.puddles() > 0.0 {
                first = Some(second as f32);
            }
            if full.is_none() && weather.puddles() >= 1.0 {
                full = Some(second as f32);
            }
        }
        (first.unwrap(), full.unwrap())
    }

    #[test]
    fn puddles_stand_a_minute_to_five_after_the_rain_comes() {
        // Arriving in the rain (not the harness): dry ground, no puddles.
        let arrived = (0..400)
            .map(|visit| WorldWeather::arrive(PrecipKind::Rain, visit_seed(1, visit * 41), None))
            .find(|weather| weather.falling)
            .unwrap();
        assert_eq!(arrived.wet_ground(), 0.0);
        assert_eq!(arrived.puddles(), 0.0);
        let mut weather = arrived;
        weather.phase_left = 10_000.0;
        let (first, full) = puddle_times(&mut weather);
        assert!((55.0..=65.0).contains(&first), "{first}");
        assert!((270.0..=300.0).contains(&full), "{full}");
        // From a clear sky, the fall fading in first.
        let mut weather = WorldWeather::arrive(PrecipKind::Rain, 5, Some(false));
        weather.falling = true;
        weather.phase_left = 10_000.0;
        let (first, full) = puddle_times(&mut weather);
        assert!((60.0..=120.0).contains(&first), "{first}");
        assert!((300.0..=345.0).contains(&full), "{full}");
        // The harness soaks the ground at once.
        let forced = WorldWeather::arrive(PrecipKind::Rain, 5, Some(true));
        assert_eq!(forced.puddles(), 1.0);
    }

    /// A rain phase whose rain does not fall (Pelagia's by day, which waits
    /// for the night, or with the rework off) leaves the ground dry: no
    /// puddle stands before it has rained. Once night lets it fall, the
    /// puddles come as from a dry start.
    #[test]
    fn no_puddles_before_the_rain_falls() {
        let mut weather = WorldWeather::arrive(PrecipKind::Rain, 5, Some(false));
        weather.falling = true;
        weather.phase_left = 10_000.0;
        for _ in 0..1800 {
            weather.tick(1.0, 0.0);
            assert_eq!(weather.wet_ground(), 0.0);
            assert_eq!(weather.puddles(), 0.0);
        }
        assert_eq!(weather.fall(), 1.0);
        // Dusk: a little rain soaks a little, and puddles still wait.
        for _ in 0..60 {
            weather.tick(1.0, 0.1);
        }
        assert!(weather.wet_ground() < 0.05, "{}", weather.wet_ground());
        assert_eq!(weather.puddles(), 0.0);
        // Night: the rain falls whole.
        let mut night = WorldWeather::arrive(PrecipKind::Rain, 5, Some(false));
        night.falling = true;
        night.phase_left = 10_000.0;
        night.fall = 1.0;
        let mut first = None;
        for second in 1..=600 {
            night.tick(1.0, 1.0);
            if first.is_none() && night.puddles() > 0.0 {
                first = Some(second);
            }
        }
        assert!((55..=65).contains(&first.unwrap()), "{first:?}");
    }

    #[test]
    fn a_network_games_weather_is_retails_none() {
        let mut weather = WorldWeather::retail(PrecipKind::Rain);
        for _ in 0..7200 {
            weather.tick(1.0, 1.0);
        }
        assert_eq!(
            (weather.fall(), weather.wet_ground(), weather.puddles()),
            (0.0, 0.0, 0.0)
        );
        assert_eq!(weather.wet_models(), 0.0);
        // Ash is the world's look, not weather: it keeps falling.
        assert_eq!(WorldWeather::retail(PrecipKind::Ash).fall(), 1.0);
    }

    #[test]
    fn snow_leaves_the_ground_dry_and_the_models_faintly_damp() {
        let weather = WorldWeather::arrive(PrecipKind::Snow, 3, Some(true));
        assert_eq!(weather.wet_ground(), 0.0);
        assert!((weather.wet_models() - SNOW_ON_MODELS).abs() < 1.0e-6);
    }

    #[test]
    fn visits_roll_both_ways() {
        let falling = (0..200)
            .filter(|visit| {
                WorldWeather::arrive(PrecipKind::Rain, visit_seed(9, visit * 37), None).falling
            })
            .count();
        assert!((50..150).contains(&falling), "{falling} of 200");
    }
}
