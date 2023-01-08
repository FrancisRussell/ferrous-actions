use crate::node;
use crate::node::path::Path;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct AgnosticPath {
    components: Vec<String>,
    trailing_separator: bool,
}

impl AgnosticPath {
    fn to_string_with_separator(&self, separator: &str) -> String {
        use itertools::Itertools as _;
        let trailing = self.trailing_separator.then_some("");
        self.components
            .iter()
            .map(String::as_str)
            .chain(trailing.into_iter())
            .join(separator)
    }
}

impl From<&Path> for AgnosticPath {
    fn from(os_path: &Path) -> AgnosticPath {
        let os_path = os_path.to_string();
        let separator = node::path::separator();
        let (os_path, trailing_separator) = if let Some(stripped) = os_path.strip_suffix(separator.as_ref()) {
            (stripped, true)
        } else {
            (os_path.as_str(), false)
        };
        let components = os_path.split(separator.as_ref()).map(str::to_string).collect();
        AgnosticPath {
            components,
            trailing_separator,
        }
    }
}

impl From<&AgnosticPath> for Path {
    fn from(path: &AgnosticPath) -> Path {
        let os_path = path.to_string_with_separator(&node::path::separator());
        Path::from(&os_path)
    }
}

impl std::fmt::Display for AgnosticPath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        let string = self.to_string_with_separator("/");
        string.fmt(formatter)
    }
}
