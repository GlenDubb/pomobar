//! Focus/break session logic. Pure Rust (no Windows APIs) so it can be unit-tested.
//!
//! Time is passed in as a monotonic millisecond count (`now`), which keeps the
//! logic deterministic and easy to test.

use crate::settings::Settings;

/// The three kinds of session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Focus,
    ShortBreak,
    LongBreak,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Focus, Mode::ShortBreak, Mode::LongBreak];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Focus => "Focus",
            Mode::ShortBreak => "Short break",
            Mode::LongBreak => "Long break",
        }
    }

    pub fn is_break(self) -> bool {
        !matches!(self, Mode::Focus)
    }
}

/// Whether the current session is waiting to start, counting down, or paused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Paused,
}

/// What happened when a session ended (or was skipped).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    pub finished: Mode,
    pub next: Mode,
    /// The next session was started automatically (per the auto-start settings).
    pub auto_started: bool,
    /// The user skipped ahead rather than the timer running out.
    pub skipped: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Timer {
    mode: Mode,
    phase: Phase,
    total_ms: u64,
    /// Remaining time while idle or paused.
    remaining_ms: u64,
    /// Monotonic deadline while running.
    ends_at: u64,
    /// Focus sessions completed today.
    completed: u32,
    settings: Settings,
    /// 60 000 normally. Demo mode uses 1 000, so "minutes" become seconds.
    ms_per_minute: u64,
}

impl Timer {
    pub fn new(settings: Settings, completed: u32, ms_per_minute: u64) -> Self {
        let mut t = Timer {
            mode: Mode::Focus,
            phase: Phase::Idle,
            total_ms: 0,
            remaining_ms: 0,
            ends_at: 0,
            completed,
            settings,
            ms_per_minute: ms_per_minute.max(1),
        };
        t.load(Mode::Focus);
        t
    }

    fn load(&mut self, mode: Mode) {
        self.mode = mode;
        self.phase = Phase::Idle;
        self.total_ms = self.duration_ms(mode);
        self.remaining_ms = self.total_ms;
    }

    pub fn duration_ms(&self, mode: Mode) -> u64 {
        u64::from(self.settings.minutes(mode)) * self.ms_per_minute
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn settings(&self) -> Settings {
        self.settings
    }

    pub fn completed(&self) -> u32 {
        self.completed
    }

    pub fn set_completed(&mut self, n: u32) {
        self.completed = n;
    }

    pub fn total_ms(&self) -> u64 {
        self.total_ms
    }

    pub fn remaining_ms(&self, now: u64) -> u64 {
        match self.phase {
            Phase::Running => self.ends_at.saturating_sub(now),
            Phase::Idle | Phase::Paused => self.remaining_ms,
        }
    }

    pub fn elapsed_ms(&self, now: u64) -> u64 {
        self.total_ms.saturating_sub(self.remaining_ms(now))
    }

    /// Whole "minutes" left, rounded up (so the final minute reads 1).
    pub fn minutes_left(&self, now: u64) -> u64 {
        self.remaining_ms(now).div_ceil(self.ms_per_minute)
    }

    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    pub fn start(&mut self, now: u64) {
        if self.phase == Phase::Running {
            return;
        }
        if self.remaining_ms == 0 {
            self.remaining_ms = self.total_ms;
        }
        self.ends_at = now + self.remaining_ms;
        self.phase = Phase::Running;
    }

    pub fn pause(&mut self, now: u64) {
        if self.phase == Phase::Running {
            self.remaining_ms = self.remaining_ms(now);
            self.phase = Phase::Paused;
        }
    }

    pub fn toggle(&mut self, now: u64) {
        if self.phase == Phase::Running {
            self.pause(now);
        } else {
            self.start(now);
        }
    }

    /// Put the current session back to its full length, stopped.
    pub fn reset(&mut self) {
        self.load(self.mode);
    }

    pub fn switch_to(&mut self, mode: Mode) {
        self.load(mode);
    }

    /// Call regularly. Returns `Some` when a running session reaches zero.
    pub fn tick(&mut self, now: u64) -> Option<Completion> {
        if self.phase == Phase::Running && self.remaining_ms(now) == 0 {
            Some(self.advance(now, false))
        } else {
            None
        }
    }

    /// End the current session now and move on to the next one.
    pub fn skip(&mut self, now: u64) -> Completion {
        self.advance(now, true)
    }

    fn advance(&mut self, now: u64, skipped: bool) -> Completion {
        let finished = self.mode;
        let next = match finished {
            Mode::Focus => {
                self.completed = self.completed.saturating_add(1);
                let every = self.settings.long_break_every.max(1);
                if self.completed % every == 0 {
                    Mode::LongBreak
                } else {
                    Mode::ShortBreak
                }
            }
            Mode::ShortBreak | Mode::LongBreak => Mode::Focus,
        };
        self.load(next);
        let auto_started = if next.is_break() {
            self.settings.auto_start_breaks
        } else {
            self.settings.auto_start_focus
        };
        if auto_started {
            self.start(now);
        }
        Completion {
            finished,
            next,
            auto_started,
            skipped,
        }
    }

    /// Use new settings. A session that hasn't been started picks up the new
    /// length straight away; a running or paused one keeps going unchanged.
    pub fn apply_settings(&mut self, settings: Settings) {
        let untouched = self.phase == Phase::Idle && self.remaining_ms == self.total_ms;
        self.settings = settings;
        if untouched {
            self.load(self.mode);
        }
    }

    /// (filled, total) dots for the "sessions until a long break" indicator.
    pub fn cycle(&self) -> (u32, u32) {
        let every = self.settings.long_break_every.max(1);
        let done = self.completed % every;
        let filled = if done == 0 && self.completed > 0 && self.mode == Mode::LongBreak {
            every
        } else {
            done
        };
        (filled, every)
    }

    /// The session number shown as "#n": the focus session you're on, or during
    /// a break, the one you just finished.
    pub fn session_number(&self) -> u32 {
        if self.mode == Mode::Focus {
            self.completed + 1
        } else {
            self.completed.max(1)
        }
    }
}

/// Format milliseconds as `mm:ss`, rounding up so a fresh 25-minute timer reads
/// 25:00 and the last partial second reads 00:01.
pub fn format_mmss(ms: u64) -> String {
    let secs = ms.div_ceil(1000);
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;

    const MIN: u64 = 60_000;

    fn timer() -> Timer {
        Timer::new(Settings::default(), 0, MIN)
    }

    #[test]
    fn starts_idle_at_full_focus_length() {
        let t = timer();
        assert_eq!(t.mode(), Mode::Focus);
        assert_eq!(t.phase(), Phase::Idle);
        assert_eq!(t.remaining_ms(123), 25 * MIN);
        assert_eq!(format_mmss(t.remaining_ms(0)), "25:00");
    }

    #[test]
    fn counts_down_pauses_and_resumes() {
        let mut t = timer();
        t.start(1_000);
        assert_eq!(t.remaining_ms(1_000 + 10_000), 25 * MIN - 10_000);
        t.pause(1_000 + 10_000);
        assert_eq!(t.phase(), Phase::Paused);
        // Time passing while paused doesn't count.
        assert_eq!(t.remaining_ms(999_999), 25 * MIN - 10_000);
        t.toggle(2_000_000);
        assert_eq!(t.phase(), Phase::Running);
        assert_eq!(t.remaining_ms(2_000_000 + 5_000), 25 * MIN - 15_000);
    }

    #[test]
    fn focus_completes_into_short_break_then_long_break() {
        let mut t = timer();
        let mut now = 0;
        for n in 1..=4 {
            t.start(now);
            now += 25 * MIN;
            let c = t.tick(now).expect("focus should complete");
            assert_eq!(c.finished, Mode::Focus);
            assert_eq!(t.completed(), n);
            let expected = if n == 4 { Mode::LongBreak } else { Mode::ShortBreak };
            assert_eq!(c.next, expected);
            assert_eq!(t.mode(), expected);
            assert!(!c.auto_started);
            assert_eq!(t.phase(), Phase::Idle);
            // Finish the break too.
            t.start(now);
            now += t.total_ms();
            let b = t.tick(now).expect("break should complete");
            assert_eq!(b.next, Mode::Focus);
        }
    }

    #[test]
    fn tick_before_deadline_does_nothing() {
        let mut t = timer();
        t.start(0);
        assert!(t.tick(25 * MIN - 1).is_none());
        assert!(t.tick(25 * MIN).is_some());
    }

    #[test]
    fn auto_start_breaks_and_focus() {
        let s = Settings {
            auto_start_breaks: true,
            auto_start_focus: false,
            ..Settings::default()
        };
        let mut t = Timer::new(s, 0, MIN);
        t.start(0);
        let c = t.tick(25 * MIN).unwrap();
        assert!(c.auto_started);
        assert_eq!(t.phase(), Phase::Running);
        assert_eq!(t.remaining_ms(25 * MIN + MIN), 4 * MIN);
        let c = t.tick(30 * MIN).unwrap();
        assert_eq!(c.next, Mode::Focus);
        assert!(!c.auto_started);
        assert_eq!(t.phase(), Phase::Idle);
    }

    #[test]
    fn skip_counts_focus_and_flags_it() {
        let mut t = timer();
        t.start(0);
        let c = t.skip(60_000);
        assert!(c.skipped);
        assert_eq!(t.completed(), 1);
        assert_eq!(t.mode(), Mode::ShortBreak);
        let c = t.skip(70_000);
        assert_eq!(c.next, Mode::Focus);
        assert_eq!(t.completed(), 1);
    }

    #[test]
    fn new_settings_apply_only_to_untouched_sessions() {
        let mut t = timer();
        let longer = Settings {
            focus_minutes: 50,
            ..Settings::default()
        };
        t.apply_settings(longer);
        assert_eq!(t.remaining_ms(0), 50 * MIN);

        let mut t = timer();
        t.start(0);
        t.apply_settings(longer);
        assert_eq!(t.remaining_ms(MIN), 24 * MIN);
        t.pause(MIN);
        t.apply_settings(Settings::default());
        assert_eq!(t.remaining_ms(MIN), 24 * MIN);
    }

    #[test]
    fn reset_and_switch() {
        let mut t = timer();
        t.start(0);
        t.reset();
        assert_eq!(t.phase(), Phase::Idle);
        assert_eq!(t.remaining_ms(10 * MIN), 25 * MIN);
        t.switch_to(Mode::LongBreak);
        assert_eq!(t.remaining_ms(0), 15 * MIN);
    }

    #[test]
    fn cycle_dots_and_session_numbers() {
        let mut t = timer();
        assert_eq!(t.cycle(), (0, 4));
        assert_eq!(t.session_number(), 1);
        for _ in 0..3 {
            t.skip(0); // focus -> break
            t.skip(0); // break -> focus
        }
        assert_eq!(t.cycle(), (3, 4));
        assert_eq!(t.session_number(), 4);
        t.skip(0); // 4th focus -> long break
        assert_eq!(t.mode(), Mode::LongBreak);
        assert_eq!(t.cycle(), (4, 4));
        assert_eq!(t.session_number(), 4);
        t.skip(0); // long break -> focus
        assert_eq!(t.cycle(), (0, 4));
        assert_eq!(t.session_number(), 5);
    }

    #[test]
    fn minutes_left_rounds_up() {
        let mut t = timer();
        t.start(0);
        assert_eq!(t.minutes_left(0), 25);
        assert_eq!(t.minutes_left(1), 25);
        assert_eq!(t.minutes_left(MIN), 24);
        assert_eq!(t.minutes_left(25 * MIN - 1), 1);
    }

    #[test]
    fn demo_scale_turns_minutes_into_seconds() {
        let t = Timer::new(Settings::default(), 0, 1_000);
        assert_eq!(format_mmss(t.remaining_ms(0)), "00:25");
    }

    #[test]
    fn mmss_formatting() {
        assert_eq!(format_mmss(0), "00:00");
        assert_eq!(format_mmss(1), "00:01");
        assert_eq!(format_mmss(999), "00:01");
        assert_eq!(format_mmss(1_000), "00:01");
        assert_eq!(format_mmss(1_001), "00:02");
        assert_eq!(format_mmss(25 * MIN - 1_000), "24:59");
        assert_eq!(format_mmss(99 * MIN), "99:00");
    }
}
