# ADR-0231: Webhook deliveries reserve budget with their deduplication key

- Status: Accepted
- Date: 2026-10-16
- Scope: webhook admission, rolling run budgets and failure semantics
- Related: [file and webhook triggers](0195-triggers-can-watch-a-folder-or-listen-for-a-request-and-quiet-hours-hold-announcements.md), [transactional run admission](0228-native-subagent-admission-counts-and-starts-in-one-transaction.md)

## Context

Webhook admission counted events already linked to runs, then separately claimed the delivery and started a run. Concurrent distinct deliveries could all observe the same remaining capacity. An isolated local HTTP reproduction accepted twelve of twenty-four concurrent requests despite the three-per-minute limit. Deduplicating each event did not reserve the budget shared by different events.

## Decision

Storage admits a webhook delivery in one `BEGIN IMMEDIATE` transaction using the existing SQLite/SQLx transaction pattern. It checks that the webhook still exists and is enabled, recognises an already-claimed delivery key, counts the committed reservations in the rolling minute and 24-hour windows, and inserts the new event only when both budgets have room. The limits remain three per minute and fifty per rolling 24 hours. The transaction commits before the daemon starts a run or calls a model.

Every webhook event row reserves capacity, including a row with no `run_id`. Webhooks have no mail/file baseline rows, so no schema change or separate reservation table is required. Existing webhook claims without a run link count conservatively until their budget windows expire. Mail and file polling retain their existing baseline and run-count semantics.

A duplicate returns the existing duplicate acknowledgement even when the budget is full, consumes no new capacity and starts no work. A budget refusal writes no delivery key, returns HTTP 429, and may be retried later. Missing or paused webhooks remain indistinguishable HTTP 404 responses. A storage failure never grants admission; failures during reservation return HTTP 503.

## Crash and failure semantics

This preserves at-most-once admission. Once committed, a reservation is not refunded if run creation fails or the daemon stops before linking a run. It continues to consume budget until its time window expires, and its delivery key remains deduplicated afterwards. The first failed start returns HTTP 503, but retrying its committed delivery key acknowledges a duplicate; it does not retry the lost run. Atomic delivery-to-run creation and reliable event replay remain separate work. No exactly-once execution guarantee is introduced.

The database transaction covers admission only. Project/token guards and ordinary run/tool policy still apply. It is not a new authorization grant or a reservation of model tokens.

## Verification

Real-store tests race twenty-four distinct claims for one remaining minute slot and, separately, one remaining daily slot; only one is admitted. Concurrent identical deliveries reserve one slot and acknowledge the other callers even at capacity. Tests reopen the database before a run exists, check both rolling windows and permanent delivery deduplication, inject an insert failure, close the database, and refuse paused/non-webhook targets.

The HTTP route test races twenty-four distinct deliveries and requires exactly three runs and twenty-one HTTP 429 responses. Route regressions also require duplicate acknowledgement at capacity, count reservations without run links, and refuse work during a database outage.

On Windows, the fourteen trigger-storage tests and seventeen trigger-service tests passed. Temporarily restoring the former `run_id IS NOT NULL` budget filter made the concurrent-capacity regression fail: all twenty-four callers were admitted for one remaining slot. Restoring reservation counting made all fourteen storage tests pass again. Hosted native-platform verification remains part of the normal CI gate.
