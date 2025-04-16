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

use behavior_tests::Testkit;
use behavior_tests::harness;
use behavior_tests::render_hex;
use insta::assert_snapshot;
use test_harness::test;

#[test(harness)]
async fn test_put_get(testkit: Testkit) {
    testkit.client.put("key", "value".as_bytes()).await.unwrap();
    let value = testkit.client.get("key").await.unwrap();
    assert_snapshot!(
        render_hex(value.unwrap()),
        @r"
        Length: 5 (0x5) bytes
        0000:   76 61 6c 75 65            value
        "
    );
}
