use std::time::{Duration, Instant};

use crate::Pace;

pub(crate) struct Measure {
    pub started: Instant,
    pub first_frame_reported: bool,
    pub last_frame: Option<Instant>,
    pub frame_intervals: Vec<Duration>,
    pub measuring: bool,
    pub pace: Option<Pace>,
}

impl Measure {
    pub fn new(started: Instant) -> Self {
        Self {
            started,
            first_frame_reported: false,
            last_frame: None,
            frame_intervals: Vec::new(),
            measuring: false,
            pace: None,
        }
    }
}
