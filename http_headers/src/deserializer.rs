//! Serde deserialization layer for header field values.
//!
//! This module provides a [`serde::Deserializer`] implementation which maps header field values
//! onto Serde's data model on top of the zero-copy parser in [`crate::line`].

use serde::de;

use super::line::HeaderValues;
use super::line::Value;

/// Error type for the serde deserialization layer.
///
/// This is deliberately separate from [`crate::error::Error`] so that consumers of the low-level
/// parser in [`crate::line`] never have to handle deserialization-specific variants.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Error {
    /// A parser error surfaced while driving the header value iterator.
    Parser(crate::error::Error),

    /// A failure specific to the deserialization process itself.
    ErrorParsing(ErrorParsing),
}

/// Deserialization-specific failures which are not raised by the low-level parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ErrorParsing {
    /// A params-only field value was encountered where an element is required.
    MissingElement,

    /// An element did not normalize-match the expected name of a struct or variant.
    NameMismatch {
        /// The name expected by the target type.
        expected: &'static str,
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
            Some(Value::Element(element)) if self.in_item_scope => {
                self.head = Some(Value::Element(element));
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

/// A deserializer over a single borrowed byte slice, emitting only string/bytes data. Used by
/// [`MapAccess`] to serve the key and value halves of a parameter pair.
struct SliceDeserializer<'de> {
    bytes: &'de [u8],
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

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("identifier")
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

                let pair_key = seed.deserialize(SliceDeserializer { bytes: key_bytes })?;
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

        seed.deserialize(SliceDeserializer { bytes: val })
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

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        unsupported_hint!("identifier")
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

#[cfg(test)]
mod test {
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
            expected: "TextPlain",
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
        let slice = SliceDeserializer { bytes: &bytes };

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
}
