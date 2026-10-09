// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: GPL-3.0-or-later

//! Auto-lock policy, as pure logic over a monotonic clock so it can be
//! tested without waiting.
//!
//! The vault locks when any of these happens:
//! - no user activity for `idle_timeout` while the app is in the foreground;
//! - the app has been in the background (cover shown, or another app in
//!   front) for `background_timeout`;
//! - the device screen locks, if `lock_on_screen_lock` is set.
//!
//! The UI calls [`AutoLock::touch`] on input, [`AutoLock::set_active`] on
//! application state changes, and [`AutoLock::should_lock`] from a
//! once-a-second timer. Timers do not run while the phone is suspended, so
//! the clock must be one that keeps counting through suspend
//! (`CLOCK_BOOTTIME`), which [`boottime`] reads.

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoLockSettings {
    /// `None` disables the idle lock.
    pub idle_timeout: Option<Duration>,
    /// `None` disables the background lock; `Some(0)` locks as soon as the
    /// app leaves the foreground.
    pub background_timeout: Option<Duration>,
    pub lock_on_screen_lock: bool,
}

impl Default for AutoLockSettings {
    fn default() -> Self {
        Self {
            idle_timeout: Some(Duration::from_secs(5 * 60)),
            background_timeout: Some(Duration::from_secs(30)),
            lock_on_screen_lock: true,
        }
    }
}

/// Why the vault locked, for the unlock page to tell the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockReason {
    Idle,
    Background,
    ScreenLock,
}

#[derive(Debug, Clone)]
pub struct AutoLock {
    pub settings: AutoLockSettings,
    last_activity: Duration,
    backgrounded_at: Option<Duration>,
    screen_locked: bool,
}

impl AutoLock {
    /// `now` is a reading of the same clock later passed to the other calls.
    pub fn new(settings: AutoLockSettings, now: Duration) -> Self {
        Self {
            settings,
            last_activity: now,
            backgrounded_at: None,
            screen_locked: false,
        }
    }

    /// User input seen.
    pub fn touch(&mut self, now: Duration) {
        self.last_activity = now;
    }

    /// Application became active (true) or inactive (false).
    pub fn set_active(&mut self, active: bool, now: Duration) {
        if active {
            self.backgrounded_at = None;
            self.last_activity = now;
        } else if self.backgrounded_at.is_none() {
            self.backgrounded_at = Some(now);
        }
    }

    /// The display locked or unlocked (from MCE's tklock state).
    pub fn set_screen_locked(&mut self, locked: bool) {
        self.screen_locked = locked;
    }

    /// Whether the vault should lock now, and why.
    pub fn should_lock(&self, now: Duration) -> Option<LockReason> {
        if self.settings.lock_on_screen_lock && self.screen_locked {
            return Some(LockReason::ScreenLock);
        }
        if let (Some(t), Some(since)) = (self.settings.background_timeout, self.backgrounded_at) {
            if now.saturating_sub(since) >= t {
                return Some(LockReason::Background);
            }
        }
        if let Some(t) = self.settings.idle_timeout {
            if self.backgrounded_at.is_none() && now.saturating_sub(self.last_activity) >= t {
                return Some(LockReason::Idle);
            }
        }
        None
    }

    /// Seconds until the idle lock fires (for a countdown), if it is armed.
    pub fn idle_remaining(&self, now: Duration) -> Option<Duration> {
        let t = self.settings.idle_timeout?;
        Some(t.saturating_sub(now.saturating_sub(self.last_activity)))
    }
}

/// `CLOCK_BOOTTIME`: monotonic and counting through suspend. Falls back to
/// a process-local monotonic clock off Linux.
pub fn boottime() -> Duration {
    use std::sync::OnceLock;
    use std::time::Instant;
    static START: OnceLock<Instant> = OnceLock::new();

    #[cfg(target_os = "linux")]
    {
        // /proc/uptime is CLOCK_BOOTTIME with centisecond resolution, which
        // is plenty for a lock timer and needs no unsafe code.
        if let Some(secs) = std::fs::read_to_string("/proc/uptime").ok().and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        }) {
            return Duration::from_secs_f64(secs);
        }
    }
    START.get_or_init(Instant::now).elapsed()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(secs: u64) -> Duration {
        Duration::from_secs(secs)
    }

    #[test]
    fn idle_lock() {
        let mut a = AutoLock::new(AutoLockSettings::default(), s(0));
        assert_eq!(a.should_lock(s(299)), None);
        a.touch(s(200));
        assert_eq!(a.should_lock(s(499)), None);
        assert_eq!(a.idle_remaining(s(499)), Some(s(1)));
        assert_eq!(a.should_lock(s(500)), Some(LockReason::Idle));
    }

    #[test]
    fn background_lock() {
        let mut a = AutoLock::new(AutoLockSettings::default(), s(0));
        a.set_active(false, s(10));
        a.set_active(false, s(20)); // repeated inactive keeps the first time
        assert_eq!(a.should_lock(s(39)), None);
        assert_eq!(a.should_lock(s(40)), Some(LockReason::Background));
        a.set_active(true, s(41));
        assert_eq!(a.should_lock(s(41)), None);
    }

    #[test]
    fn immediate_background_and_disabled() {
        let mut a = AutoLock::new(
            AutoLockSettings {
                idle_timeout: None,
                background_timeout: Some(Duration::ZERO),
                lock_on_screen_lock: false,
            },
            s(0),
        );
        assert_eq!(a.should_lock(s(100_000)), None, "idle lock disabled");
        a.set_screen_locked(true);
        assert_eq!(a.should_lock(s(1)), None, "screen lock ignored");
        a.set_active(false, s(5));
        assert_eq!(a.should_lock(s(5)), Some(LockReason::Background));
    }

    #[test]
    fn screen_lock() {
        let mut a = AutoLock::new(AutoLockSettings::default(), s(0));
        a.set_screen_locked(true);
        assert_eq!(a.should_lock(s(1)), Some(LockReason::ScreenLock));
    }

    #[test]
    fn boottime_advances() {
        let a = boottime();
        let b = boottime();
        assert!(b >= a);
    }
}
