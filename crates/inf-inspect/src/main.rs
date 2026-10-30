#[cfg(not(windows))]
fn main() {
    eprintln!("inf-original-inspect is only supported on Windows");
    std::process::exit(1);
}

#[cfg(windows)]
mod entity_list;
#[cfg(windows)]
mod model_table;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod timeline;

#[cfg(windows)]
fn main() {
    if let Err(error) = app::run() {
        eprintln!("inf-original-inspect: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
mod app {
    use std::fs::{create_dir_all, File};
    use std::io::{BufWriter, Write};
    use std::path::{Path, PathBuf};

    use clap::{Parser, Subcommand, ValueEnum};
    use serde::Serialize;

    use crate::model_table::{ModelTableSnapshot, SlotState};
    use crate::process::{BuildFingerprint, Process};
    use crate::{entity_list, model_table, timeline};

    #[derive(Debug, Parser)]
    #[command(
        name = "inf-original-inspect",
        about = "Read-only runtime inspector for the supported retail Infest.exe"
    )]
    struct Cli {
        /// Attach to this exact Infest.exe process ID.
        #[arg(long, global = true)]
        pid: Option<u32>,

        #[command(subcommand)]
        command: Command,
    }

    #[derive(Debug, Subcommand)]
    enum Command {
        /// Capture the current 760-slot Handler-8 model table once.
        Models {
            #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
            format: OutputFormat,
            #[arg(long)]
            output: Option<PathBuf>,
        },
        /// Capture the current proven world+0x60 handle list once.
        Entities {
            #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
            format: OutputFormat,
            #[arg(long)]
            output: Option<PathBuf>,
        },
        /// Capture stable Handler-8 model-table changes as JSONL.
        ModelTimeline {
            #[arg(long, default_value_t = 30.0)]
            seconds: f64,
            #[arg(long, default_value_t = 10)]
            hz: u32,
            #[arg(long)]
            stop_file: Option<PathBuf>,
            #[arg(long)]
            output: PathBuf,
        },
        /// Capture raw entity-list state as JSONL without assigning class semantics.
        EntityTimeline {
            #[arg(long, default_value_t = 30.0)]
            seconds: f64,
            #[arg(long, default_value_t = 20)]
            hz: u32,
            #[arg(long)]
            stop_file: Option<PathBuf>,
            #[arg(long)]
            output: PathBuf,
        },
    }

    #[derive(Debug, Clone, Copy, ValueEnum)]
    enum OutputFormat {
        Text,
        Json,
    }

    #[derive(Serialize)]
    struct CaptureMeta<'a> {
        record_kind: &'static str,
        tool_version: &'static str,
        command: &'a str,
        process_id: u32,
        executable: &'a str,
        build: &'a BuildFingerprint,
        evidence_scope: &'static str,
    }

    #[derive(Serialize)]
    struct TimelineSample<'a> {
        record_kind: &'static str,
        sample: u64,
        elapsed_ms: f64,
        changed: bool,
        pointer_hash: String,
        snapshot: &'a ModelTableSnapshot,
    }

    #[derive(Serialize)]
    struct EntityTimelineSample<'a> {
        record_kind: &'static str,
        sample: u64,
        elapsed_ms: f64,
        snapshot: &'a entity_list::EntityListSnapshot,
    }

    #[derive(Serialize)]
    struct TimelineEnd<'a> {
        record_kind: &'static str,
        reason: &'a str,
        samples_completed: u64,
        samples_requested: u64,
        elapsed_ms: f64,
    }

    pub fn run() -> Result<(), String> {
        let cli = Cli::parse();
        let process = Process::attach(cli.pid)?;
        match cli.command {
            Command::Models { format, output } => {
                let snapshot = model_table::capture(&process);
                let rendered = match format {
                    OutputFormat::Text => render_text(&process, &snapshot),
                    OutputFormat::Json => serde_json::to_string_pretty(&snapshot)
                        .map_err(|error| error.to_string())?,
                };
                if let Some(path) = output {
                    ensure_parent(&path)?;
                    std::fs::write(&path, rendered)
                        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                } else {
                    println!("{rendered}");
                }
                if !snapshot.stable {
                    return Err("snapshot retained explicit unstable/unreadable state".into());
                }
            }
            Command::Entities { format, output } => {
                let snapshot = entity_list::capture(&process);
                let rendered = match format {
                    OutputFormat::Text => render_entities_text(&process, &snapshot),
                    OutputFormat::Json => serde_json::to_string_pretty(&snapshot)
                        .map_err(|error| error.to_string())?,
                };
                if let Some(path) = output {
                    ensure_parent(&path)?;
                    std::fs::write(&path, rendered)
                        .map_err(|error| format!("could not write {}: {error}", path.display()))?;
                } else {
                    println!("{rendered}");
                }
                if !snapshot.stable {
                    return Err("snapshot retained explicit unstable/unreadable state".into());
                }
            }
            Command::ModelTimeline {
                seconds,
                hz,
                stop_file,
                output,
            } => {
                validate_timeline(seconds, hz)?;
                timeline::ensure_stop_file_absent(stop_file.as_deref())?;
                timeline::ensure_distinct_capture_paths(stop_file.as_deref(), &output)?;
                ensure_parent(&output)?;
                let mut writer =
                    BufWriter::new(File::create(&output).map_err(|error| {
                        format!("could not create {}: {error}", output.display())
                    })?);
                write_json_line(
                    &mut writer,
                    &CaptureMeta {
                        record_kind: "capture_meta",
                        tool_version: env!("CARGO_PKG_VERSION"),
                        command: "model_timeline",
                        process_id: process.process_id,
                        executable: &process.exe_name,
                        build: &process.build,
                        evidence_scope:
                            "passive Handler-8 registration table only; no callbacks or draw state",
                    },
                )?;
                let mut previous_hash = None;
                let outcome =
                    timeline::run(seconds, hz, stop_file.as_deref(), |sample, elapsed_ms| {
                        let pointer_snapshot = model_table::capture_pointers(&process);
                        let pointer_hash = pointer_snapshot.pointer_hash();
                        let pointer_changed = previous_hash.as_ref() != Some(&pointer_hash);
                        if pointer_changed || !pointer_snapshot.stable {
                            let snapshot = if pointer_changed && pointer_snapshot.stable {
                                model_table::capture(&process)
                            } else {
                                pointer_snapshot
                            };
                            let pointer_hash = snapshot.pointer_hash();
                            let changed = previous_hash.as_ref() != Some(&pointer_hash);
                            write_json_line(
                                &mut writer,
                                &TimelineSample {
                                    record_kind: "model_table_sample",
                                    sample,
                                    elapsed_ms,
                                    changed,
                                    pointer_hash: pointer_hash.clone(),
                                    snapshot: &snapshot,
                                },
                            )?;
                            previous_hash = Some(pointer_hash);
                        } else {
                            previous_hash = Some(pointer_hash);
                        }
                        Ok(())
                    })?;
                write_json_line(
                    &mut writer,
                    &TimelineEnd {
                        record_kind: "timeline_end",
                        reason: outcome.reason.as_str(),
                        samples_completed: outcome.samples_completed,
                        samples_requested: outcome.samples_requested,
                        elapsed_ms: outcome.elapsed_ms,
                    },
                )?;
                writer.flush().map_err(|error| error.to_string())?;
                println!("wrote model timeline to {}", output.display());
            }
            Command::EntityTimeline {
                seconds,
                hz,
                stop_file,
                output,
            } => {
                validate_timeline(seconds, hz)?;
                timeline::ensure_stop_file_absent(stop_file.as_deref())?;
                timeline::ensure_distinct_capture_paths(stop_file.as_deref(), &output)?;
                ensure_parent(&output)?;
                let mut writer =
                    BufWriter::new(File::create(&output).map_err(|error| {
                        format!("could not create {}: {error}", output.display())
                    })?);
                write_json_line(
                &mut writer,
                &CaptureMeta {
                    record_kind: "capture_meta",
                    tool_version: env!("CARGO_PKG_VERSION"),
                    command: "entity_timeline",
                    process_id: process.process_id,
                    executable: &process.exe_name,
                    build: &process.build,
                    evidence_scope:
                        "passive world+0x60 handle list; raw fields only; no callback invocation",
                },
            )?;
                write_json_line(&mut writer, &model_table::capture(&process))?;
                let outcome =
                    timeline::run(seconds, hz, stop_file.as_deref(), |sample, elapsed_ms| {
                        let snapshot = entity_list::capture(&process);
                        write_json_line(
                            &mut writer,
                            &EntityTimelineSample {
                                record_kind: "entity_list_sample",
                                sample,
                                elapsed_ms,
                                snapshot: &snapshot,
                            },
                        )
                    })?;
                write_json_line(
                    &mut writer,
                    &TimelineEnd {
                        record_kind: "timeline_end",
                        reason: outcome.reason.as_str(),
                        samples_completed: outcome.samples_completed,
                        samples_requested: outcome.samples_requested,
                        elapsed_ms: outcome.elapsed_ms,
                    },
                )?;
                writer.flush().map_err(|error| error.to_string())?;
                println!("wrote entity timeline to {}", output.display());
            }
        }
        Ok(())
    }
    fn render_entities_text(
        process: &Process,
        snapshot: &entity_list::EntityListSnapshot,
    ) -> String {
        let mut lines = vec![
            format!("{} PID {}", process.exe_name, process.process_id),
            format!(
                "world={:08X} list={:08X} stable={} entities={}",
                snapshot.world_pointer,
                snapshot.list_head,
                snapshot.stable,
                snapshot.entities.len()
            ),
        ];
        if let Some(error) = &snapshot.error {
            lines.push(format!("ERROR: {error}"));
        }
        for entity in &snapshot.entities {
            lines.push(format!(
                "entity={:08X} handle={:08X} model={:4} pos_raw={:6},{:6},{:6} flags={:08X}",
                entity.entity_pointer,
                entity.effective_handle,
                entity.model_global_id,
                entity.position_raw[0],
                entity.position_raw[1],
                entity.position_raw[2],
                entity.flags_raw
            ));
        }
        lines.join("\n")
    }

    fn render_text(process: &Process, snapshot: &ModelTableSnapshot) -> String {
        let mut lines = vec![
            format!(
                "{} PID {} (expected file SHA-256 {})",
                process.exe_name, process.process_id, process.build.expected_sha256
            ),
            format!(
                "Handler-8 table: stable={} active={}/{}",
                snapshot.stable,
                snapshot.active_slots,
                snapshot.slots.len()
            ),
        ];
        if let Some(error) = &snapshot.error {
            lines.push(format!("ERROR: {error}"));
        }
        for slot in snapshot
            .slots
            .iter()
            .filter(|slot| !matches!(slot.state, SlotState::Empty))
        {
            if let Some(descriptor) = &slot.descriptor {
                lines.push(format!(
                    "#{:03} {:08X} {:016} {:>6} bytes {}",
                    slot.global_id,
                    slot.pointer,
                    descriptor.fnv1a64,
                    descriptor.aligned_bytes,
                    descriptor.name.as_deref().unwrap_or("<unnamed>")
                ));
            } else {
                lines.push(format!(
                    "#{:03} {:08X} {:?}: {}",
                    slot.global_id,
                    slot.pointer,
                    slot.state,
                    slot.error.as_deref().unwrap_or("no detail")
                ));
            }
        }
        lines.join("\n")
    }

    fn validate_timeline(seconds: f64, hz: u32) -> Result<(), String> {
        if !seconds.is_finite() || !(0.05..=3600.0).contains(&seconds) {
            return Err("--seconds must be between 0.05 and 3600".into());
        }
        if !(1..=100).contains(&hz) {
            return Err("--hz must be between 1 and 100".into());
        }
        Ok(())
    }

    fn ensure_parent(path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                create_dir_all(parent).map_err(|error| {
                    format!(
                        "could not create output directory {}: {error}",
                        parent.display()
                    )
                })?;
            }
        }
        Ok(())
    }

    fn write_json_line(writer: &mut impl Write, value: &impl Serialize) -> Result<(), String> {
        serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
        writeln!(writer).map_err(|error| error.to_string())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn model_timeline_cli_retains_capture_controls() {
            let cli = Cli::try_parse_from([
                "inf-original-inspect",
                "--pid",
                "1234",
                "model-timeline",
                "--seconds",
                "12.5",
                "--hz",
                "20",
                "--stop-file",
                "stop.marker",
                "--output",
                "models.jsonl",
            ])
            .expect("parse");
            assert_eq!(cli.pid, Some(1234));
            match cli.command {
                Command::ModelTimeline {
                    seconds,
                    hz,
                    stop_file,
                    output,
                } => {
                    assert_eq!(seconds, 12.5);
                    assert_eq!(hz, 20);
                    assert_eq!(stop_file, Some(PathBuf::from("stop.marker")));
                    assert_eq!(output, PathBuf::from("models.jsonl"));
                }
                _ => panic!("unexpected command"),
            }
        }

        #[test]
        fn entity_timeline_cli_retains_capture_controls() {
            let cli = Cli::try_parse_from([
                "inf-original-inspect",
                "entity-timeline",
                "--seconds",
                "15",
                "--hz",
                "40",
                "--stop-file",
                "stop.marker",
                "--output",
                "entities.jsonl",
            ])
            .expect("parse");
            match cli.command {
                Command::EntityTimeline {
                    seconds,
                    hz,
                    stop_file,
                    output,
                } => {
                    assert_eq!(seconds, 15.0);
                    assert_eq!(hz, 40);
                    assert_eq!(stop_file, Some(PathBuf::from("stop.marker")));
                    assert_eq!(output, PathBuf::from("entities.jsonl"));
                }
                _ => panic!("unexpected command"),
            }
        }
    }
}
