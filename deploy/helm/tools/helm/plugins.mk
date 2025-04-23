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

ifndef BUILD_DIR
$(error BUILD_DIR is not defined)
endif

ifndef BUILD_DIR_RELPATH
$(error BUILD_DIR_RELPATH is not defined)
endif

# Supposed to be run from the root of the repo.
HELM_PLUGIN_MK_SRC_DIR := tools/helm
HELM_PLUGIN_INSTALL_DIR = $(BUILD_DIR)/helm/plugins
HELM_PLUGIN_INSTALL_DIR_RELPATH = $(BUILD_DIR_RELPATH)/helm/plugins

$(HELM_PLUGIN_INSTALL_DIR):
	@mkdir -p $(HELM_PLUGIN_INSTALL_DIR)

define helm_plugin_install
	HELM_PLUGINS=$(HELM_PLUGIN_INSTALL_DIR_RELPATH) helm plugin install $(1) --version $(2)
endef

define helm_plugin_uninstall
	HELM_PLUGINS=$(HELM_PLUGIN_INSTALL_DIR_RELPATH) helm plugin uninstall $(1)
endef

define helm_plugin_list
	HELM_PLUGINS=$(HELM_PLUGIN_INSTALL_DIR_RELPATH) helm plugin list
endef

define helm_plugin_update
	HELM_PLUGINS=$(HELM_PLUGIN_INSTALL_DIR_RELPATH) helm plugin update $(1)
endef

define helm_plugin_run
	echo "helm" $(1) $(2); \
		HELM_PLUGINS=$(HELM_PLUGIN_INSTALL_DIR_RELPATH) helm $(1) $(2)
endef

include $(HELM_PLUGIN_MK_SRC_DIR)/plugin_unittest.mk
