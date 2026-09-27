//! Thousands grouped with a dot, read in the language that writes them so.
//!
//! Spanish writes fifty thousand nine hundred and seventy-six as `50.976`;
//! English writes it `50,976`, and either may write `50976`. A token alone
//! cannot say whether `50.976` is that integer or a decimal, which is why the
//! identifier lint keeps every three-digit group literal. The language of the
//! text it came from can say it, so the lint asks the text once and, when the
//! text is Spanish, reads a rendering's `50,976` and `50976` as the `50.976`
//! the text wrote. Nothing else changes: a leading zero (`0.976`) is never a
//! grouping, a comma in the Spanish text (`50,976`) is still its decimal, and
//! an English or undecided text keeps the literal comparison.

/// The shipped language that groups thousands with a dot.
pub(super) const DOTTED_THOUSANDS_LANGUAGE: &str = "spanish";

/// How many digits a thousands group holds.
const GROUP: usize = 3;

/// A rendering token's integer spelled with dot groups, when it is one:
/// `50,976` and `50976` both give `50.976`, `1,234,567` gives `1.234.567`.
/// A sign is kept. A number below a thousand, a leading zero, a decimal or
/// anything that is not digits and commas gives nothing.
pub(super) fn dotted(token: &str) -> Option<String> {
    let number = token.strip_prefix(['-', '+', '−']).unwrap_or(token);
    let sign = &token[..token.len() - number.len()];
    let digits = if number.contains(',') {
        let mut groups = number.split(',');
        let head = groups.next()?;
        if head.is_empty() || head.len() > GROUP {
            return None;
        }
        let mut digits = head.to_string();
        for group in groups {
            if group.len() != GROUP {
                return None;
            }
            digits.push_str(group);
        }
        digits
    } else {
        number.to_string()
    };
    if digits.len() <= GROUP
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let head = match digits.len() % GROUP {
        0 => GROUP,
        rest => rest,
    };
    let mut spelled = format!("{sign}{}", &digits[..head]);
    for start in (head..digits.len()).step_by(GROUP) {
        spelled.push('.');
        spelled.push_str(&digits[start..start + GROUP]);
    }
    Some(spelled)
}

#[cfg(test)]
mod tests {
    use super::dotted;

    #[test]
    fn an_integer_is_spelled_with_dot_groups() {
        assert_eq!(dotted("50,976").as_deref(), Some("50.976"));
        assert_eq!(dotted("50976").as_deref(), Some("50.976"));
        assert_eq!(dotted("1,234,567").as_deref(), Some("1.234.567"));
        assert_eq!(dotted("1234567").as_deref(), Some("1.234.567"));
        assert_eq!(dotted("-12,000").as_deref(), Some("-12.000"));
    }

    #[test]
    fn what_is_not_a_grouped_integer_gives_nothing() {
        for token in [
            "976", "0976", "0,976", "50,97", "5,0976", ",976", "50.976", "v1,000", "1,000x", "",
        ] {
            assert_eq!(dotted(token), None, "{token}");
        }
    }
}
