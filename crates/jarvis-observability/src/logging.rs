//! Structured logging initialization and the queryable log file behind `jarvis logs`.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use jarvis_core::LogLevel;
use thiserror::Error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::redact::{MAX_LINE_BYTES, Redactor};

/// Filename of the current daemon log inside the log directory.
pub const LOG_FILE_NAME: &str = "jarvisd.jsonl";
/// Default number of records returned by `jarvis logs`.
pub const DEFAULT_TAIL_LINES: usize = 200;
/// Maximum number of records returned by `jarvis logs`.
pub const MAX_TAIL_LINES: usize = 5_000;
/// Maximum size of the active log and each retained segment.
pub const MAX_LOG_FILE_BYTES: u64 = 4 * 1024 * 1024;
/// Number of old segments kept in addition to the active log.
pub const RETAINED_LOG_FILES: usize = 3;
/// Maximum bytes inspected by a tail request, including legacy oversized logs.
pub const MAX_TAIL_BYTES: u64 = MAX_LOG_FILE_BYTES;
const READ_WINDOW_BYTES: usize = 64 * 1024;

/// Explains why logging could not be configured or read.
#[derive(Debug, Error)]
pub enum LoggingError {
    /// The log directory could not be created.
    #[error("failed to create the log directory")]
    Directory {
        /// The underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// The log file could not be opened or written.
    #[error("failed to access the log file")]
    File {
        /// The underlying operating-system error.
        #[source]
        source: io::Error,
    },
    /// A global tracing subscriber was already installed.
    #[error("a global tracing subscriber is already installed")]
    AlreadyInstalled,
    /// The configured log level was not a valid filter directive.
    #[error("the configured log level is not a valid filter")]
    InvalidLevel,
}

#[derive(Debug)]
struct RotatingFile {
    path: PathBuf,
    file: Option<File>,
    bytes: u64,
    maximum: u64,
    retained: usize,
    failed: bool,
}

fn segment_path(path: &Path, index: usize) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".{index}"));
    PathBuf::from(name)
}

fn open_log(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).read(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn segment_size(path: &Path) -> io::Result<u64> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err(io::Error::other("log segment is not a regular file")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error),
    }
}

impl RotatingFile {
    fn open(path: PathBuf, maximum: u64, retained: usize) -> io::Result<Self> {
        // Never silently destroy or archive an unbounded legacy log. Doctor
        // names this state; the owner can move it outside managed retention.
        for segment in std::iter::once(path.clone())
            .chain((1..=retained).map(|index| segment_path(&path, index)))
        {
            if segment_size(&segment)? > maximum {
                return Err(io::Error::other(
                    "existing log exceeds retention limit; archive it before restarting",
                ));
            }
        }
        let mut file = open_log(&path)?;
        let bytes = file.metadata()?.len();
        let incomplete = if bytes > 0 {
            file.seek(SeekFrom::End(-1))?;
            let mut last = [0];
            file.read_exact(&mut last)?;
            last[0] != b'\n'
        } else {
            false
        };
        let mut sink = Self {
            path,
            file: Some(file),
            bytes,
            maximum,
            retained,
            failed: false,
        };
        // Keep a crash fragment as evidence in a retained segment; never append
        // the next JSON event onto it.
        if incomplete {
            sink.rotate()?;
        }
        Ok(sink)
    }

    fn rotate(&mut self) -> io::Result<()> {
        let oldest = segment_path(&self.path, self.retained);
        match fs::remove_file(oldest) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        for index in (1..self.retained).rev() {
            let source = segment_path(&self.path, index);
            match fs::rename(source, segment_path(&self.path, index + 1)) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
        // Windows cannot reliably rename an open file. Keep the active handle
        // until archive preparation succeeds, then close before its rename.
        if let Some(file) = self.file.as_mut() {
            file.flush()?;
        }
        self.file.take();
        if let Err(error) = fs::rename(&self.path, segment_path(&self.path, 1)) {
            self.file = open_log(&self.path).ok();
            return Err(error);
        }
        self.file = Some(open_log(&self.path)?);
        self.bytes = 0;
        Ok(())
    }

    fn write_record(&mut self, record: &[u8]) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("log sink disabled after an I/O failure"));
        }
        let result = self.try_write_record(record);
        if result.is_err() {
            // A failed write can be partial, and a failed rotation may leave
            // shifted archives. Refuse subsequent appends until restart.
            self.failed = true;
        }
        result
    }

    fn try_write_record(&mut self, record: &[u8]) -> io::Result<()> {
        let length = u64::try_from(record.len()).unwrap_or(u64::MAX);
        if length > self.maximum {
            return Err(io::Error::other("log record exceeds segment limit"));
        }
        if self.bytes.saturating_add(length) > self.maximum {
            self.rotate()?;
        }
        let file = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file is unavailable"))?;
        file.write_all(record)?;
        self.bytes += length;
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(io::Error::other("log sink disabled after an I/O failure"));
        }
        self.file
            .as_mut()
            .ok_or_else(|| io::Error::other("log file is unavailable"))?
            .flush()
    }
}

#[derive(Clone, Debug)]
enum Destination {
    File(Arc<Mutex<RotatingFile>>),
    Stderr,
}

/// The pinned tracing formatter calls `write_all` once for each complete event.
/// Both renderings must pass this boundary, including on file failure.
#[derive(Clone, Debug)]
struct RedactingSink {
    destination: Destination,
    redactor: Redactor,
}

impl RedactingSink {
    fn record(&self, buffer: &[u8]) -> String {
        // Dropping an oversized event's payload avoids both a truncated secret
        // prefix and an invalid JSON object. The fixed notice has no user data.
        if buffer.len() > MAX_LINE_BYTES {
            return match self.destination {
                Destination::File(_) => format!(
                    "{{\"level\":\"WARN\",\"message\":\"log event omitted: exceeds record limit\",\"omitted_bytes\":{}}}\n",
                    buffer.len()
                ),
                Destination::Stderr => format!(
                    "WARN log event omitted: exceeds record limit ({} bytes)\n",
                    buffer.len()
                ),
            };
        }
        let text = String::from_utf8_lossy(buffer);
        let redacted = self
            .redactor
            .redact_line(text.trim_end_matches(['\r', '\n']));
        // Redaction can expand small credential values. Preserve the JSON
        // boundary even then, instead of emitting a cut object.
        if redacted.len() >= MAX_LINE_BYTES || redacted.contains("...[truncated ") {
            return match self.destination {
                Destination::File(_) =>
                    "{\"level\":\"WARN\",\"message\":\"log event omitted: redacted record exceeds limit\"}\n".into(),
                Destination::Stderr =>
                    "WARN log event omitted: redacted record exceeds limit\n".into(),
            };
        }
        if matches!(self.destination, Destination::File(_))
            && !serde_json::from_str::<serde_json::Value>(&redacted)
                .is_ok_and(|record| record.is_object())
        {
            return "{\"level\":\"WARN\",\"message\":\"log event omitted: redaction changed JSON structure\"}\n".into();
        }
        format!("{redacted}\n")
    }
}

impl Write for RedactingSink {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let record = self.record(buffer);
        match &self.destination {
            Destination::File(file) => {
                let mut guard = file
                    .lock()
                    .map_err(|_| io::Error::other("log sink lock poisoned"))?;
                let was_failed = guard.failed;
                if let Err(error) = guard.write_record(record.as_bytes()) {
                    if !was_failed {
                        // No error, path, event, or original payload is printed:
                        // formatter error fallbacks must not bypass redaction.
                        let _ = io::stderr().write_all(
                            b"JARVIS structured logging stopped after an I/O failure; run jarvis doctor.\n",
                        );
                    }
                    return Err(error);
                }
            }
            Destination::Stderr => io::stderr().lock().write_all(record.as_bytes())?,
        }
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        match &self.destination {
            Destination::File(file) => file
                .lock()
                .map_err(|_| io::Error::other("log sink lock poisoned"))?
                .flush(),
            Destination::Stderr => io::stderr().flush(),
        }
    }
}

impl<'writer> fmt::MakeWriter<'writer> for RedactingSink {
    type Writer = Self;

    fn make_writer(&'writer self) -> Self::Writer {
        self.clone()
    }
}

/// Owns the bounded log sink and the redactor applied to both output renderings.
#[derive(Debug)]
pub struct Logging {
    path: PathBuf,
    sink: RedactingSink,
}

impl Logging {
    /// Installs JSON file and human stderr layers with shared redaction rules.
    ///
    /// # Errors
    ///
    /// Returns an error for inaccessible/oversized existing logs, invalid level,
    /// or an already installed global subscriber.
    pub fn init(
        log_directory: &Path,
        level: LogLevel,
        redactor: Redactor,
    ) -> Result<Self, LoggingError> {
        fs::create_dir_all(log_directory).map_err(|source| LoggingError::Directory { source })?;
        let path = log_directory.join(LOG_FILE_NAME);
        let file = RotatingFile::open(path.clone(), MAX_LOG_FILE_BYTES, RETAINED_LOG_FILES)
            .map_err(|source| LoggingError::File { source })?;
        let sink = RedactingSink {
            destination: Destination::File(Arc::new(Mutex::new(file))),
            redactor: redactor.clone(),
        };
        let stderr_sink = RedactingSink {
            destination: Destination::Stderr,
            redactor,
        };
        let filter = EnvFilter::try_new(level.as_str()).map_err(|_| LoggingError::InvalidLevel)?;
        let file_layer = fmt::layer()
            .json()
            .with_writer(sink.clone())
            .with_ansi(false)
            .log_internal_errors(false);
        let stderr_layer = fmt::layer()
            .with_writer(stderr_sink)
            .with_ansi(false)
            .log_internal_errors(false);

        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .try_init()
            .map_err(|_| LoggingError::AlreadyInstalled)?;

        Ok(Self { path, sink })
    }

    /// Returns the current log file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reports how many known secrets the sinks mask.
    #[must_use]
    pub fn secret_count(&self) -> usize {
        self.sink.redactor.secret_count()
    }

    /// Redacts a line exactly as the shared redactor does.
    #[must_use]
    pub fn redact(&self, line: &str) -> String {
        self.sink.redactor.redact_line(line)
    }

    /// Flushes recorded events to the operating system.
    ///
    /// # Errors
    ///
    /// Returns an error if the sink previously failed or cannot flush.
    pub fn flush(&self) -> Result<(), LoggingError> {
        self.sink
            .clone()
            .flush()
            .map_err(|source| LoggingError::File { source })
    }
}

/// One complete UTF-8 line of the structured log, already redacted on disk.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogLine {
    /// One-based position in this returned tail, not an absolute file line.
    ///
    /// Absolute positions require scanning the whole file and are intentionally
    /// not calculated. This ordinal resets on every read.
    pub number: u64,
    /// Raw JSON text without its trailing newline.
    pub text: String,
}

/// Reads recent complete records in bounded byte windows, without a full count.
///
/// At most 4 MiB is scanned and 5,000 records returned. Unterminated final records,
/// blank lines, records larger than 8 KiB, and an incomplete leading fragment at
/// the scan boundary are omitted. UTF-8 is decoded only after rejoining a record.
/// A missing log returns an empty result.
///
/// # Errors
///
/// Returns an error for unreadable files or invalid UTF-8 in a complete record.
pub fn read_tail(path: &Path, lines: usize) -> Result<Vec<LogLine>, LoggingError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(LoggingError::File { source }),
    };
    read_tail_from(&mut file, lines).map_err(|source| LoggingError::File { source })
}

fn read_tail_from<R: Read + Seek>(reader: &mut R, lines: usize) -> io::Result<Vec<LogLine>> {
    let bounded = lines.clamp(1, MAX_TAIL_LINES);
    let mut end = reader.seek(SeekFrom::End(0))?;
    let minimum = end.saturating_sub(MAX_TAIL_BYTES);
    let mut reversed = Vec::new();
    let mut fragment = Vec::new();
    let mut oversized = false;
    let mut complete = false;
    let mut chunk = vec![0_u8; READ_WINDOW_BYTES];
    while end > minimum && reversed.len() < bounded {
        let start = end.saturating_sub(READ_WINDOW_BYTES as u64).max(minimum);
        let length = usize::try_from(end - start).unwrap_or(READ_WINDOW_BYTES);
        reader.seek(SeekFrom::Start(start))?;
        reader.read_exact(&mut chunk[..length])?;
        for byte in chunk[..length].iter().rev() {
            if *byte == b'\n' {
                if complete && !oversized {
                    keep_record(&mut fragment, &mut reversed)?;
                }
                fragment.clear();
                oversized = false;
                complete = true;
                if reversed.len() == bounded {
                    break;
                }
            } else if complete && !oversized {
                if fragment.len() == MAX_LINE_BYTES {
                    fragment.clear();
                    oversized = true;
                } else {
                    fragment.push(*byte);
                }
            }
        }
        end = start;
    }
    if end == 0 && reversed.len() < bounded && complete && !oversized {
        keep_record(&mut fragment, &mut reversed)?;
    }
    Ok(reversed
        .into_iter()
        .rev()
        .enumerate()
        .map(|(index, text)| LogLine {
            number: u64::try_from(index).unwrap_or(0) + 1,
            text,
        })
        .collect())
}

fn keep_record(fragment: &mut [u8], records: &mut Vec<String>) -> io::Result<()> {
    fragment.reverse();
    let text = std::str::from_utf8(fragment)
        .map_err(|_| io::Error::other("complete log record is not UTF-8"))?
        .trim_end_matches('\r');
    if !text.trim().is_empty() {
        records.push(text.to_owned());
    }
    Ok(())
}

/// Sizes of the active log and its managed retained segments.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LogStorageStats {
    /// Bytes in the active log.
    pub active_bytes: u64,
    /// Sum of bytes in the active and retained logs.
    pub total_bytes: u64,
    /// Whether any segment exceeds the configured file size bound.
    pub over_limit: bool,
}

/// Inspects only metadata for the fixed set of managed log filenames.
///
/// # Errors
///
/// Returns an error when metadata is inaccessible or a segment is not a file.
pub fn log_storage_stats(path: &Path) -> Result<LogStorageStats, LoggingError> {
    let mut stats = LogStorageStats::default();
    for index in 0..=RETAINED_LOG_FILES {
        let segment = if index == 0 {
            path.to_path_buf()
        } else {
            segment_path(path, index)
        };
        let bytes = segment_size(&segment).map_err(|source| LoggingError::File { source })?;
        if index == 0 {
            stats.active_bytes = bytes;
        }
        stats.total_bytes = stats.total_bytes.saturating_add(bytes);
        stats.over_limit |= bytes > MAX_LOG_FILE_BYTES;
    }
    Ok(stats)
}

/// Counts lines only in logs that fit the managed segment bound.
///
/// Explicit counting is separate from the tail/doctor paths. The captured file
/// length bounds the read even if another process appends concurrently.
///
/// # Errors
///
/// Returns an error for unreadable files or files larger than 4 MiB.
#[allow(clippy::naive_bytecount)] // Bounded, explicit diagnostic; no new SIMD dependency needed.
pub fn count_lines(path: &Path) -> Result<u64, LoggingError> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => return Err(LoggingError::File { source }),
    };
    let bytes = file
        .metadata()
        .map_err(|source| LoggingError::File { source })?
        .len();
    if bytes > MAX_LOG_FILE_BYTES {
        return Err(LoggingError::File {
            source: io::Error::other("log exceeds bounded line-count limit"),
        });
    }
    let mut reader = file.take(bytes);
    let mut buffer = vec![0; READ_WINDOW_BYTES];
    let mut count = 0;
    let mut last = b'\n';
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|source| LoggingError::File { source })?;
        if read == 0 {
            break;
        }
        count += buffer[..read].iter().filter(|byte| **byte == b'\n').count() as u64;
        last = buffer[read - 1];
    }
    Ok(count + u64::from(last != b'\n'))
}

#[cfg(test)]
mod tests;
