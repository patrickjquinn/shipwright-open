// SPDX-FileCopyrightText: 2026 Patrick Quinn
// SPDX-License-Identifier: MIT-0

//! The app's logic, in plain Rust so it is unit-tested without Qt. The
//! `QObject` in [`crate::bridge`] is a thin layer over it.

/// A counter with a greeting: the state behind the main page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Greeter {
    count: u32,
}

impl Default for Greeter {
    /// Starts with [`Greeter::INITIAL`] greetings, so the list shows rows.
    fn default() -> Self {
        Greeter {
            count: Self::INITIAL,
        }
    }
}

impl Greeter {
    /// Greetings at start-up.
    pub const INITIAL: u32 = 3;

    /// How many greetings there are.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.count
    }

    /// Adds a greeting; returns the new count. Saturates at `u32::MAX`.
    pub fn increment(&mut self) -> u32 {
        self.count = self.count.saturating_add(1);
        self.count
    }

    /// Back to no greetings.
    pub fn reset(&mut self) {
        self.count = 0;
    }

    /// The page header's description for the current count.
    #[must_use]
    pub fn summary(&self) -> String {
        match self.count {
            0 => "Pull down to say hello".to_owned(),
            1 => "1 greeting".to_owned(),
            n => format!("{n} greetings"),
        }
    }

    /// The text of the list row at `index` (0-based).
    #[must_use]
    pub fn line(index: u32) -> String {
        format!("Hello from Rust, #{}", u64::from(index) + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::Greeter;

    #[test]
    fn counts_and_resets() {
        let mut g = Greeter::default();
        assert_eq!(g.count(), Greeter::INITIAL);
        assert_eq!(g.summary(), "3 greetings");
        g.reset();
        assert_eq!(g.summary(), "Pull down to say hello");
        assert_eq!(g.increment(), 1);
        assert_eq!(g.summary(), "1 greeting");
        g.increment();
        assert_eq!(g.count(), 2);
        assert_eq!(g.summary(), "2 greetings");
        g.reset();
        assert_eq!(g.count(), 0);
    }

    #[test]
    fn lines_are_one_based_and_do_not_overflow() {
        assert_eq!(Greeter::line(0), "Hello from Rust, #1");
        assert_eq!(Greeter::line(u32::MAX), "Hello from Rust, #4294967296");
    }

    #[test]
    fn increment_saturates() {
        let mut g = Greeter { count: u32::MAX };
        assert_eq!(g.increment(), u32::MAX);
    }
}
