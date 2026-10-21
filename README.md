# Componentized build

The build shared by the componentized repositories: make files, scripts and GitHub workflows, synced into each repository at a released version.

Each repository keeps a copy of the build and records the version it came from, so a release tag of a repository contains the exact build that published it. A copy is never edited in place. Changes are made here, released, and each repository updates when it's ready.

## How a repository uses the build

```
Makefile                   repository-owned: settings, `include build/build.mk`, its own targets
build/                     synced, never edited in the repository
  VERSION                  the release synced, e.g. v0.2.0
  MANIFEST                 a checksum of each synced file, checked by `make check-build`
  build.mk                 the targets every repository has, includes the other make files
  tools.mk wit.mk components.mk publish.mk sync.mk pinned.jq tool-versions
  scripts/                 publish.sh, bump-version.sh, update-build.sh, ...
.github/workflows/         ci.yaml, bump-version.yaml, release.yaml, update-build.yaml: synced
tools/Cargo.toml           repository-owned, the pinned tools, bumped by dependabot
```

[`SYNC`](SYNC) lists the paths synced into each repository.

### Updating

`make update-build` updates to the latest release, `make update-build BUILD_VERSION=v0.2.0` to a given one. It removes the files synced from the previous version, copies in the release's files, and writes `build/VERSION` and `build/MANIFEST`.

The `Update build` workflow does the same each week, and opens a pull request when there is a newer release, like dependabot. The pull request is created with the custodian GitHub App, the `GITHUB_TOKEN` can't change workflow files. The app needs the `contents`, `workflows` and `pull-requests` write permissions, and the `CUSTODIAN_CLIENT_ID` and `CUSTODIAN_PRIVATE_KEY` secrets must be available to the repository.

The versions of the tools stay in the repository's `tools/Cargo.toml`, bumped by dependabot. When a release of the build needs a newer tool it says so in [`build/tool-versions`](build/tool-versions), and the update raises a tool pinned to an older version.

### Drift

The CI workflow runs `build/scripts/check-build.sh` first, which fails when a synced file was changed or removed, or a file was added to `build/`. Fix the build here rather than in the repository, so the repositories don't drift apart again.

## Settings

The repository's `Makefile` sets these before `include build/build.mk`. Each has a default that fits most repositories, detected where it can be.

| Setting | Default | |
| --- | --- | --- |
| `COMPONENT_TARGET` | `wasm32-unknown-unknown` | the rust target of the components cargo builds. `wasm32-wasip3` installs wasi-sdk with the tools, or uses `WASI_SDK_PATH` |
| `COMPONENT_PROFILE` | `component` when Cargo.toml has a `[profile.component]`, otherwise `release` | the cargo profile of a component's release build |
| `COMPONENT_DEBUG_PROFILE` | `component-debug` when Cargo.toml has a `[profile.component-debug]`, otherwise `dev` | the cargo profile of a component's debug build |
| `COMPONENT_OPTIMIZE` | set when `wasm-opt` is pinned in `tools/Cargo.toml` | optimize the release builds again for size with wasm-opt, `COMPONENT_OPTIMIZE=` builds without |
| `CARGO_COMPONENT_DEPS` | | files the components cargo builds need besides their sources, e.g. a component the library embeds |
| `LOCAL_TOOLS` | | tools built from the workspace rather than pinned in `tools/Cargo.toml`, each a package, e.g. `componentized-constants-cli` in componentized/constants. Installed with the pinned tools, with a variable for each binary like a pinned tool, e.g. `$(CONSTANTS)`, and installed again when the sources of the package, or of the workspace packages it depends on, change. The components built with a local tool are built again when it changes. A local tool replaces a pinned tool of the same name, and a file it embeds is added as a prerequisite, e.g. `$(call tool,componentized-static-config-cli): $(CONFIG_ADAPTER_WASM)` |
| `CLI_BIN` | | the cli built from the repository, e.g. `constants`. The CI workflow builds it for each platform, smoke tests it with `scripts/smoke-test-cli.sh <cli> <dir>` when the repository has one, and attaches the archives to the release |
| `PUBLISH_CRATES` | | the crates the release workflow publishes to crates.io, e.g. `componentized-constants componentized-constants-cli`, checked with `cargo publish --dry-run` by the CI workflow |

Publishing reads `DRY_RUN`: `make publish DRY_RUN=true` shows each image it would push, with its annotations, without pushing or signing anything. In the workflows the repository variable `DRY_RUN=true` does the same, also skipping the login to the registry and the GitHub release the CI workflow drafts for a tag, and the release workflow checks the crates with `cargo publish --dry-run` rather than publishing them.

## Targets

`make` runs `tools`, `generate`, `components` and `test`. Other targets: `wit`, `publish`, `tools-path`, `clean`, `clean-all`, `update-build`, `check-build`, `print-<variable>`.

A repository extends a target by adding prerequisites after the include, make merges them with the target's own:

| Target | Run by | Extended with, e.g. |
| --- | --- | --- |
| `generate` | the CI workflow, which fails when the generated files differ from those committed | `generate: config` |
| `test` | `make`, the CI workflow | `test: test-grammar` |
| `test-integration` | the CI workflow, after `make test` | `test-integration: test-valkey`, tests that need a server in a container |
| `tools` | `make`, the workflows | `tools: $(WASI_SDK)`, a tool installed other than with cargo |

## Moving a repository to the build

1. Remove `make/`, and `scripts/publish.sh`, `scripts/bump-version.sh` and `scripts/wac-deps.sh`.
2. Sync the build, from the repository's root:
   ```sh
   mkdir -p build/scripts
   curl -fsSL https://raw.githubusercontent.com/componentized/build/v0.1.0/build/scripts/update-build.sh -o build/scripts/update-build.sh
   bash build/scripts/update-build.sh v0.1.0
   ```
3. Replace the shared part of the `Makefile` with `include build/build.mk`: the settings before it, the repository's own targets after it. A repository-specific CI step becomes a prerequisite of a target, e.g. `test-integration: test-valkey`.
4. Remove the `github-actions` ecosystem from `.github/dependabot.yml`, the workflows are synced, their actions are bumped here.
5. Add `wkg.lock.orig` to `.gitignore`.
6. Check `make` and `make check-build` pass.

The default version of the `bump-version` workflow, the repository's current version, is the one line of a synced file the repository keeps: `make update-build` carries it forward, from the workflow of the previous build too, or starts it at the interface version, and `make check-build` leaves it out. `build/scripts/bump-version.sh` updates it with each bump.

## This repository builds with the build

This repository is built by its own build, with the synced workflows, like any other repository. Its components and cli are just enough to smoke test each kind of thing the build makes:

| | |
| --- | --- |
| [`wit/`](wit/worlds.wit) | `componentized:build`, the public interface, built into `interface.wasm`: a `greeter`, and its `defaults` |
| [`greeter`](components/greeter/) | a component built with cargo, implementing `greeter`, importing `defaults` and `wasi:logging` |
| [`defaults`](components/defaults/) | a component built with the constants cli, implementing `defaults` from the `@value` of each function |
| [`dep-componentized-logging-to-stderr`](components/dep-componentized-logging-to-stderr/) | a component pulled from a registry, wasi:logging written to stderr |
| [`hello`](components/hello/) | a composition of the three, run by `make test-integration` with wasmtime |
| [`crates/hello`](crates/hello/) | a cli, built for each platform, smoke tested and checked with `cargo publish --dry-run`, and installed as a local tool, `$(HELLO)`, run by `make test-integration` |

`make test` also runs [`test/sync-test.sh`](test/sync-test.sh), which syncs the build into a new repository and checks the drift check. The `Lint` workflow runs shellcheck. As the source of the build rather than a copy of it, this repository has no `build/VERSION` or `build/MANIFEST`: `make check-build` passes, and the `Update build` workflow skips it.

The repository variable `DRY_RUN` is `true` here: pushes to main and tags go through publishing without publishing anything.

## Changing the build

Change the files here, and check them with `make`. Then try the change in a repository before releasing it:

```sh
BUILD_SOURCE=../build make update-build
```

### Releasing

Release with the `Bump version` workflow like any other repository, the version of the build is the version of its `componentized:build` interface. It tags the release, e.g. `v0.1.0`, and moves on to the next pre-release.

With `DRY_RUN`, no GitHub release is drafted for the tag, publish one for it by hand: the `Update build` workflow of each repository looks for the latest release. Say in the release notes when a repository has to change, e.g. a new setting or a tool it must pin.
