#!/usr/bin/env bash
# Copyright 2025 ScopeDB <contact@scopedb.io>
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.


if [[ "${GITHUB_REF_TYPE}" != "tag" ]]; then
  echo "GITHUB_REF_TYPE=${GITHUB_REF_TYPE} Not a tag, skipping version check"
  exit 0
fi

VERSION=$( cargo metadata --format-version=1 --no-deps | jq -r '.packages[] | select(.name == "x") | .version' )
echo "VERSION: $VERSION"
echo "GITHUB_REF_NAME: $GITHUB_REF_NAME"
if [[ "$GITHUB_REF_NAME" != "v$VERSION" ]]; then
  echo "Version tag does not match the version in Cargo.toml"
  exit 1
fi
