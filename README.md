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
  tools.mk wit.mk components.mk publish.mk sync.mk pinned.jq local.jq
  tools/Cargo.toml         the tools the build provides, see Tools
  scripts/                 publish.sh, bump-version.sh, update-build.sh, ...
.github/workflows/         ci.yaml, bump-version.yaml, release.yaml, update-build.yaml: synced
tools/Cargo.toml           repository-owned and optional, the repository's own tools and overrides, bumped by dependabot
```

[`SYNC`](SYNC) lists the paths synced into each repository.

### Updating

`make update-build` updates to the latest release, `make update-build BUILD_VERSION=v0.2.0` to a given one. It removes the files synced from the previous version, copies in the release's files, and writes `build/VERSION` and `build/MANIFEST`.

The `Update build` workflow does the same each week, and opens a pull request when there is a newer release, like dependabot. The pull request is created with the custodian GitHub App, the `GITHUB_TOKEN` can't change workflow files. The app needs the `contents`, `workflows` and `pull-requests` write permissions, and the `CUSTODIAN_CLIENT_ID` and `CUSTODIAN_PRIVATE_KEY` secrets must be available to the repository.

The versions of the tools the build provides come with it, see [Tools](#tools). The update removes a tool the repository overrides in its `tools/Cargo.toml` once the build pins the same version or a newer one.

### Drift

The CI workflow runs `build/scripts/check-build.sh` first, which fails when a synced file was changed or removed, or a file was added to `build/`. Fix the build here rather than in the repository, so the repositories don't drift apart again.

## Settings

The repository's `Makefile` sets these before `include build/build.mk`. Each has a default that fits most repositories, detected where it can be.

| Setting | Default | |
| --- | --- | --- |
| `COMPONENT_TARGET` | `wasm32-unknown-unknown` | the rust target of the components cargo builds. `wasm32-wasip3` installs wasi-sdk with the tools, or uses `WASI_SDK_PATH`. `wasm32-wasip2` copies the component rust links, without `COMPONENT_OPTIMIZE`. A component names its own in its Cargo.toml, `[package.metadata.componentized]` `target = "wasm32-wasip2"`, a binary crate builds a command |
| `COMPONENT_PROFILE` | `component` when Cargo.toml has a `[profile.component]`, otherwise `release` | the cargo profile of a component's release build |
| `COMPONENT_DEBUG_PROFILE` | `component-debug` when Cargo.toml has a `[profile.component-debug]`, otherwise `dev` | the cargo profile of a component's debug build |
| `COMPONENT_OPTIMIZE` | | optimize the release builds again for size with wasm-opt when set, e.g. `COMPONENT_OPTIMIZE := true` |
| `CHECK_COMPONENTS_SKIP` | | the components whose release and debug builds may have different WIT, skipped by `make check-components`, e.g. a `wasm32-wasip3` component whose release build wasm-opt optimizes, dropping imports of the standard library its debug build keeps |
| `CARGO_COMPONENT_DEPS` | | files the components cargo builds need besides their sources, e.g. a component the library embeds |
| `LOCAL_TOOLS` | | tools built from the workspace rather than pinned, each a package, e.g. `componentized-constants-cli` in componentized/constants. Installed with the pinned tools, with a variable for each binary like a pinned tool, e.g. `$(CONSTANTS)`, and installed again when the sources of the package, or of the workspace packages it depends on, change. The components built with a local tool are built again when it changes. A local tool replaces a pinned tool of the same name, one the build provides too, and a file it embeds is added as a prerequisite, e.g. `$(call tool,componentized-static-config-cli): $(CONFIG_ADAPTER_WASM)` |
| `CLI_BIN` | | the cli built from the repository, e.g. `constants`. The CI workflow builds it for each platform, smoke tests it with `scripts/smoke-test-cli.sh <cli> <dir>` when the repository has one, and attaches the archives to the release |
| `PUBLISH_CRATES` | | the crates the release workflow publishes to crates.io, e.g. `componentized-constants componentized-constants-cli`, checked with `cargo publish --dry-run` by the CI workflow |
| `PUBLISH_INTERFACE` | `true` | publish the interface, `interface.wasm`, `false` for a repository whose interface isn't one to depend on, e.g. componentized/build, whose interface is there to test the build. The components published are those that aren't `dep-`, `internal-` or `test-` |

Publishing reads `DRY_RUN`: `make publish DRY_RUN=true` shows each image it would push, with its annotations, without pushing or signing anything. In the workflows the repository variable `DRY_RUN=true` does the same, also skipping the login to the registry and the GitHub release the CI workflow drafts for a tag, and the release workflow checks the crates with `cargo publish --dry-run` rather than publishing them.

`make components` also generates an SBOM of the interface and of each component, CycloneDX JSON beside the file it describes, e.g. `target/components/gate/gate.cdx.json` for `gate.wasm`, see [`build/scripts/sbom.sh`](build/scripts/sbom.sh). It lists what the file is made from: a component built with cargo, the crates it depends on built for its target, from `cargo-cyclonedx`, those only proc-macros use as it's compiled, e.g. `syn`, scoped `excluded` as `cargo tree -e normal,no-proc-macro` doesn't list them, the Rust standard library by the commit of rustc, the wasi-libc it links, for `wasm32-wasip3` the one of the wasi-sdk sysroot or built from `WASI_LIBC_VERSION`, and each WASI adapter it embeds, e.g. `wasi_snapshot_preview1`, by its SHA-256; a composition, `.wac` or `.plug`, the components it's composed from with what each is made from; a component generated with the `constants` or `static-config` cli, the cli's crate; a component pulled with wkg, the image it's pulled from, by its OCI coordinates, on top of the SBOM attested to the image when cosign verifies one signed by a GitHub Actions workflow of the image's org, so a component composed from another repository's lists what that component is made from; the interface, the WIT packages in `wkg.lock`. The SBOMs of a commit are the same each time they're generated, dated by the commit. Each SBOM names the component it describes by the directory's id of the package it's published as, e.g. `componentized:build-greeter`, and publishing adds its version and image, and attests it to the image with cosign, `cosign verify-attestation --type cyclonedx <image>` verifies it, and writes the SBOMs published to `target/sbom/published`, laid out like `target/components`. The CI workflow packs the components into the `components.tar` of the release it drafts with an SBOM for each, see [`build/scripts/pack-components.sh`](build/scripts/pack-components.sh): the one it's published with; for a debug build not published as it's identical to the release build, the release build's; and for a file that isn't published, e.g. a `dep-` component, the build's, which for a component pulled with wkg is based on the SBOM it was published with.

The variable `COMPONENTIZED_DIRECTORY_SCAN=true` asks [componentized/directory](https://github.com/componentized/directory) to scan the components the CI workflow publishes, there is no scan for a dry run. It is set for every repository in the componentized organization, so a repository outside it, without the custodian's access to the directory, doesn't request a scan that would fail. The scan is requested with the custodian GitHub App, which needs the `contents` write permission on componentized/directory. With it set, the release notes the CI workflow drafts lead each published interface and component with the directory's icon for its kind, a package, component or composition, linking to the page of its version in the directory, e.g. [`componentized:logging-levels@0.3.1`](https://componentized.dev/componentized/logging-levels/0.3.1/) for `ghcr.io/componentized/logging/levels:0.3.1`, which exists once the scan has indexed it. The icons are the directory's own, from [componentized.dev/_icons](https://componentized.dev/_icons/), in the colors of the reader's GitHub theme.

## Tools

The build provides the tools its rules run, pinned in [`build/tools/Cargo.toml`](build/tools/Cargo.toml) and bumped by dependabot here: `cargo-cyclonedx`, `wac`, `wkg`, `wasm-opt`, `wasm-tools`, the `constants` and `static-config` clis, and `componentized-build`, see [Updating a dependency](#updating-a-dependency), run as `$(COMPONENTIZED_BUILD)`. It also provides `cosign`, a release binary pinned in [`build/tools.mk`](build/tools.mk) and checked against the release's checksums, which signs the published components, attests their SBOMs and verifies the SBOMs of the components pulled with wkg. `make tools` installs them in `target/tools`, with cargo binstall when it's installed, and a recipe runs each by path from a variable named after its binary, e.g. `$(WASM_TOOLS)`.

A repository changes a tool the build provides, or adds its own, without changing the build:

- **A newer version**: pin it in the repository's `tools/Cargo.toml`, which overrides the build's, with the binaries it installs in `[package.metadata.bins]`. Dependabot bumps it there, and `make update-build` removes it once the build catches up, pinning the same version or a newer one.
- **Built from the workspace**: name its package in `LOCAL_TOOLS`, e.g. `LOCAL_TOOLS := componentized-constants-cli` in componentized/constants, built from the repository's sources in place of the version the build provides.
- **A binary built elsewhere**, to try a change: set its variable on the command line, e.g. `make WKG=../wkg/target/release/wkg`. The scripts the recipes run find the installed tools on the `PATH`.
- **Its own tools**, e.g. `wasmtime-cli` for its tests: pin them in `tools/Cargo.toml` like an override. A repository with no tools of its own needs no `tools/Cargo.toml`, nor its dependabot entry.

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

## Updating a dependency

A repository depends on others for their interfaces, WIT packages it names at a version, e.g. `componentized:secrets/store@0.1.1-dev`, pinned in `wkg.lock` by digest, and for their components, pulled with wkg from the images a `components/dep-*/*.wkg` file pins by digest. The `componentized-build` cli, the `componentized-build-cli` crate, [`crates/cli`](crates/cli/), updates them all at once, looking up each version and digest in the registry before anything changes:

```sh
# or `make tools`, the build provides it, run with `$(make -s tools-path)/componentized-build`
cargo install componentized-build-cli

componentized-build deps                                  # what the repository depends on, by version
componentized-build dep componentized:secrets 0.1.1       # the interface and components of componentized/secrets
componentized-build dep componentized:secrets             # the latest build of the version it's at, e.g. 0.1.1-dev
componentized-build dep componentized:secrets-gate 0.1.1  # one component alone
componentized-build dep componentized:secrets 0.1.1 -n    # what would change, without changing it
```

A dev version is published again with each build of its repository, at the same tag, so a repository testing the latest dev build updates to it again, pinning its new digests. `make` then fetches the WIT and pulls the components. `componentized-build completions` registers completions with the shell, which complete the dependencies of the repository and the versions published.

## This repository builds with the build

This repository is built by its own build, with the synced workflows, like any other repository. Its components and cli are just enough to smoke test each kind of thing the build makes. The components are tests, `test-`, and its interface is too, `PUBLISH_INTERFACE := false`, so nothing of it is published but its cli:

| | |
| --- | --- |
| [`wit/`](wit/worlds.wit) | `componentized:build`, the interface, built into `interface.wasm`, not published: a `greeter`, and its `defaults` |
| [`test-greeter`](components/test-greeter/) | a component built with cargo, implementing `greeter`, importing `defaults` and `wasi:logging` |
| [`test-greeter-wasip2`](components/test-greeter-wasip2/) | a component built with cargo for `wasm32-wasip2`, implementing `greeter` with the default name from the environment, linking the wasi-libc the Rust toolchain bundles, run by `make test-integration` with wasmtime |
| [`test-greeter-wasip3`](components/test-greeter-wasip3/) | a component built with cargo for `wasm32-wasip3`, like `test-greeter-wasip2` greeting on a cooperative thread, linking the wasi-libc of the wasi-sdk sysroot, run by `make test-integration` with wasmtime |
| [`test-defaults`](components/test-defaults/) | a component built with the constants cli, implementing `defaults` from the `@value` of each function |
| [`dep-componentized-logging-to-stderr`](components/dep-componentized-logging-to-stderr/) | a component pulled from a registry, wasi:logging written to stderr |
| [`test-hello`](components/test-hello/) | a composition of the three, run by `make test-integration` with wasmtime |
| [`test-hello-plug`](components/test-hello-plug/) | the three plugged together with `wac plug`, run by `make test-integration` with wasmtime |
| [`crates/cli`](crates/cli/) | `componentized-build`, the cli that updates a repository's dependencies, see [Updating a dependency](#updating-a-dependency), built for each platform, smoke tested with [`scripts/smoke-test-cli.sh`](scripts/smoke-test-cli.sh), checked with `cargo publish --dry-run`, and installed as a local tool, `$(COMPONENTIZED_BUILD)`, run by `make test-integration` |

`make test` also runs [`test/sync-test.sh`](test/sync-test.sh), which syncs the build into a new repository and checks the drift check. The `Lint` workflow runs shellcheck. As the source of the build rather than a copy of it, this repository has no `build/VERSION` or `build/MANIFEST`: `make check-build` passes, and the `Update build` workflow skips it.

Pushes to main and tags go through publishing with nothing to publish, and the release of a tag is drafted with `components.tar` and the cli archives. The release workflow publishes the `componentized-build-cli` crate, the cli every repository gets as a tool, at the version `build/tools/Cargo.toml` pins. crates.io's trusted publishing only applies to a crate that exists, so its first version was published by hand.

## Changing the build

Change the files here, and check them with `make`. Then try the change in a repository before releasing it:

```sh
BUILD_SOURCE=../build make update-build
```

### Releasing

Release with the `Bump version` workflow like any other repository, the version of the build is the version of its `componentized:build` interface. It tags the release, e.g. `v0.1.0`, and moves on to the next pre-release.

The CI workflow drafts the GitHub release of the tag, publish it once it's reviewed: publishing it publishes the cli crate, and the `Update build` workflow of each repository looks for the latest release. Say in the release notes when a repository has to change, e.g. a new setting or a tool it must pin.
