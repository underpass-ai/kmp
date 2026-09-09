pub(super) fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    encoded
}

pub(super) fn decode_component(value: &str) -> Option<String> {
    let mut bytes = Vec::new();
    let mut encoded = value.bytes();
    while let Some(byte) = encoded.next() {
        if byte == b'%' {
            let high = char::from(encoded.next()?).to_digit(16)?;
            let low = char::from(encoded.next()?).to_digit(16)?;
            bytes.push(u8::try_from((high << 4) | low).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    // One ref spelling per label: reject malformed/non-canonical escapes.
    (encode_component(&decoded) == value).then_some(decoded)
}
