//! Experimental #539 instrumentation, not a product API. The process must serve
//! one serial RPC with no background work. Inclusive spans may cross threads.
//! This neutral, standard-library collector deliberately lives below adapters.
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use serde::Serialize;

static ENABLED: OnceLock<bool> = OnceLock::new();
static ACTIVE: AtomicBool = AtomicBool::new(false);
static STATE: Mutex<Option<State>> = Mutex::new(None);

#[derive(Default, Serialize)]
pub struct Totals {
    pub phases: Vec<Phase>,
    pub logical_reads: Vec<LogicalRead>,
    pub sql: Sql,
    pub bodies: Bodies,
    pub nesting_errors: u64,
}

#[derive(Default, Serialize)]
pub struct Phase {
    pub path: Vec<&'static str>,
    pub calls: u64,
    pub inclusive_ns: u64,
}

#[derive(Default, Serialize)]
pub struct LogicalRead {
    pub table: String,
    pub operation: &'static str,
    pub calls: u64,
    pub rows: u64,
    pub value_bytes: u64,
    pub key_bytes: u64,
    pub errors: u64,
}

#[derive(Default, Serialize)]
pub struct Sql {
    pub statements: u64,
    pub rows: u64,
    pub profiles: u64,
    pub vm_steps: u64,
    pub profile_ns: u64,
    pub vm_step_invalid_deltas: u64,
}

#[derive(Default, Serialize)]
pub struct Bodies {
    pub single_reads: u64,
    pub batches: u64,
    pub slots: u64,
    pub found: u64,
    pub detail_bytes: u64,
}

#[derive(Default)]
struct State {
    phases: BTreeMap<Vec<&'static str>, Phase>,
    logical: BTreeMap<(String, &'static str), LogicalRead>,
    stack: Vec<(u64, &'static str)>,
    next_id: u64,
    totals: Totals,
}

pub fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("EVAL539_PROFILE").is_ok_and(|v| v == "1"))
}

pub fn active() -> bool {
    ACTIVE.load(Ordering::Relaxed)
}

pub fn begin() -> bool {
    if !enabled() {
        return false;
    }
    let mut state = STATE.lock().expect("eval539 collector lock");
    assert!(state.is_none(), "eval539 requires serial isolated RPCs");
    *state = Some(State::default());
    ACTIVE.store(true, Ordering::Relaxed);
    true
}

pub fn finish() -> Totals {
    ACTIVE.store(false, Ordering::Relaxed);
    let mut state = STATE
        .lock()
        .expect("eval539 collector lock")
        .take()
        .expect("active collector");
    state.totals.nesting_errors += state.stack.len() as u64;
    state.totals.phases = state.phases.into_values().collect();
    state.totals.logical_reads = state.logical.into_values().collect();
    state.totals
}

pub struct Span(Option<(Instant, u64, Vec<&'static str>)>);

pub fn span(name: &'static str) -> Span {
    if !active() {
        return Span(None);
    }
    let started = Instant::now();
    let mut state = STATE.lock().expect("eval539 collector lock");
    let state = state.as_mut().expect("active collector");
    let id = state.next_id;
    state.next_id += 1;
    state.stack.push((id, name));
    let path = state.stack.iter().map(|(_, name)| *name).collect();
    Span(Some((started, id, path)))
}

impl Drop for Span {
    fn drop(&mut self) {
        let Some((started, id, path)) = self.0.take() else {
            return;
        };
        let elapsed = started.elapsed().as_nanos() as u64;
        let mut state = STATE.lock().expect("eval539 collector lock");
        let Some(state) = state.as_mut() else { return };
        if state.stack.last().map(|(id, _)| *id) != Some(id) {
            state.totals.nesting_errors += 1;
        }
        state.stack.retain(|(open, _)| *open != id);
        let phase = state.phases.entry(path.clone()).or_insert_with(|| Phase {
            path,
            ..Phase::default()
        });
        phase.calls += 1;
        phase.inclusive_ns += elapsed;
    }
}

pub fn logical(
    table: String,
    operation: &'static str,
    rows: u64,
    value_bytes: u64,
    key_bytes: u64,
    error: bool,
) {
    if !active() {
        return;
    }
    let mut state = STATE.lock().expect("eval539 collector lock");
    let Some(state) = state.as_mut() else { return };
    let read = state
        .logical
        .entry((table.clone(), operation))
        .or_insert_with(|| LogicalRead {
            table,
            operation,
            ..LogicalRead::default()
        });
    read.calls += 1;
    read.rows += rows;
    read.value_bytes += value_bytes;
    read.key_bytes += key_bytes;
    read.errors += u64::from(error);
}

pub fn sql(update: impl FnOnce(&mut Sql)) {
    if !active() {
        return;
    }
    if let Some(state) = STATE.lock().expect("eval539 collector lock").as_mut() {
        update(&mut state.totals.sql);
    }
}

pub fn bodies(slots: usize, values: &[Option<crate::NodeDetailProjection>], single: bool) {
    if !active() {
        return;
    }
    if let Some(state) = STATE.lock().expect("eval539 collector lock").as_mut() {
        if single {
            state.totals.bodies.single_reads += 1;
        } else {
            state.totals.bodies.batches += 1;
        }
        state.totals.bodies.slots += slots as u64;
        for body in values.iter().flatten() {
            state.totals.bodies.found += 1;
            state.totals.bodies.detail_bytes += body.detail.len() as u64;
        }
    }
}
