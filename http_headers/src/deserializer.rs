//! Serde deserialization layer for header field values.
//!
//! This module provides a [`serde::Deserializer`] implementation which maps header field values
//! onto Serde's data model on top of the zero-copy parser in [`crate::line`].

use serde::de;

use super::line::HeaderValues;
use super::line::Headers;
use super::line::Value;

/// Error type for the serde deserialization layer.
///
/// This is deliberately separate from [`crate::error::Error`] so that consumers of the low-level
/// parser in [`crate::line`] never have to handle deserialization-specific variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// A parser error surfaced while driving the header value iterator.
    Parser(crate::error::Error),

    /// A failure specific to the deserialization process itself.
    ErrorParsing(ErrorParsing),
}

/// Deserialization-specific failures which are not raised by the low-level parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorParsing {
    /// A params-only field value was encountered where an element is required.
    MissingElement,

    /// An element did not normalize-match the expected name of a struct or variant.
    NameMismatch {
        /// The name expected by the target type (owned: serde hands the name out as a plain
        /// `&str`, so it cannot be stored borrowed).
        expected: String,
        /// The element bytes as found on the wire.
        found: Vec<u8>,
    },

    /// A scope which must be a string was not valid UTF-8.
    InvalidUtf8,

    /// The value shape cannot satisfy the requested deserialization hint.
    UnsupportedShape(&'static str),

    /// The requested deserialization hint is not supported (bool and floats in v1).
    UnsupportedHint(&'static str),

    /// A numeric hint was given a non-numeric element.
    InvalidInteger {
        /// The element bytes as found on the wire.
        found: Vec<u8>,
    },

    /// A failure reported by Serde itself (unknown variant, invalid value, ...).
    Custom(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parser(err) => err.fmt(f),
            Self::ErrorParsing(err) => err.fmt(f),
        }
    }
}

impl std::fmt::Display for ErrorParsing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingElement => write!(f, "Header field value is missing an element"),
            Self::NameMismatch { expected, found } => write!(
                f,
                "Header field element '{}' does not match expected name '{}'",
                String::from_utf8_lossy(found),
                expected
            ),
            Self::InvalidUtf8 => write!(f, "Header field value is not valid UTF-8"),
            Self::UnsupportedShape(reason) => {
                write!(f, "Unsupported header field value shape: {reason}")
            },
            Self::UnsupportedHint(hint) => {
                write!(
                    f,
                    "Deserializing a header field value as '{hint}' is not supported"
                )
            },
            Self::InvalidInteger { found } => write!(
                f,
                "Invalid integer in header field value: '{}'",
                String::from_utf8_lossy(found)
            ),
            Self::Custom(msg) => f.write_str(msg),
        }
    }
}

impl std::error::Error for Error {}
impl std::error::Error for ErrorParsing {}

impl de::Error for Error {
    fn custom<T: std::fmt::Display>(msg: T) -> Self {
        Self::ErrorParsing(ErrorParsing::Custom(msg.to_string()))
    }
}

impl From<crate::error::Error> for Error {
    fn from(err: crate::error::Error) -> Self {
        Self::Parser(err)
    }
}

impl From<crate::error::ErrorValue> for Error {
    fn from(err: crate::error::ErrorValue) -> Self {
        Self::Parser(crate::error::Error::from(err))
    }
}

impl From<ErrorParsing> for Error {
    fn from(err: ErrorParsing) -> Self {
        Self::ErrorParsing(err)
    }
}

/// The shape of a field value at the current position, as determined by the two-call lookahead
/// pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shape {
    /// No values at all (`Name:` / `Name: ;`) — deserializes to `Option::None` or unit.
    Empty,

    /// A single value: nothing followed the head (a bare element or a lone parameter).
    Single,

    /// A list: at least one value followed the head; items group by element-as-boundary during
    /// the drain.
    List,
}

/// A consumed bare-element scope: either a borrowed view of a single element or an owned
/// `", "`-rejoin of a list of elements (the one documented allocation).
#[derive(Debug)]
enum BareScope<'a> {
    /// A single bare element, borrowed from the backing buffer.
    Borrowed(&'a [u8]),

    /// A list of bare elements, rejoined with `", "`.
    Rejoined(Vec<u8>),
}

/// Serde deserializer for a single header field value.
///
/// Wraps the zero-copy [`HeaderValues`] iterator together with at most two buffered values (the
/// head and discriminator pulled by shape classification) and an item-scope flag. Consumption is
/// strictly forward-only: once a value has been yielded, its buffer region is frozen and must
/// never be mutated again.
#[derive(Debug)]
pub(crate) struct Deserializer<'a> {
    /// The underlying field-value iterator driving the parser in [`super::line`].
    values: HeaderValues<'a>,

    /// The head value pulled by shape classification, if any. Served before [`Self::lookahead`].
    head: Option<Value<'a>>,

    /// The discriminator (and first continuation) value pulled by shape classification, if any.
    lookahead: Option<Value<'a>>,

    /// Whether the current position is inside a list item (set by the sequence access).
    in_item_scope: bool,

    /// The expected-name list for identifier matching, if any. Set by `deserialize_struct` /
    /// `deserialize_enum`, consumed by `deserialize_identifier`: a normalized match against one
    /// of these names is fed back as that exact name; otherwise the raw wire bytes pass through
    /// (rename/alias fallback).
    pending_names: Option<&'static [&'static str]>,
}

impl<'a> Deserializer<'a> {
    /// Initializes a new deserializer over the field values of a parsed header line.
    #[must_use]
    pub(crate) fn from_header_values(values: HeaderValues<'a>) -> Self {
        Self {
            values,
            head: None,
            lookahead: None,
            in_item_scope: false,
            pending_names: None,
        }
    }

    /// Pulls the next buffered value in pull order (head, then lookahead) or, if both are empty,
    /// the next value from the underlying iterator.
    fn next_value(&mut self) -> Result<Option<Value<'a>>, Error> {
        if let Some(value) = self.head.take() {
            return Ok(Some(value));
        }

        if let Some(value) = self.lookahead.take() {
            return Ok(Some(value));
        }

        Ok(self.values.next().transpose()?)
    }

    /// Like [`Self::next_value`], except that while inside an item scope an `Element` is treated
    /// as the start of the next list item: it is buffered back (in [`Self::head`]) and `None` is
    /// returned instead, ending the current item's drain.
    fn next_value_in_scope(&mut self) -> Result<Option<Value<'a>>, Error> {
        match self.next_value()? {
            Some(value) if self.in_item_scope && starts_new_item(&value) => {
                // An element inside an item scope is the next item's head: buffer it back and
                // end the current item's drain.
                self.head = Some(value);
                Ok(None)
            },
            other => Ok(other),
        }
    }

    /// Classifies the shape of the field value at the current position via the two-call pattern:
    /// the first pulled value is the head; the second discriminates single from list. Both are
    /// buffered and served by [`Self::next_value`] in pull order.
    ///
    /// Whether the head is an `Element` or a `Parameter` distinguishes element-bearing shapes
    /// from params-only ones; callers read that directly off the buffered head. Must be called
    /// at a fresh position, i.e. when no values are buffered yet.
    fn classify_shape(&mut self) -> Result<Shape, Error> {
        let Some(head) = self.next_value()? else {
            return Ok(Shape::Empty);
        };

        match self.next_value()? {
            None => {
                self.head = Some(head);
                Ok(Shape::Single)
            },
            Some(discriminator) => {
                self.head = Some(head);
                self.lookahead = Some(discriminator);
                Ok(Shape::List)
            },
        }
    }

    /// Consumes a bare-element scope: a single bare element yields a borrowed view of it; a list
    /// of bare elements yields an owned `", "`-rejoin (the one documented allocation). Any
    /// parameter in scope is a hard error, as is an empty field value.
    fn bare_element_scope(&mut self) -> Result<BareScope<'a>, Error> {
        let shape = self.classify_shape()?;

        if shape == Shape::Empty {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found an empty field value",
            )));
        }

        // Consume the head buffered by the classification. A `Parameter` head means a params-only
        // scope, which cannot satisfy a bare-element hint.
        let Some(Value::Element(head)) = self.next_value()? else {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found parameters",
            )));
        };

        if shape == Shape::Single {
            return Ok(BareScope::Borrowed(shared_bytes(head)));
        }

        // The shape is `List`: drain the remaining values, rejecting any parameter. An item-scope
        // drain ends at the next element boundary (buffered back by `next_value_in_scope`); when
        // no continuation was consumed within this item, the single bare element stays borrowed.
        let head_bytes = shared_bytes(head);
        let mut rejoined: Vec<u8> = Vec::new();

        while let Some(value) = self.next_value_in_scope()? {
            match value {
                Value::Element(element) => {
                    if rejoined.is_empty() {
                        rejoined.extend_from_slice(head_bytes);
                    }
                    rejoined.extend_from_slice(b", ");
                    rejoined.extend_from_slice(shared_bytes(element));
                },
                Value::Parameter { .. } => {
                    return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                        "expected bare elements, found parameters",
                    )));
                },
            }
        }

        if rejoined.is_empty() {
            Ok(BareScope::Borrowed(head_bytes))
        } else {
            Ok(BareScope::Rejoined(rejoined))
        }
    }

    /// Emits a consumed bare-element scope to a string visitor: a single bare element is borrowed
    /// from the buffer (zero-copy); a list of bare elements is rejoined with `", "` and handed
    /// over as an owned string.
    fn visit_bare_element_as_string<V>(&mut self, visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'a>,
    {
        match self.bare_element_scope()? {
            BareScope::Borrowed(bytes) => {
                let s = std::str::from_utf8(bytes)
                    .map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))?;
                visitor.visit_borrowed_str(s)
            },
            // The rejoined string is owned, so it is handed over through `visit_str` (the
            // no-alloc-compatible path; the visitor copies it).
            BareScope::Rejoined(bytes) => {
                let s = String::from_utf8(bytes)
                    .map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))?;
                visitor.visit_str(&s)
            },
        }
    }

    /// Emits a consumed bare-element scope to a bytes visitor. See
    /// [`Self::visit_bare_element_as_string`] for the borrowing rules.
    fn visit_bare_element_as_bytes<V>(&mut self, visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'a>,
    {
        match self.bare_element_scope()? {
            BareScope::Borrowed(bytes) => visitor.visit_borrowed_bytes(bytes),
            // The rejoined buffer is owned, so it is handed over through `visit_bytes` (the
            // no-alloc-compatible path; the visitor copies it).
            BareScope::Rejoined(bytes) => visitor.visit_bytes(&bytes),
        }
    }

    /// Consumes the buffered head element and emits it to a visitor as a borrowed string.
    fn visit_head_as_string<V>(&mut self, visitor: V) -> Result<V::Value, Error>
    where
        V: de::Visitor<'a>,
    {
        let Some(Value::Element(head)) = self.next_value()? else {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element",
            )));
        };

        visitor.visit_borrowed_str(shared_str(head)?)
    }
}

macro_rules! unsupported_hint {
    ($hint:literal) => {
        Err(Error::ErrorParsing(ErrorParsing::UnsupportedHint($hint)))
    };
}

/// Strictly parses element bytes as an integer, returned wide enough (`i128`) to hold any 64-bit
/// target. Only ASCII digits are accepted, with an optional leading `-` for signed values — no
/// `+`, whitespace, or other sign forms. The digit-count guard keeps the accumulation overflow-free.
fn parse_integer(bytes: &[u8], signed: bool) -> Result<i128, Error> {
    let (negative, digits) = match (signed, bytes.first()) {
        (true, Some(b'-')) => (true, &bytes[1..]),
        _ => (false, bytes),
    };

    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) || digits.len() > 19 {
        return Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
            found: bytes.to_vec(),
        }));
    }

    let mut magnitude: i128 = 0;
    for c in digits {
        magnitude = magnitude * 10 + i128::from(*c - b'0');
    }

    Ok(if negative { -magnitude } else { magnitude })
}

macro_rules! integer_hint {
    ($method:ident, $ty:ty, $visit:ident, $signed:literal) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: de::Visitor<'de>,
        {
            match self.bare_element_scope()? {
                BareScope::Borrowed(bytes) => {
                    let value = parse_integer(bytes, $signed)?;
                    let value = <$ty>::try_from(value).map_err(|_| {
                        Error::ErrorParsing(ErrorParsing::InvalidInteger {
                            found: bytes.to_vec(),
                        })
                    })?;
                    visitor.$visit(value)
                },
                BareScope::Rejoined(_) => Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                    "expected a single bare element",
                ))),
            }
        }
    };
}

/// The [`integer_hint!`] counterpart for [`SliceDeserializer`]: the slice is already a single
/// value, so it is parsed directly.
macro_rules! slice_integer_hint {
    ($method:ident, $ty:ty, $visit:ident, $signed:literal) => {
        fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
        where
            V: de::Visitor<'de>,
        {
            let value = parse_integer(self.bytes, $signed)?;
            let value = <$ty>::try_from(value).map_err(|_| {
                Error::ErrorParsing(ErrorParsing::InvalidInteger {
                    found: self.bytes.to_vec(),
                })
            })?;

            visitor.$visit(value)
        }
    };
}

/// Sequence access over the deserializer's item stream: each element seed deserializes one list
/// item, with the item-scope flag set for the duration of the seed call so that item-scoped
/// drains stop at the next element boundary.
struct SeqAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,

    /// The expected element count for tuple-family hints; `None` for plain sequences.
    len: Option<usize>,

    /// How many elements have been served so far.
    seen: usize,
}

impl<'de> de::SeqAccess<'de> for SeqAccess<'_, 'de> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        if self.len.is_some_and(|len| self.seen >= len) {
            return Ok(None);
        }

        // An item exists iff a value is buffered: every item-scoped drain terminates either at
        // end of stream or by buffering the next item's head element back.
        if self.de.head.is_none() && self.de.lookahead.is_none() {
            return Ok(None);
        }

        self.seen += 1;
        self.de.in_item_scope = true;
        let value = seed.deserialize(&mut *self.de)?;
        self.de.in_item_scope = false;

        Ok(Some(value))
    }
}

/// A deserializer over a single borrowed byte slice, emitting string/bytes data and identifiers.
/// Used by [`MapAccess`] to serve the key and value halves of a parameter pair.
struct SliceDeserializer<'de> {
    bytes: &'de [u8],

    /// The expected-name list for identifier matching, if any; `None` serves raw bytes.
    names: Option<&'static [&'static str]>,
}

impl<'de> de::Deserializer<'de> for SliceDeserializer<'de> {
    type Error = Error;

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let s = std::str::from_utf8(self.bytes)
            .map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))?;
        visitor.visit_borrowed_str(s)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(self.bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("any")
    }

    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("bool")
    }

    slice_integer_hint!(deserialize_i8, i8, visit_i8, true);

    slice_integer_hint!(deserialize_i16, i16, visit_i16, true);

    slice_integer_hint!(deserialize_i32, i32, visit_i32, true);

    slice_integer_hint!(deserialize_i64, i64, visit_i64, true);

    fn deserialize_i128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i128")
    }

    slice_integer_hint!(deserialize_u8, u8, visit_u8, false);

    slice_integer_hint!(deserialize_u16, u16, visit_u16, false);

    slice_integer_hint!(deserialize_u32, u32, visit_u32, false);

    slice_integer_hint!(deserialize_u64, u64, visit_u64, false);

    fn deserialize_u128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u128")
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f32")
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f64")
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("char")
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // A parameter value is always present (even when empty); absence is the missing-key case,
        // which serde's map semantics resolve before any value deserialization.
        visitor.visit_some(self)
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(self, _name: &str, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("unit struct")
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("newtype struct")
    }

    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("seq")
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("tuple")
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("tuple struct")
    }

    fn deserialize_map<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("map")
    }

    fn deserialize_struct<V>(
        self,
        _name: &str,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("struct")
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("enum")
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // A normalized match feeds the exact expected name so the target's own identifier
        // visitor recognizes it.
        if let Some(name) = match_identifier(self.bytes, self.names) {
            return visitor.visit_borrowed_str(name);
        }

        // No list, or no match: feed the raw wire bytes (rename/alias fallback).
        let s = std::str::from_utf8(self.bytes)
            .map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))?;

        visitor.visit_borrowed_str(s)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // The slice is a single value; there is nothing to advance past.
        visitor.visit_unit()
    }
}

/// Map access over the deserializer's parameter stream: each pair is one `Parameter` value split
/// into its key and value halves, served to seeds through [`SliceDeserializer`]. An `Element`
/// encountered mid-drain ends the map in item scope (next-item boundary) or is a hard error at
/// value position.
struct MapAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,

    /// The value half of the parameter whose key half was just served; `None` before a key pull.
    pending_value: Option<&'de [u8]>,
}

impl<'de> de::MapAccess<'de> for MapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        debug_assert!(self.pending_value.is_none());

        match self.de.next_value_in_scope()? {
            Some(Value::Parameter { key, val }) => {
                // Coerce both halves to shared views (forward-only) and hold the value half until
                // its seed is served.
                let key_bytes = shared_bytes(key);
                let val_bytes = shared_bytes(val);
                self.pending_value = Some(val_bytes);

                // Keys are identifier-matched against the pending expected-name list (struct and
                // enum-variant fields); with no list set they pass through raw (plain maps).
                let pair_key = seed.deserialize(SliceDeserializer {
                    bytes: key_bytes,
                    names: self.de.pending_names,
                })?;
                Ok(Some(pair_key))
            },
            // An element mid-drain: a hard error at value position (mixed list), or the end of
            // this item in item scope (already buffered back by `next_value_in_scope`).
            Some(Value::Element(..)) => Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected parameters, found an element",
            ))),
            None => Ok(None),
        }
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(val) = self.pending_value.take() else {
            // Unreachable: serde alternates key and value seeds strictly.
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "parameter value requested before its key",
            )));
        };

        seed.deserialize(SliceDeserializer {
            bytes: val,
            names: None,
        })
    }
}

/// Enum access for externally-tagged enums: each list item is one variant occurrence — the head
/// element names the variant (identifier-matched against the pending variant-name list) and the
/// item's parameters, if any, are the variant's payload.
struct EnumAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'a, 'de> de::EnumAccess<'de> for EnumAccess<'a, 'de> {
    type Error = Error;
    type Variant = EnumVariantAccess<'a, 'de>;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        // The head element names the variant; a params-only item has no name.
        let Some(head) = self.de.next_value()? else {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value",
            )));
        };

        let Value::Element(element) = head else {
            return Err(Error::ErrorParsing(ErrorParsing::MissingElement));
        };

        let wire = shared_bytes(element);
        let variant = seed.deserialize(SliceDeserializer {
            bytes: wire,
            names: self.de.pending_names,
        })?;

        Ok((variant, EnumVariantAccess { de: self.de }))
    }
}

/// Variant access for externally-tagged enum variants: a unit variant is a bare element (nothing
/// more); newtype and struct variants stream the item's parameters as a map.
struct EnumVariantAccess<'a, 'de> {
    de: &'a mut Deserializer<'de>,
}

impl<'de> de::VariantAccess<'de> for EnumVariantAccess<'_, 'de> {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        // A unit variant is a bare element: nothing may follow within the item. Item scope makes
        // the pull stop at the next item boundary (buffered back for the following seed).
        let prev_scope = self.de.in_item_scope;
        self.de.in_item_scope = true;

        let result = match self.de.next_value_in_scope()? {
            Some(Value::Parameter { .. }) => Err(Error::ErrorParsing(
                ErrorParsing::UnsupportedShape("expected a bare element, found parameters"),
            )),
            _ => Ok(()),
        };

        self.de.in_item_scope = prev_scope;
        result
    }

    fn newtype_variant_seed<T>(self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        // The payload is the item's parameters: forward to the main deserializer, which now sees
        // a params-only scope (the element was consumed as the variant name) and skips the name
        // check.
        seed.deserialize(&mut *self.de)
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("tuple variant")
    }

    fn struct_variant<V>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // Stream the item's parameters as a map with keys matched against the variant's fields.
        // Both slots are saved and restored so following items keep matching variants.
        let prev_names = self.de.pending_names;
        let prev_scope = self.de.in_item_scope;
        self.de.pending_names = Some(fields);
        self.de.in_item_scope = true;

        let result = visitor.visit_map(MapAccess {
            de: &mut *self.de,
            pending_value: None,
        });

        self.de.in_item_scope = prev_scope;
        self.de.pending_names = prev_names;
        result
    }
}

impl<'de> de::Deserializer<'de> for &mut Deserializer<'de> {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.classify_shape()?;

        if self.head.is_none() {
            // An empty field value is the unit value.
            return visitor.visit_unit();
        }

        match (&self.head, &self.lookahead) {
            // A single bare element: a borrowed string.
            (Some(Value::Element(..)), None) => self.visit_head_as_string(visitor),

            // An element carrying parameters: no data model can express a name plus fields under
            // `any`.
            (Some(Value::Element(..)), Some(Value::Parameter { .. })) => Err(Error::ErrorParsing(
                ErrorParsing::UnsupportedShape("element with parameters"),
            )),

            // A list of bare elements at value position: a sequence re-dispatching each item
            // through `any`. Inside an item scope the second element is the next item's head
            // (boundary), so the item itself is a single bare element.
            (Some(Value::Element(..)), Some(Value::Element(..))) => {
                if self.in_item_scope {
                    self.visit_head_as_string(visitor)
                } else {
                    visitor.visit_seq(SeqAccess {
                        de: self,
                        len: None,
                        seen: 0,
                    })
                }
            },

            // A params-only scope (single or list): a map; list boundaries flatten.
            (Some(Value::Parameter { .. }), _) => visitor.visit_map(MapAccess {
                de: self,
                pending_value: None,
            }),

            // An `Empty` classification cannot leave a head behind.
            (None, _) => Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "unexpected field value shape",
            ))),
        }
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.visit_bare_element_as_string(visitor)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.visit_bare_element_as_string(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.visit_bare_element_as_bytes(visitor)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.visit_bare_element_as_bytes(visitor)
    }

    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("bool")
    }

    integer_hint!(deserialize_i8, i8, visit_i8, true);

    integer_hint!(deserialize_i16, i16, visit_i16, true);

    integer_hint!(deserialize_i32, i32, visit_i32, true);

    integer_hint!(deserialize_i64, i64, visit_i64, true);

    fn deserialize_i128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i128")
    }

    integer_hint!(deserialize_u8, u8, visit_u8, false);

    integer_hint!(deserialize_u16, u16, visit_u16, false);

    integer_hint!(deserialize_u32, u32, visit_u32, false);

    integer_hint!(deserialize_u64, u64, visit_u64, false);

    fn deserialize_u128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u128")
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f32")
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f64")
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("char")
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // The classification only buffers values (it never consumes them), so handing `self` to
        // the inner deserialization re-serves the same stream from the same position.
        match self.classify_shape()? {
            Shape::Empty => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("unit")
    }

    fn deserialize_unit_struct<V>(self, _name: &str, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("unit struct")
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("newtype struct")
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // The classification buffers the head and discriminator; items are then served one per
        // element seed by the sequence access.
        self.classify_shape()?;
        visitor.visit_seq(SeqAccess {
            de: self,
            len: None,
            seen: 0,
        })
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.classify_shape()?;
        visitor.visit_seq(SeqAccess {
            de: self,
            len: Some(len),
            seen: 0,
        })
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        self.classify_shape()?;
        visitor.visit_seq(SeqAccess {
            de: self,
            len: Some(len),
            seen: 0,
        })
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let shape = self.classify_shape()?;

        // An empty field value is an empty map.
        if shape == Shape::Empty {
            return visitor.visit_map(MapAccess {
                de: self,
                pending_value: None,
            });
        }

        // Only a params-only scope (a `Parameter` head) can satisfy a map hint; an element head —
        // bare or parameterized — carries a name, not key-value data. The head stays buffered for
        // the map access to pull as its first pair.
        if !matches!(self.head, Some(Value::Parameter { .. })) {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected parameters, found an element",
            )));
        }

        visitor.visit_map(MapAccess {
            de: self,
            pending_value: None,
        })
    }

    fn deserialize_struct<V>(
        self,
        name: &str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let shape = self.classify_shape()?;

        if shape == Shape::Empty {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value",
            )));
        }

        // The head names the item: validate it against the struct name. A params-only item
        // carries no name and skips the check.
        let Some(head) = self.next_value()? else {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value",
            )));
        };

        match head {
            Value::Element(element) => {
                let wire = shared_bytes(element);

                if normalize(wire) != normalize(name.as_bytes()) {
                    return Err(Error::ErrorParsing(ErrorParsing::NameMismatch {
                        expected: name.to_owned(),
                        found: wire.to_vec(),
                    }));
                }
            },
            // A params-only item carries no name: buffer the head back so the map serves it.
            Value::Parameter { .. } => self.head = Some(head),
        }

        // Stream the item's parameters as a map, with keys identifier-matched against the
        // fields. Item scope makes the drain stop at the next item boundary (buffered back);
        // both slots are saved and restored so nested uses (e.g. an enum variant payload) keep
        // their own expectations.
        let prev_names = self.pending_names;
        let prev_scope = self.in_item_scope;
        self.pending_names = Some(fields);
        self.in_item_scope = true;

        let result = visitor.visit_map(MapAccess {
            de: &mut *self,
            pending_value: None,
        });

        self.in_item_scope = prev_scope;
        self.pending_names = prev_names;

        // A struct is a single item: at value position nothing may follow the item. Inside a
        // sequence item the trailing boundary element belongs to the next seed.
        if !prev_scope && (self.head.is_some() || self.lookahead.is_some()) {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a single item",
            )));
        }

        result
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let shape = self.classify_shape()?;

        if shape == Shape::Empty {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value",
            )));
        }

        // Each item re-matches a variant: the variant-name list stays pending for the whole
        // enum and is restored afterwards.
        let prev_names = self.pending_names;
        self.pending_names = Some(variants);

        let result = visitor.visit_enum(EnumAccess { de: &mut *self });

        self.pending_names = prev_names;

        // An enum value is a single item: at value position nothing may follow. Inside a
        // sequence item the trailing boundary element belongs to the next seed.
        if !self.in_item_scope && (self.head.is_some() || self.lookahead.is_some()) {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a single item",
            )));
        }

        result
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // The identifier is the current value: an element's bytes or a parameter's key half.
        let Some(value) = self.next_value()? else {
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an identifier",
            )));
        };

        let wire = match value {
            Value::Element(element) => shared_bytes(element),
            Value::Parameter { key, .. } => shared_bytes(key),
        };

        SliceDeserializer {
            bytes: wire,
            names: self.pending_names,
        }
        .deserialize_identifier(visitor)
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        // Consume the head value first so that in item scope exactly one list item is skipped:
        // the head is this item's own element (or its first parameter), and the subsequent drain
        // stops at the next item boundary, which `next_value_in_scope` buffers back for the
        // following seed. At value position the drain consumes the whole remaining stream. The
        // value carried nothing of interest, so the visitor is completed as unit.
        if self.next_value()?.is_none() {
            return visitor.visit_unit();
        }

        while self.next_value_in_scope()?.is_some() {}

        visitor.visit_unit()
    }
}

/// Element-as-boundary rule: an `Element` starts a new list item; a `Parameter` continues the
/// current item, whose parameters belong to it until the next `Element` or end of stream.
fn starts_new_item(value: &Value<'_>) -> bool {
    matches!(value, Value::Element(..))
}

/// Coerces an owned element of the backing buffer into a shared view.
///
/// Forward-only invariant: taking the mutable slice by value is what enforces this — once a
/// region has been coerced and yielded, no mutable handle to it remains, so it can never be
/// mutated again for the lifetime of the buffer.
fn shared_bytes(element: &mut [u8]) -> &[u8] {
    element
}

/// Coerces an owned element of the backing buffer into a shared string view, validating UTF-8.
///
/// See [`shared_bytes`] for the forward-only invariant this coercion relies on.
fn shared_str(element: &mut [u8]) -> Result<&str, Error> {
    std::str::from_utf8(element).map_err(|_| Error::ErrorParsing(ErrorParsing::InvalidUtf8))
}

/// Normalizes an identifier for case- and separator-insensitive matching: strips every
/// non-alphanumeric byte and lowercases the remaining ASCII letters. This is what lets a wire
/// form like `text/plain` or `Max-Age` bridge to a Rust-side name like `TextPlain` or `max_age`.
fn normalize(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());

    for &c in bytes {
        if c.is_ascii_alphanumeric() {
            normalized.push(c.to_ascii_lowercase());
        }
    }

    normalized
}

/// Matches identifier wire bytes against an expected-name list: the first name whose normalized
/// form equals the wire's wins and is returned; with no list or no match, `None` signals the raw
/// wire-bytes fallback (rename/alias). Matching is byte-level, so non-UTF-8 wire bytes can still
/// match — only the fallback path requires valid UTF-8.
fn match_identifier(wire: &[u8], names: Option<&'static [&'static str]>) -> Option<&'static str> {
    let expected = names?;

    let needle = normalize(wire);

    expected
        .iter()
        .copied()
        .find(|name| normalize(name.as_bytes()) == needle)
}

/// Serde deserializer for a whole header section: presents the run of lines as a map from
/// lower-cased field name to field value, with each value served by a fresh value-level
/// [`Deserializer`] over that line.
pub(crate) struct SectionDeserializer<'a> {
    /// The underlying line iterator driving the parser in [`super::line`].
    headers: Headers<'a>,
}

impl<'a> SectionDeserializer<'a> {
    /// Initializes a new section deserializer over a parsed header section.
    #[must_use]
    pub(crate) fn from_headers(headers: Headers<'a>) -> Self {
        Self { headers }
    }
}

/// Map access over a header section: each pair is one line — the lower-cased field name as key,
/// the line's values (served by a fresh value-level deserializer) as value. Holding a line's
/// values while iterating is sound: every line is a disjoint slice of the input buffer.
struct SectionMapAccess<'b, 'de> {
    headers: &'b mut Headers<'de>,

    /// The expected field-name list for identifier matching (struct targets); `None` serves raw
    /// names (plain map targets).
    names: Option<&'static [&'static str]>,

    /// The current line's values, pending their value seed.
    pending_values: Option<HeaderValues<'de>>,
}

impl<'de> de::MapAccess<'de> for SectionMapAccess<'_, 'de> {
    type Error = Error;

    fn next_key_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        debug_assert!(self.pending_values.is_none());

        let Some(line_result) = self.headers.next() else {
            return Ok(None);
        };

        let line = line_result?;

        let name = shared_bytes(line.name);
        self.pending_values = Some(line.values);

        seed.deserialize(SliceDeserializer {
            bytes: name,
            names: self.names,
        })
        .map(Some)
    }

    fn next_value_seed<T>(&mut self, seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        let Some(values) = self.pending_values.take() else {
            // Unreachable: serde alternates key and value seeds strictly.
            return Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "header value requested before its name",
            )));
        };

        seed.deserialize(&mut Deserializer::from_header_values(values))
    }
}

impl<'de> de::Deserializer<'de> for &mut SectionDeserializer<'de> {
    type Error = Error;

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(SectionMapAccess {
            headers: &mut self.headers,
            names: None,
            pending_values: None,
        })
    }

    fn deserialize_struct<V>(
        self,
        _name: &str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        visitor.visit_map(SectionMapAccess {
            headers: &mut self.headers,
            names: Some(fields),
            pending_values: None,
        })
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("any")
    }

    fn deserialize_bool<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("bool")
    }

    fn deserialize_i8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i8")
    }

    fn deserialize_i16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i16")
    }

    fn deserialize_i32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i32")
    }

    fn deserialize_i64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i64")
    }

    fn deserialize_i128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("i128")
    }

    fn deserialize_u8<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u8")
    }

    fn deserialize_u16<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u16")
    }

    fn deserialize_u32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u32")
    }

    fn deserialize_u64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u64")
    }

    fn deserialize_u128<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("u128")
    }

    fn deserialize_f32<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f32")
    }

    fn deserialize_f64<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("f64")
    }

    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("char")
    }

    fn deserialize_str<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("str")
    }

    fn deserialize_string<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("string")
    }

    fn deserialize_bytes<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("bytes")
    }

    fn deserialize_byte_buf<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("byte buf")
    }

    fn deserialize_option<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("option")
    }

    fn deserialize_unit<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("unit")
    }

    fn deserialize_unit_struct<V>(self, _name: &str, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("unit struct")
    }

    fn deserialize_newtype_struct<V>(
        self,
        _name: &str,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("newtype struct")
    }

    fn deserialize_seq<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("seq")
    }

    fn deserialize_tuple<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("tuple")
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &str,
        _len: usize,
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("tuple struct")
    }

    fn deserialize_enum<V>(
        self,
        _name: &str,
        _variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("enum")
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("identifier")
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("ignored any")
    }
}

#[cfg(test)]
mod test {
    use std::marker::PhantomData;

    use de::MapAccess as _;

    use super::*;
    use crate::element;
    use crate::error;
    use crate::line;
    use crate::parameter;

    #[test]
    fn parser_error_conversion() {
        let err = Error::from(error::Error::MissingFieldDelimiter);

        assert!(matches!(
            err,
            Error::Parser(error::Error::MissingFieldDelimiter)
        ));
    }

    #[test]
    fn error_value_conversion() {
        let err = Error::from(error::ErrorValue::EmptyList);

        assert_eq!(
            err,
            Error::Parser(error::Error::Value(error::ErrorValue::EmptyList))
        );
    }

    #[test]
    fn error_parsing_conversion() {
        let err = Error::from(ErrorParsing::MissingElement);

        assert!(matches!(
            err,
            Error::ErrorParsing(ErrorParsing::MissingElement)
        ));
    }

    #[test]
    fn display_parser_error() {
        let err = Error::from(error::Error::MissingFieldDelimiter);

        assert_eq!(
            err.to_string(),
            "Missing ':' field delimiter in header line"
        );
    }

    #[test]
    fn display_name_mismatch() {
        let err = Error::ErrorParsing(ErrorParsing::NameMismatch {
            expected: "TextPlain".to_owned(),
            found: b"text/html".to_vec(),
        });

        assert_eq!(
            err.to_string(),
            "Header field element 'text/html' does not match expected name 'TextPlain'"
        );
    }

    #[test]
    fn display_invalid_utf8() {
        let err = Error::ErrorParsing(ErrorParsing::InvalidUtf8);

        assert_eq!(err.to_string(), "Header field value is not valid UTF-8");
    }

    #[test]
    fn display_unsupported_shape() {
        let err = Error::ErrorParsing(ErrorParsing::UnsupportedShape(
            "expected single bare element",
        ));

        assert_eq!(
            err.to_string(),
            "Unsupported header field value shape: expected single bare element"
        );
    }

    #[test]
    fn display_unsupported_hint() {
        let err = Error::ErrorParsing(ErrorParsing::UnsupportedHint("bool"));

        assert_eq!(
            err.to_string(),
            "Deserializing a header field value as 'bool' is not supported"
        );
    }

    #[test]
    fn display_invalid_integer() {
        let err = Error::ErrorParsing(ErrorParsing::InvalidInteger {
            found: b"abc".to_vec(),
        });

        assert_eq!(
            err.to_string(),
            "Invalid integer in header field value: 'abc'"
        );
    }

    #[test]
    fn serde_custom_error() {
        let err = <Error as de::Error>::custom("unknown variant `foo`");

        assert!(matches!(err, Error::ErrorParsing(ErrorParsing::Custom(_))));
        assert_eq!(err.to_string(), "unknown variant `foo`");
    }

    #[test]
    fn deserializer_construction() {
        let mut input_a = *b"Content-Type: text/plain";
        let mut input_b = *b"Accept: */*";

        let header_a = line::HeaderLine::new(&mut input_a).unwrap();
        let header_b = line::HeaderLine::new(&mut input_b).unwrap();

        let de_a = Deserializer::from_header_values(header_a.values);
        let de_b = Deserializer::from_header_values(header_b.values);

        assert_ne!(format!("{de_a:?}"), format!("{de_b:?}"));
    }

    #[test]
    fn next_value_pulls_in_order() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        assert_eq!(de.next_value(), Ok(Some(element!(b"b"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn next_value_serves_buffered_in_pull_order() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        // Pull both values out of the iterator...
        let first = de.next_value().unwrap();
        let second = de.next_value().unwrap();
        assert_eq!(first, Some(element!(b"a")));
        assert_eq!(second, Some(element!(b"b")));

        // ...then re-buffer them: head is served before lookahead, lookahead before the iterator.
        de.lookahead = second;
        de.head = first;
        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        assert_eq!(de.next_value(), Ok(Some(element!(b"b"))));

        // The buffered pulls did not advance the iterator: it is exhausted.
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_empty() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.classify_shape(), Ok(Shape::Empty));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.classify_shape(), Ok(Shape::Single));
        assert_eq!(de.next_value(), Ok(Some(element!(b"text/plain"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_single_params_only() {
        let mut input = *b"Example-Params: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.classify_shape(), Ok(Shape::Single));
        assert_eq!(de.next_value(), Ok(Some(parameter!(b"a", b"1"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_list_bare_elements() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.classify_shape(), Ok(Shape::List));
        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        assert_eq!(de.next_value(), Ok(Some(element!(b"b"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_list_element_with_params() {
        let mut input = *b"Example-Mixed: a;b=1, c";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        // Items group by element-as-boundary: `a` + its parameter, then the bare `c`.
        assert_eq!(de.classify_shape(), Ok(Shape::List));
        assert_eq!(de.next_value(), Ok(Some(element!(b"a"))));
        assert_eq!(de.next_value(), Ok(Some(parameter!(b"b", b"1"))));
        assert_eq!(de.next_value(), Ok(Some(element!(b"c"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn classify_shape_list_params_only() {
        let mut input = *b"Example-Params: ;a=1, ;b=2";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        // List boundaries between consecutive params-only items are lost in the value stream.
        assert_eq!(de.classify_shape(), Ok(Shape::List));
        assert_eq!(de.next_value(), Ok(Some(parameter!(b"a", b"1"))));
        assert_eq!(de.next_value(), Ok(Some(parameter!(b"b", b"2"))));
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn starts_new_item_element_as_boundary() {
        assert!(starts_new_item(&element!(b"a")));
        assert!(!starts_new_item(&parameter!(b"k", b"v")));
    }

    #[test]
    fn shared_bytes_borrows_element() {
        let mut input = *b"Content-Length: 3";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert_eq!(shared_bytes(element), b"3".as_slice());
    }

    #[test]
    fn shared_str_borrows_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert_eq!(shared_str(element), Ok("text/plain"));
    }

    #[test]
    fn shared_str_invalid_utf8() {
        // `obs-text` (0x80-FF) is legal per the parser, so non-UTF-8 elements do reach this layer.
        let mut input = *b"Example-X: te\xffxt";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let Some(Value::Element(element)) = de.next_value().unwrap() else {
            panic!("expected a single element");
        };

        assert!(matches!(
            shared_str(element),
            Err(Error::ErrorParsing(ErrorParsing::InvalidUtf8))
        ));
    }

    #[test]
    fn deserialize_str_borrows_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: &str = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, "text/plain");
    }

    #[test]
    fn deserialize_string_borrows_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: String = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, "text/plain");
    }

    #[test]
    fn deserialize_string_rejoins_bare_element_list() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: String = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, "a, b");
    }

    #[test]
    fn deserialize_bytes_borrows_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: &[u8] = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, b"text/plain");
    }

    /// A byte-buffer target which explicitly requests `deserialize_byte_buf`, since serde's own
    /// `Vec<u8>` impl deserializes as a sequence instead.
    #[derive(Debug, PartialEq)]
    struct ByteBuf(Vec<u8>);

    impl<'de> serde::Deserialize<'de> for ByteBuf {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;

            impl serde::de::Visitor<'_> for Visitor {
                type Value = ByteBuf;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a byte buffer")
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(ByteBuf(v.to_vec()))
                }
            }

            deserializer.deserialize_byte_buf(Visitor)
        }
    }

    #[test]
    fn deserialize_byte_buf_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: ByteBuf = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, ByteBuf(b"text/plain".to_vec()));
    }

    #[test]
    fn deserialize_byte_buf_rejoins_bare_element_list() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: ByteBuf = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, ByteBuf(b"a, b".to_vec()));
    }

    #[test]
    fn deserialize_str_rejects_element_with_params() {
        let mut input = *b"Content-Type: text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected bare elements, found parameters"
            )))
        );
    }

    #[test]
    fn deserialize_str_rejects_params_only() {
        let mut input = *b"Example-Params: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found parameters"
            )))
        );
    }

    #[test]
    fn deserialize_str_rejects_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found an empty field value"
            )))
        );
    }

    #[test]
    fn deserialize_str_rejects_invalid_utf8() {
        let mut input = *b"Example-X: te\xffxt";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(result, Err(Error::ErrorParsing(ErrorParsing::InvalidUtf8)));
    }

    #[test]
    fn deserialize_option_some_single_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Option<&str> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Some("text/plain"));
    }

    #[test]
    fn deserialize_option_some_rejoins_bare_element_list() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Option<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Some("a, b".to_owned()));
    }

    #[test]
    fn deserialize_option_none_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Option<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, None);
    }

    #[test]
    fn deserialize_option_none_bare_parameter_delimiter() {
        // A lone leading `;` parses to zero values, just like an empty field value.
        let mut input = *b"Example-X: ;";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Option<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, None);
    }

    #[test]
    fn parse_integer_accepts_digits_and_optional_minus() {
        assert_eq!(parse_integer(b"0", false), Ok(0));
        assert_eq!(parse_integer(b"007", false), Ok(7));
        assert_eq!(parse_integer(b"12345", true), Ok(12345));
        assert_eq!(parse_integer(b"-42", true), Ok(-42));
        assert_eq!(
            parse_integer(b"9223372036854775807", false),
            Ok(i128::from(i64::MAX))
        );
        assert_eq!(
            parse_integer(b"-9223372036854775808", true),
            Ok(i128::from(i64::MIN))
        );
    }

    #[test]
    fn parse_integer_rejects_non_digit_bytes() {
        for (bytes, signed) in [
            (b"" as &[u8], false),
            (b"-" as &[u8], true),
            (b"+5" as &[u8], true),
            (b"1.5" as &[u8], false),
            (b"-1" as &[u8], false),
            (b"a" as &[u8], true),
            (b"92233720368547758078" as &[u8], false),
        ] {
            assert!(
                matches!(
                    parse_integer(bytes, signed),
                    Err(Error::ErrorParsing(ErrorParsing::InvalidInteger { .. }))
                ),
                "expected {bytes:?} (signed: {signed}) to be rejected"
            );
        }
    }

    #[test]
    fn deserialize_i64_single_bare_element() {
        let mut input = *b"Content-Length: 3";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: i64 = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, 3);
    }

    #[test]
    fn deserialize_i8_max() {
        let mut input = *b"Example-Range: 127";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: i8 = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, 127);
    }

    #[test]
    fn deserialize_i8_min() {
        let mut input = *b"Example-Range: -128";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: i8 = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, -128);
    }

    #[test]
    fn deserialize_i8_rejects_out_of_range() {
        let mut input = *b"Example-Range: 300";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<i8, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                found: b"300".to_vec()
            }))
        );
    }

    #[test]
    fn deserialize_u8_rejects_negative() {
        let mut input = *b"Example-Range: -1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<u8, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                found: b"-1".to_vec()
            }))
        );
    }

    #[test]
    fn deserialize_u64_single_bare_element() {
        let mut input = *b"Example-Range: 255";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: u64 = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, 255);
    }

    #[test]
    fn deserialize_i64_rejects_list() {
        let mut input = *b"Example-List: 1, 2";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<i64, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a single bare element"
            )))
        );
    }

    #[test]
    fn deserialize_i64_rejects_params_only() {
        let mut input = *b"Example-Params: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<i64, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found parameters"
            )))
        );
    }

    #[test]
    fn deserialize_seq_list_of_bare_elements() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn deserialize_seq_single_bare_element_is_one_item() {
        // Bare single-value leniency: a lone element deserializes as a 1-element seq.
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, vec!["text/plain".to_owned()]);
    }

    #[test]
    fn deserialize_seq_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<String> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Vec::<String>::new());
    }

    #[test]
    fn deserialize_seq_list_of_integers() {
        let mut input = *b"Example-List: 1, 2, 3";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<i64> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, vec![1, 2, 3]);
    }

    #[test]
    fn deserialize_tuple() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: (String, String) = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, ("a".to_owned(), "b".to_owned()));
    }

    #[test]
    fn deserialize_tuple_struct() {
        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Pair(String, String);

        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Pair = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Pair("a".to_owned(), "b".to_owned()));
    }

    #[test]
    fn deserialize_map_params_only() {
        let mut input = *b"Example-Params: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("a".to_owned(), "1".to_owned());

        assert_eq!(value, expected);
    }

    #[test]
    fn deserialize_map_flattens_params_only_list() {
        // List boundaries between consecutive params-only items are lost in the value stream.
        let mut input = *b"Example-Params: ;a=1, ;b=2";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("a".to_owned(), "1".to_owned());
        expected.insert("b".to_owned(), "2".to_owned());

        assert_eq!(value, expected);
    }

    #[test]
    fn deserialize_seq_flattens_params_only_items_into_one_map() {
        // The seq params-only clause: one list of params-only items is a 1-element seq holding a
        // single map with all the parameters.
        let mut input = *b"Example-Params: ;a=1, ;b=2";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<std::collections::HashMap<String, String>> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected_item = std::collections::HashMap::new();
        expected_item.insert("a".to_owned(), "1".to_owned());
        expected_item.insert("b".to_owned(), "2".to_owned());

        assert_eq!(value, vec![expected_item]);
    }

    #[test]
    fn deserialize_map_rejects_bare_element() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<std::collections::HashMap<String, String>, Error> =
            serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected parameters, found an element"
            )))
        );
    }

    #[test]
    fn deserialize_map_rejects_element_with_params() {
        let mut input = *b"Content-Type: text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<std::collections::HashMap<String, String>, Error> =
            serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected parameters, found an element"
            )))
        );
    }

    #[test]
    fn deserialize_map_rejects_element_mid_drain() {
        // A mixed list (params-only item followed by a bare element) at value position.
        let mut input = *b"Example-Mixed: ;a=1, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<std::collections::HashMap<String, String>, Error> =
            serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected parameters, found an element"
            )))
        );
    }

    #[test]
    fn deserialize_map_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value.len(), 0);
    }

    /// A dynamic value capturing whatever `deserialize_any` emits (test-only).
    #[derive(Debug, PartialEq)]
    enum AnyValue {
        Unit,
        Str(String),
        Seq(Vec<AnyValue>),
        Map(std::collections::HashMap<String, String>),
    }

    impl<'de> serde::Deserialize<'de> for AnyValue {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;

            impl<'v> serde::de::Visitor<'v> for Visitor {
                type Value = AnyValue;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("any value")
                }

                fn visit_unit<E>(self) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(AnyValue::Unit)
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(AnyValue::Str(v.to_owned()))
                }

                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::SeqAccess<'v>,
                {
                    let mut items = Vec::new();
                    while let Some(item) = seq.next_element::<AnyValue>()? {
                        items.push(item);
                    }
                    Ok(AnyValue::Seq(items))
                }

                fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                where
                    A: serde::de::MapAccess<'v>,
                {
                    let mut entries = std::collections::HashMap::new();
                    while let Some(key) = map.next_key::<String>()? {
                        let value: String = map.next_value()?;
                        entries.insert(key, value);
                    }
                    Ok(AnyValue::Map(entries))
                }
            }

            deserializer.deserialize_any(Visitor)
        }
    }

    #[test]
    fn deserialize_any_bare_element_is_str() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: AnyValue = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, AnyValue::Str("text/plain".to_owned()));
    }

    #[test]
    fn deserialize_any_list_is_seq() {
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: AnyValue = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            AnyValue::Seq(vec![
                AnyValue::Str("a".to_owned()),
                AnyValue::Str("b".to_owned())
            ])
        );
    }

    #[test]
    fn deserialize_any_params_only_is_map() {
        let mut input = *b"Example-Params: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: AnyValue = serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("a".to_owned(), "1".to_owned());

        assert_eq!(value, AnyValue::Map(expected));
    }

    #[test]
    fn deserialize_any_params_only_list_flattens() {
        let mut input = *b"Example-Params: ;a=1, ;b=2";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: AnyValue = serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("a".to_owned(), "1".to_owned());
        expected.insert("b".to_owned(), "2".to_owned());

        assert_eq!(value, AnyValue::Map(expected));
    }

    #[test]
    fn deserialize_any_empty_field_value_is_unit() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: AnyValue = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, AnyValue::Unit);
    }

    #[test]
    fn deserialize_any_rejects_list_item_with_params() {
        let mut input = *b"Example-Mixed: text/plain; charset=utf-8, application/json";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<AnyValue, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "element with parameters"
            )))
        );
    }

    #[test]
    fn deserialize_any_rejects_single_element_with_params() {
        // No comma: a single element+params item, which `any` cannot express.
        let mut input = *b"Content-Type: text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<AnyValue, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "element with parameters"
            )))
        );
    }

    #[test]
    fn deserialize_any_rejects_parameterized_list_item() {
        // The second item carries parameters, which `any` cannot express inside a seq.
        let mut input = *b"Example-Mixed: a, b;c=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<AnyValue, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "element with parameters"
            )))
        );
    }

    #[test]
    fn deserialize_ignored_any_drains_value_position() {
        let mut input = *b"Example-Mixed: text/plain; charset=utf-8, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: serde::de::IgnoredAny = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, serde::de::IgnoredAny);

        // The whole stream was consumed.
        assert_eq!(de.next_value(), Ok(None));
    }

    #[test]
    fn deserialize_ignored_any_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: serde::de::IgnoredAny = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, serde::de::IgnoredAny);
    }

    #[test]
    fn deserialize_ignored_any_skips_one_item_in_seq() {
        // In item scope the drain stops at the next item boundary: each seq element skips exactly
        // one item.
        let mut input = *b"Example-List: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<serde::de::IgnoredAny> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value.len(), 2);
    }

    #[test]
    fn slice_deserializer_ignored_any() {
        // A single parameter value is ignored in place; there is nothing to advance past.
        let bytes = *b"value";
        let slice = SliceDeserializer {
            bytes: &bytes,
            names: None,
        };

        let value: serde::de::IgnoredAny = serde::Deserialize::deserialize(slice).unwrap();

        assert_eq!(value, serde::de::IgnoredAny);
    }

    /// Unit-struct target for the unsupported-hint tests (the derive needs a named type).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct UnitStructTarget;

    /// Newtype-struct target for the unsupported-hint tests.
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct NewtypeStructTarget(String);

    macro_rules! assert_unsupported_hint {
        ($test:ident, $input:literal, $ty:ty, $hint:literal) => {
            #[test]
            fn $test() {
                let mut input = *$input;

                let header = line::HeaderLine::new(&mut input).unwrap();
                let mut de = Deserializer::from_header_values(header.values);

                let result: Result<$ty, Error> = serde::Deserialize::deserialize(&mut de);

                assert_eq!(
                    result,
                    Err(Error::ErrorParsing(ErrorParsing::UnsupportedHint($hint)))
                );
            }
        };
    }

    // The v1-rejected hints fail at the hint level with a clear error, before any content is
    // inspected — even when the field value would look plausible for the target type.
    assert_unsupported_hint!(deserialize_bool_rejected, b"Example-X: true", bool, "bool");
    assert_unsupported_hint!(deserialize_f32_rejected, b"Example-X: 1.5", f32, "f32");
    assert_unsupported_hint!(deserialize_f64_rejected, b"Example-X: 1.5", f64, "f64");
    assert_unsupported_hint!(deserialize_char_rejected, b"Example-X: a", char, "char");
    assert_unsupported_hint!(deserialize_i128_rejected, b"Example-X: 1", i128, "i128");
    assert_unsupported_hint!(deserialize_u128_rejected, b"Example-X: 1", u128, "u128");
    assert_unsupported_hint!(deserialize_unit_rejected, b"Example-X: a", (), "unit");
    assert_unsupported_hint!(
        deserialize_unit_struct_rejected,
        b"Example-X: a",
        UnitStructTarget,
        "unit struct"
    );
    assert_unsupported_hint!(
        deserialize_newtype_struct_rejected,
        b"Example-X: a",
        NewtypeStructTarget,
        "newtype struct"
    );

    #[test]
    fn normalize_strips_separators_and_lowercases() {
        assert_eq!(normalize(b"text/plain"), b"textplain");
        assert_eq!(normalize(b"Max-Age"), b"maxage");
        assert_eq!(normalize(b"content_type"), b"contenttype");
        assert_eq!(normalize(b""), b"");
    }

    #[test]
    fn normalize_bridges_wire_and_rust_names() {
        // Media-type-style slash vs. CamelCase Rust name.
        assert_eq!(normalize(b"text/plain"), normalize(b"TextPlain"));
        // Hyphenated wire form vs. snake_case Rust name.
        assert_eq!(normalize(b"max-age"), normalize(b"max_age"));
        assert_eq!(normalize(b"content-type"), normalize(b"content_type"));
    }

    #[test]
    fn normalize_collides_distinct_names() {
        // Distinct spellings that collapse onto one normalized form: matching is first-wins.
        assert_eq!(normalize(b"max_age"), normalize(b"maxage"));
        assert_eq!(normalize(b"Max-Age"), normalize(b"max_age"));
    }

    #[test]
    fn normalize_drops_non_ascii_bytes() {
        // `obs-text` (0x80-FF) is legal on the wire but carries no identifier content.
        assert_eq!(normalize(b"a\xffb"), b"ab");
    }

    #[test]
    fn pending_names_slot_roundtrip() {
        let mut input = *b"Content-Type: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        assert_eq!(de.pending_names, None);

        // Simulates what `deserialize_struct` / `deserialize_enum` will do.
        de.pending_names = Some(&["a", "b"][..]);

        assert_eq!(de.pending_names, Some(&["a", "b"][..]));
    }

    /// A target capturing whatever `deserialize_identifier` feeds (test-only).
    #[derive(Debug, PartialEq)]
    struct Identifier(String);

    impl<'de> serde::Deserialize<'de> for Identifier {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;

            impl serde::de::Visitor<'_> for Visitor {
                type Value = Identifier;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("an identifier")
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(Identifier(v.to_owned()))
                }
            }

            deserializer.deserialize_identifier(Visitor)
        }
    }

    #[test]
    fn deserialize_identifier_normalized_hit() {
        let mut input = *b"Example-X: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);
        de.pending_names = Some(&["TextPlain"]);

        let value: Identifier = serde::Deserialize::deserialize(&mut de).unwrap();

        // The exact expected name is fed, not the wire bytes.
        assert_eq!(value, Identifier("TextPlain".to_owned()));
    }

    #[test]
    fn deserialize_identifier_collision_first_wins() {
        let mut input = *b"Example-X: max-age";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);
        de.pending_names = Some(&["MaxAge", "max_age"]);

        let value: Identifier = serde::Deserialize::deserialize(&mut de).unwrap();

        // Both names normalize to `maxage`; the first in the list wins.
        assert_eq!(value, Identifier("MaxAge".to_owned()));
    }

    #[test]
    fn deserialize_identifier_rename_fallback() {
        let mut input = *b"Example-X: bndry";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);
        de.pending_names = Some(&["boundary"]);

        let value: Identifier = serde::Deserialize::deserialize(&mut de).unwrap();

        // No normalized match: the raw wire bytes pass through for alias recognition.
        assert_eq!(value, Identifier("bndry".to_owned()));
    }

    #[test]
    fn deserialize_identifier_no_list_passthrough() {
        let mut input = *b"Example-X: bndry";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Identifier = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Identifier("bndry".to_owned()));
    }

    #[test]
    fn deserialize_identifier_parameter_key_half() {
        // A parameter contributes its key half as the identifier.
        let mut input = *b"Example-X: ;charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Identifier = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Identifier("charset".to_owned()));
    }

    /// A struct target whose name normalizes to `textplain` (test-only).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct TextPlain {
        charset: Option<String>,
    }

    /// A struct target with a `snake_case` field bridged from a hyphenated wire key.
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct NoCache {
        max_age: Option<String>,
    }

    /// Same as [`NoCache`] but rejecting unknown fields (serde error path). The container rename
    /// stands in for a type name that would normalize to `nocache`.
    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(rename = "no-cache", deny_unknown_fields)]
    struct StrictNoCache {
        max_age: Option<String>,
    }

    /// A struct target with a required field (missing-field error path).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct ApplicationJson {
        boundary: String,
    }

    #[test]
    fn deserialize_struct_element_with_params() {
        let mut input = *b"Example-X: text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: TextPlain = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            TextPlain {
                charset: Some("utf-8".to_owned())
            }
        );
    }

    #[test]
    fn deserialize_struct_bare_element_is_zero_entry_map() {
        let mut input = *b"Example-X: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: TextPlain = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, TextPlain { charset: None });
    }

    #[test]
    fn deserialize_struct_name_mismatch() {
        // `NoCache` normalizes to `nocache`, not `textplain`.
        let mut input = *b"Example-X: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<NoCache, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::NameMismatch {
                expected: "NoCache".to_owned(),
                found: b"text/plain".to_vec()
            }))
        );
    }

    #[test]
    fn deserialize_struct_params_only_skips_name_check() {
        let mut input = *b"Example-X: ;charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: TextPlain = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            TextPlain {
                charset: Some("utf-8".to_owned())
            }
        );
    }

    #[test]
    fn deserialize_struct_identifier_matches_field_key() {
        // The hyphenated wire key `max-age` bridges to the snake_case field `max_age`.
        let mut input = *b"Example-X: no-cache; max-age=3600";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: NoCache = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            NoCache {
                max_age: Some("3600".to_owned())
            }
        );
    }

    #[test]
    fn deserialize_struct_rejects_multi_item_list() {
        let mut input = *b"Example-X: text/plain, text/html";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<TextPlain, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a single item"
            )))
        );
    }

    #[test]
    fn deserialize_struct_rejects_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<TextPlain, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value"
            )))
        );
    }

    #[test]
    fn deserialize_struct_unknown_field_ignored_by_default() {
        // Plain serde semantics: without `deny_unknown_fields`, unknown keys are ignored.
        let mut input = *b"Example-X: no-cache; bogus=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: NoCache = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, NoCache { max_age: None });
    }

    #[test]
    fn deserialize_struct_denied_unknown_field_is_serde_error() {
        let mut input = *b"Example-X: no-cache; bogus=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<StrictNoCache, Error> = serde::Deserialize::deserialize(&mut de);

        let Err(Error::ErrorParsing(ErrorParsing::Custom(message))) = &result else {
            panic!("expected a serde custom error, got {result:?}");
        };

        assert!(
            message.contains("unknown field"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn deserialize_struct_missing_required_field() {
        let mut input = *b"Example-X: application/json";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<ApplicationJson, Error> = serde::Deserialize::deserialize(&mut de);

        let Err(Error::ErrorParsing(ErrorParsing::Custom(message))) = &result else {
            panic!("expected a serde custom error, got {result:?}");
        };

        assert!(
            message.contains("missing field"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn deserialize_struct_list_of_items() {
        let mut input = *b"Example-X: text/plain, text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<TextPlain> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            vec![
                TextPlain { charset: None },
                TextPlain {
                    charset: Some("utf-8".to_owned())
                }
            ]
        );
    }

    /// An enum target covering unit, struct, and newtype variants (test-only).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    enum Mime {
        TextPlain,
        TextHtml { charset: Option<String> },
        NoCache(NoCache),
    }

    #[test]
    fn deserialize_enum_unit_variant() {
        let mut input = *b"Example-X: text/plain";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Mime = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, Mime::TextPlain);
    }

    #[test]
    fn deserialize_enum_struct_variant() {
        let mut input = *b"Example-X: text/html; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Mime = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            Mime::TextHtml {
                charset: Some("utf-8".to_owned())
            }
        );
    }

    #[test]
    fn deserialize_enum_newtype_variant() {
        let mut input = *b"Example-X: no-cache; max-age=3600";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Mime = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            Mime::NoCache(NoCache {
                max_age: Some("3600".to_owned())
            })
        );
    }

    #[test]
    fn deserialize_enum_missing_element() {
        // A params-only item has no element to name the variant.
        let mut input = *b"Example-X: ;a=1";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Mime, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::MissingElement))
        );
    }

    #[test]
    fn deserialize_enum_unit_variant_rejects_params() {
        let mut input = *b"Example-X: text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Mime, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a bare element, found parameters"
            )))
        );
    }

    #[test]
    fn deserialize_enum_rejects_empty_field_value() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Mime, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an element, found an empty field value"
            )))
        );
    }

    #[test]
    fn deserialize_enum_rejects_multi_item_list() {
        let mut input = *b"Example-X: text/plain, text/html";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Mime, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected a single item"
            )))
        );
    }

    /// A single-variant enum whose field is optional: the Option-field discipline target.
    #[derive(Debug, PartialEq, serde::Deserialize)]
    enum MimeItem {
        TextPlain { charset: Option<String> },
    }

    #[test]
    fn deserialize_enum_mixed_bare_and_parameterized_list() {
        // A bare and a parameterized occurrence of the same element both land in the one
        // variant; the `Option` field is what distinguishes them.
        let mut input = *b"Example-X: text/plain, text/plain; charset=utf-8";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: Vec<MimeItem> = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            vec![
                MimeItem::TextPlain { charset: None },
                MimeItem::TextPlain {
                    charset: Some("utf-8".to_owned())
                }
            ]
        );
    }

    /// A section target with hyphenated header names bridged to `snake_case` fields (test-only).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct Section {
        content_type: Option<String>,
        content_length: Option<u64>,
    }

    #[test]
    fn deserialize_section_map() {
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("content-type".to_owned(), "text/plain".to_owned());
        expected.insert("content-length".to_owned(), "3".to_owned());

        assert_eq!(value, expected);
    }

    #[test]
    fn deserialize_section_struct() {
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let value: Section = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            Section {
                content_type: Some("text/plain".to_owned()),
                content_length: Some(3)
            }
        );
    }

    #[test]
    fn deserialize_section_struct_missing_header_is_none() {
        let mut input = *b"Content-Type: text/plain\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let value: Section = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(
            value,
            Section {
                content_type: Some("text/plain".to_owned()),
                content_length: None
            }
        );
    }

    #[test]
    fn deserialize_section_empty() {
        let mut input = *b"\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value.len(), 0);
    }

    #[test]
    fn deserialize_section_duplicate_field_map_last_wins() {
        // Plain serde map semantics: the last occurrence wins.
        let mut input = *b"Content-Length: 1\r\nContent-Length: 2\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let value: std::collections::HashMap<String, String> =
            serde::Deserialize::deserialize(&mut de).unwrap();

        let mut expected = std::collections::HashMap::new();
        expected.insert("content-length".to_owned(), "2".to_owned());

        assert_eq!(value, expected);
    }

    #[test]
    fn deserialize_section_duplicate_field_struct_errors() {
        // Plain serde struct semantics: a repeated field is an error.
        let mut input = *b"Content-Length: 1\r\nContent-Length: 2\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let result: Result<Section, Error> = serde::Deserialize::deserialize(&mut de);

        let Err(Error::ErrorParsing(ErrorParsing::Custom(message))) = result else {
            panic!("expected a serde custom error");
        };

        assert!(
            message.contains("duplicate field"),
            "unexpected message: {message}"
        );
    }

    #[test]
    fn deserialize_section_parser_error_surfaces() {
        // A space is not a legal header field name byte; the parser error surfaces through the
        // `Parser` wrapper.
        let mut input = *b"Bad Name: x\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let mut de = SectionDeserializer::from_headers(headers);

        let result: Result<Section, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::InvalidFieldName(b' ')))
        );
    }

    #[test]
    fn public_entry_point_deserializes_struct() {
        // The public API drives the section deserializer end to end.
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let value: Section = crate::de::from_headers(headers).unwrap();

        assert_eq!(
            value,
            Section {
                content_type: Some("text/plain".to_owned()),
                content_length: Some(3)
            }
        );
    }

    #[test]
    fn public_entry_point_deserializes_map() {
        let mut input = *b"Content-Type: text/plain\r\nContent-Length: 3\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let value: std::collections::HashMap<String, String> =
            crate::de::from_headers(headers).unwrap();

        assert_eq!(value.len(), 2);
        assert_eq!(value.get("content-type"), Some(&"text/plain".to_owned()));
        assert_eq!(value.get("content-length"), Some(&"3".to_owned()));
    }

    #[test]
    fn public_entry_point_propagates_parser_error() {
        // The error type is public and carries the parser error through the `Parser` wrapper.
        let mut input = *b"Bad Name: x\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let result: Result<Section, crate::de::Error> = crate::de::from_headers(headers);

        assert_eq!(
            result,
            Err(crate::de::Error::Parser(error::Error::InvalidFieldName(
                b' '
            )))
        );
    }

    /// The six canonical value-level shapes for the hint × shape matrix (test-only).
    const MATRIX_EMPTY: &[u8] = b"Example-X:";
    const MATRIX_BARE: &[u8] = b"Example-X: a";
    const MATRIX_LIST: &[u8] = b"Example-X: a, b";
    const MATRIX_PARAMS_ONLY: &[u8] = b"Example-X: ;k=v";
    const MATRIX_ELEMENT_PARAMS: &[u8] = b"Example-X: a; k=v";
    const MATRIX_MIXED_LIST: &[u8] = b"Example-X: a, ;k=v";

    /// Runs one hint × shape matrix cell: deserializes `T` from a fresh deserializer over
    /// `input` and compares the outcome with `expect`.
    fn check_matrix_cell<T>(input: &[u8], expect: &Result<T, Error>)
    where
        T: serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let mut buffer = input.to_vec();

        let header = line::HeaderLine::new(&mut buffer).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let actual: Result<T, Error> = serde::Deserialize::deserialize(&mut de);

        match (&actual, expect) {
            (Ok(got), Ok(want)) => {
                assert_eq!(got, want, "input: {}", String::from_utf8_lossy(input));
            },
            (Err(got), Err(want)) => {
                assert_eq!(got, want, "input: {}", String::from_utf8_lossy(input));
            },
            (other_actual, other_expect) => panic!(
                "input {}: expected {other_expect:?}, got {other_actual:?}",
                String::from_utf8_lossy(input)
            ),
        }
    }

    fn shape_err(msg: &'static str) -> Error {
        Error::ErrorParsing(ErrorParsing::UnsupportedShape(msg))
    }

    fn hint_err(hint: &'static str) -> Error {
        Error::ErrorParsing(ErrorParsing::UnsupportedHint(hint))
    }

    #[test]
    fn matrix_str_shape_table() {
        let rows: &[(&[u8], Result<String, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err(
                    "expected a bare element, found an empty field value",
                )),
            ),
            (MATRIX_BARE, Ok("a".to_owned())),
            (MATRIX_LIST, Ok("a, b".to_owned())),
            (
                MATRIX_PARAMS_ONLY,
                Err(shape_err("expected a bare element, found parameters")),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("expected bare elements, found parameters")),
            ),
            (
                MATRIX_MIXED_LIST,
                Err(shape_err("expected bare elements, found parameters")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_bytes_shape_table() {
        let rows: &[(&[u8], Result<ByteBuf, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err(
                    "expected a bare element, found an empty field value",
                )),
            ),
            (MATRIX_BARE, Ok(ByteBuf(b"a".to_vec()))),
            (MATRIX_LIST, Ok(ByteBuf(b"a, b".to_vec()))),
            (
                MATRIX_PARAMS_ONLY,
                Err(shape_err("expected a bare element, found parameters")),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("expected bare elements, found parameters")),
            ),
            (
                MATRIX_MIXED_LIST,
                Err(shape_err("expected bare elements, found parameters")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_unsigned_integer_shape_table() {
        let rows: &[(&[u8], Result<u32, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err(
                    "expected a bare element, found an empty field value",
                )),
            ),
            (b"Example-X: 42", Ok(42)),
            (
                b"Example-X: a",
                Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                    found: b"a".to_vec(),
                })),
            ),
            // Fits `i128` but overflows the target type.
            (
                b"Example-X: 4294967296",
                Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                    found: b"4294967296".to_vec(),
                })),
            ),
            // More than 19 digits is rejected before any accumulation.
            (
                b"Example-X: 99999999999999999999",
                Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                    found: b"99999999999999999999".to_vec(),
                })),
            ),
            (
                b"Example-X: 1, 2",
                Err(shape_err("expected a single bare element")),
            ),
            (
                MATRIX_PARAMS_ONLY,
                Err(shape_err("expected a bare element, found parameters")),
            ),
            (
                b"Example-X: 42; k=v",
                Err(shape_err("expected bare elements, found parameters")),
            ),
            (
                b"Example-X: 1, ;k=v",
                Err(shape_err("expected bare elements, found parameters")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_signed_integer_shape_table() {
        let rows: &[(&[u8], Result<i64, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err(
                    "expected a bare element, found an empty field value",
                )),
            ),
            (b"Example-X: -7", Ok(-7)),
            // No `+` sign form is accepted.
            (
                b"Example-X: +5",
                Err(Error::ErrorParsing(ErrorParsing::InvalidInteger {
                    found: b"+5".to_vec(),
                })),
            ),
            (
                b"Example-X: 1, 2",
                Err(shape_err("expected a single bare element")),
            ),
            (
                MATRIX_PARAMS_ONLY,
                Err(shape_err("expected a bare element, found parameters")),
            ),
            (
                b"Example-X: -7; k=v",
                Err(shape_err("expected bare elements, found parameters")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_seq_shape_table() {
        let rows = &[
            (MATRIX_EMPTY, Ok(vec![])),
            (MATRIX_BARE, Ok(vec![AnyValue::Str("a".to_owned())])),
            (
                MATRIX_LIST,
                Ok(vec![
                    AnyValue::Str("a".to_owned()),
                    AnyValue::Str("b".to_owned()),
                ]),
            ),
            // Params-only: a 1-element seq of one map holding all params.
            (
                MATRIX_PARAMS_ONLY,
                Ok(vec![AnyValue::Map({
                    let mut map = std::collections::HashMap::new();
                    map.insert("k".to_owned(), "v".to_owned());
                    map
                })]),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("element with parameters")),
            ),
            (MATRIX_MIXED_LIST, Err(shape_err("element with parameters"))),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_tuple_shape_table() {
        // A 2-element list fills the tuple; shorter streams end in serde's invalid-length error.
        let mut input = *b"Example-X: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: (String, String) = serde::Deserialize::deserialize(&mut de).unwrap();

        assert_eq!(value, ("a".to_owned(), "b".to_owned()));

        for short in [MATRIX_EMPTY, MATRIX_BARE] {
            let mut buffer = short.to_vec();

            let short_header = line::HeaderLine::new(&mut buffer).unwrap();
            let mut short_de = Deserializer::from_header_values(short_header.values);

            let result: Result<(String, String), Error> =
                serde::Deserialize::deserialize(&mut short_de);

            let Err(Error::ErrorParsing(ErrorParsing::Custom(message))) = result else {
                panic!("expected a serde custom error, got {result:?}");
            };

            assert!(
                message.contains("invalid length"),
                "unexpected message: {message}"
            );
        }
    }

    #[test]
    fn matrix_map_shape_table() {
        let rows = &[
            (MATRIX_EMPTY, Ok(std::collections::HashMap::new())),
            (
                MATRIX_BARE,
                Err(shape_err("expected parameters, found an element")),
            ),
            (
                MATRIX_LIST,
                Err(shape_err("expected parameters, found an element")),
            ),
            (
                MATRIX_PARAMS_ONLY,
                Ok({
                    let mut map = std::collections::HashMap::new();
                    map.insert("k".to_owned(), "v".to_owned());
                    map
                }),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("expected parameters, found an element")),
            ),
            (
                MATRIX_MIXED_LIST,
                Err(shape_err("expected parameters, found an element")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_struct_shape_table() {
        let rows: &[(&[u8], Result<TextPlain, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err("expected an element, found an empty field value")),
            ),
            (b"Example-X: text/plain", Ok(TextPlain { charset: None })),
            (
                b"Example-X: text/plain, text/html",
                Err(shape_err("expected a single item")),
            ),
            (
                b"Example-X: ;charset=utf-8",
                Ok(TextPlain {
                    charset: Some("utf-8".to_owned()),
                }),
            ),
            (
                b"Example-X: text/plain; charset=utf-8",
                Ok(TextPlain {
                    charset: Some("utf-8".to_owned()),
                }),
            ),
            // The params-only tail attaches to the preceding element: one item.
            (
                b"Example-X: text/plain, ;charset=utf-8",
                Ok(TextPlain {
                    charset: Some("utf-8".to_owned()),
                }),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    /// An enum target for the matrix: one struct variant and one unit variant (test-only).
    #[derive(Debug, PartialEq, serde::Deserialize)]
    enum MatrixMime {
        TextPlain { charset: Option<String> },
        TextHtml,
    }

    #[test]
    fn matrix_enum_shape_table() {
        let rows: &[(&[u8], Result<MatrixMime, Error>)] = &[
            (
                MATRIX_EMPTY,
                Err(shape_err("expected an element, found an empty field value")),
            ),
            // A bare element matching a struct variant is a zero-entry map.
            (
                b"Example-X: text/plain",
                Ok(MatrixMime::TextPlain { charset: None }),
            ),
            // A bare element naming the unit variant.
            (b"Example-X: text/html", Ok(MatrixMime::TextHtml)),
            (
                b"Example-X: text/plain, text/html",
                Err(shape_err("expected a single item")),
            ),
            (
                MATRIX_PARAMS_ONLY,
                Err(Error::ErrorParsing(ErrorParsing::MissingElement)),
            ),
            (
                b"Example-X: text/plain; charset=utf-8",
                Ok(MatrixMime::TextPlain {
                    charset: Some("utf-8".to_owned()),
                }),
            ),
            // The params-only tail attaches to the preceding element: one item.
            (
                b"Example-X: text/plain, ;charset=utf-8",
                Ok(MatrixMime::TextPlain {
                    charset: Some("utf-8".to_owned()),
                }),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_option_shape_table() {
        let rows = &[
            (MATRIX_EMPTY, Ok(None)),
            (MATRIX_BARE, Ok(Some("a".to_owned()))),
            (MATRIX_LIST, Ok(Some("a, b".to_owned()))),
            (
                MATRIX_PARAMS_ONLY,
                Err(shape_err("expected a bare element, found parameters")),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("expected bare elements, found parameters")),
            ),
            (
                MATRIX_MIXED_LIST,
                Err(shape_err("expected bare elements, found parameters")),
            ),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_any_shape_table() {
        let rows: &[(&[u8], Result<AnyValue, Error>)] = &[
            (MATRIX_EMPTY, Ok(AnyValue::Unit)),
            (MATRIX_BARE, Ok(AnyValue::Str("a".to_owned()))),
            (
                MATRIX_LIST,
                Ok(AnyValue::Seq(vec![
                    AnyValue::Str("a".to_owned()),
                    AnyValue::Str("b".to_owned()),
                ])),
            ),
            (
                MATRIX_PARAMS_ONLY,
                Ok(AnyValue::Map({
                    let mut map = std::collections::HashMap::new();
                    map.insert("k".to_owned(), "v".to_owned());
                    map
                })),
            ),
            (
                MATRIX_ELEMENT_PARAMS,
                Err(shape_err("element with parameters")),
            ),
            (MATRIX_MIXED_LIST, Err(shape_err("element with parameters"))),
        ];

        for (input, expect) in rows {
            check_matrix_cell(input, expect);
        }
    }

    #[test]
    fn matrix_ignored_any_shape_table() {
        // Every shape is consumed without visiting and completes as unit.
        let shapes: &[&[u8]] = &[
            MATRIX_EMPTY,
            MATRIX_BARE,
            MATRIX_LIST,
            MATRIX_PARAMS_ONLY,
            MATRIX_ELEMENT_PARAMS,
            MATRIX_MIXED_LIST,
        ];

        for input in shapes {
            check_matrix_cell(input, &Ok(serde::de::IgnoredAny));
        }
    }

    #[test]
    fn matrix_rejected_hints_table() {
        // The v1-rejected hints fail at the hint level, independent of shape.
        let shapes: &[&[u8]] = &[
            MATRIX_EMPTY,
            MATRIX_BARE,
            MATRIX_LIST,
            MATRIX_PARAMS_ONLY,
            MATRIX_ELEMENT_PARAMS,
            MATRIX_MIXED_LIST,
        ];

        for input in shapes {
            check_matrix_cell::<bool>(input, &Err(hint_err("bool")));
            check_matrix_cell::<i128>(input, &Err(hint_err("i128")));
            check_matrix_cell::<u128>(input, &Err(hint_err("u128")));
            check_matrix_cell::<f32>(input, &Err(hint_err("f32")));
            check_matrix_cell::<f64>(input, &Err(hint_err("f64")));
            check_matrix_cell::<char>(input, &Err(hint_err("char")));
            check_matrix_cell::<()>(input, &Err(hint_err("unit")));
            check_matrix_cell::<UnitStructTarget>(input, &Err(hint_err("unit struct")));
            check_matrix_cell::<NewtypeStructTarget>(input, &Err(hint_err("newtype struct")));
        }
    }

    /// Captures how a string hint emitted its data (borrowed vs owned) plus the content
    /// (test-only).
    #[derive(Debug, PartialEq)]
    struct StrEmission {
        borrowed: bool,
        content: String,
    }

    impl<'de> serde::Deserialize<'de> for StrEmission {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;

            impl serde::de::Visitor<'_> for Visitor {
                type Value = StrEmission;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a string")
                }

                fn visit_borrowed_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(StrEmission {
                        borrowed: true,
                        content: v.to_owned(),
                    })
                }

                fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(StrEmission {
                        borrowed: false,
                        content: v.to_owned(),
                    })
                }
            }

            deserializer.deserialize_str(Visitor)
        }
    }

    /// The byte-level counterpart of [`StrEmission`] (test-only).
    #[derive(Debug, PartialEq)]
    struct BytesEmission {
        borrowed: bool,
        content: Vec<u8>,
    }

    impl<'de> serde::Deserialize<'de> for BytesEmission {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            struct Visitor;

            impl serde::de::Visitor<'_> for Visitor {
                type Value = BytesEmission;

                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a byte slice")
                }

                fn visit_borrowed_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(BytesEmission {
                        borrowed: true,
                        content: v.to_vec(),
                    })
                }

                fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
                where
                    E: serde::de::Error,
                {
                    Ok(BytesEmission {
                        borrowed: false,
                        content: v.to_vec(),
                    })
                }
            }

            deserializer.deserialize_bytes(Visitor)
        }
    }

    #[test]
    fn matrix_str_hint_single_element_is_borrowed() {
        let mut input = *b"Example-X: a";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: StrEmission = serde::Deserialize::deserialize(&mut de).unwrap();

        assert!(value.borrowed, "a single bare element must be borrowed");
        assert_eq!(value.content, "a");
    }

    #[test]
    fn matrix_str_hint_list_is_owned_rejoin() {
        let mut input = *b"Example-X: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: StrEmission = serde::Deserialize::deserialize(&mut de).unwrap();

        assert!(!value.borrowed, "a rejoin must be owned");
        assert_eq!(value.content, "a, b");
    }

    #[test]
    fn matrix_bytes_hint_single_element_is_borrowed() {
        let mut input = *b"Example-X: a";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: BytesEmission = serde::Deserialize::deserialize(&mut de).unwrap();

        assert!(value.borrowed, "a single bare element must be borrowed");
        assert_eq!(value.content, b"a".to_vec());
    }

    #[test]
    fn matrix_bytes_hint_list_is_owned_rejoin() {
        let mut input = *b"Example-X: a, b";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let value: BytesEmission = serde::Deserialize::deserialize(&mut de).unwrap();

        assert!(!value.borrowed, "a rejoin must be owned");
        assert_eq!(value.content, b"a, b".to_vec());
    }

    #[test]
    fn error_path_invalid_utf8_str() {
        // `obs-text` (0x80-FF) is legal on the wire but fails UTF-8 validation on str paths.
        let mut input = *b"Example-X: a\xff";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(result, Err(Error::ErrorParsing(ErrorParsing::InvalidUtf8)));
    }

    #[test]
    fn error_path_invalid_utf8_identifier_fallback() {
        // The rename/alias fallback requires valid UTF-8; matching itself is byte-level.
        let mut input = *b"Example-X: a\xff";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Identifier, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(result, Err(Error::ErrorParsing(ErrorParsing::InvalidUtf8)));
    }

    #[test]
    fn error_path_identifier_empty_stream() {
        let mut input = *b"Example-X:";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<Identifier, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "expected an identifier"
            )))
        );
    }

    #[test]
    fn error_path_map_access_value_before_key() {
        // Defensive ordering check: the map serves keys before values strictly.
        let mut input = *b"Example-X: ;k=v";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let mut map = MapAccess {
            de: &mut de,
            pending_value: None,
        };

        let result: Result<String, Error> = map.next_value_seed(PhantomData::<String>);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "parameter value requested before its key"
            )))
        );
    }

    #[test]
    fn error_path_section_map_access_value_before_name() {
        // Defensive ordering check: the section serves names before values strictly.
        let mut input = *b"Content-Type: x\r\n\r\n";

        let mut headers = line::Headers::new(&mut input);
        let mut map = SectionMapAccess {
            headers: &mut headers,
            names: None,
            pending_values: None,
        };

        let result: Result<String, Error> = map.next_value_seed(PhantomData::<String>);

        assert_eq!(
            result,
            Err(Error::ErrorParsing(ErrorParsing::UnsupportedShape(
                "header value requested before its name"
            )))
        );
    }

    #[test]
    fn error_path_parser_missing_crlf_at_section_level() {
        // A section without a terminating CRLF is a parser error, surfaced through the wrapper.
        let mut input = *b"Content-Type: x";

        let headers = line::Headers::new(&mut input);
        let result: Result<Section, Error> = crate::de::from_headers(headers);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::MissingCRLFDelimiter))
        );
    }

    #[test]
    fn error_path_parser_missing_field_delimiter_at_section_level() {
        let mut input = *b"NoColon\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let result: Result<Section, Error> = crate::de::from_headers(headers);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::MissingFieldDelimiter))
        );
    }

    #[test]
    fn error_path_parser_missing_field_name_at_section_level() {
        let mut input = *b": x\r\n\r\n";

        let headers = line::Headers::new(&mut input);
        let result: Result<Section, Error> = crate::de::from_headers(headers);

        assert_eq!(result, Err(Error::Parser(error::Error::MissingFieldName)));
    }

    #[test]
    fn error_path_value_empty_list() {
        // A list of only empty elements is a value-level parser error.
        let mut input = *b"Example-X: ,  ,, ,";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::EmptyList
            )))
        );
    }

    #[test]
    fn error_path_value_missing_closing_quote() {
        let mut input = *b"Example-X: \"abc";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::MissingClosingQuote
            )))
        );
    }

    #[test]
    fn error_path_value_leftover_bytes() {
        let mut input = *b"Example-X: \"ab\"/cd";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::LeftoverBytes
            )))
        );
    }

    #[test]
    fn error_path_value_missing_parameter_delimiter() {
        // A parameter key without its `=` delimiter.
        let mut input = *b"Example-X: ;abc123";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::MissingParameterDelimiter
            )))
        );
    }

    #[test]
    fn error_path_value_missing_parameter_key() {
        let mut input = *b"Example-X: ;=v";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::MissingParameterKey
            )))
        );
    }

    #[test]
    fn error_path_value_missing_parameter_value() {
        let mut input = *b"Example-X: ;k=";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let result: Result<String, Error> = serde::Deserialize::deserialize(&mut de);

        assert_eq!(
            result,
            Err(Error::Parser(error::Error::Value(
                error::ErrorValue::MissingParameterValue
            )))
        );
    }

    /// A per-item target holding a *borrowed* parameter value (test-only): pins the forward-only
    /// invariant across continued iteration.
    #[derive(Debug, PartialEq, serde::Deserialize)]
    struct FrozenItem<'a> {
        note: Option<&'a str>,
    }

    #[test]
    fn forward_only_yielded_regions_stay_frozen() {
        // The parser mutates later regions in place (lowercasing parameter keys, decoding quoted
        // pairs). A region already yielded as a borrowed view must be untouched by that work:
        // item 1's value is borrowed before the parser rewrites item 2's key and value.
        let mut input = *b"Example-X: Frozen-Item; note=alpha, Frozen-Item; Note=\"bet\\\"a\"";

        let header = line::HeaderLine::new(&mut input).unwrap();
        let mut de = Deserializer::from_header_values(header.values);

        let items: Vec<FrozenItem<'_>> = serde::Deserialize::deserialize(&mut de).unwrap();

        // Item 1's borrowed view is byte-identical to the wire, despite item 2's in-place
        // mutations happening after it was yielded.
        assert_eq!(
            items[0],
            FrozenItem {
                note: Some("alpha")
            }
        );

        // Item 2 shows the parser did mutate its own region: the key was lower-cased and the
        // quoted pair decoded in place.
        assert_eq!(
            items[1],
            FrozenItem {
                note: Some("bet\"a")
            }
        );
    }

    #[test]
    fn forward_only_shared_coercion_moves_mutable_borrow() {
        // `shared_bytes` takes the mutable borrow by value: once a region has been shared, no
        // mutable handle to it remains — the compile-time half of the forward-only invariant.
        let mut buffer = *b"abc";

        let owned: &mut [u8] = &mut buffer;
        let shared = shared_bytes(owned);

        assert_eq!(shared, b"abc");
    }
}
