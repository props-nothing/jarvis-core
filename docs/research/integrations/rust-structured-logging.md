---
integration: rust-structured-logging
status: implemented
last_verified: 2026-10-16
owners: []
selected_spec_version: tracing 0.1.44 / tracing-subscriber 0.3.23
selected_sdk: tracing-subscriber 0.3.23
---

# Rust Structured Logging, Correlation, And Redaction

## Scope

The `tracing` ecosystem used for the daemon's structured logs: crate and feature selection, JSON file output, human console output derived from the same event, the filter, the redaction sink, and the bounded log reader behind `jarvis logs`. Metrics, traces, OpenTelemetry export, and audit receipts are out of scope.

## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| `llms.txt` or docs index | not found for the tracing crates | 2026-09-21 | discovery |
| `tracing` crate docs | https://docs.rs/tracing/0.1.44/tracing/ | 2026-09-21 | event/span API, feature flags |
| `tracing-subscriber` crate docs | https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/ | 2026-09-21 | layers, `fmt`, JSON, feature flags |
| `cargo search` | tracing 0.1.44, tracing-subscriber 0.3.23, tracing-appender 0.2.5 | 2026-09-21 | current published versions |
| changelog/release notes | `Cargo.lock` resolved versions | 2026-09-21 | compatibility |

No `llms.txt` exists for these crates. The normative contract is the official rustdoc for the selected version.

## Verified Contract

### 2026-10-16 writer and retention repair

Live official checks before implementation: repository `llms.txt` at
https://raw.githubusercontent.com/tokio-rs/tracing/master/llms.txt returned 404;
https://docs.rs/tracing-subscriber/0.3.23/tracing_subscriber/fmt/writer/trait.MakeWriter.html,
https://raw.githubusercontent.com/tokio-rs/tracing/tracing-subscriber-0.3.23/tracing-subscriber/src/fmt/fmt_layer.rs,
https://github.com/tokio-rs/tracing/blob/tracing-subscriber-0.3.23/tracing-subscriber/CHANGELOG.md,
and https://doc.rust-lang.org/std/fs/fn.rename.html were read on 2026-10-16 UTC.
The pinned formatter constructs an event buffer then calls `write_all` on the
selected writer. Internal error reporting can write directly to stderr and is
disabled explicitly in both JARVIS layers; our sink reports a fixed content-free
failure message instead. `MakeWriter` can return an independently cloned writer
sharing synchronized state. Rust file rename has platform-dependent replacement
semantics; rotation closes the active handle and removes an existing destination
before rename. No dependency version or feature changes are needed.

https://docs.rs/serde_json/1.0.151/serde_json/fn.from_str.html and
https://docs.rs/serde_json/1.0.151/serde_json/fn.to_string.html were also read on
2026-10-16 UTC. The existing workspace JSON parser validates the final bounded
file record: structural text masking can otherwise replace a numeric credential
field with an unquoted placeholder. Invalid rewritten JSON becomes a fixed
content-free omission object instead of an invalid JSON Lines record.
Known secrets retain their JSON-escaped spelling as a match form so quoted or
backslash-bearing credentials cannot bypass the sink when formatted as JSON.

This is local file/stderr I/O: no authentication, scopes, network quotas, remote
retention, or webhook contract. JARVIS owns record/segment/scan limits and serialized
rotation; it retains no SDK types outside this adapter. Rotation errors refuse
further file growth, while the independent redacted stderr layer remains available.
Concurrent external mutation of the log files is unsupported; native platform
checks are still required for filesystem behavior. The selected changelog reports
ANSI sanitization changes, with no relevant writer API change observed.

### Crate And Feature Selection

- `tracing = "=0.1.44"` was already resolved transitively through SQLx, so declaring it directly pinned the existing version rather than adding a second one.
- `tracing-subscriber = "=0.3.23"` with `default-features = false` and features `ansi`, `env-filter`, `fmt`, `json`, `registry`, `std`.
- `tracing-appender` (0.2.5) is deliberately **not** used: bounded synchronous rotation is owned by JARVIS and needs no background writer or additional package. The existing workspace `serde_json` dependency validates bounded rewritten JSON and emitted test records.

### Documented Feature-Flag Behaviour

From the 0.3.23 rustdoc, `default-features = false` removes `ansi`. `fmt::layer().with_ansi(true)` is a **runtime assertion**, not a compile-time one, so omitting the feature compiles cleanly and panics when the layer is first built:

```text
tracing-subscriber: the `ansi` crate feature is required to enable ANSI terminal colors
```

This was observed live before being fixed. The repaired console layer uses
`with_ansi(false)` so credential-shaped field matching receives plain text.
The existing dependency feature set is unchanged; ANSI is no longer required by
this layer's current rendering.

### Output Shape

- File: one JSON object per line (`fmt::layer().json().with_ansi(false)`); over-budget events become a fixed JSON omission notice rather than a truncated object.
- Console: `fmt::layer().with_ansi(false)` to stderr.
- Both layers use the same redactor implementation. The 2026-10-16 review found the old stderr writer bypassed this boundary; child-process tests now exercise both real layers rather than only the file writer.
- Filter: `EnvFilter::try_new(level.as_str())` from the validated `jarvis_core::LogLevel`.

### Limits And Failure Semantics

- `MAX_LINE_BYTES = 8 KiB` including the emitted newline; oversized event payloads are omitted with a byte count. Reusable text redaction masks before UTF-8-safe truncation and includes the notice in the byte ceiling.
- `MAX_TAIL_LINES = 5_000`, `DEFAULT_TAIL_LINES = 200`.
- `MAX_LOG_FILE_BYTES = 4 MiB`; three rotated files plus active file give a 16 MiB retention ceiling. Rotation precedes appending a record which would exceed the ceiling. New Unix files use mode `0600`; Windows files inherit the private parent directory ACL. The daemon singleton owns writing.
- Rotation and writes are serialized under one mutex. A failure disables future appends until restart; no unredacted error payload is sent to stderr. The independent stderr layer remains available. Oversized pre-existing logs are preserved and refused until explicitly archived by the operator.
- `read_tail` scans at most 4 MiB backwards in 64 KiB windows and rejoins bytes before UTF-8 decoding. It omits blank, oversized and incomplete records. It no longer counts the whole file: `LogLine.number` is a returned-tail ordinal; CLI output remains JSON without numbers. Invalid UTF-8 in a complete record returns an error.
- Doctor reads metadata for the fixed four filenames and a bounded recent sample, with explicit `logs.over_limit`/`logs.unreadable` findings. The separate `count_lines` helper refuses files beyond 4 MiB and is not used by tail or doctor.
- A missing log is an empty result, not an error; `jarvis logs` exits 0 with an explanatory message.

## JARVIS Mapping

- `jarvis_core::LogLevel` is the single definition of the level vocabulary; `jarvis-storage` (config) and `jarvis-observability` (logging) both reference it instead of the adapter crates depending on each other.
- `jarvis_observability::Redactor` masks known secrets, credential-shaped key/value and query text, `Bearer` values, and URL userinfo passwords.
- The daemon builds the redactor from the profile credential before logging starts, so the credential is masked from the first recorded line.

## Decisions

- Redaction is applied in the **writer**, not at call sites, so a forgotten rule cannot leak. A call site can still avoid constructing a sensitive field, which remains the stronger layer.
- Two layers (JSON file, human console) share one redactor rather than formatting separately.
- `jarvis logs` reads the file directly instead of asking the daemon, because logs must remain diagnosable precisely when the daemon cannot start.
- `LogLevel` moved to `jarvis-core`; keeping it in `jarvis-storage` would force the logging adapter to depend on the storage adapter for a five-variant enum.

## Rejected Alternatives

- `tracing-appender` non-blocking writer: adds a background thread and shutdown ordering for no current requirement.
- One formatter for both sinks: the operator wants human output and the tool wants JSON, and deriving both from the same event satisfies the observability rule.
- Redacting only known secret values: a redactor that handles only the credential passes a naive test while leaving API keys in query strings and passwords in DSNs intact. Each class has its own test.

## Evidence Of Implementation

- `crates/jarvis-observability/src/redact.rs` (secret classes, over-matching, strict byte bounds, and a known secret straddling the truncation boundary)
- `crates/jarvis-observability/src/logging/tests.rs` (both real sinks in child processes, JSON validity, omission bounds, rotation/retention/reopen, preserved crash fragments, fail-closed rotation, oversized legacy preservation, and instrumented tail I/O over many UTF-8 windows)
- `crates/jarvis-diagnostics/src/doctor.rs` (healthy, oversized retained-segment, malformed/partial/invalid UTF-8 logs; non-mutation assertions)
- `crates/jarvis-core/src/loglevel.rs`
- `apps/jarvisd/src/main.rs` (startup/shutdown events, redactor built from the credential)
- `apps/jarvis-cli/src/main.rs` (`jarvis logs`, `--lines`)
- Manual verification: `jarvis logs` returned the structured record; the 64-character credential did not appear anywhere in the log file.

## Unresolved Ambiguities

- Retention is fixed-size local retention, not time-based archival, compression, or a durable audit facility. External concurrent file mutation is unsupported. Native platform CI must verify Unix permissions and Windows/macOS filesystem behavior.
- The graceful-shutdown `daemon stopped` event is implemented and the signal handlers are unit-tested, but a console close event could not be delivered in the automated environment, so that specific line was not observed end to end.
- Metrics/trace export remains future work. Canonical audit receipts are implemented in the storage/application layers and are separate from this logging adapter.
