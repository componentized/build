//! What's published: the versions of WIT packages and the digests of their releases, looked up as
//! wkg looks them up, with wasm-pkg-client, and the digests and tags of the images of components,
//! looked up as `wkg oci pull` pulls them, with oci-client.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use docker_credential::DockerCredential;
use oci_client::errors::{OciDistributionError, OciErrorCode};
use oci_client::secrets::RegistryAuth;
use oci_client::{Reference, client::ClientConfig, client::ClientProtocol};
use semver::Version;
use wasm_pkg_client::{Config, PackageRef};

use crate::id::Id;
use crate::repo::Image;

/// The wkg config of a repository synced with the build, see WKG_CONFIG_FILE in build/build.mk.
pub const WKG_CONFIG: &str = ".config/wasm-pkg/config.toml";

/// Looks up what's published, in [`Registries`], or in what a test fakes.
pub trait Registry {
    /// The registry a WIT package is published to, e.g. `componentized.dev`.
    fn package_registry(&self, id: &Id) -> Option<String>;
    /// The versions of a WIT package published, none when the package isn't published.
    fn package_versions(&self, id: &Id) -> Result<Vec<Version>>;
    /// The digest of the content of a WIT package published at a version, what wkg.lock pins.
    fn package_digest(&self, id: &Id, version: &Version) -> Result<String>;
    /// The digest of the manifest an image is tagged with, what a .wkg file pins, None when the
    /// image has no such tag.
    fn manifest_digest(&self, image: &Image, tag: &str) -> Result<Option<String>>;
    /// The tags of an image, none when the image doesn't exist.
    fn tags(&self, image: &Image) -> Result<Vec<String>>;
}

/// The registries, as wkg reaches them.
pub struct Registries {
    runtime: tokio::runtime::Runtime,
    timeout: Duration,
    config: Config,
    packages: wasm_pkg_client::Client,
    images: oci_client::Client,
}

impl Registries {
    /// The registries of the repository, its WIT packages found with the wkg config wkg reads in
    /// it: the file WKG_CONFIG_FILE names, which the build sets to the repository's
    /// `.config/wasm-pkg/config.toml`, the global config of wkg without one. Each lookup is answered
    /// within the timeout.
    pub fn new(root: &Path, timeout: Duration) -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .context("couldn't start the async runtime")?;
        let path = std::env::var_os("WKG_CONFIG_FILE").map(PathBuf::from).or_else(|| {
            let path = root.join(WKG_CONFIG);
            path.is_file().then_some(path)
        });
        let config = runtime.block_on(async {
            match &path {
                Some(path) => Config::from_file(path)
                    .await
                    .with_context(|| format!("couldn't read the wkg config {}", path.display())),
                None => Config::global_defaults().await.context("couldn't read the global wkg config"),
            }
        })?;
        // as `wkg oci` has it, the registries WKG_OCI_INSECURE names are reached over http
        let insecure: Vec<String> = std::env::var("WKG_OCI_INSECURE")
            .unwrap_or_default()
            .split(',')
            .filter(|registry| !registry.is_empty())
            .map(str::to_string)
            .collect();
        // the client is made in the runtime it's run in
        let _runtime = runtime.enter();
        let images = oci_client::Client::new(ClientConfig {
            protocol: match insecure.is_empty() {
                true => ClientProtocol::Https,
                false => ClientProtocol::HttpsExcept(insecure),
            },
            ..Default::default()
        });
        Ok(Self {
            runtime,
            timeout,
            packages: wasm_pkg_client::Client::new(config.clone()),
            config,
            images,
        })
    }

    fn block_on<T>(&self, what: impl FnOnce() -> String, future: impl Future<Output = T>) -> Result<T> {
        // the timer is made in the runtime
        self.runtime
            .block_on(async { tokio::time::timeout(self.timeout, future).await })
            .map_err(|_| anyhow::anyhow!("{} timed out after {}s", what(), self.timeout.as_secs()))
    }

    /// The credentials for a registry, as `wkg oci pull` has them: WKG_OCI_USERNAME and
    /// WKG_OCI_PASSWORD, otherwise those of docker's config, otherwise none.
    fn auth(&self, registry: &str) -> Result<RegistryAuth> {
        if let (Ok(username), Ok(password)) = (std::env::var("WKG_OCI_USERNAME"), std::env::var("WKG_OCI_PASSWORD")) {
            return Ok(RegistryAuth::Basic(username, password));
        }
        let key = match registry {
            "docker.io" | "index.docker.io" => "https://index.docker.io/v1/",
            registry => registry,
        };
        match docker_credential::get_credential(key) {
            Ok(DockerCredential::UsernamePassword(username, password)) => Ok(RegistryAuth::Basic(username, password)),
            Ok(DockerCredential::IdentityToken(_)) => {
                bail!("the docker credentials for {registry} are an identity token, which isn't supported")
            }
            Err(_) => Ok(RegistryAuth::Anonymous),
        }
    }
}

fn package_ref(id: &Id) -> Result<PackageRef> {
    id.to_string()
        .parse()
        .map_err(|err| anyhow::anyhow!("{id} isn't a package wkg can look up, {err}"))
}

/// Whether an error is the registry saying an image or its tag doesn't exist.
fn not_found(err: &OciDistributionError) -> bool {
    match err {
        OciDistributionError::ImageManifestNotFoundError(_) => true,
        OciDistributionError::ServerError { code, .. } => *code == 404,
        OciDistributionError::RegistryError { envelope, .. } => envelope
            .errors
            .iter()
            .all(|error| matches!(error.code, OciErrorCode::ManifestUnknown | OciErrorCode::NameUnknown)),
        _ => false,
    }
}

impl Registry for Registries {
    fn package_registry(&self, id: &Id) -> Option<String> {
        self.config
            .resolve_registry(&package_ref(id).ok()?)
            .map(|registry| registry.to_string())
    }

    fn package_versions(&self, id: &Id) -> Result<Vec<Version>> {
        let package = package_ref(id)?;
        match self.block_on(
            || format!("looking up the versions of {id}"),
            self.packages.list_all_versions(&package),
        )? {
            Ok(versions) => Ok(versions.into_iter().map(|info| info.version).collect()),
            Err(wasm_pkg_client::Error::PackageNotFound) => Ok(Vec::new()),
            Err(wasm_pkg_client::Error::NoRegistryForNamespace(_)) => bail!(
                "there's no registry for the {} namespace of {id}\n\
                 \n  name one in {WKG_CONFIG}, e.g.\n\
                 \n  [namespace_registries]\n  {} = \"componentized.dev\"",
                id.namespace,
                id.namespace
            ),
            Err(err) => Err(err).with_context(|| format!("couldn't look up the versions of {id}")),
        }
    }

    fn package_digest(&self, id: &Id, version: &Version) -> Result<String> {
        let package = package_ref(id)?;
        let release = self
            .block_on(
                || format!("looking up {id} {version}"),
                self.packages.get_release(&package, version),
            )?
            .with_context(|| format!("couldn't look up {id} {version}"))?;
        Ok(release.content_digest.to_string())
    }

    fn manifest_digest(&self, image: &Image, tag: &str) -> Result<Option<String>> {
        let reference = Reference::with_tag(image.registry.clone(), image.repository.clone(), tag.to_string());
        let auth = self.auth(&image.registry)?;
        match self.block_on(
            || format!("looking up {image}:{tag}"),
            self.images.pull_manifest(&reference, &auth),
        )? {
            Ok((_, digest)) => Ok(Some(digest)),
            Err(err) if not_found(&err) => Ok(None),
            Err(err) => Err(err).with_context(|| format!("couldn't look up {image}:{tag}")),
        }
    }

    fn tags(&self, image: &Image) -> Result<Vec<String>> {
        let reference = Reference::with_tag(image.registry.clone(), image.repository.clone(), "latest".into());
        let auth = self.auth(&image.registry)?;
        match self.block_on(
            || format!("looking up the tags of {image}"),
            self.images.list_tags(&reference, &auth, None, None),
        )? {
            Ok(tags) => Ok(tags.tags),
            Err(err) if not_found(&err) => Ok(Vec::new()),
            Err(err) => Err(err).with_context(|| format!("couldn't look up the tags of {image}")),
        }
    }
}
