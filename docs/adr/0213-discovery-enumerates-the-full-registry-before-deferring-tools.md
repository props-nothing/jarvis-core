# ADR-0213: Discovery enumerates the full registry before deferring tools

**Status:** Accepted

**Date:** 2026-10-14

## Context

The normal owner profile has 83 callable tools. The bounded registry discovery summary stops at 64
identifiers. Native model offers and `jarvis.tools.search` both incorrectly used this summary as their
complete catalog. The first six browser tools appeared, but navigate, snapshot, fill, typing,
evaluate and screenshots were after the cutoff. The browser server itself offered all 25 tools.

This violates [ADR-0176](0176-a-big-tool-catalog-costs-no-tokens-until-a-tool-is-needed.md): searching
for a group must make the complete group available, not whichever prefix fit an unrelated summary.

## Decision

Discovery and native model offers enumerate the complete registered, callable set (bounded at 512
by the existing registry). Existing group deferral still limits routine prompt cost. A found group
becomes fully offered on the next model request, including tools beyond summary position 64.
The public bounded summary API remains unchanged; availability checks and all policy, approvals,
scope and audit controls remain unchanged. No model receives authority by discovering a tool.

The browser's explicit snapshot is narrowed to inline readback without an output filename.
Automatic navigation/click snapshots remain provider-managed runtime artifacts; the model calls
`browser_snapshot {}` for inline elements instead of loading arbitrary files. Optional target/depth
can limit the read. Fill and typing retain the provider's existing target-based schemas and acting
permissions. No new JavaScript tool, capability flag, package upgrade or hidden extra call is added.

## Verification

A regression registers 83 callable tools, asserts the summary is still truncated, searches for the
late navigation tool, and verifies that the actual post-discovery model request includes every
browser tool. A real isolated pinned Playwright server opens a disposable loopback page, returns
inline element references, fills/types a field under explicit fixture-owner trust, and returns the
changed value in another inline snapshot. Invalid/file-output snapshot inputs are refused before
provider execution. Owner pages, account credentials and permission overrides are not modified.
