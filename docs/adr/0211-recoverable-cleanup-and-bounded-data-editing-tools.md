# ADR-0211: Recoverable cleanup and bounded data editing tools

**Status:** Accepted

**Date:** 2026-10-14

## Context

The owner requested file/contact/email deletion, reliable browser navigation and fixed-text search,
bulk CSV contact import, and editing existing XLSX workbooks. These are acceptance gaps in existing
capabilities, delivered as the owner-selected next slice rather than unrelated roadmap expansion.

## Decision

- Rust remains the authority. Every new tool uses the existing registry, schemas, scopes, risk policy,
  approval binding, deadline and durable outcome/audit pipeline. No model gains an approval tool or
  unrestricted browser JavaScript execution.
- File deletion means moving one regular file to `.jarvis-trash/<random-id>/<name>` within the same
  granted root, using capability-relative operations. Return a recovery path. This is not OS trash;
  there is no recursive deletion or purge. Recovery uses the existing non-overwriting move tool.
- Gmail deletion means the documented single-message trash operation, never hard delete or thread
  deletion. The owner chose to include restricted `gmail.modify` in optional Google actions consent;
  existing sign-ins must reauthorize before trash is usable. Lost provider confirmation is unknown,
  not proof that nothing happened. No automatic retry follows an ambiguous effect.
- Deletion asks by default. The owner explicitly chose to allow existing standing-trust settings,
  as for editing tools; this slice does not change approval policy or add steps to answering.
- Contact deletion remains workspace-scoped and cannot erase an owner's `do_not_contact` protection
  through the model path. CSV import reads a bounded granted file and validates the complete batch
  before committing; deduplication and suppression rules remain those of the canonical contacts store.
- Browser navigation and literal text search use the existing browser adapter and read/act posture.
  Website content remains untrusted, with bounded results and explicit failure/no-match outcomes.
- XLSX editing updates selected cells in the existing package, preserving untouched parts rather than
  reconstructing the workbook from extracted values. Validate the full edit before writing and replace
  the bounded file through a unique create-new sibling and atomic rename inside its granted root.

## Consequences

Scratch artifacts can be removed from active folders without silently losing recovery options; trash
still consumes disk until the owner clears it manually. Google actions consent is broader, and Settings
must say so. Bulk import cannot partially apply invalid input. Complex workbooks that cannot be updated
without compromising their structure fail closed rather than silently losing formulas or formatting.

No schema migration, new process, canonical provider-owned state, or new approval surface is introduced.
Provider and platform live evidence remains distinct from offline regression tests in the backlog.
