use thiserror::Error;

use crate::{Entity, Namespace};

#[derive(Error, Debug, Clone, PartialEq)]
pub enum Error {
    #[error("The namespace {0} does not exist.")]
    NoSuchNamespace(Namespace),
    #[error("The entity {0} does not exist or has been despawned.")]
    NoSuchEntity(Entity),
    #[error("The entity {0} does not have the component {1:?}.")]
    MissingComponent(Entity, &'static str),
    #[error("The entity {0} did not match the fetch {1:?}.\nMissing {2:?}.")]
    UnmatchedFetch(Entity, String, Vec<String>),
    #[error("Component {0} is already borrowed mutably")]
    Borrow(&'static str),
    #[error("Component {0} can not be borrowed mutably as it is already borrowed")]
    BorrowMut(&'static str),
    #[error("Entities {0:?} were not disjoint")]
    Disjoint(Vec<Entity>),
}

#[derive(Debug, Error)]
#[error("Failed to execute system {:?}", name.as_ref().map(|v| v.as_str()).unwrap_or_else(|| "unnkown"))]
pub struct SystemError {
    pub(crate) name: Option<String>,
    #[source]
    pub(crate) report: eyre::Report,
}

pub type Result<T> = std::result::Result<T, Error>;
pub type SystemResult<T> = std::result::Result<T, SystemError>;
