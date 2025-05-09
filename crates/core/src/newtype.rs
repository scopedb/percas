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

use std::num::NonZeroUsize;

use serde::Deserialize;
use serde::Serialize;

#[derive(Default, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDuration(jiff::SignedDuration);

impl From<SignedDuration> for jiff::SignedDuration {
    fn from(value: SignedDuration) -> Self {
        value.0
    }
}

impl From<jiff::SignedDuration> for SignedDuration {
    fn from(value: jiff::SignedDuration) -> Self {
        Self(value)
    }
}

impl std::ops::Deref for SignedDuration {
    type Target = jiff::SignedDuration;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for SignedDuration {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Throttle config for the device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct DiskThrottle {
    /// The maximum write iops for the device.
    pub write_iops: Option<NonZeroUsize>,
    /// The maximum read iops for the device.
    pub read_iops: Option<NonZeroUsize>,
    /// The maximum write throughput for the device.
    pub write_throughput: Option<NonZeroUsize>,
    /// The maximum read throughput for the device.
    pub read_throughput: Option<NonZeroUsize>,
    /// The iops counter for the device.
    pub iops_counter: IopsCounter,
}

impl From<DiskThrottle> for foyer::Throttle {
    fn from(value: DiskThrottle) -> Self {
        Self {
            write_iops: value.write_iops,
            read_iops: value.read_iops,
            write_throughput: value.write_throughput,
            read_throughput: value.read_throughput,
            iops_counter: value.iops_counter.into(),
        }
    }
}

impl From<foyer::Throttle> for DiskThrottle {
    fn from(value: foyer::Throttle) -> Self {
        Self {
            write_iops: value.write_iops,
            read_iops: value.read_iops,
            write_throughput: value.write_throughput,
            read_throughput: value.read_throughput,
            iops_counter: value.iops_counter.into(),
        }
    }
}

/// Device iops counter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(test, derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[serde(tag = "mode")]
pub enum IopsCounter {
    /// Count 1 iops for each read/write.
    #[serde(rename = "per_io")]
    PerIo,
    /// Count 1 iops for each read/write with the size of the i/o.
    #[serde(rename = "per_io_size")]
    PerIoSize(NonZeroUsize),
}

impl From<IopsCounter> for foyer::IopsCounter {
    fn from(value: IopsCounter) -> Self {
        match value {
            IopsCounter::PerIo => foyer::IopsCounter::PerIo,
            IopsCounter::PerIoSize(size) => foyer::IopsCounter::PerIoSize(size),
        }
    }
}

impl From<foyer::IopsCounter> for IopsCounter {
    fn from(value: foyer::IopsCounter) -> Self {
        match value {
            foyer::IopsCounter::PerIo => IopsCounter::PerIo,
            foyer::IopsCounter::PerIoSize(size) => IopsCounter::PerIoSize(size),
        }
    }
}

#[cfg(test)]
mod json_schema {
    use std::borrow::Cow;

    use schemars::Schema;
    use schemars::SchemaGenerator;
    use schemars::json_schema;

    use super::*;

    impl schemars::JsonSchema for SignedDuration {
        fn always_inline_schema() -> bool {
            true
        }

        fn schema_name() -> Cow<'static, str> {
            Cow::Borrowed("SignedDuration")
        }

        fn schema_id() -> Cow<'static, str> {
            Cow::Borrowed("jiff::SignedDuration")
        }

        fn json_schema(_: &mut SchemaGenerator) -> Schema {
            json_schema!({
                "type": "string",
                "format": "duration",
            })
        }
    }
}
