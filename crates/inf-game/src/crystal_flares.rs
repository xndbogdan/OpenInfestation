//! The scenery crystals drinking the guns' fire: a plasma bolt passing through a crystal (`xshard*`, `xclump*`) makes
//! it flare a strong blue, a laser beam through one a yellow, fading over
//! about a second, and a shield's sound rings from it. Only a look and a
//! sound: the shot flies on as in retail.
//!
//! A crystal is struck only when a shot really meets it (they light up only
//! when actually struck): a bolt the
//! game's own projectile collision stops on the crystal's hull
//! ([`FurnitureHit`]), or any shot whose line crosses the crystal's own
//! faces, placed as its draw places them (its cell, at the ground's height
//! there, turned by its record's state): a bolt along the way it moved since
//! the last frame, a laser along its beam. A shot merely near a crystal
//! strikes nothing. The world keeps each crystal's flare, by its
//! cell, for the tracer's crystals to read ([`crate::software_scene`]'s
//! `crystal_catch`), and a fresh strike plays its sound through the
//! ambience.

use std::collections::HashMap;
use std::sync::Arc;

use crate::process_particles::splashes::ParticleBolt;
use crate::terrain_furniture::draw::FurnitureDrawState;
use crate::world_backend::WorldSession;

/// How much faster than real time a spark runs through its crystal.
const SPARK_PACE: f32 = 1.5;
/// A crystal struck again within this long (microseconds) is the same bolt
/// still passing through it, which strikes once; each of a burst's bolts
/// strikes anew.
const RESTRIKE: u64 = 120_000;
/// `00465660` moves a particle `velocity * (delta >> 5) >> 15` units a
/// step: this many units a second for each velocity unit.
const VELOCITY_UNITS_PER_SECOND: f32 = 1.0e6 / 32.0 / 32768.0;
/// The longest a bolt's way back is traced: a stalled frame does not turn
/// it into a long line through the scenery.
const MOST_SECONDS: f32 = 0.1;
/// The pairs of points on a struck crystal's faces its arcs jump between
/// (`pt_composite.frag`'s `crystal_arcs`), and how far apart a pair lies, as
/// shares of the crystal's reach.
pub const ARC_PAIRS: usize = 6;
const ARC_APART: [f32; 3] = [0.12, 0.3, 0.5];

/// Pairs of points on a crystal's faces, world words.
pub type ArcAnchors = [[[u16; 3]; 2]; ARC_PAIRS];

/// What struck a crystal, which picks the colour it flares.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StrikeKind {
    /// A gun's bolt (the plasma's among them): the flare's blue.
    #[default]
    Plasma,
    /// A laser beam: the flare's yellow.
    Laser,
}

/// A gun's shot this frame, as the crystals meet it: the line it covers
/// (world words, wrapping at 16 bits) and what it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shot {
    pub start: [u16; 3],
    pub end: [u16; 3],
    pub kind: StrikeKind,
}

/// Each crystal's flare (1 at a strike, fading to 0, with what struck it),
/// when a shot was last inside it, and its spark, by its cell.
#[derive(Debug, Default)]
pub struct CrystalFlares {
    levels: HashMap<[u16; 2], (f32, StrikeKind)>,
    struck: HashMap<[u16; 2], u64>,
    sparks: HashMap<[u16; 2], Spark>,
    last: Option<u64>,
    /// The bolts' furniture contacts since the last frame's update, which
    /// the projectile collision adds and [`Self::update`] clears.
    pub furniture_hits: Vec<FurnitureHit>,
}

/// A crystal struck: its cell, its middle from the camera's eye (world
/// units, Y up) and in world words, where the shot entered it (world
/// words), how far the crystal reaches from its middle, and what struck it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Strike {
    pub cell: [u16; 2],
    pub offset: [f32; 3],
    pub middle: [u16; 3],
    /// Where its arcs jump, on its faces (`Crystal::anchors`).
    pub anchors: ArcAnchors,
    pub hit: [u16; 3],
    pub radius: f32,
    pub kind: StrikeKind,
}

/// A spark running through a struck crystal (the plasma spreads
/// through it while disappearing from the place where it was shot):
/// where the shot struck, the crystal's middle (the arcs crawl round
/// it), when, the crystal's reach and what struck.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Spark {
    hit: [u16; 3],
    middle: [u16; 3],
    anchors: ArcAnchors,
    at: u64,
    radius: f32,
    kind: StrikeKind,
}

/// A spark as a draw takes it: where it struck and the crystal's middle,
/// from the eye; its age and whole life in seconds; how far the crystal
/// reaches; and its light, the flare's colour for what struck it times its
/// strength.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrystalSpark {
    pub hit: [f32; 3],
    pub middle: [f32; 3],
    /// The pairs of points on its faces its arcs jump between, from the eye.
    pub anchors: [[[f32; 3]; 2]; ARC_PAIRS],
    pub age: f32,
    pub seconds: f32,
    pub radius: f32,
    pub light: [f32; 3],
}

impl CrystalSpark {
    /// Its glow at a point `distance` from where it struck.
    pub fn glow(&self, distance: f32) -> f32 {
        spark_glow(distance, self.age, self.radius, self.seconds)
    }
}

/// A spark's glow at `distance` from where it struck, `age` seconds after,
/// in a crystal reaching `radius`, living `seconds`: a bright point where
/// it struck that is gone in a tenth of a second, and a front spreading
/// from there through the crystal in a third of a second, dimming as it
/// goes; nothing is left behind it. `pt_trace.comp`'s `spark_glow` is the
/// same.
pub fn spark_glow(distance: f32, age: f32, radius: f32, seconds: f32) -> f32 {
    if !(0.0..seconds).contains(&age) {
        return 0.0;
    }
    let radius = radius.max(1.0);
    let speed = radius * 1.2 / 0.35;
    let front = speed * age;
    let width = 0.22 * radius;
    let band = (-((distance - front) / width).powi(2)).exp()
        * (1.0 - smoothstep(0.2, seconds.max(0.3), age));
    let core = 2.0 * (-(distance / (0.12 * radius)).powi(2)).exp() * (-age * 14.0).exp();
    (band + core).min(2.0)
}

fn smoothstep(from: f32, to: f32, value: f32) -> f32 {
    let t = ((value - from) / (to - from)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl CrystalFlares {
    /// The flare of the crystal at `cell`, 0 to 1, and what struck it.
    pub fn flare(&self, cell: [u16; 2]) -> Option<(f32, StrikeKind)> {
        self.levels.get(&cell).copied()
    }

    /// Every flaring crystal's cell and flare, 0 to 1.
    pub fn levels(&self) -> Vec<([u16; 2], f32)> {
        self.levels
            .iter()
            .map(|(cell, (level, _))| (*cell, *level))
            .collect()
    }

    /// The flare of the crystal at `cell`, 0 to 1.
    pub fn level(&self, cell: [u16; 2]) -> f32 {
        self.flare(cell).map_or(0.0, |(level, _)| level)
    }

    /// The spark running through the crystal at `cell`, if any, as a draw
    /// from `eye` takes it under `flare`.
    pub fn spark(
        &self,
        cell: [u16; 2],
        eye: [f64; 3],
        flare: &crate::modern_tuning::CrystalFlareTuning,
    ) -> Option<CrystalSpark> {
        let spark = self.sparks.get(&cell)?;
        Some(self.as_drawn(spark, eye, flare))
    }

    /// Every spark running now, as a draw from `eye` takes it.
    pub fn sparks(
        &self,
        eye: [f64; 3],
        flare: &crate::modern_tuning::CrystalFlareTuning,
    ) -> Vec<CrystalSpark> {
        self.sparks
            .values()
            .map(|spark| self.as_drawn(spark, eye, flare))
            .collect()
    }

    fn as_drawn(
        &self,
        spark: &Spark,
        eye: [f64; 3],
        flare: &crate::modern_tuning::CrystalFlareTuning,
    ) -> CrystalSpark {
        // The spark plays half as fast again as it lives (so the
        // effect feels snappier); the light under the crystal keeps its own pace.
        let age = SPARK_PACE
            * self
                .last
                .map_or(0.0, |now| now.saturating_sub(spark.at) as f32 / 1.0e6);
        let from_eye = |word: [u16; 3]| -> [f32; 3] {
            std::array::from_fn(|axis| {
                ((f64::from(word[axis]) - eye[axis] + 32768.0).rem_euclid(65536.0) - 32768.0) as f32
            })
        };
        CrystalSpark {
            hit: from_eye(spark.hit),
            middle: from_eye(spark.middle),
            anchors: spark.anchors.map(|pair| pair.map(from_eye)),
            age,
            seconds: flare.seconds.max(0.3),
            radius: spark.radius,
            light: flare
                .color_of(spark.kind)
                .map(|channel| channel.max(0.0) * flare.strength.max(0.0)),
        }
    }

    /// Fade every flare toward nothing over `seconds` from the last call
    /// (`now`, microseconds), light those `strikes` reach, and give back
    /// the fresh ones. A laser held through a crystal sparks again each
    /// time its last spark has run out.
    pub fn update(&mut self, strikes: &[Strike], now: u64, seconds: f32) -> Vec<Strike> {
        let elapsed = self
            .last
            .map_or(0.0, |last| now.saturating_sub(last) as f32 / 1.0e6);
        self.last = Some(now);
        let fade = elapsed / seconds.max(0.05);
        self.levels.retain(|_, (level, _)| {
            *level -= fade;
            *level > 0.0
        });
        self.struck
            .retain(|_, at| now.saturating_sub(*at) < RESTRIKE);
        // A spark lives as long as it shows: its life, played faster.
        let life = (seconds.max(0.3) / SPARK_PACE * 1.0e6) as u64;
        self.sparks
            .retain(|_, spark| now.saturating_sub(spark.at) < life);
        let mut fresh = Vec::new();
        for strike in strikes {
            let new = self.struck.insert(strike.cell, now).is_none();
            if new {
                fresh.push(*strike);
            }
            if new || (strike.kind == StrikeKind::Laser && !self.sparks.contains_key(&strike.cell))
            {
                self.sparks.insert(
                    strike.cell,
                    Spark {
                        hit: strike.hit,
                        middle: strike.middle,
                        anchors: strike.anchors,
                        at: now,
                        radius: strike.radius,
                        kind: strike.kind,
                    },
                );
            }
            self.levels.insert(strike.cell, (1.0, strike.kind));
        }
        self.furniture_hits.clear();
        fresh
    }
}

/// A crystal model's faces as triangles in its own frame (world units, Y
/// up), with the sphere round them.
#[derive(Debug)]
struct CrystalMesh {
    triangles: Vec<[[f32; 3]; 3]>,
    centre: [f32; 3],
    radius: f32,
}

impl CrystalMesh {
    /// The model's static faces (`inf_formats`' proven projection, fans and
    /// quads cut into triangles). A model whose faces cannot be proven
    /// statically falls back to the box round its vertices, logged once:
    /// CrystalMeshBoxApproximation.
    fn of(model: &inf_formats::ModelEntry, name: &str) -> Option<Self> {
        let mut triangles = Vec::new();
        if let inf_formats::ModelDiagnosticProjection::Ready(geometry) =
            inf_formats::analyze_model_diagnostic(
                inf_formats::ModelDiagnosticRequest::unconditioned(model),
            )
        {
            let point = |index: usize| geometry.vertices[index].position.map(|word| word as f32);
            for face in &geometry.faces {
                for k in 1..face.len().saturating_sub(1) {
                    triangles.push([point(face[0]), point(face[k]), point(face[k + 1])]);
                }
            }
        }
        if triangles.is_empty() {
            let low: [i16; 3] = std::array::from_fn(|axis| {
                model
                    .vertices
                    .iter()
                    .map(|vertex| [vertex.x, vertex.y, vertex.z][axis])
                    .min()
                    .unwrap_or(0)
            });
            let high: [i16; 3] = std::array::from_fn(|axis| {
                model
                    .vertices
                    .iter()
                    .map(|vertex| [vertex.x, vertex.y, vertex.z][axis])
                    .max()
                    .unwrap_or(0)
            });
            if model.vertices.is_empty() {
                return None;
            }
            eprintln!(
                "CrystalMeshBoxApproximation: crystal {name} has no statically proven faces; \
                 its strikes take the box round its vertices"
            );
            triangles = box_triangles(low.map(f32::from), high.map(f32::from));
        }
        let corners = triangles.iter().flatten();
        let low: [f32; 3] =
            std::array::from_fn(|axis| corners.clone().map(|p| p[axis]).fold(f32::MAX, f32::min));
        let high: [f32; 3] =
            std::array::from_fn(|axis| corners.clone().map(|p| p[axis]).fold(f32::MIN, f32::max));
        let centre = std::array::from_fn(|axis| 0.5 * (low[axis] + high[axis]));
        let radius = corners
            .map(|p| distance(*p, centre))
            .fold(0.0_f32, f32::max);
        Some(Self {
            triangles,
            centre,
            radius,
        })
    }

    /// Where along the segment `a` to `b` (its own frame) it first enters
    /// the mesh, 0 to 1; or 1 where the segment ends inside it (a short
    /// step within a large crystal crosses no face). None where it misses.
    fn entry(&self, a: [f32; 3], b: [f32; 3]) -> Option<f32> {
        if segment_distance(a, b, self.centre) > self.radius {
            return None;
        }
        let first = self
            .triangles
            .iter()
            .filter_map(|triangle| segment_triangle(a, b, triangle))
            .fold(None, |best: Option<f32>, t| {
                Some(best.map_or(t, |best| best.min(t)))
            });
        if first.is_some() {
            return first;
        }
        // Inside: a ray up from the end crosses the faces an odd number of
        // times.
        let up = [
            b[0],
            b[1] + 2.0 * self.radius + distance(b, self.centre),
            b[2],
        ];
        let crossings = self
            .triangles
            .iter()
            .filter(|triangle| segment_triangle(b, up, triangle).is_some())
            .count();
        (crossings % 2 == 1).then_some(1.0)
    }
}

fn box_triangles(low: [f32; 3], high: [f32; 3]) -> Vec<[[f32; 3]; 3]> {
    let corner = |i: usize| {
        [
            if i & 1 == 0 { low[0] } else { high[0] },
            if i & 2 == 0 { low[1] } else { high[1] },
            if i & 4 == 0 { low[2] } else { high[2] },
        ]
    };
    [
        [0, 1, 3, 2],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 3, 7, 5],
    ]
    .iter()
    .flat_map(|[a, b, c, d]| {
        [
            [corner(*a), corner(*b), corner(*c)],
            [corner(*a), corner(*c), corner(*d)],
        ]
    })
    .collect()
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    dot(d, d).sqrt()
}

/// How far `point` lies from the segment `a` to `b`.
fn segment_distance(a: [f32; 3], b: [f32; 3], point: [f32; 3]) -> f32 {
    let along = sub(b, a);
    let length = dot(along, along);
    let t = if length > 0.0 {
        (dot(sub(point, a), along) / length).clamp(0.0, 1.0)
    } else {
        0.0
    };
    distance(point, std::array::from_fn(|axis| a[axis] + along[axis] * t))
}

/// Where along the segment `a` to `b` it crosses `triangle`, 0 to 1, from
/// either side (Möller and Trumbore).
fn segment_triangle(a: [f32; 3], b: [f32; 3], triangle: &[[f32; 3]; 3]) -> Option<f32> {
    let direction = sub(b, a);
    let edge1 = sub(triangle[1], triangle[0]);
    let edge2 = sub(triangle[2], triangle[0]);
    let p = cross(direction, edge2);
    let determinant = dot(edge1, p);
    if determinant.abs() < 1.0e-6 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let s = sub(a, triangle[0]);
    let u = dot(s, p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, edge1);
    let v = dot(direction, q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(edge2, q) * inverse;
    (0.0..=1.0).contains(&t).then_some(t)
}

/// A crystal: its cell, where its draw stands it (world words), its axes
/// as rows (from the record's basis, as its draw takes them) and its faces.
#[derive(Clone, Debug)]
struct Crystal {
    cell: [u16; 2],
    origin: [u16; 3],
    rows: [[f32; 3]; 3],
    mesh: Arc<CrystalMesh>,
}

impl Crystal {
    /// `point` (world words) in the crystal's own frame.
    fn local(&self, point: [u16; 3]) -> [f32; 3] {
        let offset: [f32; 3] = std::array::from_fn(|axis| {
            f32::from(point[axis].wrapping_sub(self.origin[axis]) as i16)
        });
        std::array::from_fn(|row| dot(self.rows[row], offset))
    }

    /// A point of its own frame in world units from its origin.
    fn world(&self, local: [f32; 3]) -> [f32; 3] {
        std::array::from_fn(|axis| (0..3).map(|row| local[row] * self.rows[row][axis]).sum())
    }

    /// The pairs of points on its faces a strike at `hit` (world words)
    /// throws its arcs between: the first from where it was struck, each
    /// [`ARC_APART`] of its reach apart (about), on its own faces; the same
    /// for the same strike.
    fn anchors(&self, hit: [u16; 3]) -> ArcAnchors {
        let mut seed =
            (u32::from(hit[0]) ^ (u32::from(hit[1]) << 11) ^ (u32::from(hit[2]) << 21)) | 1;
        let mut unit = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed >> 8) as f32 / 16_777_216.0
        };
        let triangles = &self.mesh.triangles;
        let on_face = |unit: &mut dyn FnMut() -> f32| -> [f32; 3] {
            let corners = triangles[(unit() * triangles.len() as f32) as usize % triangles.len()];
            let (mut u, mut v) = (unit(), unit());
            if u + v > 1.0 {
                (u, v) = (1.0 - u, 1.0 - v);
            }
            std::array::from_fn(|axis| {
                corners[0][axis]
                    + (corners[1][axis] - corners[0][axis]) * u
                    + (corners[2][axis] - corners[0][axis]) * v
            })
        };
        let word = |local: [f32; 3]| -> [u16; 3] {
            let at = self.world(local);
            std::array::from_fn(|axis| {
                self.origin[axis].wrapping_add(at[axis].round() as i32 as u16)
            })
        };
        let radius = self.mesh.radius.max(1.0);
        std::array::from_fn(|pair| {
            let from = if pair == 0 {
                self.local(hit)
            } else {
                on_face(&mut unit)
            };
            // Of a dozen points on its faces, the one nearest the pair's
            // spacing from the first.
            let wanted = ARC_APART[pair % ARC_APART.len()] * radius;
            let mut best = (f32::MAX, from);
            for _ in 0..12 {
                let to = on_face(&mut unit);
                let off = (distance(from, to) - wanted).abs();
                if off < best.0 {
                    best = (off, to);
                }
            }
            [word(from), word(best.1)]
        })
    }

    /// Its middle, where it stands (world words).
    fn middle(&self) -> [u16; 3] {
        let centre = self.world(self.mesh.centre);
        std::array::from_fn(|axis| {
            self.origin[axis].wrapping_add(centre[axis].round() as i32 as u16)
        })
    }

    /// Where `shot` strikes it, if it does. A laser aimed at the crystal
    /// ends where the crosshair's sight met its collision hull, which
    /// stands out from its faces: a beam ending within the crystal's reach
    /// strikes where its line on meets them.
    fn struck_by(&self, shot: &Shot, eye: [u16; 3]) -> Option<Strike> {
        self.struck_along(shot, eye).or_else(|| {
            let on = self
                .past_hull(shot)
                .filter(|_| shot.kind == StrikeKind::Laser)?;
            self.struck_along(&on, eye)
        })
    }

    /// A shot ending within the crystal's reach, carried on along its line
    /// past the crystal's far side; none for one ending farther off.
    fn past_hull(&self, shot: &Shot) -> Option<Shot> {
        if distance(self.local(shot.end), self.mesh.centre) > self.mesh.radius {
            return None;
        }
        let along: [f32; 3] = std::array::from_fn(|axis| {
            f32::from(shot.end[axis].wrapping_sub(shot.start[axis]) as i16)
        });
        let length = dot(along, along).sqrt();
        if length < 1.0 {
            return None;
        }
        let reach = 2.0 * self.mesh.radius / length;
        Some(Shot {
            start: shot.end,
            end: std::array::from_fn(|axis| {
                shot.end[axis].wrapping_add((along[axis] * reach).round() as i32 as u16)
            }),
            kind: shot.kind,
        })
    }

    /// Where `shot`'s own line strikes it, if it does.
    fn struck_along(&self, shot: &Shot, eye: [u16; 3]) -> Option<Strike> {
        let t = self
            .mesh
            .entry(self.local(shot.start), self.local(shot.end))?;
        let hit = std::array::from_fn(|axis| {
            let span = f32::from(shot.end[axis].wrapping_sub(shot.start[axis]) as i16);
            shot.start[axis].wrapping_add((span * t).round() as i32 as u16)
        });
        let centre = self.world(self.mesh.centre);
        Some(Strike {
            cell: self.cell,
            offset: std::array::from_fn(|axis| {
                f32::from(self.origin[axis].wrapping_sub(eye[axis]) as i16) + centre[axis]
            }),
            middle: self.middle(),
            anchors: self.anchors(hit),
            hit,
            radius: self.mesh.radius.max(1.0),
            kind: shot.kind,
        })
    }
}

/// The crystals of the world in play, found once as it loads, and when the
/// survey last ran.
#[derive(Debug, Default)]
pub struct CrystalSurvey {
    world: Option<u32>,
    crystals: Vec<Crystal>,
    last: Option<u64>,
}

impl CrystalSurvey {
    /// The crystals the world's shots strike this frame (`now`,
    /// microseconds): its `bolts` along the way each moved since the last
    /// frame, and its lasers along their beams.
    pub fn survey(
        &mut self,
        world: &WorldSession,
        residency: &crate::ResidentRuntime,
        furniture: &FurnitureDrawState,
        bolts: &[ParticleBolt],
        now: u64,
    ) -> Vec<Strike> {
        let number = world.mission.current_world_a0;
        if self.world != Some(number) {
            self.world = Some(number);
            self.crystals = crystals(world, residency, furniture);
        }
        let seconds = self
            .last
            .map_or(1.0 / 60.0, |last| now.saturating_sub(last) as f32 / 1.0e6)
            .min(MOST_SECONDS);
        self.last = Some(now);
        let Some(draw) = world.last_draw else {
            return Vec::new();
        };
        let eye = draw.eye.map(|word| word as u16);
        let mut shots: Vec<Shot> = bolts.iter().map(|bolt| bolt_shot(bolt, seconds)).collect();
        shots.extend(laser_shots(&world.arena));
        let mut struck = contact_strikes(&self.crystals, &world.crystal_flares.furniture_hits, eye);
        for strike in strikes(&self.crystals, &shots, eye) {
            if !struck.iter().any(|known| known.cell == strike.cell) {
                struck.push(strike);
            }
        }
        struck
    }
}

/// A gun's bolt that met a furniture item this tick (the game's own
/// projectile collision, `004653B0`): the item's cell and the probe's
/// refined centre (world words). A crystal's collision hull stands out
/// from its faces, so the bolt dies there before it would reach them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FurnitureHit {
    pub cell: [u16; 2],
    pub at: [u16; 3],
}

/// The crystals the game's own collision says bolts met, each once, the
/// spark placed where the bolt's line toward the crystal's middle meets its
/// faces.
fn contact_strikes(crystals: &[Crystal], hits: &[FurnitureHit], eye: [u16; 3]) -> Vec<Strike> {
    let mut struck: Vec<Strike> = Vec::new();
    for hit in hits {
        let Some(crystal) = crystals.iter().find(|crystal| crystal.cell == hit.cell) else {
            continue;
        };
        if struck.iter().any(|known| known.cell == crystal.cell) {
            continue;
        }
        let centre = crystal.world(crystal.mesh.centre);
        let middle: [u16; 3] = std::array::from_fn(|axis| {
            crystal.origin[axis].wrapping_add(centre[axis].round() as i32 as u16)
        });
        let toward = Shot {
            start: hit.at,
            end: middle,
            kind: StrikeKind::Plasma,
        };
        let strike = crystal.struck_by(&toward, eye).unwrap_or(Strike {
            cell: crystal.cell,
            middle,
            anchors: crystal.anchors(hit.at),
            offset: std::array::from_fn(|axis| {
                f32::from(crystal.origin[axis].wrapping_sub(eye[axis]) as i16) + centre[axis]
            }),
            hit: hit.at,
            radius: crystal.mesh.radius.max(1.0),
            kind: StrikeKind::Plasma,
        });
        struck.push(strike);
    }
    struck
}

/// A bolt's way since the last frame, `seconds` ago.
fn bolt_shot(bolt: &ParticleBolt, seconds: f32) -> Shot {
    let back = |axis: usize| {
        (f32::from(bolt.velocity[axis]) * VELOCITY_UNITS_PER_SECOND * seconds).round() as i32
    };
    Shot {
        start: std::array::from_fn(|axis| bolt.position[axis].wrapping_sub(back(axis) as u16)),
        end: bolt.position,
        kind: StrikeKind::Plasma,
    }
}

/// The lasers drawn this frame, each from its muzzle along its aim as its
/// draw (`00485520`) lays the beam: the aim its last draw reached.
fn laser_shots(arena: &crate::object_runtime::ObjectArena) -> Vec<Shot> {
    use crate::object_runtime::ObjectList;
    let mut seen = std::collections::HashSet::new();
    let mut shots = Vec::new();
    for list in [ObjectList::Flag0Primary, ObjectList::Flag0Secondary] {
        for node in arena.list(list) {
            let laser = node.object;
            if !seen.insert(laser) || !arena.is_laser31(laser) {
                continue;
            }
            let (Ok(beam), Some(origin)) = (
                arena.laser31_beam::<()>(laser),
                arena
                    .object(laser)
                    .ok()
                    .and_then(|object| object.base.position_words),
            ) else {
                continue;
            };
            shots.push(Shot {
                start: origin,
                end: std::array::from_fn(|axis| origin[axis].wrapping_add(beam.aim[axis] as u16)),
                kind: StrikeKind::Laser,
            });
        }
    }
    shots
}

/// The world's crystals: its furniture the crystals' tuning names, each
/// where its draw stands it.
fn crystals(
    world: &WorldSession,
    residency: &crate::ResidentRuntime,
    furniture: &FurnitureDrawState,
) -> Vec<Crystal> {
    let tuning = crate::modern_tuning::current();
    let names = tuning.crystals();
    let Ok(table) = crate::world_furniture_effect::resident::resident_placement_table(residency)
    else {
        return Vec::new();
    };
    let mut meshes: HashMap<u16, Option<Arc<CrystalMesh>>> = HashMap::new();
    let mut found = Vec::new();
    for bucket in world.terrain.buckets() {
        for record in &bucket.records {
            let lookup = usize::from(record.resource_lookup.raw());
            let Some(slot) = table.slots.get(lookup).filter(|_| lookup != 0) else {
                continue;
            };
            let mesh = meshes.entry(slot.resource_word).or_insert_with(|| {
                let binding =
                    crate::object_runtime::resident_model_resource(residency, slot.resource_word)
                        .ok()??;
                let name = binding
                    .model
                    .name
                    .as_ref()
                    .and_then(|name| name.ascii(binding.allocation.bytes()))?;
                if !names.is_crystal(name) {
                    return None;
                }
                CrystalMesh::of(binding.model, name).map(Arc::new)
            });
            let Some(mesh) = mesh.clone() else {
                continue;
            };
            // 0044CEF0's place: the record's cell, at its height, turned
            // by the record's basis (ultra_footings.rs reads it alike).
            let [x, z] = [record.x << 7, bucket.y << 7];
            let ground = world.terrain.raw_height_at([x, z]) as u16;
            let basis = furniture.basis(record.state).words;
            found.push(Crystal {
                cell: [x, z],
                origin: [x, ground, z],
                rows: [1, 4, 7]
                    .map(|first| std::array::from_fn(|k| f32::from_bits(basis[first + k]))),
                mesh,
            });
        }
    }
    found
}

/// The crystals the shots strike, each once (its first shot's strike), with
/// its middle from `eye`.
fn strikes(crystals: &[Crystal], shots: &[Shot], eye: [u16; 3]) -> Vec<Strike> {
    crystals
        .iter()
        .filter_map(|crystal| shots.iter().find_map(|shot| crystal.struck_by(shot, eye)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A crystal standing at `origin`: a box 100 across and 200 tall from its
    /// foot, unturned.
    fn crystal(origin: [u16; 3]) -> Crystal {
        let triangles = box_triangles([-50.0, 0.0, -50.0], [50.0, 200.0, 50.0]);
        Crystal {
            cell: [origin[0], origin[2]],
            origin,
            rows: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            mesh: Arc::new(CrystalMesh {
                triangles,
                centre: [0.0, 100.0, 0.0],
                radius: 122.5,
            }),
        }
    }

    fn shot(start: [u16; 3], end: [u16; 3], kind: StrikeKind) -> Shot {
        Shot { start, end, kind }
    }

    /// A shot through a crystal's faces strikes it where it enters; one
    /// passing beside it, however close, does not (a shot near it must not
    /// light it); the strike's place
    /// is the crystal's middle from the eye, across the ground's wrap.
    #[test]
    fn only_a_shot_through_its_faces_strikes_a_crystal() {
        let crystals = [crystal([1_000, 0, 2_000])];
        let through = shot([800, 100, 2_000], [1_200, 100, 2_000], StrikeKind::Plasma);
        let struck = strikes(&crystals, &[through], [900, 0, 2_000]);
        assert_eq!(struck.len(), 1);
        assert_eq!(struck[0].hit, [950, 100, 2_000]);
        assert_eq!(struck[0].offset, [100.0, 100.0, 0.0]);
        assert_eq!(struck[0].kind, StrikeKind::Plasma);
        // Ten units beside its face, and over its top.
        let beside = shot([800, 100, 2_060], [1_200, 100, 2_060], StrikeKind::Plasma);
        let over = shot([800, 210, 2_000], [1_200, 210, 2_000], StrikeKind::Plasma);
        assert!(strikes(&crystals, &[beside, over], [900, 0, 2_000]).is_empty());
        // Across the wrap.
        let wrapped = [crystal([10, 0, 3_000])];
        let across = shot([65_400, 50, 3_000], [100, 50, 3_000], StrikeKind::Laser);
        let struck = strikes(&wrapped, &[across], [65_400, 0, 3_000]);
        assert_eq!(struck.len(), 1);
        assert_eq!(struck[0].offset, [146.0, 100.0, 0.0]);
        assert_eq!(struck[0].kind, StrikeKind::Laser);
    }

    /// A bolt the game's collision stopped on a crystal's hull strikes it,
    /// once however many probes met it, its spark on the face toward the
    /// bolt; a contact with other furniture strikes nothing.
    #[test]
    fn a_collision_contact_strikes_its_crystal() {
        let crystals = [crystal([1_000, 0, 2_000])];
        let hits = [
            FurnitureHit {
                cell: [1_000, 2_000],
                at: [880, 100, 2_000],
            },
            FurnitureHit {
                cell: [1_000, 2_000],
                at: [885, 100, 2_000],
            },
            FurnitureHit {
                cell: [5_000, 5_000],
                at: [5_000, 50, 5_000],
            },
        ];
        let struck = contact_strikes(&crystals, &hits, [0; 3]);
        assert_eq!(struck.len(), 1);
        assert_eq!(struck[0].hit, [950, 100, 2_000]);
        assert_eq!(struck[0].kind, StrikeKind::Plasma);
    }

    /// A laser aimed with the crosshair on a crystal ends where the sight
    /// met its hull, short of its faces: it strikes the face its line meets
    /// on; a beam ending beyond the crystal's reach, and a bolt, do not.
    #[test]
    fn a_laser_stopped_on_the_hull_strikes_its_crystal() {
        let crystals = [crystal([1_000, 0, 2_000])];
        let stopped = shot([600, 100, 2_000], [920, 100, 2_000], StrikeKind::Laser);
        let struck = strikes(&crystals, &[stopped], [0; 3]);
        assert_eq!(struck.len(), 1);
        assert_eq!(struck[0].hit, [950, 100, 2_000]);
        let short = shot([600, 100, 2_000], [860, 100, 2_000], StrikeKind::Laser);
        let bolt = shot([600, 100, 2_000], [920, 100, 2_000], StrikeKind::Plasma);
        assert!(strikes(&crystals, &[short, bolt], [0; 3]).is_empty());
    }

    /// A bolt's short step inside a large crystal strikes it though it
    /// crosses no face; one wholly outside does not.
    #[test]
    fn a_step_inside_a_crystal_strikes_it() {
        let crystals = [crystal([1_000, 0, 2_000])];
        let inside = shot([990, 100, 2_000], [1_010, 100, 2_000], StrikeKind::Plasma);
        assert_eq!(
            strikes(&crystals, &[inside], [0; 3])[0].hit,
            [1_010, 100, 2_000]
        );
        let outside = shot([1_100, 100, 2_000], [1_120, 100, 2_000], StrikeKind::Plasma);
        assert!(strikes(&crystals, &[outside], [0; 3]).is_empty());
    }

    /// A crystal turned by its record's basis is struck where its turned
    /// faces stand: a slab long in its own X, turned a quarter, is long in Z.
    #[test]
    fn a_turned_crystal_is_struck_where_it_stands() {
        let mut slab = crystal([1_000, 0, 1_000]);
        slab.mesh = Arc::new(CrystalMesh {
            triangles: box_triangles([-300.0, 0.0, -20.0], [300.0, 100.0, 20.0]),
            centre: [0.0, 50.0, 0.0],
            radius: 305.0,
        });
        slab.rows = [[0.0, 0.0, 1.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        let along_z = shot([900, 50, 1_250], [1_100, 50, 1_250], StrikeKind::Plasma);
        let along_x = shot([1_250, 50, 900], [1_250, 50, 1_100], StrikeKind::Plasma);
        assert_eq!(strikes(&[slab.clone()], &[along_z], [0; 3]).len(), 1);
        assert!(strikes(&[slab], &[along_x], [0; 3]).is_empty());
    }

    /// Terra Nova's clumps (world 2) and Stendahl Alpha's shards (world 3)
    /// take their own faces from their models, standing on the ground at
    /// their cells: no box stands in, and each is a crystal's size, not the
    /// half of its visibility extent the old sphere took.
    #[test]
    fn the_crystals_take_their_own_faces() {
        for selection in [2, 3] {
            crystals_take_their_own_faces(selection);
        }
    }

    fn crystals_take_their_own_faces(selection: u32) {
        let (residency, assets, world) = crate::test_worlds::offline_world_retained(
            selection,
            crate::terrain_runtime::TerrainSpacing::Units128,
        );
        let furniture = FurnitureDrawState::new(&inf_formats::native_basis::NativeBasisMath::new(
            assets.sine_table().values,
        ));
        let found = crystals(&world, &residency, &furniture);
        assert!(!found.is_empty(), "world {selection} has crystals");
        let mut shapes = std::collections::BTreeSet::new();
        for crystal in &found {
            let mesh = &crystal.mesh;
            assert!(mesh.triangles.len() > 12, "{} faces", mesh.triangles.len());
            assert!((10.0..1_000.0).contains(&mesh.radius), "{}", mesh.radius);
            let lowest = mesh
                .triangles
                .iter()
                .flatten()
                .map(|p| p[1])
                .fold(f32::MAX, f32::min);
            assert!(lowest.abs() < 60.0, "stands at its foot: {lowest}");
            shapes.insert((mesh.triangles.len(), mesh.radius as i32));
        }
        eprintln!(
            "world {selection}: {} crystals, shapes (faces, radius): {shapes:?}",
            found.len()
        );
        // A bolt through a crystal's middle strikes it; one passing a radius
        // and a half beside it does not.
        let crystal = &found[0];
        let centre = crystal.world(crystal.mesh.centre);
        let middle: [u16; 3] = std::array::from_fn(|axis| {
            crystal.origin[axis].wrapping_add(centre[axis].round() as i32 as u16)
        });
        let side = (crystal.mesh.radius * 1.5) as u16;
        let through = shot(
            [middle[0].wrapping_sub(side), middle[1], middle[2]],
            [middle[0].wrapping_add(side), middle[1], middle[2]],
            StrikeKind::Plasma,
        );
        let beside = Shot {
            start: [through.start[0], middle[1], middle[2].wrapping_add(side)],
            end: [through.end[0], middle[1], middle[2].wrapping_add(side)],
            ..through
        };
        let struck = crystal.struck_by(&through, [0; 3]).unwrap();
        assert!(crystal.struck_by(&beside, [0; 3]).is_none());
        // Its arcs' anchors lie on it, within its reach of its middle; the
        // first from where it was struck; the same strike, the same anchors.
        let near = |a: [u16; 3], b: [u16; 3]| {
            (0..3)
                .map(|axis| f32::from(a[axis].wrapping_sub(b[axis]) as i16).powi(2))
                .sum::<f32>()
                .sqrt()
        };
        assert_eq!(struck.anchors[0][0], struck.hit);
        for point in struck.anchors.iter().flatten() {
            assert!(near(*point, struck.middle) <= crystal.mesh.radius + 2.0);
        }
        assert!(struck.anchors.iter().all(|[a, b]| near(*a, *b) > 1.0));
        assert_eq!(crystal.anchors(struck.hit), struck.anchors);
    }

    /// A bolt's way back is its velocity over the frame's time.
    #[test]
    fn a_bolt_covers_the_way_it_moved() {
        let bolt = ParticleBolt {
            position: [1_000, 100, 2_000],
            velocity: [3_200, 0, -1_600],
            sprite: 2,
            scale: 1.0,
            fresh: false,
        };
        let way = bolt_shot(&bolt, 0.05);
        assert_eq!(way.end, [1_000, 100, 2_000]);
        assert_eq!(way.start, [847, 100, 2_076]);
        assert_eq!(way.kind, StrikeKind::Plasma);
    }

    /// A strike flares its crystal fully, fading over the given seconds; a
    /// bolt still inside the next frame is no fresh strike, the next bolt
    /// of a burst is.
    #[test]
    fn flares_fade_and_strike_once_a_pass() {
        let mut flares = CrystalFlares::default();
        let strike = Strike {
            cell: [7, 9],
            offset: [0.0; 3],
            middle: [7, 50, 9],
            anchors: [[[7, 50, 9]; 2]; ARC_PAIRS],
            hit: [7, 50, 9],
            radius: 100.0,
            kind: StrikeKind::Plasma,
        };
        assert_eq!(flares.update(&[strike], 0, 1.0).len(), 1);
        assert_eq!(flares.level([7, 9]), 1.0);
        assert!(flares.update(&[strike], 16_000, 1.0).is_empty());
        flares.update(&[], 516_000, 1.0);
        assert!((flares.level([7, 9]) - 0.5).abs() < 1.0e-3);
        flares.update(&[], 1_100_000, 1.0);
        assert_eq!(flares.level([7, 9]), 0.0);
        assert_eq!(flares.update(&[strike], 1_116_000, 1.0).len(), 1);
        // Inside for three frames, out for nine, then the next bolt.
        assert!(flares.update(&[strike], 1_132_000, 1.0).is_empty());
        assert!(flares.update(&[strike], 1_148_000, 1.0).is_empty());
        flares.update(&[], 1_292_000, 1.0);
        assert_eq!(flares.update(&[strike], 1_308_000, 1.0).len(), 1);
    }

    /// A laser held through a crystal strikes it once, keeps it flaring in
    /// the laser's colour, and sparks again as each spark runs out.
    #[test]
    fn a_held_laser_keeps_its_crystal_sparking() {
        let mut flares = CrystalFlares::default();
        let strike = Strike {
            cell: [7, 9],
            offset: [0.0; 3],
            middle: [7, 50, 9],
            anchors: [[[7, 50, 9]; 2]; ARC_PAIRS],
            hit: [7, 50, 9],
            radius: 100.0,
            kind: StrikeKind::Laser,
        };
        let look = look();
        assert_eq!(flares.update(&[strike], 0, 1.2).len(), 1);
        assert_eq!(flares.flare([7, 9]), Some((1.0, StrikeKind::Laser)));
        let first = flares.spark([7, 9], [0.0; 3], &look).unwrap();
        assert_eq!(first.light, [2.0, 1.6, 0.3]);
        let mut now = 0;
        let mut restarted = false;
        while now < 1_200_000 {
            now += 16_000;
            assert!(flares.update(&[strike], now, 1.2).is_empty());
            restarted |= flares.spark([7, 9], [0.0; 3], &look).unwrap().age == 0.0;
        }
        assert!(restarted, "the spark starts again while the beam holds");
    }

    fn look() -> crate::modern_tuning::CrystalFlareTuning {
        crate::modern_tuning::CrystalFlareTuning {
            color: [0.25, 0.5, 1.0],
            laser_color: [1.0, 0.8, 0.15],
            strength: 2.0,
            under: 1.0,
            reach: 900.0,
            seconds: 1.2,
        }
    }

    /// A spark flashes where the bolt struck and is gone from there in a
    /// tenth of a second, while a front runs out through the crystal and
    /// dims; it is over at its life's end.
    #[test]
    fn a_spark_runs_out_from_where_it_struck() {
        let (radius, life) = (100.0, 1.2);
        assert!(spark_glow(0.0, 0.0, radius, life) > 1.5);
        assert!(spark_glow(0.0, 0.2, radius, life) < 0.2);
        // A third of a second in, the front is near the crystal's edge.
        let front = |age: f32| {
            (0..=150)
                .map(|d| d as f32)
                .max_by(|a, b| {
                    spark_glow(*a, age, radius, life).total_cmp(&spark_glow(*b, age, radius, life))
                })
                .unwrap()
        };
        assert!(front(0.1) < front(0.2) && front(0.2) < front(0.3));
        assert!(
            spark_glow(front(0.6), 0.6, radius, life) < spark_glow(front(0.1), 0.1, radius, life)
        );
        assert_eq!(spark_glow(50.0, life, radius, life), 0.0);
    }

    /// A fresh strike starts a spark at the shot in the colour of what
    /// struck; it lives as long as it shows, and a draw takes its place from
    /// the eye.
    #[test]
    fn a_strike_leaves_a_spark_for_its_life() {
        let mut flares = CrystalFlares::default();
        let strike = Strike {
            cell: [7, 9],
            offset: [0.0; 3],
            middle: [7, 50, 9],
            anchors: [[[7, 50, 9]; 2]; ARC_PAIRS],
            hit: [1_000, 200, 3_000],
            radius: 120.0,
            kind: StrikeKind::Plasma,
        };
        flares.update(&[strike], 1_000_000, 1.2);
        let look = look();
        let spark = flares
            .spark([7, 9], [900.0, 100.0, 3_000.0], &look)
            .unwrap();
        assert_eq!(spark.hit, [100.0, 100.0, 0.0]);
        assert_eq!(spark.age, 0.0);
        assert_eq!(spark.light, [0.5, 1.0, 2.0]);
        flares.update(&[], 1_500_000, 1.2);
        assert!((flares.spark([7, 9], [0.0; 3], &look).unwrap().age - 0.75).abs() < 1.0e-6);
        flares.update(&[], 1_900_000, 1.2);
        assert!(flares.spark([7, 9], [0.0; 3], &look).is_none());
    }
}
