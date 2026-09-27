//! LEB128 varints and zigzag, the byte form the lexical sidecar keeps its
//! rows, postings and language signals in.

pub(super) fn push_unsigned(buffer: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        buffer.push((value as u8) | 0x80);
        value >>= 7;
    }
    buffer.push(value as u8);
}

pub(super) fn push_signed(buffer: &mut Vec<u8>, value: i64) {
    push_unsigned(buffer, ((value << 1) ^ (value >> 63)) as u64);
}

pub(super) fn push_bytes(buffer: &mut Vec<u8>, bytes: &[u8]) {
    push_unsigned(buffer, bytes.len() as u64);
    buffer.extend_from_slice(bytes);
}

/// Reads what the `push_*` functions wrote, refusing a truncated or
/// overlong value instead of guessing.
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(super) fn unsigned(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self
                .bytes
                .get(self.at)
                .ok_or_else(|| "truncated varint".to_string())?;
            self.at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("overlong varint".into())
    }

    pub(super) fn signed(&mut self) -> Result<i64, String> {
        let raw = self.unsigned()?;
        Ok(((raw >> 1) as i64) ^ -((raw & 1) as i64))
    }

    pub(super) fn bytes(&mut self) -> Result<&'a [u8], String> {
        let length = usize::try_from(self.unsigned()?).map_err(|_| "length overflow")?;
        let end = self
            .at
            .checked_add(length)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| "truncated bytes".to_string())?;
        let bytes = &self.bytes[self.at..end];
        self.at = end;
        Ok(bytes)
    }

    pub(super) fn finished(&self) -> bool {
        self.at == self.bytes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_round_trips() {
        let mut buffer = Vec::new();
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            push_unsigned(&mut buffer, value);
        }
        for value in [0i64, -1, 1, -64, 64, i64::MIN, i64::MAX] {
            push_signed(&mut buffer, value);
        }
        push_bytes(&mut buffer, b"valkey");
        let mut reader = Reader::new(&buffer);
        for value in [0u64, 1, 127, 128, 300, u64::from(u32::MAX), u64::MAX] {
            assert_eq!(reader.unsigned().expect("fixture"), value);
        }
        for value in [0i64, -1, 1, -64, 64, i64::MIN, i64::MAX] {
            assert_eq!(reader.signed().expect("fixture"), value);
        }
        assert_eq!(reader.bytes().expect("fixture"), b"valkey");
        assert!(reader.finished());
    }

    #[test]
    fn a_truncated_value_is_refused() {
        assert!(Reader::new(&[0x80]).unsigned().is_err());
        assert!(Reader::new(&[5, b'a']).bytes().is_err());
        assert!(Reader::new(&[0xff; 11]).unsigned().is_err());
    }
}
