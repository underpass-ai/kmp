//! Exact JSON byte sizing without retaining an intermediate output buffer.

use std::io::{self, Write};

use serde::Serialize;

#[derive(Default)]
struct JsonByteCounter {
    bytes: usize,
}

impl Write for JsonByteCounter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(buffer.len())
            .expect("serialized recall projection size should fit usize");
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn serialized_size(value: &impl Serialize) -> usize {
    let mut counter = JsonByteCounter::default();
    serde_json::to_writer(&mut counter, value).expect("recall projection should serialize");
    counter.bytes
}
