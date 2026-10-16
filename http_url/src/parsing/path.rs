use macro_util::prelude::*;

use crate::Error;
use crate::percent;

pub(super) struct PathStack<'parsing> {
    segment_start: *const u8,
    segment_stop: *mut u8,
    segment_sizes: *mut usize,

    _phantom: std::marker::PhantomData<&'parsing mut [u8]>,
}

impl<'parsing> PathStack<'parsing> {
    pub(super) fn new(backing: &'parsing mut [u8]) -> Result<Self, Error> {
        if backing.is_empty() {
            return Err(Error::OOM);
        }

        let std::ops::Range { start, end } = backing.as_mut_ptr_range();

        // SAFETY: `backing` is checked above to be non-empty.
        let segment_start = unsafe { start.add(1) };
        let segment_stop = segment_start;

        let align = std::mem::align_of::<usize>();

        // Alignment failure returns `usize::MAX`
        let offset_add = match end.align_offset(align) {
            usize::MAX => return Err(Error::OOM),
            n => n,
        };

        assert_greq!(align, offset_add);

        let offset_sub = align - offset_add;

        // SAFETY: we check below to make sure `tail` does not index into invalid memory.
        let tail = unsafe { end.sub(offset_sub).cast::<usize>().sub(1) };

        assert!(tail.is_aligned());

        if tail.addr() < segment_start.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `backing` is checked above to be non-empty, so `size` must point to initialized
        // memory. We take this invariant one step further and only commit to changes if there is
        // also enough space for at least one segment size.
        unsafe { *start = b'/' };

        // SAFETY: `tail` is guaranteed to point to valid memory.
        unsafe { *tail = 0 };

        Ok(Self {
            segment_start,
            segment_stop,
            segment_sizes: tail,
            _phantom: std::marker::PhantomData,
        })
    }

    pub(super) fn segment_push_byte(&mut self, c: u8) -> Result<(), Error> {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        // SAFETY: we check below to make sure `head_new` does not index into invalid memory.
        let head_new = unsafe { self.segment_stop.add(1) };

        // We must not overwrite the tail!
        if head_new.addr() >= self.segment_sizes.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `head` is bounded by `tail` and the start of the backing slice and cannot point
        // into invalid memory.
        unsafe { *self.segment_stop = c };

        self.segment_stop = head_new;

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into invalid memory.
        unsafe { *self.segment_sizes += 1 };

        Ok(())
    }

    pub(super) fn segment_push_str(&mut self, bytes: &[u8]) -> Result<(), Error> {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        // SAFETY: we check below to make sure `head_new` does not index into invalid memory.
        let head_new = unsafe { self.segment_stop.add(bytes.len()) };

        // We must not overwrite the tail!
        if head_new.addr() >= self.segment_sizes.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `head + bytes.len()` is bounded by `tail` and the start of the backing slice and
        // cannot point into invalid memory.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.segment_stop, bytes.len()) };

        self.segment_stop = head_new;

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into invalid memory.
        unsafe { *self.segment_sizes += bytes.len() };

        Ok(())
    }

    pub(super) fn segment_push_encode(
        &mut self,
        c: u8,
        set: percent::EncodeSet,
    ) -> Result<(), Error> {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        if set.should_percent_encode(c) {
            self.segment_push_str(percent::encode_ascii_byte(c))
        } else {
            self.segment_push_byte(c)
        }
    }

    pub(super) fn segment_peek(&self) -> &[u8] {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into invalid memory.
        let size = unsafe { *self.segment_sizes };

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it. This means we
        // cannot subtract more than the number of bytes already appended to the `head`, therefore
        // this cannot index into invalid memory.
        let start = unsafe { self.segment_stop.sub(size) };

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it.
        unsafe { std::slice::from_raw_parts(start, size) }
    }

    pub(super) fn segment_pop(&mut self) {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        // We only pop if we have segments lefts, otherwise we would be incrementing `segment_stop`
        // into invalid memory!
        if self.segment_stop.addr() <= self.segment_start.addr() {
            return;
        }

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into invalid memory.
        let size = unsafe { *self.segment_sizes };

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it. This means we
        // cannot subtract more than the number of bytes already appended to the `head`, therefore
        // this cannot index into invalid memory.
        unsafe { self.segment_stop = self.segment_stop.sub(size) };

        // SAFETY: `tail` is already guaranteed above to point into valid memory
        unsafe { *self.segment_sizes = 0 };

        // SAFETY: `tail` is only popped if `size > 0`, and only the first `/` segment has size 0.
        unsafe { self.segment_sizes = self.segment_sizes.add(1) };
    }

    pub(super) fn segment_next(&mut self) -> Result<(), Error> {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        // SAFETY: we check below to make sure `tail` points into valid memory.
        unsafe { self.segment_sizes = self.segment_sizes.sub(1) };

        if self.segment_stop.addr() >= self.segment_sizes.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `tail` is already guaranteed above to point into valid memory
        unsafe { *self.segment_sizes = 0 };

        Ok(())
    }

    pub(super) fn len(&self) -> usize {
        assert!(self.segment_start.addr() <= self.segment_stop.addr());
        assert!(self.segment_stop.addr() < self.segment_sizes.addr());

        self.segment_stop.addr() - self.segment_start.addr() + 1
    }
}

impl AsRef<[u8]> for PathStack<'_> {
    fn as_ref(&self) -> &[u8] {
        // SAFETY: `segment_start` is incremented by 1 on initialization to account for the added
        // U+002F (/) code point. This simply undoes that work to include it in the resulting slice.
        let start = unsafe { self.segment_start.sub(1) };
        let size = self.segment_stop.addr() - start.addr();

        // SAFETY: `segment_start` and `segment_stop` are bounded by the backing memory provided to
        // PathStack
        unsafe { std::slice::from_raw_parts(start, size) }
    }
}

#[cfg(test)]
mod test {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[test]
    fn path_stack_simple() {
        // let mut backing = [0; 111];
        let mut backing: [u8; 111] = std::array::from_fn(|i| i as u8);
        let mut path_stack = PathStack::new(&mut backing).unwrap();

        for c in b"Documents/" {
            path_stack.segment_push_byte(*c).unwrap();
        }

        assert_utf8_eq!(path_stack.segment_peek(), b"Documents/");

        path_stack.segment_next().unwrap();

        path_stack.segment_push_str(b"code/").unwrap();

        assert_utf8_eq!(path_stack.segment_peek(), b"code/");

        path_stack.segment_next().unwrap();

        for c in b"main.rs" {
            path_stack.segment_push_byte(*c).unwrap();
        }

        assert_utf8_eq!(path_stack.segment_peek(), b"main.rs");

        assert_utf8_eq!(path_stack.as_ref(), b"/Documents/code/main.rs");
        assert_eq!(path_stack.len(), b"/Documents/code/main.rs".len());

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_ref(), b"/Documents/code/");
        assert_eq!(path_stack.len(), b"/Documents/code/".len());

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_ref(), b"/Documents/");
        assert_eq!(path_stack.len(), b"/Documents/".len());

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_ref(), b"/");
        assert_eq!(path_stack.len(), b"/".len());

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_ref(), b"/");
        assert_eq!(path_stack.len(), b"/".len());
    }

    #[test]
    fn path_stack_empty() {
        let mut backing = *b"/Documents/code/main.rs";
        let mut path_stack = PathStack::new(&mut backing).unwrap();

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.segment_peek(), b"");
        assert_utf8_eq!(path_stack.as_ref(), b"/");
        assert_utf8_eq!(path_stack.as_ref(), b"/");
        assert_eq!(path_stack.len(), b"/".len());
        assert!(path_stack.segment_stop.addr() <= path_stack.segment_sizes.addr());
    }

    #[test]
    fn path_stack_push_encode() {
        let mut backing = [0; 32];
        let mut path_stack = PathStack::new(&mut backing).unwrap();

        path_stack
            .segment_push_encode(b' ', percent::FRAGMENT)
            .unwrap();

        assert_utf8_eq!(path_stack.segment_peek(), b"%20");

        path_stack
            .segment_push_encode(b'a', percent::FRAGMENT)
            .unwrap();

        assert_utf8_eq!(path_stack.segment_peek(), b"%20a");
    }
}
