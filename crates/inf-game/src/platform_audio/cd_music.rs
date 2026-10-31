//! The CD player's tracks on disk, after OpenV2K's `v2k-render::music`:
//! `cdaudio/trackNN.ogg`, or `trackNN.wav`, the lossless CD-DA the
//! launcher's Extract music copies off a disc image ([`crate::disc_image`]).
//! Each file keeps its physical track number, so a missing track stays
//! missing rather than another taking its place. A valid OGG wins over a
//! WAV; either must be 44,100 Hz stereo, the disc's own format.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::OUTPUT_RATE;

/// Bytes of one stereo 16-bit frame.
const FRAME_BYTES: u64 = 4;

/// `track`'s file in `directory`: a valid OGG, else a valid WAV; `None`
/// while neither is there. Present files that are no CD audio (a bad
/// header, another format) are an error naming each one, so a damaged rip
/// is not reported as merely missing. Headers only: the body's length is
/// the decoder's check.
pub fn music_track(directory: &Path, track: u32) -> Result<Option<PathBuf>, String> {
    let mut errors = Vec::new();
    for extension in ["ogg", "wav"] {
        let name = format!("track{track:02}.{extension}");
        let path = crate::retail_path(directory, &name);
        if !path.is_file() {
            continue;
        }
        let checked = if extension == "ogg" {
            check_ogg(&path)
        } else {
            open_wav(&path).map(drop)
        };
        match checked {
            Ok(()) => return Ok(Some(path)),
            Err(error) => errors.push(format!("{name}: {error}")),
        }
    }
    if errors.is_empty() {
        Ok(None)
    } else {
        Err(errors.join("; "))
    }
}

fn check_ogg(path: &Path) -> Result<(), String> {
    let decoder = super::open_decoder(path).map_err(|error| error.to_string())?;
    let header = &decoder.ident_hdr;
    if header.audio_channels != 2 || header.audio_sample_rate != OUTPUT_RATE {
        return Err(format!(
            "{} Hz with {} channels, not 44,100 Hz stereo",
            header.audio_sample_rate, header.audio_channels
        ));
    }
    Ok(())
}

/// A RIFF WAVE file of 44,100 Hz stereo 16-bit PCM: the file positioned at
/// its samples, and their length in bytes. Chunk lengths are checked against
/// the file before anything is read; other chunks are skipped by seeking.
pub fn open_wav(path: &Path) -> Result<(File, u64), String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let length = file.metadata().map_err(|error| error.to_string())?.len();
    let io = |error: std::io::Error| format!("cannot read the WAVE file: {error}");
    let mut header = [0; 12];
    file.read_exact(&mut header)
        .map_err(|_| "not a RIFF WAVE file".to_string())?;
    if &header[..4] != b"RIFF" || &header[8..] != b"WAVE" {
        return Err("not a RIFF WAVE file".to_string());
    }
    let riff_end = 8 + u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap()));
    if riff_end < 12 || riff_end > length {
        return Err("truncated RIFF WAVE file".to_string());
    }
    let (mut format, mut samples) = (false, None);
    let mut position = 12;
    while position < riff_end {
        if riff_end - position < 8 {
            return Err("truncated WAVE chunk header".to_string());
        }
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk).map_err(io)?;
        let size = u64::from(u32::from_le_bytes(chunk[4..].try_into().unwrap()));
        let body = position + 8;
        let next = body + size + (size & 1);
        if next > riff_end {
            return Err("truncated WAVE chunk".to_string());
        }
        match &chunk[..4] {
            b"fmt " => {
                if format || size < 16 {
                    return Err("short or repeated WAVE format chunk".to_string());
                }
                let mut fields = [0; 16];
                file.read_exact(&mut fields).map_err(io)?;
                let word = |at: usize| u16::from_le_bytes([fields[at], fields[at + 1]]);
                let long = |at: usize| u32::from_le_bytes(fields[at..at + 4].try_into().unwrap());
                let (encoding, channels, rate) = (word(0), word(2), long(4));
                let (byte_rate, block, bits) = (long(8), word(12), word(14));
                if encoding != 1
                    || channels != 2
                    || rate != OUTPUT_RATE
                    || bits != 16
                    || byte_rate != OUTPUT_RATE * FRAME_BYTES as u32
                    || u64::from(block) != FRAME_BYTES
                {
                    return Err(format!(
                        "encoding {encoding}, {rate} Hz, {channels} channels, {bits} bits; \
                         CD audio is PCM at 44,100 Hz, stereo, 16 bits"
                    ));
                }
                format = true;
            }
            b"data" => {
                if samples.is_some() || size == 0 || size % FRAME_BYTES != 0 {
                    return Err("empty, repeated or partial WAVE data".to_string());
                }
                samples = Some((body, size));
            }
            _ => {}
        }
        file.seek(SeekFrom::Start(next)).map_err(io)?;
        position = next;
    }
    if !format {
        return Err("no WAVE format chunk".to_string());
    }
    let (start, size) = samples.ok_or("no WAVE data chunk")?;
    file.seek(SeekFrom::Start(start)).map_err(io)?;
    Ok((file, size))
}

/// The canonical 44-byte header of `data_bytes` of CD audio, as Extract
/// music writes it.
pub fn wav_header(data_bytes: u32) -> Vec<u8> {
    [
        b"RIFF".as_slice(),
        &(36 + data_bytes).to_le_bytes(),
        b"WAVEfmt ",
        &16u32.to_le_bytes(),
        &1u16.to_le_bytes(),
        &2u16.to_le_bytes(),
        &OUTPUT_RATE.to_le_bytes(),
        &(OUTPUT_RATE * FRAME_BYTES as u32).to_le_bytes(),
        &(FRAME_BYTES as u16).to_le_bytes(),
        &16u16.to_le_bytes(),
        b"data",
        &data_bytes.to_le_bytes(),
    ]
    .concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inf-cd-music-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wav(samples: &[i16]) -> Vec<u8> {
        let mut bytes = wav_header(samples.len() as u32 * 2);
        bytes.extend(samples.iter().flat_map(|sample| sample.to_le_bytes()));
        bytes
    }

    #[test]
    fn a_wav_opens_at_its_samples_past_other_chunks() {
        let dir = scratch("chunks");
        let mut bytes = wav(&[1, -2, 3, -4]);
        // A LIST chunk of odd length (padded) before the data.
        let list = [b"LIST".as_slice(), &3u32.to_le_bytes(), b"abc\0"].concat();
        bytes.splice(36..36, list.iter().copied());
        let riff = bytes.len() as u32 - 8;
        bytes[4..8].copy_from_slice(&riff.to_le_bytes());
        let path = dir.join("track02.wav");
        std::fs::write(&path, &bytes).unwrap();
        let (mut file, size) = open_wav(&path).unwrap();
        assert_eq!(size, 8);
        let mut samples = [0; 8];
        file.read_exact(&mut samples).unwrap();
        assert_eq!(samples, [1, 0, 0xfe, 0xff, 3, 0, 0xfc, 0xff]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn malformed_wavs_are_refused() {
        let dir = scratch("malformed");
        let good = wav(&[1, 2]);
        let mut mono = good.clone();
        mono[22] = 1;
        let mut rate = good.clone();
        rate[24..28].copy_from_slice(&22_050u32.to_le_bytes());
        let mut partial = good.clone();
        partial[40..44].copy_from_slice(&2u32.to_le_bytes());
        let cases: [(&str, &[u8]); 6] = [
            ("empty", &[]),
            ("not riff", b"RIFX\0\0\0\0WAVEfmt "),
            ("truncated", &good[..good.len() - 2]),
            ("mono", &mono),
            ("rate", &rate),
            ("partial frame", &partial),
        ];
        for (name, bytes) in cases {
            let path = dir.join("track02.wav");
            std::fs::write(&path, bytes).unwrap();
            assert!(open_wav(&path).is_err(), "{name}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_valid_ogg_wins_then_a_valid_wav_and_bad_files_are_named() {
        let dir = scratch("inventory");
        assert_eq!(music_track(&dir, 2), Ok(None));
        std::fs::write(dir.join("track02.wav"), wav(&[5, 6])).unwrap();
        assert_eq!(music_track(&dir, 2), Ok(Some(dir.join("track02.wav"))));
        // A broken OGG falls back to the WAV beside it.
        std::fs::write(dir.join("track02.ogg"), b"not ogg").unwrap();
        assert_eq!(music_track(&dir, 2), Ok(Some(dir.join("track02.wav"))));
        // Alone, it is an error, not a missing track.
        std::fs::write(dir.join("track03.ogg"), b"not ogg").unwrap();
        let error = music_track(&dir, 3).unwrap_err();
        assert!(error.starts_with("track03.ogg: "), "{error}");
        // A real OGG wins over the WAV.
        let retail = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail/cdaudio");
        std::fs::copy(retail.join("track02.ogg"), dir.join("track02.ogg")).unwrap();
        assert_eq!(music_track(&dir, 2), Ok(Some(dir.join("track02.ogg"))));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
