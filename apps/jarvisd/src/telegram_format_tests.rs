//! What a model writes, as a phone shows it. Every expectation here is a string Telegram's documented HTML subset accepts.

use super::*;

fn one(markdown: &str) -> String {
    let pieces = to_pieces(markdown, 3900);
    assert_eq!(pieces.len(), 1, "{pieces:?}");
    pieces[0].html.clone()
}

#[test]
fn emphasis_code_strike_and_links_become_their_tags() {
    assert_eq!(
        one("**bold**, *italic*, _also italic_, ~~gone~~ and `code`"),
        "<b>bold</b>, <i>italic</i>, <i>also italic</i>, <s>gone</s> and <code>code</code>"
    );
    assert_eq!(one("***both***"), "<b><i>both</i></b>");
    assert_eq!(
        one("**bold with *italic* inside**"),
        "<b>bold with <i>italic</i> inside</b>"
    );
    assert_eq!(
        one("see [the docs](https://example.com/a?b=1&c=2) now"),
        "see <a href=\"https://example.com/a?b=1&amp;c=2\">the docs</a> now"
    );
}

/// **Marks that are not emphasis stay as typed.**
#[test]
fn arithmetic_snake_case_and_stray_marks_are_left_alone() {
    assert_eq!(one("2 * 3 * 4 = 24"), "2 * 3 * 4 = 24");
    assert_eq!(
        one("use snake_case_names and my_var_name"),
        "use snake_case_names and my_var_name"
    );
    assert_eq!(
        one("5 stars ** and an unclosed *mark"),
        "5 stars ** and an unclosed *mark"
    );
    assert_eq!(one("a ~ b and ~~ c"), "a ~ b and ~~ c");
    assert_eq!(one("\\*not italic\\*"), "*not italic*");
    assert_eq!(one("price: 3 * (x + y)"), "price: 3 * (x + y)");
}

/// **Nothing a page or a person wrote can become a tag.**
#[test]
fn angle_brackets_and_ampersands_are_escaped_and_html_is_never_passed_through() {
    assert_eq!(
        one("<script>alert(1)</script> & <b>not bold</b> <a href=\"x\">"),
        "&lt;script&gt;alert(1)&lt;/script&gt; &amp; &lt;b&gt;not bold&lt;/b&gt; &lt;a href=\"x\"&gt;"
    );
    assert_eq!(one("a`<b>`b"), "a<code>&lt;b&gt;</code>b");
    assert_eq!(one("line<br>next<BR/>end"), "line\nnext\nend");
    // A link may only go to the web or a mail address, and its attribute cannot be broken out of.
    assert_eq!(one("[x](javascript:alert(1))"), "[x](javascript:alert(1))");
    assert_eq!(one("[x](tg://user?id=1)"), "[x](tg://user?id=1)");
    assert_eq!(
        one("[x](https://e.com/\"onmouseover=\"y)"),
        "<a href=\"https://e.com/&quot;onmouseover=&quot;y\">x</a>"
    );
}

#[test]
fn headings_lists_tasks_rules_and_quotes_are_shown_as_a_phone_can() {
    assert_eq!(one("# Title\n## Sub **x**"), "<b>Title</b>\n\n<b>Sub x</b>");
    let list = one("- one\n- two with **bold**\n  - nested\n1. first\n2) second");
    assert_eq!(
        list,
        "\u{2022} one\n\u{2022} two with <b>bold</b>\n\u{a0}\u{a0}\u{a0}\u{25e6} nested\n1. first\n2. second"
    );
    assert_eq!(
        one("- [ ] todo\n- [x] done"),
        "\u{2610} todo\n\u{2611} done"
    );
    assert_eq!(
        one("above\n\n---\n\nbelow"),
        "above\n\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\n\nbelow"
    );
    assert_eq!(
        one("> quoted *text*\n> second"),
        "<blockquote>quoted <i>text</i>\nsecond</blockquote>"
    );
    // A blank line inside a list does not end it.
    assert_eq!(one("- a\n\n- b"), "\u{2022} a\n\u{2022} b");
}

#[test]
fn fenced_code_is_preformatted_escaped_and_named() {
    assert_eq!(
        one("run:\n```rust\nlet a = 1 < 2 && **x**;\n```\ndone"),
        "run:\n\n<pre><code class=\"language-rust\">let a = 1 &lt; 2 &amp;&amp; **x**;</code></pre>\n\ndone"
    );
    assert_eq!(one("```\nplain\n```"), "<pre>plain</pre>");
    // An unclosed fence runs to the end; a language that is not a plain word is not named.
    assert_eq!(one("```\"><script>\ncode"), "<pre>code</pre>");
    assert_eq!(one("```x\" onclick=\"y\ncode"), "<pre>code</pre>");
    assert_eq!(one("~~~\nx\n"), "<pre>x\n</pre>");
}

#[test]
fn a_table_that_fits_is_aligned_monospace() {
    let text =
        one("| Stad | Inwoners |\n|---|---:|\n| Utrecht | 360000 |\n| **Den Haag** | 560000 |");
    assert_eq!(
        text,
        "<pre>Stad     \u{2502} Inwoners\n\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{253c}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\nUtrecht  \u{2502} 360000\nDen Haag \u{2502} 560000</pre>"
    );
}

/// **A table too wide for a phone becomes one small card per row.**
#[test]
fn a_wide_table_becomes_cards() {
    let text = one(
        "| Company | City | Employees | Note |\n|---|---|---|---|\n| Rabobank | Utrecht | 43000 | the biggest cooperative bank around |\n| Vopak | Rotterdam | 5900 | |",
    );
    assert_eq!(
        text,
        "<b>Rabobank</b>\nCity: Utrecht\nEmployees: 43000\nNote: the biggest cooperative bank around\n\n<b>Vopak</b>\nCity: Rotterdam\nEmployees: 5900"
    );
}

#[test]
fn a_pipe_in_ordinary_text_is_not_a_table() {
    assert_eq!(one("a | b | c\nsecond line"), "a | b | c\nsecond line");
}

/// **Long text is cut between blocks, every piece is within the limit, and no tag is ever cut.**
#[test]
fn long_text_is_cut_between_blocks_and_code_is_rewrapped() {
    let paragraph = "word ".repeat(100);
    let text = format!(
        "{paragraph}\n\n{paragraph}\n\n{paragraph}\n\n```\n{}\n```\n\n**tail**",
        "line of code\n".repeat(80)
    );
    let pieces = to_pieces(&text, 700);
    assert!(pieces.len() >= 4, "{}", pieces.len());
    for piece in &pieces {
        assert!(
            piece.plain.chars().count() <= 700,
            "{}",
            piece.plain.chars().count()
        );
        for tag in ["b", "i", "pre", "code", "blockquote"] {
            assert_eq!(
                piece.html.matches(&format!("<{tag}>")).count()
                    + piece.html.matches(&format!("<{tag} ")).count(),
                piece.html.matches(&format!("</{tag}>")).count(),
                "unbalanced <{tag}> in {}",
                piece.html
            );
        }
    }
    let code_pieces = pieces
        .iter()
        .filter(|piece| piece.html.contains("<pre>"))
        .count();
    assert!(
        code_pieces >= 2,
        "a long code block must be cut into several <pre> blocks"
    );
    assert!(
        pieces
            .last()
            .is_some_and(|piece| piece.html.ends_with("<b>tail</b>"))
    );
}

#[test]
fn a_line_longer_than_a_message_is_cut_at_a_space() {
    let text = "alpha ".repeat(300);
    let pieces = to_pieces(&text, 500);
    assert!(pieces.len() >= 4);
    assert!(
        pieces
            .iter()
            .all(|piece| piece.plain.chars().count() <= 500)
    );
    assert_eq!(
        pieces
            .iter()
            .map(|piece| piece.plain.split_whitespace().count())
            .sum::<usize>(),
        300
    );
}

#[test]
fn the_plain_text_is_what_a_reader_sees_and_blank_input_sends_nothing() {
    let pieces = to_pieces("**a** < b & `c` [d](https://e.com)", 3900);
    assert_eq!(pieces[0].plain, "a < b & c d");
    assert!(to_pieces("", 3900).is_empty());
    assert!(to_pieces("  \n\n \r\n", 3900).is_empty());
    assert_eq!(strip_tags("<b>x &lt;y&gt; &amp;amp;</b>"), "x <y> &amp;");
}

/// A realistic model answer, end to end.
#[test]
fn a_typical_answer_comes_out_whole() {
    let answer = "## Resultaat\n\nIk heb **3 bedrijven** gevonden:\n\n1. *Rabobank* \u{2014} Utrecht\n2. Vopak \u{2014} Rotterdam\n\n| Naam | Plaats |\n|---|---|\n| ASML | Veldhoven |\n\nZie [bron](https://example.com).\n\n> Let op: cijfers uit 2024.";
    let html = one(answer);
    for wanted in [
        "<b>Resultaat</b>",
        "<b>3 bedrijven</b>",
        "1. <i>Rabobank</i>",
        "<pre>Naam",
        "<a href=\"https://example.com\">bron</a>",
        "<blockquote>Let op: cijfers uit 2024.</blockquote>",
    ] {
        assert!(html.contains(wanted), "{wanted:?} missing from {html}");
    }
    assert!(!html.contains("**") && !html.contains("## ") && !html.contains("|---"));
}
