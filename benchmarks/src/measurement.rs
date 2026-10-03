use std::time::Duration;

/// Windows job counters include exited children, so process churn cannot reset totals.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counters {
    pub user_100ns: u64,
    pub kernel_100ns: u64,
    pub read_ops: u64,
    pub write_ops: u64,
    pub other_ops: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
    pub other_bytes: u64,
}
impl Counters {
    pub fn delta(self, old: Self) -> Self {
        Self {
            user_100ns: self.user_100ns.saturating_sub(old.user_100ns),
            kernel_100ns: self.kernel_100ns.saturating_sub(old.kernel_100ns),
            read_ops: self.read_ops.saturating_sub(old.read_ops),
            write_ops: self.write_ops.saturating_sub(old.write_ops),
            other_ops: self.other_ops.saturating_sub(old.other_ops),
            read_bytes: self.read_bytes.saturating_sub(old.read_bytes),
            write_bytes: self.write_bytes.saturating_sub(old.write_bytes),
            other_bytes: self.other_bytes.saturating_sub(old.other_bytes),
        }
    }
}
pub fn cpu_pct(ticks: u64, elapsed: Duration, cpus: usize) -> (f64, f64) {
    let core = if elapsed.is_zero() {
        0.0
    } else {
        ticks as f64 / 10_000_000.0 / elapsed.as_secs_f64() * 100.0
    };
    (core, core / cpus.max(1) as f64)
}
/// Advance on the original time grid, skipping missed slots instead of catch-up bursts.
pub fn next_slot(previous: Duration, now: Duration, interval: Duration) -> Duration {
    let mut next = previous + interval;
    if next <= now {
        let skipped = (now - next).as_nanos() / interval.as_nanos() + 1;
        next += interval * skipped.min(u32::MAX as u128) as u32;
    }
    next
}
#[derive(Clone, Copy)]
pub struct IdlePoint {
    pub seconds: f64,
    pub cpu: f64,
    pub working_mib: f64,
    pub private_mib: f64,
    pub io_bytes: f64,
}
pub fn is_idle(points: &[IdlePoint], window: f64, cpu: f64, memory: f64, io_rate: f64) -> bool {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return false;
    };
    let span = last.seconds - first.seconds;
    if points.len() < 2 || span < window || span <= 0.0 {
        return false;
    }
    let range = |private: bool| {
        let values = points.iter().map(|p| {
            if private {
                p.private_mib
            } else {
                p.working_mib
            }
        });
        let (low, high) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), v| {
            (l.min(v), h.max(v))
        });
        high - low
    };
    points.iter().all(|p| p.cpu.is_finite() && p.cpu <= cpu)
        && range(false) <= memory
        && range(true) <= memory
        && points.iter().skip(1).map(|p| p.io_bytes).sum::<f64>() / span <= io_rate
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_uses_actual_elapsed_and_machine_capacity() {
        assert_eq!(
            cpu_pct(1_000_000, Duration::from_millis(100), 8),
            (100.0, 12.5)
        );
        assert_eq!(
            cpu_pct(2_000_000, Duration::from_millis(100), 8),
            (200.0, 25.0)
        );
        assert_eq!(
            cpu_pct(1_000_000, Duration::from_millis(200), 8),
            (50.0, 6.25)
        );
        assert_eq!(cpu_pct(1, Duration::ZERO, 0), (0.0, 0.0));
    }
    #[test]
    fn lifetime_totals_keep_exited_and_new_child_work() {
        // old child exits at 40 ticks; new child consumes 10: job total is 50,
        // even though live-process totals would fall from 30 to 10.
        let old = Counters {
            user_100ns: 30,
            read_bytes: 300,
            ..Default::default()
        };
        let new = Counters {
            user_100ns: 50,
            read_bytes: 500,
            ..Default::default()
        };
        assert_eq!(new.delta(old).user_100ns, 20);
        assert_eq!(new.delta(old).read_bytes, 200);
        assert_eq!(old.delta(new).read_bytes, 0);
    }
    #[test]
    fn deadline_skips_late_slots_without_drift() {
        let ms = Duration::from_millis;
        assert_eq!(next_slot(ms(100), ms(145), ms(100)), ms(200));
        assert_eq!(next_slot(ms(100), ms(350), ms(100)), ms(400));
        assert_eq!(next_slot(ms(100), ms(200), ms(100)), ms(300));
    }
    #[test]
    fn idle_requires_duration_cpu_both_memories_and_io() {
        let p = IdlePoint {
            seconds: 0.0,
            cpu: 0.0,
            working_mib: 20.0,
            private_mib: 10.0,
            io_bytes: 0.0,
        };
        let mut points = vec![p, IdlePoint { seconds: 2.0, ..p }];
        assert!(is_idle(&points, 2.0, 5.0, 1.0, 1024.0));
        assert!(!is_idle(&points, 3.0, 5.0, 1.0, 1024.0));
        points[1].cpu = 6.0;
        assert!(!is_idle(&points, 2.0, 5.0, 1.0, 1024.0));
        points[1].cpu = 0.0;
        points[1].private_mib = 12.0;
        assert!(!is_idle(&points, 2.0, 5.0, 1.0, 1024.0));
        points[1].private_mib = 10.0;
        points[1].io_bytes = 3000.0;
        assert!(!is_idle(&points, 2.0, 5.0, 1.0, 1024.0));
    }
}
