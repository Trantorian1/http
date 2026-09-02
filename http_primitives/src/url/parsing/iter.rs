pub struct ByteIter<'data> {
    next: usize,
    bytes: &'data [u8],
    next_tab_or_newline: usize,
}

impl<'data> ByteIter<'data> {
    pub fn new(bytes: &'data [u8]) -> Self {
        Self {
            next: 0,
            bytes,
            next_tab_or_newline: find_next_tab_or_newline(bytes),
        }
    }

    pub fn peek(&mut self) -> Option<&'data u8> {
        self.skip_tabs_and_newlines();
        self.bytes.get(self.next)
    }

    pub fn starts_with(&self, needle: &[u8]) -> bool {
        let mut i = self.next;
        let mut next_tab_or_newline = self.next_tab_or_newline;

        for c in needle {
            while i == next_tab_or_newline && i < self.bytes.len() {
                i += 1;
                next_tab_or_newline = find_next_tab_or_newline(&self.bytes[i..]) + i;
            }

            if i >= self.bytes.len() || c != &self.bytes[i] {
                return false;
            }

            i += 1;
        }

        true
    }

    pub fn reset(&mut self) {
        *self = Self::new(self.bytes);
    }

    fn skip_tabs_and_newlines(&mut self) {
        while self.next == self.next_tab_or_newline && self.next < self.bytes.len() {
            self.next += 1;
            self.next_tab_or_newline =
                find_next_tab_or_newline(&self.bytes[self.next..]) + self.next;
        }
    }
}

fn find_next_tab_or_newline(bytes: &[u8]) -> usize {
    memchr::memchr3(b'\t', b'\n', b'\r', bytes).unwrap_or(bytes.len())
}

impl<'data> Iterator for ByteIter<'data> {
    type Item = &'data u8;

    fn next(&mut self) -> Option<Self::Item> {
        self.skip_tabs_and_newlines();

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

    #[test]
    fn byte_iter_peek_some() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert_eq!(*iter.peek().unwrap(), b'H');

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

    #[test]
    fn byte_iter_peek_none() {
        let mut iter = ByteIter::new(b"");
        assert_eq!(iter.peek(), None);
    }

    #[test]
    fn byte_iter_starts_with() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert!(iter.starts_with(b"Hello, World"));

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

    #[test]
    fn byte_iter_reset() {
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

        iter.reset();

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
