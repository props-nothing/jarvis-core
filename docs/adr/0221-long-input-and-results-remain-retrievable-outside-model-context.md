# ADR-0221: Long input and results remain retrievable outside model context

- Status: Accepted
- Date: 2026-10-15
- Selected by: Owner request to fix chat ceilings and inaccessible truncated text

## Context

The console admitted 4,000 characters while storage admitted 4,096. Native file reads returned only a prefix, delegated answers silently lost everything after 3,000 characters, and command readers discarded the middle of their output. Enlarging model context alone cannot repair discarded source text.

## Decision

1. Keep bounded admission and model requests, but separate retained source text from inline previews. Accept objectives up to 200,000 Unicode characters with an 8 MiB HTTP body ceiling. Store the full objective in a new schema-28 column; the old column remains a compatibility preview. Existing migrations are unchanged. Repository reads prefer the full objective, including restart recovery.
2. The console uses a multiline textarea without a browser typing ceiling or a send-time slice. CLI validation shares the storage bound. A long objective has a marked 4,000-character initial preview instructing the model to fetch the remainder before acting.
3. `jarvis.context.read` reads the current run's stored input, discovers session history by sequence, or pages a retained message/tool result by character offset. The requesting durable call identifies the run; messages and results must belong to the same conversation. Its distinct `context.read` scope does not restore sub-agent delegation authority. External text remains fenced; retention does not confer authority.
4. Native file reads return JSON pages with byte offsets and `next_offset`. UTF-8 boundaries are preserved; malformed text is refused. Each serialized page fits the existing model-facing budget. EOF is explicit, so a read is not limited to a prefix or line count. Files are read live, not as immutable snapshots.
5. Sub-agent result pages use character offsets over the stored answer. `total_chars`, `truncated` and `next_offset` make omitted text visible and recoverable without rerunning the task. Native delegated tasks admit 50,000 characters. ACP coding task admission is unchanged.
6. Command stdout/stderr retain up to 16 MiB each, then save sanitized text in unique `.jarvis-output` files under a granted root. Results expose file paths and `capture_complete`; inline previews shrink before JSON escaping can hide recovery metadata. Failure, timeout, capture overflow or inability to save is not presented as complete capture. Files remain until the owner removes them; no automatic deletion is introduced.

## Consequences

Model context windows, timeouts, spend controls, confinement and finite capture/admission bounds remain. Automatic history replay and observation masking still limit request size, but older stored messages and results are retrievable. This is not an unlimited-memory model or proof that a real model follows every continuation instruction.

Generic tool retention remains bounded at 32 KiB. `source_truncated` reports that bound; the context reader cannot reconstruct source text an external adapter or provider never retained. Document extraction ceilings and external agents' own limits are unchanged. Provider pagination or source-specific retrieval is still needed for those paths. Capture files contain local command output and should be treated as workspace data, not committed automatically.

## Verification

Focused regressions reconstruct a multilingual Markdown file byte-for-byte, retrieve a long delegated answer without rerunning it, recover the tail of an 80,000-character request, reject another conversation's message, traverse history beyond 40 messages, recover the middle of a 6,000-line command output, preserve capture metadata under JSON escaping and upgrade a schema-27 database without breaking existing run relationships. Console tests preserve a long multiline draft and distinguish Enter, Shift+Enter and IME composition. Native and live-model acceptance remain separate from deterministic tests.