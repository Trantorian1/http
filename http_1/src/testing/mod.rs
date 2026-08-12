pub struct MockTCP {
    read: std::collections::VecDeque<u8>,
    write: std::collections::VecDeque<u8>,
}

impl MockTCP {
    pub fn new<const N: usize>(data: [u8; N]) -> Self {
        Self {
            read: std::collections::VecDeque::from(data),
            write: std::collections::VecDeque::new(),
        }
    }
}

impl std::io::Read for MockTCP {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read.read(buf)
    }
}

impl std::io::Write for MockTCP {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.write.flush()
    }
}
