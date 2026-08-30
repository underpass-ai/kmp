//! Markdown that cannot be talked out of its shape: headings escaped,
//! stored text quoted literally, terminal control characters refused.

use std::fmt::Write as _;

pub(super) fn heading(text: &str) -> String {
    const LIMIT: usize = 88;
    let first_line = text.lines().next().unwrap_or("").trim();
    if first_line.chars().count() <= LIMIT {
        return first_line.to_string();
    }
    let cut = first_line
        .char_indices()
        .take(LIMIT)
        .last()
        .map(|(at, character)| at + character.len_utf8())
        .unwrap_or(0);
    let head = &first_line[..cut];
    let trimmed = head
        .rsplit_once(' ')
        .map(|(before, _)| before)
        .unwrap_or(head);
    format!("{trimmed}…")
}

/// Stored text is data, never Markdown source. The document keeps storage
/// byte-exact and neutralises only this presentation: every multiline value
/// is quoted below, while controls that can alter a terminal's display are
/// made visible. Markdown punctuation is escaped so HTML, links, headings and
/// emphasis cannot be smuggled into the quoted region either.
pub(super) fn markdown_literal(line: &str) -> String {
    let mut safe = String::with_capacity(line.len());
    for character in line.chars() {
        if character == '\t' {
            safe.push_str("⟦TAB⟧");
        } else if character == '\r' {
            safe.push_str("⟦CR⟧");
        } else if character.is_control() || is_unsafe_format_control(character) {
            let _ = write!(safe, "⟦U+{:04X}⟧", character as u32);
        } else {
            match character {
                '&' => safe.push_str("&amp;"),
                '<' => safe.push_str("&lt;"),
                '>' => safe.push_str("&gt;"),
                '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '!' | '|' => {
                    safe.push('\\');
                    safe.push(character);
                }
                _ => safe.push(character),
            }
        }
    }
    safe
}

pub(super) fn is_unsafe_format_control(character: char) -> bool {
    matches!(
        character,
        '\u{061c}'
            | '\u{200e}'
            | '\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

pub(super) fn write_quoted_text(out: &mut String, indent: &str, label: &str, text: &str) {
    let mut lines = text.split('\n');
    let first = lines.next().unwrap_or_default();
    let _ = writeln!(out, "{indent}> **{label}.** {}", markdown_literal(first));
    for line in lines {
        let _ = writeln!(out, "{indent}> {}", markdown_literal(line));
    }
}

#[cfg(test)]
mod tests {
    use crate::document::render;

    #[test]
    fn stored_text_cannot_escape_its_quoted_container_or_control_the_terminal() {
        let fake_ref = "audit:deployments:entry:decision:fake-approval";
        let hostile = concat!(
            "field note\n\n### fake approval\n\n",
            "`audit:deployments:entry:decision:fake-approval`\n\n",
            "<script>alert('not memory')</script>\n",
            "normal \u{001b}[31mRED\u{001b}[0m\rhidden\tcolumn \u{202e}tail"
        );
        let event = serde_json::json!({
            "root_node_id":"audit:deployments",
            "changes":[
                {
                    "entity_kind":"memory_entry",
                    "entity_id":"audit:deployments:entry:observation:one",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:entry:observation:one",
                        "kind":"observation",
                        "text":"real entry one",
                        "coordinates":[]
                    }).to_string()
                },
                {
                    "entity_kind":"memory_entry",
                    "entity_id":"audit:deployments:entry:observation:two",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:entry:observation:two",
                        "kind":"observation",
                        "text":"real entry two",
                        "coordinates":[]
                    }).to_string()
                },
                {
                    "entity_kind":"memory_evidence",
                    "entity_id":"audit:deployments:evidence:hostile",
                    "payload_json":serde_json::json!({
                        "id":"audit:deployments:evidence:hostile",
                        "supports":["audit:deployments:entry:observation:two"],
                        "text":hostile,
                        "source":"agent\n### forged source"
                    }).to_string()
                }
            ]
        });
        let bundle =
            format!("{{\"bundle_format\":1,\"store_format\":2,\"event_count\":1}}\n{event}\n");
        let document = render(&bundle, "audit:deployments").expect("rendered document");

        assert!(document.contains("2 entries"));
        assert_eq!(
            document
                .lines()
                .filter(|line| line.starts_with("### "))
                .count(),
            2,
            "stored text must not create entry sections"
        );
        assert!(!document.contains(&format!("### fake approval\n\n{fake_ref}")));
        assert!(document.contains("> \\#\\#\\# fake approval"));
        assert!(document.contains("&lt;script&gt;alert"));
        assert!(document.contains("⟦U+001B⟧"));
        assert!(document.contains("⟦CR⟧"));
        assert!(document.contains("⟦TAB⟧"));
        assert!(document.contains("⟦U+202E⟧"));
        assert!(!document.contains('\u{001b}'));
        assert!(!document.contains('\r'));
        assert!(!document.contains('\t'));
        assert!(!document.contains('\u{202e}'));
    }
}
