type Chunk = u32;
const ASCII_RANGE: usize = 0x80;
const BITS_PER_CHUNK: usize = 8 * std::mem::size_of::<Chunk>();

/// The C0 control percent-encode set is a [`PercentEncodeSet`] consisting of C0 controls and
/// all code points greater than U+007E (~).
pub const C0_CONTROL: PercentEncodeSet = PercentEncodeSet::new()
    // C0 controls
    .add(0x0)
    .add(0x1)
    .add(0x2)
    .add(0x3)
    .add(0x4)
    .add(0x5)
    .add(0x6)
    .add(0x7)
    .add(0x8)
    .add(0x9)
    .add(0xA)
    .add(0xB)
    .add(0xC)
    .add(0xD)
    .add(0xE)
    .add(0xF)
    .add(0x10)
    .add(0x11)
    .add(0x12)
    .add(0x13)
    .add(0x14)
    .add(0x15)
    .add(0x16)
    .add(0x17)
    .add(0x18)
    .add(0x19)
    .add(0x1A)
    .add(0x1B)
    .add(0x1C)
    .add(0x1D)
    .add(0x1E)
    .add(0x1F)
    // Code points greater than U+007E (~)
    .add(0x7F);

/// The fragment [`PercentEncodeSet`] is a percent-encode set consisting of the C0 control
/// percent-encode set and U+0020 SPACE, U+0022 ("), U+003C (<), U+003E (>), and U+0060 (`).
pub const FRAGMENT: PercentEncodeSet = C0_CONTROL
    // U+0020
    .add(b' ')
    // U+0022
    .add(b'"')
    // U+003C
    .add(b'<')
    // U+003E
    .add(b'>')
    //  U+0060
    .add(b'`');

/// The query [`PercentEncodeSet`] is a percent-encode set consisting of the C0 control percent-
/// encode set and U+0020 SPACE, U+0022 ("), U+0023 (#), U+003C (<), and U+003E (>).
pub const QUERY: PercentEncodeSet = C0_CONTROL
    // U+0020
    .add(b' ')
    // U+0022
    .add(b'"')
    // U+0023
    .add(b'#')
    // U+003C
    .add(b'<')
    // U+003E
    .add(b'>');

/// The special-query [`PercentEncodeSet`] is a percent-encode set consisting of the query
/// percent-encode set and U+0027 (').
pub const QUERY_SPECIAL: PercentEncodeSet = QUERY
    // U+0027
    .add(b'\'');

/// The path percent-encode set is a [`PercentEncodeSet`] consisting of the query percent-encode
/// set and U+003F (?), U+005E (^), U+0060 (`), U+007B ({), and U+007D (}).
pub const PATH: PercentEncodeSet = QUERY
    // U+003F
    .add(b'?')
    // U+005E
    .add(b'^')
    // U+0060
    .add(b'`')
    // U+007B
    .add(b'{')
    // U+007D
    .add(b'}');

/// The userinfo [`PercentEncodeSet`] is a percent-encode set consisting of the path
/// percent-encode set and U+002F (/), U+003A (:), U+003B (;), U+003D (=), U+0040 (@),
/// U+005B ([) to U+005D (]), inclusive, and U+007C (|).
pub const USERINFO: PercentEncodeSet = PATH
    // U+002F
    .add(b'/')
    // U+003A
    .add(b':')
    // U+003B
    .add(b';')
    // U+003D
    .add(b'=')
    // U+0040
    .add(b'@')
    // U+005B
    .add(b'[')
    // U+005C
    .add(b'\\')
    // U+005D
    .add(b']')
    // U+007C
    .add(b'|');

/// The component [`PercentEncodeSet`] is a percent-encode set consisting of the userinfo
/// percent-encode set and U+0024 ($) to U+0026 (&), inclusive, U+002B (+), and U+002C (,).
pub const COMPONENT: PercentEncodeSet = USERINFO
    // U+0024
    .add(b'$')
    // U+0025
    .add(b'%')
    // U+0026
    .add(b'&')
    // U+002B
    .add(b'+')
    // U+002C
    .add(b',');

/// A percent-encode set is a set specifying which [code points] to [percent-encode].
///
/// [code points]: https://infra.spec.whatwg.org/#code-point
/// [percent-encode]: https://url.spec.whatwg.org/#percent-encoded-bytes
#[derive(Clone, Copy)]
pub struct PercentEncodeSet {
    bitset: [Chunk; ASCII_RANGE / BITS_PER_CHUNK],
}

impl PercentEncodeSet {
    pub const fn new() -> Self {
        Self {
            bitset: [0; ASCII_RANGE / BITS_PER_CHUNK],
        }
    }

    pub const fn add(mut self, c: u8) -> Self {
        assert!(c.is_ascii());

        let i = c as usize / BITS_PER_CHUNK;
        let mask = 1 << c as usize - i * BITS_PER_CHUNK;

        self.bitset[i] |= mask;

        self
    }

    pub(crate) const fn contains(&self, c: u8) -> bool {
        let i = c as usize / BITS_PER_CHUNK;
        let mask = 1 << c as usize - i * BITS_PER_CHUNK;

        self.bitset[i] & mask != 0
    }

    pub(crate) fn should_percent_encode(&self, c: u8) -> bool {
        !c.is_ascii() || self.contains(c)
    }
}

impl Default for PercentEncodeSet {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for PercentEncodeSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut format = f.debug_list();

        for (i, chunk) in self.bitset.iter().enumerate() {
            for j in 0..BITS_PER_CHUNK {
                if chunk & 1 << j > 0 {
                    let c = j + i * BITS_PER_CHUNK;
                    // SAFETY: PercentEncodeSet may only contain ASCII digits, which is valid utf-8
                    format.entry(&unsafe { char::from_u32_unchecked(c as u32) });
                }
            }
        }

        format.finish()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn percent_encode_set_example() {
        let s1 = PercentEncodeSet::default();
        assert!(!s1.contains(b'~'));
        assert!(!s1.should_percent_encode(b'~'));
        assert!(s1.should_percent_encode(0x80));

        let s2 = s1.add(b'~');
        assert!(s2.contains(b'~'));
        assert!(s2.should_percent_encode(b'~'));
        assert!(s2.should_percent_encode(0x80));
    }

    #[test]
    fn percent_encode_set_debug() {
        let s1 = PercentEncodeSet::default();
        pretty_assertions::assert_str_eq!(&format!("{s1:?}"), "[]");

        let s2 = s1.add(0x0).add(b' ').add(b'@').add(b'`');
        pretty_assertions::assert_str_eq!(&format!("{s2:?}"), "['\\0', ' ', '@', '`']");

        pretty_assertions::assert_str_eq!(
            &format!("{PATH:?}"),
            "[\
                '\\0', \
                '\\u{1}', \
                '\\u{2}', \
                '\\u{3}', \
                '\\u{4}', \
                '\\u{5}', \
                '\\u{6}', \
                '\\u{7}', \
                '\\u{8}', \
                '\\t', \
                '\\n', \
                '\\u{b}', \
                '\\u{c}', \
                '\\r', \
                '\\u{e}', \
                '\\u{f}', \
                '\\u{10}', \
                '\\u{11}', \
                '\\u{12}', \
                '\\u{13}', \
                '\\u{14}', \
                '\\u{15}', \
                '\\u{16}', \
                '\\u{17}', \
                '\\u{18}', \
                '\\u{19}', \
                '\\u{1a}', \
                '\\u{1b}', \
                '\\u{1c}', \
                '\\u{1d}', \
                '\\u{1e}', \
                '\\u{1f}', \
                ' ', \
                '\"', \
                '#', \
                '<', \
                '>', \
                '?', \
                '^', \
                '`', \
                '{', \
                '}', \
                '\\u{7f}'\
            ]"
        );
    }
}
