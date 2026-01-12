//! Time control and search limits for chess engines.
//!
//! This module provides shared time management functionality that can be used
//! by any engine implementation to respect time constraints during search.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Search limits that control when an engine should stop searching.
///
/// Engines should respect both depth and time limits, stopping when either
/// is reached. The time limit takes precedence - if time runs out, the engine
/// must return immediately with the best move found so far.
#[derive(Debug, Clone)]
pub struct SearchLimits {
    /// Maximum search depth in plies (half-moves)
    pub depth: u8,
    /// Maximum time allowed for this move (None = infinite)
    pub move_time: Option<Duration>,
    /// Time controller for checking if search should stop
    pub time_control: TimeControl,
}

impl SearchLimits {
    /// Create limits with only depth constraint (no time limit).
    pub fn depth(depth: u8) -> Self {
        Self {
            depth,
            move_time: None,
            time_control: TimeControl::new(None),
        }
    }

    /// Create limits with both depth and time constraints.
    pub fn depth_and_time(depth: u8, move_time: Duration) -> Self {
        Self {
            depth,
            move_time: Some(move_time),
            time_control: TimeControl::new(Some(move_time)),
        }
    }

    /// Create limits with only time constraint (infinite depth).
    pub fn time(move_time: Duration) -> Self {
        Self {
            depth: u8::MAX,
            move_time: Some(move_time),
            time_control: TimeControl::new(Some(move_time)),
        }
    }

    /// Check if search should stop due to time limit.
    #[inline]
    pub fn should_stop(&self) -> bool {
        self.time_control.is_stopped()
    }

    /// Start the time control clock. Call this when search begins.
    pub fn start(&self) {
        self.time_control.start();
    }
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self::depth(4)
    }
}

/// Thread-safe time controller that tracks whether search should stop.
///
/// This is designed to be cheaply cloneable and shareable across search threads.
/// The `is_stopped()` check is very fast (atomic load) so it can be called
/// frequently during search without performance impact.
#[derive(Debug, Clone)]
pub struct TimeControl {
    /// Shared stop flag
    stopped: Arc<AtomicBool>,
    /// Start time of the search
    start_time: Arc<std::sync::RwLock<Option<Instant>>>,
    /// Time limit for this search (None = infinite)
    time_limit: Option<Duration>,
    /// How often to check the clock (in nodes). Checking every node is wasteful.
    check_interval: u64,
}

impl TimeControl {
    /// Create a new time controller.
    ///
    /// # Arguments
    /// * `time_limit` - Maximum time allowed for search (None = infinite)
    pub fn new(time_limit: Option<Duration>) -> Self {
        Self {
            stopped: Arc::new(AtomicBool::new(false)),
            start_time: Arc::new(std::sync::RwLock::new(None)),
            time_limit,
            check_interval: 1024, // Check clock every 1024 nodes
        }
    }

    /// Start the clock. Should be called when search begins.
    pub fn start(&self) {
        *self.start_time.write().unwrap() = Some(Instant::now());
        self.stopped.store(false, Ordering::SeqCst);
    }

    /// Force stop the search immediately.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    /// Check if search should stop.
    ///
    /// This is a fast atomic load, suitable for calling frequently.
    #[inline]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }

    /// Check time and update stopped flag if time expired.
    ///
    /// This does the actual clock check. Call this periodically (e.g., every N nodes)
    /// rather than on every node to avoid performance overhead.
    pub fn check_time(&self) -> bool {
        if self.is_stopped() {
            return true;
        }

        if let Some(limit) = self.time_limit
            && let Some(start) = *self.start_time.read().unwrap()
            && start.elapsed() >= limit
        {
            self.stop();
            return true;
        }

        false
    }

    /// Check if it's time to check the clock based on node count.
    ///
    /// Returns true every `check_interval` nodes.
    #[inline]
    pub fn should_check_time(&self, nodes: u64) -> bool {
        nodes.is_multiple_of(self.check_interval)
    }

    /// Get elapsed time since search started.
    pub fn elapsed(&self) -> Duration {
        self.start_time
            .read()
            .unwrap()
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Get remaining time (None if no limit or not started).
    pub fn remaining(&self) -> Option<Duration> {
        let limit = self.time_limit?;
        let elapsed = self.elapsed();
        if elapsed >= limit {
            Some(Duration::ZERO)
        } else {
            Some(limit - elapsed)
        }
    }
}

impl Default for TimeControl {
    fn default() -> Self {
        Self::new(None)
    }
}

#[cfg(test)]
#[path = "time_control_tests.rs"]
mod time_control_tests;

