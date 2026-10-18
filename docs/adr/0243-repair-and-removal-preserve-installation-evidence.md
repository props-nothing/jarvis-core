# ADR-0243: Repair and removal preserve installation evidence

- Status: Accepted
- Date: 2026-10-18
- Scope: clean-machine lifecycle continuation of ADR-0242.

## Decision

`update repair` uses a settled update record to restore exactly its selected image
after accidental executable loss or corruption. It holds exclusive installation and
profile ownership, checks the exact profile/schema and healthy current database,
verifies the retained source before execution, and replaces the target atomically.
Candidate repair rechecks the signed archive. Previous-image repair uses the retained
pre-activation identity. Existing damaged bytes are retained before replacement.
Pending/malformed evidence requires recovery, not repair. The unchanged journal
continues to block startup unless the target matches its recorded identity; interrupted
repair can be retried. Repair never restores a database or silently changes versions.

Uninstall requires a stopped daemon and a separate executable (the retained updater
or verified installer copy). The installed executable cannot report successful
self-removal. Hold installation/profile ownership through exact service/PATH cleanup
and executable removal. A foreign or drifted service, pending update or cleanup error
refuses removal. Absence of the executable is an idempotent outcome after checking
remaining ownership/evidence. Preserve profile and update evidence, including helper
copies; report their retention explicitly. No recursive cleanup or data-deletion flag.

This deliberately uses the existing offline maintenance boundary. It does not add a
privileged background uninstaller, delete-on-reboot job, updater daemon or hidden
retry. Service removal must inspect failures and verify manager state where absence
can also produce an error. Owner work is never stopped forcibly by uninstall.

## Acceptance boundary

Native development CI can prove implementation behavior but cannot establish a
clean OS or successful live onboarding. Clean acceptance needs identified OS images,
no Rust/Node/Python installation, actual provider checks, authenticated readiness,
signed update/rollback, damaged/missing executable repair, service/PATH removal,
preserved data and repeat operations. Host/provider access is a separate prerequisite.

[Research](../research/integrations/clean-machine-lifecycle.md) records the official
contracts, negative controls and infrastructure discovery.
