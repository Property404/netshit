/// Result type for this crate
pub type Result<T=()> = std::result::Result<T, Error>;

/// Error type for this crate
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Nix(nix::errno::Errno)
}

impl From<nix::errno::Errno> for  Error {
    fn from(value: nix::errno::Errno) -> Self {
        Self::Nix(value)
    }
}


