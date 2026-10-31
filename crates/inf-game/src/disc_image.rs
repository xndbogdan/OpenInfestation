//! The music off a disc image, for the launcher's Extract music and
//! `inf-game extract-music`: a CloneCD image (`.ccd` with its `.img`, the
//! usual copy of the retail disc) or a CUE sheet with its BIN files. After
//! OpenV2K's `setup::disc`; port setup, not a retail path.
//!
//! The retail disc is mixed mode: track 1 is data, tracks 2 to 9 are CD-DA.
//! Each music track is copied as lossless 44,100 Hz stereo 16-bit WAV to
//! `cdaudio/trackNN.wav`, keeping its physical number, which the CD player
//! streams as it does the OGGs ([`crate::platform_audio::cd_music`]).
//!
//! A track runs from its INDEX 1 to the next track's INDEX 1, the last to the
//! lead-out: the disc's table of contents, which MCI reports as a track's
//! length, so a pregap belongs to the track before it. Every track must have
//! the retail disc's length ([`TRACK_SECTORS`]), the length the CD player
//! checks; another disc's tracks are refused. The image is only read.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

use crate::launcher_setup::MUSIC_TRACKS;
use crate::platform_audio::cd_music::{music_track, wav_header};
use crate::platform_audio::TRACK_SECTORS;
use crate::retail_path;

/// One raw CD sector: 588 stereo 16-bit frames of audio.
const RAW_SECTOR: u64 = 2352;
/// A `.ccd` or `.cue` is a few KB; refuse anything that is plainly not one.
const MAX_SHEET_BYTES: u64 = 1 << 20;

/// A run of whole sectors in one of the image's files.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Segment {
    file: usize,
    start: u64,
    end: u64,
}

#[derive(Clone, Debug)]
struct ImageFile {
    path: PathBuf,
    /// Whole sectors in the file; `None` when it cannot be opened.
    sectors: Option<u64>,
}

/// A track of the image's table of contents.
#[derive(Clone, Debug)]
struct Track {
    number: u8,
    audio: bool,
    /// Its sectors, or why they cannot be read.
    segments: Result<Vec<Segment>, String>,
}

/// A disc image's table of contents and files, read from its `.ccd` or
/// `.cue`.
#[derive(Debug)]
pub struct DiscImage {
    files: Vec<ImageFile>,
    tracks: Vec<Track>,
}

/// What Extract music did, track by track.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MusicExtraction {
    pub directory: PathBuf,
    /// Tracks written as `trackNN.wav`.
    pub written: Vec<u32>,
    /// Tracks left alone because a valid `trackNN.ogg` already plays.
    pub kept: Vec<u32>,
    /// Tracks the image could not give, with the reason.
    pub unavailable: Vec<(u32, String)>,
}

impl MusicExtraction {
    /// The launcher's and the command line's report.
    pub fn summary(&self) -> String {
        let names = |tracks: &[u32]| {
            tracks
                .iter()
                .map(|track| format!("{track:02}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let mut lines = vec![format!("Music folder: {}", self.directory.display())];
        if !self.written.is_empty() {
            lines.push(format!("Extracted tracks {} as WAV.", names(&self.written)));
        }
        if !self.kept.is_empty() {
            lines.push(format!("Kept the OGG of tracks {}.", names(&self.kept)));
        }
        for (track, reason) in &self.unavailable {
            lines.push(format!("Track {track:02} unavailable: {reason}"));
        }
        lines.join("\n")
    }
}

impl DiscImage {
    /// Open `path`: a `.ccd` or `.cue`, or an `.img` or `.bin` beside its
    /// `.ccd` or `.cue` (the track boundaries are only there).
    pub fn open(path: &Path) -> Result<Self, String> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let sheet = match extension.as_str() {
            "ccd" | "cue" => path.to_path_buf(),
            "img" | "bin" => {
                let kind = if extension == "img" { "ccd" } else { "cue" };
                let sheet = companion(path, kind)?;
                if !sheet.is_file() {
                    return Err(format!(
                        "{} has no {} beside it, which holds the track boundaries; select that \
                         file instead",
                        path.display(),
                        sheet.display()
                    ));
                }
                sheet
            }
            _ => {
                return Err(
                    "Select the disc image's .ccd (CloneCD) or .cue (BIN/CUE) file".to_string(),
                )
            }
        };
        let text = read_sheet(&sheet)?;
        if sheet
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ccd"))
        {
            let image = companion(&sheet, "img")?;
            if extension == "img" && fs::canonicalize(path).ok() != fs::canonicalize(&image).ok() {
                return Err(format!(
                    "{} describes {}, not the selected image",
                    sheet.display(),
                    image.display()
                ));
            }
            parse_ccd(&text, image)
        } else {
            let image = parse_cue(&text, &sheet)?;
            if extension == "bin" {
                let selected = fs::canonicalize(path).map_err(|error| error.to_string())?;
                if !image.files.iter().any(|file| file.path == selected) {
                    return Err(format!(
                        "{} does not name the selected BIN; select the CUE",
                        sheet.display()
                    ));
                }
            }
            Ok(image)
        }
    }

    /// Track `number`'s sectors, if the image holds it as audio of the
    /// retail disc's length.
    fn music(&self, number: u32) -> Result<&[Segment], String> {
        let track = self
            .tracks
            .iter()
            .find(|track| u32::from(track.number) == number)
            .ok_or("the image has no such track")?;
        if !track.audio {
            return Err("it is a data track, not audio".to_string());
        }
        let segments = track.segments.as_ref().map_err(Clone::clone)?;
        for segment in segments {
            let file = &self.files[segment.file];
            match file.sectors {
                None => return Err(format!("{} cannot be read", file.path.display())),
                Some(sectors) if segment.end > sectors => {
                    return Err(format!("{} ends early (truncated)", file.path.display()))
                }
                Some(_) => {}
            }
        }
        let sectors: u64 = segments.iter().map(|s| s.end - s.start).sum();
        let retail = u64::from(TRACK_SECTORS[(number - MUSIC_TRACKS.start()) as usize]);
        if sectors != retail {
            return Err(format!(
                "it has {sectors} sectors where the Infestation disc's has {retail}; \
                 is this the Infestation CD?"
            ));
        }
        Ok(segments)
    }

    /// Copy the music tracks to `data_root`'s `cdaudio/` as WAV, telling
    /// `progress` each track before it is written. A track already playing
    /// from a valid OGG is kept; a WAV is replaced, through a `.part` file,
    /// so an interrupted copy leaves no damaged track. Only a write failure
    /// is an error; tracks the image lacks are reported in the result.
    pub fn extract_music(
        &self,
        data_root: &Path,
        mut progress: impl FnMut(u32),
    ) -> Result<MusicExtraction, String> {
        let directory = retail_path(data_root, "cdaudio");
        let mut result = MusicExtraction {
            directory: directory.clone(),
            ..MusicExtraction::default()
        };
        let mut wanted = Vec::new();
        for number in MUSIC_TRACKS {
            let playing = music_track(&directory, number);
            let ogg = playing.as_ref().is_ok_and(|path| {
                path.as_ref().is_some_and(|path| {
                    path.extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("ogg"))
                })
            });
            if ogg {
                result.kept.push(number);
                continue;
            }
            match self.music(number) {
                Ok(segments) => wanted.push((number, segments)),
                Err(reason) => result.unavailable.push((number, reason)),
            }
        }
        if !wanted.is_empty() {
            fs::create_dir_all(&directory)
                .map_err(|error| format!("Cannot create {}: {error}", directory.display()))?;
        }
        for (number, segments) in wanted {
            progress(number);
            let path = directory.join(format!("track{number:02}.wav"));
            let part = directory.join(format!("track{number:02}.wav.part"));
            let written = self
                .write_wav(segments, &part)
                .and_then(|()| fs::rename(&part, &path));
            if let Err(error) = written {
                let _ = fs::remove_file(&part);
                return Err(format!("Cannot write {}: {error}", path.display()));
            }
            result.written.push(number);
        }
        if result.written.is_empty() && result.kept.is_empty() {
            return Err(format!(
                "This image holds none of Infestation's music.\n{}",
                result.summary()
            ));
        }
        Ok(result)
    }

    fn write_wav(&self, segments: &[Segment], path: &Path) -> io::Result<()> {
        let sectors: u64 = segments.iter().map(|s| s.end - s.start).sum();
        // At most 20,251 sectors (track 4), well inside a WAV's 4 GB.
        let bytes = u32::try_from(sectors * RAW_SECTOR).map_err(io::Error::other)?;
        let mut out = BufWriter::new(File::create(path)?);
        out.write_all(&wav_header(bytes))?;
        for segment in segments {
            let mut file = File::open(&self.files[segment.file].path)?;
            file.seek(SeekFrom::Start(segment.start * RAW_SECTOR))?;
            let length = (segment.end - segment.start) * RAW_SECTOR;
            // Raw images keep CD-DA as little-endian 16-bit stereo: WAV's own.
            let copied = io::copy(&mut file.take(length), &mut out)?;
            if copied != length {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
        }
        out.into_inner()
            .map_err(|error| error.into_error())?
            .sync_all()
    }
}

/// `path`'s sibling with the same stem and `extension`, in any case.
fn companion(path: &Path, extension: &str) -> Result<PathBuf, String> {
    let parent = path.parent().unwrap_or(Path::new("."));
    let stem = path
        .file_stem()
        .ok_or_else(|| format!("{} has no file name", path.display()))?;
    let mut name = stem.to_os_string();
    name.push(".");
    name.push(extension);
    Ok(retail_path(parent, name))
}

fn read_sheet(path: &Path) -> Result<String, String> {
    let file =
        File::open(path).map_err(|error| format!("Cannot open {}: {error}", path.display()))?;
    let mut bytes = Vec::new();
    file.take(MAX_SHEET_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Cannot read {}: {error}", path.display()))?;
    if bytes.len() as u64 > MAX_SHEET_BYTES {
        return Err(format!("{} is too large for a track sheet", path.display()));
    }
    // Sheets are ASCII; a stray Latin-1 title must not refuse the disc.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn image_file(path: PathBuf) -> ImageFile {
    let sectors = fs::metadata(&path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len() / RAW_SECTOR);
    ImageFile { path, sectors }
}

/// A CloneCD sheet: INI sections, of which the `[Entry N]` rows are the
/// disc's raw table of contents. Point 1 to 99 starts a track at PLBA
/// (sector of the `.img`), Control bit 2 marks data, and point 0xA2 is the
/// session's lead-out. The `[TRACK N]` sections repeat the starts with the
/// pregaps (INDEX 0), which the table of contents does not count; the
/// `.sub` subchannel file is not needed.
fn parse_ccd(text: &str, image: PathBuf) -> Result<DiscImage, String> {
    let mut sections: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            sections.push((name.trim().to_ascii_lowercase(), BTreeMap::new()));
        } else if let (Some((key, value)), Some((_, keys))) =
            (line.split_once('='), sections.last_mut())
        {
            keys.insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        } else {
            return Err(format!("Malformed CloneCD sheet at line {}", index + 1));
        }
    }
    if !sections.iter().any(|(name, _)| name == "clonecd") {
        return Err("Not a CloneCD sheet: no [CloneCD] section".to_string());
    }
    // (session, number, audio, start)
    let mut starts: Vec<(i64, u8, bool, u64)> = Vec::new();
    let mut lead_outs = BTreeMap::new();
    for (name, keys) in &sections {
        if !name.starts_with("entry ") {
            continue;
        }
        let field = |key: &str| -> Result<i64, String> {
            let value = keys
                .get(key)
                .ok_or_else(|| format!("CloneCD [{name}] has no {key}"))?;
            match value
                .strip_prefix("0x")
                .or_else(|| value.strip_prefix("0X"))
            {
                Some(hex) => i64::from_str_radix(hex, 16),
                None => value.parse(),
            }
            .map_err(|_| format!("CloneCD [{name}] {key}={value} is not a number"))
        };
        let session = keys.get("session").map_or(Ok(1), |_| field("session"))?;
        let point = field("point")?;
        if point != 0xa2 && !(1..=99).contains(&point) {
            continue;
        }
        let sector = u64::try_from(field("plba")?)
            .map_err(|_| format!("CloneCD [{name}] starts before the disc"))?;
        if point == 0xa2 {
            lead_outs.insert(session, sector);
        } else {
            starts.push((session, point as u8, field("control")? & 4 == 0, sector));
        }
    }
    starts.sort_by_key(|&(_, number, _, _)| number);
    if starts.is_empty() {
        return Err("The CloneCD sheet lists no tracks".to_string());
    }
    let mut tracks = Vec::new();
    for (index, &(session, number, audio, start)) in starts.iter().enumerate() {
        if tracks.last().is_some_and(|t: &Track| t.number == number) {
            return Err(format!("The CloneCD sheet lists track {number} twice"));
        }
        let end = match starts.get(index + 1) {
            Some(&(next_session, _, _, next)) if next_session == session => Some(next),
            _ => lead_outs.get(&session).copied(),
        };
        let segments = match end {
            Some(end) if end > start => Ok(vec![Segment {
                file: 0,
                start,
                end,
            }]),
            Some(_) => Err("its table of contents entry is out of order".to_string()),
            None => Err("its session has no lead-out".to_string()),
        };
        tracks.push(Track {
            number,
            audio,
            segments,
        });
    }
    Ok(DiscImage {
        files: vec![image_file(image)],
        tracks,
    })
}

/// A CUE sheet of BINARY files (one for the disc, or one per track). Its
/// INDEX 01 times are sectors into the track's FILE; a track ends at the next
/// track's INDEX 01, in the same file or at the start of the next.
// shortcut: PREGAP/POSTGAP silence is not in the files and is not added, so
// a sheet that uses them gives tracks shorter than the disc's, which are
// refused; generate the silence if such a sheet turns up.
fn parse_cue(text: &str, sheet: &Path) -> Result<DiscImage, String> {
    let parent = sheet.parent().unwrap_or(Path::new("."));
    let canonical_parent = fs::canonicalize(parent).map_err(|error| error.to_string())?;
    let mut files: Vec<ImageFile> = Vec::new();
    // (number, audio, file, INDEX 01)
    let mut starts: Vec<(u8, bool, usize, Option<u64>)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        let words: Vec<&str> = line.split_whitespace().collect();
        let command = words
            .first()
            .map_or(String::new(), |w| w.to_ascii_uppercase());
        let malformed = || format!("Malformed CUE {command} on line {}", index + 1);
        match command.as_str() {
            "FILE" => {
                let rest = line[4..].trim();
                let (name, kind) = match rest.strip_prefix('"') {
                    Some(quoted) => {
                        let end = quoted.find('"').ok_or_else(malformed)?;
                        (&quoted[..end], quoted[end + 1..].trim())
                    }
                    None => rest
                        .rsplit_once(char::is_whitespace)
                        .ok_or_else(malformed)?,
                };
                if !kind.eq_ignore_ascii_case("BINARY") {
                    return Err(format!(
                        "CUE FILE type {kind} is not supported; only BINARY"
                    ));
                }
                let relative = Path::new(name);
                if name.contains(':')
                    || !relative
                        .components()
                        .all(|c| matches!(c, Component::Normal(_)))
                {
                    return Err("A CUE FILE must name a file beside the sheet".to_string());
                }
                let path = retail_path(parent, relative);
                let path = fs::canonicalize(&path).unwrap_or(canonical_parent.join(relative));
                if !path.starts_with(&canonical_parent) {
                    return Err("A CUE FILE must name a file beside the sheet".to_string());
                }
                if files.iter().any(|file| file.path == path) {
                    return Err("The CUE sheet names one FILE twice".to_string());
                }
                files.push(image_file(path));
            }
            "TRACK" => {
                let [_, number, mode] = words[..] else {
                    return Err(malformed());
                };
                let number: u8 = number.parse().map_err(|_| malformed())?;
                if number == 0 || number > 99 || starts.last().is_some_and(|t| t.0 >= number) {
                    return Err("CUE tracks must be numbered upward from 01 to 99".to_string());
                }
                let mode = mode.to_ascii_uppercase();
                let audio = match mode.as_str() {
                    "AUDIO" => true,
                    "MODE1/2352" | "MODE2/2352" => false,
                    _ => return Err(format!("CUE track mode {mode} is not supported")),
                };
                let file = files
                    .len()
                    .checked_sub(1)
                    .ok_or("A CUE TRACK comes before any FILE")?;
                starts.push((number, audio, file, None));
            }
            "INDEX" => {
                let [_, point, time] = words[..] else {
                    return Err(malformed());
                };
                let track = starts
                    .last_mut()
                    .ok_or("A CUE INDEX comes before any TRACK")?;
                let sector = cue_time(time).ok_or_else(malformed)?;
                if point == "01" && track.3.replace(sector).is_some() {
                    return Err(format!("CUE track {} has two INDEX 01", track.0));
                }
            }
            "" | "REM" | "TITLE" | "PERFORMER" | "SONGWRITER" | "CATALOG" | "ISRC" | "FLAGS"
            | "CDTEXTFILE" | "PREGAP" | "POSTGAP" => {}
            _ => return Err(format!("Unsupported CUE command {command}")),
        }
    }
    if starts.is_empty() {
        return Err("The CUE sheet lists no tracks".to_string());
    }
    let mut tracks = Vec::new();
    for (index, &(number, audio, file, start)) in starts.iter().enumerate() {
        let segments = (|| {
            let start = start.ok_or("it has no INDEX 01")?;
            let file_end = || files[file].sectors.ok_or("its FILE cannot be read");
            match starts.get(index + 1) {
                Some(&(_, _, next_file, Some(next))) if next_file == file => {
                    if next <= start {
                        return Err("its INDEX 01 is out of order");
                    }
                    Ok(vec![Segment {
                        file,
                        start,
                        end: next,
                    }])
                }
                Some(&(_, _, next_file, Some(next))) if next_file == file + 1 => {
                    let mut segments = vec![Segment {
                        file,
                        start,
                        end: file_end()?,
                    }];
                    if next > 0 {
                        segments.push(Segment {
                            file: next_file,
                            start: 0,
                            end: next,
                        });
                    }
                    Ok(segments)
                }
                Some(&(_, _, _, Some(_))) => Err("the next track skips a FILE"),
                Some(_) => Err("the next track has no INDEX 01"),
                None => Ok(vec![Segment {
                    file,
                    start,
                    end: file_end()?,
                }]),
            }
        })()
        .and_then(|segments| {
            if segments.iter().any(|s| s.end <= s.start) {
                Err("its FILE ends before it starts")
            } else {
                Ok(segments)
            }
        })
        .map_err(str::to_string);
        tracks.push(Track {
            number,
            audio,
            segments,
        });
    }
    Ok(DiscImage { files, tracks })
}

/// `MM:SS:FF` as sectors, 75 to the second.
fn cue_time(value: &str) -> Option<u64> {
    let mut parts = value.split(':').map(|part| part.parse::<u64>().ok());
    let (Some(Some(minutes)), Some(Some(seconds)), Some(Some(frames)), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return None;
    };
    (seconds < 60 && frames < 75).then(|| (minutes * 60 + seconds) * 75 + frames)
}

#[cfg(test)]
#[path = "disc_image_tests.rs"]
mod tests;
