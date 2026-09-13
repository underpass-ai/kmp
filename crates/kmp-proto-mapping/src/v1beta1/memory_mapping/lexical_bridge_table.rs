use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) const MAGIC: &[u8; 8] = b"KMPBRIDG";
pub(super) const VERSION: u16 = 1;

/// One validated lexical bridge allocation and the ranges that give its
/// sections meaning. Keeping the file bytes whole lets the filesystem adapter
/// hand ownership across without copying its words or vectors.
#[derive(Debug)]
pub(super) struct LexicalBridgeTable {
    bytes: Vec<u8>,
    dims: usize,
    count: usize,
    offsets: Range<usize>,
    words: Range<usize>,
    vectors: Range<usize>,
    norms: Box<[AtomicU64]>,
    provenance: String,
}

impl LexicalBridgeTable {
    pub(super) fn parse(bytes: Vec<u8>) -> Result<Self, String> {
        let mut cursor = bytes.as_slice();
        if take(&mut cursor, MAGIC.len())? != MAGIC {
            return Err("not a lexical bridge table: bad magic".to_string());
        }
        let version = u16_le(&mut cursor)?;
        if version != VERSION {
            return Err(format!("lexical bridge version {version} is not {VERSION}"));
        }
        let dims = usize::from(u16_le(&mut cursor)?);
        if dims == 0 {
            return Err("lexical bridge vectors have no dimensions".to_string());
        }
        let count = u32_le(&mut cursor)? as usize;
        let provenance_len = usize::from(u16_le(&mut cursor)?);
        let provenance = std::str::from_utf8(take(&mut cursor, provenance_len)?)
            .map_err(|_| "lexical bridge provenance is not UTF-8".to_string())?
            .to_string();

        let offsets_start = position(&bytes, cursor);
        let offsets_len = count
            .checked_add(1)
            .and_then(|value| value.checked_mul(size_of::<u32>()))
            .ok_or_else(|| "lexical bridge offset table is too large".to_string())?;
        let offsets_bytes = take(&mut cursor, offsets_len)?;
        let offsets = offsets_start..offsets_start + offsets_len;
        let first = offset_at(offsets_bytes, 0);
        let mut previous = first;
        for index in 1..=count {
            let offset = offset_at(offsets_bytes, index);
            if offset < previous {
                return Err("lexical bridge word offsets are not increasing".to_string());
            }
            previous = offset;
        }
        if first != 0 {
            return Err("lexical bridge word offsets are not increasing".to_string());
        }

        let words_start = position(&bytes, cursor);
        let words_len = previous as usize;
        let words_bytes = take(&mut cursor, words_len)?;
        let words = words_start..words_start + words_len;
        validate_words(words_bytes, offsets_bytes, count)?;

        let vectors_start = position(&bytes, cursor);
        let vectors_len = count
            .checked_mul(dims)
            .ok_or_else(|| "lexical bridge vector table is too large".to_string())?;
        take(&mut cursor, vectors_len)?;
        let vectors = vectors_start..vectors_start + vectors_len;
        if !cursor.is_empty() {
            return Err(format!(
                "lexical bridge has {} trailing bytes",
                cursor.len()
            ));
        }

        let norms = (0..count)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self {
            bytes,
            dims,
            count,
            offsets,
            words,
            vectors,
            norms,
            provenance,
        })
    }

    pub(super) fn len(&self) -> usize {
        self.count
    }

    pub(super) fn provenance(&self) -> &str {
        &self.provenance
    }

    pub(super) fn index_of(&self, word: &str) -> Option<usize> {
        let (mut low, mut high) = (0usize, self.count);
        while low < high {
            let middle = low + (high - low) / 2;
            match self.word(middle).cmp(word.as_bytes()) {
                std::cmp::Ordering::Less => low = middle + 1,
                std::cmp::Ordering::Greater => high = middle,
                std::cmp::Ordering::Equal => return Some(middle),
            }
        }
        None
    }

    pub(super) fn similarity_between(&self, left: usize, right: usize) -> f64 {
        // Preserve the format-v1 arithmetic byte for byte: vectors are still
        // visited left to right as signed i8 values, and only the integer
        // norms are cached. The dot-to-f64 conversion, square root and
        // division therefore happen in the same order as the original reader.
        let dot = self
            .vector(left)
            .iter()
            .zip(self.vector(right))
            .map(|(a, b)| i64::from(*a as i8) * i64::from(*b as i8))
            .sum::<i64>();
        let norms = self.norm(left) * self.norm(right);
        if norms == 0 {
            return 0.0;
        }
        dot as f64 / (norms as f64).sqrt()
    }

    fn word(&self, index: usize) -> &[u8] {
        let offsets = &self.bytes[self.offsets.clone()];
        let start = offset_at(offsets, index) as usize;
        let end = offset_at(offsets, index + 1) as usize;
        &self.bytes[self.words.start + start..self.words.start + end]
    }

    fn vector(&self, index: usize) -> &[u8] {
        let start = self.vectors.start + index * self.dims;
        &self.bytes[start..start + self.dims]
    }

    fn norm(&self, index: usize) -> u64 {
        let cached = self.norms[index].load(Ordering::Relaxed);
        if cached != 0 {
            return cached - 1;
        }
        let norm = self
            .vector(index)
            .iter()
            .map(|value| u64::from((*value as i8).unsigned_abs()).pow(2))
            .sum::<u64>();
        // A v1 row has at most u16::MAX dimensions of i8 values, so its norm
        // plus this cache sentinel is far below u64::MAX.
        self.norms[index].store(norm + 1, Ordering::Relaxed);
        norm
    }
}

fn validate_words(words: &[u8], offsets: &[u8], count: usize) -> Result<(), String> {
    let mut previous: Option<&[u8]> = None;
    for index in 0..count {
        let start = offset_at(offsets, index) as usize;
        let end = offset_at(offsets, index + 1) as usize;
        let word = &words[start..end];
        if std::str::from_utf8(word).is_err() {
            return Err(format!("lexical bridge word {index} is not UTF-8"));
        }
        if previous.is_some_and(|previous| previous >= word) {
            return Err(format!("lexical bridge words are not sorted at {index}"));
        }
        previous = Some(word);
    }
    Ok(())
}

fn offset_at(offsets: &[u8], index: usize) -> u32 {
    let start = index * size_of::<u32>();
    u32::from_le_bytes(
        offsets[start..start + size_of::<u32>()]
            .try_into()
            .expect("the validated offset section contains complete values"),
    )
}

fn position(bytes: &[u8], cursor: &[u8]) -> usize {
    bytes.len() - cursor.len()
}

fn take<'a>(cursor: &mut &'a [u8], len: usize) -> Result<&'a [u8], String> {
    if cursor.len() < len {
        return Err(format!(
            "lexical bridge is truncated: needed {len} bytes, {} left",
            cursor.len()
        ));
    }
    let (head, tail) = cursor.split_at(len);
    *cursor = tail;
    Ok(head)
}

fn u16_le(cursor: &mut &[u8]) -> Result<u16, String> {
    let bytes = take(cursor, size_of::<u16>())?;
    Ok(u16::from_le_bytes(
        bytes
            .try_into()
            .expect("the parser took exactly one complete u16"),
    ))
}

fn u32_le(cursor: &mut &[u8]) -> Result<u32, String> {
    let bytes = take(cursor, size_of::<u32>())?;
    Ok(u32::from_le_bytes(
        bytes
            .try_into()
            .expect("the parser took exactly one complete u32"),
    ))
}
