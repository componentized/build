# componentized-build-cli

Updates a componentized repository's dependencies on the interfaces and components of other repositories, the cli of [componentized/build](https://github.com/componentized/build).

A repository synced with the componentized build depends on other repositories for:

- **their interfaces**, WIT packages it names at a version, e.g. `componentized:secrets/store@0.1.1-dev`, in WIT, WAC, Rust or Markdown, each pinned in a `wkg.lock` by the digest of its wasm
- **their components**, pulled with wkg from the images a `components/dep-*/*.wkg` file pins, the release build's and the debug build's, e.g. `ghcr.io/componentized/secrets/gate:0.1.1-dev@sha256:...`

Updating one by hand means finding each of them, and a component missed leaves the repository testing against a build it didn't mean to. `componentized-build dep` updates them all at once, and looks up each version and digest in the registry before anything changes.

## Install

```sh
cargo install componentized-build-cli
```

The cli is `componentized-build`. Or `cargo binstall componentized-build-cli`, for a binary of the [release](https://github.com/componentized/build/releases).

## Use

Run it in a repository synced with the build, or a directory of one, or name one with `--directory`:

```sh
# what the repository depends on, by version
componentized-build deps

# the interface and components of componentized/secrets at 0.1.1
componentized-build dep componentized:secrets 0.1.1

# the latest build of the version it's at, e.g. 0.1.1-dev, published again with each build
componentized-build dep componentized:secrets

# one component alone
componentized-build dep componentized:secrets-gate 0.1.1

# what would change, without changing anything
componentized-build dep componentized:secrets 0.1.1 --dry-run
```

```
Updating componentized:secrets 0.1.1-dev → 0.1.1
  interface          ghcr.io/componentized/secrets:0.1.1 sha256:4fecb198d95e
  gate               ghcr.io/componentized/secrets/gate:0.1.1 sha256:65bf990900b5
  gate (debug)       ghcr.io/componentized/secrets/gate:0.1.1_debug sha256:e36a56703135
Updated 4 files
  components/dep-componentized-secrets-gate/dep-componentized-secrets-gate.wkg
    gate 0.1.1-dev sha256:65bf990900b5 → 0.1.1 sha256:...
  ...
```

Then `make` fetches the WIT and pulls the components it now pins. Files git ignores are left alone, as are the build synced into the repository, `build/`, and the WIT fetched into `wit/deps`. A version that isn't published is an error naming those that are, a dependency the repository doesn't have one naming those it has.

The interface is looked up as wkg looks it up, with `wasm-pkg-client`: with the wkg config `WKG_CONFIG_FILE` names, which the build sets to the repository's `.config/wasm-pkg/config.toml`, read there when it's not set, otherwise wkg's global config. The components are looked up as `wkg oci pull` pulls them, with `oci-client`: with the credentials of `WKG_OCI_USERNAME` and `WKG_OCI_PASSWORD`, otherwise docker's, otherwise anonymously, over http for the registries `WKG_OCI_INSECURE` names.

## Completions

`componentized-build completions <shell>` prints the script that registers the completions with bash, zsh, fish, elvish or powershell, which complete the commands, the dependencies of the repository and the versions published:

```sh
echo 'source <(componentized-build completions zsh)' >> ~/.zshrc
```

See `componentized-build completions --help` for each shell.

## License

Apache-2.0
