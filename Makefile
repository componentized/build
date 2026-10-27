# This repository builds with its own build, its components and cli exist to smoke test it, see
# README.md

# the cli, released with an archive for each platform, and published to crates.io
CLI_BIN := hello
PUBLISH_CRATES := componentized-build-hello

# the cli installed as a tool built from the workspace, like a cli the components are built with, e.g.
# the constants cli in componentized/constants, run as `$(HELLO)`
LOCAL_TOOLS := componentized-build-hello

include build/build.mk

test: test-sync check-components validate-components
test-integration: test-hello test-hello-tool

.PHONY: test-sync ## Test syncing the build into a repository, and its drift check
test-sync:
	test/sync-test.sh

# the composed hello component, greeting through the greeter, the defaults and the logger pulled
# from componentized/logging
.PHONY: test-hello ## Run the hello component
test-hello: ${COMPONENTS_DIR}/hello/hello.wasm | $(call tool,wasmtime-cli)
	@set -e; \
	for greeting in 'greet("build")=Hello, build!' 'greet("")=Hello, world!'; do \
		call=$${greeting%%=*}; expected="\"$${greeting#*=}\""; \
		actual=$$($(WASMTIME) run --invoke "$$call" ${COMPONENTS_DIR}/hello/hello.wasm); \
		echo "$$call = $$actual"; \
		if [ "$$actual" != "$$expected" ]; then echo "expected $$call to return $$expected" >&2; exit 1; fi; \
	done

.PHONY: test-hello-tool ## Run the hello cli installed with the tools
test-hello-tool: $(call tool_prerequisite,componentized-build-hello)
	@actual=$$($(HELLO) build); \
	echo "$(notdir $(HELLO)) build = $$actual"; \
	if [ "$$actual" != "Hello, build!" ]; then echo "expected 'Hello, build!'" >&2; exit 1; fi
