# ADR-0227: Transactional memory corrections commit one versioned change

- Status: Accepted
- Date: 2026-10-16
- Scope: canonical SQLite memory correction
- Related: [memory lifecycle](0050-forgetting-is-a-purge-and-the-receipt-names-what-it-cannot-reach.md), [memory discovery](0223-memory-discovery-browses-before-guessing-and-reads-by-id.md)

## Context

Correction previously inserted the new claim and its entity links before attempting to archive the original. Correcting an already-superseded record with its current version could therefore return an error while leaving another active claim. Concurrent corrections and an interrupted write had the same partial-commit risk.

## Decision

The storage adapter performs correction in one SQLite `BEGIN IMMEDIATE` transaction. It reads the original in the replacement's workspace, rechecks the inspected version and supersession state, validates the domain transition, checks the replacement's tombstone and duplicate key, writes the replacement and entity links, and archives/links the original. The guarded update and every dependent insert commit together; any refusal or write failure rolls them all back.

A stale version or an already-superseded original returns a conflict. Repeating a completed request does not add a new record. The caller must inspect and correct the current replacement. Original content remains archived for the correction trail. This changes no source-trust rule, approval requirement, provider contract or database schema. Ordinary admission, forgetting and summary transactions keep their existing contracts.

## Verification

Real SQLite tests exercise repeated and already-superseded corrections, concurrent competing corrections, and a trigger that deliberately fails the final supersession write. The latter proves that a failed update leaves neither a replacement nor its entity links. A daemon service regression drives a successful correction followed by a second correction of the archived original using its current version and checks that only the two legitimate records exist.

These are offline correctness checks. They do not establish new model-quality or provider acceptance evidence.
