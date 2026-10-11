---
integration: webhook-and-file-triggers
status: researched
last_verified: 2026-10-11
owners: []
selected_spec_version: "GitHub webhook delivery headers (docs.github.com, read 2026-10-11); IETF draft-ietf-httpapi-idempotency-key-header (draft, not an RFC)"
selected_sdk: none; plain HTTP route and the standard library
---

# Webhook and file triggers (`P6-012`, `ADR-0195`)

## Scope

**In scope:** what a service that calls a webhook is known to send that JARVIS can use to recognise a retry, and what that means for a secret-address webhook with no signature check. A file trigger watches a local folder and calls no vendor, so it has no external contract; it is recorded in the ADR.

**Out of scope:** verifying a sender's HMAC signature (see Unresolved), reaching the daemon from the internet, and any provider SDK.

## Official sources

| Source | URL | Accessed | Purpose |
| --- | --- | --- | --- |
| GitHub webhook events and payloads | https://docs.github.com/en/webhooks/webhook-events-and-payloads | 2026-10-11 | the delivery headers, the payload cap |
| IETF HTTPAPI working group, Idempotency-Key header | https://datatracker.ietf.org/doc/draft-ietf-httpapi-idempotency-key-header/ | 2026-10-11 | the `Idempotency-Key` request header, which is a **draft** |

No `llms.txt` was looked for: neither is a product API with a documentation index; both are a convention read from the page.

## Verified facts

- GitHub sends `X-GitHub-Delivery`, "a globally unique identifier (GUID) to identify the event", on every delivery, plus `X-GitHub-Event` and `X-GitHub-Hook-ID`.
- GitHub signs a delivery with HMAC-SHA256 in `X-Hub-Signature-256` **only if** the webhook was configured with a secret; `X-Hub-Signature` (SHA-1) is kept for compatibility.
- GitHub caps a payload at 25 MB and does not deliver a larger one. Real payloads are tens of kilobytes, so a 16 KB bound would refuse ordinary deliveries; JARVIS accepts 128 KB and shows the run at most 3,000 characters.
- `Idempotency-Key` is an Internet-Draft of the HTTPAPI working group (`draft-ietf-httpapi-idempotency-key-header`), not a published standard; its use by senders varies.

## JARVIS decisions

- The delivery id used to recognise a retry is `Idempotency-Key` or `X-GitHub-Delivery`, taken only when plain (letters, digits, `- _ . :`, at most 100). A sender with neither gets no retry protection, and its requests are bounded by three a minute and 50 a day.
- Authentication is the **secret in the address** (64 hex digits from 32 random bytes, only its SHA-256 stored), not a signature. This is the weaker of the two designs a sender may offer: anyone who sees the address can use it, and a captured request can be replayed unless the sender supplies a delivery id.
- The body is untrusted: fenced, cut to 3,000 characters, never in the owner's objective.

## Unresolved

- **HMAC signature verification** (`X-Hub-Signature-256` and the other vendors' variants) is not built. It would let a sender that signs be authenticated beyond the secrecy of the address and would resist replay with a timestamp. It blocks only a use where the address might leak; it is the natural next step if a named sender needs it.
- Whether a given sender retries on `4xx`/`5xx` and how often is the sender's documentation, not read here; JARVIS answers `429` with `Retry-After: 60` and `503` while held.
- The daemon listens on loopback. Reaching it from the internet needs a tunnel to the console's port, which also serves the credential-protected console; the existing quick tunnel serves the MCP endpoint, not this route. Not built, and not recommended without a separate listener.

## Verification plan

Contract tests through the real router (no credential, secret in the address, same `404` for unknown, wrong-shape and paused addresses, replay by delivery id, a per-minute and per-day bound, an oversized body, a forged fence); a live run against the owner's daemon with a real `curl`.
