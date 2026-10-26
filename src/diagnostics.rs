//! Presentation pacing is measured only in forced-redraw profiling mode. Idle
//! wakeups are not frames, and CPU submission time is not a GPU timestamp.
use serde::Serialize;
use std::time::Instant;

const WARMUP: usize = 10;

#[derive(Default)]
pub struct Profile {
    presented: usize,
    previous: Option<Instant>,
    intervals: Vec<f64>,
    cpu: Vec<f64>,
}
#[derive(Serialize)]
pub struct Timing {
    pub samples: usize,
    pub mean_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}
#[derive(Serialize)]
pub struct Summary {
    pub presented: usize,
    pub warmup_frames: usize,
    pub pacing: Option<Timing>,
    pub cpu_submission: Option<Timing>,
}
impl Profile {
    pub fn record(&mut self, now: Instant, cpu_ms: f64, exclude: bool) {
        self.presented += 1;
        if self.presented > WARMUP
            && !exclude
            && let Some(previous) = self.previous
        {
            self.intervals
                .push(now.duration_since(previous).as_secs_f64() * 1000.);
            self.cpu.push(cpu_ms);
        }
        self.previous = Some(now);
    }
    pub fn pause(&mut self) {
        self.previous = None;
    }
    pub fn summary(&self) -> Summary {
        Summary {
            presented: self.presented,
            warmup_frames: WARMUP,
            pacing: summarize(&self.intervals),
            cpu_submission: summarize(&self.cpu),
        }
    }
}
fn summarize(samples: &[f64]) -> Option<Timing> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    Some(Timing {
        samples: sorted.len(),
        mean_ms: sorted.iter().sum::<f64>() / sorted.len() as f64,
        p95_ms: sorted[(sorted.len() * 95).div_ceil(100) - 1],
        max_ms: *sorted.last()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn profiling_excludes_warmup_and_capture_stalls() {
        let mut profile = Profile::default();
        let start = Instant::now();
        for index in 0..20 {
            profile.record(start + Duration::from_millis(index * 16), 2., index == 19);
        }
        let summary = profile.summary();
        let pacing = summary.pacing.unwrap();
        assert_eq!(pacing.samples, 9);
        assert_eq!(pacing.mean_ms, 16.);
        assert_eq!(pacing.p95_ms, 16.);
        assert_eq!(summary.cpu_submission.unwrap().mean_ms, 2.);
    }
}
