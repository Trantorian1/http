use http_primitives::prelude::*;

use crate::prelude::*;

pub struct RequestHandle<'buf, 'data, 'stream, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    stream: &'stream mut W,
}

impl<'buf, 'data, 'writer, W> RequestHandle<'buf, 'data, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    pub fn new(buffer: &'buf mut BufferForWriting<'data>, stream: &'writer mut W) -> Self {
        Self { buffer, stream }
    }

    pub fn get<'inner>(self) -> Request<'buf, 'data, 'inner, 'writer, W> {
        Request::get(self.buffer, self.stream)
    }
}

pub struct Request<'buf, 'data, 'payload, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    buffer: &'buf mut BufferForWriting<'data>,
    stream: &'writer mut W,

    method: &'payload [u8],
    target: &'payload [u8],
}

impl<'buf, 'data, 'payload, 'writer, W> Request<'buf, 'data, 'payload, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    pub fn get(buffer: &'buf mut BufferForWriting<'data>, stream: &'writer mut W) -> Self {
        Self {
            buffer,
            stream,

            method: methods::GET,
            target: b"/",
        }
    }

    pub fn target(mut self, target: &'payload [u8]) -> Self {
        self.target = target;
        self
    }

    pub fn send(self) -> std::io::Result<()> {
        self.buffer.write_out(self.stream, |writer| {
            writer.write(self.method)?;
            writer.write(SP)?;
            writer.write(self.target)?;
            writer.write(SP)?;
            writer.write(PROTOCOL)?;
            writer.write(CRLF)?;
            writer.write(CRLF)?;
            Ok(())
        })
    }
}

impl<'buf, 'data, 'payload, 'writer, W> Drop for Request<'buf, 'data, 'payload, 'writer, W>
where
    'data: 'buf,
    W: std::io::Write,
{
    fn drop(&mut self) {
        self.buffer.clear();
    }
}
