use super::{
    ensure, invalid, ObservationError, ObservedModelCallback, ObservedModelEvent,
    ObservedModelStage as Stage, TtdPosition, CONTEXT_BYTES,
};
use std::collections::BTreeMap;

pub(super) struct DumpSpec {
    pub name: &'static str,
    pub length: usize,
    stage: Stage,
}
pub(super) const DUMPS: &[DumpSpec] = &[
    DumpSpec {
        name: "model-entry-before.bin",
        length: 4696,
        stage: Stage::Entry,
    },
    DumpSpec {
        name: "model-entry.bin",
        length: 4696,
        stage: Stage::End,
    },
    DumpSpec {
        name: "f5-context.bin",
        length: CONTEXT_BYTES,
        stage: Stage::F5,
    },
    DumpSpec {
        name: "f5-slot0.bin",
        length: 24,
        stage: Stage::F5,
    },
    DumpSpec {
        name: "callback-return-context.bin",
        length: CONTEXT_BYTES,
        stage: Stage::Return,
    },
    DumpSpec {
        name: "post05-context.bin",
        length: CONTEXT_BYTES,
        stage: Stage::Post05,
    },
    DumpSpec {
        name: "post05-vertex-slots.bin",
        length: 4080,
        stage: Stage::Post05,
    },
    DumpSpec {
        name: "post05-projection-globals.bin",
        length: 144,
        stage: Stage::Post05,
    },
];
pub(super) struct ParsedTranscript {
    pub events: Vec<ObservedModelEvent>,
    pub fields: Vec<BTreeMap<String, String>>,
    pub scans: u32,
    pub parent_context: u32,
    pub vertex_address: u32,
    pub callback: ObservedModelCallback,
    pub dumps: BTreeMap<String, Vec<u8>>,
}
struct ActiveDump {
    name: String,
    address: u32,
    length: usize,
    bytes: Vec<u8>,
}

pub(super) fn parse(text: &str, max_hits: u32) -> Result<ParsedTranscript, ObservationError> {
    let expected = [
        Stage::Entry,
        Stage::F5,
        Stage::Call,
        Stage::Return,
        Stage::Write,
        Stage::Op05,
        Stage::Post05,
        Stage::End,
    ];
    let mut events: Vec<ObservedModelEvent> = Vec::new();
    let mut fields = Vec::new();
    let mut pending = None;
    let mut current_position = None;
    let mut dumps = BTreeMap::new();
    let mut active: Option<ActiveDump> = None;
    let (mut pe, mut complete, mut query_complete) = (false, false, false);
    let mut scans = None;
    for original in text.lines() {
        let line = original.trim_end();
        if let Some(dump) = active.as_mut() {
            if line == "INF_HEALTH_DUMP_END" {
                ensure(
                    dump.bytes.len() == dump.length,
                    format!("{} transcript byte count mismatch", dump.name),
                )?;
                let done = active.take().unwrap();
                dumps.insert(done.name, done.bytes);
            } else if !line.is_empty() {
                parse_row(line, dump)?;
            }
            continue;
        }
        if [
            "Cannot execute",
            "Syntax error",
            "Extra character error",
            "Command file execution failed",
            "Unable to open command file",
            "E_FAIL",
            "Memory access error",
            "Unable to read memory",
            "Cannot read memory",
        ]
        .iter()
        .any(|error| line.contains(error))
        {
            return Err(invalid("debugger reported an error"));
        }
        let unindented = line.trim_start();
        ensure(
            unindented == line
                || !(unindented.starts_with("Current position:")
                    || unindented.starts_with("Time Travel Position:")),
            "indented replay position marker",
        )?;
        let marker = line.split_whitespace().next().unwrap_or("");
        if matches!(
            marker,
            "INF_HEALTH_INTERRUPTED"
                | "INF_HEALTH_HIT_LIMIT"
                | "INF_TTD_HIT_LIMIT"
                | "INF_HEALTH_ERROR"
                | "INF_HEALTH_PARTIAL"
                | "INF_HEALTH_ABORT"
                | "INF_HEALTH_MISMATCH"
        ) {
            return Err(invalid(format!("incomplete replay marker {marker}")));
        }
        if line == "INF_TTD_TARGET_PE_VALIDATED" {
            ensure(
                !pe && events.is_empty(),
                "duplicate or late PE validation marker",
            )?;
            pe = true;
        } else if marker == "INF_HEALTH_EVENT" {
            ensure(
                pe && !complete && scans.is_none() && !query_complete && pending.is_none(),
                "event outside the active positioned invocation",
            )?;
            ensure(events.len() < expected.len(), "extra observation event")?;
            let data = parse_fields(
                line.strip_prefix("INF_HEALTH_EVENT")
                    .ok_or_else(|| invalid("indented event marker"))?,
            )?;
            let stage = expected[events.len()];
            ensure(
                data.get("stage").map(String::as_str) == Some(stage.as_str()),
                "events are missing, duplicated, or out of order",
            )?;
            let event = ObservedModelEvent {
                stage,
                position: TtdPosition {
                    sequence: 0,
                    steps: 0,
                },
                thread_id: hex(&data, "thread")?,
                context_address: hex(&data, "ctx")?,
                entry_address: hex(&data, "entry")?,
                args_address: hex(&data, "args")?,
            };
            ensure(
                event.thread_id != 0 && event.context_address != 0 && event.entry_address != 0,
                "null invocation identity",
            )?;
            if let Some(first) = events.first() {
                ensure(
                    (event.thread_id, event.context_address, event.entry_address)
                        == (first.thread_id, first.context_address, first.entry_address),
                    "event thread/context/entry changed",
                )?;
            }
            let args_offset = match stage {
                Stage::Entry | Stage::End => None,
                Stage::Op05 | Stage::Post05 => Some(0x420),
                _ => Some(0x418),
            };
            let args = if let Some(offset) = args_offset {
                event
                    .entry_address
                    .checked_add(offset)
                    .ok_or_else(|| invalid("entry argument address overflows"))?
            } else {
                0
            };
            ensure(
                event.args_address == args,
                format!(
                    "{} argument pointer does not match canonical source offset",
                    stage.as_str()
                ),
            )?;
            pending = Some(events.len());
            events.push(event);
            fields.push(data);
        } else if let Some(position) = line.strip_prefix("Time Travel Position: ") {
            // Debugger stop/seek output also reports a position. It changes
            // dump custody, but only the event's explicit !tt output below
            // completes its pending position association.
            current_position = Some(position.trim().parse()?);
        } else if let Some(position) = line.strip_prefix("Current position: ") {
            let position: TtdPosition = position.trim().parse()?;
            current_position = Some(position);
            if let Some(index) = pending.take() {
                ensure(
                    index == 0 || position > events[index - 1].position,
                    "event positions do not strictly increase",
                )?;
                events[index].position = position;
            }
        } else if marker == "INF_HEALTH_DUMP" {
            ensure(
                pending.is_none() && !complete && scans.is_none() && !query_complete,
                "dump outside positioned event",
            )?;
            let header = parse_fields(
                line.strip_prefix("INF_HEALTH_DUMP")
                    .ok_or_else(|| invalid("indented dump marker"))?,
            )?;
            ensure(header.len() == 3, "dump header has unexpected fields")?;
            let name = header
                .get("name")
                .ok_or_else(|| invalid("dump name missing"))?;
            let spec = DUMPS
                .iter()
                .find(|spec| spec.name == name)
                .ok_or_else(|| invalid(format!("unknown dump {name}")))?;
            ensure(
                events.last().map(|event| event.stage) == Some(spec.stage),
                format!("{name} dump has wrong producing event"),
            )?;
            ensure(
                events.last().map(|event| event.position) == current_position,
                format!("{name} dump position differs from its producing event"),
            )?;
            ensure(!dumps.contains_key(name), format!("duplicate dump {name}"))?;
            let length = decimal(&header, "bytes")? as usize;
            ensure(
                length == spec.length,
                format!("{name} declared length mismatch"),
            )?;
            let address_text = header
                .get("address")
                .ok_or_else(|| invalid("dump address missing"))?;
            ensure(address_text.len() == 8, "dump address is not an x86 DWORD")?;
            let address = hex(&header, "address")?;
            let expected_address = match name.as_str() {
                "model-entry-before.bin" | "model-entry.bin" => events[0].entry_address,
                "f5-slot0.bin" | "post05-vertex-slots.bin" => hex(&fields[1], "vertices")?,
                "post05-projection-globals.bin" => 0x0055f1c4,
                _ => events[0].context_address,
            };
            ensure(
                address == expected_address && u64::from(address) + length as u64 <= 0x1_0000_0000,
                format!("{name} dump address does not match its invocation"),
            )?;
            active = Some(ActiveDump {
                name: name.clone(),
                address,
                length,
                bytes: Vec::with_capacity(length),
            });
        } else if line == "INF_HEALTH_DUMP_END" {
            return Err(invalid("orphan dump end marker"));
        } else if line == "INF_HEALTH_COMPLETE" {
            ensure(
                !complete && scans.is_none() && pending.is_none() && events.len() == 8,
                "completion before positioned draw end, or duplicate completion",
            )?;
            complete = true;
        } else if marker == "INF_HEALTH_STOP" {
            ensure(
                complete && scans.is_none() && pending.is_none(),
                "missing completion or duplicate stop",
            )?;
            let data = parse_fields(
                line.strip_prefix("INF_HEALTH_STOP")
                    .ok_or_else(|| invalid("indented stop marker"))?,
            )?;
            let count = decimal(&data, "scans")?;
            ensure(
                data.len() == 3
                    && count > 0
                    && count <= max_hits
                    && decimal(&data, "matches")? == 1
                    && decimal(&data, "stage")? == 8,
                "stop counts do not describe one completed bounded invocation",
            )?;
            scans = Some(count);
        } else if line == "INF_TTD_QUERY_COMPLETE" {
            ensure(
                !query_complete && complete && scans.is_some(),
                "query completed without a validated draw stop",
            )?;
            query_complete = true;
        }
    }
    ensure(
        pe && complete && query_complete && pending.is_none() && active.is_none(),
        "incomplete observation transcript",
    )?;
    ensure(
        DUMPS.iter().all(|spec| dumps.contains_key(spec.name)),
        "missing positioned dump",
    )?;
    ensure(
        events.last().map(|event| event.position) == current_position,
        "final replay position differs from the completed draw",
    )?;
    let parent_context = hex(&fields[0], "parent")?;
    let vertex_address = hex(&fields[1], "vertices")?;
    ensure(
        vertex_address != 0 && hex(&fields[6], "vertices")? == vertex_address,
        "post05 vertex bank changed",
    )?;
    ensure(
        decimal(&fields[1], "near_far")? == 1 && decimal(&fields[6], "near_far")? == 1,
        "only the observed far path has a supported field-validity contract",
    )?;
    ensure(
        hex(&fields[2], "selector")? == 0
            && hex(&fields[2], "f5_args")? == events[1].args_address
            && hex(&fields[2], "return_site")? == 0x004a03b0,
        "callback is not the matched first F5 operand",
    )?;
    let result = hex(&fields[3], "value")?;
    let register1 = hex(&fields[4], "register1")?;
    ensure(
        result <= 0xffff && register1 == (result << 9) & 0xffff,
        "callback WORD or F5 register-shift result mismatch",
    )?;
    let callback = ObservedModelCallback {
        object_address: hex(&fields[2], "object")?,
        vtable_address: hex(&fields[2], "vtable")?,
        target_address: hex(&fields[2], "target")?,
        selector: 0,
        result: result as u16,
        register1: register1 as u16,
    };
    ensure(
        callback.object_address != 0
            && callback.vtable_address != 0
            && callback.target_address != 0,
        "null callback identity",
    )?;
    Ok(ParsedTranscript {
        events,
        fields,
        scans: scans.unwrap(),
        parent_context,
        vertex_address,
        callback,
        dumps,
    })
}

fn parse_row(line: &str, dump: &mut ActiveDump) -> Result<(), ObservationError> {
    let row = line.as_bytes();
    ensure(
        row.len() >= 10 && row[..8].iter().all(u8::is_ascii_hexdigit) && row[8..10] == *b"  ",
        format!("{} contains a malformed memory row", dump.name),
    )?;
    let address =
        u32::from_str_radix(&line[..8], 16).map_err(|_| invalid("bad memory row address"))?;
    ensure(
        u64::from(address) == u64::from(dump.address) + dump.bytes.len() as u64,
        format!("{} contains a noncontiguous memory row", dump.name),
    )?;
    let mut ended = false;
    let before = dump.bytes.len();
    for index in 0..16 {
        let offset = 10 + index * 3;
        let pair = [
            *row.get(offset).unwrap_or(&b' '),
            *row.get(offset + 1).unwrap_or(&b' '),
        ];
        if pair == *b"  " {
            ended = true;
        } else {
            ensure(
                !ended && pair.iter().all(u8::is_ascii_hexdigit),
                format!("{} has missing/unreadable byte cells", dump.name),
            )?;
            ensure(
                dump.bytes.len() < dump.length,
                format!("{} contains excess bytes", dump.name),
            )?;
            let byte = u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap();
            dump.bytes.push(byte);
        }
        if index < 15 {
            let separator = *row.get(offset + 2).unwrap_or(&b' ');
            ensure(
                separator == b' ' || (index == 7 && separator == b'-'),
                "invalid db byte separator",
            )?;
        }
    }
    ensure(dump.bytes.len() > before, "empty memory row")
}
fn parse_fields(text: &str) -> Result<BTreeMap<String, String>, ObservationError> {
    let mut fields = BTreeMap::new();
    for token in text.split_whitespace() {
        let (key, value) = token
            .split_once('=')
            .ok_or_else(|| invalid("malformed record field"))?;
        ensure(
            !key.is_empty()
                && key
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                && !value.is_empty(),
            "malformed record field",
        )?;
        ensure(
            fields.insert(key.to_owned(), value.to_owned()).is_none(),
            "duplicate record field",
        )?;
    }
    Ok(fields)
}
pub(super) fn hex(fields: &BTreeMap<String, String>, key: &str) -> Result<u32, ObservationError> {
    let value = fields
        .get(key)
        .ok_or_else(|| invalid(format!("missing hexadecimal field {key}")))?;
    ensure(
        !value.is_empty() && value.len() <= 8 && value.bytes().all(|b| b.is_ascii_hexdigit()),
        format!("invalid hexadecimal field {key}"),
    )?;
    u32::from_str_radix(value, 16).map_err(|_| invalid(format!("invalid hexadecimal field {key}")))
}
fn decimal(fields: &BTreeMap<String, String>, key: &str) -> Result<u32, ObservationError> {
    let value = fields
        .get(key)
        .ok_or_else(|| invalid(format!("missing decimal field {key}")))?;
    ensure(
        !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()),
        format!("invalid decimal field {key}"),
    )?;
    value
        .parse()
        .map_err(|_| invalid(format!("invalid decimal field {key}")))
}
