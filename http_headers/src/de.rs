use super::bytestr::*;
use super::error::*;
use super::line::*;

pub fn from_mut_bytes<'de, T>(bytes: &'de mut [u8]) -> Result<T, ErrorSerde>
where
    T: serde::de::Deserialize<'de>,
{
    T::deserialize(Deserializer::from_mut_bytes(bytes))
}

struct Deserializer<'de> {
    headers: Headers<'de>,
}

impl<'de> Deserializer<'de> {
    pub fn from_mut_bytes(bytes: &'de mut [u8]) -> Self {
        Self {
            headers: Headers::new(bytes),
        }
    }
}

impl<'de> serde::de::Deserializer<'de> for Deserializer<'de> {
    type Error = ErrorSerde;

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_map(self)
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_map(self)
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct enum identifier ignored_any
    }
}

impl<'de> serde::de::MapAccess<'de> for Deserializer<'de> {
    type Error = ErrorSerde;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.headers.next_key()? {
            Some(key) => seed.deserialize(DeserializeKey::new(key)).map(Some),
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        let mut values = self.headers.next_val();

        if values.is_seq() {
            seed.deserialize(DeserializeList::new(values))
        } else {
            seed.deserialize(DeserializeValue::new(values.next_val()?))
        }
    }
}

struct DeserializeList<'de> {
    values: HeaderList<'de>,
}

impl<'de> serde::de::Deserializer<'de> for DeserializeList<'de> {
    type Error = ErrorSerde;

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum identifier ignored_any
    }
}

impl<'de> serde::de::SeqAccess<'de> for DeserializeList<'de> {
    type Error = ErrorSerde;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Self::Error>
    where
        T: serde::de::DeserializeSeed<'de>,
    {
        seed.deserialize(DeserializeValue::new(self.values.next_val()?))
            .map(Some)
    }
}

impl<'de> DeserializeList<'de> {
    fn new(values: HeaderList<'de>) -> Self {
        Self { values }
    }
}

struct DeserializeKey<'de> {
    key: ByteStr<'de>,
}

impl<'de> DeserializeKey<'de> {
    fn new(key: ByteStr<'de>) -> Self {
        Self { key }
    }
}

impl<'de> serde::de::Deserializer<'de> for DeserializeKey<'de> {
    type Error = ErrorSerde;

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_bytes(self.key.0)
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!()
    }

    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str string
        bytes byte_buf option unit unit_struct newtype_struct seq tuple
        tuple_struct map struct enum ignored_any
    }
}

struct DeserializeValue<'de, 'a> {
    value: Option<(ByteStr<'de>, HeaderParameters<'de, 'a>)>,
}

impl<'de, 'a> DeserializeValue<'de, 'a> {
    fn new(value: Option<(ByteStr<'de>, HeaderParameters<'de, 'a>)>) -> Self {
        Self { value }
    }

    #[inline]
    fn deserialize_element<V>(
        self,
        visitor: V,
        de: impl FnOnce(V, &'de mut [u8], HeaderParameters<'de, 'a>) -> Result<V::Value, ErrorSerde>,
    ) -> Result<V::Value, ErrorSerde>
    where
        V: serde::de::Visitor<'de>,
    {
        let Some((ByteStr(bytes), parameters)) = self.value else {
            return visitor.visit_none();
        };

        if !parameters.is_empty() {
            return visitor.visit_map(DeserializeParameters::new(parameters));
        }

        de(visitor, bytes, parameters)
    }
}

impl<'de, 'a> serde::de::Deserializer<'de> for DeserializeValue<'de, 'a> {
    type Error = ErrorSerde;

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| match &*bytes {
            b"true" => visitor.visit_bool(true),
            b"false" => visitor.visit_bool(false),
            _ => visitor.visit_bytes(bytes),
        })
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_i8(n)
        })
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_i16(n)
        })
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_i32(n)
        })
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_i64(n)
        })
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_u8(n)
        })
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_u16(n)
        })
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_u32(n)
        })
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_u64(n)
        })
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_f32(n)
        })
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let n = lexical_core::parse(bytes).map_err(ErrorSerde::Arithmetic)?;
            visitor.visit_f64(n)
        })
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            let string = std::str::from_utf8(bytes).map_err(ErrorSerde::Utf8Error)?;
            visitor.visit_str(string)
        })
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            visitor.visit_borrowed_bytes(bytes)
        })
    }

    #[cfg(feature = "alloc")]
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| visitor.visit_bytes(bytes))
    }

    #[cfg(not(feature = "alloc"))]
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!(
            "`string` deserialization requires the `alloc` feature to be enabled for \
             `http_headers`"
        );
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            visitor.visit_borrowed_bytes(bytes)
        })
    }

    #[cfg(feature = "alloc")]
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, _| {
            visitor.visit_byte_buf(bytes.to_vec())
        })
    }

    #[cfg(not(feature = "alloc"))]
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!(
            "`byte_buf` deserialization requires the `alloc` feature to be enabled for \
             `http_headers`"
        );
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        self.deserialize_element(visitor, |visitor, bytes, parameters| {
            visitor.visit_some(DeserializeValue::new(Some((ByteStr(bytes), parameters))))
        })
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!()
    }
}

struct DeserializeParameters<'de, 'a> {
    parameters: HeaderParameters<'de, 'a>,
}

impl<'de, 'a> DeserializeParameters<'de, 'a> {
    fn new(parameters: HeaderParameters<'de, 'a>) -> Self {
        Self { parameters }
    }
}

impl<'de, 'a> serde::de::MapAccess<'de> for DeserializeParameters<'de, 'a> {
    type Error = ErrorSerde;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Self::Error>
    where
        K: serde::de::DeserializeSeed<'de>,
    {
        match self.parameters.next_key()? {
            Some(key) => seed.deserialize(DeserializeKey::new(key)).map(Some),
            None => Ok(None),
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::DeserializeSeed<'de>,
    {
        seed.deserialize(DeserializeParameterValue::new(self.parameters.next_val()?))
    }
}

struct DeserializeParameterValue<'de> {
    value: ByteStr<'de>,
}

impl<'de> DeserializeParameterValue<'de> {
    fn new(value: ByteStr<'de>) -> Self {
        Self { value }
    }
}

impl<'de> serde::de::Deserializer<'de> for DeserializeParameterValue<'de> {
    type Error = ErrorSerde;

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        match &*self.value.0 {
            b"true" => visitor.visit_bool(true),
            b"false" => visitor.visit_bool(false),
            _ => visitor.visit_bytes(self.value.0),
        }
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_i8(n)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_i16(n)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_i32(n)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_i64(n)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_u8(n)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_u16(n)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_u32(n)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_u64(n)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_f32(n)
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let n = lexical_core::parse(self.value.0).map_err(ErrorSerde::Arithmetic)?;
        visitor.visit_f64(n)
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        let string = std::str::from_utf8(self.value.0).map_err(ErrorSerde::Utf8Error)?;
        visitor.visit_str(string)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(self.value.0)
    }

    #[cfg(feature = "alloc")]
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_bytes(self.value.0)
    }

    #[cfg(not(feature = "alloc"))]
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!(
            "`string` deserialization requires the `alloc` feature to be enabled for \
             `http_headers`"
        );
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_borrowed_bytes(self.value.0)
    }

    #[cfg(feature = "alloc")]
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        visitor.visit_byte_buf(self.value.0.to_vec())
    }

    #[cfg(not(feature = "alloc"))]
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!(
            "`byte_buf` deserialization requires the `alloc` feature to be enabled for \
             `http_headers`"
        );
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        if self.value.0.is_empty() {
            visitor.visit_none()
        } else {
            visitor.visit_some(self)
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_unit_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_newtype_struct<V>(
        self,
        name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_tuple_struct<V>(
        self,
        name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_struct<V>(
        self,
        name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_enum<V>(
        self,
        name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_ignored_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        todo!()
    }

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: serde::de::Visitor<'de>,
    {
        unimplemented!()
    }

    serde::forward_to_deserialize_any! { identifier }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn deserialize_headers_simple() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers<'a> {
            #[serde(borrow)]
            #[serde(rename = "Content-Type")]
            content_type: &'a str,
            #[serde(rename = "Content-Length")]
            content_length: usize,
        }

        let mut lines = *b"\
            Content-Type: text/plain\r\n\
            Content-Length: 3\r\n\
            \r\n\
        ";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                content_type: "text/plain",
                content_length: 3
            })
        );
    }

    #[test]
    fn deserialize_headers_value_bool() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-1")]
            field_1: bool,
            #[serde(rename = "Field-2")]
            field_2: bool,
        }

        let mut lines = *b"\
            Field-1: true\r\n\
            Field-2: false\r\n\
            \r\n\
            \
        ";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_1: true,
                field_2: false
            })
        )
    }

    #[test]
    fn deserialize_headers_value_i8() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: i8,
        }

        let mut lines = *b"Field-Value: -128\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: i8::MIN
            })
        )
    }

    #[test]
    fn deserialize_headers_value_i16() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: i16,
        }

        let mut lines = *b"Field-Value: -32768\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: i16::MIN
            })
        )
    }

    #[test]
    fn deserialize_headers_value_i32() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: i32,
        }

        let mut lines = *b"Field-Value: -2147483648\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: i32::MIN
            })
        )
    }

    #[test]
    fn deserialize_headers_value_i64() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: i64,
        }

        let mut lines = *b"Field-Value: -9223372036854775808\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: i64::MIN
            })
        )
    }

    #[test]
    fn deserialize_headers_value_u8() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: u8,
        }

        let mut lines = *b"Field-Value: 255\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: u8::MAX
            })
        )
    }

    #[test]
    fn deserialize_headers_value_u16() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: u16,
        }

        let mut lines = *b"Field-Value: 65535\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: u16::MAX
            })
        )
    }

    #[test]
    fn deserialize_headers_value_u32() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: u32,
        }

        let mut lines = *b"Field-Value: 4294967295\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: u32::MAX
            })
        )
    }

    #[test]
    fn deserialize_headers_value_u64() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: u64,
        }

        let mut lines = *b"Field-Value: 18446744073709551615\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: u64::MAX
            })
        )
    }

    #[test]
    fn deserialize_headers_value_f32() {
        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: f32,
        }

        let mut lines = *b"Field-Value: +inf\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: f32::INFINITY
            })
        )
    }

    #[test]
    fn deserialize_headers_value_f64() {
        #[derive(Debug, PartialEq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: f64,
        }

        let mut lines = *b"Field-Value: -inf\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: f64::NEG_INFINITY
            })
        )
    }

    #[test]
    fn deserialize_headers_value_char() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: char,
        }

        let mut lines = *b"Field-Value: c\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(headers, Ok(Headers { field_value: 'c' }))
    }

    #[test]
    fn deserialize_headers_value_str() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers<'a> {
            #[serde(rename = "Field-Value")]
            #[serde(borrow)]
            field_value: &'a str,
        }

        let mut lines = *b"Field-Value: \"Hello, World\"\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: "Hello, World"
            })
        )
    }

    #[test]
    fn deserialize_headers_value_string() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: String,
        }

        let mut lines = *b"Field-Value: \"Hello, World\"\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: String::from("Hello, World")
            })
        )
    }

    #[test]
    fn deserialize_headers_value_bytes() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers<'a> {
            #[serde(rename = "Field-Value")]
            #[serde(borrow)]
            field_value: &'a [u8],
        }

        let mut lines = *b"Field-Value: \"Hello, World\"\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: b"Hello, World"
            })
        )
    }

    #[test]
    fn deserialize_headers_value_byte_buf() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-Value")]
            field_value: ByteBuf,
        }

        #[derive(Debug, PartialEq, Eq)]
        struct ByteBuf(Vec<u8>);

        impl<'de> serde::de::Deserialize<'de> for ByteBuf {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                struct Visitor;

                impl<'de> serde::de::Visitor<'de> for Visitor {
                    type Value = ByteBuf;

                    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                        write!(formatter, "a byte buf")
                    }

                    fn visit_byte_buf<E>(self, v: Vec<u8>) -> Result<Self::Value, E>
                    where
                        E: serde::de::Error,
                    {
                        Ok(ByteBuf(v))
                    }
                }

                deserializer.deserialize_byte_buf(Visitor)
            }
        }

        let mut lines = *b"Field-Value: \"Hello, World\"\r\n\r\n";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_value: ByteBuf(b"Hello, World".to_vec())
            })
        )
    }

    #[test]
    fn deserialize_headers_value_option() {
        #[derive(Debug, PartialEq, Eq, serde::Deserialize)]
        struct Headers {
            #[serde(rename = "Field-1")]
            field_1: Option<bool>,
            #[serde(rename = "Field-2")]
            field_2: Option<bool>,
            #[serde(rename = "Field-3")]
            field_3: Option<bool>,
        }

        let mut lines = *b"\
            Field-1: true\r\n\
            Field-2: \r\n\
            \r\n\
            \
        ";

        let headers = from_mut_bytes(&mut lines);

        assert_eq!(
            headers,
            Ok(Headers {
                field_1: Some(true),
                field_2: None,
                field_3: None
            })
        )
    }
}
