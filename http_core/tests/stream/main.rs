use std::io::Read as _;
use std::io::Write as _;

#[macro_export]
macro_rules! with_stream_size {
    ($SIZE:literal, $data:ident) => {
        test::<$SIZE>($data)
    };
}

fn main() {
    bolero::check!().for_each(|data| {
        with_stream_size!(1, data);
        with_stream_size!(2, data);
        with_stream_size!(3, data);
        with_stream_size!(4, data);
        with_stream_size!(5, data);
        with_stream_size!(8, data);
        with_stream_size!(16, data);
    })
}

fn test<const SIZE: usize>(data: &[u8]) {
    let mut buffer = [0; SIZE];
    let mut stream = http_core::ByteStream::<SIZE>::new();
    let mut oracle = std::collections::VecDeque::<u8>::with_capacity(SIZE);

    let bytes = stream
        .write(data)
        .expect("Writing to a byte stream cannot fail");

    if data.len() > SIZE {
        assert_eq!(bytes, SIZE);
    } else {
        assert_eq!(bytes, data.len());
    }

    for byte in data.iter().copied().take(SIZE) {
        oracle.push_front(byte);
    }

    assert_eq!(stream.len(), bytes);
    assert_eq!(stream.len(), oracle.len());

    let bytes = stream
        .read(&mut buffer)
        .expect("Reading from a byte stream cannot fail");

    if data.len() > SIZE {
        assert_eq!(bytes, SIZE);
    } else {
        assert_eq!(bytes, data.len());
    }

    for (index, byte) in oracle.drain(..).rev().enumerate() {
        assert_eq!(byte, buffer[index]);
    }

    assert_eq!(stream.len(), 0);
    assert!(stream.is_empty());
}
