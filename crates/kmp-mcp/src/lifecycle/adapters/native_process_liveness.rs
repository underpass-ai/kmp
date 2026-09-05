use crate::lifecycle::ports::process_liveness::ProcessLiveness;

/// Asks this platform whether a process id is alive.
///
/// Every branch answers the same question with the cheapest thing the
/// platform offers and never signals anything: uninstall reads liveness to
/// decide what to report, and a verb that removes files has no business
/// touching a process it did not start.
pub struct NativeProcessLiveness;

impl ProcessLiveness for NativeProcessLiveness {
    fn is_running(&self, pid: u32) -> bool {
        Self::running(pid)
    }
}

impl NativeProcessLiveness {
    #[cfg(target_os = "linux")]
    fn running(pid: u32) -> bool {
        // procfs answers without spawning anything, which matters because a
        // survey asks this once per marker.
        std::path::Path::new("/proc").join(pid.to_string()).exists()
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    fn running(pid: u32) -> bool {
        // No procfs here. Signal zero performs the existence and permission
        // checks and delivers nothing, which is exactly the question.
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    #[cfg(windows)]
    fn running(pid: u32) -> bool {
        // `tasklist` prints a header either way, so the id has to be found in
        // the output rather than inferred from the exit status.
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout).contains(&format!("\"{pid}\""))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::NativeProcessLiveness;
    use crate::lifecycle::ports::process_liveness::ProcessLiveness;

    #[test]
    fn this_very_process_is_running() {
        assert!(NativeProcessLiveness.is_running(std::process::id()));
    }

    #[test]
    fn a_process_id_that_cannot_be_allocated_is_not_running() {
        // Above every platform's pid_max, so it is not a race against a real
        // process that happened to take the number.
        assert!(!NativeProcessLiveness.is_running(u32::MAX - 1));
    }
}
