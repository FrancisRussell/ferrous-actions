use crate::node::path::Path;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub enum EntryType {
    File,
    Directory,
}

impl FromStr for EntryType {
    type Err = PackageManifestParseError;

    fn from_str(string: &str) -> Result<EntryType, PackageManifestParseError> {
        let result = match string {
            "file" => EntryType::File,
            "dir" => EntryType::Directory,
            _ => {
                return Err(PackageManifestParseError::UnknownEntryType(
                    string.to_string(),
                ))
            }
        };
        Ok(result)
    }
}

#[derive(Debug, Clone, Error)]
pub enum PackageManifestParseError {
    #[error("Unknown entry type: {0}")]
    UnknownEntryType(String),

    #[error("Malformed line: {0}")]
    MalformedLine(String),
}

#[derive(Debug, Clone)]
pub struct PackageManifest {
    entries: Vec<(EntryType, Path)>,
}

impl PackageManifest {
    pub fn iter(&self) -> std::slice::Iter<'_, (EntryType, Path)> {
        self.entries.iter()
    }
}

impl FromStr for PackageManifest {
    type Err = PackageManifestParseError;

    fn from_str(string: &str) -> Result<PackageManifest, PackageManifestParseError> {
        let mut entries = Vec::new();
        for line in string.lines() {
            let split: Vec<_> = line.splitn(2, ':').collect();
            if split.len() != 2 {
                return Err(PackageManifestParseError::MalformedLine(line.to_string()));
            }
            let entry_type = EntryType::from_str(split[0])?;
            let path = Path::from(split[1]);
            entries.push((entry_type, path));
        }
        Ok(PackageManifest { entries })
    }
}
