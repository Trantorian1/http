use macro_util::prelude::*;

/// Stack-allocated unsafe stack
pub(crate) struct Stack<T, const SIZE: usize> {
    backing: [std::mem::MaybeUninit<T>; SIZE],
    index: usize,
}

impl<T, const SIZE: usize> Stack<T, SIZE> {
    pub(crate) fn new() -> Self {
        Self {
            backing: std::array::from_fn(|_| std::mem::MaybeUninit::uninit()),
            index: 0,
        }
    }

    pub(crate) fn push(&mut self, elem: T) {
        assert_le!(
            self.index,
            SIZE,
            "Cannot push more than SIZE elements to a ParsingStack"
        );

        self.backing[self.index].write(elem);
        self.index += 1;
    }

    pub(crate) fn pop(&mut self) -> Option<T> {
        if self.index > 0 {
            self.index -= 1;
            // SAFETY: index is kept in sync with stack push/pop operations and therefore only
            // points to initialized memory. Moreover, once this value has been popped, the only way
            // for it to be read again is if it is overwritten, which guards against double reads.
            unsafe { Some(self.backing[self.index].assume_init_read()) }
        } else {
            None
        }
    }

    pub(crate) fn clear(&mut self) {
        self.index = 0;
    }
}

impl<T, const SIZE: usize> PartialEq for Stack<T, SIZE>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        if self.index == other.index {
            for i in 0..self.index {
                // SAFETY: index is kept in sync with stack push/pop operations and therefore only
                // points to initialized memory. Where index - 1 points to the last written slot,
                // i < index guarantees that i points to initialized memory.
                unsafe {
                    let left = self.backing[i].assume_init_ref();
                    let right = other.backing[i].assume_init_ref();

                    if left != right {
                        return false;
                    }
                };
            }
            true
        } else {
            false
        }
    }
}

impl<T, const SIZE: usize> Eq for Stack<T, SIZE> where T: Eq {}

impl<T, const SIZE: usize> std::fmt::Debug for Stack<T, SIZE>
where
    T: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries(
                self.backing
                    .iter()
                    .take(self.index)
                    // SAFETY: index is kept in sync with stack push/pop operations. As we only read
                    // the first `index` elements in the backing array this is guaranteed to only
                    // point to initialized memory.
                    .map(|elem| unsafe { elem.assume_init_ref() }),
            )
            .finish()
    }
}

impl<T, const SIZE: usize> Drop for Stack<T, SIZE> {
    fn drop(&mut self) {
        for elem in &mut self.backing[..self.index] {
            // SAFETY: index is kept in sync with stack push/pop operations and therefore only
            // points to initialized memory.
            unsafe { elem.assume_init_drop() };
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn stack_drop() {
        struct DropTester<'a>(&'a std::cell::RefCell<usize>);

        impl<'a> Drop for DropTester<'a> {
            fn drop(&mut self) {
                *self.0.borrow_mut() += 1;
            }
        }

        let n = std::cell::RefCell::new(0);

        {
            let mut stack = Stack::<_, 4>::new();
            for _ in 0..4 {
                stack.push(DropTester(&n));
            }
        }

        assert_eq!(n.into_inner(), 4);
    }

    #[test]
    fn stack_debug() {
        let mut stack = Stack::<_, 4>::new();
        for n in 0..4 {
            stack.push(n);
        }

        assert_eq!(format!("{stack:?}"), "[0, 1, 2, 3]");
    }

    #[test]
    #[should_panic(expected = "Cannot push more than SIZE elements to a ParsingStack")]
    fn stack_push_over_size() {
        let mut stack = Stack::<_, 1>::new();

        stack.push(());
        stack.push(());
    }
}
