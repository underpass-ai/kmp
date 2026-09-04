//! Scores `kmp_relate` against a judged collection, offline and without a
//! model.
//!
//! A relate reading is not a citation but a set: the facts that fell in the
//! span, the relations each about declared between them, the coordinate
//! relations read between abouts, and the tensions that still stand. A
//! reader judges each set, and the score is precision and recall per set —
//! what came back that should have, what should have and did not — which
//! is arithmetic and therefore something CI can hold.
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use kmp_mcp::KernelMcpServer;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
struct JudgedCollection {
    cases: Vec<JudgedCase>,
}

#[derive(Debug, Deserialize)]
struct JudgedCase {
    id: String,
    probes: String,
    about: String,
    #[serde(default)]
    dimensions: Option<Value>,
    #[serde(default)]
    interval: Option<Value>,
    #[serde(default)]
    axis: Option<String>,
    /// Every about's memory, each written through its own ingest.
    memories: Vec<SeededMemory>,
    expected: Expected,
}

#[derive(Debug, Deserialize)]
struct SeededMemory {
    about: String,
    memory: Value,
}

/// What a reader judged the reading should hold. A section left out is
/// judged empty: nothing of that kind should come back.
#[derive(Debug, Deserialize, Default)]
struct Expected {
    #[serde(default)]
    facts: Vec<String>,
    /// `[from, rel, to]`.
    #[serde(default)]
    declared: Vec<[String; 3]>,
    /// `[from, kind, to]`.
    #[serde(default)]
    coordinate: Vec<[String; 3]>,
    /// `[ref, other]`.
    #[serde(default)]
    tensions: Vec<[String; 2]>,
    /// The lifecycle state a fact must carry, by ref.
    #[serde(default)]
    states: std::collections::BTreeMap<String, String>,
    /// The ref the proof must name as nearest outside an empty span.
    #[serde(default)]
    nearest_outside: Option<String>,
}

#[derive(Debug, Clone, Copy, Default)]
struct SectionScore {
    precision: f64,
    recall: f64,
}

fn score_section(expected: &BTreeSet<String>, got: &BTreeSet<String>) -> SectionScore {
    let hits = expected.intersection(got).count() as f64;
    SectionScore {
        precision: if got.is_empty() {
            f64::from(expected.is_empty())
        } else {
            hits / got.len() as f64
        },
        recall: if expected.is_empty() {
            f64::from(got.is_empty())
        } else {
            hits / expected.len() as f64
        },
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Outcome {
    facts: SectionScore,
    declared: SectionScore,
    coordinate: SectionScore,
    tensions: SectionScore,
    /// 1 when every judged state matched and the nearest-outside expectation
    /// held, 0 otherwise.
    states_and_nearest: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let cases_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "crates/kmp-testkit/judged/relate_cases.json".to_string()),
    );
    let baseline_path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "docs/development/relate-baseline.tsv".to_string()),
    );
    let record = std::env::var("RELATE_BASELINE").as_deref() == Ok("write");

    let collection: JudgedCollection = serde_json::from_str(&fs::read_to_string(&cases_path)?)?;
    let mut outcomes = Vec::new();
    println!(
        "{:<34} {:>9} {:>9} {:>9} {:>9} {:>6}  note",
        "case", "facts P/R", "decl P/R", "coord P/R", "tens P/R", "state"
    );
    for case in &collection.cases {
        let outcome = run_case(case).await?;
        let cell = |score: SectionScore| format!("{:.2}/{:.2}", score.precision, score.recall);
        println!(
            "{:<34} {:>9} {:>9} {:>9} {:>9} {:>6}",
            case.id,
            cell(outcome.facts),
            cell(outcome.declared),
            cell(outcome.coordinate),
            cell(outcome.tensions),
            if outcome.states_and_nearest >= 1.0 {
                "yes"
            } else {
                "no"
            }
        );
        let perfect = [
            outcome.facts,
            outcome.declared,
            outcome.coordinate,
            outcome.tensions,
        ]
        .iter()
        .all(|score| score.precision >= 1.0 && score.recall >= 1.0)
            && outcome.states_and_nearest >= 1.0;
        if !perfect {
            println!("{:<34} {:>41}  {}", "", "", case.probes);
        }
        outcomes.push(outcome);
    }

    let columns = scorecard(&outcomes);
    println!("\n{} cases", outcomes.len());
    for (name, value) in &columns {
        println!("  {name:<24} {value:.4}");
    }

    if record {
        write_baseline(&baseline_path, outcomes.len(), &columns)?;
        println!("\nrecorded baseline at {}", baseline_path.display());
        return Ok(());
    }
    enforce_baseline(&baseline_path, outcomes.len(), &columns)
}

async fn run_case(case: &JudgedCase) -> Result<Outcome, Box<dyn Error>> {
    let data_dir = std::env::temp_dir().join(format!("kmp-relate-{}", case.id));
    let _ = fs::remove_dir_all(&data_dir);
    fs::create_dir_all(&data_dir)?;
    let server = KernelMcpServer::embedded(&data_dir)?;
    for (index, seeded) in case.memories.iter().enumerate() {
        call(
            &server,
            1 + index as u64,
            "kmp_ingest",
            json!({
                "about": seeded.about,
                "idempotency_key": format!("judged:{}:{}", case.id, seeded.about),
                "memory": seeded.memory
            }),
        )
        .await?;
    }

    let mut facts = BTreeSet::new();
    let mut states = std::collections::BTreeMap::new();
    let mut declared = BTreeSet::new();
    let mut coordinate = BTreeSet::new();
    let mut tensions = BTreeSet::new();
    let mut nearest = None;
    let mut cursor: Option<String> = None;
    let mut id = 100;
    loop {
        let mut arguments = json!({
            "about": case.about,
            "budget": {"depth": 3},
            "page": {"entries": 64}
        });
        if let Some(dimensions) = &case.dimensions {
            arguments["dimensions"] = dimensions.clone();
        }
        if let Some(interval) = &case.interval {
            arguments["interval"] = interval.clone();
        }
        if let Some(axis) = &case.axis {
            arguments["axis"] = json!(axis);
        }
        if let Some(cursor) = &cursor {
            arguments["page"]["cursor"] = json!(cursor);
        }
        let page = call(&server, id, "kmp_relate", arguments).await?;
        id += 1;
        for fact in page["facts"].as_array().into_iter().flatten() {
            let reference = fact["ref"].as_str().unwrap_or_default().to_string();
            states.insert(
                reference.clone(),
                fact["state"].as_str().unwrap_or_default().to_string(),
            );
            facts.insert(reference);
        }
        for relation in page["declared"].as_array().into_iter().flatten() {
            declared.insert(triple(&relation["from"], &relation["rel"], &relation["to"]));
        }
        for relation in page["coordinate"].as_array().into_iter().flatten() {
            coordinate.insert(triple(
                &relation["from"],
                &relation["kind"],
                &relation["to"],
            ));
        }
        for tension in page["tensions"].as_array().into_iter().flatten() {
            tensions.insert(format!(
                "{} {}",
                tension["ref"].as_str().unwrap_or_default(),
                tension["other"].as_str().unwrap_or_default()
            ));
        }
        if nearest.is_none() {
            nearest = page["proof"]["nearest_outside"]["ref"]
                .as_str()
                .map(str::to_string);
        }
        match page["page"]["next_cursor"].as_str() {
            Some(next) if page["page"]["has_more"] == true => cursor = Some(next.to_string()),
            _ => break,
        }
    }
    let _ = fs::remove_dir_all(&data_dir);

    let expected = &case.expected;
    let states_hold = expected
        .states
        .iter()
        .all(|(reference, state)| states.get(reference) == Some(state));
    let nearest_holds = expected.nearest_outside == nearest;
    Ok(Outcome {
        facts: score_section(&expected.facts.iter().cloned().collect(), &facts),
        declared: score_section(
            &expected
                .declared
                .iter()
                .map(|[from, rel, to]| format!("{from} {rel} {to}"))
                .collect(),
            &declared,
        ),
        coordinate: score_section(
            &expected
                .coordinate
                .iter()
                .map(|[from, kind, to]| format!("{from} {kind} {to}"))
                .collect(),
            &coordinate,
        ),
        tensions: score_section(
            &expected
                .tensions
                .iter()
                .map(|[reference, other]| format!("{reference} {other}"))
                .collect(),
            &tensions,
        ),
        states_and_nearest: f64::from(states_hold && nearest_holds),
    })
}

fn triple(from: &Value, middle: &Value, to: &Value) -> String {
    format!(
        "{} {} {}",
        from.as_str().unwrap_or_default(),
        middle.as_str().unwrap_or_default(),
        to.as_str().unwrap_or_default()
    )
}

fn scorecard(outcomes: &[Outcome]) -> Vec<(&'static str, f64)> {
    let mean = |value: &dyn Fn(&Outcome) -> f64| {
        if outcomes.is_empty() {
            0.0
        } else {
            outcomes.iter().map(value).sum::<f64>() / outcomes.len() as f64
        }
    };
    vec![
        ("facts_precision", mean(&|o| o.facts.precision)),
        ("facts_recall", mean(&|o| o.facts.recall)),
        ("declared_precision", mean(&|o| o.declared.precision)),
        ("declared_recall", mean(&|o| o.declared.recall)),
        ("coordinate_precision", mean(&|o| o.coordinate.precision)),
        ("coordinate_recall", mean(&|o| o.coordinate.recall)),
        ("tensions_precision", mean(&|o| o.tensions.precision)),
        ("tensions_recall", mean(&|o| o.tensions.recall)),
        ("states_and_nearest", mean(&|o| o.states_and_nearest)),
    ]
}

async fn call(
    server: &KernelMcpServer,
    id: u64,
    name: &str,
    arguments: Value,
) -> Result<Value, Box<dyn Error>> {
    let request = json!({
        "jsonrpc": "2.0", "id": id, "method": "tools/call",
        "params": {"name": name, "arguments": arguments}
    })
    .to_string();
    let response = server
        .handle_json_line(&request)
        .await
        .ok_or_else(|| format!("tool `{name}` produced no response"))?;
    let value: Value = serde_json::from_str(&response)?;
    if value["result"]["isError"].as_bool() == Some(true) {
        return Err(format!("tool `{name}` failed: {}", value["result"]).into());
    }
    Ok(value["result"]["structuredContent"].clone())
}

fn write_baseline(
    path: &Path,
    cases: usize,
    columns: &[(&'static str, f64)],
) -> Result<(), Box<dyn Error>> {
    let mut out = String::from(
        "# Relate baseline: the floors `scripts/ci/relate-baseline.sh` holds `kmp_relate` to.\n\
         # Each row is the mean over the judged collection of one precision or recall per\n\
         # section — facts, declared, coordinate, tensions — plus the share of cases whose\n\
         # judged lifecycle states and nearest-outside expectation held. A number may rise\n\
         # freely; lowering one is a reviewed change that says why. `cases` is exact, so a\n\
         # case added or removed is a deliberate refresh, never a silent one.\n\
         # Refresh with: RELATE_BASELINE=write bash scripts/ci/relate-baseline.sh\n\
         metric\tfloor\n",
    );
    out.push_str(&format!("cases\t{cases}\n"));
    for (name, value) in columns {
        // Floored, not rounded, so a refresh never records a floor above
        // what was measured.
        out.push_str(&format!(
            "{name}\t{:.4}\n",
            (value * 10_000.0).floor() / 10_000.0
        ));
    }
    fs::write(path, out)?;
    Ok(())
}

fn enforce_baseline(
    path: &Path,
    cases: usize,
    columns: &[(&'static str, f64)],
) -> Result<(), Box<dyn Error>> {
    let recorded = fs::read_to_string(path)
        .map_err(|error| format!("no relate baseline at {}: {error}", path.display()))?;
    let mut floors = std::collections::BTreeMap::new();
    for line in recorded.lines() {
        if line.starts_with('#') || line.starts_with("metric") || line.trim().is_empty() {
            continue;
        }
        let (name, value) = line
            .split_once('\t')
            .ok_or_else(|| format!("malformed baseline row: {line}"))?;
        floors.insert(name.to_string(), value.trim().parse::<f64>()?);
    }
    let mut regressions = Vec::new();
    if let Some(recorded_cases) = floors.get("cases")
        && (*recorded_cases as usize) != cases
    {
        regressions.push(format!(
            "the collection changed size: {recorded_cases} judged cases recorded, {cases} run"
        ));
    }
    for (name, measured) in columns {
        if let Some(floor) = floors.get(*name)
            && measured + 1e-9 < *floor
        {
            regressions.push(format!("{name} fell to {measured:.4}, below {floor:.4}"));
        }
    }
    if regressions.is_empty() {
        println!("\nrelate baseline holds");
        Ok(())
    } else {
        Err(format!("relate quality regressed:\n  {}", regressions.join("\n  ")).into())
    }
}
