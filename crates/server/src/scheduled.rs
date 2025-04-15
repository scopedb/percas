use std::sync::Arc;

use atrium_metrics::GlobalMetrics;

use crate::AtriumContext;

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

impl fastimer::schedule::BaseAction for ReportMetricsAction {
    fn name(&self) -> &str {
        "report_metrics"
    }
}

impl fastimer::schedule::SimpleAction for ReportMetricsAction {
    async fn run(&mut self) {
        self.do_report().await;
    }
}
