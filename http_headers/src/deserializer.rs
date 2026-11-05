//! Serde integration: a [`serde::Deserializer`] over a raw header section.
//!
//! The deserializer is *data-driven*: the set of headers to extract is learned from the target
//! struct through serde itself (its field list, post-`rename`, arrives in
//! [`deserialize_struct`](serde::de::Deserializer::deserialize_struct)), after which a two-pass
//! scan materializes exactly those headers. This avoids indexing or sorting the entire header
//! section: memory is O(desired fields) regardless of how many headers the wire carries.

use crate::error::Error;
use crate::line::iter_lines;
use crate::line::match_name;

/// The maximum number of headers a single target struct may declare.
const MAX_FIELDS: usize = 64;

/// A resolved index over the desired headers found in a header section.
struct Index<'de> {
    /// The number of desired fields (the length of the serde field list).
    k: usize,

    /// Whether field `i` was present in the header section at least once.
    found: [bool; MAX_FIELDS],

    /// The resolved value bytes for each field. Single occurrences borrow directly from the
    /// input (zero-copy); repeated occurrences point at their joined form in the backing buffer.
    value: [&'de [u8]; MAX_FIELDS],
}

/// Scans a raw header section twice, materializing the joined values of the desired headers into
/// the backing buffer.
///
/// Pass one counts occurrences and measures each field's joined size; pass two copies the values
/// (separated by `", "`) into prefix-summed regions of the backing buffer. A field that occurs
/// exactly once is served directly from the input bytes instead.
///
/// # Errors
///
/// Returns [`Error::TooManyFields`] if more than [`MAX_FIELDS`] headers are desired,
/// [`Error::OOM`] if the joined values do not fit into the backing buffer, or a line-level error
/// ([`Error::Eof`], [`Error::ExpectedColon`], [`Error::InvalidToken`], [`Error::ObsFold`]) on
/// malformed input.
fn scan<'de>(
    input: &'de [u8],
    desired: &[&str],
    backing: &'de mut [u8],
) -> Result<Index<'de>, Error> {
    if desired.len() > MAX_FIELDS {
        return Err(Error::TooManyFields);
    }

    let k = desired.len();
    let mut count = [0u32; MAX_FIELDS];
    let mut len = [0usize; MAX_FIELDS];

    // Pass 1: count occurrences and measure joined sizes.
    for entry in iter_lines(input) {
        let line = entry?;
        if let Some(i) = match_name(line.name, desired) {
            count[i] += 1;
            // `", "` separator between occurrences.
            len[i] += line.value.len() + usize::from(count[i] > 1) * 2;
        }
    }

    // Layout: prefix sums over the desired fields.
    let mut offset = [0usize; MAX_FIELDS];
    let mut total = 0usize;
    for i in 0..k {
        offset[i] = total;
        total += len[i];
    }
    if total > backing.len() {
        return Err(Error::OOM);
    }

    // Pass 2: materialize the joined values, remembering the first occurrence's input range for
    // the zero-copy fast path.
    let mut pos = [0usize; MAX_FIELDS];
    let mut written = [false; MAX_FIELDS];
    let mut first_off = [0usize; MAX_FIELDS];
    let mut first_len = [0usize; MAX_FIELDS];
    pos[..k].copy_from_slice(&offset[..k]);

    for entry in iter_lines(input) {
        let line = entry?;
        if let Some(i) = match_name(line.name, desired) {
            if written[i] {
                backing[pos[i]..pos[i] + 2].copy_from_slice(b", ");
                pos[i] += 2;
            } else {
                first_off[i] = line.value.as_ptr() as usize - input.as_ptr() as usize;
                first_len[i] = line.value.len();
            }

            backing[pos[i]..pos[i] + line.value.len()].copy_from_slice(line.value);
            pos[i] += line.value.len();
            written[i] = true;
        }
    }

    // All writes are done: take the shared `'de` view of the backing buffer used to resolve
    // joined values.
    let backing_view: &'de [u8] = backing;

    let mut index = Index {
        k,
        found: [false; MAX_FIELDS],
        value: [&[]; MAX_FIELDS],
    };
    for i in 0..k {
        if count[i] > 0 {
            index.found[i] = true;
            if count[i] == 1 {
                // Single occurrence: serve directly from the input bytes (zero-copy).
                index.value[i] = &input[first_off[i]..first_off[i] + first_len[i]];
            } else {
                index.value[i] = &backing_view[offset[i]..offset[i] + len[i]];
            }
        }
    }

    Ok(index)
}

/// A [`serde::Deserializer`] over a raw header section and its backing buffer.
///
/// This type is consumed by value during deserialization; see [`crate::from_bytes`].
pub(super) struct Deserializer<'de> {
    pub(super) input: &'de [u8],
    pub(super) backing: &'de mut [u8],
}

impl<'de> serde::de::Deserializer<'de> for Deserializer<'de> {
    type Error = Error;

    /// The entry point for struct targets.
    ///
    /// Serde passes the target struct's field list here, which doubles as the set of desired
    /// header names (post-`rename`). The two-pass scan runs before any field is visited, so
    /// repeated headers are combined and unknown headers are skipped by construction.
    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let Deserializer { input, backing } = self;
        let index = scan(input, fields, backing)?;
        visitor.visit_map(MapAccess {
            index,
            fields,
            next: 0,
        })
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::Unsupported)
    }

    serde::forward_to_deserialize_any!(
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map enum
        identifier ignored_any
    );
}

/// A [`serde::de::MapAccess`] yielding the found headers in field declaration order.
struct MapAccess<'de> {
    index: Index<'de>,
    fields: &'static [&'static str],
    next: usize,
}

impl<'de> serde::de::MapAccess<'de> for MapAccess<'de> {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        while self.next < self.index.k && !self.index.found[self.next] {
            self.next += 1;
        }

        if self.next >= self.index.k {
            return Ok(None);
        }

        let i = self.next;
        self.next += 1;

        seed.deserialize(FieldKey {
            name: self.fields[i],
        })
        .map(Some)
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        let i = self.next - 1;
        seed.deserialize(ValueDeserializer {
            bytes: self.index.value[i],
        })
    }
}

/// A [`serde::de::Deserializer`] for a single header field name (a map key).
struct FieldKey<'a> {
    name: &'a str,
}

impl<'de> serde::de::Deserializer<'de> for FieldKey<'_> {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::Unsupported)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_str(self.name)
    }

    serde::forward_to_deserialize_any!(
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        enum ignored_any
    );
}

/// A [`serde::de::Deserializer`] for a single (already joined) header value.
struct ValueDeserializer<'de> {
    bytes: &'de [u8],
}

/// Parses an ASCII decimal integer, rejecting empty input and overflow.
fn parse_u64(bytes: &[u8]) -> Result<u64, Error> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(Error::InvalidValue);
    }

    let mut value = 0u64;
    for &c in bytes {
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(u64::from(c - b'0')))
            .ok_or(Error::InvalidValue)?;
    }

    Ok(value)
}

impl<'de> serde::de::Deserializer<'de> for ValueDeserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        Err(Error::Unsupported)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let s = std::str::from_utf8(self.bytes).map_err(|_| Error::InvalidUtf8)?;
        visitor.visit_borrowed_str(s)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(self.bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_u64(parse_u64(self.bytes)?)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let negative = self.bytes.first() == Some(&b'-');
        let digits = if negative {
            &self.bytes[1..]
        } else {
            self.bytes
        };
        let magnitude = parse_u64(digits)?;

        // The only i64 whose magnitude does not fit in a positive i64 is i64::MIN (2^63).
        if magnitude > (i64::MAX as u64) + 1 || (!negative && magnitude > i64::MAX as u64) {
            return Err(Error::InvalidValue);
        }

        let value = match (negative, i64::try_from(magnitude)) {
            // `magnitude` is exactly 2^63: the value is i64::MIN.
            (true, Err(_)) => i64::MIN,
            // Unreachable given the range check above, but kept for exhaustiveness.
            (false, Err(_)) => return Err(Error::InvalidValue),
            (_, Ok(value)) => {
                if negative {
                    -value
                } else {
                    value
                }
            },
        };

        visitor.visit_i64(value)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match self.bytes {
            b"true" => visitor.visit_bool(true),
            b"false" => visitor.visit_bool(false),
            _ => Err(Error::InvalidValue),
        }
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        if self.bytes.is_empty() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    serde::forward_to_deserialize_any!(
        i8 i16 i32 u8 u16 u32 f32 f64 char unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    );
}
