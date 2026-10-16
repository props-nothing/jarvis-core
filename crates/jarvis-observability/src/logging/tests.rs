use std::{fmt::Write as _, io::Cursor, process::Command};

use super::*;

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jarvis-observability-logging-{}",
            jarvis_core::scratch_tag()
        ));
        fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create temp dir: {error}"));
        Self(path)
    }

    fn file(&self) -> PathBuf {
        self.0.join(LOG_FILE_NAME)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        jarvis_core::remove_scratch_dir(&self.0);
    }
}

#[test]
fn a_missing_log_reads_as_empty_rather_than_erroring() {
    let directory = TempDirectory::new();
    assert!(
        read_tail(&directory.file(), DEFAULT_TAIL_LINES)
            .unwrap_or_else(|error| panic!("read: {error}"))
            .is_empty()
    );
    assert_eq!(
        count_lines(&directory.file()).unwrap_or_else(|error| panic!("count: {error}")),
        0
    );
    assert_eq!(
        log_storage_stats(&directory.file()).unwrap_or_else(|error| panic!("stats: {error}")),
        LogStorageStats::default()
    );
}

#[test]
fn tail_returns_exact_records_across_many_windows_and_utf8_boundaries() {
    let directory = TempDirectory::new();
    let records: Vec<_> = (1..=700)
        .map(|index| format!("{{\"n\":{index},\"pad\":\"{}\"}}", "🙂é".repeat(170)))
        .collect();
    let content = records.join("\n") + "\n";
    assert!(content.len() > READ_WINDOW_BYTES * 10);
    fs::write(directory.file(), content).unwrap_or_else(|error| panic!("write: {error}"));
    let tail = read_tail(&directory.file(), 600).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(tail.len(), 600);
    for (index, line) in tail.iter().enumerate() {
        assert_eq!(line.text, records[index + 100]);
        assert_eq!(line.number, (index + 1) as u64);
        let _: serde_json::Value = serde_json::from_str(&line.text)
            .unwrap_or_else(|error| panic!("invalid JSON: {error}"));
    }
}

#[test]
fn tail_omits_blank_oversized_and_partial_records_without_splitting_neighbors() {
    let directory = TempDirectory::new();
    let oversized = "é".repeat(READ_WINDOW_BYTES);
    let content = format!("{{\"first\":1}}\r\n\n{oversized}\n{{\"last\":2}}\n{{\"incomplete\":");
    fs::write(directory.file(), content).unwrap_or_else(|error| panic!("write: {error}"));
    let tail = read_tail(&directory.file(), 10).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(
        tail.iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        [r#"{"first":1}"#, r#"{"last":2}"#]
    );
    fs::write(directory.file(), "{\"unterminated\":1}")
        .unwrap_or_else(|error| panic!("write: {error}"));
    assert!(
        read_tail(&directory.file(), 1)
            .unwrap_or_else(|error| panic!("read: {error}"))
            .is_empty()
    );
}

struct CountingReader {
    cursor: Cursor<Vec<u8>>,
    bytes_read: usize,
}

impl Read for CountingReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.cursor.read(buffer)?;
        self.bytes_read += read;
        Ok(read)
    }
}

impl Seek for CountingReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.cursor.seek(position)
    }
}

#[test]
fn tail_io_is_bounded_even_for_an_oversized_legacy_file_without_newlines() {
    let budget =
        usize::try_from(MAX_TAIL_BYTES).unwrap_or_else(|_| panic!("tail budget fits usize"));
    let mut bytes = vec![b'x'; budget * 2];
    bytes.extend_from_slice(b"\n{\"recent\":true}\n");
    let mut reader = CountingReader {
        cursor: Cursor::new(bytes),
        bytes_read: 0,
    };
    let tail = read_tail_from(&mut reader, 10).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(tail.len(), 1);
    assert_eq!(tail[0].text, r#"{"recent":true}"#);
    assert_eq!(reader.bytes_read as u64, MAX_TAIL_BYTES);
}

#[test]
fn a_small_tail_does_not_scan_the_file_to_count_absolute_lines() {
    let mut bytes = Vec::new();
    for index in 0..80_000 {
        bytes.extend_from_slice(format!("{{\"n\":{index}}}\n").as_bytes());
    }
    let mut reader = CountingReader {
        cursor: Cursor::new(bytes),
        bytes_read: 0,
    };
    let tail = read_tail_from(&mut reader, 2).unwrap_or_else(|error| panic!("read: {error}"));
    assert_eq!(tail[0].text, r#"{"n":79998}"#);
    assert_eq!(tail[1].text, r#"{"n":79999}"#);
    assert_eq!(reader.bytes_read, READ_WINDOW_BYTES);
}

#[test]
fn malformed_utf8_is_reported_and_limits_are_applied() {
    let mut invalid = Cursor::new(vec![b'{', 0xff, b'}', b'\n']);
    assert!(read_tail_from(&mut invalid, 1).is_err());
    let content = "{}\n".repeat(MAX_TAIL_LINES + 2);
    assert_eq!(
        read_tail_from(&mut Cursor::new(content.as_bytes()), usize::MAX)
            .unwrap_or_else(|error| panic!("read: {error}"))
            .len(),
        MAX_TAIL_LINES
    );
    assert_eq!(
        read_tail_from(&mut Cursor::new(content.as_bytes()), 0)
            .unwrap_or_else(|error| panic!("read: {error}"))
            .len(),
        1
    );
}

fn rotating(directory: &TempDirectory, maximum: u64) -> RotatingFile {
    RotatingFile::open(directory.file(), maximum, RETAINED_LOG_FILES)
        .unwrap_or_else(|error| panic!("open: {error}"))
}

#[test]
fn rotation_keeps_whole_records_and_a_fixed_number_of_bounded_segments() {
    let directory = TempDirectory::new();
    let mut file = rotating(&directory, 256);
    for index in 0..100 {
        let line = format!("{{\"n\":{index},\"pad\":\"{}\"}}\n", "x".repeat(60));
        file.write_record(line.as_bytes())
            .unwrap_or_else(|error| panic!("write: {error}"));
    }
    file.flush()
        .unwrap_or_else(|error| panic!("flush: {error}"));
    drop(file);
    let files: Vec<_> = fs::read_dir(&directory.0)
        .unwrap_or_else(|error| panic!("list: {error}"))
        .map(|entry| {
            entry
                .unwrap_or_else(|error| panic!("entry: {error}"))
                .path()
        })
        .collect();
    assert_eq!(files.len(), RETAINED_LOG_FILES + 1);
    let mut retained = Vec::new();
    for path in files {
        let text = fs::read_to_string(path).unwrap_or_else(|error| panic!("read: {error}"));
        assert!(text.len() <= 256);
        assert!(text.ends_with('\n'));
        for line in text.lines() {
            let value: serde_json::Value =
                serde_json::from_str(line).unwrap_or_else(|error| panic!("JSON: {error}"));
            retained.push(value["n"].as_u64().unwrap_or_else(|| panic!("number")));
        }
    }
    retained.sort_unstable();
    assert_eq!(retained.last(), Some(&99));
    assert!(!retained.contains(&0));
    assert!(retained.windows(2).all(|pair| pair[0] + 1 == pair[1]));
    let mut reopened = rotating(&directory, 256);
    reopened
        .write_record(b"{\"restarted\":true}\n")
        .unwrap_or_else(|error| panic!("restart append: {error}"));
}

#[test]
fn a_crash_fragment_is_rotated_before_new_events_are_written() {
    let directory = TempDirectory::new();
    fs::write(directory.file(), b"{\"partial\":").unwrap_or_else(|error| panic!("write: {error}"));
    let mut file = rotating(&directory, 256);
    file.write_record(b"{\"new\":true}\n")
        .unwrap_or_else(|error| panic!("append: {error}"));
    assert_eq!(
        fs::read_to_string(directory.file()).unwrap_or_else(|error| panic!("read: {error}")),
        "{\"new\":true}\n"
    );
    assert_eq!(
        fs::read_to_string(segment_path(&directory.file(), 1))
            .unwrap_or_else(|error| panic!("read: {error}")),
        "{\"partial\":"
    );
}

#[test]
fn rotation_failure_fails_closed_without_growing_or_truncating_the_active_log() {
    let directory = TempDirectory::new();
    let mut file = rotating(&directory, 16);
    file.write_record(b"{\"first\":true}\n")
        .unwrap_or_else(|error| panic!("first: {error}"));
    let before = fs::read(directory.file()).unwrap_or_else(|error| panic!("read: {error}"));
    let obstruction = segment_path(&directory.file(), RETAINED_LOG_FILES);
    fs::create_dir(&obstruction).unwrap_or_else(|error| panic!("obstruction: {error}"));
    assert!(file.write_record(b"{\"new\":true}\n").is_err());
    assert!(
        file.file.is_some(),
        "archive failure need not close the active handle"
    );
    assert_eq!(
        fs::read(directory.file()).unwrap_or_else(|error| panic!("read: {error}")),
        before
    );
    fs::remove_dir(obstruction).unwrap_or_else(|error| panic!("remove obstruction: {error}"));
    assert!(
        file.write_record(b"{}\n").is_err(),
        "a failed sink stays closed"
    );
    assert!(file.flush().is_err());
}

#[test]
fn oversized_legacy_logs_are_reported_and_never_silently_deleted() {
    let directory = TempDirectory::new();
    let file = File::create(directory.file()).unwrap_or_else(|error| panic!("create: {error}"));
    file.set_len(MAX_LOG_FILE_BYTES + 1)
        .unwrap_or_else(|error| panic!("size: {error}"));
    drop(file);
    let stats =
        log_storage_stats(&directory.file()).unwrap_or_else(|error| panic!("stats: {error}"));
    assert!(stats.over_limit);
    assert_eq!(stats.active_bytes, MAX_LOG_FILE_BYTES + 1);
    assert!(RotatingFile::open(directory.file(), MAX_LOG_FILE_BYTES, RETAINED_LOG_FILES).is_err());
    assert!(count_lines(&directory.file()).is_err());
    assert_eq!(
        fs::metadata(directory.file())
            .unwrap_or_else(|error| panic!("metadata: {error}"))
            .len(),
        MAX_LOG_FILE_BYTES + 1
    );
}

#[cfg(unix)]
#[test]
fn created_and_rotated_segments_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let directory = TempDirectory::new();
    let mut file = rotating(&directory, 3);
    file.write_record(b"{}\n")
        .unwrap_or_else(|error| panic!("write: {error}"));
    file.write_record(b"{}\n")
        .unwrap_or_else(|error| panic!("write: {error}"));
    for path in [directory.file(), segment_path(&directory.file(), 1)] {
        let mode = fs::metadata(path)
            .unwrap_or_else(|error| panic!("metadata: {error}"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o077, 0);
    }
}

const CANARY: &str = "logging-canary-938a67c2d5f04b81";
const ESCAPED_CANARY: &str = "quoted-canary-\"secret\"-\\suffix";
const PROBE_DIRECTORY: &str = "JARVIS_LOGGING_TEST_DIRECTORY";
const PROBE_FAILURE: &str = "JARVIS_LOGGING_TEST_FAILURE";

/// Run only in the subprocess: the global subscriber must not affect other tests.
#[test]
fn subprocess_sink_probe() {
    let Some(directory) = std::env::var_os(PROBE_DIRECTORY) else {
        return;
    };
    let logging = Logging::init(
        Path::new(&directory),
        LogLevel::Info,
        Redactor::new([CANARY, ESCAPED_CANARY]),
    )
    .unwrap_or_else(|error| panic!("init: {error}"));
    if std::env::var_os(PROBE_FAILURE).is_some() {
        let Destination::File(file) = &logging.sink.destination else {
            panic!("file sink");
        };
        file.lock().unwrap_or_else(|_| panic!("lock")).maximum = 512;
        fs::create_dir(segment_path(logging.path(), RETAINED_LOG_FILES))
            .unwrap_or_else(|error| panic!("obstruct: {error}"));
        for _ in 0..10 {
            tracing::info!(secret = CANARY, pad = "x".repeat(150), "failure probe");
        }
        assert!(logging.flush().is_err());
    } else {
        tracing::info!(value = CANARY, "ordinary event");
        tracing::info!("Authorization: Bearer unknown-fake-token-7291");
        let span = tracing::info_span!("probe", secret = CANARY);
        let _guard = span.enter();
        tracing::info!("in a span");
        tracing::info!(password = 123_456_789_u64, "numeric credential field");
        tracing::info!(value = ESCAPED_CANARY, "escaped known credential");
        tracing::info!(
            password = "unknown-\"quoted\"-credential-tail",
            "escaped credential field"
        );
        let mut oversized = "é".repeat(MAX_LINE_BYTES);
        write!(oversized, " {CANARY}").unwrap_or_else(|_| unreachable!());
        tracing::info!(payload = oversized, "oversized event");
        logging
            .flush()
            .unwrap_or_else(|error| panic!("flush: {error}"));
    }
}

fn run_probe(directory: &TempDirectory, failure: bool) -> std::process::Output {
    let mut command =
        Command::new(std::env::current_exe().unwrap_or_else(|error| panic!("test exe: {error}")));
    command
        .args([
            "--exact",
            "logging::tests::subprocess_sink_probe",
            "--nocapture",
        ])
        .env(PROBE_DIRECTORY, &directory.0);
    if failure {
        command.env(PROBE_FAILURE, "1");
    } else {
        command.env_remove(PROBE_FAILURE);
    }
    let output = command
        .output()
        .unwrap_or_else(|error| panic!("child: {error}"));
    assert!(output.status.success(), "child failed: {output:?}");
    output
}

#[test]
fn both_real_tracing_sinks_mask_canaries_and_emit_bounded_records() {
    let directory = TempDirectory::new();
    let output = run_probe(&directory, false);
    let stderr = String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("UTF-8: {error}"));
    let file = fs::read_to_string(directory.file()).unwrap_or_else(|error| panic!("read: {error}"));
    for text in [&stderr, &file] {
        assert!(!text.contains(CANARY), "secret reached a real tracing sink");
        assert!(!text.contains("123456789"), "numeric credential escaped");
        assert!(
            !text.contains("quoted-canary-"),
            "JSON escaping bypassed known-secret masking"
        );
        assert!(
            !text.contains("credential-tail"),
            "quoted credential leaked a suffix"
        );
        assert!(
            !text.contains("unknown-fake-token-7291"),
            "bearer credential escaped"
        );
        assert!(text.contains("[REDACTED]"));
        assert!(text.contains("log event omitted: exceeds record limit"));
        assert!(text.lines().all(|line| line.len() < MAX_LINE_BYTES));
        assert_eq!(text.lines().count(), 7, "{text}");
    }
    assert!(file.contains("redaction changed JSON structure"));
    for line in file.lines() {
        let _: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|error| panic!("invalid JSON: {error}"));
    }
}

#[test]
fn file_failure_keeps_stderr_redacted_and_has_no_raw_formatter_fallback() {
    let directory = TempDirectory::new();
    let output = run_probe(&directory, true);
    let stderr = String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("UTF-8: {error}"));
    assert!(!stderr.contains(CANARY), "writer error exposed raw event");
    assert!(stderr.contains("[REDACTED]"));
    assert_eq!(stderr.matches("structured logging stopped").count(), 1);
    assert_eq!(stderr.matches("failure probe").count(), 10);
    assert!(
        fs::metadata(directory.file())
            .unwrap_or_else(|error| panic!("metadata: {error}"))
            .len()
            <= 512
    );
}
