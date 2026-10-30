//! The banner's prerendered flipbook, after the V2000 port's launcher: new
//! INFESTATION lettering over a line, against the frontend's starfield and a
//! still nebula, with a shine that runs along the line once a loop, timed by
//! the menu logo's recovered 64-tick ping. The baked frames are in
//! `data/branding/`; their sources are kept outside this repository.
//!
//! Runtime only selects baked frames. The atlas stays indexed in memory, with
//! one opaque BGRA frame expanded for GDI. Repaints neither allocate nor
//! advance the animation.

use std::io::Cursor;

use serde::Deserialize;

#[derive(Deserialize)]
struct AnimationManifest {
    frame_width: u32,
    frame_height: u32,
    frame_count: u32,
    columns: u32,
    frame_micros: u64,
}

pub(super) struct HeaderFrame {
    pub width: i32,
    pub height: i32,
    /// Top-down, opaque BGRA rows for GDI's SRCCOPY submission.
    pub bgra: Vec<u8>,
}

pub(super) struct HeaderAnimation {
    manifest: AnimationManifest,
    indices: Vec<u8>,
    palette: [[u8; 4]; 256],
    atlas_width: usize,
    loop_micros: u64,
    phase_micros: u64,
    selected_frame: usize,
    cached: HeaderFrame,
}

impl HeaderAnimation {
    pub fn load() -> Result<Self, String> {
        Self::decode(
            include_bytes!("../../data/branding/header-animation.json"),
            include_bytes!("../../data/branding/infestation-header-animation.png"),
        )
    }

    fn decode(manifest_bytes: &[u8], png_bytes: &[u8]) -> Result<Self, String> {
        let manifest: AnimationManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|error| format!("Cannot read the launcher animation's manifest: {error}"))?;
        let (atlas_width, atlas_height, loop_micros) = manifest.layout()?;
        let mut decoder = png::Decoder::new(Cursor::new(png_bytes));
        // Keep palette indices: expanding all 192 frames to RGBA would
        // quadruple the animation's resident memory.
        decoder.set_transformations(png::Transformations::IDENTITY);
        let mut reader = decoder
            .read_info()
            .map_err(|error| format!("Cannot read the launcher animation's atlas: {error}"))?;
        let info = reader.info();
        if info.width != atlas_width || info.height != atlas_height {
            return Err("The launcher animation's atlas does not match its manifest".into());
        }
        if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
            return Err("The launcher animation's atlas must be 8-bit indexed".into());
        }
        if info.trns.is_some() {
            return Err("The launcher animation's atlas must be opaque".into());
        }
        let colors = info
            .palette
            .as_deref()
            .ok_or_else(|| "The launcher animation's atlas has no palette".to_string())?;
        if colors.is_empty() || colors.len() > 768 || colors.len() % 3 != 0 {
            return Err("The launcher animation's atlas has an invalid palette".into());
        }
        let color_count = colors.len() / 3;
        let mut palette = [[0, 0, 0, 255]; 256];
        for (entry, color) in palette.iter_mut().zip(colors.chunks_exact(3)) {
            *entry = [color[2], color[1], color[0], 255];
        }
        let mut indices = vec![0; reader.output_buffer_size()];
        let output = reader
            .next_frame(&mut indices)
            .map_err(|error| format!("Cannot decode the launcher animation's atlas: {error}"))?;
        indices.truncate(output.buffer_size());
        let expected_pixels = usize::try_from(u64::from(atlas_width) * u64::from(atlas_height))
            .map_err(|_| "The launcher animation's atlas is too large".to_string())?;
        if indices.len() != expected_pixels
            || indices
                .iter()
                .any(|index| usize::from(*index) >= color_count)
        {
            return Err("The launcher animation's atlas has invalid palette indices".into());
        }
        let frame_bytes =
            usize::try_from(u64::from(manifest.frame_width) * u64::from(manifest.frame_height) * 4)
                .map_err(|_| "The launcher animation's frame is too large".to_string())?;
        let cached = HeaderFrame {
            width: manifest.frame_width as i32,
            height: manifest.frame_height as i32,
            bgra: vec![0; frame_bytes],
        };
        let mut animation = Self {
            manifest,
            indices,
            palette,
            atlas_width: atlas_width as usize,
            loop_micros,
            phase_micros: 0,
            selected_frame: 0,
            cached,
        };
        animation.expand_selected_frame();
        Ok(animation)
    }

    pub fn frame(&self) -> &HeaderFrame {
        &self.cached
    }

    /// Select straight from elapsed time, a late timer included. Returns
    /// whether a different baked frame needs the banner repainted.
    pub fn advance(&mut self, elapsed_micros: u64) -> bool {
        self.phase_micros = ((u128::from(self.phase_micros) + u128::from(elapsed_micros))
            % u128::from(self.loop_micros)) as u64;
        let selected = (self.phase_micros / self.manifest.frame_micros) as usize;
        if selected == self.selected_frame {
            return false;
        }
        self.selected_frame = selected;
        self.expand_selected_frame();
        true
    }

    fn expand_selected_frame(&mut self) {
        let width = self.manifest.frame_width as usize;
        let height = self.manifest.frame_height as usize;
        let columns = self.manifest.columns as usize;
        let left = self.selected_frame % columns * width;
        let top = self.selected_frame / columns * height;
        for y in 0..height {
            let source = (top + y) * self.atlas_width + left;
            let destination = y * width * 4;
            let row = &mut self.cached.bgra[destination..destination + width * 4];
            for (pixel, index) in row
                .chunks_exact_mut(4)
                .zip(&self.indices[source..source + width])
            {
                pixel.copy_from_slice(&self.palette[usize::from(*index)]);
            }
        }
    }
}

impl AnimationManifest {
    fn layout(&self) -> Result<(u32, u32, u64), String> {
        if self.frame_width == 0
            || self.frame_height == 0
            || self.frame_width > i32::MAX as u32
            || self.frame_height > i32::MAX as u32
            || self.frame_count == 0
            || self.columns == 0
            || self.columns > self.frame_count
            || self.frame_micros == 0
        {
            return Err(
                "The launcher animation needs positive frame sizes, a count and a duration".into(),
            );
        }
        let rows =
            self.frame_count / self.columns + u32::from(self.frame_count % self.columns != 0);
        let width = self.frame_width.checked_mul(self.columns);
        let height = self.frame_height.checked_mul(rows);
        let duration = self.frame_micros.checked_mul(u64::from(self.frame_count));
        match (width, height, duration) {
            (Some(width), Some(height), Some(duration)) => Ok((width, height, duration)),
            _ => Err("The launcher animation's sizes or duration overflow".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &[u8] = br#"{
        "frame_width":2,"frame_height":1,"frame_count":4,
        "columns":2,"frame_micros":20000
    }"#;

    fn atlas(transparent: bool, indices: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 4, 2);
            encoder.set_color(png::ColorType::Indexed);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_palette(vec![10, 20, 30, 80, 90, 100]);
            if transparent {
                encoder.set_trns(vec![0, 255]);
            }
            encoder
                .write_header()
                .unwrap()
                .write_image_data(indices)
                .unwrap();
        }
        bytes
    }

    fn fixture() -> HeaderAnimation {
        HeaderAnimation::decode(MANIFEST, &atlas(false, &[0, 1, 1, 0, 1, 1, 0, 0])).unwrap()
    }

    #[test]
    fn indexed_atlas_expands_selected_tiles_to_top_down_opaque_bgra() {
        let mut animation = fixture();
        assert_eq!((animation.frame().width, animation.frame().height), (2, 1));
        assert_eq!(animation.frame().bgra, [30, 20, 10, 255, 100, 90, 80, 255]);
        assert!(animation.advance(20_000));
        assert_eq!(animation.frame().bgra, [100, 90, 80, 255, 30, 20, 10, 255]);
        assert!(animation.advance(20_000));
        assert_eq!(animation.frame().bgra, [100, 90, 80, 255, 100, 90, 80, 255]);
        assert!(animation.advance(20_000));
        assert_eq!(animation.frame().bgra, [30, 20, 10, 255, 30, 20, 10, 255]);
    }

    #[test]
    fn exact_frame_boundaries_and_wrap_select_only_changed_frames() {
        let mut animation = fixture();
        assert!(!animation.advance(0));
        assert!(!animation.advance(19_999));
        assert_eq!(animation.selected_frame, 0);
        assert!(animation.advance(1));
        assert_eq!(animation.selected_frame, 1);
        assert!(animation.advance(59_999));
        assert_eq!(animation.selected_frame, 3);
        assert!(animation.advance(1));
        assert_eq!((animation.selected_frame, animation.phase_micros), (0, 0));
        assert!(!animation.advance(80_000));
    }

    #[test]
    fn a_late_timer_lands_on_the_same_frame_as_steady_ticks() {
        let mut late = fixture();
        let mut steady = fixture();
        assert!(late.advance(1_020_000));
        for _ in 0..51 {
            steady.advance(20_000);
        }
        assert_eq!(late.phase_micros, 60_000);
        assert_eq!(late.frame().bgra, steady.frame().bgra);
        late.advance(u64::MAX);
        assert_eq!(
            late.phase_micros,
            ((60_000_u128 + u128::from(u64::MAX)) % 80_000) as u64
        );
    }

    #[test]
    fn paints_read_the_same_cached_frame_without_moving_the_clock() {
        let animation = fixture();
        let address = animation.frame().bgra.as_ptr();
        for _ in 0..10 {
            assert_eq!(animation.frame().bgra.as_ptr(), address);
            assert_eq!(animation.phase_micros, 0);
            assert_eq!(animation.selected_frame, 0);
        }
    }

    #[test]
    fn a_malformed_manifest_atlas_or_palette_fails_before_the_window_opens() {
        let bytes = atlas(false, &[0, 1, 1, 0, 1, 1, 0, 0]);
        assert!(HeaderAnimation::decode(br#"{}"#, &bytes).is_err());
        for (field, value) in [
            ("frame_width", 0),
            ("frame_height", 0),
            ("frame_count", 0),
            ("columns", 0),
            ("frame_micros", 0),
            ("columns", 3),
            ("frame_width", 3),
        ] {
            let mut manifest: serde_json::Value = serde_json::from_slice(MANIFEST).unwrap();
            manifest[field] = value.into();
            assert!(
                HeaderAnimation::decode(&serde_json::to_vec(&manifest).unwrap(), &bytes).is_err(),
                "invalid {field}={value}"
            );
        }
        assert!(
            HeaderAnimation::decode(MANIFEST, &atlas(true, &[0, 1, 1, 0, 1, 1, 0, 0])).is_err()
        );
        assert!(
            HeaderAnimation::decode(MANIFEST, &atlas(false, &[2, 1, 1, 0, 1, 1, 0, 0])).is_err()
        );
    }

    /// The menu logo's halo pings every 64 ticks of the frontend's 20 ms
    /// clock (child model 746's sixteen face groups of four ticks). The
    /// banner bakes one frame a tick: in the first 64 the shine crosses the
    /// line under the word (header row 70) from left to right, brightest
    /// halfway, as the ping is; for the next 128 the line rests and the
    /// banner holds one still picture.
    #[test]
    fn the_embedded_flipbook_runs_one_shine_across_the_line_in_one_ping() {
        let mut animation = HeaderAnimation::load().unwrap();
        assert_eq!(
            (animation.frame().width, animation.frame().height),
            (624, 98)
        );
        assert_eq!(animation.manifest.frame_micros, 20_000);
        assert_eq!(animation.manifest.frame_count, 3 * 64);
        assert_eq!(animation.loop_micros, 3 * 1_280_000);
        assert_eq!(animation.indices.len(), 624 * 98 * 192);
        assert_eq!(animation.frame().bgra.len(), 624 * 98 * 4);
        let first = animation.frame().bgra.clone();
        let mut frames = vec![first.clone()];
        for tick in 1..=192 {
            assert!(animation.advance(20_000));
            assert!(animation
                .frame()
                .bgra
                .chunks_exact(4)
                .all(|pixel| pixel[3] == 255));
            if tick < 192 {
                frames.push(animation.frame().bgra.clone());
            }
        }
        assert_eq!(animation.phase_micros, 0);
        assert_eq!(animation.frame().bgra, first);
        // Light well above a resting frame's in the line's band: its total
        // and its centre column.
        let rest = &frames[128];
        let shine = |frame: &[u8]| {
            let (mut weight, mut moment) = (0u64, 0u64);
            for at in (60 * 624..80 * 624).map(|pixel| pixel * 4) {
                let light =
                    |bgra: &[u8]| -> u64 { bgra[at..at + 3].iter().map(|&c| u64::from(c)).sum() };
                let gain = light(frame).saturating_sub(light(rest));
                if gain > 200 {
                    weight += gain;
                    moment += gain * (at / 4 % 624) as u64;
                }
            }
            (weight, moment as f64 / weight.max(1) as f64)
        };
        for tick in (8..=48).step_by(8) {
            let (weight, centre) = shine(&frames[tick]);
            let expected = 624.0 * tick as f64 / 64.0;
            assert!(weight > 20_000, "tick {tick}: {weight}");
            assert!((centre - expected).abs() < 16.0, "tick {tick}: {centre}");
        }
        let brightest = (0..64).max_by_key(|&tick| shine(&frames[tick]).0).unwrap();
        assert!((28..=36).contains(&brightest), "{brightest}");
        // Before and after the shine, nothing moves: no pixel of the
        // lettering, stars or nebula changes from frame to frame.
        for (tick, frame) in frames.iter().enumerate().skip(60) {
            assert!(frame == &frames[0], "tick {tick}");
        }
    }
}
