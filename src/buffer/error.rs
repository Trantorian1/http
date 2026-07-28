#[derive(Debug)]
pub enum Error {
    NoSpaceLeft,
    EndOfStream,
    Io(std::io::Error),
}
