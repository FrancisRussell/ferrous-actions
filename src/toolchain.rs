use crate::info;
use crate::node::path::Path;
use crate::rustup::ToolchainConfig;
use crate::Error;
use rust_toolchain_manifest::manifest::ManifestPackage;
use rust_toolchain_manifest::HashValue;
use target_lexicon::Triple;

async fn get_cacheable_path(key: &HashValue) -> Result<Path, Error> {
    use crate::node;
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push(base64::encode_config(key, base64::URL_SAFE).as_str());
    Ok(dir)
}

fn compute_cache_key(package: &ManifestPackage) -> String {
    let package_hash = package.unique_identifier();
    let package_hash = base64::encode_config(package_hash, base64::URL_SAFE);
    let key = format!(
        "{} ({}, {}) - {}",
        package.name, package.supported_target, package.version, package_hash
    );
    // Keys cannot contain commas. Of course this is not documented.
    key.replace(',', ";")
}

fn default_target_for_platform() -> Result<Triple, Error> {
    use crate::node;
    use std::str::FromStr;
    let target = Triple::from_str(
        match (node::os::arch().as_str(), node::os::platform().as_str()) {
            ("arm64", "linux") => "aarch64-unknown-linux-gnu",
            ("ia32", "linux") => "i686-unknown-linux-gnu",
            ("ia32", "win32") => "i686-pc-windows-msvc",
            ("x64", "darwin") => "x86_64-apple-darwin",
            ("x64", "linux") => "x86_64-unknown-linux-gnu",
            ("x64", "win32") => "x86_64-pc-windows-msvc",
            (arch, platform) => {
                return Err(Error::UnsupportedPlatform(format!("{}-{}", platform, arch)))
            }
        },
    )
    .expect("Failed to parse hardcoded platform triple");
    Ok(target)
}

async fn install_package(package: &ManifestPackage) -> Result<(), Error> {
    use crate::actions::cache::CacheEntry;
    use crate::actions::tool_cache::{self, StreamCompression};
    use rust_toolchain_manifest::manifest::Compression;

    let key = compute_cache_key(package);
    let package_hash = package.unique_identifier();
    let extract_path = get_cacheable_path(&package_hash).await?;
    let mut cache_entry = CacheEntry::new(key.as_str());
    cache_entry.path(&extract_path);
    if let Some(key) = cache_entry.restore().await? {
        info!("Restored files from cache with key {}", key);
    } else {
        let remote_binary = package
            .tarballs
            .iter()
            .find(|(c, _)| *c == Compression::Gzip)
            .expect("Unable to find tar.gz")
            .1
            .clone();
        info!("Will need to download the following: {:#?}", remote_binary);
        let tarball_path = tool_cache::download_tool(remote_binary.url.as_str())
            .await
            .map_err(Error::Js)?;
        info!("Downloaded tarball to {}", tarball_path);
        info!("Will extract to {}", extract_path);
        tool_cache::extract_tar(&tarball_path, StreamCompression::Gzip, Some(&extract_path))
            .await?;
        info!("Extracted to {}", extract_path);
        let cache_id = cache_entry.save().await?;
        info!("Saved as {}", cache_id);
    }
    Ok(())
}

pub async fn install_toolchain(toolchain_config: &ToolchainConfig) -> Result<(), Error> {
    use crate::actions::tool_cache;
    use crate::node;
    use rust_toolchain_manifest::{InstallSpec, Manifest, Toolchain};
    use std::str::FromStr;

    let toolchain = Toolchain::from_str(&toolchain_config.name)?;
    let manifest_url = toolchain.manifest_url();
    info!(
        "Will download manifest for toolchain {} from {}",
        toolchain, manifest_url
    );
    let manifest_path = tool_cache::download_tool(manifest_url.as_str())
        .await
        .map_err(Error::Js)?;
    info!("Downloaded manifest to {}", manifest_path);
    let manifest = node::fs::read_file(&manifest_path).await?;
    let manifest = String::from_utf8(manifest).map_err(|_| Error::ManifestNotUtf8)?;
    let manifest = Manifest::try_from(manifest.as_str())?;
    let target = toolchain.host.unwrap_or(default_target_for_platform()?);
    info!("Attempting to find toolchain for target {}", target);
    let install_spec = InstallSpec {
        profile: toolchain_config.profile.clone(),
        components: toolchain_config.components.iter().cloned().collect(),
        targets: toolchain_config.targets.iter().cloned().collect(),
    };
    let downloads = manifest.find_downloads_for_install(&target, &install_spec)?;
    for download in downloads.iter() {
        install_package(download).await?;
    }
    Ok(())
}
