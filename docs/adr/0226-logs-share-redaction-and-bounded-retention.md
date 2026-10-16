# ADR-0226: Logs share redaction and bounded retention

- Status: accepted
- Date: 2026-10-16

## Context

The review found that stderr bypassed the file redactor, backwards log reads
could splice records at 64 KiB boundaries, and both tail and doctor performed a
full-file line count. The file grew indefinitely. Truncating formatted JSON also
produced malformed records, and truncating text before masking a known secret
could expose a credential prefix.

## Decision

1. Both tracing renderings use the same sink redaction rules. Human output is
   plain text; JSON output remains JSON Lines. Events beyond 8 KiB, including
   framing, become fixed content-free omission records with byte counts. The
   reusable text redactor masks before UTF-8-safe truncation, matches known
   values in raw and JSON-escaped form, and respects escaped quotes in credential
   fields. Rewritten JSON is validated; structural damage (such as masking a
   numeric password) produces a fixed omission object. Formatter internal
   error output is disabled to keep fallback output inside the JARVIS boundary.
2. The active file and three retained segments each have a 4 MiB ceiling, for
   16 MiB total. Before an append would cross that bound, rotation drops `.3`,
   shifts the other archives and starts a new active file. A partial final event
   from a previous process is retained in an archive rather than joined to a
   new event. Unix files are created with `0600`; Windows files inherit the
   managed directory's private ACL. The existing daemon singleton remains the
   single-writer authority and is acquired before logging initialization can
   rotate or open the active file. Rejected duplicate startup preserves the
   owner's log bytes, including an unfinished record. No SDK, background thread
   or service is added.
3. Write/rotation failures stop further file appends until restart and emit one
   fixed content-free stderr notice. Independent redacted stderr events remain
   available. Existing oversized logs are preserved and block logging startup
   until the owner archives them outside the managed paths; initialization never
   silently deletes oversized historical evidence.
4. Tail reads stop after 5,000 records or 4 MiB scanned, whichever comes first.
   Bytes are joined before UTF-8 decoding. Unterminated final records, blanks,
   oversized records and an incomplete head at the scan ceiling are omitted.
   Invalid UTF-8 in a complete bounded record is an error. Tail reads only the
   active segment. `LogLine.number` becomes a one-based ordinal in the returned
   tail, explicitly replacing an absolute line count; CLI output has never shown
   that field and continues to print only JSON.
5. Doctor inspects metadata for the four managed paths and a bounded JSON sample.
   `logs.over_limit` and `logs.unreadable` carry specific remediation without
   changing evidence. Exact line counting is an explicit separate helper,
   limited to one bounded segment, outside the tail and doctor paths.

## Consequences

Old diagnostic records expire under the size policy; canonical audit receipts
retain their separate storage semantics. Oversized event payloads are discarded,
not retrievable from logging. Failures may leave fewer archived segments because
rotation is a sequence of filesystem operations, not a multi-file transaction;
the active file is retained until archive preparation succeeds. File mutation by
another writer/rotator is unsupported. Native CI remains necessary for platform
permissions and close/rename/reopen behavior.

## Verification

Child-process tests capture real stderr and JSON output with known-secret and
credential-shaped canaries, escaped strings, numeric fields, long events, span fields, and a forced rotation
failure. The failure fixture proves the active file cannot continue growing and
the console cannot fall back to a raw event. Other tests compare hundreds of
exact UTF-8 records across many read windows, measure byte reads, reject invalid
UTF-8, skip partial/oversized records, verify repeated rotation and restart,
preserve legacy oversized files, and confirm doctor does not modify evidence.

See [official source research](../research/integrations/rust-structured-logging.md)
and [operator behavior](../operations/observability.md).
