#[cfg(feature = "nightly")]
use std::alloc::AllocError;
#[cfg(feature = "nightly")]
use std::alloc::Allocator;

#[cfg(not(feature = "nightly"))]
use allocator_api2::alloc::AllocError;
#[cfg(not(feature = "nightly"))]
use allocator_api2::alloc::Allocator;

pub struct Arena {
    alloc: region::Allocation,
    inner: std::cell::Cell<ArenaInner>,
}

#[derive(Clone, Copy)]
struct ArenaInner {
    head: std::ptr::NonNull<u8>,
    prev: Option<std::ptr::NonNull<u8>>,
}

impl Arena {
    pub fn new(size: usize) -> Result<Self, std::io::Error> {
        let mut alloc = match region::alloc(size, region::Protection::READ_WRITE) {
            Ok(alloc) => alloc,
            Err(err) => return Err(err.into_io_error()),
        };

        // SAFETY: allocation succeeded, therefore `start` cannot be Null.
        let head = unsafe { std::ptr::NonNull::new_unchecked(alloc.as_mut_ptr()) };
        let prev = None;

        Ok(Self {
            alloc,
            inner: std::cell::Cell::new(ArenaInner { head, prev }),
        })
    }
}

unsafe impl Allocator for Arena {
    fn allocate(&self, layout: std::alloc::Layout) -> Result<std::ptr::NonNull<[u8]>, AllocError> {
        let align = layout.align();
        let len = layout.size();

        let inner = self.inner.get();
        let head = inner.head.as_ptr() as usize;
        let offset = inner.head.align_offset(align);

        let aligned = match head.checked_add(offset) {
            Some(ptr) => ptr,
            None => return Err(AllocError),
        };

        let head_bumb = match aligned.checked_add(len) {
            Some(ptr) => ptr,
            None => return Err(AllocError),
        };

        assert_ne!(aligned, 0);
        assert_ne!(head_bumb, 0);

        // SAFETY: the arena's backing array was successfully initialized and therefore cannot be
        // null, and the checked math above ensure that we cannot wrap-around back to a null value.
        let (head_new, head_prev) = unsafe {
            (
                std::ptr::NonNull::new_unchecked(head_bumb as *mut u8),
                std::ptr::NonNull::new_unchecked(aligned as *mut u8),
            )
        };

        let start = self.alloc.as_ptr::<u8>().addr();
        if head_new.as_ptr().addr() - start > self.alloc.len() {
            return Err(AllocError);
        }

        self.inner.set(ArenaInner {
            head: head_new,
            prev: Some(head_prev),
        });

        Ok(std::ptr::NonNull::slice_from_raw_parts(head_prev, len))
    }

    unsafe fn deallocate(&self, _ptr: std::ptr::NonNull<u8>, _layout: std::alloc::Layout) {}

    unsafe fn grow(
        &self,
        ptr_old: std::ptr::NonNull<u8>,
        layout_old: std::alloc::Layout,
        layout_new: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, AllocError> {
        let len_old = layout_old.size();
        let len_new = layout_new.size();

        assert!(len_old >= len_new);

        let inner = self.inner.get();

        if let Some(ptr_prev) = inner.prev
            && ptr_prev == ptr_old
            && layout_old.align() == layout_new.align()
        {
            let head = inner.head.as_ptr() as usize;
            let head_bump = match head.checked_add(len_new - len_old) {
                Some(ptr) => ptr,
                None => return Err(AllocError),
            };

            assert_ne!(head_bump, 0);

            // SAFETY: the arena's backing array was successfully initialized and therefore cannot
            // be null, and the checked math above ensure that we cannot wrap-around back to a null
            // value.
            let head_new = unsafe { std::ptr::NonNull::new_unchecked(head_bump as *mut u8) };

            let start = self.alloc.as_ptr::<u8>().addr();
            if head_new.as_ptr().addr() - start > self.alloc.len() {
                return Err(AllocError);
            }

            self.inner.set(ArenaInner {
                head: head_new,
                prev: inner.prev,
            });

            Ok(std::ptr::NonNull::slice_from_raw_parts(ptr_old, len_new))
        } else {
            let ptr_new = self.allocate(layout_new)?;

            unsafe {
                let src = ptr_old.as_ptr();
                let dst = ptr_new.cast().as_ptr();
                let count = layout_old.size();

                std::ptr::copy_nonoverlapping(src, dst, count);
            }

            Ok(ptr_new)
        }
    }
}

#[cfg(test)]
mod test {
    #[cfg(feature = "nightly")]
    use std::boxed::Box;

    #[cfg(not(feature = "nightly"))]
    use allocator_api2::boxed::Box;

    use super::*;

    #[test]
    fn simple_alloc() {
        let arena = Arena::new(64).unwrap();

        let a = Box::new_in('a', &arena);
        let b = Box::new_in('b', &arena);
        let c = Box::new_in('c', &arena);

        assert_eq!(a.as_ref(), &'a');
        assert_eq!(b.as_ref(), &'b');
        assert_eq!(c.as_ref(), &'c');
    }
}
