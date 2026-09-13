//! #773 diagnostic for the format-v1 work the original reader performed.
//!
//! Compile with the same unoptimized, line-table development settings as the
//! workspace, then pass the shipped `.kmpb`, a sample count and an output path. Each row times
//! file read, offset decoding, word copy, vector copy, word validation and
//! eager vector normalization separately. This deliberately mirrors the old
//! reader instead of calling the candidate reader, because the candidate
//! removes both copies and defers normalization per word.

use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::time::Instant;

fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1_000.0
}

fn take<'a>(cursor: &mut &'a [u8], len: usize) -> &'a [u8] {
    let (head, tail) = cursor.split_at(len);
    *cursor = tail;
    head
}

fn u16_le(cursor: &mut &[u8]) -> u16 {
    u16::from_le_bytes(take(cursor, 2).try_into().expect("u16"))
}

fn u32_le(cursor: &mut &[u8]) -> u32 {
    u32::from_le_bytes(take(cursor, 4).try_into().expect("u32"))
}

fn sample(path: &Path, iteration: usize, output: &mut impl Write) {
    let started = Instant::now();
    let bytes = std::fs::read(path).expect("bridge read");
    let read_ms = elapsed_ms(started);

    let started = Instant::now();
    let mut cursor = bytes.as_slice();
    assert_eq!(take(&mut cursor, 8), b"KMPBRIDG");
    assert_eq!(u16_le(&mut cursor), 1);
    let dims = usize::from(u16_le(&mut cursor));
    let count = u32_le(&mut cursor) as usize;
    let provenance_len = usize::from(u16_le(&mut cursor));
    black_box(take(&mut cursor, provenance_len));
    let mut offsets = Vec::with_capacity(count + 1);
    for _ in 0..=count {
        offsets.push(u32_le(&mut cursor));
    }
    let decode_offsets_ms = elapsed_ms(started);

    let started = Instant::now();
    let words = take(&mut cursor, offsets[count] as usize).to_vec();
    let copy_words_ms = elapsed_ms(started);

    let started = Instant::now();
    let vectors = take(&mut cursor, count * dims)
        .iter()
        .map(|byte| *byte as i8)
        .collect::<Vec<_>>();
    let copy_vectors_ms = elapsed_ms(started);
    assert!(cursor.is_empty());

    let started = Instant::now();
    for index in 0..count {
        let word = &words[offsets[index] as usize..offsets[index + 1] as usize];
        assert!(std::str::from_utf8(word).is_ok());
        if index > 0 {
            let previous = &words[offsets[index - 1] as usize..offsets[index] as usize];
            assert!(previous < word);
        }
    }
    let validate_words_ms = elapsed_ms(started);

    let started = Instant::now();
    let norms = (0..count)
        .map(|index| {
            vectors[index * dims..(index + 1) * dims]
                .iter()
                .map(|value| u64::from(value.unsigned_abs()).pow(2))
                .sum::<u64>()
        })
        .collect::<Vec<_>>();
    let normalize_vectors_ms = elapsed_ms(started);
    black_box((bytes, offsets, words, vectors, norms));

    writeln!(
        output,
        "{{\"iteration\":{iteration},\"read_ms\":{read_ms},\"decode_offsets_ms\":{decode_offsets_ms},\"copy_words_ms\":{copy_words_ms},\"copy_vectors_ms\":{copy_vectors_ms},\"validate_words_ms\":{validate_words_ms},\"normalize_vectors_ms\":{normalize_vectors_ms}}}"
    )
    .expect("phase sample write");
}

fn main() {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().expect("bridge path");
    let samples = args
        .next()
        .expect("sample count")
        .to_string_lossy()
        .parse::<usize>()
        .expect("numeric sample count");
    let output = args.next().expect("output path");
    assert!(args.next().is_none(), "unexpected argument");
    let mut output = BufWriter::new(std::fs::File::create(output).expect("output create"));
    for iteration in 0..samples {
        sample(Path::new(&path), iteration, &mut output);
    }
}
