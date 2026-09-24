// Copyright 2025 ScopeDB <contact@scopedb.io>
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;

use arc_swap::ArcSwap;
use percas_core::StorageStatistics;
use percas_metrics::GlobalMetrics;
use percas_metrics::StorageIOMetrics;

use crate::PercasContext;

pub struct ReportMetricsAction {
    ctx: Arc<PercasContext>,
    snapshot: ArcSwap<StorageStatistics>,
}

impl ReportMetricsAction {
    pub fn new(ctx: Arc<PercasContext>) -> Self {
        ReportMetricsAction {
            ctx,
            snapshot: ArcSwap::new(Arc::<StorageStatistics>::default()),
        }
    }

    async fn do_report(&self) {
        let metrics = GlobalMetrics::get();

        let engine = &self.ctx.engine;
        // Both engines reserve their backing storage.
        metrics.storage.used.record(engine.capacity(), &[]);
        metrics.storage.capacity.record(engine.capacity(), &[]);

        let current = match engine.statistics() {
            Ok(stats) => stats,
            Err(err) => {
                log::warn!(err:?; "failed to collect storage statistics");
                return;
            }
        };
        let previous = self.snapshot.load();
        let difference = StorageStatistics {
            disk_read_bytes: current
                .disk_read_bytes
                .saturating_sub(previous.disk_read_bytes),
            disk_write_bytes: current
                .disk_write_bytes
                .saturating_sub(previous.disk_write_bytes),
            disk_read_ios: current.disk_read_ios.saturating_sub(previous.disk_read_ios),
            disk_write_ios: current
                .disk_write_ios
                .saturating_sub(previous.disk_write_ios),
        };
        self.snapshot.store(Arc::new(current));

        let io = &metrics.storage.io;
        let read_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_READ);
        let write_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_WRITE);
        io.bytes.add(difference.disk_read_bytes, &read_label);
        io.bytes.add(difference.disk_write_bytes, &write_label);
        io.count.add(difference.disk_read_ios, &read_label);
        io.count.add(difference.disk_write_ios, &write_label);
    }
}

impl fastimer::schedule::SimpleAction for ReportMetricsAction {
    fn name(&self) -> &str {
        "report_metrics"
    }

    async fn run(&mut self) {
        self.do_report().await;
    }
}
