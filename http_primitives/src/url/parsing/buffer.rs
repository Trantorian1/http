use super::*;

pub(super) struct UrlBuffer<'data> {
    backing: &'data mut [u8],
    next: usize,
}

impl<'data> UrlBuffer<'data> {
    pub(super) fn new(backing: &'data mut [u8]) -> Self {
        assert!(!backing.is_empty());
        Self { backing, next: 0 }
    }

    pub(super) fn push(&mut self, c: u8) -> Result<usize, Error> {
        if self.next < self.backing.len() {
            self.backing[self.next] = c;
            self.next += 1;
            Ok(self.next)
        } else {
            Err(Error::Overflow)
        }
    }

    pub(super) fn push_str(&mut self, bytes: &[u8]) -> Result<usize, Error> {
        if self.next + bytes.len() <= self.backing.len() {
            self.backing[self.next..self.next + bytes.len()].copy_from_slice(bytes);
            self.next += bytes.len();
            Ok(self.next)
        } else {
            Err(Error::Overflow)
        }
    }

    pub(super) fn push_encode_byte(
        &mut self,
        c: u8,
        set: percent::EncodeSet,
    ) -> Result<usize, Error> {
        let n = percent::encode(c, &mut self.backing[self.next..], set);
        if n == 0 {
            Err(Error::Overflow)
        } else {
            self.next += n;
            Ok(self.next)
        }
    }

    pub(super) fn clear(&mut self) {
        self.next = 0;
    }

    pub(super) fn into_inner(self) -> &'data [u8] {
        &self.backing[..self.next]
    }

    pub(super) fn len(&self) -> usize {
        self.next
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'data> AsRef<[u8]> for UrlBuffer<'data> {
    fn as_ref(&self) -> &[u8] {
        &self.backing[..self.next]
    }
}

impl<'data> std::ops::Index<usize> for UrlBuffer<'data> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.as_ref()[index]
    }
}

impl<'data> std::ops::Index<std::ops::Range<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeFrom<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeTo<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeInclusive<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeInclusive<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeToInclusive<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeToInclusive<usize>) -> &Self::Output {
        &self.as_ref()[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeFull> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFull) -> &Self::Output {
        &self.as_ref()[range]
    }
}
