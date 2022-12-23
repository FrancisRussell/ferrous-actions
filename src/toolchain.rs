use crate::action_paths::{get_action_cache_dir, get_action_share_dir};
use crate::actions::cache::Entry as CacheEntry;
use crate::node::path::Path;
use crate::node::{self};
use crate::rustup::ToolchainConfig;
use crate::{actions, info, Error};
use async_recursion::async_recursion;
use rust_toolchain_manifest::manifest::ManifestPackage;
use rust_toolchain_manifest::Toolchain;
use std::str::FromStr;
use target_lexicon::Triple;

const MAX_CONCURRENT_PACKAGE_INSTALLS: usize = 4;

fn get_toolchain_home(toolchain: &Toolchain) -> Result<Path, Error> {
    let dir = get_action_share_dir()?
        .join("toolchains")
        .join(toolchain.to_string().as_str());
    Ok(dir)
}

fn get_package_decompress_path(package: &ManifestPackage) -> Result<Path, Error> {
    // We must not use base64 encoding for the folder name because that
    // implies the platform filename is case sensitive.
    let package_hash = package.unique_identifier();
    let dir = get_action_cache_dir()?
        .join("package-decompression")
        .join(package_hash.to_string().as_str());
    Ok(dir)
}

fn compute_package_cache_key(package: &ManifestPackage) -> CacheEntry {
    use crate::cache_key_builder::CacheKeyBuilder;

    let mut builder = CacheKeyBuilder::new(&package.name);
    builder.add_id_bytes(package.unique_identifier().as_ref());
    builder.set_attribute("target", &package.supported_target.to_string());
    builder.set_attribute("version", &package.version);
    builder.into_entry()
}

fn default_target_for_platform() -> Result<Triple, Error> {
    let target = Triple::from_str(match (node::os::arch().as_str(), node::os::platform().as_str()) {
        ("arm64", "linux") => "aarch64-unknown-linux-gnu",
        ("ia32", "linux") => "i686-unknown-linux-gnu",
        ("ia32", "win32") => "i686-pc-windows-msvc",
        ("x64", "darwin") => "x86_64-apple-darwin",
        ("x64", "linux") => "x86_64-unknown-linux-gnu",
        ("x64", "win32") => "x86_64-pc-windows-msvc",
        (arch, platform) => return Err(Error::UnsupportedPlatform(format!("{}-{}", platform, arch))),
    })
    .expect("Failed to parse hardcoded platform triple");
    Ok(target)
}

#[async_recursion(?Send)]
async fn overlay_and_move_dir(from: &Path, to: &Path) -> Result<(), Error> {
    node::fs::create_dir_all(to).await?;
    {
        let dir = node::fs::read_dir(from).await?;
        for entry in dir {
            let from = entry.path();
            let to = to.join(entry.file_name().as_str());
            let file_type = entry.file_type();
            if file_type.is_dir() {
                overlay_and_move_dir(&from, &to).await?;
            } else {
                node::fs::rename(&from, &to).await?;
            }
        }
    }
    node::fs::remove_dir(from).await?;
    Ok(())
}

async fn install_components(toolchain: &Toolchain, package: &ManifestPackage) -> Result<(), Error> {
    use crate::package_manifest::{EntryType, PackageManifest};

    let cargo_home = get_toolchain_home(toolchain)?;
    node::fs::create_dir_all(&cargo_home).await?;

    let extract_path = get_package_decompress_path(package)?;
    let dir = node::fs::read_dir(&extract_path).await?;
    for entry in dir.filter(|d| d.file_type().is_dir()) {
        let components_path = entry.path().join("components");
        let components: Vec<String> = node::fs::read_file(&components_path)
            .await
            .map(|data| String::from_utf8_lossy(&data[..]).into_owned())?
            .lines()
            .map(String::from)
            .collect();
        for component in components {
            let component_path = entry.path().join(component.as_str());
            let manifest_path = component_path.clone().join("manifest.in");
            let manifest = node::fs::read_file(&manifest_path)
                .await
                .map(|data| String::from_utf8_lossy(&data[..]).into_owned())?;
            let manifest = PackageManifest::from_str(manifest.as_str())?;
            for (entry_type, path) in manifest.iter() {
                let source = component_path.join(path.clone());
                let dest = cargo_home.join(path.clone());
                node::fs::create_dir_all(&dest.parent()).await?;

                match *entry_type {
                    EntryType::File => node::fs::rename(&source, &dest).await?,
                    EntryType::Directory => overlay_and_move_dir(&source, &dest).await?,
                }
            }
        }
    }
    Ok(())
}

async fn cleanup_decompressed_package(package: &ManifestPackage) -> Result<(), Error> {
    let extract_path = get_package_decompress_path(package)?;
    actions::io::rm_rf(&extract_path).await?;
    Ok(())
}

async fn fetch_and_decompress_package(package: &ManifestPackage) -> Result<(), Error> {
    use actions::tool_cache::{self, StreamCompression};
    use rust_toolchain_manifest::manifest::Compression;

    let extract_path = get_package_decompress_path(package)?;
    let mut cache_entry = compute_package_cache_key(package);
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
        tool_cache::extract_tar(&tarball_path, StreamCompression::Gzip, Some(&extract_path)).await?;
        info!("Extracted to {}", extract_path);
        let cache_id = cache_entry.save().await?;
        info!("Saved as {}", cache_id);
    }
    Ok(())
}

pub async fn install(toolchain_config: &ToolchainConfig) -> Result<(), Error> {
    use actions::tool_cache;
    use futures::{StreamExt as _, TryStreamExt as _};
    use rust_toolchain_manifest::{InstallSpec, Manifest};

    let toolchain = {
        let mut toolchain = Toolchain::from_str(&toolchain_config.name)?;
        toolchain.host = Some(match toolchain.host {
            Some(host) => host,
            None => default_target_for_platform()?,
        });
        toolchain
    };
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
    let target = toolchain.host.clone().expect("Toolchain target unexpectedly missing");
    info!("Attempting to find toolchain for target {}", target);
    let install_spec = InstallSpec {
        profile: toolchain_config.profile.clone(),
        components: toolchain_config.components.iter().cloned().collect(),
        targets: toolchain_config.targets.iter().cloned().collect(),
    };
    let downloads = manifest.find_downloads_for_install(&target, &install_spec)?;
    let process_packages = futures::stream::iter(downloads.iter())
        .map(|download| async {
            fetch_and_decompress_package(download).await?;
            install_components(&toolchain, download).await?;
            cleanup_decompressed_package(download).await?;
            Ok::<_, Error>(())
        })
        .buffer_unordered(MAX_CONCURRENT_PACKAGE_INSTALLS);
    process_packages.try_collect().await?;

    if toolchain_config.set_default {
        let cargo_bin = get_toolchain_home(&toolchain)?.join("bin");
        actions::core::add_path(&cargo_bin);
    } else {
        return Err(Error::ToolchainInstallFunctionality("default=false".into()));
    }
    if toolchain_config.set_override {
        return Err(Error::ToolchainInstallFunctionality("override".into()));
    }
    Ok(())
}
