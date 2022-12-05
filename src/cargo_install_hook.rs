use crate::actions;
use crate::actions::cache::CacheEntry;
use crate::actions::exec::Command;
use crate::cargo_hook::CargoHook;
use crate::error;
use crate::info;
use crate::node;
use crate::node::path::Path;
use crate::warning;
use crate::Error;
use async_trait::async_trait;
use rust_toolchain_manifest::HashValue;
use std::borrow::Cow;

fn get_package_build_dir(hash: &HashValue) -> Result<Path, Error> {
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push("package-build-artifacts");
    dir.push(base64::encode_config(hash, base64::URL_SAFE).as_str());
    Ok(dir)
}

pub struct CargoInstallHook {
    hash: HashValue,
    nonce: HashValue,
    build_dir: String,
    fingerprint: Option<HashValue>,
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
        for arg in args.into_iter() {
            let arg = arg.as_ref();
            hasher.update(arg.as_bytes());
        }
        let hash = hasher.finalize();
        let hash = HashValue::from_bytes(hash.as_bytes());
        let build_dir = get_package_build_dir(&hash)?.to_string();
        let result = CargoInstallHook {
            hash,
            nonce: build_nonce(),
            build_dir,
            fingerprint: None,
        };
        node::fs::create_dir_all(result.build_dir.as_str()).await?;
        let cache_entry = result.build_cache_entry();
        if let Some(key) = cache_entry.restore().await? {
            info!("Restored files from cache with key {}", key);
            //TODO: compute fingerprint of build folder
        }
        Ok(result)
    }

    fn build_key(&self, with_nonce: bool) -> String {
        let mut result = format!(
            "Cargo package build artifacts - {}",
            base64::encode_config(&self.hash, base64::URL_SAFE)
        );
        if with_nonce {
            result += &format!(
                " - {}",
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
        if let Err(e) = actions::io::rm_rf(self.build_dir.as_str())
            .await
            .map_err(Error::Js)
        {
            warning!(
                "Failed to clean up build folder at {}: {}",
                self.build_dir,
                e
            );
        }
    }
}

#[async_trait(?Send)]
impl CargoHook for CargoInstallHook {
    fn additional_cargo_options(&self) -> Vec<Cow<str>> {
        vec!["--target-dir".into(), self.build_dir.as_str().into()]
    }

    fn modify_command(&self, _command: &mut Command) {}

    async fn succeeded(&mut self) {
        //TODO: compute fingerprint of build folder and only save if it has changed
        //or there was no previous fingerprint. New items should be saved with a nonce
        //since we cannot overwrite cache items.
        let cache_entry = self.build_cache_entry();
        if let Err(e) = cache_entry.save().await.map_err(Error::Js) {
            error!("Failed to save package build artifacts to cache: {}", e);
        } else {
            info!("Saved package build artifacts to cache");
        }
        self.cleanup().await;
    }

    async fn failed(&mut self) {
        self.cleanup().await;
    }
}
