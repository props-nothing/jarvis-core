# Observability And Diagnostics

Observability explains JARVIS without creating a second store of private content.

## Signals

### Logs

Structured events for lifecycle, normalized state changes, failures, and operator actions. Fields use stable names and IDs. Human console rendering is derived from the same event, not separately formatted logic.

The current daemon writes JSON Lines to `<logs>/jarvisd.jsonl` and plain text to
stderr. Both renderings pass through the same redaction rules before output.
Every emitted record, including its newline, fits within 8 KiB. An oversized
event is replaced by a content-free omission notice (valid JSON in the file),
with the omitted byte count; the daemon does not cut a JSON object or a secret in
half. Redaction happens before truncation in the reusable text redactor.
Known values are matched in raw and JSON-escaped form, and quoted credential
fields are masked through escaped quotes. The sink validates the rewritten JSON;
if structural masking changes its syntax (for example, a numeric password
field), it emits a fixed omission object with no original field content.

The active log and each of three retained segments are at most 4 MiB: 16 MiB total
for the four managed files. Rotation happens before a record would exceed the
limit; `.1` is newest and `.3` oldest. Rotation drops the oldest segment. A crash
fragment is rotated before appending a new event. New Unix files use mode `0600`;
Windows files inherit the managed log directory's private permissions. The daemon
singleton is the only supported writer; do not externally truncate or rotate its
files while it is running.

An I/O or rotation failure disables further file appends until restart, emits
one fixed content-free stderr message, and leaves the independently redacted
console layer available. Existing oversized files are preserved and refused at
startup. Stop the daemon and move those files outside the managed log directory
for archival before restarting; `jarvis doctor` identifies `logs.over_limit`.
Logging is diagnostic evidence, not the durable audit ledger.

`jarvis logs --lines N` reads the active file only, with a 5,000-record and 4 MiB
scan ceiling. It joins raw bytes across 64 KiB windows before UTF-8 decoding,
omits incomplete final records, blank lines, and oversized legacy records, and
never scans the whole file merely to count earlier lines. The internal
`LogLine.number` is now a one-based ordinal within the returned tail; the CLI
continues to print JSON records without line numbers. Retained files remain
available for explicit offline inspection. See [ADR-0226](../adr/0226-logs-share-redaction-and-bounded-retention.md).

### Metrics

Target signals: counts, latency histograms, queue depth, failures, retries, saturation, usage, and cost. This is not a claim that a metrics exporter is shipped. Metrics never contain user text, email addresses, phone numbers, filenames, URLs with credentials, or unbounded IDs.

### Traces

Tracing spans connect instrumented operations. A distributed trace exporter and temporary sensitive-tracing mode are future work; any such mode must have a warning and retention controls. Routine events remain content-free by default.

### Audit

Durable security/business decision receipts. Audit is not debug logging and does not store full prompts/tool payloads simply because they are available.

## Correlation

Carry where applicable:

```text
request_id
correlation_id
trace_id / span_id
session_id
run_id
workflow_run_id / step_id / attempt_id
model_call_id
tool_call_id
approval_id
event_id
voice_session_id / call_id
runtime_instance_id
```

Provider request/call IDs are recorded as external references and redacted if provider format embeds sensitive data.

## Core Metrics

- daemon start/readiness/shutdown and uptime
- active/queued/failed/cancelled runs
- state-transition and resume counts
- model time to first token, duration, tokens, cost, errors, retries, fallback
- tool wait/execute duration, approval latency, outcome and unknown-effect count
- connector auth health, request latency, rate limits, webhook lag/replay
- event inbox/outbox depth, age, attempts, dead letters
- workflow state, timer lag, lease expiry, retries, completion
- memory candidates admitted/rejected, retrieval latency, result mix, correction/deletion
- runtime health, crashes, restarts, protocol errors, resource use
- voice time to first audible response, interruption latency, call status/cost/failures
- database latency, lock contention, pool saturation, migration/backup health

Avoid high-cardinality labels such as raw user, session, run, tool argument, URL, or error text. Use bounded categories and investigate individual IDs through access-controlled logs/audit.

## Redaction

Redaction is defense in depth:

1. Do not construct log fields from secret-bearing objects.
2. Keep secrets out of URLs; inject headers at the final adapter.
3. Mark sensitive wrappers as non-display/non-serialize.
4. Apply structural redaction for authorization headers, cookies, JSON fields, URL userinfo/path/query patterns, and known live secret values.
5. Test canaries across errors, traces, diagnostics, and panic/crash output.

The implemented sink masks known secret values, credential-shaped key/value text,
Bearer tokens, and URL-userinfo passwords. This is defense in depth, not a promise
to recognize every possible credential format. Reusable text truncation is UTF-8
safe and reports omitted byte counts within its size ceiling; structured logging
uses an omission record instead of truncating JSON. Tool/provider output should
remain bounded before logging.

## Health

Liveness says the process can respond. Readiness says it can safely accept the relevant class of work. Optional adapter failure does not necessarily make the whole daemon unready; capability health is reported separately.

The Phase 1 daemon has four monotonic lifecycle phases:

| Phase | Live | Ready | Meaning |
| --- | --- | --- | --- |
| `booting` | yes | no | paths, singleton ownership, configuration, and storage are being established |
| `ready` | yes | yes | bootstrap succeeded and local clients may be served |
| `stopping` | yes | no | new work is refused while owned resources settle |
| `stopped` | no | no | storage is closed and the process is releasing singleton ownership |

Each process lifecycle has a UUIDv7 `DaemonRunId` in `daemon_instances` with
bounded build identity and timestamps. This record is diagnostic evidence, not
the singleton mechanism.

Health components include storage, migrations, worker loop, event backlog, secret store, runtime instances, models, connectors, MCP servers, voice providers, and update state. Public health endpoints expose minimal status; detailed evidence requires operator scope.

## Doctor Versus Health

- **Health:** current runtime snapshot, cheap and non-mutating.
- **Status:** combines service installation, process, protocol, profile, and selected capability summaries.
- **Doctor:** deeper checks, offline inspection, drift detection, migrations, optional explicit repairs, and post-repair proof.

Every recurring production failure should lead to a stable doctor finding or targeted diagnostic when feasible.

### Implemented Doctor Checks (P1-010)

`jarvis doctor` runs offline and mutates nothing unless `--repair` is passed. Each
finding carries a stable ID, severity, bounded facts, and a specific remediation.
Report outcome maps to an exit code: `0` passed, `6` warnings, `7` failed, `8` a
repair did not verify.

| Check | Detects |
| --- | --- |
| `configuration` | parse errors, unknown keys, a newer config schema |
| `paths` | missing managed directories, permissions not private to the current user |
| `database` | absent, current, pending migration, newer schema, inconsistent markers, foreign file, corruption, integrity failure |
| `credential` | missing or unusable profile credential (value never read out) |
| `daemon` | singleton lock ownership, probed without stealing the lock |
| `protocol` | this build's supported protocol window |
| `redaction` | a canary pushed through the real redactor |
| `logs` | metadata for four managed filenames enforces the 4 MiB segment bound; a bounded sample verifies a readable JSON object. `logs.over_limit` and `logs.unreadable` provide explicit archival/permission remediation without modifying evidence. |

A database is inspected read-only with creation disabled, because opening it for
real would migrate, create, and enable WAL. A newer schema is reported as a finding
rather than an error: explaining that state is the purpose. Only idempotent,
JARVIS-owned directory permissions are ever repaired, and the repair re-runs the
same check that failed.

The per-profile `jarvisd.lock` file is retained after exit. Kernel lock
ownership, not file existence or its diagnostic PID text, determines whether an
instance is active. Retention avoids splitting the lock domain by unlinking a
locked Unix inode.

## Diagnostics Bundles

Before export, show the file/category manifest. Default bundles include versions, platform, config schema with secret refs only, health/findings, bounded recent structural logs, migration/update/service status, and optional trace IDs. Exclude conversations, prompts, tool content, email/calendar data, memories, transcripts, recordings, raw environment, and secret values.

Users explicitly opt in to additional content and see warnings/retention instructions.

## Telemetry

Local mode has no JARVIS-hosted product telemetry by default. If optional telemetry is introduced, it requires an ADR, explicit consent, documented schema/destination/retention, a disable switch, and a local preview. Provider SDK telemetry/tracing defaults are reviewed and disabled unless selected intentionally.

## Alerts And Objectives

Before server stable, define objectives and alerts for availability, run failure, event/workflow lag, unknown external effects, dead letters, connector auth loss, approval backlog, provider spend, backup age/failure, update rollback, and security/audit anomalies.

Alerts link to a runbook and avoid sending sensitive payloads to notification systems.
