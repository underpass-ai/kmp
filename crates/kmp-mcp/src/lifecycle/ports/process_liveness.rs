/// Outbound port for whether a process id is still running.
///
/// Narrow on purpose. Host lock files name a process, and a name is not a
/// hold: the marker outlives a host that crashed, and acting on a dead one
/// would refuse a removal forever with no way out. Asking the platform is
/// the only way to tell a live holder from a leftover file, and it is the
/// one thing a survey cannot do from paths alone.
pub trait ProcessLiveness: Send + Sync {
    fn is_running(&self, pid: u32) -> bool;
}
