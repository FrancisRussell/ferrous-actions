use crate::action_paths::get_action_cache_dir;
use crate::dir_tree::{self, DirTreeVisitor};
use crate::node::path::Path;
use crate::nonce::build_nonce;
use crate::{node, warning, Error};
use async_trait::async_trait;

const WAIT_ATIME_UPDATED_MS: u64 = 5;

fn default_access_time_offset() -> chrono::Duration {
    // This is somewhat arbitrary - we could set all access timestamps back to the
    // epoch. The offset time is guaranteed to be valid and is far enough in the
    // past to cover even vFAT access time granularity (days).
    chrono::Duration::hours(36)
}

pub struct RevertAccessTime {
    duration: chrono::Duration,
}

#[async_trait(?Send)]
impl DirTreeVisitor for RevertAccessTime {
    async fn enter_folder(&mut self, _: &Path) -> Result<(), Error> {
        Ok(())
    }

    async fn exit_folder(&mut self, _: &Path) -> Result<(), Error> {
        Ok(())
    }

    async fn visit_file(&mut self, path: &Path) -> Result<(), Error> {
        set_atime_behind_mtime(path, &self.duration).await
    }
}

pub async fn revert_folder_access_times(path: &Path) -> Result<(), Error> {
    let mut visitor = RevertAccessTime {
        duration: default_access_time_offset(),
    };
    let ignores = dir_tree::Ignores::default();
    dir_tree::apply_visitor(path, &ignores, &mut visitor).await?;
    Ok(())
}

async fn get_atime_check_dir() -> Result<Path, Error> {
    let mut dir = get_action_cache_dir()?;
    dir.push("check-atime-support");
    node::fs::create_dir_all(&dir).await?;
    Ok(dir)
}

async fn set_atime_behind_mtime(path: &Path, duration: &chrono::Duration) -> Result<(), Error> {
    let metadata = node::fs::symlink_metadata(path).await?;
    let m_time = metadata.modified();
    let a_time = m_time - *duration;
    node::fs::utimes(path, &a_time, &m_time).await?;
    Ok(())
}

pub async fn supports_atime() -> Result<bool, Error> {
    use crate::sleep;

    let atime_check_dir = get_atime_check_dir().await?;
    let file_path = {
        let mut file_path = atime_check_dir.clone();
        let nonce = build_nonce(8);
        file_path.push(nonce.to_string().as_str());
        file_path
    };
    let data = [0u8; 1];
    node::fs::write_file(&file_path, &data).await?;
    set_atime_behind_mtime(&file_path, &default_access_time_offset()).await?;
    {
        let metadata = node::fs::symlink_metadata(&file_path).await?;
        if metadata.accessed() >= metadata.modified() {
            // We expect setting access time to work
            // even on filesystems that never update it.
            warning!("Appeared to be unable to even set file time-stamps");
            return Ok(false);
        }
    }
    node::fs::read_file(&file_path).await?;
    // Wait a few ms, just in case
    sleep::sleep(&std::time::Duration::from_millis(WAIT_ATIME_UPDATED_MS)).await;
    let metadata = node::fs::symlink_metadata(&file_path).await?;
    // This needs to be >= and not > since times are discrete
    Ok(metadata.accessed() >= metadata.modified())
}
