# ADR-0189: The calendar checks before it books, events can be deleted, and contacts can be merged

Status: Accepted
Date: 2026-10-11

## Context

The owner's capability audit left five "JARVIS tool test" events it could not remove, and wished for a calendar that checks for a clash before booking and a contact list that can merge duplicates (`P11-016`, wishes 18 and 19). With `jarvis.calendar.create` trusted, a booking ran unasked and could land on top of an existing event.

## Decision

1. **A booking over a busy event is refused, not asked about.** Before creating, `jarvis.calendar.create` asks the calendar for events in the window (`events.list`, `singleEvents`, the permission it already holds). Cancelled events and events marked free do not count. If any remain, nothing is created and the answer says how many and when; `allow_overlap: true` books anyway, for an owner who wants a double booking. If the check itself fails, nothing is booked. The answer deliberately leaves the clashing events' **titles** out: they can come from invitations written by strangers, and this sentence is not fenced. `jarvis.calendar.events` shows them, fenced. The Calendar API's separate free/busy method was not used: it needs a different permission and answers the same question for the one calendar JARVIS reads.
2. **`jarvis.calendar.events` lists each event's id** (`[id: ...]`, only if shaped like an id), so a delete can name one.
3. **`jarvis.calendar.delete`** (`events.delete` on the primary calendar): a write, risk 2, asks. The id must be letters, digits, `_` or `-`, so it cannot reach another path. It notifies no guests (the API's default). Gone or missing is said plainly.
4. **`jarvis.contacts.duplicates`** (read-only) finds pairs that look alike: the same person at a company spelled two ways (case, punctuation and a legal ending such as B.V. ignored), or the same phone number (7 or more digits). It names the fuller entry to keep and shows what the merge would make of them. It changes nothing.
5. **`jarvis.contacts.merge`** (write, risk 2, asks): the kept entry's words win; the other fills empty fields; a different value it held is written into the notes (capped at the notes limit); `do_not_contact` on either side wins (a merge can never lift the owner's word); a kept `new` takes the other's status. Both changes happen in one transaction, the removed entry goes first because addresses are unique, and the answer returns the removed entry whole, so the owner can restore it with `jarvis.contacts.save`.

## Consequences

Tests: the fixture Google now answers per method. Overlap refused with nothing posted, a stranger's title never repeated, free and cancelled events ignored, `allow_overlap`, a failed check books nothing, delete by id (success, gone, malformed ids sending no request, no permission sending nothing), ids listed. Storage: look-alikes found and strangers not, company normalisation, merge keeps everything and do-not-contact wins, an address moves across, unknown or identical ids change nothing, notes capped. Tool: preview then merge. Run live with a real model: two entries saved, the pair found, merged, and one contact left with every field.

Not proven: the Google calls on the owner's real calendar (no sign-in on the build machine): the owner confirms a real clash is flagged and a real event is deleted. Not done: matching by a person's name alone, merging more than two at once, a stored merge history (the removed entry in the answer is the undo).
