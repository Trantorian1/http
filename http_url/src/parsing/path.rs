use macro_util::prelude::*;

use super::Error;
use crate::macros::*;

pub(crate) struct PathStack<'parsing> {
    // write-then-update
    head: *mut u8,

    // update-then-write
    tail: *mut usize,

    // Counts the number of path segments
    size: usize,

    _phantom: std::marker::PhantomData<&'parsing mut [u8]>,
}

impl<'parsing> PathStack<'parsing> {
    pub(crate) fn new(backing: &'parsing mut [u8]) -> Result<Self, Error> {
        if backing.is_empty() {
            return Err(Error::OOM);
        }

        let std::ops::Range { start, end } = backing.as_mut_ptr_range();

        // SAFETY: todo
        let head = unsafe { start.add(1) };

        let align = std::mem::align_of::<usize>();

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

    fn segment_push(&mut self, c: u8) -> Result<(), Error> {
        let head_prev = self.head;

        // SAFETY: todo
        unsafe { self.head = self.head.add(1) };

        // We cannot write at an address that would have been the tail
        if self.head.addr() > self.tail.addr() {
            return Err(Error::OOM);
        }

        // SAFETY: todo
        unsafe { *head_prev = c };

        // SAFETY: `tail` is <= than then end of the backing memory segment and greater than `head`,
        // which is itself >= to the end of the backing memory segment, and is therefore guaranteed
        // to point to initialized memory.
        unsafe { *self.tail += 1 };

        Ok(())
    }

    fn segment_peek(&self) -> &[u8] {
        // SAFETY: todo
        let size = unsafe { *self.tail };

        // SAFETY: todo
        let start = unsafe { self.head.sub(size) };

        // SAFETY: todo
        unsafe { std::slice::from_raw_parts(start, size) }
    }

    fn segment_pop(&mut self) {
        if self.size == 0 {
            return;
        }

        // SAFETY: todo
        let size = unsafe { *self.tail };

        // SAFETY: todo
        self.head = unsafe { self.head.sub(size) };

        // SAFETY: todo
        self.tail = unsafe { self.tail.add(1) };

        self.size -= 1;
    }

    fn segment_next(&mut self) -> Result<(), Error> {
        // SAFETY: we check below to make sure `tail` points into initialized memory.
        unsafe { self.tail = self.tail.sub(1) };

        if self.head.addr() >= self.tail.addr() {
            return Err(Error::OOM);
        }

        self.size += 1;

        Ok(())
    }

    fn as_segment_sizes(&self) -> &[usize] {
        // SAFETY: todo
        unsafe { std::slice::from_raw_parts(self.tail, self.size) }
    }

    fn as_segment_paths(&self) -> &[u8] {
        let mut size = 0;
        for n in self.as_segment_sizes() {
            size += n;
        }

        // SAFETY: gnrjkgbkrbnrkj
        let start = unsafe { self.head.sub(size + 1) };

        // SAFETY: todo
        unsafe { std::slice::from_raw_parts(start, size + 1) }
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
            path_stack.segment_push(*c).unwrap();
        }

        assert_utf8_eq!(path_stack.segment_peek(), b"Documents/");

        path_stack.segment_next().unwrap();

        for c in b"code/" {
            path_stack.segment_push(*c).unwrap();
        }

        assert_utf8_eq!(path_stack.segment_peek(), b"code/");

        path_stack.segment_next().unwrap();

        for c in b"main.rs" {
            path_stack.segment_push(*c).unwrap();
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
    }
}
