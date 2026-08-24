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
