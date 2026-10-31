use alloc::string::String;

#[derive(Debug, thiserror::Error)]
pub enum AssetError {
    #[error("file {0} not found, neither loose nor in the archive")]
    Missing(String),
    #[error("archive {0}: {1}")]
    BadArchive(&'static str, &'static str),
    #[error("{file}: {reason}")]
    Malformed { file: String, reason: &'static str },
    #[error("no complete episode found in the game data")]
    NoEpisodes,
}

impl AssetError {
    /// A [`AssetError::Malformed`] in `file`.
    pub fn malformed(file: &str, reason: &'static str) -> Self {
        Self::Malformed {
            file: String::from(file),
            reason,
        }
    }
}
