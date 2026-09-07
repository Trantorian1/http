pub(super) struct ByteIter<'data> {
    next: usize,
    bytes: &'data [u8],
    next_tab_or_newline: usize,
}

pub(super) struct Checkpoint {
    next: usize,
    next_tab_or_newline: usize,
}

impl<'data> ByteIter<'data> {
    pub(super) fn new(bytes: &'data [u8]) -> Self {
        Self {
            next: 0,
            bytes,
            next_tab_or_newline: find_next_tab_or_newline(bytes),
        }
    }

    pub(super) fn peek(&mut self) -> Option<&'data u8> {
        self.skip_tabs_and_newlines();
        self.bytes.get(self.next)
    }

    pub(super) fn skip_if_matches(&mut self, needle: &[u8]) -> bool {
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

        self.next = i;
        self.next_tab_or_newline = find_next_tab_or_newline(&self.bytes[i..]) + i;

        true
    }

    pub(super) fn skip_while_matches2(&mut self, needle1: u8, needle2: u8) -> bool {
        let mut i = self.next;
        let mut next_tab_or_newline = self.next_tab_or_newline;

        // We need this here to ensure we start the skip routine on a non-tab or newline character
        while i == next_tab_or_newline && i < self.bytes.len() {
            i += 1;
            next_tab_or_newline = find_next_tab_or_newline(&self.bytes[i..]) + i;
        }

        let prev = i;

        // Keep skipping while we still have matches.
        while i < self.bytes.len() && (self.bytes[i] == needle1 || self.bytes[i] == needle2) {
            i += 1;

            // FIXME: this can be replaced by a single inverse search routine with minimal looping
            while i == next_tab_or_newline && i < self.bytes.len() {
                i += 1;
                next_tab_or_newline = find_next_tab_or_newline(&self.bytes[i..]) + i;
            }
        }

        self.next = i;
        self.next_tab_or_newline = find_next_tab_or_newline(&self.bytes[i..]) + i;

        i != prev
    }

    pub(super) fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            next: self.next,
            next_tab_or_newline: self.next_tab_or_newline,
        }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::new(self.bytes);
    }

    pub(super) fn reset_to(&mut self, checkpoint: Checkpoint) {
        self.next = checkpoint.next;
        self.next_tab_or_newline = checkpoint.next_tab_or_newline;
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
    fn byte_iter_skip_if_matches_with_match() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert!(iter.skip_if_matches(b"Hello, World"));

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn byte_iter_skip_if_matches_with_no_match() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert!(!iter.skip_if_matches(b"FizzBuzz"));

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
    fn skip_while_matches2_with_match() {
        let mut iter = ByteIter::new(b"\t\n\r//\\/\\\\\\/\\///\\///\\example.com");

        assert!(iter.skip_while_matches2(b'/', b'\\'));

        assert_char_eq!(*iter.next().unwrap(), b'e');
        assert_char_eq!(*iter.next().unwrap(), b'x');
        assert_char_eq!(*iter.next().unwrap(), b'a');
        assert_char_eq!(*iter.next().unwrap(), b'm');
        assert_char_eq!(*iter.next().unwrap(), b'p');
        assert_char_eq!(*iter.next().unwrap(), b'l');
        assert_char_eq!(*iter.next().unwrap(), b'e');
        assert_char_eq!(*iter.next().unwrap(), b'.');
        assert_char_eq!(*iter.next().unwrap(), b'c');
        assert_char_eq!(*iter.next().unwrap(), b'o');
        assert_char_eq!(*iter.next().unwrap(), b'm');

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn skip_while_matches2_with_no_match() {
        let mut iter = ByteIter::new(b"\t\n\rHello, \t\t\tWo\n\n\nrl\r\r\rd\t\n\r");

        assert!(!iter.skip_while_matches2(b'/', b'\\'));

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
