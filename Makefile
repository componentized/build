# This repository builds with its own build, its components exist to smoke test it, as does its cli,
# componentized-build, which updates a repository's dependencies, see README.md

# the cli, released with an archive for each platform, and published to crates.io
CLI_BIN := componentized-build
PUBLISH_CRATES := componentized-build-cli

# the cli installed as a tool built from the workspace, like a cli the components are built with, e.g.
# the constants cli in componentized/constants, run as `$(COMPONENTIZED_BUILD)`
LOCAL_TOOLS := componentized-build-cli

# the greeter's release build is optimized again with wasm-opt
COMPONENT_OPTIMIZE := true
# wasm-opt drops wasi:clocks imports from greeter-wasip3's release build that the standard library
# brings in, its debug build keeps them, see CHECK_COMPONENTS_SKIP in build/components.mk
CHECK_COMPONENTS_SKIP := greeter-wasip2 greeter-wasip3

include build/build.mk

test: test-sync check-components validate-components
test-integration: test-hello test-greeters test-cli

.PHONY: test-sync ## Test syncing the build into a repository, and its drift check
test-sync:
	test/sync-test.sh

# the hello components, composed and plugged, greeting through the greeter, the defaults and the
# logger pulled from componentized/logging
.PHONY: test-hello ## Run the hello components
test-hello: ${COMPONENTS_DIR}/hello/hello.wasm ${COMPONENTS_DIR}/hello-plug/hello-plug.wasm | $(call tool,wasmtime-cli)
	@set -e; \
	for component in hello hello-plug; do \
		for greeting in 'greet("build")=Hello, build!' 'greet("")=Hello, world!'; do \
			call=$${greeting%%=*}; expected="\"$${greeting#*=}\""; \
			actual=$$($(WASMTIME) run --invoke "$$call" ${COMPONENTS_DIR}/$$component/$$component.wasm); \
			echo "$$component $$call = $$actual"; \
			if [ "$$actual" != "$$expected" ]; then echo "expected $$call of $$component to return $$expected" >&2; exit 1; fi; \
		done; \
	done

# the greeters built for wasm32-wasip2 and wasm32-wasip3, release and debug builds, greeting with the
# default name from the environment, wasip3's on a component model cooperative thread
.PHONY: test-greeters ## Run the greeters built for wasm32-wasip2 and wasm32-wasip3
test-greeters: $(foreach component,greeter-wasip2 greeter-wasip3,${COMPONENTS_DIR}/$(component)/$(component).wasm ${COMPONENTS_DIR}/$(component)/$(component).debug.wasm) | $(call tool,wasmtime-cli)
	@set -e; \
	for component in greeter-wasip2 greeter-wasip3; do \
		flags=""; \
		if [ "$$component" = greeter-wasip3 ]; then flags="-S p3 -W component-model-async,component-model-threading"; fi; \
		for file in $$component.wasm $$component.debug.wasm; do \
			for greeting in 'greet("build")=Hello, build!' 'greet("")=Hello, world!'; do \
				call=$${greeting%%=*}; expected="\"$${greeting#*=}\""; \
				actual=$$($(WASMTIME) run $$flags --env GREETER_NAME=world --invoke "$$call" ${COMPONENTS_DIR}/$$component/$$file); \
				echo "$$file $$call = $$actual"; \
				if [ "$$actual" != "$$expected" ]; then echo "expected $$call of $$file to return $$expected" >&2; exit 1; fi; \
			done; \
		done; \
	done

# the cli installed with the tools, run in this repository as the CI workflow runs the cli unpacked
# from its archive, see scripts/smoke-test-cli.sh
.PHONY: test-cli ## Run the componentized-build cli installed with the tools
test-cli: $(call tool_prerequisite,componentized-build-cli)
	scripts/smoke-test-cli.sh $(COMPONENTIZED_BUILD)
