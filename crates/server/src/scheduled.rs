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
use percas_metrics::GlobalMetrics;
use percas_metrics::StorageIOMetrics;

use crate::PercasContext;

#[derive(Debug, Default)]
struct MetricsSnapshot {
    disk_read_bytes: u64,
    disk_write_bytes: u64,
    disk_read_ios: u64,
    disk_write_ios: u64,
}

impl From<&foyer::Statistics> for MetricsSnapshot {
    fn from(stats: &foyer::Statistics) -> Self {
        Self {
            disk_read_bytes: stats.disk_read_bytes() as _,
            disk_write_bytes: stats.disk_write_bytes() as _,
            disk_read_ios: stats.disk_read_ios() as _,
            disk_write_ios: stats.disk_write_ios() as _,
        }
    }
}

pub struct ReportMetricsAction {
    ctx: Arc<PercasContext>,
    snapshot: ArcSwap<MetricsSnapshot>,
}

impl ReportMetricsAction {
    pub fn new(ctx: Arc<PercasContext>) -> Self {
        ReportMetricsAction {
            ctx,
            snapshot: ArcSwap::new(Arc::<MetricsSnapshot>::default()),
        }
    }

    async fn do_report(&self) {
        let metrics = GlobalMetrics::get();
        let engine = &self.ctx.engine;

        metrics.storage.capacity.record(engine.capacity(), &[]);
        // Foyer will reserve all the space in the disk, so the used space is meaningless
        metrics.storage.used.record(engine.capacity(), &[]);

        let read_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_READ);
        let write_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_WRITE);
        let current = MetricsSnapshot::from(engine.statistics().as_ref());
        let previous = self.snapshot.load();
        metrics.storage.io.bytes.add(
            current.disk_read_bytes - previous.disk_read_bytes,
            &read_label,
        );
        metrics.storage.io.bytes.add(
            current.disk_write_bytes - previous.disk_write_bytes,
            &write_label,
        );
        metrics
            .storage
            .io
            .count
            .add(current.disk_read_ios - previous.disk_read_ios, &read_label);
        metrics.storage.io.count.add(
            current.disk_write_ios - previous.disk_write_ios,
            &write_label,
        );

        self.snapshot.store(Arc::new(current));
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
