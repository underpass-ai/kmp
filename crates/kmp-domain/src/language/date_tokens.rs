//! Canonical date tokens for rendering fidelity, never stored text or search.
//!
//! Recognize named dates in Spanish and English, retaining a missing year.
//! Other formats stay literal; this is not a general date parser or translation.

pub(super) fn canonical_dates(tokens: Vec<String>) -> Vec<String> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        if let Some((date, consumed)) = named_date(&tokens[index..]) {
            result.push(date);
            index += consumed;
        } else {
            result.push(tokens[index].clone());
            index += 1;
        }
    }
    result
}

fn named_date(tokens: &[String]) -> Option<(String, usize)> {
    let (day, month_name, mut consumed) = match tokens {
        [day, de, month, ..] if de == "de" => (day, month, 3),
        [month, day, ..] if month_number(month).is_some() => (day, month, 2),
        [day, month, ..] => (day, month, 2),
        _ => return None,
    };
    if !(1..=2).contains(&day.len()) || !day.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let day = day.parse::<u32>().ok()?;
    let month = month_number(month_name)?;
    let year_token = match tokens.get(consumed) {
        Some(de) if de == "de" => {
            consumed += 2;
            Some(tokens.get(consumed - 1)?)
        }
        Some(year) if year.bytes().all(|b| b.is_ascii_digit()) && !year.is_empty() => {
            consumed += 1;
            Some(year)
        }
        _ => None,
    };
    let year = if let Some(token) = year_token {
        // An explicit but invalid year cannot degrade into a partial date.
        if token.len() != 4 || !token.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some(token.parse::<u32>().ok()?)
    } else {
        None
    };
    if !valid_day(year, month, day) {
        return None;
    }
    let date = match year {
        Some(year) => format!("{year:04}-{month:02}-{day:02}"),
        None => format!("--{month:02}-{day:02}"),
    };
    Some((date, consumed))
}

/// A full rendering can carry the known components of a partial source date.
/// This neither supplies nor validates its extra year. Never use the inverse
/// comparison: a complete source must retain its year.
pub(super) fn carries_partial_date(partial: &str, candidate: &str) -> bool {
    if !partial.starts_with("--") || partial.len() != 7 || candidate.len() != 10 {
        return false;
    }
    if candidate.get(4..) != partial.get(1..) {
        return false;
    }
    let parts = candidate.split('-').collect::<Vec<_>>();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || !parts
            .iter()
            .all(|part| part.bytes().all(|b| b.is_ascii_digit()))
    {
        return false;
    }
    let parsed = (parts[0].parse(), parts[1].parse(), parts[2].parse());
    matches!(parsed, (Ok(year), Ok(month), Ok(day)) if valid_day(Some(year), month, day))
}

fn valid_day(year: Option<u32>, month: u32, day: u32) -> bool {
    let leap = year.is_none_or(|year| {
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400))
    });
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    year != Some(0) && (1..=12).contains(&month) && day > 0 && day <= days
}

fn month_number(name: &str) -> Option<u32> {
    Some(match name {
        "january" | "enero" => 1,
        "february" | "febrero" => 2,
        "march" | "marzo" => 3,
        "april" | "abril" => 4,
        "may" | "mayo" => 5,
        "june" | "junio" => 6,
        "july" | "julio" => 7,
        "august" | "agosto" => 8,
        "september" | "septiembre" | "setiembre" => 9,
        "october" | "octubre" => 10,
        "november" | "noviembre" => 11,
        "december" | "diciembre" => 12,
        _ => return None,
    })
}
