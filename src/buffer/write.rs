use super::Buffer;

/// Misuse-resistant [`Buffer`] mutator. Allows the user to specify which parts of the HTTP message
/// to push to a given [`Writer`] while handling flushing and other buffering operations.
///
/// [`Writer`]: std::io::Write
pub struct BufWriter<'a, const SIZE: usize, W: std::io::Write> {
    view: &'a mut Buffer<SIZE>,
    writer: W,
}

impl<'a, const SIZE: usize, W: std::io::Write> BufWriter<'a, SIZE, W> {
    /// Writes new data to a [`Buffer`], handling flushing and buffering.
    pub fn write(&mut self, mut data: &[u8]) -> std::io::Result<()> {
        assert!(!data.is_empty());

        loop {
            data = self.view.append(data);

            if !data.is_empty() {
                self.writer.write_all(self.view.as_ref())?;
                self.view.reset();
            } else {
                break Ok(());
            }
        }
    }

    /// Force-flushes the [`Buffer`]. This should not need to be called by the end user but is
    /// exposed just in case.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.writer.write_all(self.view.as_ref())?;
        self.view.reset();

        Ok(())
    }

    /// New [`BufView`]s cannot be created by the user: they are only exposed by [`write_out`] as a
    /// safe writing interface.
    ///
    /// [`write_out`]: Buffer::write_out
    pub(crate) fn new(view: &'a mut Buffer<SIZE>, writer: W) -> Self {
        Self { view, writer }
    }
}
