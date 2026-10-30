use std::path::{absolute, Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    MaximumSamples,
    StopFile,
}

impl StopReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MaximumSamples => "maximum_samples",
            Self::StopFile => "stop_file",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub reason: StopReason,
    pub samples_completed: u64,
    pub samples_requested: u64,
    pub elapsed_ms: f64,
}

pub fn ensure_stop_file_absent(stop_file: Option<&Path>) -> Result<(), String> {
    if stop_requested(stop_file)? {
        return Err(format!(
            "--stop-file {} already exists; remove the stale marker before capture",
            stop_file.expect("present").display()
        ));
    }
    Ok(())
}

pub fn ensure_distinct_capture_paths(
    stop_file: Option<&Path>,
    output: &Path,
) -> Result<(), String> {
    let Some(stop_file) = stop_file else {
        return Ok(());
    };
    let stop_file = comparison_path(stop_file)?;
    let output = comparison_path(output)?;
    if stop_file
        .to_string_lossy()
        .eq_ignore_ascii_case(&output.to_string_lossy())
    {
        return Err("--stop-file must not resolve to the output path".into());
    }
    Ok(())
}

pub fn run(
    seconds: f64,
    hz: u32,
    stop_file: Option<&Path>,
    mut sample_fn: impl FnMut(u64, f64) -> Result<(), String>,
) -> Result<Outcome, String> {
    let samples_requested = (seconds * f64::from(hz)).ceil().max(1.0) as u64;
    let interval = Duration::from_secs_f64(1.0 / f64::from(hz));
    let start = Instant::now();
    for sample in 0..samples_requested {
        let target = start + interval.mul_f64(sample as f64);
        let now = Instant::now();
        if target > now {
            sleep(target - now);
        }
        if stop_requested(stop_file)? {
            return Ok(Outcome {
                reason: StopReason::StopFile,
                samples_completed: sample,
                samples_requested,
                elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            });
        }
        sample_fn(sample, start.elapsed().as_secs_f64() * 1000.0)?;
    }
    let reason = if stop_requested(stop_file)? {
        StopReason::StopFile
    } else {
        StopReason::MaximumSamples
    };
    Ok(Outcome {
        reason,
        samples_completed: samples_requested,
        samples_requested,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
    })
}

fn stop_requested(stop_file: Option<&Path>) -> Result<bool, String> {
    stop_file.map_or(Ok(false), |path| {
        path.try_exists()
            .map_err(|error| format!("could not inspect stop file {}: {error}", path.display()))
    })
}

fn comparison_path(path: &Path) -> Result<PathBuf, String> {
    let absolute = absolute(path)
        .map_err(|error| format!("could not resolve path {}: {error}", path.display()))?;
    if let Ok(canonical) = absolute.canonicalize() {
        return Ok(canonical);
    }
    let Some(file_name) = absolute.file_name() else {
        return Ok(absolute);
    };
    let Some(parent) = absolute.parent() else {
        return Ok(absolute);
    };
    Ok(parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf())
        .join(file_name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{remove_file, File};

    fn marker(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("inf-inspect-{}-{name}.marker", std::process::id()))
    }

    #[test]
    fn stale_stop_marker_is_rejected() {
        let path = marker("stale");
        let _ = remove_file(&path);
        File::create(&path).expect("create marker");
        assert!(ensure_stop_file_absent(Some(&path)).is_err());
        remove_file(path).expect("remove marker");
    }

    #[test]
    fn output_cannot_alias_stop_marker() {
        let path = marker("alias");
        assert!(ensure_distinct_capture_paths(Some(&path), &path).is_err());
    }

    #[test]
    fn stop_marker_prevents_first_sample() {
        let path = marker("early");
        let _ = remove_file(&path);
        File::create(&path).expect("create marker");
        let mut calls = 0;
        let outcome = run(1.0, 10, Some(&path), |_, _| {
            calls += 1;
            Ok(())
        })
        .expect("timeline");
        assert_eq!(calls, 0);
        assert_eq!(outcome.reason, StopReason::StopFile);
        remove_file(path).expect("remove marker");
    }
}
