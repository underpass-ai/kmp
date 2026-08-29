use std::io::{self, Read};
use std::process::{Child, Output};
use std::thread;

use wait_timeout::ChildExt;

use crate::lifecycle::domain::process_timeout::ProcessTimeout;

/// Waits for a child without allowing either output pipe to back-pressure it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BoundedChild;

impl BoundedChild {
    pub fn wait(mut child: Child, timeout: ProcessTimeout) -> io::Result<(Output, bool)> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("stdout pipe is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("stderr pipe is unavailable"))?;
        let stdout_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut stdout = stdout;
            stdout.read_to_end(&mut bytes).map(|_| bytes)
        });
        let stderr_reader = thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut stderr = stderr;
            stderr.read_to_end(&mut bytes).map(|_| bytes)
        });

        let waited = child.wait_timeout(timeout.duration())?;
        let timed_out = waited.is_none();
        let status = match waited {
            Some(status) => status,
            None => {
                let _ = child.kill();
                child.wait()?
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| io::Error::other("stdout reader panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| io::Error::other("stderr reader panicked"))??;
        Ok((
            Output {
                status,
                stdout,
                stderr,
            },
            timed_out,
        ))
    }
}
