#[derive(Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub(crate) struct ByteStr<'a>(pub &'a mut [u8]);

impl AsRef<[u8]> for ByteStr<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0
    }
}

impl<'a> From<&'a mut [u8]> for ByteStr<'a> {
    fn from(bytes: &'a mut [u8]) -> Self {
        Self(bytes)
    }
}

impl<'a, const N: usize> From<&'a mut [u8; N]> for ByteStr<'a> {
    fn from(bytes: &'a mut [u8; N]) -> Self {
        Self(bytes)
    }
}

impl<'a> From<ByteStr<'a>> for &'a [u8] {
    fn from(bytestr: ByteStr<'a>) -> Self {
        bytestr.0
    }
}

impl<'a> From<ByteStr<'a>> for &'a mut [u8] {
    fn from(bytestr: ByteStr<'a>) -> Self {
        bytestr.0
    }
}

#[cfg(feature = "alloc")]
impl From<ByteStr<'_>> for Vec<u8> {
    fn from(bytestr: ByteStr<'_>) -> Self {
        bytestr.0.to_owned()
    }
}

impl<'a> TryFrom<ByteStr<'a>> for &'a str {
    type Error = std::str::Utf8Error;

    fn try_from(bytestr: ByteStr<'a>) -> Result<Self, Self::Error> {
        std::str::from_utf8(bytestr.0)
    }
}

impl<'a> TryFrom<ByteStr<'a>> for &'a mut str {
    type Error = std::str::Utf8Error;

    fn try_from(bytestr: ByteStr<'a>) -> Result<Self, Self::Error> {
        std::str::from_utf8_mut(bytestr.0)
    }
}

#[cfg(feature = "alloc")]
impl<'a> TryFrom<ByteStr<'a>> for String {
    type Error = std::str::Utf8Error;

    fn try_from(bytestr: ByteStr<'a>) -> Result<Self, Self::Error> {
        std::str::from_utf8(bytestr.0).map(String::from)
    }
}

impl PartialEq<&[u8]> for ByteStr<'_> {
    fn eq(&self, bytestr: &&[u8]) -> bool {
        &self.0 == bytestr
    }
}

impl PartialEq<&mut [u8]> for ByteStr<'_> {
    fn eq(&self, bytestr: &&mut [u8]) -> bool {
        &self.0 == bytestr
    }
}

impl PartialEq<ByteStr<'_>> for &[u8] {
    fn eq(&self, bytestr: &ByteStr<'_>) -> bool {
        self == &bytestr.0
    }
}

impl PartialEq<ByteStr<'_>> for &mut [u8] {
    fn eq(&self, bytestr: &ByteStr<'_>) -> bool {
        self == &bytestr.0
    }
}

impl<const N: usize> PartialEq<[u8; N]> for ByteStr<'_> {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.0 == other
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for ByteStr<'_> {
    fn eq(&self, other: &&[u8; N]) -> bool {
        &self.0 == other
    }
}

impl<const N: usize> PartialEq<&mut [u8; N]> for ByteStr<'_> {
    fn eq(&self, other: &&mut [u8; N]) -> bool {
        &self.0 == other
    }
}

impl std::fmt::Debug for ByteStr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ByteStr")
            .field(&std::str::from_utf8(self.0).unwrap_or_default())
            .finish()
    }
}

impl std::fmt::Display for ByteStr<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", std::str::from_utf8(self.0).unwrap_or_default())
    }
}
