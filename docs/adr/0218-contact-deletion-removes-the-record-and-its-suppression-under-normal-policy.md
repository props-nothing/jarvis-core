# ADR-0218: Contact deletion removes the record and its suppression under normal policy

**Status:** Accepted

**Date:** 2026-10-15

**Supersedes:** The model contact-deletion suppression veto in [ADR-0211](0211-recoverable-cleanup-and-bounded-data-editing-tools.md).

## Context

An owner-requested contact cleanup left unwanted records behind because `jarvis.contacts.delete`
advertised and enforced a blanket refusal for `do_not_contact`, even after normal authorization.
The owner explicitly chose deletion of both the record and its restriction, rather than retaining
an independent suppression entry. A contact status must not override that authorized cleanup.

## Decision

- `jarvis.contacts.delete` removes any contact in its workspace, including `do_not_contact`.
  The canonical owner and automated deletion paths use the same workspace-scoped atomic delete.
- Deletion remains destructive, risk 3, requires `contacts.write` and asks by default. Existing
  explicit owner standing trust may authorize it without another question. Schema validation,
  authentication, authorization, deadlines and durable outcome/audit recording remain unchanged.
  No model-supplied owner override or separate suppression-reset step is introduced.
- Removing the record also removes its restriction. There is no suppression tombstone. A later
  save/import can recreate the address with a new status; this is an intentional consequence.
- Save/import still cannot change an existing suppressed record to another status. Merge still
  preserves suppression, and Gmail send/draft still fail closed for a stored suppressed address
  or an unreadable contact store. Deletion sends nothing and changes no external address book.
- The delete tool's description and schema state the consequence, and its version becomes `1.1.0`.
  Other contact tool versions and wire result fields are unchanged; no storage migration is needed.

## Consequences and verification

The owner can request "keep this contact and remove the others" without first changing each
unwanted contact's status. Deletion is permanent, not contact trash; retaining a contact remains
necessary if its address must stay blocked. Approval to delete or standing trust for this tool now
includes removing that restriction, not authority to send a message.

Regression tests cover all contact statuses while retaining one chosen contact, workspace isolation,
missing/repeated identifiers, removal of suppression and subsequent reimport. Real SQLite/tool-pipeline
tests prove missing permission, pending approval and denial leave a suppressed record untouched,
while approval and explicit standing trust execute and return confirmed deletion. Existing save/import,
merge and Gmail suppression tests remain required. Live owner data is not modified by these fixtures;
rebuild/restart the daemon before retrying the cleanup in an existing installation.
