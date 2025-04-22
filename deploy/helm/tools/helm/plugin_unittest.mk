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
