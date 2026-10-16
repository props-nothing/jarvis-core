---
integration: google
status: implemented
last_verified: 2026-10-14
status_reconciled: 2026-10-16
owners: []
selected_spec_version: "Gmail API v1; Calendar API v3; source-specific access dates below"
selected_sdk: "Direct HTTP with pinned reqwest; no Google SDK"
---

# Google Workspace: current integration and evidence

## Implemented scope and remaining acceptance

The production daemon implements Google sign-in/refresh and Gmail/Calendar tools directly. Mail read/send/reply/draft/attachments/trash and calendar operations are composed into the tool pipeline. Historical claims that no production caller exists describe earlier connector-library scaffolding and are no longer current product status. Selected live owner-account evidence exists; it does not prove every operation or failure mode. See [Google setup and limits](../../user/google.md) and [the active acceptance ledger](../../../TODO.md).

Mail triggers poll a query and deduplicate ordinary runs. The unused library push/watch framework is not the production event path; its separate retirement task requires owner confirmation (ADR-0167). It is neither deleted nor extended by this documentation correction.

## Contract boundaries

- Rust owns account binding, private credential files, policy, approvals, provider outcomes and redacted diagnostics. Signing in is not permission to send.
- Follow the operation-specific scopes, pagination, cursor, quota and error-classification contracts in the dated research evidence. A successful send request is not proof of final delivery.
- Gmail Trash is recoverable message movement, not permanent deletion. Calendar overlap/delete acceptance and several owner-account failure cases remain open.
- A bodyless Calendar notification and Pub/Sub OIDC authentication cannot be generalized as HMAC-over-body. No Google push acceptance is implied by polling triggers.

## Historical findings and provenance

The [complete original record](../../history/google-research-through-2026-10-16.md) retains all 26 findings, source tables, verification plans/logs, rejected alternatives and unresolved questions. Its [connector findings](../../history/google-research-through-2026-10-16.md#findings-that-affect-slices-other-than-p5-005) and [verification log](../../history/google-research-through-2026-10-16.md#verification-log) are dated evidence. Recheck official sources before modifying an external contract; the status reconciliation on 2026-10-16 is not a new API verification.

## 2026-10-14 refresh: recoverable message deletion

**Scope:** move ONE Gmail message to trash, never permanently delete or delete a thread. Direct HTTP with the existing
`GoogleAccount` and pinned reqwest dependency; no Google SDK is introduced.

Live official sources accessed 2026-10-14 UTC:

- https://developers.google.com/llms.txt returned 404 (no usable index).
- https://gmail.googleapis.com/$discovery/rest?version=v1, revision **20261005**, specifies
  `POST gmail/v1/users/{userId}/messages/{id}/trash`, no request body, response `Message`.
- https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/trash confirms an empty body,
  `userId=me` and `gmail.modify` (or the unnecessarily broad `mail.google.com`) authorization scope.
- https://developers.google.com/workspace/gmail/api/auth/scopes classes `gmail.modify` restricted; it permits mailbox
  modification but not immediate permanent deletion bypassing trash.
- https://developers.google.com/workspace/gmail/api/reference/quota: trash costs 20 units; project 1,200,000/minute,
  user/project 6,000/minute; 80,000,000 daily project units before future billing applies.
- https://developers.google.com/workspace/gmail/api/guides/handle-errors: 401 credentials, 403 permission/quota,
  404 missing resource, 429 rate limiting, 5xx server failure.
- https://raw.githubusercontent.com/googleapis/google-api-python-client/main/docs/dyn/gmail_v1.users.messages.html
  independently confirms `trash` and recommends it instead of irreversible `delete`; used as contract evidence only.
- https://developers.google.com/workspace/gmail/release-notes: latest entry June 24, 2026; May 1 quota update already
  reflected above. No relevant trash endpoint deprecation observed.

**Decisions:** user approved bundling `gmail.modify` into the existing optional Google actions consent, not a new switch.
Existing grants continue to read/send as before, but trash fails closed until sign-in again grants modify. Settings must
explain the extra mailbox permission. Token placement/refresh/revocation stay inside `GoogleAccount`; bearer header,
no redirects, no tokens in receipts. `jarvis.gmail.delete` means **move to Gmail Trash** and asks approval by default
(risk 3, destructive effect, `google.act`, no blind retries); the owner explicitly chose to permit standing trust,
as for existing editing tools. No new approval-policy bypass is added. Validate message id and scope before network access; require
response id to match and `labelIds` to contain `TRASH` before claiming confirmation. Network loss, 5xx or malformed 2xx
are ambiguous effects, never a false claim that nothing changed. Provider error bodies are not returned to the model.
Message content remains at Google, subject to Gmail's trash retention rules; JARVIS stores only ordinary tool receipts.
No pagination, webhook, send, thread operation, hard delete or new retention store is added.

**Falsification plan:** local HTTP stand-in checks POST path, empty body, bearer header; wrong/missing permission and bad
ids cause zero requests. Missing TRASH, wrong id, 5xx or invalid response must never confirm deletion; 401/403/404/429
must fail clearly. Live Gmail trash/restore requires owner approval and is NOT run by this task. No copied SDK code.

**Verification observed:** `cargo test -p jarvisd google_ --lib` passed 37 tests, including empty-body POST/bearer
checks, confirmation shape, missing modify grant, invalid ids, and provider errors. `cargo test -p jarvis-connectors
google:: --lib` passed 333 tests after matching the research freshness metadata. Gmail trash/restore itself remains
**not run live**; no owner's message was altered.

## 2026-10-11 refresh: polling a Gmail query for `P6-012`

**Question.** Poll one owner-written Gmail search query, identify matching messages, and let JARVIS deduplicate each Gmail message id before starting an ordinary run. This refresh does not select Gmail push, change OAuth, or add a new provider operation; it reuses the direct HTTP read path already used by `jarvis.gmail.search`.

**Live official evidence checked 2026-10-11 UTC:**

- `https://developers.google.com/llms.txt` and `https://developers.google.com/gmail/api/llms.txt` still return HTTP 404. Google publishes no usable `llms.txt` for this API.
- `https://gmail.googleapis.com/$discovery/rest?version=v1` is the machine-readable Gmail API v1 discovery document selected for request/response names.
- `https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/list`: `GET /gmail/v1/users/{userId}/messages`; `userId=me`; `q` accepts Gmail search syntax and cannot be used with the `gmail.metadata` scope.
- `https://developers.google.com/workspace/gmail/api/guides/filtering`: `messages.list` supports the Gmail UI's search syntax with documented differences (no alias expansion and no thread-wide search).
- `https://developers.google.com/workspace/gmail/api/guides/sync`: the list operation returns message ids; content requires `messages.get`. History is normally available for at least a week but can be shorter, and stale history ids return HTTP 404. This slice deliberately polls the query and does not introduce a history cursor.
- `https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages/get`: a message id from `messages.list` can be read with `GET /gmail/v1/users/{userId}/messages/{id}`; the request body is empty.
- `https://developers.google.com/workspace/gmail/api/auth/scopes`: `gmail.readonly` remains a restricted scope; `gmail.metadata` cannot support `q`, so the existing readonly grant is required. Tokens remain in the existing `GoogleAccount` boundary and never enter trigger records.
- `https://developers.google.com/workspace/gmail/api/reference/quota`: per-minute limits remain 1,200,000 units per project and 6,000 per user per project; `messages.list` costs 5 units. Polling must therefore have a bounded minimum interval and no backlog loop.
- `https://developers.google.com/workspace/gmail/api/guides/handle-errors`: 401 is invalid credentials; 403 covers permission and rate-limit reasons; 429 and 5xx are retryable with exponential backoff. The trigger records a failed poll and waits for its next interval rather than spinning.
- `https://developers.google.com/workspace/gmail/release-notes`: the newest listed entry remains 2026-06-24; no later deprecation or wire change relevant to `messages.list` was observed.

**JARVIS decisions.**

- The Gmail query is owner configuration, not model output. It is bounded and encoded as one `q` parameter by the existing account client.
- The provider message id is untrusted input but is stored only as a bounded dedupe identity. A unique `(trigger_id, event_key)` claim is persisted before a run starts; replay starts no second run.
- Polling reads Gmail (private/restricted data), starts an ordinary run with no additional authority, and never sends or modifies mail. Any effect requested by that run re-enters the normal tool policy.
- The first poll establishes a baseline instead of replaying every old match. Later polls start runs only for ids not previously observed. The newest bounded page is sufficient for this slice; pagination and a durable history cursor remain out of scope.
- 401/403 authentication failures are visible trigger failures and require reauthentication where applicable. 429/5xx/network failures are not treated as empty mailboxes.

**Falsifying tests.** Against the existing local Google stand-in, return the same message id twice and prove exactly one run is created; return a provider error and prove no dedupe claim or run is created; seed an old matching message before the first poll and prove baseline establishment starts no run. The TODO item still requires a live harmless run against the owner's mailbox before `P6-012` can be marked complete.

**Unresolved.** Polling can miss more matches than the bounded first page receives between polls. That blocks high-volume guarantees, not this personal-mail first slice. A future history cursor must use the already researched `history.list` stale-cursor recovery rather than silently increasing the page size.

**Implemented 2026-10-11 (`ADR-0193`):** the poller lists the ten newest ids (`maxResults=10`), records the first look as a baseline, claims each id before its run, and starts at most three runs per look. A hostile or malformed id is ignored. **Verified live 2026-10-11** against the owner's real mailbox: `messages.list` with `q=in:drafts subject:...` answered the documented shape with the existing `gmail.readonly` grant, a new message started one run, and re-listing it started none. Not verified live: a 4xx/5xx from Google, token expiry mid-poll, or a mailbox with more than ten matches between looks.


## Official Sources

| Source | URL/version | Accessed | Purpose |
| --- | --- | --- | --- |
| Google Workspace `llms.txt` | **`not found`** — `https://developers.google.com/llms.txt` and `https://developers.google.com/gmail/api/llms.txt` both returned **HTTP 404** | 2026-09-27 | discovery — recorded as absent, not invented |
| Gmail API overview | https://developers.google.com/workspace/gmail/api/guides (page footer: last updated **2026-09-18**) | 2026-09-27 | the resource model and vocabulary |
| Gmail push notifications | https://developers.google.com/workspace/gmail/api/guides/push (last updated **2026-09-15**) | 2026-09-27 | `users.watch`, the Pub/Sub handshake, `history.list`, the notification envelope, rate and reliability limits |
| Gmail sync | https://developers.google.com/workspace/gmail/api/guides/sync (last updated **2026-09-15**) | 2026-09-27 | full vs partial sync, `historyId`, the failure signal |
| Gmail usage limits | https://developers.google.com/workspace/gmail/api/reference/quota (last updated **2026-09-10**) | 2026-09-27 | quota units, per-method costs, the **May 2026 model change**, the daily billing threshold |
| Gmail batch requests | https://developers.google.com/workspace/gmail/api/guides/batch (last updated **2026-09-10**) | 2026-09-30 | the **hard** batch limit (100), the **recommended** size (50), and that a batch counts as *n* requests against quota |
| Gmail scopes | https://developers.google.com/workspace/gmail/api/auth/scopes (last updated **2026-09-10**) | 2026-09-27 | the non-sensitive / sensitive / restricted taxonomy |
| Gmail error handling | https://developers.google.com/workspace/gmail/api/guides/handle-errors (last updated **2026-09-15**) | 2026-09-30 | the status and `reason` taxonomy, backoff guidance — and that its sections are 400/401/403/404/429/5xx, i.e. it documents **no `410`**, unlike the Calendar page (Finding 6) |
| Gmail MCP server reference | https://developers.google.com/workspace/gmail/api/reference/mcp (last updated **2026-07-21**) | 2026-09-27 | the first-party MCP endpoint and its toolset |
| Calendar push notifications | https://developers.google.com/workspace/calendar/api/guides/push (last updated **2026-09-11**) | 2026-09-27 | channel creation, the `X-Goog-*` headers, the sync message, renewal, delivery semantics |
| Calendar sync | https://developers.google.com/workspace/calendar/api/guides/sync (last updated **2026-09-11**) | 2026-09-27 | `nextSyncToken`, the `410 Gone` signal |
| Calendar error handling | https://developers.google.com/workspace/calendar/api/guides/errors (last updated **2026-09-11**) | 2026-09-30 | the per-status JSON bodies, **including the three distinct `410` causes** — one of which needs no action |
| Calendar scopes | https://developers.google.com/workspace/calendar/api/auth (last updated **2026-09-03**) | 2026-09-27 | the Calendar scope list |
| OAuth consent and scope categories | https://developers.google.com/workspace/guides/configure-oauth-consent (last updated **2026-09-03**) | 2026-09-27 | which review each scope category requires |
| Cloud Pub/Sub push authentication | https://docs.cloud.google.com/pubsub/docs/authenticate-push-subscriptions (last updated **2026-09-24**) | 2026-09-27 | **the JWT-bearer mechanism** — see Finding 1 |
| Cloud Pub/Sub push subscriptions | https://docs.cloud.google.com/pubsub/docs/push (last updated **2026-09-24**) | 2026-09-30 | the wrapped envelope, **at-least-once delivery**, the **five** acknowledging statuses, unwrapped delivery, push backoff — and the **padded** example that corrects Finding 11 (`ADR-0089`) |
| Cloud Pub/Sub `PubsubMessage` | https://docs.cloud.google.com/pubsub/docs/reference/rest/v1/PubsubMessage (last updated **2026-05-14**) | 2026-09-30 | the envelope's field **types** — including `data`'s, which **contradicts** the Gmail guide (`ADR-0088`) |
| Google OIDC discovery document | https://accounts.google.com/.well-known/openid-configuration | 2026-09-27 | **machine-readable**: every endpoint, the PKCE methods, and whether the `iss` response parameter is supported |
| Google OAuth 2.0 for native apps | https://developers.google.com/identity/protocols/oauth2/native-app (last updated **2026-09-14**) | 2026-09-27 | the installed-app flow: the loopback method, the token and refresh exchanges, the response fields, DPoP, revocation |
| Gmail `users.history.list` method reference | https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.history/list (page footer: last updated **2026-04-15**) | 2026-10-01 | the incremental-sync read: **the query parameters, the required `startHistoryId`, and the response body `{ history[], nextPageToken, historyId }`** — the shape the `gmail_history_list` fixtures reproduce. **The storing rule is in `startHistoryId`'s description, not in `historyId`'s**: *"If you receive no nextPageToken in the response, there are no updates to retrieve and you can store the returned historyId for a future request."* (`ADR-0108`, Finding 22) |
| Gmail `Format` enum + `Message` resource | https://developers.google.com/workspace/gmail/api/reference/rest/v1/Format (last updated **2026-03-24**) and https://developers.google.com/workspace/gmail/api/reference/rest/v1/users.messages | 2026-09-30 | **which fields each `format` returns** — the fact that bounds what a `messages.get` output can declare (`ADR-0083`) |
| Calendar `events.list` method reference | https://developers.google.com/workspace/calendar/api/v3/reference/events/list (last updated **2026-07-29**) | 2026-09-30 | the full parameter list and **the eight parameters that cannot be combined with `syncToken`** (`ADR-0084`), the `maxResults` default and ceiling, and the `nextPageToken`/`nextSyncToken` exclusivity |
| Calendar `events.watch` method reference | https://developers.google.com/workspace/calendar/api/v3/reference/events/watch (last updated **2026-05-12**) | 2026-10-01 | the **response** `expiration` typed **`long`** — *"a Unix timestamp, in milliseconds"* — which **contradicts the guide's "property string"** for the same field; `params.ttl` *"Default is 604800 seconds"*; the channel fields and the `resourceId`/`resourceUri` distinction (`ADR-0106`, Finding 20) |
| Calendar `events.watch` **request body** (method reference + push guide, *Make watch requests*) | https://developers.google.com/workspace/calendar/api/v3/reference/events/watch (last updated **2026-05-12**) and https://developers.google.com/workspace/calendar/api/guides/push (last updated **2026-09-11**) | 2026-10-01 | **the fields a channel *creation* carries** — the fact that made channel creation constructible at all (`ADR-0112`, Finding 23): the **required** trio (`id` ≤ 64 chars, *"echoed back in the `X-Goog-Channel-Id`"*; `type` = `web_hook`; `address`, **https-only with a valid certificate**), the **optional** pair (`token` ≤ 256 chars for anti-spoofing, `expiration` as a **Unix timestamp in milliseconds**), and that `address` and `token` are **credential-adjacent** fields a rendered body would disclose |
| Calendar `channels.stop` method reference | https://developers.google.com/workspace/calendar/api/v3/reference/channels/stop (last updated **2025-04-01**) | 2026-10-01 | the stop body is exactly `{ id, resourceId, token? }` with **no `expiration`**, so the `resourceId` a `watch` response returns is what makes teardown constructible (`ADR-0106`) |

The shared OAuth protocol facts — PKCE, loopback redirects, rotation, revocation — are recorded once in
[oauth2-pkce-native-apps.md](oauth2-pkce-native-apps.md) and are not restated here.


## Sign-in as built (`ADR-0149`, 2026-10-09)

Verified again against the live native-app page on 2026-10-09 (the structure and parameter list match the record above):

- **The owner brings the OAuth client.** A Desktop-app client belongs to a Google Cloud project, and JARVIS cannot ship a shared one (it would be
  a distributed secret, and the Gmail scopes are restricted, which needs Google's verification and a security assessment for a public app). So the
  owner creates one in their own project; the client id is a setting and the client secret a key file.
- **Testing mode ends the sign-in after 7 days.** Verified on https://developers.google.com/identity/protocols/oauth2: a project whose consent screen
  is External with publishing status "Testing" is issued a refresh token that expires in 7 days (unless only name/email/profile scopes are requested).
  The setup steps therefore say to **publish** the app; an unverified published app works for the owner, with a warning screen.
- **Redirect:** `http://127.0.0.1:<daemon port>/oauth/google/callback`. This chooses a path other than the connector crate's registered `/`
  (`GoogleConnector::registered_redirect`); the native-app page allows a loopback redirect with any port, and Desktop-app clients accept it
  without registration. **Unresolved Question 7 stays open in one respect:** whether the console would reject a path for a Desktop client was not
  established by a real sign-in here, only that Google accepted the authorization request's shape (below).
- **Authorization request:** `response_type=code`, `client_id`, `redirect_uri`, `state`, `code_challenge`, `code_challenge_method=S256`,
  `scope=<gmail.readonly calendar.readonly>`, and `prompt=consent` so a repeat sign-in returns a refresh token.
- **Code exchange** (form POST to the token endpoint): `grant_type=authorization_code`, `code`, `client_id`, `client_secret`, `code_verifier`,
  `redirect_uri`. The secret is sent because a Desktop client is issued one, even though the page lists it as optional.
- **Refresh:** `grant_type=refresh_token`, `refresh_token`, `client_id`, `client_secret`; an `invalid_grant` answer means the grant is gone (revoked,
  or expired in Testing mode), so the stored sign-in is deleted and the owner is told to sign in again.
- **Profile:** `GET https://gmail.googleapis.com/gmail/v1/users/me/profile` gives the mailbox address shown in Settings.
- **Reads:** `GET .../gmail/v1/users/me/messages?q&maxResults`, `.../messages/{id}?format=metadata|full`, and
  `GET https://www.googleapis.com/calendar/v3/calendars/primary/events?timeMin&timeMax&singleEvents=true&orderBy=startTime&maxResults`.
- **Live evidence:** a deliberately fake client id was sent to Google's real authorization endpoint with the exact URL JARVIS builds; Google answered
  `Error 401: invalid_client, The OAuth client was not found` (so the request was parsed and only the client was refused). **Not verified:** a real
  sign-in, the token exchange against Google, and the Gmail and Calendar reads, which need the owner's own client and account.
