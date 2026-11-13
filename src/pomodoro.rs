use std::time::{Duration, Instant};

use crate::config::Config;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Focus,
    Break,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    FocusComplete,
    BreakComplete,
}

pub struct Pomodoro {
    pub phase: Phase,
    pub running: bool,
    pub completed: u32,
    remaining: Duration,
    last_tick: Instant,
}

impl Pomodoro {
    pub fn new(config: &Config) -> Self {
        let mut timer = Self {
            phase: Phase::Focus,
            running: false,
            completed: 0,
            remaining: Duration::ZERO,
            last_tick: Instant::now(),
        };
        timer.reset_phase(config);
        timer
    }

    pub fn tick(&mut self, config: &Config) -> Option<Event> {
        if !self.running {
            return None;
        }
        let now = Instant::now();
        let elapsed = now.saturating_duration_since(self.last_tick);
        self.last_tick = now;
        if elapsed < self.remaining {
            self.remaining -= elapsed;
            return None;
        }

        match self.phase {
            Phase::Focus => {
                self.completed = self.completed.saturating_add(1);
                self.phase = Phase::Break;
                self.remaining = self.duration(config, Phase::Break);
                self.last_tick = Instant::now();
                Some(Event::FocusComplete)
            }
            Phase::Break => {
                self.phase = Phase::Focus;
                self.remaining = self.duration(config, Phase::Focus);
                self.running = false;
                self.last_tick = Instant::now();
                Some(Event::BreakComplete)
            }
        }
    }

    pub fn toggle(&mut self) {
        if self.running {
            self.accrue();
            self.running = false;
        } else {
            self.last_tick = Instant::now();
            self.running = true;
        }
    }

    pub fn reset(&mut self, config: &Config) {
        self.phase = Phase::Focus;
        self.running = false;
        self.reset_phase(config);
    }

    pub fn skip(&mut self, config: &Config) {
        self.phase = match self.phase {
            Phase::Focus => Phase::Break,
            Phase::Break => Phase::Focus,
        };
        self.running = false;
        self.remaining = self.duration(config, self.phase);
        self.last_tick = Instant::now();
    }

    pub fn apply_duration(&mut self, config: &Config) {
        if !self.running {
            self.remaining = self.duration(config, self.phase);
        }
    }

    pub fn progress(&self, config: &Config) -> f32 {
        let total = self.duration(config, self.phase);
        if total.is_zero() {
            return 0.0;
        }
        let remaining = self.remaining.min(total).as_secs_f32();
        (1.0 - remaining / total.as_secs_f32()).clamp(0.0, 1.0)
    }

    pub fn remaining_secs(&self) -> u64 {
        self.remaining
            .as_secs()
            .saturating_add(u64::from(self.remaining.subsec_millis() > 0))
    }

    pub fn phase_label(&self) -> &'static str {
        match self.phase {
            Phase::Focus => "FOCUS",
            Phase::Break => "BREAK",
        }
    }

    fn reset_phase(&mut self, config: &Config) {
        self.remaining = self.duration(config, self.phase);
        self.last_tick = Instant::now();
    }

    fn accrue(&mut self) {
        let now = Instant::now();
        self.remaining = self
            .remaining
            .saturating_sub(now.saturating_duration_since(self.last_tick));
        self.last_tick = now;
    }

    fn duration(&self, config: &Config, phase: Phase) -> Duration {
        let minutes = match phase {
            Phase::Focus => config.pomodoro_focus_min,
            Phase::Break => config.pomodoro_break_min,
        };
        Duration::from_secs(u64::from(minutes.max(1)) * 60)
    }
}
