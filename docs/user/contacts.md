# Contacts

JARVIS keeps contacts in its own local database. Saving or importing a contact does not send a message or change an external address book.

- Ask to find contacts: `jarvis.contacts.search` searches company, person, email and notes; `jarvis.contacts.stats` counts statuses.
- Ask to save or update a contact: `jarvis.contacts.save` matches email without regard to case, otherwise company and person when the stored contact has no email.
- Ask to delete a contact: `jarvis.contacts.delete` takes its `id` from search and **asks for approval by default**, including contacts marked `do_not_contact`. It removes the whole contact and its restriction from JARVIS, not from Google, email or another address book. Contacts have no trash folder. A missing id reports `deleted: false`.
- Ask to import contacts from a CSV in a folder granted in Settings: `jarvis.contacts.import` takes a relative `path` and **asks for approval by default**. Both `files.read` and `contacts.write` permissions are required. The CSV file is not changed.

The normal approval policy applies: explicit standing trust ("Run without asking") can allow future calls without another question. For deletion, that trust also allows removing a `do_not_contact` contact and its restriction. Keep the contact if its address must remain blocked.

## CSV format

Use a UTF-8, comma-separated file (an optional UTF-8 BOM is accepted). A header and at least one contact are required. Header names are case-insensitive, surrounding header whitespace is ignored, and columns may be in any order:

| Header | Required | Limit |
| --- | --- | --- |
| `company` | Yes, with a nonblank value on every row | 200 characters |
| `person` | No | 200 characters |
| `role` | No | 200 characters |
| `email` | No | 254 characters; a nonblank value needs `@` and no whitespace |
| `phone` | No | 40 characters |
| `status` | No | `new`, `contacted`, `replied`, `meeting`, `won`, `lost`, `do_not_contact` |
| `notes` | No | 2,000 characters; newlines and tabs are allowed |
| `source` | No | 300 characters |

```csv
company,person,email,status,notes
"Acme, BV",Eva,eva@example.test,new,"Met at the fair"
Beta,Tom,tom@example.test,contacted,"First line
Second line with a ""quoted word"""
```

LF and CRLF line endings work; CRLF inside a quoted field becomes LF. Quote fields containing commas, newlines or quotes, and double a quote inside a quoted field. Unknown, duplicate or blank headers, blank data rows, mismatched column counts, malformed quoting, invalid UTF-8 and invalid contact fields are refused. Semicolon-separated files and spreadsheet files must first be saved as comma-separated UTF-8 CSV.

An import is limited to **1,000 data rows and 2 MiB**; the workspace holds at most 20,000 contacts. Paths must stay inside a granted folder, including when a directory link or Windows junction is present. Absolute paths, drive prefixes and parent traversal are refused.

## Updates and suppression

Import uses the same matching rules as save, including matches created by earlier rows in the same file. An omitted column leaves an existing field unchanged; an empty cell clears a text field. A blank or omitted status preserves an existing status and defaults to `new` for a new contact. Existing project links stay; contacts first found during a project run are filed under that project.

**Every row succeeds or nothing changes.** A later invalid row, storage failure, capacity limit or forbidden status change rolls back earlier creates and updates. The result reports processed rows as `imported`, plus `created` and `updated` row counts; repeated matches count as updates, not additional contacts.

Save and import cannot change an existing `do_not_contact` contact to another status. An import explicitly changing such a contact is refused in full; omit its status or keep `do_not_contact`. Merging preserves suppression from either entry, and Gmail send/draft continue to refuse a stored suppressed address.

Deletion is different: after normal policy authorization (approval or explicit standing trust), it removes the record **and** its `do_not_contact` restriction. No hidden suppression entry remains. Saving or importing that address later creates a new contact under the usual rules; deletion itself never sends anything. This owner-selected behavior is recorded in [ADR-0218](../adr/0218-contact-deletion-removes-the-record-and-its-suppression-under-normal-policy.md).
