pub fn decode<I: Iterator<Item = u8>>(iter: I) -> DecodeIter<I> {
    DecodeIter::new(iter)
}

pub struct DecodeIter<I: Iterator<Item = u8>> {
    iter: I,
    backing: [u8; 2],
    size: u8,
}

impl<I: Iterator<Item = u8>> DecodeIter<I> {
    pub fn new(iter: I) -> Self {
        Self {
            iter,
            backing: [0; 2],
            size: 0,
        }
    }

    fn after_percent_sign(&mut self) -> Option<u8> {
        self.backing[0] = self.iter.next()?;
        self.size = 1;

        #[cfg(test)]
        let _h = char::from_u32(self.backing[0] as u32).unwrap_or_default();
        let h = (self.backing[0] as char).to_digit(16)? as u8;

        self.backing[1] = self.iter.next()?;
        self.size = 2;

        #[cfg(test)]
        let _l = char::from_u32(self.backing[1] as u32).unwrap_or_default();
        let l = (self.backing[1] as char).to_digit(16)? as u8;

        self.size = 0;

        Some(h * 16 + l)
    }

    fn pop(&mut self) -> Option<u8> {
        if self.size > 0 {
            let i = 2 - self.size as usize;
            self.size -= 1;
            Some(self.backing[i])
        } else {
            None
        }
    }
}

impl<I: Iterator<Item = u8>> Iterator for DecodeIter<I> {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(prev) = self.pop() {
            return Some(prev);
        }

        match self.iter.next() {
            Some(b'%') => Some(self.after_percent_sign().unwrap_or(b'%')),
            Some(c) => Some(c),
            None => None,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn decode_simple() {
        let message = b"Hello%20World";
        let mut iter = decode(message.into_iter().copied());

        assert_char_eq!(iter.next().unwrap(), b'H');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b' ');
        assert_char_eq!(iter.next().unwrap(), b'W');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'd');

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decode_uppercase() {
        let message = b"username%3Apassword";
        let mut iter = decode(message.into_iter().copied());

        assert_char_eq!(iter.next().unwrap(), b'u');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'n');
        assert_char_eq!(iter.next().unwrap(), b'a');
        assert_char_eq!(iter.next().unwrap(), b'm');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b':');
        assert_char_eq!(iter.next().unwrap(), b'p');
        assert_char_eq!(iter.next().unwrap(), b'a');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b'w');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'd');

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decode_lowercase() {
        let message = b"username%3apassword";
        let mut iter = decode(message.into_iter().copied());

        assert_char_eq!(iter.next().unwrap(), b'u');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'n');
        assert_char_eq!(iter.next().unwrap(), b'a');
        assert_char_eq!(iter.next().unwrap(), b'm');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b':');
        assert_char_eq!(iter.next().unwrap(), b'p');
        assert_char_eq!(iter.next().unwrap(), b'a');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b's');
        assert_char_eq!(iter.next().unwrap(), b'w');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'd');

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn decode_invalid_encoding() {
        let message = b"Hello%0GWorld";
        let mut iter = decode(message.into_iter().copied());

        assert_char_eq!(iter.next().unwrap(), b'H');
        assert_char_eq!(iter.next().unwrap(), b'e');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b'%');
        assert_char_eq!(iter.next().unwrap(), b'0');
        assert_char_eq!(iter.next().unwrap(), b'G');
        assert_char_eq!(iter.next().unwrap(), b'W');
        assert_char_eq!(iter.next().unwrap(), b'o');
        assert_char_eq!(iter.next().unwrap(), b'r');
        assert_char_eq!(iter.next().unwrap(), b'l');
        assert_char_eq!(iter.next().unwrap(), b'd');

        assert_eq!(iter.next(), None);
    }
}
