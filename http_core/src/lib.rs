//! Shared HTTP primitives.

pub mod macros;
pub mod mem;
pub mod size;
pub mod status;
pub mod uri;

pub mod prelude {
    pub use super::mem::buffer::Buffer;
    pub use super::mem::buffer::BufferForReading;
    pub use super::mem::buffer::BufferForWriting;

    pub use super::mem::stream::ByteStream;

    pub use super::status::Status;

    pub use super::size::GB;
    pub use super::size::KB;
    pub use super::size::MB;

    pub use super::assert_le;
    pub use super::assert_leq;
}

pub use prelude::*;
