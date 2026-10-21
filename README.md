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
| `CONSTANTS`, `STATIC_CONFIG` | the pinned `componentized-constants-cli` and `componentized-static-config-cli` | a repository that builds its own cli sets its path, e.g. `CONSTANTS = $(TOOLS_BIN)/constants`, with `=` as the tools aren't read yet, and adds a rule for it |
| `CLI_BIN` | | the cli built from the repository, e.g. `constants`. The CI workflow builds it for each platform, smoke tests it with `scripts/smoke-test-cli.sh <cli> <dir>` when the repository has one, and attaches the archives to the release |
| `PUBLISH_CRATES` | | the crates the release workflow publishes to crates.io, e.g. `componentized-constants componentized-constants-cli`, checked with `cargo publish --dry-run` by the CI workflow |

## Targets

`make` runs `tools`, `generate`, `components` and `test`. Other targets: `wit`, `publish`, `tools-path`, `clean`, `clean-all`, `update-build`, `check-build`, `print-<variable>`.

A repository extends a target by adding prerequisites after the include, make merges them with the target's own:

| Target | Run by | Extended with, e.g. |
| --- | --- | --- |
| `generate` | the CI workflow, which fails when the generated files differ from those committed | `generate: config` |
| `test` | `make`, the CI workflow | `test: test-grammar` |
| `test-integration` | the CI workflow, after `make test` | `test-integration: test-valkey`, tests that need a server in a container |
| `tools` | `make`, the workflows | `tools: $(CONSTANTS)`, a cli the repository builds |

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

The `bump-version` workflow no longer has a default version, which changed with every bump. An empty version releases the current pre-release, e.g. 0.2.0 for 0.2.0-dev.

## Changing the build

Change the files here, then try the change in a repository before releasing it:

```sh
BUILD_SOURCE=../build make update-build
```

`test/sync-test.sh` checks the sync and the drift check, run with shellcheck by the `Self test` workflow.

### Releasing

Tag a commit `vX.Y.Z` and publish a GitHub release for it, the `Update build` workflow of each repository looks for the latest release. Say in the release notes when a repository has to change, e.g. a new setting or a tool it must pin.

The synced workflows are disabled in this repository's Actions settings, they're here to be synced and for dependabot to bump their actions.
