use super::OldError;

/// [`Url`] `x-www-form-urlencoded` query.
///
/// # Example
///
/// ```text
/// ?a=1&b=2&c=3
/// ```
///
/// [`Url`]: super::Url
#[derive(PartialEq, Eq)]
pub struct Query<'data> {
    backing: &'data [u8],
}

impl<'data> Query<'data> {
    /// Tries to parse a byte string into a `x-www-form-urlencoded` [`Query`].
    ///
    /// # Errors
    ///
    /// Returns [`EmptyQueryParameter`] if the query has no data between two parameter delimiters (`&`).
    ///
    /// Returns [`InvalidQuery`]  if the query does not adhere to the `x-www-form-urlencoded` format.
    ///
    /// Returns [`MissingQueryKey`] if the query contains a parameter with a value but no key.
    ///
    /// Returns [`MissingQueryValue`] if the query contains a parameter with a key but no value.
    ///
    /// Returns [`ReservedCharacter`] if the query contains an invalid character.
    ///
    /// [`MissingQuery`]: Error::MissingQuery
    /// [`EmptyQueryParameter`]: Error::EmptyQueryParameter
    /// [`InvalidQuery`]: Error::InvalidQuery
    /// [`MissingQueryKey`]: Error::MissingQueryKey
    /// [`MissingQueryValue`]: Error::MissingQueryValue
    /// [`ReservedCharacter`]: Error::ReservedCharacter
    pub fn new(query: &'data [u8]) -> Result<Self, OldError> {
        if !query.is_empty() {
            let mut prev = 0;
            for n in memchr::memchr_iter(b'&', query).chain(std::iter::once(query.len())) {
                let parameter = prev..n;
                if parameter.is_empty() {
                    return Err(OldError::EmptyQueryParameter);
                }

                let (key, value) = match memchr::memchr(b'=', &query[parameter]) {
                    None => return Err(OldError::InvalidQuery),
                    Some(m) => (prev..prev + m, prev + m + 1..n),
                };

                if key.is_empty() {
                    return Err(OldError::MissingQueryKey);
                }

                if value.is_empty() {
                    return Err(OldError::MissingQueryValue);
                }

                for i in key {
                    if !super::old::is_valid_unreserved(query[i]) {
                        return Err(OldError::ReservedCharacter(query[i]));
                    }
                }

                for i in value {
                    if !super::old::is_valid_unreserved(query[i]) {
                        return Err(OldError::ReservedCharacter(query[i]));
                    }
                }

                prev = n + 1;
            }
        }

        Ok(Self { backing: query })
    }

    /// Returns an iterator over the query's parameters.
    #[must_use]
    pub fn iter(&self) -> Iter<'_, 'data> {
        Iter {
            query: self,
            prev: 0,
        }
    }

    /// Returns the number of bytes in the query. Keep in mind that with percent-encoding this
    /// might not be the same as the number of _characters_ contained in the query.
    #[must_use]
    fn len(&self) -> usize {
        self.backing.len()
    }
}

impl<'query, 'data> IntoIterator for &'query Query<'data>
where
    'data: 'query,
{
    type Item = QueryParameter<'data>;
    type IntoIter = Iter<'query, 'data>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl std::fmt::Debug for Query<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field(
                "query",
                &std::str::from_utf8(self.backing).unwrap_or_default(),
            )
            .finish()
    }
}

impl std::fmt::Display for Query<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::str::from_utf8(self.backing).unwrap_or_default())
    }
}

/// A single parameter in a `x-www-form-urlencoded` [`Query`].
#[derive(PartialEq, Eq)]
pub struct QueryParameter<'data> {
    /// Parameter key.
    pub key: &'data [u8],
    /// Parameter value.
    pub val: &'data [u8],
}

impl std::fmt::Debug for QueryParameter<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = std::str::from_utf8(self.key).unwrap_or_default();
        let value = std::str::from_utf8(self.val).unwrap_or_default();

        f.debug_map()
            .entry(&"key", &key)
            .entry(&"value", &value)
            .finish()
    }
}

/// An iterator over each [`QueryParameter`]s in a `x-www-form-urlencoded` [`Query`].
#[derive(Debug)]
pub struct Iter<'query, 'data>
where
    'data: 'query,
{
    query: &'query Query<'data>,
    prev: usize,
}

impl<'query, 'data> Iterator for Iter<'query, 'data>
where
    'data: 'query,
{
    type Item = QueryParameter<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let parameter = if let Some(n) = memchr::memchr(b'&', &self.query.backing[self.prev..]) {
            self.prev..self.prev + n
        } else if self.prev < self.query.len() {
            self.prev..self.query.len()
        } else {
            return None;
        };

        let (key, value) = match memchr::memchr(b'=', &self.query.backing[parameter.clone()]) {
            None => unreachable!(),
            Some(m) => (self.prev..self.prev + m, self.prev + m + 1..parameter.end),
        };

        self.prev = (parameter.end + 1).min(self.query.len());

        Some(QueryParameter {
            key: &self.query.backing[key],
            val: &self.query.backing[value],
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prelude::*;

    #[test]
    fn url_query_simple() {
        let query = Query::new(b"a=12&bcd=2&ef=345").expect("Should be a valid query");
        let mut iter = query.iter();

        let a = iter.next().unwrap();
        assert_str_eq!(a.key, b"a");
        assert_str_eq!(a.val, b"12");

        let bcd = iter.next().unwrap();
        assert_str_eq!(bcd.key, b"bcd");
        assert_str_eq!(bcd.val, b"2");

        let ef = iter.next().unwrap();
        assert_str_eq!(ef.key, b"ef");
        assert_str_eq!(ef.val, b"345");

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn url_query_single_parameter() {
        let query = Query::new(b"a=1").expect("Should be a valid query");
        let mut iter = query.iter();

        let a = iter.next().unwrap();
        assert_eq!(a.key, b"a");
        assert_eq!(a.val, b"1");

        assert_eq!(iter.next(), None);
    }

    #[test]
    fn url_query_empty() {
        let query = Query::new(b"").expect("Should be a valid query");
        assert_eq!(query.iter().next(), None);
    }

    #[test]
    fn url_query_err_empty_parameter() {
        assert_eq!(Query::new(b"&"), Err(OldError::EmptyQueryParameter));
    }

    #[test]
    fn url_query_err_invalid() {
        assert_eq!(Query::new(b"a1"), Err(OldError::InvalidQuery));
    }

    #[test]
    fn url_query_err_missing_key() {
        assert_eq!(Query::new(b"=1"), Err(OldError::MissingQueryKey));
    }

    #[test]
    fn url_query_err_missing_value() {
        assert_eq!(Query::new(b"a="), Err(OldError::MissingQueryValue));
    }

    #[test]
    fn url_query_err_reserved_character_in_key() {
        assert_eq!(Query::new(b"%=1"), Err(OldError::ReservedCharacter(b'%')));
    }

    #[test]
    fn url_query_err_reserved_character_in_val() {
        assert_eq!(Query::new(b"a=%"), Err(OldError::ReservedCharacter(b'%')));
    }
}
