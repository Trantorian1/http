use macro_util::prelude::*;

use crate::Error;
use crate::percent;

pub(super) struct PathStack<'parsing> {
    // write-then-update
    head: *mut u8,

    // update-then-write
    tail: *mut usize,

    // Counts the number of path segments
    size: usize,

    _phantom: std::marker::PhantomData<&'parsing mut [u8]>,
}

impl<'parsing> PathStack<'parsing> {
    pub(super) fn new(backing: &'parsing mut [u8]) -> Result<Self, Error> {
        if backing.is_empty() {
            return Err(Error::OOM);
        }

        let std::ops::Range { start, end } = backing.as_mut_ptr_range();

        // SAFETY: `backing` is checked above to be non-empty.
        let head = unsafe { start.add(1) };

        let align = std::mem::align_of::<usize>();

        // Alignment failure returns `usize::MAX`
        let offset_add = match end.align_offset(align) {
            usize::MAX => return Err(Error::OOM),
            n => n,
        };

        assert_greq!(align, offset_add);

        let offset_sub = align - offset_add;

        // SAFETY: we check below to make sure `tail` does not index into uninitialized memory.
        let tail = unsafe { end.sub(offset_sub).cast::<usize>().sub(1) };

        assert!(tail.is_aligned());

        if tail.addr() < head.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `backing` is checked above to be non-empty, so `size` must point to initialized
        // memory. We take this invariant one step further and only commit to changes if there is
        // also enough space for at least one segment size.
        unsafe { *start = b'/' };

        // SAFETY: `tail` is guaranteed to point to initialized memory.
        unsafe { *tail = 0 };

        Ok(Self {
            head,
            tail,
            size: 1,
            _phantom: std::marker::PhantomData,
        })
    }

    pub(super) fn segment_push_byte(&mut self, c: u8) -> Result<(), Error> {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: we check below to make sure `head_new` does not index into uninitialized memory.
        let head_new = unsafe { self.head.add(1) };

        // We must not overwrite the tail!
        if head_new.addr() >= self.tail.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `head` is bounded by `tail` and the start of the backing slice and cannot point
        // into uninitialized memory.
        unsafe { *self.head = c };

        self.head = head_new;

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into uninitialized memory.
        unsafe { *self.tail += 1 };

        Ok(())
    }

    pub(super) fn segment_push_str(&mut self, bytes: &[u8]) -> Result<(), Error> {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: we check below to make sure `head_new` does not index into uninitialized memory.
        let head_new = unsafe { self.head.add(bytes.len()) };

        // We must not overwrite the tail!
        if head_new.addr() >= self.tail.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: `head + bytes.len()` is bounded by `tail` and the start of the backing slice and
        // cannot point into uninitialized memory.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), self.head, bytes.len()) };

        self.head = head_new;

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into uninitialized memory.
        unsafe { *self.tail += bytes.len() };

        Ok(())
    }

    pub(super) fn segment_push_encode(
        &mut self,
        c: u8,
        set: percent::EncodeSet,
    ) -> Result<(), Error> {
        assert!(self.head.addr() < self.tail.addr());

        if set.should_percent_encode(c) {
            self.segment_push_str(percent::encode_ascii_byte(c))
        } else {
            self.segment_push_byte(c)
        }
    }

    pub(super) fn segment_peek(&self) -> &[u8] {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into uninitialized memory.
        let size = unsafe { *self.tail };

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it. This means we
        // cannot subtract more than the number of bytes already appended to the `head`, therefore
        // this cannot index into uninitialized memory.
        let start = unsafe { self.head.sub(size) };

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it.
        unsafe { std::slice::from_raw_parts(start, size) }
    }

    pub(super) fn segment_pop(&mut self) {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: `tail` is bounded by `head` and the end of the backing slice and cannot point
        // into uninitialized memory.
        let size = unsafe { *self.tail };

        if size == 0 {
            return;
        }

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it. This means we
        // cannot subtract more than the number of bytes already appended to the `head`, therefore
        // this cannot index into uninitialized memory.
        unsafe { self.head = self.head.sub(size) };

        // SAFETY: `tail` is already guaranteed above to point into initialized memory
        unsafe { *self.tail = 0 };

        // SAFETY: todo
        unsafe { self.tail = self.tail.add(1) };

        self.size -= 1;
    }

    pub(super) fn segment_next(&mut self) -> Result<(), Error> {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: we check below to make sure `tail` points into initialized memory.
        unsafe { self.tail = self.tail.sub(1) };

        if self.head.addr() >= self.tail.addr() {
            return Err(Error::OOM);
        }

        self.size += 1;

        Ok(())
    }

    pub(super) fn as_segment_sizes(&self) -> &[usize] {
        assert!(self.head.addr() < self.tail.addr());

        // SAFETY: the number of segments is kept in sync with the tail
        unsafe { std::slice::from_raw_parts(self.tail, self.size) }
    }

    #[allow(unused)]
    pub(super) fn as_segment_paths(&self) -> &[u8] {
        assert!(self.head.addr() < self.tail.addr());

        let size = self.len();

        // SAFETY: a segment's size is kept in sync as new bytes are appended to it. This means we
        // cannot subtract more than the number of bytes already appended to the `head`, therefore
        // this cannot index into uninitialized memory. The extra ``+1` accounts for the added `/`
        // at the start of the path.
        let start = unsafe { self.head.sub(size) };

        // SAFETY: the number of segments is kept in sync with the head
        unsafe { std::slice::from_raw_parts(start, size) }
    }

    pub(super) fn len(&self) -> usize {
        assert!(self.head.addr() < self.tail.addr());

        let mut size = 0;
        for n in self.as_segment_sizes() {
            size += n;
        }

        size + 1
    }
}

#[cfg(test)]
mod test {
    #[allow(clippy::wildcard_imports)]
    use super::*;

    #[test]
    fn path_stack_simple() {
        let mut backing = [0; 111];
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

        assert_eq!(
            path_stack.as_segment_sizes(),
            &[b"main.rs".len(), b"code/".len(), b"Documents/".len()]
        );

        assert_utf8_eq!(path_stack.as_segment_paths(), b"/Documents/code/main.rs");

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_segment_paths(), b"/Documents/code/");

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_segment_paths(), b"/Documents/");

        path_stack.segment_pop();

        assert_utf8_eq!(path_stack.as_segment_paths(), b"/");
    }

    #[test]
    fn path_stack_empty() {
        let mut backing = *b"/Documents/code/main.rs";
        let path_stack = PathStack::new(&mut backing).unwrap();

        assert_utf8_eq!(path_stack.segment_peek(), b"");
        assert_utf8_eq!(path_stack.as_segment_paths(), b"/");
        assert_eq!(path_stack.as_segment_sizes(), &[0]);
        assert!(path_stack.head.addr() <= path_stack.tail.addr());
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
