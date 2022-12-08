use crate::action_paths::get_action_cache_dir;
use crate::actions::cache::CacheEntry;
use crate::cargo_hook::CargoHook;
use crate::fingerprinting::{render_delta_items, Fingerprint};
use crate::node::path::Path;
use crate::{actions, error, info, node, warning, Error};
use async_trait::async_trait;
use rust_toolchain_manifest::HashValue;
use std::borrow::Cow;

const NONCE_SIZE_BYTES: usize = 8;

const MAX_ARG_STRING_LENGTH: usize = 80;

fn get_package_build_dir(hash: &HashValue) -> Result<Path, Error> {
    let mut dir = get_action_cache_dir()?;
    dir.push("package-build-artifacts");
    dir.push(base64::encode_config(hash, base64::URL_SAFE).as_str());
    Ok(dir)
}

pub struct CargoInstallHook {
    hash: HashValue,
    nonce: HashValue,
    build_dir: String,
    fingerprint: Option<Fingerprint>,
    arg_string: String,
}

impl CargoInstallHook {
    pub async fn new<I, A>(toolchain_hash: &HashValue, args: I) -> Result<CargoInstallHook, Error>
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        use crate::nonce::build_nonce;
        let mut hasher = blake3::Hasher::new();
        hasher.update(toolchain_hash.as_ref());
        let arg_string = {
            let mut arg_string = String::new();
            let mut first = true;
            for arg in args {
                let arg = arg.as_ref();
                hasher.update(arg.as_bytes());
                if first {
                    first = false;
                } else {
                    arg_string += " ";
                }
                arg_string += &shlex::quote(arg);
            }
            arg_string
        };
        let hash = hasher.finalize();
        let hash = HashValue::from_bytes(hash.as_bytes());
        let build_dir = get_package_build_dir(&hash)?;
        let mut result = CargoInstallHook {
            hash,
            nonce: build_nonce(NONCE_SIZE_BYTES),
            build_dir: build_dir.to_string(),
            fingerprint: None,
            arg_string,
        };
        node::fs::create_dir_all(result.build_dir.as_str()).await?;
        let cache_entry = result.build_cache_entry();
        if let Some(key) = cache_entry.restore().await? {
            info!("Restored files from cache with key {}", key);
            result.fingerprint = Some(Self::fingerprint_build_dir(&build_dir).await?);
        }
        Ok(result)
    }

    async fn fingerprint_build_dir(path: &Path) -> Result<Fingerprint, Error> {
        use crate::fingerprinting::{fingerprint_directory_with_ignores, Ignores};

        // It seems that between runs something causes the rustc fingerprint to change.
        // It looks like this could simply be the file modification timestamp. This
        // would also explain why it seemed to occur with Rustup but not the
        // internal toolchain downloader.
        //
        // https://github.com/rust-lang/cargo/blob/70898e522116f6c23971e2a554b2dc85fd4c84cd/src/cargo/util/rustc.rs#L306

        let mut ignores = Ignores::default();
        ignores.add(0, ".rustc_info.json");

        let fingerprint = fingerprint_directory_with_ignores(path, &ignores).await?;
        Ok(fingerprint)
    }

    fn build_key(&self, with_nonce: bool) -> String {
        let mut result = format!(
            "Cargo package build artifacts - {}",
            base64::encode_config(&self.hash, base64::URL_SAFE),
        );
        if with_nonce {
            let arg_string = {
                let mut arg_string = self.arg_string.clone();
                if arg_string.len() > MAX_ARG_STRING_LENGTH {
                    let ellipsis = "...";
                    arg_string.truncate(MAX_ARG_STRING_LENGTH - ellipsis.len());
                    arg_string += ellipsis;
                }
                arg_string.replace(',', ";")
            };
            result += &format!(
                " ({}; {})",
                arg_string,
                base64::encode_config(&self.nonce, base64::URL_SAFE)
            );
        }
        result
    }

    fn build_cache_entry(&self) -> CacheEntry {
        let primary_key = self.build_key(true);
        let mut cache_entry = CacheEntry::new(primary_key.as_str());
        let secondary_key = self.build_key(false);
        cache_entry.restore_key(secondary_key.as_str());
        cache_entry.path(&Path::from(self.build_dir.as_str()));
        cache_entry
    }

    async fn cleanup(&self) {
        if let Err(e) = actions::io::rm_rf(self.build_dir.as_str()).await.map_err(Error::Js) {
            warning!("Failed to clean up build folder at {}: {}", self.build_dir, e);
        }
    }
}

#[async_trait(?Send)]
impl CargoHook for CargoInstallHook {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        vec!["--target-dir".into(), self.build_dir.as_str().into()]
    }

    async fn succeeded(&mut self) {
        let save = if let Some(old_fingerprint) = &self.fingerprint {
            let path = Path::from(self.build_dir.as_str());
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
                        info!("{}", render_delta_items(&delta));
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
            if let Err(e) = cache_entry.save().await.map_err(Error::Js) {
                error!("Failed to save package build artifacts to cache: {}", e);
            } else {
                info!("Saved package build artifacts to cache.");
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
