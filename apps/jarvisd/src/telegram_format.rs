//! Markdown to the HTML Telegram accepts (`docs/research/integrations/telegram.md`, `ADR-0186`).
//!
//! A model answers in Markdown. Telegram shows plain text unless a `parse_mode` is given, and even then it knows only a few tags
//! (`b`, `i`, `u`, `s`, `a`, `code`, `pre`, `blockquote`): no headings, lists or tables. So this turns what a model writes into what a
//! phone can show:
//!
//! - `**bold**`, `*italic*`, `~~strike~~`, `` `code` ``, `[links](https://...)` become their tags; fenced code becomes `<pre>`.
//! - A heading becomes a bold line, a list item gets a bullet or its number, a task item a box, a rule a line, a quote a `<blockquote>`.
//! - A **table** becomes an aligned monospace block when it fits a phone's width, and otherwise one small card per row (a wide table
//!   in monospace wraps into mush).
//! - Everything else is escaped: `<`, `>` and `&` in the text can never become markup, so a page a model read cannot inject a tag.
//!
//! Telegram limits a message to 4096 characters *after* tags are removed, so long text is cut **between blocks** (and inside a long code
//! block between lines, re-wrapped in its own `<pre>`), never through a tag. Each piece also carries its plain text, because a message
//! Telegram refuses to parse is sent again as plain text rather than lost.

use std::fmt::Write as _;

/// How many levels of list nesting are shown.
const MAX_LIST_DEPTH: usize = 4;
/// The widest a monospace table may be, in characters, before it becomes cards. A phone shows about 40 to 45 in a code block.
const TABLE_MAX_WIDTH: usize = 44;

/// One message: its HTML, and the same text with the tags removed, for when Telegram will not parse the HTML.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Piece {
    /// The HTML to send with `parse_mode: HTML`.
    pub html: String,
    /// What it says without markup.
    pub plain: String,
}

/// A run of lines that stay together, wrapped in the same tags if it has to be cut.
struct Block {
    open: String,
    close: String,
    lines: Vec<String>,
}

impl Block {
    fn text(lines: Vec<String>) -> Self {
        Self {
            open: String::new(),
            close: String::new(),
            lines,
        }
    }

    fn code(language: &str, code: &[&str]) -> Self {
        let (open, close) = if language.is_empty() {
            ("<pre>".to_owned(), "</pre>".to_owned())
        } else {
            (
                format!("<pre><code class=\"language-{language}\">"),
                "</code></pre>".to_owned(),
            )
        };
        let mut lines: Vec<String> = code.iter().map(|line| escape(line)).collect();
        if lines.is_empty() {
            lines.push("\u{a0}".to_owned());
        }
        Self { open, close, lines }
    }

    fn render(&self, lines: &[String]) -> String {
        format!("{}{}{}", self.open, lines.join("\n"), self.close)
    }
}

/// Turns Markdown into messages of at most `max_visible` visible characters each.
#[must_use]
pub fn to_pieces(markdown: &str, max_visible: usize) -> Vec<Piece> {
    let normalised = markdown.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<String> = normalised
        .split('\n')
        .flat_map(|line| wrap_long(line, max_visible))
        .collect();
    pack(&parse_blocks(&lines), max_visible)
}

/// A line longer than a message is cut at whitespace so that one line always fits.
fn wrap_long(line: &str, max: usize) -> Vec<String> {
    if line.chars().count() <= max {
        return vec![line.to_owned()];
    }
    let mut pieces = Vec::new();
    let mut rest: Vec<char> = line.chars().collect();
    while rest.len() > max {
        let cut = rest[..max]
            .iter()
            .rposition(|c| c.is_whitespace())
            .filter(|at| *at > max / 2)
            .unwrap_or(max);
        pieces.push(rest[..cut].iter().collect::<String>().trim_end().to_owned());
        rest = rest[cut..]
            .iter()
            .copied()
            .skip_while(|c| c.is_whitespace())
            .collect();
    }
    pieces.push(rest.into_iter().collect());
    pieces
}

fn pack(blocks: &[Block], max: usize) -> Vec<Piece> {
    let mut finished: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_visible = 0_usize;
    for block in blocks {
        let whole = block.render(&block.lines);
        let visible = visible_len(&whole);
        if !current.is_empty() && current_visible + 2 + visible <= max {
            current.push_str("\n\n");
            current.push_str(&whole);
            current_visible += 2 + visible;
            continue;
        }
        if visible <= max {
            if !current.is_empty() {
                finished.push(std::mem::take(&mut current));
            }
            current = whole;
            current_visible = visible;
            continue;
        }
        // A block longer than a message is cut between its lines.
        if !current.is_empty() {
            finished.push(std::mem::take(&mut current));
        }
        let mut group: Vec<String> = Vec::new();
        let mut group_visible = 0_usize;
        for line in &block.lines {
            let line_visible = visible_len(line);
            if !group.is_empty() && group_visible + 1 + line_visible > max {
                finished.push(block.render(&group));
                group.clear();
                group_visible = 0;
            }
            group_visible += if group.is_empty() {
                line_visible
            } else {
                1 + line_visible
            };
            group.push(line.clone());
        }
        if !group.is_empty() {
            current = block.render(&group);
            current_visible = group_visible;
        }
    }
    if !current.is_empty() {
        finished.push(current);
    }
    finished
        .into_iter()
        .map(|html| Piece {
            plain: strip_tags(&html),
            html,
        })
        .collect()
}

// ---- blocks -----------------------------------------------------------------------------------------------------------------

fn parse_blocks(lines: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut paragraph: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if trimmed.is_empty() {
            flush(&mut paragraph, &mut blocks);
            i += 1;
        } else if let Some((fence, language)) = fence_open(trimmed) {
            flush(&mut paragraph, &mut blocks);
            let mut code: Vec<&str> = Vec::new();
            i += 1;
            while i < lines.len() && !fence_close(lines[i].trim(), fence) {
                code.push(lines[i].as_str());
                i += 1;
            }
            i += 1;
            blocks.push(Block::code(&language, &code));
        } else if is_table_start(lines, i) {
            flush(&mut paragraph, &mut blocks);
            i = parse_table(lines, i, &mut blocks);
        } else if let Some(heading) = heading(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::text(vec![format!("<b>{}</b>", inline(&heading))]));
            i += 1;
        } else if is_rule(trimmed) {
            flush(&mut paragraph, &mut blocks);
            blocks.push(Block::text(vec!["\u{2500}".repeat(12)]));
            i += 1;
        } else if trimmed.starts_with('>') {
            flush(&mut paragraph, &mut blocks);
            let mut quoted = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let text = lines[i].trim_start().trim_start_matches(['>', ' ']);
                quoted.push(inline(text.trim_end()));
                i += 1;
            }
            blocks.push(Block {
                open: "<blockquote>".to_owned(),
                close: "</blockquote>".to_owned(),
                lines: quoted,
            });
        } else if list_item(&lines[i]).is_some() {
            flush(&mut paragraph, &mut blocks);
            i = parse_list(lines, i, &mut blocks);
        } else {
            paragraph.push(inline(trimmed));
            i += 1;
        }
    }
    flush(&mut paragraph, &mut blocks);
    blocks
}

fn flush(paragraph: &mut Vec<String>, blocks: &mut Vec<Block>) {
    if !paragraph.is_empty() {
        blocks.push(Block::text(std::mem::take(paragraph)));
    }
}

/// An opening code fence: its character and run length, and the language if it is one Telegram can be told.
fn fence_open(trimmed: &str) -> Option<((char, usize), String)> {
    let first = trimmed.chars().next().filter(|c| matches!(c, '`' | '~'))?;
    let run = trimmed.chars().take_while(|c| *c == first).count();
    if run < 3 {
        return None;
    }
    let info = trimmed[run..].trim();
    // An inline use of three backticks on one line ("```x```") is not a fence.
    if first == '`' && info.contains('`') {
        return None;
    }
    // The language ends up in an attribute: it is used only when it is a plain word, never cleaned up into one.
    let word = info.split_whitespace().next().unwrap_or_default();
    let language = if word.chars().count() <= 20
        && word
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '_' | '#'))
    {
        word.to_owned()
    } else {
        String::new()
    };
    Some(((first, run), language))
}

fn fence_close(trimmed: &str, (character, run): (char, usize)) -> bool {
    trimmed.chars().take_while(|c| *c == character).count() >= run
        && trimmed.chars().all(|c| c == character)
}

fn heading(trimmed: &str) -> Option<String> {
    let hashes = trimmed.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim_end();
    // A heading is already bold: marks inside it would nest a tag in itself.
    (!text.is_empty()).then(|| text.replace("**", "").replace("__", ""))
}

fn is_rule(trimmed: &str) -> bool {
    let compact: String = trimmed.chars().filter(|c| *c != ' ').collect();
    compact.len() >= 3
        && matches!(compact.chars().next(), Some('-' | '*' | '_'))
        && compact.chars().all(|c| compact.starts_with(c))
}

fn is_table_start(lines: &[String], i: usize) -> bool {
    lines[i].contains('|')
        && lines.get(i + 1).is_some_and(|next| {
            let next = next.trim();
            next.contains('|')
                && next.contains('-')
                && next.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
        })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Marker {
    Bullet,
    Numbered,
}

/// A list item: its indent, its marker, the number text and the rest of the line.
fn list_item(line: &str) -> Option<(usize, Marker, String, &str)> {
    let indent = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    let text = line.trim_start();
    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = text.strip_prefix(bullet) {
            return Some((indent, Marker::Bullet, String::new(), rest));
        }
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits)
        && matches!(text[digits..].chars().next(), Some('.' | ')'))
        && text[digits + 1..].starts_with(' ')
    {
        return Some((
            indent,
            Marker::Numbered,
            text[..digits].to_owned(),
            &text[digits + 2..],
        ));
    }
    None
}

fn pad(level: usize) -> String {
    "\u{a0}\u{a0}\u{a0}".repeat(level.min(MAX_LIST_DEPTH))
}

fn parse_list(lines: &[String], mut i: usize, blocks: &mut Vec<Block>) -> usize {
    let mut out: Vec<String> = Vec::new();
    while let Some((indent, marker, number, rest)) = list_item(&lines[i]) {
        let level = (indent / 2).min(MAX_LIST_DEPTH);
        let rest = rest.trim();
        let (prefix, rest) = if let Some(task) = rest.strip_prefix("[ ] ") {
            ("\u{2610}".to_owned(), task)
        } else if let Some(task) = rest
            .strip_prefix("[x] ")
            .or_else(|| rest.strip_prefix("[X] "))
        {
            ("\u{2611}".to_owned(), task)
        } else if marker == Marker::Numbered {
            (format!("{number}."), rest)
        } else {
            (
                ["\u{2022}", "\u{25e6}", "\u{25aa}"][level.min(2)].to_owned(),
                rest,
            )
        };
        out.push(format!("{}{prefix} {}", pad(level), inline(rest.trim())));
        i += 1;
        // Indented lines after an item belong to it.
        while i < lines.len()
            && !lines[i].trim().is_empty()
            && list_item(&lines[i]).is_none()
            && lines[i].starts_with("  ")
            && fence_open(lines[i].trim()).is_none()
        {
            out.push(format!("{}{}", pad(level + 1), inline(lines[i].trim())));
            i += 1;
        }
        // A blank line between items of one list does not end it.
        let mut next = i;
        while next < lines.len() && lines[next].trim().is_empty() {
            next += 1;
        }
        if next < lines.len() && next > i && list_item(&lines[next]).is_some() {
            i = next;
        }
        if i >= lines.len() || list_item(&lines[i]).is_none() {
            break;
        }
    }
    blocks.push(Block::text(out));
    i
}

// ---- tables -----------------------------------------------------------------------------------------------------------------

fn split_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix('|').unwrap_or(trimmed);
    let mut cells = vec![String::new()];
    let mut chars = trimmed.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'|') {
            chars.next();
            if let Some(cell) = cells.last_mut() {
                cell.push('|');
            }
        } else if c == '|' {
            cells.push(String::new());
        } else if let Some(cell) = cells.last_mut() {
            cell.push(c);
        }
    }
    cells
        .into_iter()
        .map(|cell| strip_tags(&inline(cell.trim())))
        .collect()
}

fn parse_table(lines: &[String], start: usize, blocks: &mut Vec<Block>) -> usize {
    let header = split_cells(&lines[start]);
    let mut i = start + 2;
    let mut rows: Vec<Vec<String>> = Vec::new();
    while i < lines.len() && !lines[i].trim().is_empty() && lines[i].contains('|') {
        rows.push(split_cells(&lines[i]));
        i += 1;
    }
    let columns = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(header.len());
    let fill = |mut row: Vec<String>| {
        row.resize(columns, String::new());
        row
    };
    let header = fill(header);
    let rows: Vec<Vec<String>> = rows.into_iter().map(fill).collect();
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            std::iter::once(&header)
                .chain(rows.iter())
                .map(|row| row[column].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    let total: usize = widths.iter().sum::<usize>() + 3 * columns.saturating_sub(1);
    if total <= TABLE_MAX_WIDTH {
        let line = |row: &[String]| {
            row.iter()
                .zip(&widths)
                .map(|(cell, width)| format!("{cell:<width$}"))
                .collect::<Vec<_>>()
                .join(" \u{2502} ")
                .trim_end()
                .to_owned()
        };
        let mut shown = vec![line(&header)];
        shown.push(
            widths
                .iter()
                .map(|width| "\u{2500}".repeat(*width))
                .collect::<Vec<_>>()
                .join("\u{2500}\u{253c}\u{2500}"),
        );
        shown.extend(rows.iter().map(|row| line(row)));
        let refs: Vec<&str> = shown.iter().map(String::as_str).collect();
        blocks.push(Block::code("", &refs));
    } else {
        // Too wide for a phone: one card per row, the first cell as its title.
        for row in &rows {
            let mut card = vec![format!("<b>{}</b>", escape(&row[0]))];
            for (name, value) in header.iter().zip(row).skip(1) {
                if !value.is_empty() {
                    card.push(format!("{}: {}", escape(name), escape(value)));
                }
            }
            blocks.push(Block::text(card));
        }
        if rows.is_empty() {
            blocks.push(Block::text(vec![format!(
                "<b>{}</b>",
                escape(&header.join(" \u{b7} "))
            )]));
        }
    }
    i
}

// ---- inline -----------------------------------------------------------------------------------------------------------------

/// The HTML for the inline Markdown of one line.
fn inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    inline_chars(&chars)
}

fn run_len(c: &[char], at: usize, ch: char) -> usize {
    c[at..].iter().take_while(|x| **x == ch).count()
}

fn find_backtick_close(c: &[char], from: usize, run: usize) -> Option<usize> {
    let mut j = from;
    while j < c.len() {
        if c[j] == '`' {
            let found = run_len(c, j, '`');
            if found == run {
                return Some(j);
            }
            j += found;
        } else {
            j += 1;
        }
    }
    None
}

fn find_close(c: &[char], from: usize, ch: char, n: usize) -> Option<usize> {
    let mut j = from;
    while j < c.len() {
        if c[j] == '`' {
            let run = run_len(c, j, '`');
            j = find_backtick_close(c, j + run, run).map_or(j + run, |end| end + run);
        } else if c[j] == '\\' {
            j += 2;
        } else if c[j] == ch {
            let run = run_len(c, j, ch);
            let closes = run == n
                && j > from
                && !c[j - 1].is_whitespace()
                && (ch != '_' || c.get(j + n).is_none_or(|next| !next.is_alphanumeric()));
            if closes {
                return Some(j);
            }
            j += run;
        } else {
            j += 1;
        }
    }
    None
}

/// An emphasis starting at `i`: how many marks it uses and where it closes.
fn emphasis(c: &[char], i: usize, ch: char, run: usize) -> Option<(usize, usize)> {
    let n = match ch {
        '~' if run == 2 => 2,
        '*' | '_' if run <= 3 => run,
        _ => return None,
    };
    if c.get(i + n).is_none_or(|next| next.is_whitespace()) {
        return None;
    }
    if ch == '_' && i > 0 && c[i - 1].is_alphanumeric() {
        return None;
    }
    let close = find_close(c, i + n, ch, n)?;
    (close > i + n).then_some((n, close))
}

/// A link at `i`: where its text ends and the address, if the address is one a message may carry.
fn parse_link(c: &[char], i: usize) -> Option<(usize, String, usize)> {
    let mut depth = 0_usize;
    let mut close = None;
    for (offset, ch) in c[i..].iter().enumerate() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close?;
    if c.get(close + 1) != Some(&'(') {
        return None;
    }
    let mut parens = 0_usize;
    let mut end = None;
    for (offset, ch) in c[close + 1..].iter().enumerate() {
        match ch {
            '(' => parens += 1,
            ')' => {
                parens -= 1;
                if parens == 0 {
                    end = Some(close + 1 + offset);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end?;
    let inner: String = c[close + 2..end].iter().collect();
    let url = inner
        .split_whitespace()
        .next()?
        .trim_matches(['<', '>'])
        .to_owned();
    let lower = url.to_ascii_lowercase();
    (lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("mailto:"))
        .then_some((close, url, end))
}

fn inline_chars(c: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        match ch {
            '\\' if c.get(i + 1).is_some_and(char::is_ascii_punctuation) => {
                push_escaped(&mut out, c[i + 1]);
                i += 2;
            }
            '`' => {
                let run = run_len(c, i, '`');
                if let Some(end) = find_backtick_close(c, i + run, run) {
                    let inner: String = c[i + run..end].iter().collect();
                    let shown = if inner.len() >= 2
                        && inner.starts_with(' ')
                        && inner.ends_with(' ')
                        && !inner.trim().is_empty()
                    {
                        inner[1..inner.len() - 1].to_owned()
                    } else {
                        inner
                    };
                    if shown.is_empty() {
                        out.push_str(&"`".repeat(run * 2));
                    } else {
                        out.push_str("<code>");
                        out.push_str(&escape(&shown));
                        out.push_str("</code>");
                    }
                    i = end + run;
                } else {
                    out.push_str(&"`".repeat(run));
                    i += run;
                }
            }
            // An image is shown as the link to it.
            '!' if c.get(i + 1) == Some(&'[') => i += 1,
            '[' => {
                if let Some((close, url, end)) = parse_link(c, i) {
                    let text = inline_chars(&c[i + 1..close]);
                    let shown = if text.trim().is_empty() {
                        escape(&url)
                    } else {
                        text
                    };
                    let _ = write!(out, "<a href=\"{}\">{shown}</a>", attribute(&url));
                    i = end + 1;
                } else {
                    out.push('[');
                    i += 1;
                }
            }
            '*' | '_' | '~' => {
                let run = run_len(c, i, ch);
                if let Some((n, close)) = emphasis(c, i, ch, run) {
                    let inner = inline_chars(&c[i + n..close]);
                    out.push_str(
                        match (ch, n) {
                            ('~', _) => format!("<s>{inner}</s>"),
                            (_, 1) => format!("<i>{inner}</i>"),
                            (_, 2) => format!("<b>{inner}</b>"),
                            _ => format!("<b><i>{inner}</i></b>"),
                        }
                        .as_str(),
                    );
                    i = close + n;
                } else {
                    out.push_str(&ch.to_string().repeat(run));
                    i += run;
                }
            }
            '<' => {
                let rest: String = c[i..c.len().min(i + 6)]
                    .iter()
                    .collect::<String>()
                    .to_ascii_lowercase();
                if let Some(length) = ["<br />", "<br/>", "<br>"]
                    .iter()
                    .find(|tag| rest.starts_with(**tag))
                    .map(|tag| tag.len())
                {
                    out.push('\n');
                    i += length;
                } else {
                    out.push_str("&lt;");
                    i += 1;
                }
            }
            _ => {
                push_escaped(&mut out, ch);
                i += 1;
            }
        }
    }
    out
}

// ---- escaping ---------------------------------------------------------------------------------------------------------------

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        other => out.push(other),
    }
}

/// Text made safe to sit between tags.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        push_escaped(&mut out, ch);
    }
    out
}

fn attribute(url: &str) -> String {
    escape(url).replace('"', "&quot;")
}

/// The text of some HTML as a person reads it: tags removed, the four entities Telegram knows turned back.
#[must_use]
pub fn strip_tags(html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    let mut inside = false;
    for ch in html.chars() {
        match ch {
            '<' => inside = true,
            '>' if inside => inside = false,
            _ if !inside => text.push(ch),
            _ => {}
        }
    }
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

fn visible_len(html: &str) -> usize {
    strip_tags(html).chars().count()
}

#[cfg(test)]
#[path = "telegram_format_tests.rs"]
mod tests;
