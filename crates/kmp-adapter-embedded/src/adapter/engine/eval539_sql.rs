//! Experimental trace_v2 observer. Triggers are disabled by the engine; each
//! SQLite connection runs serially on its current worker thread. The stack is
//! bounded by in-flight statements, not historical SQL text or statement keys.
use kmp_domain::eval539_profile;
use rusqlite::{
    Connection, StatementStatus,
    trace::{TraceEvent, TraceEventCodes},
};
use std::cell::RefCell;

thread_local! {
    static VM_BASELINES: RefCell<Vec<i32>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn install(connection: &Connection) {
    if eval539_profile::enabled() {
        connection.trace_v2(
            TraceEventCodes::SQLITE_TRACE_STMT
                | TraceEventCodes::SQLITE_TRACE_ROW
                | TraceEventCodes::SQLITE_TRACE_PROFILE,
            Some(observe),
        );
    }
}

fn observe(event: TraceEvent<'_>) {
    match event {
        TraceEvent::Stmt(statement, _) => {
            // sqlite3_reset does not clear VM_STEP. Subtract the status seen at
            // this execution's STMT event, never sum lifetime status repeatedly.
            VM_BASELINES.with(|s| {
                s.borrow_mut()
                    .push(statement.get_status(StatementStatus::VmStep))
            });
            eval539_profile::sql(|s| s.statements += 1);
        }
        TraceEvent::Row(_) => eval539_profile::sql(|s| s.rows += 1),
        TraceEvent::Profile(statement, duration) => {
            let baseline = VM_BASELINES.with(|s| s.borrow_mut().pop());
            let current = statement.get_status(StatementStatus::VmStep);
            eval539_profile::sql(|s| {
                s.profiles += 1;
                s.profile_ns += duration.as_nanos() as u64;
                if let Some(delta) = baseline
                    .and_then(|v| current.checked_sub(v))
                    .filter(|v| *v >= 0)
                {
                    s.vm_steps += delta as u64;
                } else {
                    s.vm_step_invalid_deltas += 1;
                }
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepared_cached_vm_work_is_per_execution_and_partial_rows_are_counted() {
        assert!(eval539_profile::enabled(), "run with EVAL539_PROFILE=1");
        let connection = Connection::open_in_memory().unwrap();
        install(&connection);
        let mut observed = Vec::new();
        for _ in 0..3 {
            assert!(eval539_profile::begin());
            {
                let mut statement = connection
                    .prepare_cached("SELECT 1 UNION ALL SELECT 2 UNION ALL SELECT 3")
                    .unwrap();
                let mut rows = statement.query([]).unwrap();
                assert!(rows.next().unwrap().is_some());
                // Deliberately stop before SQLITE_DONE: resetting/dropping must
                // still report exactly the VM work and row consumed this run.
            }
            let result = eval539_profile::finish();
            assert_eq!(result.sql.statements, 1);
            assert_eq!(result.sql.profiles, 1);
            assert_eq!(result.sql.rows, 1);
            assert_eq!(result.sql.vm_step_invalid_deltas, 0);
            assert!(result.sql.vm_steps > 0);
            observed.push(result.sql.vm_steps);
        }
        assert_eq!(observed[0], observed[1]);
        assert_eq!(observed[1], observed[2]);
    }
}
