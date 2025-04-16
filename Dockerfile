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

FROM public.ecr.aws/docker/library/rust:1.85.0-bullseye AS build
WORKDIR /build/
COPY . .
RUN ./xtask/scripts/docker-build.sh

FROM public.ecr.aws/docker/library/debian:bullseye-slim
WORKDIR /app/

COPY --from=build /build/target/dist/percas /bin/
COPY LICENSE README.md /app/

ENTRYPOINT ["/bin/percas"]
CMD ["start"]
