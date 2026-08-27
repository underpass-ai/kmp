//! A pulse crossing a three-node graph: what the terminal shows while the
//! CLI is saving memory, bringing it back, or replaying it onto another
//! engine.
//!
//! Decorative only, and gone without a trace. Frames go to stderr behind
//! `\r`, only when stderr is a styled terminal, and the line is erased
//! before anything else prints — so a script, a CI log or a plugin host
//! sees exactly the bytes it saw before this file existed. `NO_COLOR`
//! silences the pulse too: someone who asked for a quiet terminal asked
//! for all of it.

use std::io::Write as _;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::banner;
use crate::style::{self, Style};

/// One memory travelling the graph, back and forth.
const FRAMES: [&str; 4] = ["●──○──○", "○──●──○", "○──○──●", "○──●──○"];
const FRAME_EVERY: std::time::Duration = std::time::Duration::from_millis(120);

/// One full crossing of the graph: every frame once. Work that finishes
/// faster still gets this much screen time — anything shorter reads as a
/// flicker, not an animation, and a human cannot tell what just blinked.
/// Only the animated pulse waits; a pipe never pays it.
const MIN_VISIBLE: std::time::Duration = std::time::Duration::from_millis(480);

pub struct Pulse {
    running: Option<(
        Arc<AtomicBool>,
        std::thread::JoinHandle<()>,
        std::time::Instant,
    )>,
}

impl Pulse {
    /// Starts the pulse when a human is watching; does nothing otherwise.
    pub fn start(label: &str) -> Self {
        match Style::for_stderr() {
            Style::Ansi => Self::animated(label.to_string()),
            Style::Plain => Self::inert(),
        }
    }

    /// The pulse that never draws. Tests use it directly so they cannot
    /// depend on whether the test runner holds a terminal.
    pub fn inert() -> Self {
        Self { running: None }
    }

    fn animated(label: String) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let seen = stop.clone();
        let handle = std::thread::spawn(move || {
            // The travelling node takes the accent; the rest of the little
            // graph stays quiet. Painted once, not per frame.
            let frames: Vec<String> = FRAMES
                .iter()
                .map(|frame| frame.replace('●', &Style::Ansi.rgb(banner::ACCENT, "●")))
                .collect();
            let mut stderr = std::io::stderr();
            let mut at = 0usize;
            while !seen.load(Ordering::Relaxed) {
                let _ = write!(stderr, "\r{} {label}\x1b[K", frames[at % frames.len()]);
                let _ = stderr.flush();
                at += 1;
                std::thread::sleep(FRAME_EVERY);
            }
            let _ = write!(stderr, "\r\x1b[2K");
            let _ = stderr.flush();
        });
        Self {
            running: Some((stop, handle, std::time::Instant::now())),
        }
    }

    /// Erases the pulse and gives the line back. Call it before printing
    /// anything else, or the next line lands on top of a frame.
    pub fn clear(mut self) {
        self.halt();
    }

    fn halt(&mut self) -> bool {
        match self.running.take() {
            Some((stop, handle, started)) => {
                if let Some(remaining) = MIN_VISIBLE.checked_sub(started.elapsed()) {
                    std::thread::sleep(remaining);
                }
                stop.store(true, Ordering::Relaxed);
                let _ = handle.join();
                true
            }
            None => false,
        }
    }
}

/// A drop erases the line too, so an early `?` or a panic cannot leave half
/// a frame under the error that follows it.
impl Drop for Pulse {
    fn drop(&mut self) {
        self.halt();
    }
}

/// One green check on stderr, after the work proved out — the last frame of
/// the pulse, in spirit. Same gate as the pulse: a pipe hears nothing.
pub fn mark_done(message: &str) {
    let style = Style::for_stderr();
    if style == Style::Ansi {
        eprintln!("{} {message}", style.paint(style::OK, "✓"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inert_pulse_clears_without_ever_having_run() {
        Pulse::inert().clear();
    }

    #[test]
    fn an_animated_pulse_stops_when_told() {
        // Constructed directly: whether the test runner is a terminal must
        // not decide whether this test exercises the thread.
        let pulse = Pulse::animated("testing…".to_string());
        std::thread::sleep(std::time::Duration::from_millis(10));
        pulse.clear();
    }

    /// Work faster than one crossing still shows a whole crossing:
    /// anything shorter is a flicker a human cannot read.
    #[test]
    fn a_fast_job_still_shows_one_full_crossing() {
        let started = std::time::Instant::now();
        Pulse::animated("blink…".to_string()).clear();
        assert!(started.elapsed() >= MIN_VISIBLE);
    }

    /// The wait is for the watcher; a pipe must never pay it.
    #[test]
    fn an_inert_pulse_never_waits() {
        let started = std::time::Instant::now();
        Pulse::inert().clear();
        assert!(started.elapsed() < MIN_VISIBLE);
    }

    #[test]
    fn dropping_a_running_pulse_does_not_hang_or_panic() {
        let pulse = Pulse::animated("dropped…".to_string());
        drop(pulse);
    }

    #[test]
    fn every_frame_is_the_same_width_so_the_line_never_jitters() {
        let widths = FRAMES
            .iter()
            .map(|frame| frame.chars().count())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(widths.len(), 1, "frames differ in width: {widths:?}");
    }
}
