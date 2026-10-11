# ADR-0186: Telegram answers are formatted, not raw Markdown

Status: Accepted
Date: 2026-10-11

## Context

`ADR-0184` sent answers as plain text. The owner ran Telegram on a real phone: it worked, but every answer showed the model's Markdown as typed (`**bold**`, `## headings`, `|---|` tables). Telegram renders nothing unless a `parse_mode` is given, and even then knows only a few tags: no headings, lists or tables.

## Decision

1. **Convert Markdown to Telegram's HTML** (`apps/jarvisd/src/telegram_format.rs`) and send with `parse_mode: HTML`. HTML rather than MarkdownV2: MarkdownV2 requires escaping eighteen characters everywhere, and one missed `.` makes Telegram refuse the whole message; HTML needs only `<`, `>` and `&` escaped.
2. **What becomes what**: `**x**` `<b>`, `*x*` and `_x_` `<i>`, `~~x~~` `<s>`, `` `x` `` `<code>`, fenced code `<pre>` (with `<code class="language-x">` when the language is a plain word), links `<a>` (only `http`, `https` and `mailto` addresses), quotes `<blockquote>`, headings a bold line, list items a bullet, a number or a box, a rule a line. A table that fits 44 characters is an aligned monospace block; a wider one becomes one card per row (first cell as the title), because a wide table wraps into mush on a phone.
3. **Never markup from content.** Every `<`, `>` and `&` in text is escaped; raw HTML in an answer is shown as text (`<br>` becomes a line break); a code fence's language is used only if it is already a plain word, never cleaned up into one (`` ```"><script> `` is a plain block); marks that are not emphasis (`2 * 3 * 4`, `snake_case`) stay as typed.
4. **Cut between blocks.** Telegram allows 4096 characters after tags are removed; text is packed into messages of at most 3900 visible characters between blocks, a long code block is cut between lines and re-wrapped in its own `<pre>`, a line longer than a message is cut at a space. A tag is never cut.
5. **A message Telegram will not parse is sent again as plain text** (the piece carries both): a `400 ... can't parse entities` costs a prettier message, never the answer.
6. **Approval cards stay plain**: they hold raw tool arguments, and a card must show exactly what will run.

## Consequences

23 formatter and bridge tests, including the hostile cases (injected tags, a hostile fence, `javascript:` and `tg://` links, an attribute break-out), balanced tags in every piece of a long answer, the plain fallback on a 400, and a long answer arriving as several messages within the limit. Run live with the real daemon, a real model and a stand-in Telegram: a Dutch answer with a heading, a wide table, a list with bold names, a Python block, a quote and italics arrived as `<b>`, cards, bullets, `<pre><code class="language-python">`, `<blockquote>` and `<i>`.

Not proven: Telegram's own parser (no way to call it without a token), so the plain fallback is the safety net, and the owner's phone is the final check. Not handled: nested lists beyond four levels (flattened), numbered lists keep the author's numbers, inline HTML other than `<br>`, images (shown as their link), Markdown footnotes and definition lists.