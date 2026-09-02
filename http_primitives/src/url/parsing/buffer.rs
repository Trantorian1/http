use super::Error;

pub struct UrlBuffer<'data> {
    backing: &'data mut [u8],
    next: usize,
}

impl<'data> UrlBuffer<'data> {
    pub fn new(backing: &'data mut [u8]) -> Self {
        assert!(!backing.is_empty());
        Self { backing, next: 0 }
    }

    pub fn push(&mut self, c: u8) -> Result<(), Error> {
        if self.next < self.backing.len() {
            self.backing[self.next] = c;
            self.next += 1;
            Ok(())
        } else {
            Err(Error::Overflow)
        }
    }

    pub fn clear(&mut self) {
        self.next = 0;
    }

    pub fn into_inner(self) -> &'data [u8] {
        &self.backing[..self.next]
    }

    pub fn len(&self) -> usize {
        self.next
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<'data> AsRef<[u8]> for UrlBuffer<'data> {
    fn as_ref(&self) -> &[u8] {
        &self.backing[..self.next]
    }
}

impl<'data> std::ops::Index<std::ops::Range<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::Range<usize>) -> &Self::Output {
        &self.backing[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeFrom<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFrom<usize>) -> &Self::Output {
        &self.backing[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeTo<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeTo<usize>) -> &Self::Output {
        &self.backing[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeInclusive<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeInclusive<usize>) -> &Self::Output {
        &self.backing[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeToInclusive<usize>> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeToInclusive<usize>) -> &Self::Output {
        &self.backing[range]
    }
}

impl<'data> std::ops::Index<std::ops::RangeFull> for UrlBuffer<'data> {
    type Output = [u8];

    fn index(&self, range: std::ops::RangeFull) -> &Self::Output {
        &self.backing[range]
    }
}
