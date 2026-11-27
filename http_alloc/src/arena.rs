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
    head: std::cell::Cell<std::ptr::NonNull<u8>>,
}

impl Arena {
    pub fn new(size: usize) -> Result<Self, std::io::Error> {
        assert_ne!(size, 0);

        let mut alloc = match region::alloc(size, region::Protection::READ_WRITE) {
            Ok(alloc) => alloc,
            Err(err) => return Err(err.into_io_error()),
        };

        // SAFETY: allocation succeeded, therefore `start` cannot be Null.
        let head = unsafe { std::ptr::NonNull::new_unchecked(alloc.as_mut_ptr()) };

        Ok(Self {
            alloc,
            head: std::cell::Cell::new(head),
        })
    }

    pub fn available(&self) -> usize {
        let range = self.alloc.as_range();
        let head = self.head.get().addr().get();

        assert!(range.contains(&head));

        range.end - head
    }

    pub fn capacity(&self) -> usize {
        self.alloc.len()
    }

    /// # Panics
    ///
    /// If this method is called while the arena has no outstanding allocation left.
    fn head_prev(&self) -> std::ptr::NonNull<u8> {
        assert_ne!(self.head.get().addr().get(), self.alloc.as_range().start);

        let head = self.head.get();

        // SAFETY: each allocation is succeeded by short usize write containing the address to the
        // previous head of the arena. This head is always aligned to `usize`.
        let head_ptr = unsafe { head.cast::<usize>().sub(1) };

        assert!(self.alloc.as_range().contains(&head_ptr.addr().get()));

        // SAFETY: each allocation writes the address of the previous head of the arena after the
        // allocated block of memory. The caller is responsible for ensuring this method is only
        // called after at least one allocation has taken place.
        let head_prev = unsafe { head_ptr.read() };

        assert_ne!(head_prev, 0);
        assert!(self.alloc.as_range().contains(&head_prev));

        // SAFETY: head_prev contains the previous head of the arena, which can never be null.
        unsafe { std::ptr::NonNull::new_unchecked(head.as_ptr().with_addr(head_prev)) }
    }
}

unsafe impl Allocator for Arena {
    fn allocate(
        &self,
        layout_alloc: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, AllocError> {
        let cell = self.head.get();
        let ptr = cell.as_ptr();
        let head = ptr.addr();

        assert!(self.alloc.as_range().contains(&head));

        let alloc_align = cell.align_offset(layout_alloc.align());
        let alloc_len = layout_alloc.size();

        let alloc_start = match head.checked_add(alloc_align) {
            Some(addr) => addr,
            None => return Err(AllocError),
        };

        let alloc_stop = match alloc_start.checked_add(layout_alloc.size()) {
            Some(addr) => addr,
            None => return Err(AllocError),
        };

        assert_ne!(alloc_start, 0);
        assert_ne!(alloc_stop, 0);

        let alloc_start_ptr = ptr.with_addr(alloc_start);
        let alloc_stop_ptr = ptr.with_addr(alloc_stop);

        let size_layout = std::alloc::Layout::new::<usize>();
        let size_align = alloc_stop_ptr.align_offset(size_layout.align());
        let size_len = size_layout.size();

        let size_start = match alloc_stop.checked_add(size_align) {
            Some(addr) => addr,
            None => return Err(AllocError),
        };

        let size_stop = match size_start.checked_add(size_len) {
            Some(addr) => addr,
            None => return Err(AllocError),
        };

        assert_ne!(size_start, 0);
        assert_ne!(size_stop, 0);

        let size_start_ptr = ptr.with_addr(size_start).cast::<usize>();
        let size_stop_ptr = ptr.with_addr(size_stop);

        // SAFETY: the arena's backing array was successfully initialized and therefore cannot be
        // null, and the checked math above ensure that we cannot wrap-around back to a null value.
        let (alloc, head_new) = unsafe {
            (
                std::ptr::NonNull::new_unchecked(alloc_start_ptr),
                std::ptr::NonNull::new_unchecked(size_stop_ptr),
            )
        };

        let start = self.alloc.as_ptr::<u8>().addr();
        if head_new.as_ptr().addr() - start > self.alloc.len() {
            return Err(AllocError);
        }

        // SAFETY: size_start is ensured to be aligned before the write and the above check ensure
        // it lies withing allocated memory.
        unsafe { std::ptr::write(size_start_ptr, head) };

        assert!(self.alloc.as_range().contains(&alloc_start));
        assert!(self.alloc.as_range().contains(&alloc_stop));
        assert!(self.alloc.as_range().contains(&size_start));
        assert!(self.alloc.as_range().contains(&size_stop));

        self.head.set(head_new);

        Ok(std::ptr::NonNull::slice_from_raw_parts(alloc, alloc_len))
    }

    unsafe fn deallocate(&self, ptr: std::ptr::NonNull<u8>, layout: std::alloc::Layout) {
        assert!(self.alloc.as_range().contains(&ptr.addr().get()));

        // `head_prev` is a pointer to the previous head of the arena. To that needs to be added the
        // offset which was necessary to have written `ptr` in the first place.
        let head_prev = self.head_prev();
        let alloc_align = head_prev.align_offset(layout.align());
        let alloc_start = head_prev.addr().get().checked_add(alloc_align).unwrap_or(0);

        let _ptr = ptr.addr().get();

        // Head-allocation optimization: if the block of memory being deallocated is at the head of
        // the arena, we pop back the arena pointer to its previous value. This allows us to keep
        // benefiting from grow-in-place optimizations on multiple sequential collections as long as
        // we are careful about the way in which we allocate.
        if alloc_start == ptr.addr().get() {
            self.head.set(head_prev);
        }
    }

    unsafe fn grow(
        &self,
        ptr_prev: std::ptr::NonNull<u8>,
        layout_old: std::alloc::Layout,
        layout_new: std::alloc::Layout,
    ) -> Result<std::ptr::NonNull<[u8]>, AllocError> {
        assert!(self.alloc.as_range().contains(&ptr_prev.addr().get()));

        let len_old = layout_old.size();
        let len_new = layout_new.size();

        assert!(len_old <= len_new);

        // `head_prev` is a pointer to the previous head of the arena. To that needs to be added the
        // offset which was necessary to have written `ptr_prev` in the first place.
        let head_prev = self.head_prev();
        let alloc_align = head_prev.align_offset(layout_old.align());
        let alloc_start = head_prev.addr().get().checked_add(alloc_align).unwrap_or(0);

        // Head-allocation optimization: if the block of memory we want to grow is at the head of
        // the arena, we grow it in-place. This is important so that dynamically sized collections
        // like `Vec` don't end up polluting the arena on each size capacity increase.
        if alloc_start == ptr_prev.addr().get() && layout_old.align() == layout_new.align() {
            let cell = self.head.get();
            let ptr = cell.as_ptr();
            let head = ptr.addr();

            let alloc_stop = match head.checked_add(len_new - len_old) {
                Some(addr) => addr,
                None => return Err(AllocError),
            };

            assert!(self.alloc.as_range().contains(&head));
            assert_ne!(alloc_stop, 0);

            let size_layout = std::alloc::Layout::new::<usize>();
            let size_align = size_layout.align();
            let size_len = size_layout.size();

            let size_start = match alloc_stop.checked_add(size_align) {
                Some(addr) => addr,
                None => return Err(AllocError),
            };

            let size_stop = match size_start.checked_add(size_len) {
                Some(addr) => addr,
                None => return Err(AllocError),
            };

            assert_ne!(size_start, 0);
            assert_ne!(size_stop, 0);

            let size_start_ptr = ptr.with_addr(size_start).cast::<usize>();
            let size_stop_ptr = ptr.with_addr(size_stop);

            // SAFETY: the arena's backing array was successfully initialized and therefore cannot
            // be null, and the checked math above ensure that we cannot wrap-around back to a null
            // value.
            let head_new = unsafe { std::ptr::NonNull::new_unchecked(size_stop_ptr) };

            let start = self.alloc.as_ptr::<u8>().addr();
            if head_new.as_ptr().addr() - start > self.alloc.len() {
                return Err(AllocError);
            }

            // SAFETY: size_start is ensured to be aligned before the write and the above check
            // ensure it lies withing allocated memory.
            unsafe { std::ptr::write(size_start_ptr, head_prev.addr().get()) };

            assert!(self.alloc.as_range().contains(&alloc_stop));
            assert!(self.alloc.as_range().contains(&size_start));
            assert!(self.alloc.as_range().contains(&size_stop));

            self.head.set(head_new);

            Ok(std::ptr::NonNull::slice_from_raw_parts(ptr_prev, len_new))
        } else {
            let ptr_new = self.allocate(layout_new)?;

            // SAFETY: because `new_layout.size()` must be greater than or equal to
            // `old_layout.size()`, both the old and new memory allocation are valid for reads and
            // writes for `old_layout.size()` bytes. Also, because the old allocation wasn't yet
            // deallocated, it cannot overlap `new_ptr`. Thus, the call to `copy_nonoverlapping` is
            // safe. The safety contract for `dealloc` must be upheld by the caller.
            unsafe {
                let src = ptr_prev.as_ptr();
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
    #[cfg(feature = "nightly")]
    use std::vec::Vec;

    #[cfg(not(feature = "nightly"))]
    use allocator_api2::boxed::Box;
    #[cfg(not(feature = "nightly"))]
    use allocator_api2::vec::Vec;
    use macro_util::prelude::*;

    use super::*;

    /// Simple box alloc to make sure the arena doesn't contain any obvious mistakes.
    #[test]
    fn alloc_simple() {
        let arena = Arena::new(64).unwrap();

        let a = Box::new_in('a', &arena);
        let b = Box::new_in('b', &arena);
        let c = Box::new_in('c', &arena);

        assert_eq!(a.as_ref(), &'a');
        assert_eq!(b.as_ref(), &'b');
        assert_eq!(c.as_ref(), &'c');
    }

    /// The arena implements in-place growth for collections at the head of allocated memory.
    #[test]
    fn alloc_collection_grow_in_place_success() {
        let arena = Arena::new(64).unwrap();
        let mut vec = Vec::new_in(arena);

        assert_eq!(vec.capacity(), 0);

        // Vec hasn't allocated yet at this point.

        vec.push('a');
        vec.push('b');
        vec.push('c');
        vec.push('d');

        // Initial allocation of 4 elements, the next push will cause the vector to grow.

        let ptr_prev = vec.as_ptr();

        assert_eq!(vec.capacity(), 4);

        vec.push('e');
        vec.push('f');
        vec.push('g');
        vec.push('h');

        // Because of head-allocation optimization, the vector's backing pointer should not have
        // changed as a result of growing in-place.

        let ptr_new = vec.as_ptr();

        assert_eq!(vec.capacity(), 8);
        assert_eq!(ptr_prev, ptr_new);
    }

    /// In-place growth fails for collections which are NOT at the head of the arena.
    #[test]
    fn alloc_collection_grow_in_place_fail() {
        let arena = Arena::new(64).unwrap();
        let mut vec = Vec::new_in(&arena);

        assert_eq!(vec.capacity(), 0);

        // Vec hasn't allocated yet at this point.

        vec.push('a');
        vec.push('b');
        vec.push('c');
        vec.push('d');

        // Initial allocation of 4 elements, the next push will cause the vector to grow.

        let ptr_prev = vec.as_ptr();

        // Box allocation at the head of the arena, now the vector will not be able to grow
        // in-place.
        let _box = Box::new_in(42, &arena);

        assert_eq!(vec.capacity(), 4);

        vec.push('e');
        vec.push('f');
        vec.push('g');
        vec.push('h');

        // Vector is no longer at the head of the arena so it cannot grow in-place: its backing
        // pointer should have updated.

        let ptr_new = vec.as_ptr();

        assert_eq!(vec.capacity(), 8);
        assert_ne!(ptr_prev, ptr_new);
    }

    /// A previous collection can still be grown in-place after the head of the arena has been freed.
    #[test]
    fn alloc_collection_grow_in_place_after_free() {
        let arena = Arena::new(64).unwrap();
        let mut vec_1 = Vec::new_in(&arena);

        assert_eq!(vec_1.capacity(), 0);

        vec_1.push('a');
        vec_1.push('b');
        vec_1.push('c');
        vec_1.push('d');

        let ptr_prev_1 = vec_1.as_ptr();

        // `vec_2` takes the head of the arena
        {
            let mut vec_2 = Vec::new_in(&arena);

            assert_eq!(vec_2.capacity(), 0);

            vec_2.push(1);
            vec_2.push(2);
            vec_2.push(3);
            vec_2.push(4);

            let ptr_prev_2 = vec_2.as_ptr();

            assert_eq!(vec_2.capacity(), 4);

            vec_2.push(5);
            vec_2.push(6);
            vec_2.push(7);
            vec_2.push(8);

            let ptr_new_2 = vec_2.as_ptr();

            assert_eq!(vec_2.capacity(), 8);
            assert_eq!(ptr_prev_2, ptr_new_2);
        }
        // `vec_2` is freed, resetting the head of the arena to `vec_1`

        assert_eq!(vec_1.capacity(), 4);

        vec_1.push('e');
        vec_1.push('f');
        vec_1.push('g');
        vec_1.push('h');

        let ptr_new_1 = vec_1.as_ptr();

        assert_eq!(vec_1.capacity(), 8);
        assert_eq!(ptr_prev_1, ptr_new_1);
    }

    #[test]
    fn alloc_fail_allocate() {
        let arena = Arena::new(1).unwrap();

        let mut vec = Vec::<u8, _>::new_in(&arena);
        assert_matches!(vec.try_reserve(arena.available() + 1), Err(_));
    }

    #[test]
    fn alloc_fail_grow() {
        let arena = Arena::new(1).unwrap();

        let mut vec = Vec::new_in(&arena);

        assert_eq!(vec.capacity(), 0);

        // First allocation call is made to `allocate` directly
        vec.push(1);
        vec.push(2);
        vec.push(3);
        vec.push(4);
        // Subsequent allocation calls are made to `grow`

        assert_eq!(vec.capacity(), 4);

        assert_matches!(vec.try_reserve(arena.available() + 1), Err(_));
    }
}
