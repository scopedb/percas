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

HELM_PLUGIN_UNITTEST_NAME=unittest

HELM_UNITTEST_URL=https://github.com/helm-unittest/helm-unittest.git
# https://github.com/helm-unittest/helm-unittest/releases
HELM_UNITTEST_VERSION=v0.8.0

HELM_PLUGIN_UNITTEST_TARGET_FILE=$(HELM_PLUGIN_INSTALL_DIR)/helm-unittest.git

$(HELM_PLUGIN_UNITTEST_TARGET_FILE): $(HELM_PLUGIN_INSTALL_DIR) $(HELM_PLUGIN_MK_SRC_DIR)/plugin_unittest.mk
	@rm -rf $(HELM_PLUGIN_INSTALL_DIR)/helm-unittest.git
	$(call helm_plugin_install,$(HELM_UNITTEST_URL),$(HELM_UNITTEST_VERSION))

.PHONY: helm_plugin_unittest_install
helm_plugin_unittest_install: $(HELM_PLUGIN_UNITTEST_TARGET_FILE)

.PHONY: helm_plugin_unittest_uninstall
helm_plugin_unittest_uninstall:
	$(call helm_plugin_uninstall,unittest)

define helm_unittest_run
	$(call helm_plugin_run,unittest,--strict -f '$(1)' $(2))
endef

define helm_unittest_run_all
	$(call helm_plugin_run,unittest,--strict -f 'tests/**/*_test.yaml' $(1))
endef

define helm_unittest_run_uncommitted
	$(eval uncommitted_files := $(shell git diff --name-status head -- $(1)/tests | grep -v '^D' | grep '_test.yaml' | awk '{print $$NF}' | sed -e "s|$(1)/||g"))
	if [ "$(uncommitted_files)" != "" ]; then \
		$(call helm_plugin_run,unittest,$(addprefix -f ,$(uncommitted_files)) $(1)); \
	fi
endef
