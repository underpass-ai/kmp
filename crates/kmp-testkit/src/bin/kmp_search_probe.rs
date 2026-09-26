//! Shows what the kernel's tokenizer makes of a text, offline.
//!
//! Reads JSON Lines — `{"about_texts": [...], "text": "..."}` or
//! `{"about_texts": [...], "texts": ["...", ...]}`, with an optional `id` and
//! an optional `carries_search_summary` — from the file named as the only
//! argument, or from standard input when there is none or it is `-`. Writes
//! one JSON line per text to standard output with the layers
//! `kmp_proto_mapping::v1beta1::SearchProbe` reports: informative terms,
//! concept keys, search keys under the about's morphology, identifiers, and
//! and the identifiers the ranker also reads as whole search terms.
//!
//! It opens no store and calls no model; the output is a pure function of
//! the input and the kernel build.
use std::collections::BTreeSet;
use std::fmt;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};

use kmp_proto_mapping::v1beta1::SearchProbe;
use serde::Deserialize;
use serde_json::{Value, json};

/// The schema tag every output line carries.
const OUTPUT_SCHEMA: &str = "kmp.bench.search_probe.v1";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeRequest {
    #[serde(default)]
    id: Option<Value>,
    about_texts: Vec<String>,
    #[serde(default)]
    carries_search_summary: bool,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    texts: Option<Vec<String>>,
}

#[derive(Debug)]
enum ProbeError {
    Io(io::Error),
    Json { line: usize, message: String },
    Texts { line: usize },
    Usage,
}

impl fmt::Display for ProbeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "io: {error}"),
            Self::Json { line, message } => write!(formatter, "line {line}: {message}"),
            Self::Texts { line } => write!(
                formatter,
                "line {line}: give exactly one of `text` or `texts`"
            ),
            Self::Usage => write!(formatter, "usage: kmp_search_probe [INPUT.jsonl|-]"),
        }
    }
}

impl std::error::Error for ProbeError {}

impl From<io::Error> for ProbeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

fn main() {
    if let Err(error) = run(std::env::args().skip(1).collect()) {
        eprintln!("kmp_search_probe: {error}");
        std::process::exit(2);
    }
}

fn run(args: Vec<String>) -> Result<(), ProbeError> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    match args.as_slice() {
        [] => probe_lines(io::stdin().lock(), &mut output),
        [path] if path == "-" => probe_lines(io::stdin().lock(), &mut output),
        [path] => probe_lines(BufReader::new(File::open(path)?), &mut output),
        _ => Err(ProbeError::Usage),
    }
}

/// Probes every request line of `input` and writes one output line per text.
/// Blank lines are skipped; line numbers count from one, blank lines included.
fn probe_lines(input: impl BufRead, output: &mut impl Write) -> Result<(), ProbeError> {
    for (index, line) in input.lines().enumerate() {
        let line_number = index + 1;
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let request: ProbeRequest =
            serde_json::from_str(&line).map_err(|error| ProbeError::Json {
                line: line_number,
                message: error.to_string(),
            })?;
        for record in probe_request(&request, line_number)? {
            serde_json::to_writer(&mut *output, &record).map_err(io::Error::other)?;
            output.write_all(b"\n")?;
        }
    }
    output.flush()?;
    Ok(())
}

fn probe_request(request: &ProbeRequest, line: usize) -> Result<Vec<Value>, ProbeError> {
    let texts = match (&request.text, &request.texts) {
        (Some(text), None) => std::slice::from_ref(text),
        (None, Some(texts)) => texts.as_slice(),
        _ => return Err(ProbeError::Texts { line }),
    };
    let probe = SearchProbe::from_about_texts(
        request.about_texts.iter().map(String::as_str),
        request.carries_search_summary,
    );
    Ok(texts
        .iter()
        .enumerate()
        .map(|(index, text)| {
            let terms = probe.probe(text);
            json!({
                "schema": OUTPUT_SCHEMA,
                "line": line,
                "id": request.id,
                "index": index,
                "text": text,
                "language": probe.language(),
                "informative_terms": sorted(&terms.informative_terms),
                "concept_keys": sorted(&terms.concept_keys),
                "search_keys": sorted(&terms.search_keys),
                "identifiers": sorted(&terms.identifiers),
                "compound_identifiers": sorted(&terms.compound_identifiers),
            })
        })
        .collect())
}

fn sorted(terms: &BTreeSet<String>) -> Vec<&str> {
    terms.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_text(input: &str) -> Result<Vec<Value>, ProbeError> {
        let mut output = Vec::new();
        probe_lines(input.as_bytes(), &mut output)?;
        Ok(String::from_utf8(output)
            .expect("utf-8 output")
            .lines()
            .map(|line| serde_json::from_str(line).expect("json line"))
            .collect())
    }

    #[test]
    fn one_line_per_text_with_every_layer() {
        let input = concat!(
            r#"{"id":"q1","about_texts":["The deployment of the gateway was frozen during the audit."],"text":"The old store was relocated for KMP-469."}"#,
            "\n\n",
            r#"{"about_texts":["El despliegue de la pasarela se congelo durante la auditoria."],"texts":["Desplegamos las valvulas.","Las reuniones"]}"#,
            "\n"
        );
        let records = probe_text(input).expect("probe");

        assert_eq!(records.len(), 3);
        let first = &records[0];
        assert_eq!(first["schema"], OUTPUT_SCHEMA);
        assert_eq!(first["id"], "q1");
        assert_eq!(first["line"], 1);
        assert_eq!(first["language"], "english");
        let search_keys = first["search_keys"].as_array().expect("array");
        assert!(search_keys.contains(&json!("concept:movement")));
        assert!(search_keys.contains(&json!("concept:historical")));
        assert!(
            first["identifiers"]
                .as_array()
                .expect("array")
                .contains(&json!("kmp-469"))
        );
        assert_eq!(first["compound_identifiers"], json!(["kmp.469"]));
        assert!(search_keys.contains(&json!("kmp.469")));

        assert_eq!(records[1]["line"], 3);
        assert_eq!(records[1]["id"], Value::Null);
        assert_eq!(records[1]["language"], "spanish");
        assert_eq!(records[2]["index"], 1);
        assert_eq!(records[2]["informative_terms"], json!(["reuniones"]));
    }

    #[test]
    fn a_summary_bearing_mixed_about_falls_back_to_the_kernel_language() {
        let input = r#"{"about_texts":[],"carries_search_summary":true,"text":"valves"}"#;
        let records = probe_text(input).expect("probe");
        assert_eq!(records[0]["language"], "english");
        assert_eq!(records[0]["search_keys"], json!(["valv"]));
    }

    #[test]
    fn malformed_requests_name_their_line() {
        let both = r#"{"about_texts":[],"text":"a","texts":["b"]}"#;
        assert!(matches!(
            probe_text(both),
            Err(ProbeError::Texts { line: 1 })
        ));
        let neither = "\n{\"about_texts\":[]}";
        assert!(matches!(
            probe_text(neither),
            Err(ProbeError::Texts { line: 2 })
        ));
        let broken = r#"{"about_texts": "#;
        let error = probe_text(broken).expect_err("broken json");
        assert!(matches!(error, ProbeError::Json { line: 1, .. }));
        assert!(error.to_string().starts_with("line 1:"));
        let unknown = r#"{"about_texts":[],"text":"a","extra":1}"#;
        assert!(matches!(
            probe_text(unknown),
            Err(ProbeError::Json { line: 1, .. })
        ));
    }

    #[test]
    fn run_reads_a_file_and_rejects_extra_arguments() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("input.jsonl");
        std::fs::write(&path, r#"{"about_texts":[],"text":"x"}"#).expect("write");
        run(vec![path.display().to_string()]).expect("file input");
        assert!(matches!(
            run(vec!["a".into(), "b".into()]),
            Err(ProbeError::Usage)
        ));
        assert!(matches!(
            run(vec![directory.path().join("missing").display().to_string()]),
            Err(ProbeError::Io(_))
        ));
        assert!(ProbeError::Usage.to_string().starts_with("usage:"));
        assert!(
            ProbeError::Io(io::Error::other("x"))
                .to_string()
                .starts_with("io:")
        );
    }
}
