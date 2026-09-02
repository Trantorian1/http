//! [Url] parsing utilities.

mod old;
mod parsing;
mod percent;
mod query;

pub use old::*;
pub use parsing::*;

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

impl<'data> std::fmt::Debug for Url<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let backing = str::from_utf8(&self.backing).unwrap_or_default();
        let scheme = str::from_utf8(&self.scheme).unwrap_or_default();
        let username = str::from_utf8(&self.username).unwrap_or_default();
        let host = str::from_utf8(&self.host).unwrap_or_default();
        let port = str::from_utf8(&self.port).unwrap_or_default();
        let path = str::from_utf8(&self.path).unwrap_or_default();
        let query = str::from_utf8(&self.query).unwrap_or_default();
        let fragment = str::from_utf8(&self.fragment).unwrap_or_default();

        f.debug_struct("Url")
            .field("backing", &backing)
            .field("scheme", &scheme)
            .field("username", &username)
            .field("host", &host)
            .field("port", &port)
            .field("path", &path)
            .field("query", &query)
            .field("fragment", &fragment)
            .finish()
    }
}

impl<'data> std::fmt::Display for Url<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(str::from_utf8(&self.backing).unwrap_or_default())
    }
}
