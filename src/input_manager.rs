use std::collections::HashMap;
use strum::{EnumIter, IntoEnumIterator as _, IntoStaticStr};

#[derive(IntoStaticStr, Clone, Copy, Debug, EnumIter)]
pub enum Input {
    #[strum(serialize = "annotation")]
    Annotations,

    #[strum(serialize = "args")]
    Args,

    #[strum(serialize = "cache-only")]
    CacheOnly,

    #[strum(serialize = "command")]
    Command,

    #[strum(serialize = "default")]
    Default,

    #[strum(serialize = "min-recache-crates")]
    MinRecacheCrates,

    #[strum(serialize = "min-recache-git-repos")]
    MinRecacheGitRepos,

    #[strum(serialize = "min-recache-indices")]
    MinRecacheIndices,

    #[strum(serialize = "profile")]
    Profile,

    #[strum(serialize = "targets")]
    Targets,

    #[strum(serialize = "toolchain")]
    Toolchain,

    #[strum(serialize = "use-cross")]
    UseCross,
}

#[derive(Debug)]
pub struct Manager {
    inputs: HashMap<Input, String>,
}

impl Manager {
    pub fn build() -> Manager {
        let mut inputs = HashMap::new();

        for input in Input::iter() {
            let input: &'static str = input.into();
            crate::info!("Input name: {}", input);
        }

        Manager { inputs }
    }
}
