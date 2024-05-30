use core::{
    fmt,
    ops::{AddAssign, Deref, DerefMut},
};

use crate::{
    system::{Access, AsBorrowed, SystemAccess, SystemContext},
    World,
};

use super::{InitStateContext, SystemParam};

pub struct Local<'s, T> {
    data: &'s mut T,
}

pub struct LocalState<T>(T);

impl<'s, T> SystemParam for Local<'s, T>
where
    T: Default + 'static,
{
    type Value<'a> = Local<'a, T>;
    type State = LocalState<T>;

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {
        LocalState(T::default())
    }

    fn acquire<'a>(
        state: &'a mut Self::State,
        _: &'a SystemContext<'_, '_, '_>,
    ) -> Self::Value<'a> {
        Local { data: &mut state.0 }
    }

    fn describe(_: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Local<{}>", tynm::type_name::<T>())
    }
}

impl<'s, 'a, T: 'static> AsBorrowed<'a> for Local<'s, T> {
    type Borrowed = Local<'a, T>;

    fn as_borrowed(&'a mut self) -> Self::Borrowed {
        Local { data: self.data }
    }
}

impl<T> SystemAccess for LocalState<T> {
    fn access(&self, _: &World, _: &mut Vec<Access>) {}
}

impl<T> Deref for Local<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.data
    }
}

impl<T> DerefMut for Local<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.data
    }
}

impl<T: PartialEq<U>, U> PartialEq<U> for Local<'_, T> {
    fn eq(&self, other: &U) -> bool {
        (*self.data).eq(other)
    }
}

impl<T: PartialOrd<U>, U> PartialOrd<U> for Local<'_, T> {
    fn partial_cmp(&self, other: &U) -> Option<core::cmp::Ordering> {
        (*self.data).partial_cmp(other)
    }
}

impl<T: AddAssign<U>, U> AddAssign<U> for Local<'_, T> {
    fn add_assign(&mut self, rhs: U) {
        self.data.add_assign(rhs);
    }
}
