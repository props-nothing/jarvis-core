# ADR-0232: Stop and restart check background work and fail closed

- Status: Accepted
- Date: 2026-10-16
- Related: [ADR-0222](0222-coding-delivery-uses-owner-workspaces-and-durable-evidence.md), [ADR-0224](0224-coding-waits-and-verification-preserve-real-task-outcomes.md)

## Context

The authenticated stop and Settings restart guard counted only native agent runs. Coding delegation returns a durable job ID before its queued or running work finishes, and managed preview processes can outlive the parent conversation. A completed parent therefore let an ordinary restart interrupt work the guard did not count. Database read failures also became a zero count.

## Decision

Read native runs and queued/running coding jobs in one SQLite snapshot. Exclude terminal jobs and native runs parked for a durable owner approval. Count all workspaces in the profile because the daemon owns them all.

For managed host processes, consult the same live registry used by their tool adapter. Persisted request files from an earlier daemon are interrupted history, not proof of a running process; never count or signal a stale PID based on those files.

Stop and Settings restart share this guard. Known unfinished work returns HTTP 409 with the existing `runs_in_flight` code and total `in_flight`, plus `agent_runs`, `coding_jobs` and `managed_processes` counts. The total counts work records and processes, not unique user requests. An unreadable work table or unavailable process registry returns HTTP 503 with `work_state_unavailable`; it neither claims idleness nor launches a restart. The CLI reports this as unavailable work state. Explicit `force=true` / `--force` still bypasses the preflight, including its read failure.

## Boundaries

This is a point-in-time preflight, not an atomic admission barrier. New requests, schedules or tool calls can arrive after the check and before listener shutdown. Coordinated draining across every ingress remains separate work. OS shutdown/signals and explicit force still stop the process. No automatic coding replay, process resurrection, OS sandbox, cross-platform child cleanup acceptance or live-provider acceptance is introduced by this change.

## Verification

Real SQLite fixtures exercise native runs and children, durable approval waits, queued/running coding work after its parent settles, terminal jobs and read failures. Restart launch is observed through an injected launcher so refusal proves no launch and force preserves the selected root and flag. A real disposable managed child exercises registry admission, refusal, explicit cleanup and the terminal state; a stale on-disk record does not block. A poisoned registry refuses stop. Native macOS/Linux process behavior remains part of platform CI.

Local Windows verification on 2026-10-16 passed six stop tests, seven managed-process tests, two restart tests and two CLI lifecycle tests, including a real loopback HTTP 503 response. A temporary control restored the earlier native-run-only count and error-to-zero behavior: the queued-coding regression failed with HTTP 202 instead of 409, and the unreadable-state regression failed with 202 instead of 503. The corrected source was restored and byte equality checked; all four narrow suites then passed. Logs are retained locally under `artifacts/reviews/shutdown-followup-2026-10-16/`. This narrow evidence does not replace the workspace and native-platform gates.
