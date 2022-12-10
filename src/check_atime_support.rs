use crate::action_paths::get_action_cache_dir;
use crate::node::path::Path;
use crate::nonce::build_nonce;
use crate::node;
use crate::Error;
use crate::warning;
use std::time::Duration;

async fn get_atime_check_dir() -> Result<Path, Error> {
    let mut dir = get_action_cache_dir()?;
    dir.push("check-atime-support");
    node::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

async fn set_atime_behind_mtime(path: &Path) -> Result<(), Error> {
    let metadata = node::fs::symlink_metadata(path).await?;
    let mtime = metadata.modified();
    let atime = mtime - chrono::Duration::minutes(1);
    node::fs::utimes(path, &atime, &mtime).await?;
    Ok(())
}

pub async fn supports_atime() -> Result<bool, Error> {
    let atime_check_dir = get_atime_check_dir().await?;
    let file_path = {
        let mut file_path = atime_check_dir.clone();
        let nonce = build_nonce(8);
        file_path.push(nonce.to_string().as_str());
        file_path
    };
    let data = [0u8; 1];
    node::fs::write_file(&file_path, &data).await?;
    set_atime_behind_mtime(&file_path).await?;
    {
        let metadata = node::fs::symlink_metadata(&file_path).await?;
        if metadata.accessed() >= metadata.modified() {
            // We expect setting access time to work
            // even on filesystems that never update it.
            warning!("Appeared to be unable to set file time-stamps");
            return Ok(false);
        }
    }
    // Even HFS+ only has second-granularity timestamps. ext3 *may* be second granularity in
    // certain cases.
    async_std::task::sleep(Duration::from_secs(2)).await;
    node::fs::read_file(&file_path).await?;
    {
        let metadata = node::fs::symlink_metadata(&file_path).await?;
        Ok(metadata.accessed() > metadata.modified())
    }
}
