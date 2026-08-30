//! Canonical package-content source decoding split by authority.

mod capability;
mod contribution;
mod conversation;
mod schema;
mod semantic;

pub(super) use capability::*;
pub(super) use contribution::*;
pub(super) use conversation::*;
pub(super) use schema::*;
pub(super) use semantic::*;
