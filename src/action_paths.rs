use crate::{node, Error};
use node::path::Path;
use std::borrow::Cow;

pub fn get_action_name() -> Cow<'static, str> {
    "github-rust-actions".into()
}

pub fn get_action_share_dir() -> Result<Path, Error> {
    let mut dir = node::os::homedir();
    dir.push(".local");
    dir.push("share");
    dir.push(get_action_name().as_ref());
    Ok(dir)
}

pub fn get_action_cache_dir() -> Result<Path, Error> {
    let mut dir = node::os::homedir();
    dir.push(".cache");
    dir.push(get_action_name().as_ref());
    Ok(dir)
}
