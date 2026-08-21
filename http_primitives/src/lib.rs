//! Shared HTTP primitives.

mod macros;
pub mod mem;
pub mod size;
pub mod status;

pub mod prelude {
    //! A “prelude” for crates using the `http_primitives` crate.
    //!
    //! This prelude is similar to the standard library’s prelude in that you’ll almost always want
    //! to import its entire contents, but unlike the standard library’s prelude you’ll have to do
    //! so manually:
    //!
    //! ```rust
    //! use http_primitives::prelude::*;
    //! ```
    //!
    //! The prelude may grow over time as additional items see ubiquitous use.

    pub use super::assert_gr;
    pub use super::assert_greq;
    pub use super::assert_le;
    pub use super::assert_leq;
    pub use super::mem::buffer::Buffer;
    pub use super::mem::buffer::BufferForReading;
    pub use super::mem::buffer::BufferForWriting;
    pub use super::mem::stream::ByteStream;
    pub use super::size::GB;
    pub use super::size::KB;
    pub use super::size::MB;
    pub use super::status::Status;
}

pub use prelude::*;
