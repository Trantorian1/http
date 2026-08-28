use super::Error;

#[derive(PartialEq, Eq)]
pub struct Query<'data> {
    query: &'data [u8],
}

impl<'data> Query<'data> {
    pub(super) fn new(query: &'data [u8]) -> Result<Self, Error> {
        if !query.is_empty() {
            let mut prev = 0;
            for n in memchr::memchr_iter(b'&', query).chain(std::iter::once(query.len())) {
                let parameter = prev..n;
                if parameter.is_empty() {
                    return Err(Error::EmptyQueryParameter);
                }

                let (key, value) = match memchr::memchr(b'=', &query[parameter]) {
                    None => return Err(Error::InvalidQuery),
                    Some(m) => (prev..prev + m, prev + m + 1..n),
                };

                if key.is_empty() {
                    return Err(Error::MissingQueryKey);
                }

                if value.is_empty() {
                    return Err(Error::MissingQueryValue);
                }

                for i in key {
                    if !super::unreserved(query[i]) {
                        return Err(Error::ReservedCharacter(query[i]));
                    }
                }

                for i in value {
                    if !super::unreserved(query[i]) {
                        return Err(Error::ReservedCharacter(query[i]));
                    }
                }

                prev = n + 1;
            }
        }

        Ok(Self { query })
    }

    pub fn iter(&self) -> Iter<'data> {
        Iter {
            query: self.query,
            prev: 0,
        }
    }
}

impl<'data> std::fmt::Debug for Query<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Query")
            .field(
                "query",
                &std::str::from_utf8(self.query).unwrap_or_default(),
            )
            .finish()
    }
}

impl<'data> std::fmt::Display for Query<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(std::str::from_utf8(self.query).unwrap_or_default())
    }
}

#[derive(PartialEq, Eq)]
pub struct QueryParameter<'data> {
    pub key: &'data [u8],
    pub val: &'data [u8],
}

impl<'data> std::fmt::Debug for QueryParameter<'data> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let key = std::str::from_utf8(self.key).unwrap_or_default();
        let value = std::str::from_utf8(self.val).unwrap_or_default();

        f.debug_map()
            .entry(&"key", &key)
            .entry(&"value", &value)
            .finish()
    }
}

pub struct Iter<'data> {
    query: &'data [u8],
    prev: usize,
}

impl<'data> Iterator for Iter<'data> {
    type Item = QueryParameter<'data>;

    fn next(&mut self) -> Option<Self::Item> {
        let parameter = if let Some(n) = memchr::memchr(b'&', &self.query[self.prev..]) {
            self.prev..self.prev + n
        } else if self.prev < self.query.len() {
            self.prev..self.query.len()
        } else {
            return None;
        };

        let (key, value) = match memchr::memchr(b'=', &self.query[parameter.clone()]) {
            None => unreachable!(),
            Some(m) => (self.prev..self.prev + m, self.prev + m + 1..parameter.end),
        };

        self.prev = (parameter.end + 1).min(self.query.len());

        Some(QueryParameter {
            key: &self.query[key],
            val: &self.query[value],
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
        assert_streq!(a.key, b"a");
        assert_streq!(a.val, b"12");

        let bcd = iter.next().unwrap();
        assert_streq!(bcd.key, b"bcd");
        assert_streq!(bcd.val, b"2");

        let ef = iter.next().unwrap();
        assert_streq!(ef.key, b"ef");
        assert_streq!(ef.val, b"345");

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
        assert_eq!(query.iter().next(), None)
    }

    #[test]
    fn url_query_err_empty_parameter() {
        assert_eq!(Query::new(b"&"), Err(Error::EmptyQueryParameter));
    }

    #[test]
    fn url_query_err_invalid() {
        assert_eq!(Query::new(b"a1"), Err(Error::InvalidQuery));
    }

    #[test]
    fn url_query_err_missing_key() {
        assert_eq!(Query::new(b"=1"), Err(Error::MissingQueryKey));
    }

    #[test]
    fn url_query_err_missing_value() {
        assert_eq!(Query::new(b"a="), Err(Error::MissingQueryValue));
    }

    #[test]
    fn url_query_err_reserved_character_in_key() {
        assert_eq!(Query::new(b"%=1"), Err(Error::ReservedCharacter(b'%')))
    }

    #[test]
    fn url_query_err_reserved_character_in_val() {
        assert_eq!(Query::new(b"a=%"), Err(Error::ReservedCharacter(b'%')))
    }
}
