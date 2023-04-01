// Most of the code in this file is copied from std::Vec's Drain implementation.

use core::fmt;
use core::iter::FusedIterator;
use core::mem;
use core::ptr::{self, NonNull};
use core::slice;

use super::RawVector;

/// A draining iterator for `Vector<T>`.
///
/// This `struct` is created by [`Vector::drain`].
/// See its documentation for more.
pub struct Drain<'a, T: 'a> {
    /// Index of tail to preserve
    pub(super) tail_start: usize,
    /// Length of tail
    pub(super) tail_len: usize,
    /// Current remaining range to remove
    pub(super) iter: slice::Iter<'a, T>,
    pub(super) vec: NonNull<RawVector<T>>,
}

impl<T: fmt::Debug> fmt::Debug for Drain<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Drain").field(&self.iter.as_slice()).finish()
    }
}

impl<'a, T> Drain<'a, T> {
    /// Returns the remaining items of this iterator as a slice.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut vec = vec!['a', 'b', 'c'];
    /// let mut drain = vec.drain(..);
    /// assert_eq!(drain.as_slice(), &['a', 'b', 'c']);
    /// let _ = drain.next().unwrap();
    /// assert_eq!(drain.as_slice(), &['b', 'c']);
    /// ```
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        self.iter.as_slice()
    }
}

impl<'a, T> AsRef<[T]> for Drain<'a, T> {
    fn as_ref(&self) -> &[T] {
        self.as_slice()
    }
}

unsafe impl<T: Sync> Sync for Drain<'_, T> {}
unsafe impl<T: Send> Send for Drain<'_, T> {}

impl<T> Iterator for Drain<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<T> {
        self.iter.next().map(|elt| unsafe { ptr::read(elt as *const _) })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl<T> DoubleEndedIterator for Drain<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<T> {
        self.iter.next_back().map(|elt| unsafe { ptr::read(elt as *const _) })
    }
}

impl<T> Drop for Drain<'_, T> {
    fn drop(&mut self) {
        /// Moves back the un-`Drain`ed elements to restore the original `Vec`.
        struct DropGuard<'r, 'a, T>(&'r mut Drain<'a, T>);

        impl<'r, 'a, T> Drop for DropGuard<'r, 'a, T> {
            fn drop(&mut self) {
                if self.0.tail_len > 0 {
                    unsafe {
                        let source_vec = self.0.vec.as_mut();
                        // memmove back untouched tail, update to new length
                        let start = source_vec.len();
                        let tail = self.0.tail_start;
                        if tail != start {
                            let src = source_vec.as_ptr().add(tail);
                            let dst = source_vec.as_mut_ptr().add(start);
                            ptr::copy(src, dst, self.0.tail_len);
                        }
                        source_vec.len = (start + self.0.tail_len) as u32;
                    }
                }
            }
        }

        let iter = mem::replace(&mut self.iter, (&mut []).iter());
        let drop_len = iter.len();

        let mut vec = self.vec;

        // if T::IS_ZST {
        //     // ZSTs have no identity, so we don't need to move them around, we only need to drop the correct amount.
        //     // this can be achieved by manipulating the Vec length instead of moving values out from `iter`.
        //     unsafe {
        //         let vec = vec.as_mut();
        //         let old_len = vec.len();
        //         vec.set_len(old_len + drop_len + self.tail_len);
        //         vec.truncate(old_len + self.tail_len);
        //     }
        //     return;
        // }

        // ensure elements are moved back into their appropriate places, even when drop_in_place panics
        let _guard = DropGuard(self);

        if drop_len == 0 {
            return;
        }

        // as_slice() must only be called when iter.len() is > 0 because
        // it also gets touched by vec::Splice which may turn it into a dangling pointer
        // which would make it and the vec pointer point to different allocations which would
        // lead to invalid pointer arithmetic below.
        let drop_ptr = iter.as_slice().as_ptr();

        unsafe {
            // drop_ptr comes from a slice::Iter which only gives us a &[T] but for drop_in_place
            // a pointer with mutable provenance is necessary. Therefore we must reconstruct
            // it from the original vec but also avoid creating a &mut to the front since that could
            // invalidate raw pointers to it which some unsafe code might rely on.
            let vec_ptr = vec.as_mut().as_mut_ptr();
            let drop_offset = sub_ptr(drop_ptr, vec_ptr);
            let to_drop = ptr::slice_from_raw_parts_mut(vec_ptr.add(drop_offset), drop_len);
            ptr::drop_in_place(to_drop);
        }
    }
}

fn sub_ptr<T>(a: *const T, b: *const T) -> usize {
    debug_assert!(a >= b);

    (a as usize - b as usize) / mem::size_of::<T>()
}

impl<T> FusedIterator for Drain<'_, T> {}

