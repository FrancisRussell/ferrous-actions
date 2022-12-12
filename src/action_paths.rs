use crate::{node, Error};
use node::path::Path;
use std::borrow::Cow;

pub fn get_action_name() -> Cow<'static, str> {
    "github-rust-actions".into()
}

pub fn get_action_share_dir() -> Result<Path, Error> {
    Ok(node::os::homedir()
        .join(".local")
        .join("share")
        .join(get_action_name().as_ref()))
}

pub fn get_action_cache_dir() -> Result<Path, Error> {
    Ok(node::os::homedir().join(".cache").join(get_action_name().as_ref()))
}
