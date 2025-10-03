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

use std::sync::LazyLock;

use opentelemetry::KeyValue;
use opentelemetry::metrics::Counter;
use opentelemetry::metrics::Gauge;
use opentelemetry::metrics::Histogram;
use opentelemetry::metrics::Meter;

pub struct GlobalMetrics {
    pub meter: Meter,
    pub storage: StorageMetrics,
    pub operation: OperationMetrics,
}

impl GlobalMetrics {
    fn new() -> Self {
        let meter = opentelemetry::global::meter("percas");
        Self {
            storage: StorageMetrics::new(meter.clone()),
            operation: OperationMetrics::new(meter.clone()),
            meter,
        }
    }

    pub fn get() -> &'static GlobalMetrics {
        static GLOBAL_METRICS: LazyLock<GlobalMetrics> = LazyLock::new(GlobalMetrics::new);
        &GLOBAL_METRICS
    }
}

pub struct StorageMetrics {
    pub capacity: Gauge<u64>,
    pub used: Gauge<u64>,
    pub io: StorageIOMetrics,
}

impl StorageMetrics {
    pub fn new(meter: Meter) -> Self {
        Self {
            capacity: meter
                .u64_gauge("percas.storage.capacity")
                .with_description("The total capacity of the storage")
                .with_unit("byte")
                .build(),
            used: meter
                .u64_gauge("percas.storage.used")
                .with_description("The used capacity of the storage")
                .with_unit("byte")
                .build(),

            io: StorageIOMetrics::new(meter),
        }
    }
}

pub struct StorageIOMetrics {
    pub count: Counter<u64>,
    pub bytes: Counter<u64>,
}

impl StorageIOMetrics {
    pub fn new(meter: Meter) -> Self {
        Self {
            count: meter
                .u64_counter("percas.storage.io.count")
                .with_description("The number of IOs")
                .build(),
            bytes: meter
                .u64_counter("percas.storage.io.bytes")
                .with_description("The number of IO bytes")
                .with_unit("byte")
                .build(),
        }
    }

    pub const OPERATION_READ: &str = "read";
    pub const OPERATION_WRITE: &str = "write";
    pub const OPERATION_FLUSH: &str = "flush";

    pub fn operation_labels(operation: &str) -> [KeyValue; 1] {
        [KeyValue::new("operation", operation.to_string())]
    }
}

pub struct OperationMetrics {
    pub count: Counter<u64>,
    pub bytes: Counter<u64>,
    pub duration: Histogram<f64>,
}

impl OperationMetrics {
    pub fn new(meter: Meter) -> Self {
        Self {
            count: meter
                .u64_counter("percas.operation.count")
                .with_description("The number of operations")
                .build(),
            bytes: meter
                .u64_counter("percas.operation.bytes")
                .with_description("The number of bytes")
                .with_unit("byte")
                .build(),
            duration: meter
                .f64_histogram("percas.operation.duration")
                .with_description("The duration of the operation")
                .with_unit("second")
                .with_boundaries(
                    [
                        0.0001, 0.0005, 0.001, 0.005, 0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 5.0,
                    ]
                    .into(),
                )
                .build(),
        }
    }

    pub const OPERATION_GET: &str = "get";
    pub const OPERATION_PUT: &str = "put";
    pub const OPERATION_DELETE: &str = "delete";
    pub const OPERATION_UNKNOWN: &str = "unknown";

    pub const STATUS_SUCCESS: &str = "ok";
    pub const STATUS_NOT_FOUND: &str = "not_found";
    pub const STATUS_FAILURE: &str = "error";
    pub const STATUS_REDIRECT: &str = "redirect";

    pub fn operation_labels(operation: &str, status: &str) -> [KeyValue; 2] {
        [
            KeyValue::new("operation", operation.to_string()),
            KeyValue::new("status", status.to_string()),
        ]
    }
}
