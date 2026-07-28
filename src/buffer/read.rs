use super::*;

pub struct ReadIn;

pub struct BufReader<'a, 'b, const SIZE: usize, R: std::io::Read> {
    buffer: &'a mut Buffer<SIZE, ReadIn>,
    reader: &'b mut R,
}

impl<const SIZE: usize> Buffer<SIZE, ReadIn> {
    pub fn for_reading() -> Self {
        Buffer::new()
    }

    pub fn read_in<'a, 'b, R: std::io::Read, T>(
        &'a mut self,
        reader: &'b mut R,
        apply_reads: impl FnOnce(&mut BufReader<'a, 'b, SIZE, R>) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut buf_reader = BufReader::new(self, reader);
        let reads = apply_reads(&mut buf_reader);

        buf_reader.flush();

        reads
    }

    fn process(&mut self, target: std::num::NonZeroUsize) -> std::ops::Range<usize> {
        let stop = self.window.start + target.get();
        let start = std::mem::replace(&mut self.window.start, stop);

        assert!(start < stop, "{start} < {stop}");
        assert!(stop <= SIZE, "{stop} < {SIZE}");
        start..stop
    }

    fn append_from<R: std::io::Read>(&mut self, stream: &mut R) -> std::io::Result<usize> {
        let bytes_read = stream.read(&mut self.buffer[self.window.end..])?;
        self.window.end += bytes_read;

        Ok(bytes_read)
    }
}

impl<'a, 'b, const SIZE: usize, R: std::io::Read> BufReader<'a, 'b, SIZE, R> {
    pub(crate) fn new(view: &'a mut Buffer<SIZE, ReadIn>, reader: &'b mut R) -> Self {
        Self {
            buffer: view,
            reader,
        }
    }

    pub fn read(
        &mut self,
        parse: fn(&[u8]) -> Result<Option<std::num::NonZeroUsize>, Error>,
    ) -> Result<std::ops::Range<usize>, Error> {
        loop {
            if let Some(index) = parse(self.buffer.as_ref())? {
                break Ok(self.buffer.process(index));
            }

            let new_bytes = self
                .buffer
                .append_from(&mut self.reader)
                .map_err(Error::Io)?;

            if self.buffer.len() == SIZE {
                return Err(Error::NoSpaceLeft);
            } else if new_bytes == 0 {
                return Err(Error::EndOfStream);
            }
        }
    }

    pub fn flush(&mut self) {
        self.buffer.window.start = 0;
    }
}
