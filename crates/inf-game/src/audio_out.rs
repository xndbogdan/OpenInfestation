//! `--audio-out`: the mix rendered from the frames into a WAV file (16-bit
//! stereo at 44.1 kHz), each frame its fixed step's share of samples, so a
//! fixed-step run's sound repeats sample for sample. For checking the
//! port's ambience without listening.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

use crate::platform_audio::PlatformAudio;

const RATE: u64 = 44_100;

pub struct AudioOut {
    file: BufWriter<File>,
    frames: u64,
    /// The microsecond-samples a frame's share left over.
    carry: u64,
}

impl AudioOut {
    pub fn create(path: &Path) -> io::Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        write_header(&mut file, 0)?;
        Ok(Self {
            file,
            frames: 0,
            carry: 0,
        })
    }

    /// Render `micros` of `audio`'s mix and append it.
    pub fn frame(&mut self, micros: u32, audio: &mut PlatformAudio) -> io::Result<()> {
        let total = self.carry + u64::from(micros) * RATE;
        let frames = total / 1_000_000;
        self.carry = total % 1_000_000;
        for sample in audio.render(frames as usize) {
            let value = (sample.clamp(-1.0, 1.0) * 32767.0).round() as i16;
            self.file.write_all(&value.to_le_bytes())?;
        }
        self.frames += frames;
        Ok(())
    }
}

impl Drop for AudioOut {
    /// The header learns the length at the end.
    fn drop(&mut self) {
        let frames = self.frames;
        let _ = self
            .file
            .seek(SeekFrom::Start(0))
            .and_then(|_| write_header(&mut self.file, frames))
            .and_then(|_| self.file.flush());
    }
}

fn write_header(out: &mut impl Write, frames: u64) -> io::Result<()> {
    let data = (frames * 4).min(u64::from(u32::MAX - 36)) as u32;
    out.write_all(b"RIFF")?;
    out.write_all(&(36 + data).to_le_bytes())?;
    out.write_all(b"WAVEfmt ")?;
    out.write_all(&16_u32.to_le_bytes())?;
    out.write_all(&1_u16.to_le_bytes())?; // PCM
    out.write_all(&2_u16.to_le_bytes())?;
    out.write_all(&(RATE as u32).to_le_bytes())?;
    out.write_all(&(RATE as u32 * 4).to_le_bytes())?;
    out.write_all(&4_u16.to_le_bytes())?;
    out.write_all(&16_u16.to_le_bytes())?;
    out.write_all(b"data")?;
    out.write_all(&data.to_le_bytes())
}
