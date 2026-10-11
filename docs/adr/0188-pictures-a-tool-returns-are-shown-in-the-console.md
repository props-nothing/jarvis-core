# ADR-0188: Pictures a tool returns are shown in the console, not given to the model

Status: Accepted
Date: 2026-10-11

## Context

The built-in browser's `browser_take_screenshot` returns the picture as an MCP `image` block (base64 and a media type). `McpCallResult::from_sdk` kept only the text blocks, so the picture was dropped: the owner asked for a screenshot and got a sentence. Putting the bytes in the model's context would cost far more than the picture is worth, and the model is not the one who needs to see it.

## Decision

1. **Keep the pictures, bounded.** `McpCallResult` carries up to 4 images per result, each at most 6 MB decoded; the encoded length is checked before decoding, and a block that is not valid base64 is dropped without failing the call.
2. **A sink keeps them** (`jarvis_mcp_transport::ImageSink`, installed once per process, like one daemon per process). Without a sink the pictures are dropped as before. The daemon's `ImageStore` writes `<data>/images/<32 hex>.<ext>` (written whole, then renamed) and prunes to 120 files and 256 MB, oldest first.
3. **Decided from the bytes.** Only a PNG, JPEG, GIF or WebP signature is kept, whatever the server declared; SVG and anything that can carry script is not a picture. The file is checked again on every read.
4. **The model is told, not shown.** The tool result begins with `[picture <id>: shown to the user in the console; you cannot see it]`, outside the untrusted fence and ahead of the text, so a long result cannot cut it off. A live model answered correctly that the picture is displayed to the owner and that it described the page from text, not from pixels.
5. **An authenticated route**, `GET /api/v1/images/{id}`: the identifier must be 32 lowercase hex characters (so no path can be named), the answer carries the signature's media type, `nosniff`, `Content-Security-Policy: default-src 'none'; sandbox` and a private immutable cache. Without the credential it is `401`.
6. **The console shows it.** `result_payload` names the picture ids found in a successful result (at most 4, each re-validated) in the `tool_result` activity event; the console fetches each with the credential, shows it from a blob in the chat bubble (click to open full size), and keeps the ids in the saved chat so a reload shows it again. The page's policy gains `img-src blob:` and nothing else: no remote or `data:` image can load.
7. A forged marker in a server's text can only name an id the store made (random, unguessable), so it cannot make the console show anything the owner was not already going to see.

## Consequences

Tests: image decoding and bounds (client), the sink contract and a refused picture not stopping the rest, the model-facing note and a picture-only result, signature detection for the four kinds, a declared type not deciding, hostile ids and a hand-placed file refused, pruning, the route's headers, `401` without the credential and `404` for unknown or hostile ids, and marker extraction (hostile, malformed, capped, failures name none). Run live with a real model and the real Playwright server: a 45 KB PNG was stored, shown in the chat (1280 x 720), and still shown after a reload. The run found a defect the unit tests could not: the console's policy (`default-src 'none'`) blocked blob images, so `img-src blob:` was added.

Not done: a screenshot the model asked to be saved under a **filename** is written by the Playwright server and is not returned, so it is not shown (the model is better asked for no filename); images from sources other than MCP (a generated picture arrives through the same sink once a provider is added, `P11-011`); a long-lived archive (pictures are pruned).

## Addendum: Telegram (2026-10-11)

When a run that was asked in the paired Telegram chat ends with an answer, the pictures its tools showed (at most 4, found in the run's `tool_result` events, read back from the image store) are sent first as `sendPhoto` uploads (multipart, `reqwest` feature `multipart`, which adds `mime_guess` and `unicase`), then the answer. A photo Telegram refuses (size or proportions, `400`) is sent again as `sendDocument`. Tests: the upload's method, content type, field names and bytes, the token never in the body, the document fallback, and which events name pictures. Run live with a real model, the real Playwright server and a stand-in Telegram: a 45 KB PNG arrived as a photo upload before the answer. Not proven: Telegram's own acceptance of the upload (no real token here), so the owner's phone is the final check.
