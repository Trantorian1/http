pub(crate) struct ByteIter<'data> {
    next: usize,
    bytes: &'data [u8],
    next_tab_or_newline: usize,
}

impl<'data> ByteIter<'data> {
    pub(crate) fn new(bytes: &'data [u8]) -> Self {
        Self {
            next: 0,
            bytes,
            next_tab_or_newline: memchr::memchr3(b'\t', b'\n', b'\r', bytes).unwrap_or(bytes.len()),
        }
    }
}

impl<'data> Iterator for ByteIter<'data> {
    type Item = &'data u8;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next == self.next_tab_or_newline && self.next < self.bytes.len() {
            self.next += 1;
            self.next_tab_or_newline =
                memchr::memchr3(b'\t', b'\n', b'\r', &self.bytes[self.next..])
                    .unwrap_or(self.bytes.len())
                    + self.next;
        }

        if self.next < self.bytes.len() {
            let next = self.next + 1;
            let prev = std::mem::replace(&mut self.next, next);

            Some(&self.bytes[prev])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn byte_iter_skip_tabs_and_newlines() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert_char_eq!(*iter.next().unwrap(), b'H');
        assert_char_eq!(*iter.next().unwrap(), b'e');
        assert_char_eq!(*iter.next().unwrap(), b'l');
        assert_char_eq!(*iter.next().unwrap(), b'l');
        assert_char_eq!(*iter.next().unwrap(), b'o');
        assert_char_eq!(*iter.next().unwrap(), b',');
        assert_char_eq!(*iter.next().unwrap(), b' ');
        assert_char_eq!(*iter.next().unwrap(), b'W');
        assert_char_eq!(*iter.next().unwrap(), b'o');
        assert_char_eq!(*iter.next().unwrap(), b'r');
        assert_char_eq!(*iter.next().unwrap(), b'l');
        assert_char_eq!(*iter.next().unwrap(), b'd');

        assert_eq!(iter.next(), None);
    }
}
