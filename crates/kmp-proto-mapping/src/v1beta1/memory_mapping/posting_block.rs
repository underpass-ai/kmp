use super::posting::Posting;
use super::varint::{Reader, push_unsigned};

/// Up to [`PostingBlock::CAPACITY`] postings of one term, ascending by
/// ordinal, stored as varint deltas (DESIGN L6 `LexPost`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PostingBlock {
    postings: Vec<Posting>,
}

/// The layout of an encoded block; a different one is refused.
const BLOCK_LAYOUT: u64 = 1;

impl PostingBlock {
    /// How many postings a block holds before it splits.
    pub const CAPACITY: usize = 128;

    pub fn postings(&self) -> &[Posting] {
        &self.postings
    }

    pub fn is_empty(&self) -> bool {
        self.postings.is_empty()
    }

    pub fn len(&self) -> usize {
        self.postings.len()
    }

    /// The ordinal a block is keyed by: its first posting's.
    pub fn first_ordinal(&self) -> Option<u64> {
        self.postings.first().map(|posting| posting.ordinal)
    }

    /// Sets the counts of one ordinal, adding it in order when new.
    pub fn upsert(&mut self, posting: Posting) {
        match self
            .postings
            .binary_search_by_key(&posting.ordinal, |held| held.ordinal)
        {
            Ok(index) => self.postings[index] = posting,
            Err(index) => self.postings.insert(index, posting),
        }
    }

    /// Takes one ordinal out; absent is not an error.
    pub fn remove(&mut self, ordinal: u64) {
        if let Ok(index) = self
            .postings
            .binary_search_by_key(&ordinal, |held| held.ordinal)
        {
            self.postings.remove(index);
        }
    }

    /// Splits an over-full block into blocks of at most `CAPACITY`, in order.
    pub fn split(self) -> Vec<Self> {
        if self.postings.len() <= Self::CAPACITY {
            return vec![self];
        }
        self.postings
            .chunks(Self::CAPACITY)
            .map(|chunk| Self {
                postings: chunk.to_vec(),
            })
            .collect()
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut buffer = Vec::with_capacity(2 + self.postings.len() * 3);
        push_unsigned(&mut buffer, BLOCK_LAYOUT);
        push_unsigned(&mut buffer, self.postings.len() as u64);
        let mut previous = 0u64;
        for posting in &self.postings {
            push_unsigned(&mut buffer, posting.ordinal - previous);
            push_unsigned(&mut buffer, posting.content);
            push_unsigned(&mut buffer, posting.direct);
            previous = posting.ordinal;
        }
        buffer
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let mut reader = Reader::new(bytes);
        let layout = reader.unsigned()?;
        if layout != BLOCK_LAYOUT {
            return Err(format!(
                "posting block layout {layout} is not {BLOCK_LAYOUT}"
            ));
        }
        let count = reader.unsigned()?;
        let mut postings = Vec::with_capacity(count.min(4096) as usize);
        let mut ordinal = 0u64;
        for index in 0..count {
            let delta = reader.unsigned()?;
            if index > 0 && delta == 0 {
                return Err("posting block repeats an ordinal".into());
            }
            ordinal = ordinal
                .checked_add(delta)
                .ok_or_else(|| "posting ordinal overflow".to_string())?;
            postings.push(Posting {
                ordinal,
                content: reader.unsigned()?,
                direct: reader.unsigned()?,
            });
        }
        if !reader.finished() {
            return Err("trailing bytes after a posting block".into());
        }
        Ok(Self { postings })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posting(ordinal: u64) -> Posting {
        Posting {
            ordinal,
            content: ordinal % 3,
            direct: ordinal % 3 + 1,
        }
    }

    #[test]
    fn a_block_keeps_order_and_round_trips() {
        let mut block = PostingBlock::default();
        for ordinal in [9, 2, 40, 7] {
            block.upsert(posting(ordinal));
        }
        block.upsert(Posting {
            ordinal: 7,
            content: 5,
            direct: 6,
        });
        block.remove(40);
        block.remove(41);
        let ordinals = block
            .postings()
            .iter()
            .map(|p| p.ordinal)
            .collect::<Vec<_>>();
        assert_eq!(ordinals, [2, 7, 9]);
        assert_eq!(block.postings()[1].content, 5);
        assert_eq!(
            PostingBlock::decode(&block.encode()).expect("fixture"),
            block
        );
        assert_eq!(block.first_ordinal(), Some(2));
    }

    #[test]
    fn an_over_full_block_splits_in_order() {
        let mut block = PostingBlock::default();
        for ordinal in 1..=300 {
            block.upsert(posting(ordinal));
        }
        let parts = block.split();
        assert_eq!(
            parts.iter().map(PostingBlock::len).collect::<Vec<_>>(),
            [128, 128, 44]
        );
        assert_eq!(parts[1].first_ordinal(), Some(129));
    }

    #[test]
    fn a_corrupt_block_is_refused() {
        let block = PostingBlock {
            postings: vec![posting(3), posting(5)],
        };
        let bytes = block.encode();
        assert!(PostingBlock::decode(&bytes[..bytes.len() - 1]).is_err());
        assert!(PostingBlock::decode(&[1, 2, 3, 0, 0, 0, 0, 0]).is_err());
    }
}
