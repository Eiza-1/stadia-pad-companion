//! Rolling input-rate and processing-latency statistics over the last N reports.

use serde::Serialize;

#[derive(Serialize, Clone, Copy, Default, Debug, PartialEq)]
pub struct StatsSnapshot {
    pub rate_hz: f32,
    pub interval_p50_ms: f32,
    pub interval_p99_ms: f32,
    pub interval_max_ms: f32,
    pub proc_p99_us: u32,
    pub samples: u32,
}

/// Fixed-size ring of (arrival, processing time) samples; recording is O(1) and allocation-free.
pub struct Stats {
    arrival: Vec<u64>,
    proc_us: Vec<u32>,
    next: usize,
    len: usize,
}

impl Stats {
    pub fn new(window: usize) -> Self {
        let window = window.max(2);
        Stats { arrival: vec![0; window], proc_us: vec![0; window], next: 0, len: 0 }
    }

    pub fn record(&mut self, arrival_us: u64, done_us: u64) {
        self.arrival[self.next] = arrival_us;
        self.proc_us[self.next] = done_us.saturating_sub(arrival_us).min(u32::MAX as u64) as u32;
        self.next = (self.next + 1) % self.arrival.len();
        self.len = (self.len + 1).min(self.arrival.len());
    }

    pub fn clear(&mut self) {
        self.next = 0;
        self.len = 0;
    }

    pub fn snapshot(&self) -> StatsSnapshot {
        if self.len == 0 {
            return StatsSnapshot::default();
        }
        let cap = self.arrival.len();
        let start = (self.next + cap - self.len) % cap;
        let times: Vec<u64> = (0..self.len).map(|i| self.arrival[(start + i) % cap]).collect();
        let mut intervals: Vec<u64> = times.windows(2).map(|w| w[1].saturating_sub(w[0])).collect();
        intervals.sort_unstable();
        let mut procs: Vec<u32> = (0..self.len).map(|i| self.proc_us[(start + i) % cap]).collect();
        procs.sort_unstable();

        let span_s = (times[self.len - 1].saturating_sub(times[0])) as f32 / 1e6;
        let ms = |us: u64| us as f32 / 1000.0;
        StatsSnapshot {
            rate_hz: if span_s > 0.0 { (self.len - 1) as f32 / span_s } else { 0.0 },
            interval_p50_ms: intervals.get(pct_index(intervals.len(), 50)).map_or(0.0, |&v| ms(v)),
            interval_p99_ms: intervals.get(pct_index(intervals.len(), 99)).map_or(0.0, |&v| ms(v)),
            interval_max_ms: intervals.last().map_or(0.0, |&v| ms(v)),
            proc_p99_us: procs[pct_index(procs.len(), 99)],
            samples: self.len as u32,
        }
    }
}

fn pct_index(len: usize, pct: usize) -> usize {
    (len.saturating_sub(1) * pct + 50) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_125hz() {
        let mut s = Stats::new(1024);
        for i in 0..500u64 {
            s.record(i * 8000, i * 8000 + 150);
        }
        let x = s.snapshot();
        assert!((x.rate_hz - 125.0).abs() < 0.5);
        assert!((x.interval_p50_ms - 8.0).abs() < 0.01);
        assert_eq!(x.proc_p99_us, 150);
    }

    #[test]
    fn outlier_shows_in_max_not_p50() {
        let mut s = Stats::new(1024);
        let mut t = 0;
        for i in 0..200 {
            t += if i == 100 { 30000 } else { 8000 };
            s.record(t, t + 10);
        }
        let x = s.snapshot();
        assert!((x.interval_max_ms - 30.0).abs() < 0.01);
        assert!((x.interval_p50_ms - 8.0).abs() < 0.01);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(Stats::new(8).snapshot(), StatsSnapshot::default());
    }
}
