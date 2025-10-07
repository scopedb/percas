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

use std::borrow::Cow;
use std::time::Duration;

use logforth::append;
use logforth::append::file::FileBuilder;
use logforth::diagnostic::FastraceDiagnostic;
use logforth::diagnostic::StaticDiagnostic;
use logforth::filter::EnvFilter;
use logforth::filter::env_filter::EnvFilterBuilder;
use logforth::layout;
use opentelemetry_otlp::WithExportConfig;
use percas_core::MetricsConfig;
use percas_core::Runtime;
use percas_core::TelemetryConfig;
use percas_core::TracesConfig;

pub fn init(
    rt: &Runtime,
    service_name: &'static str,
    node_id: uuid::Uuid,
    config: TelemetryConfig,
) -> Vec<Box<dyn Send + Sync + 'static>> {
    let mut drop_guards = vec![];
    if let Some(metrics) = &config.metrics {
        drop_guards.extend(init_metrics(rt, service_name, metrics));
    }
    if let Some(traces) = &config.traces {
        drop_guards.extend(init_traces(rt, service_name, traces));
    }
    drop_guards.extend(init_logs(rt, service_name, node_id, &config));
    drop_guards
}

fn init_metrics(
    rt: &Runtime,
    service_name: &'static str,
    config: &MetricsConfig,
) -> Vec<Box<dyn Send + Sync + 'static>> {
    let MetricsConfig {
        opentelemetry: Some(config),
        ..
    } = config
    else {
        return vec![];
    };

    rt.block_on(async {
        let exporter = opentelemetry_otlp::MetricExporter::builder()
            .with_tonic()
            .with_protocol(opentelemetry_otlp::Protocol::Grpc)
            .with_endpoint(&config.otlp_endpoint)
            .build()
            .expect("initialize oltp metrics exporter");
        let reader = opentelemetry_sdk::metrics::PeriodicReader::builder(exporter)
            .with_interval(Duration::from_secs_f64(config.push_interval.as_secs_f64()))
            .build();
        let resource = opentelemetry_sdk::Resource::builder()
            .with_attributes([opentelemetry::KeyValue::new("service.name", service_name)])
            .build();
        let provider = opentelemetry_sdk::metrics::SdkMeterProvider::builder()
            .with_reader(reader)
            .with_resource(resource)
            .build();

        opentelemetry::global::set_meter_provider(provider.clone());

        vec![Box::new(scopeguard::guard((), move |_| {
            provider
                .shutdown()
                .inspect_err(|err| log::error!("failed to shutdown metrics provider: {err:?}"))
                .ok();
        })) as _]
    })
}

fn init_traces(
    rt: &Runtime,
    service_name: &'static str,
    config: &TracesConfig,
) -> Vec<Box<dyn Send + Sync + 'static>> {
    let TracesConfig {
        opentelemetry: Some(opentelemetry),
        ..
    } = config
    else {
        return vec![];
    };

    let resource = opentelemetry_sdk::Resource::builder()
        .with_attributes([opentelemetry::KeyValue::new("service.name", service_name)])
        .build();
    let otlp_reporter = rt.block_on(async move {
        fastrace_opentelemetry::OpenTelemetryReporter::new(
            opentelemetry_otlp::SpanExporter::builder()
                .with_tonic()
                .with_endpoint(&opentelemetry.otlp_endpoint)
                .with_protocol(opentelemetry_otlp::Protocol::Grpc)
                .with_timeout(opentelemetry_otlp::OTEL_EXPORTER_OTLP_TIMEOUT_DEFAULT)
                .build()
                .expect("initialize oltp trace exporter"),
            Cow::Owned(resource),
            opentelemetry::InstrumentationScope::builder(service_name).build(),
        )
    });
    fastrace::set_reporter(otlp_reporter, fastrace::collector::Config::default());

    vec![Box::new(scopeguard::guard((), |_| {
        struct NoopReporter;
        impl fastrace::collector::Reporter for NoopReporter {
            fn report(&mut self, _batch: Vec<fastrace::prelude::SpanRecord>) {}
        }
        fastrace::flush();
        fastrace::set_reporter(NoopReporter, fastrace::collector::Config::default());
    }))]
}

fn init_logs(
    rt: &Runtime,
    service_name: &'static str,
    node_id: uuid::Uuid,
    config: &TelemetryConfig,
) -> Vec<Box<dyn Send + Sync + 'static>> {
    let static_diagnostic = {
        let mut static_diagnostic = StaticDiagnostic::default();
        static_diagnostic.insert("node_id", node_id.to_string());
        static_diagnostic
    };

    let drop_guards: Vec<Box<dyn Send + Sync + 'static>> = Vec::new();
    let mut builder = logforth::starter_log::builder();

    // fastrace appender
    if let Some(TracesConfig {
        capture_log_filter, ..
    }) = &config.traces
    {
        builder = builder.dispatch(|b| {
            b.filter(make_rust_log_filter(capture_log_filter))
                .append(append::FastraceEvent::default())
        });
    }

    // file appender
    if let Some(file) = &config.logs.file {
        let mut b = FileBuilder::new(&file.dir, service_name)
            .layout(layout::JsonLayout::default())
            .rollover_hourly()
            .filename_suffix("log");

        if let Some(max_files) = file.max_files {
            b = b.max_log_files(max_files);
        }

        let append = b.build().expect("failed to init file appender");
        builder = builder.dispatch(|b| {
            b.filter(make_rust_log_filter(&file.filter))
                .diagnostic(FastraceDiagnostic::default())
                .diagnostic(static_diagnostic.clone())
                .append(append)
        });
    }

    // stderr appender
    if let Some(stderr) = &config.logs.stderr {
        builder = builder.dispatch(|b| {
            b.filter(make_rust_log_filter_with_default_env(&stderr.filter))
                .diagnostic(FastraceDiagnostic::default())
                .diagnostic(static_diagnostic.clone())
                .append(append::Stderr::default().with_layout(layout::TextLayout::default()))
        });
    }

    // opentelemetry appender
    if let Some(opentelemetry) = &config.logs.opentelemetry {
        let filter = make_rust_log_filter(&opentelemetry.filter);
        let appender = rt.block_on(async {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_tonic()
                .with_endpoint(&opentelemetry.otlp_endpoint)
                .with_protocol(opentelemetry_otlp::Protocol::Grpc)
                .build()
                .expect("failed to initialize opentelemetry logger");

            append::opentelemetry::OpentelemetryLogBuilder::new(service_name, exporter)
                .label("service.name", service_name)
                .build()
        });
        builder = builder.dispatch(|b| {
            b.filter(filter)
                .diagnostic(FastraceDiagnostic::default())
                .diagnostic(static_diagnostic.clone())
                .append(appender)
        });
    }

    // apply returns err if already set; ignored
    let _ = builder.try_apply();

    drop_guards
}

fn make_rust_log_filter(filter: &str) -> EnvFilter {
    let builder = EnvFilterBuilder::try_from_spec(filter)
        .unwrap_or_else(|_| panic!("failed to parse filter: {filter}"));
    builder.build()
}

fn make_rust_log_filter_with_default_env(filter: &str) -> EnvFilter {
    if let Ok(filter) = std::env::var("RUST_LOG") {
        make_rust_log_filter(&filter)
    } else {
        make_rust_log_filter(filter)
    }
}
