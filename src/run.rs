use crate::actions::core::{self, Input};
use crate::node::path::Path;
use crate::Error;
use crate::{debug, info};
use crate::{rustup::ToolchainConfig, Cargo, Rustup};
use rust_toolchain_manifest::manifest::ManifestPackage;
use rust_toolchain_manifest::HashValue;
use target_lexicon::Triple;

pub async fn run() -> Result<(), Error> {
    // Get the action input.
    let actor = core::get_input("actor")?.unwrap_or_else(|| String::from("world"));

    // Greet the workflow actor.
    info!("Hello, {}!", actor);

    let command: String = Input::from("command").get_required()?;
    let split: Vec<&str> = command.split_whitespace().collect();
    match split[..] {
        ["install-rustup"] => install_rustup().await?,
        ["toolchain"] => install_toolchain().await?,
        ["cargo", cargo_subcommand] => {
            let mut cargo = Cargo::from_environment().await?;
            let cargo_args = Input::from("args").get()?.unwrap_or_default();
            let cargo_args = shlex::split(&cargo_args)
                .ok_or_else(|| Error::ArgumentsParseError(cargo_args.clone()))?;
            let toolchain = core::get_input("toolchain")?;
            cargo
                .run(
                    toolchain.as_deref(),
                    cargo_subcommand,
                    cargo_args.iter().map(String::as_str),
                )
                .await?;
        }
        _ => return Err(Error::UnknownCommand(command)),
    }

    // Set the action output.
    core::set_output("result", "success");

    Ok(())
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

fn get_toolchain_config() -> Result<ToolchainConfig, Error> {
    let mut toolchain_config = ToolchainConfig::default();
    if let Some(toolchain) = core::get_input("toolchain")? {
        toolchain_config.name = toolchain;
    }
    if let Some(profile) = core::get_input("profile")? {
        toolchain_config.profile = profile;
    }
    if let Some(components) = core::get_input("components")? {
        toolchain_config.components = components.split_whitespace().map(String::from).collect();
    }
    if let Some(targets) = core::get_input("targets")? {
        toolchain_config.targets = targets.split_whitespace().map(String::from).collect();
    }
    if let Some(set_default) = core::get_input("default")? {
        let set_default = set_default
            .parse::<bool>()
            .map_err(|_| Error::OptionParseError("default".into(), set_default.clone()))?;
        toolchain_config.default = set_default;
    }
    Ok(toolchain_config)
}

async fn get_cacheable_path(key: &HashValue) -> Result<Path, Error> {
    use crate::node;
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push("github-rust-actions");
    dir.push(key.to_string().as_str());
    Ok(dir)
}

async fn install_rustup() -> Result<(), Error> {
    let rustup = Rustup::get_or_install().await?;
    debug!("Rustup installed at: {}", rustup.get_path());
    rustup.update().await?;
    let toolchain_config = get_toolchain_config()?;
    rustup.install_toolchain(&toolchain_config).await?;
    Ok(())
}

async fn install_package(package: &ManifestPackage) -> Result<(), Error> {
    use crate::actions::cache;
    use crate::actions::tool_cache::{self, StreamCompression};
    use rust_toolchain_manifest::manifest::Compression;

    let package_hash = package.unique_identifier();
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
    let extract_path = get_cacheable_path(&package_hash).await?;
    info!("Will extract to {}", extract_path);
    let extracted =
        tool_cache::extract_tar(&tarball_path, StreamCompression::Gzip, Some(&extract_path))
            .await?;
    info!("Extracted to {}", extracted);
    let key = format!("{} ({}) - {}", package.name, package.version, package_hash);
    let paths = [extracted];
    let cache_id = cache::save_cache(&paths, &key).await?;
    info!("Saved as {}", cache_id);
    Ok(())
}

async fn install_toolchain() -> Result<(), Error> {
    use crate::actions::tool_cache;
    use crate::node;
    use rust_toolchain_manifest::{InstallSpec, Manifest, Toolchain};
    use std::str::FromStr;

    let toolchain_config = get_toolchain_config()?;
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
    for download in downloads.iter().take(1) {
        install_package(download).await?;
    }
    Ok(())
}
