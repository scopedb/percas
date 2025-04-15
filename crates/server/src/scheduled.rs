use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use atrium_metrics::GlobalMetrics;
use fastimer::schedule::BaseAction;
use fastimer::schedule::SimpleAction;

use crate::AtriumContext;

pub struct TokioSpawn;

impl fastimer::Spawn for TokioSpawn {
    fn spawn<F>(&self, f: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        tokio::spawn(f);
    }
}

pub struct Timer;

impl fastimer::MakeDelay for Timer {
    type Delay = tokio::time::Sleep;

    fn delay_util(&self, at: Instant) -> Self::Delay {
        tokio::time::sleep_until(tokio::time::Instant::from_std(at))
    }

    fn delay(&self, duration: Duration) -> Self::Delay {
        tokio::time::sleep(duration)
    }
}

pub struct ReportMetricsAction {
    ctx: Arc<AtriumContext>,
}

impl ReportMetricsAction {
    pub fn new(ctx: Arc<AtriumContext>) -> Self {
        ReportMetricsAction { ctx }
    }

    async fn do_report(&self) {
        let metrics = GlobalMetrics::get();
        let engine = &self.ctx.engine;

        metrics.storage.capacity.record(engine.capacity(), &[]);
        // Foyer will reserve all the space in the disk, so the used space is meaningless
        metrics.storage.used.record(engine.capacity(), &[]);
    }
}

impl BaseAction for ReportMetricsAction {
    fn name(&self) -> &str {
        "report_metrics"
    }
}

impl SimpleAction for ReportMetricsAction {
    async fn run(&mut self) {
        self.do_report().await;
    }
}
