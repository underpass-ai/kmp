//! Thousands grouped with a dot, read in the language that writes them so.
//!
//! Spanish writes fifty thousand nine hundred and seventy-six as `50.976`;
//! English writes it `50,976`, and either may write `50976`. A token alone
//! cannot say whether `50.976` is that integer or a decimal, which is why the
//! identifier lint keeps every three-digit group literal. The language of the
//! text it came from can say it, so the lint asks the text once and, when the
//! text is Spanish, reads a rendering's `50,976` and `50976` as the `50.976`
//! the text wrote. The same reading settles the comma: in a Spanish text
//! `50,976` is the decimal English writes `50.976`, so that rendering carries
//! it, while `50976` or `50,976` read as English (an integer) do not. A
//! leading zero (`0.976`) is never a grouping, and an English or undecided
//! text keeps the literal comparison.

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

/// A rendering token's decimal point spelled with the Spanish comma, when it
/// is one decimal point followed by exactly three digits: `50.976` gives
/// `50,976` and `0.976` gives `0,976`. Every other decimal is already read by
/// the separator swap that does not depend on language; only three digits
/// after the point needed the text's language to say it is a decimal.
pub(super) fn decimal_comma(token: &str) -> Option<String> {
    let number = token.strip_prefix(['-', '+', '−']).unwrap_or(token);
    let sign = &token[..token.len() - number.len()];
    let (whole, fraction) = number.split_once('.')?;
    let digits = |run: &str| !run.is_empty() && run.bytes().all(|byte| byte.is_ascii_digit());
    (digits(whole) && digits(fraction) && fraction.len() == GROUP)
        .then(|| format!("{sign}{whole},{fraction}"))
}

#[cfg(test)]
mod tests {
    use super::{decimal_comma, dotted};

    #[test]
    fn a_three_digit_decimal_point_is_spelled_with_the_comma() {
        assert_eq!(decimal_comma("50.976").as_deref(), Some("50,976"));
        assert_eq!(decimal_comma("0.976").as_deref(), Some("0,976"));
        assert_eq!(decimal_comma("-1.500").as_deref(), Some("-1,500"));
        for token in [
            "50.97",
            "50.9760",
            "1.234.567",
            "50,976",
            "v1.500",
            "50976",
            ".976",
        ] {
            assert_eq!(decimal_comma(token), None, "{token}");
        }
    }

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
