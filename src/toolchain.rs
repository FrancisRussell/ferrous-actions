use crate::actions;
use crate::info;
use crate::node::{self, path::Path};
use crate::rustup::ToolchainConfig;
use crate::Error;
use async_recursion::async_recursion;
use rust_toolchain_manifest::{manifest::ManifestPackage, Toolchain};
use std::str::FromStr;
use target_lexicon::Triple;

const MAX_CONCURRENT_PACKAGE_INSTALLS: usize = 4;

fn get_cargo_home(toolchain: &Toolchain) -> Result<Path, Error> {
    let mut dir = node::os::homedir();
    dir.push(".local");
    dir.push("share");
    dir.push("github-rust-actions");
    dir.push("toolchains");
    dir.push(format!("{}", toolchain).as_str());
    Ok(dir)
}

fn get_package_decompress_path(package: &ManifestPackage) -> Result<Path, Error> {
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push("package-decompression");
    let package_hash = package.unique_identifier();
    dir.push(base64::encode_config(package_hash, base64::URL_SAFE).as_str());
    Ok(dir)
}

fn compute_package_cache_key(package: &ManifestPackage) -> String {
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

#[async_recursion(?Send)]
async fn overlay_and_move_dir(from: &Path, to: &Path) -> Result<(), Error> {
    node::fs::create_dir_all(to).await?;
    {
        let dir = node::fs::read_dir(from).await?;
        for entry in dir {
            let from = entry.path();
            let mut to = to.clone();
            to.push(entry.file_name().as_str());
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

    let cargo_home = get_cargo_home(toolchain)?;
    node::fs::create_dir_all(&cargo_home).await?;

    let extract_path = get_package_decompress_path(package)?;
    let dir = node::fs::read_dir(&extract_path).await?;
    info!("Directory: {}", extract_path);
    for entry in dir.filter(|d| d.file_type().is_dir()) {
        let mut components_path = entry.path();
        components_path.push("components");
        let components: Vec<String> = node::fs::read_file(&components_path)
            .await
            .map(|data| String::from_utf8_lossy(&data[..]).into_owned())?
            .lines()
            .map(String::from)
            .collect();
        for component in components {
            let mut component_path = entry.path();
            component_path.push(Path::from(component.as_str()));
            let mut manifest_path = component_path.clone();
            manifest_path.push("manifest.in");
            let manifest = node::fs::read_file(&manifest_path)
                .await
                .map(|data| String::from_utf8_lossy(&data[..]).into_owned())?;
            let manifest = PackageManifest::from_str(manifest.as_str())?;
            for (entry_type, path) in manifest.iter() {
                let mut source = component_path.clone();
                source.push(path.clone());
                let mut dest = cargo_home.clone();
                dest.push(path.clone());
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
    use actions::cache::CacheEntry;
    use actions::tool_cache::{self, StreamCompression};
    use rust_toolchain_manifest::manifest::Compression;

    let key = compute_package_cache_key(package);
    let extract_path = get_package_decompress_path(package)?;
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
    use actions::tool_cache;
    use futures::{StreamExt as _, TryStreamExt as _};
    use rust_toolchain_manifest::{InstallSpec, Manifest};

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
    let target = toolchain
        .host
        .clone()
        .unwrap_or(default_target_for_platform()?);
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
    let _collected: () = process_packages.try_collect().await?;
    Ok(())
}
