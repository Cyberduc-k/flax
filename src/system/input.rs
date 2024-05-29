use core::{any::TypeId, ptr::NonNull};

use atomic_refcell::AtomicRefCell;

use crate::Query;

/// Extract a reference from a [`AtomicRefCell`]
/// # Safety
///
/// The returned value must be of the type specified by `ty`
pub unsafe trait ExtractDyn<'a, 'b>: Send + Sync {
    /// Dynamically extract a reference of `ty` contained within
    /// # Safety
    ///
    /// The returned pointer is of type `ty` which has a lifetime of `'b`
    unsafe fn extract_dyn(&'a self, ty: TypeId) -> Option<&'a AtomicRefCell<NonNull<()>>>;
}

/// Convert a tuple of references into a tuple of reference checked erased cells
pub trait IntoInput<'a> {
    /// The type erased cell
    type Output: for<'x> ExtractDyn<'x, 'a>;
    /// # Safety
    ///
    /// The caller must that the returned cell is used with the lifetime of `'a`
    fn into_input(self) -> Self::Output;
}

impl<'a> IntoInput<'a> for () {
    type Output = ();
    fn into_input(self) -> Self::Output {}
}

unsafe impl<'a, 'b> ExtractDyn<'a, 'b> for () {
    unsafe fn extract_dyn(&'a self, _: TypeId) -> Option<&'a AtomicRefCell<NonNull<()>>> {
        None
    }
}

unsafe impl<'a, 'b, T: 'static + Send + Sync> ExtractDyn<'a, 'b> for ErasedCell<'b, T> {
    #[inline]
    unsafe fn extract_dyn(&'a self, ty: TypeId) -> Option<&'a AtomicRefCell<NonNull<()>>> {
        if TypeId::of::<T>() == ty {
            Some(&self.cell)
        } else {
            None
        }
    }
}

impl<'a, T: 'static + Send + Sync> IntoInput<'a> for &'a mut T {
    type Output = ErasedCell<'a, T>;
    fn into_input(self) -> Self::Output {
        unsafe { ErasedCell::new_ref(self) }
    }
}

impl<'a, Q, F, S> IntoInput<'a> for Query<Q, F, S>
where
    Q: 'static + Sync + Send,
    F: 'static + Sync + Send,
    S: 'static + Sync + Send,
{
    type Output = ErasedCell<'a, Query<Q, F, S>>;
    fn into_input(self) -> Self::Output {
        unsafe { ErasedCell::new(self) }
    }
}

pub struct ErasedCell<'a, T: ?Sized> {
    cell: AtomicRefCell<NonNull<()>>,
    drop_fn: fn(NonNull<()>),
    _marker: core::marker::PhantomData<&'a mut T>,
}

impl<'a, T: ?Sized> ErasedCell<'a, T> {
    unsafe fn new_ref(value: &'a mut T) -> Self {
        Self {
            cell: AtomicRefCell::new(NonNull::from(value).cast::<()>()),
            drop_fn: |_| {},
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'a, T: 'static> ErasedCell<'a, T> {
    unsafe fn new(value: T) -> Self {
        let boxed = Box::leak(Box::new(value));
        let drop_fn = |ptr: NonNull<()>| {
            ptr.cast::<T>().drop_in_place();
        };
        Self {
            cell: AtomicRefCell::new(NonNull::from(boxed).cast::<()>()),
            drop_fn,
            _marker: core::marker::PhantomData,
        }
    }
}

impl<'a, T: ?Sized> Drop for ErasedCell<'a, T> {
    fn drop(&mut self) {
        (self.drop_fn)(*self.cell.borrow())
    }
}

unsafe impl<'a, T: ?Sized> Send for ErasedCell<'a, T> where T: Send {}
unsafe impl<'a, T: ?Sized> Sync for ErasedCell<'a, T> where T: Sync {}

macro_rules! tuple_impl {
    ($($idx: tt => $ty: ident),*) => {
        impl<'a, $($ty),*> IntoInput<'a> for ($($ty,)*)
        where
            $($ty: IntoInput<'a>,)*
        {
            type Output = ($($ty::Output,)*);

            fn into_input(self) -> Self::Output {
                ($($ty::into_input(self.$idx),)*)
            }
        }

        unsafe impl<'a, 'b, $($ty),*> ExtractDyn<'a, 'b> for ($($ty,)*)
        where
            $($ty: ExtractDyn<'a, 'b>,)*
        {
            unsafe fn extract_dyn(&'a self, ty: TypeId) -> Option<&'a AtomicRefCell<NonNull<()>>> {
                $(
                    if let Some(v) = self.$idx.extract_dyn(ty) {
                        Some(v)
                    } else
                )*
                {
                    None
                }
            }
        }
    };
}

tuple_impl! { 0 => A }
tuple_impl! { 0 => A, 1 => B }
tuple_impl! { 0 => A, 1 => B, 2 => C }
tuple_impl! { 0 => A, 1 => B, 2 => C, 3 => D }
tuple_impl! { 0 => A, 1 => B, 2 => C, 3 => D, 4 => E }

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    #[test]
    fn extract_2() {
        let mut a = String::from("Foo");
        let mut b = 5_i32;
        let values = unsafe { (ErasedCell::new_ref(&mut a), ErasedCell::new_ref(&mut b)) };

        unsafe {
            assert_eq!(
                values
                    .extract_dyn(TypeId::of::<String>())
                    .map(|v| v.borrow().cast::<alloc::string::String>().as_ref())
                    .map(|v| &**v),
                Some("Foo")
            );

            assert_eq!(
                values
                    .extract_dyn(TypeId::of::<i32>())
                    .map(|v| v.borrow().cast::<i32>().as_ref()),
                Some(&5)
            );

            assert_eq!(
                values
                    .extract_dyn(TypeId::of::<f32>())
                    .map(|v| v.borrow().cast::<f32>().as_ref()),
                None
            )
        }
    }
}
