use crate::prelude::*;

pub(crate) const MAX_SIZE: usize = 16;
const _: () = assert!(MAX_SIZE > 0);

pub(crate) fn buffer_write_invariant_problem(n_write: usize, capacity: usize) {
    let bytes: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);

    let mut stream_backing = [0; MAX_SIZE];
    let mut stream = ByteStream::new(&mut stream_backing);

    let mut buffer_backing = [0; MAX_SIZE];
    let mut buffer = BufferForWriting::new(&mut buffer_backing[..capacity]);

    buffer
        .write_out(&mut stream, |writer| writer.write(&bytes[..n_write]))
        .unwrap();

    let mut iter = stream.iter();
    for b in &bytes[..n_write] {
        assert_eq!(Some(*b), iter.next());
    }

    assert_eq!(stream.len(), n_write);
}

pub(crate) fn buffer_read_invariant_problem(
    n_read: usize,
    capacity: std::num::NonZeroUsize,
    chunk: std::num::NonZeroUsize,
) {
    let parser = |data: &[u8]| {
        if data.len() >= chunk.get() {
            // SAFETY: chunk is non-zero
            Ok(Some(unsafe {
                std::num::NonZeroUsize::new_unchecked(chunk.get())
            }))
        } else {
            Ok(None)
        }
    };

    let mut stream_backing: [u8; MAX_SIZE] = std::array::from_fn(|i| i as u8);
    let mut stream = ByteStream::any(&mut stream_backing, 0, n_read);

    let mut buffer_backing = [0; MAX_SIZE];
    let mut buffer = BufferForReading::new(&mut buffer_backing[..capacity.get()]);

    let res = buffer.read_in(&mut stream, |reader| reader.read(parser));

    if chunk > capacity {
        assert_eq!(res, Err(Status::ContentTooLarge));
    } else if chunk.get() > n_read {
        assert_eq!(res, Err(Status::RequestTimetout));
    } else {
        assert_eq!(res, Ok(0..chunk.get()));
    }
}
