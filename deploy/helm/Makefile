BUILD_DIR := build
BUILD_DIR_RELPATH := $(shell realpath .)/$(BUILD_DIR)
PERCAS_CHART_VERSION := $(shell helm show chart charts/percas | grep '^version:' | cut -d: -f2 | tr -d '[:space:]')
PERCAS_CHART_TARBALL := $(BUILD_DIR)/charts/percas-$(PERCAS_CHART_VERSION).tgz
PERCAS_CHART_DIR := charts/percas
PERCAS_CHART_SOURCE_FILES := $(shell find $(PERCAS_CHART_DIR) -type f)

##@ General

# The help target prints out all targets with their descriptions organized
# beneath their categories. The categories are represented by '##@' and the
# target descriptions by '##'. The awk commands is responsible for reading the
# entire set of makefiles included in this invocation, looking for lines of the
# file as xyz: ## something, and then pretty-format the target and help. Then,
# if there's a line with ##@ something, that gets pretty-printed as a category.
# More info on the usage of ANSI control characters for terminal formatting:
# https://en.wikipedia.org/wiki/ANSI_escape_code#SGR_parameters
# More info on the awk command:
# http://linuxcommand.org/lc3_adv_awk.php

.PHONY: help
help: ## Display this help.
	@awk 'BEGIN {FS = ":.*##"; printf "\nUsage:\n  make \033[36m<target>\033[0m\n"} /^[^[:space:]]+:.*?##/ { printf "  \033[36m%-28s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)

include tools/helm/plugins.mk

##@ Development
$(BUILD_DIR):
	@mkdir -p $(BUILD_DIR)

$(BUILD_DIR)/charts:
	@mkdir -p $(BUILD_DIR)/charts

.PHONY: chart/percas/bump-major
chart/percas/bump-major:
	@CURRENT_VERSION=$$(grep '^version:' $(PERCAS_CHART_DIR)/Chart.yaml | awk '{print $$2}'); \
	MAJOR=$$(echo $$CURRENT_VERSION | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT_VERSION | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT_VERSION | cut -d. -f3); \
	NEW_MAJOR=$$((MAJOR + 1)); \
	NEW_VERSION="$$NEW_MAJOR.0.0"; \
	sed -i.bak "s/^version: .*/version: $$NEW_VERSION/" $(PERCAS_CHART_DIR)/Chart.yaml && rm $(PERCAS_CHART_DIR)/Chart.yaml.bak

.PHONY: chart/percas/bump-minor
chart/percas/bump-minor:
	@CURRENT_VERSION=$$(grep '^version:' $(PERCAS_CHART_DIR)/Chart.yaml | awk '{print $$2}'); \
	MAJOR=$$(echo $$CURRENT_VERSION | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT_VERSION | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT_VERSION | cut -d. -f3); \
	NEW_MINOR=$$((MINOR + 1)); \
	NEW_VERSION="$$MAJOR.$$NEW_MINOR.0"; \
	sed -i.bak "s/^version: .*/version: $$NEW_VERSION/" $(PERCAS_CHART_DIR)/Chart.yaml && rm $(PERCAS_CHART_DIR)/Chart.yaml.bak

.PHONY: chart/percas/bump-patch
chart/percas/bump-patch:
	@CURRENT_VERSION=$$(grep '^version:' $(PERCAS_CHART_DIR)/Chart.yaml | awk '{print $$2}'); \
	MAJOR=$$(echo $$CURRENT_VERSION | cut -d. -f1); \
	MINOR=$$(echo $$CURRENT_VERSION | cut -d. -f2); \
	PATCH=$$(echo $$CURRENT_VERSION | cut -d. -f3); \
	NEW_PATCH=$$((PATCH + 1)); \
	NEW_VERSION="$$MAJOR.$$MINOR.$$NEW_PATCH"; \
	sed -i.bak "s/^version: .*/version: $$NEW_VERSION/" $(PERCAS_CHART_DIR)/Chart.yaml && rm $(PERCAS_CHART_DIR)/Chart.yaml.bak

.PHONY: chart/percas/set-app-version
chart/percas/set-app-version:
	@if [ -z "$(VERSION)" ]; then \
		echo "Error: VERSION is not set. Usage: make chart/percas/set-app-version VERSION=<version>"; \
		exit 1; \
	fi; \
	sed -i.bak "s/image: percas\/percas:.*/image: percas\/percas:$(VERSION)/" $(PERCAS_CHART_DIR)/Chart.yaml && \
	sed -i.bak "s/^appVersion: .*/appVersion: \"$(VERSION)\"/" $(PERCAS_CHART_DIR)/Chart.yaml && rm $(PERCAS_CHART_DIR)/Chart.yaml.bak && \
	$(MAKE) chart/percas/bump-patch

.PHONY: clean
clean: ## Clean up build artifacts and local cache.
	@rm -rf $(BUILD_DIR)

.PHONY: clean/artifacts
clean/artifacts: ## Clean up build artifacts.
	@rm -rf $(BUILD_DIR)/charts

.PHONY: dependency/build
dependency/build: repo/add $(BUILD_DIR)/charts ## Build the dependencies.
	@helm dependency build charts/percas

.PHONY: dependency/update
dependency/update: repo/add $(BUILD_DIR)/charts ## Update the dependencies.
	@helm dependency update charts/percas

.PHONY: fmt/docs
fmt/docs: ## Format the docs.
	@find docs -name "*.yaml" -type f -exec yq eval -P -i {} \;

.PHONY: fmt/test
fmt/test: ## Format the test files.
	@find $(PERCAS_CHART_DIR)/tests -name "*.yaml" -type f -exec yq eval -P -i {} \;

.PHONY: lint
lint: ## Run lint.
	@helm lint --strict $(PERCAS_CHART_DIR)

.PHONY: package
package: $(PERCAS_CHART_TARBALL) ## Package the chart.

$(PERCAS_CHART_TARBALL): $(BUILD_DIR)/charts $(PERCAS_CHART_SOURCE_FILES)
	@helm package charts/percas -d $(BUILD_DIR)/charts

.PHONY: repo/add
repo/add:
	@helm repo add bitnami https://charts.bitnami.com/bitnami

.PHONY: test
test: helm_plugin_unittest_install ## Run tests.
	@$(call helm_unittest_run_all,charts/percas)
	@find $(PERCAS_CHART_DIR)/tests -depth -name "__snapshot__" -type d -exec rm -r {} \;

.PHONY: test/diff
test/diff: helm_plugin_unittest_install ## Run tests on uncommitted changes.
	@$(call helm_unittest_run_uncommitted,charts/percas)
	@find $(PERCAS_CHART_DIR)/tests -depth -name "__snapshot__" -type d -exec rm -r {} \;

##@ Example

.PHONY: example/install
example/install: $(PERCAS_CHART_TARBALL) ## Install the example.
	helm install percas $(PERCAS_CHART_TARBALL)

.PHONY: example/uninstall
example/uninstall: ## Uninstall the example.
	helm uninstall percas

.PHONY: example/test
example/test: ## Test the example.
	helm test percas --hide-notes --logs
