//! [Url] parsing utilities.

mod error;
mod old;
mod parsing;
mod percent;
mod query;

pub use error::*;
pub use old::*;

pub struct Url<'data> {
    backing: &'data [u8],

    pub scheme: &'data [u8],
    pub username: &'data [u8],
    pub host: &'data [u8],
    pub port: &'data [u8],
    pub path: &'data [u8],
    pub query: &'data [u8],
    pub fragment: &'data [u8],
}
