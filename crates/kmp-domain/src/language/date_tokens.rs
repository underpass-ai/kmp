//! Canonical date tokens for rendering fidelity, never stored text or search.
//!
//! Recognize complete day/month-name/year dates in Spanish and English. Other
//! formats stay literal; this is not a general date parser or a translation.

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
    let (day, month_name, year, consumed) = match tokens {
        [day, de, month, de_year, year, ..] if de == "de" && de_year == "de" => {
            (day, month, year, 5)
        }
        [month, day, year, ..] if month_number(month).is_some() => (day, month, year, 3),
        [day, month, year, ..] => (day, month, year, 3),
        _ => return None,
    };
    if !(1..=2).contains(&day.len())
        || year.len() != 4
        || !day.bytes().chain(year.bytes()).all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let day = day.parse::<u32>().ok()?;
    let year = year.parse::<u32>().ok()?;
    let month = month_number(month_name)?;
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    };
    if year == 0 || day == 0 || day > days {
        return None;
    }
    Some((format!("{year:04}-{month:02}-{day:02}"), consumed))
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
