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

use percas_metrics::GlobalMetrics;
use percas_metrics::StorageIOMetrics;

use crate::PercasContext;

pub struct ReportMetricsAction {
    ctx: Arc<PercasContext>,
}

impl ReportMetricsAction {
    pub fn new(ctx: Arc<PercasContext>) -> Self {
        ReportMetricsAction { ctx }
    }

    async fn do_report(&self) {
        let metrics = GlobalMetrics::get();
        let engine = &self.ctx.engine;

        metrics.storage.capacity.record(engine.capacity(), &[]);
        // Foyer will reserve all the space in the disk, so the used space is meaningless
        metrics.storage.used.record(engine.capacity(), &[]);

        let stats = engine.stats();
        let read_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_READ);
        let write_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_WRITE);
        let flush_label = StorageIOMetrics::operation_labels(StorageIOMetrics::OPERATION_FLUSH);
        metrics.storage.io.bytes.add(
            stats.read_bytes.load(std::sync::atomic::Ordering::Relaxed) as u64,
            &read_label,
        );
        metrics.storage.io.bytes.add(
            stats.write_bytes.load(std::sync::atomic::Ordering::Relaxed) as u64,
            &write_label,
        );
        metrics.storage.io.count.add(
            stats.read_ios.load(std::sync::atomic::Ordering::Relaxed) as u64,
            &read_label,
        );
        metrics.storage.io.count.add(
            stats.write_ios.load(std::sync::atomic::Ordering::Relaxed) as u64,
            &write_label,
        );
        metrics.storage.io.count.add(
            stats.flush_ios.load(std::sync::atomic::Ordering::Relaxed) as u64,
            &flush_label,
        );

        // Reset the stats
        stats
            .read_ios
            .store(0, std::sync::atomic::Ordering::Relaxed);
        stats
            .read_bytes
            .store(0, std::sync::atomic::Ordering::Relaxed);
        stats
            .write_ios
            .store(0, std::sync::atomic::Ordering::Relaxed);
        stats
            .write_bytes
            .store(0, std::sync::atomic::Ordering::Relaxed);
        stats
            .flush_ios
            .store(0, std::sync::atomic::Ordering::Relaxed);
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
