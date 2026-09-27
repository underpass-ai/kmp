//! How deep into an ask's ranking a page reads (P14, `kmp2` lazy pages).

/// How many of the best eligible candidates make an ask's head (P14): the
/// window novelty reorders and every rescue walks from.
pub const RANK_HEAD_WINDOW: usize = 64;

/// How many tail items a continuation reads beyond the offset it resumes
/// at: more than a page can carry, so a page never ends for want of them.
pub const RANK_DEPTH_CHUNK: usize = 64;

/// The tail depth the page `cursor` asks for: none on a first page (the
/// head alone), and the resumed offset plus [`RANK_DEPTH_CHUNK`] on a
/// `kmp2` continuation. The offset counts every item before it, so the depth
/// always covers the page. Anything else is read as a first page; the
/// projection then refuses the cursor.
pub fn ask_rank_depth(cursor: Option<&str>) -> usize {
    cursor
        .and_then(|cursor| cursor.strip_prefix("kmp2:"))
        .and_then(|rest| rest.split(':').next())
        .and_then(|offset| offset.parse::<usize>().ok())
        .map_or(0, |offset| offset.saturating_add(RANK_DEPTH_CHUNK))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_continuation_reads_past_its_offset() {
        assert_eq!(ask_rank_depth(None), 0);
        assert_eq!(ask_rank_depth(Some("kmp2:10:0000000000000000:ab")), 74);
        assert_eq!(ask_rank_depth(Some("kmp1:10:ab")), 0);
    }
}
