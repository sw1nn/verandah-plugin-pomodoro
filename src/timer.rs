use crate::config::Config;

/// The current phase of the pomodoro cycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Work,
    ShortBreak,
    LongBreak,
}

/// Represents a phase transition after tick
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    /// No transition occurred
    None,
    /// Work phase completed, transitioned to break
    WorkComplete,
    /// Break phase completed, transitioned to work
    BreakComplete,
}

impl Phase {
    pub fn is_break(self) -> bool {
        matches!(self, Phase::ShortBreak | Phase::LongBreak)
    }
}

/// Pomodoro timer state machine
#[derive(Debug, Clone)]
pub struct Timer {
    /// Current phase
    phase: Phase,
    /// Elapsed seconds in current phase
    elapsed_secs: u64,
    /// Duration settings in seconds
    work_secs: u64,
    short_break_secs: u64,
    long_break_secs: u64,
    /// Work iterations completed (0-3, resets after long break)
    iterations: u8,
    /// Total completed pomodoro sessions
    sessions_completed: u32,
    /// Whether the timer is running
    running: bool,
    /// Auto-start work after break
    auto_start_work: bool,
    /// Auto-start break after work
    auto_start_break: bool,
}

impl Timer {
    pub fn new(config: &Config) -> Self {
        Timer {
            phase: Phase::Work,
            elapsed_secs: 0,
            work_secs: config.work * 60,
            short_break_secs: config.short_break * 60,
            long_break_secs: config.long_break * 60,
            iterations: 0,
            sessions_completed: 0,
            running: false,
            auto_start_work: config.auto_start_work,
            auto_start_break: config.auto_start_break,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn iterations(&self) -> u8 {
        self.iterations
    }

    pub fn sessions_completed(&self) -> u32 {
        self.sessions_completed
    }

    /// Get the duration of the current phase in seconds
    fn current_duration(&self) -> u64 {
        match self.phase {
            Phase::Work => self.work_secs,
            Phase::ShortBreak => self.short_break_secs,
            Phase::LongBreak => self.long_break_secs,
        }
    }

    /// Get remaining time in seconds
    pub fn remaining_secs(&self) -> u64 {
        self.current_duration().saturating_sub(self.elapsed_secs)
    }

    /// Format remaining time as MM:SS or HH:MM:SS
    pub fn remaining_formatted(&self) -> String {
        let secs = self.remaining_secs();
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        let secs = secs % 60;

        if hours > 0 {
            format!("{hours}:{mins:02}:{secs:02}")
        } else {
            format!("{mins:02}:{secs:02}")
        }
    }

    /// Toggle between running and paused
    pub fn toggle(&mut self) {
        self.running = !self.running;
    }

    /// Start the timer
    pub fn start(&mut self) {
        self.running = true;
    }

    /// Pause the timer
    pub fn pause(&mut self) {
        self.running = false;
    }

    /// Reset to initial state
    pub fn reset(&mut self) {
        self.phase = Phase::Work;
        self.elapsed_secs = 0;
        self.iterations = 0;
        self.running = false;
    }

    /// Skip to the next phase without waiting
    /// Returns the type of transition that occurred
    pub fn skip(&mut self) -> Transition {
        self.transition_to_next_phase()
    }

    /// Advance time by one second, handling phase transitions
    /// Returns the type of transition that occurred (if any)
    pub fn tick(&mut self) -> Transition {
        if !self.running {
            return Transition::None;
        }

        self.elapsed_secs += 1;

        if self.elapsed_secs >= self.current_duration() {
            return self.transition_to_next_phase();
        }

        Transition::None
    }

    fn transition_to_next_phase(&mut self) -> Transition {
        self.elapsed_secs = 0;
        let from_phase = self.phase;

        match self.phase {
            Phase::Work => {
                self.iterations += 1;

                if self.iterations >= 4 {
                    self.phase = Phase::LongBreak;
                } else {
                    self.phase = Phase::ShortBreak;
                }

                self.running = self.auto_start_break;
            }
            Phase::ShortBreak => {
                self.phase = Phase::Work;
                self.running = self.auto_start_work;
            }
            Phase::LongBreak => {
                self.phase = Phase::Work;
                self.iterations = 0;
                self.sessions_completed += 1;
                self.running = self.auto_start_work;
            }
        }

        if from_phase == Phase::Work {
            Transition::WorkComplete
        } else {
            Transition::BreakComplete
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> Config {
        Config {
            work: 1, // 1 minute for faster tests
            short_break: 1,
            long_break: 1,
            auto_start_work: false,
            auto_start_break: false,
            ..Config::default()
        }
    }

    #[test]
    fn test_new_timer() -> crate::error::Result<()> {
        let timer = Timer::new(&test_config());
        assert_eq!(timer.phase(), Phase::Work);
        assert!(!timer.is_running());
        assert_eq!(timer.iterations(), 0);
        assert_eq!(timer.sessions_completed(), 0);
        Ok(())
    }

    #[test]
    fn test_toggle() -> crate::error::Result<()> {
        let mut timer = Timer::new(&test_config());
        assert!(!timer.is_running());
        timer.toggle();
        assert!(timer.is_running());
        timer.toggle();
        assert!(!timer.is_running());
        Ok(())
    }

    #[test]
    fn test_remaining_formatted() -> crate::error::Result<()> {
        let config = Config {
            work: 25,
            ..Config::default()
        };
        let timer = Timer::new(&config);
        assert_eq!(timer.remaining_formatted(), "25:00");
        Ok(())
    }

    #[test]
    fn test_tick_advances_time() -> crate::error::Result<()> {
        let mut timer = Timer::new(&test_config());
        timer.start();
        let remaining_before = timer.remaining_secs();
        timer.tick();
        assert_eq!(timer.remaining_secs(), remaining_before - 1);
        Ok(())
    }

    #[test]
    fn test_work_to_short_break_transition() -> crate::error::Result<()> {
        let mut timer = Timer::new(&test_config());
        timer.start();

        // Run through work phase (60 seconds)
        for _ in 0..60 {
            timer.tick();
        }

        assert_eq!(timer.phase(), Phase::ShortBreak);
        assert_eq!(timer.iterations(), 1);
        assert!(!timer.is_running()); // auto_start_break is false
        Ok(())
    }

    #[test]
    fn test_full_pomodoro_cycle() -> crate::error::Result<()> {
        let config = Config {
            work: 1,
            short_break: 1,
            long_break: 1,
            auto_start_work: true,
            auto_start_break: true,
            ..Config::default()
        };
        let mut timer = Timer::new(&config);
        timer.start();

        // Run through 4 work/break cycles + long break
        // Each cycle: 60s work + 60s break = 120s
        // Total: 4 * 60s work + 3 * 60s short break + 60s long break = 480s
        for _ in 0..480 {
            timer.tick();
        }

        assert_eq!(timer.sessions_completed(), 1);
        assert_eq!(timer.iterations(), 0);
        assert_eq!(timer.phase(), Phase::Work);
        Ok(())
    }

    #[test]
    fn test_reset() -> crate::error::Result<()> {
        let config = Config {
            work: 1,
            auto_start_break: true,
            ..Config::default()
        };
        let mut timer = Timer::new(&config);
        timer.start();

        // Advance past first work phase
        for _ in 0..60 {
            timer.tick();
        }

        assert_eq!(timer.phase(), Phase::ShortBreak);
        assert_eq!(timer.iterations(), 1);

        timer.reset();

        assert_eq!(timer.phase(), Phase::Work);
        assert_eq!(timer.iterations(), 0);
        assert!(!timer.is_running());
        Ok(())
    }

    #[test]
    fn test_skip() -> crate::error::Result<()> {
        let mut timer = Timer::new(&test_config());
        assert_eq!(timer.phase(), Phase::Work);

        timer.skip();
        assert_eq!(timer.phase(), Phase::ShortBreak);
        assert_eq!(timer.iterations(), 1);

        timer.skip();
        assert_eq!(timer.phase(), Phase::Work);
        Ok(())
    }
}
