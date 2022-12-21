use crate::actions::core;
use crate::Error;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
use strum::{EnumIter, IntoEnumIterator as _, IntoStaticStr};

#[derive(IntoStaticStr, Clone, Copy, Debug, EnumIter, Eq, Hash, PartialEq)]
pub enum Input {
    #[strum(serialize = "annotations")]
    Annotations,

    #[strum(serialize = "args")]
    Args,

    #[strum(serialize = "cache-only")]
    CacheOnly,

    #[strum(serialize = "command")]
    Command,

    #[strum(serialize = "components")]
    Components,

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
    accessed: Mutex<HashSet<Input>>,
}

impl Manager {
    pub fn build() -> Result<Manager, Error> {
        let mut inputs = HashMap::new();
        for input in Input::iter() {
            let input_name: &str = input.into();
            if let Some(value) = core::Input::from(input_name).get()? {
                inputs.insert(input, value);
            }
        }
        Ok(Manager {
            inputs,
            accessed: Mutex::default(),
        })
    }

    pub fn get(&self, input: Input) -> Option<&str> {
        self.accessed.lock().insert(input);
        self.inputs.get(&input).map(String::as_str)
    }

    pub fn get_required(&self, input: Input) -> Result<&str, Error> {
        self.get(input).ok_or_else(|| {
            let input_name: &str = input.into();
            Error::MissingInput(input_name.into())
        })
    }

    pub fn unused(&self) -> HashSet<Input> {
        let available: HashSet<_> = self.inputs.keys().copied().collect();
        &available - &self.accessed.lock()
    }
}
