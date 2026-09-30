//! Opt-in, bounded diagnostics for background cooling latency.
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Metric {
    count: u64,
    total: Duration,
    max: Duration,
}
struct Stats {
    since: Instant,
    context: Option<String>,
    slow_last_logged: Option<Instant>,
    slow_suppressed: u64,
    metrics: BTreeMap<&'static str, Metric>,
}
static ENABLED: OnceLock<bool> = OnceLock::new();
static STATS: OnceLock<Mutex<Stats>> = OnceLock::new();

fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var("ASENSE_TIMING").is_ok_and(|v| v == "1"))
}
fn stats() -> &'static Mutex<Stats> {
    STATS.get_or_init(|| {
        Mutex::new(Stats {
            since: Instant::now(),
            context: None,
            slow_last_logged: None,
            slow_suppressed: 0,
            metrics: BTreeMap::new(),
        })
    })
}

pub(crate) struct Timer(Option<(&'static str, Instant)>);
pub(crate) fn scope(name: &'static str) -> Timer {
    Timer(enabled().then(|| (name, Instant::now())))
}
pub(crate) fn context(state: &str, requested: Option<[u8; 2]>) {
    if !enabled() {
        return;
    }
    let lock = stats();
    if let Ok(mut stats) = lock.lock() {
        stats.context = Some(format!("state={state} requested={requested:?}"));
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let Some((name, started)) = self.0 else {
            return;
        };
        let elapsed = started.elapsed();
        let lock = stats();
        let Ok(mut stats) = lock.lock() else {
            return;
        };
        let metric = stats.metrics.entry(name).or_default();
        metric.count += 1;
        metric.total += elapsed;
        metric.max = metric.max.max(elapsed);
        if elapsed > Duration::from_millis(20) {
            let now = Instant::now();
            if stats
                .slow_last_logged
                .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1))
            {
                eprintln!(
                    "asense slow operation: stage={name} elapsed_ms={:.3} suppressed={} {}",
                    elapsed.as_secs_f64() * 1000.0,
                    stats.slow_suppressed,
                    stats
                        .context
                        .as_deref()
                        .unwrap_or("state=unknown requested=unknown")
                );
                stats.slow_last_logged = Some(now);
                stats.slow_suppressed = 0;
            } else {
                stats.slow_suppressed += 1;
            }
        }
        if stats.since.elapsed() >= Duration::from_secs(30) {
            if let Some(context) = &stats.context {
                eprintln!("asense timing: {context}");
            }
            for (stage, metric) in &stats.metrics {
                eprintln!(
                    "asense timing: stage={stage} count={} total_ms={:.3} max_ms={:.3}",
                    metric.count,
                    metric.total.as_secs_f64() * 1000.0,
                    metric.max.as_secs_f64() * 1000.0
                );
            }
            stats.metrics.clear();
            stats.since = Instant::now();
        }
    }
}
