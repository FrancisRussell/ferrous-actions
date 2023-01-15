use super::Hook;
use crate::action_paths::get_action_cache_dir;
use crate::actions::cache::Entry as CacheEntry;
use crate::cargo::ToolchainVersion;
use crate::delta::render_list as render_delta_list;
use crate::fingerprinting::Fingerprint;
use crate::hasher::Blake3 as Blake3Hasher;
use crate::node::path::Path;
use crate::{actions, error, info, node, warning, Error};
use async_trait::async_trait;
use rustup_toolchain_manifest::HashValue;
use std::borrow::Cow;

const MAX_ARG_STRING_LENGTH: usize = 80;

fn get_package_build_dir(hash: &HashValue) -> Result<Path, Error> {
    // Don't use safe_encoding here because the platform filesystem
    // might not be case sensitive
    let dir = get_action_cache_dir()?
        .join("package-build-artifacts")
        .join(&hash.to_string());
    Ok(dir)
}

pub struct Install {
    hash: HashValue,
    build_dir: String,
    fingerprint: Option<Fingerprint>,
    arg_string: String,
    restore_key: Option<String>,
    toolchain_version_short: String,
}

impl Install {
    pub async fn new<I, A>(toolchain_version: &ToolchainVersion, args: I) -> Result<Install, Error>
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        use std::hash::Hash as _;

        let mut hasher = Blake3Hasher::default();
        toolchain_version.long().hash(&mut hasher);
        let arg_string = {
            let mut arg_string = String::new();
            let mut first = true;
            for arg in args {
                let arg = arg.as_ref();
                if first {
                    first = false;
                } else {
                    arg_string += " ";
                }
                arg_string += &shlex::quote(arg);
            }
            arg_string
        };
        arg_string.hash(&mut hasher);
        let hash = hasher.hash_value();
        let build_dir = get_package_build_dir(&hash)?;
        node::fs::create_dir_all(&build_dir).await?;
        let mut result = Install {
            hash,
            build_dir: build_dir.to_string(),
            fingerprint: None,
            arg_string,
            restore_key: None,
            toolchain_version_short: toolchain_version.short().to_string(),
        };
        let cache_entry = result.build_cache_entry();
        if let Some(key) = cache_entry.restore().await? {
            info!("Restored files from cache with key {}", key);
            result.fingerprint = Some(Self::fingerprint_build_dir(&build_dir).await?);
            result.restore_key = Some(key);
        }
        Ok(result)
    }

    async fn fingerprint_build_dir(path: &Path) -> Result<Fingerprint, Error> {
        use crate::fingerprinting::{fingerprint_path_with_ignores, Ignores};

        // It seems that between runs something causes the rustc fingerprint to change.
        // It looks like this could simply be the file modification timestamp. This
        // would also explain why it seemed to occur with Rustup but not the
        // internal toolchain downloader.
        //
        // https://github.com/rust-lang/cargo/blob/70898e522116f6c23971e2a554b2dc85fd4c84cd/src/cargo/util/rustc.rs#L306

        let mut ignores = Ignores::default();
        ignores.add(1, ".rustc_info.json");

        let fingerprint = fingerprint_path_with_ignores(path, &ignores).await?;
        Ok(fingerprint)
    }

    fn build_cache_entry(&self) -> CacheEntry {
        use crate::cache_key_builder::{Attribute, CacheKeyBuilder};

        let mut key_builder = CacheKeyBuilder::new("cargo install build artifacts");
        key_builder.add_key_data(&self.hash);
        key_builder.set_attribute(Attribute::ToolchainVersion, self.toolchain_version_short.clone());
        let arg_string = {
            let mut arg_string = self.arg_string.clone();
            if arg_string.len() > MAX_ARG_STRING_LENGTH {
                let ellipsis = "...";
                arg_string.truncate(MAX_ARG_STRING_LENGTH - ellipsis.len());
                arg_string += ellipsis;
            }
            arg_string
        };
        key_builder.set_attribute(Attribute::ArgsTruncated, arg_string);
        let mut cache_entry = key_builder.into_entry();
        cache_entry.path(&Path::from(&self.build_dir));
        cache_entry
    }

    async fn cleanup(&self) {
        if let Err(e) = actions::io::rm_rf(self.build_dir.as_str()).await.map_err(Error::Js) {
            warning!("Failed to clean up build folder at {}: {}", self.build_dir, e);
        }
    }
}

#[async_trait(?Send)]
impl Hook for Install {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        vec!["--target-dir".into(), self.build_dir.as_str().into()]
    }

    async fn succeeded(&mut self) {
        let save = if let Some(old_fingerprint) = &self.fingerprint {
            let path = Path::from(&self.build_dir);
            match Self::fingerprint_build_dir(&path).await {
                Ok(new_fingerprint) => {
                    let changed = new_fingerprint.content_hash() != old_fingerprint.content_hash();
                    if changed {
                        info!(
                            "Package artifact cache changed fingerprint from {} to {}",
                            old_fingerprint.content_hash(),
                            new_fingerprint.content_hash()
                        );
                        let delta = new_fingerprint.changes_from(old_fingerprint);
                        info!("{}", render_delta_list(&delta));
                    }
                    changed
                }
                Err(e) => {
                    error!("Could not fingerprint build artifact directory: {}", e);
                    false
                }
            }
        } else {
            true
        };
        if save {
            let cache_entry = self.build_cache_entry();
            match cache_entry
                .save_if_update(self.restore_key.as_deref())
                .await
                .map_err(Error::Js)
            {
                Err(e) => {
                    error!("Failed to save package build artifacts to cache: {}", e);
                }
                Ok(r) => {
                    if r.is_some() {
                        info!("Saved package build artifacts to cache.");
                    } else {
                        info!("Looks like a concurrent CI job updated the artifacts, not saving back to cache");
                    }
                }
            }
        } else {
            info!("Build artifacts unchanged, no need to save back to cache.");
        }
        self.cleanup().await;
    }

    async fn failed(&mut self) {
        self.cleanup().await;
    }
}
