# This repository builds with its own build, its components and cli exist to smoke test it, see
# README.md

# the cli, released with an archive for each platform, and published to crates.io
CLI_BIN := hello
PUBLISH_CRATES := componentized-build-hello

include build/build.mk

test: test-sync
test-integration: test-hello

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
