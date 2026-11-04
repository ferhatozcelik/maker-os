//! Testlerde ileri sarılabilen monoton saat.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Default)]
pub struct Clock {
    skew: Mutex<Duration>,
}

impl Clock {
    pub fn now(&self) -> Instant {
        Instant::now() + *self.skew.lock().unwrap_or_else(|e| e.into_inner())
    }
    /// Yalnız testler için: saati ileri sarar.
    pub fn advance(&self, d: Duration) {
        *self.skew.lock().unwrap_or_else(|e| e.into_inner()) += d;
    }
}
