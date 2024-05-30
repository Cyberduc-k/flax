use core::any::TypeId;

use atomic_refcell::{AtomicRef, AtomicRefMut};

use crate::{util::TuplePush, BoxedSystem, System};

use super::{input::ExtractDyn, DynSystem, IntoInput};

mod function;
mod input;
mod param;

pub use param::{Local, SystemParam};

/// Transform into a system.
pub trait IntoSystem<Ret, Marker>: Sized {
    /// The concrete system type to transform into.
    type System;

    /// Transform into a system.
    fn into_system(self) -> Self::System;
}

/// Extension trait for [`IntoSystem`]
pub trait IntoSystemExt<Ret, Marker>: IntoSystem<Ret, Marker> {
    /// Add input to the system
    fn with_input<I>(self, input: I) -> WithInput<Self::System, (I,)>;

    /// Transform into a [`BoxedSystem`]
    fn boxed(self) -> BoxedSystem;
}

impl<T, Ret, Marker> IntoSystemExt<Ret, Marker> for T
where
    T: IntoSystem<Ret, Marker>,
    T::System: DynSystem,
{
    fn with_input<I>(self, input: I) -> WithInput<Self::System, (I,)> {
        WithInput {
            system: self.into_system(),
            input: (input,),
        }
    }

    fn boxed(self) -> BoxedSystem {
        todo!()
    }
}

pub trait InitState {
    fn init_state(&mut self, ctx: &InitStateContext);
}

/// Context for [`SystemParam::init_state`]
pub struct InitStateContext<'b, 'input> {
    input: &'b dyn ExtractDyn<'b, 'input>,
}

impl<'b, 'input> InitStateContext<'b, 'input> {
    /// Creates a new init state context
    pub fn new(input: &'b dyn ExtractDyn<'b, 'input>) -> Self {
        Self { input }
    }

    /// Access user provided input data
    #[inline]
    pub fn input<T: 'static>(&self) -> Option<AtomicRef<T>> {
        let cell = unsafe { self.input.extract_dyn(TypeId::of::<T>()) };
        cell.map(|v| AtomicRef::map(v.borrow(), unsafe { |v| v.cast().as_ref() }))
    }

    /// Access user provided input data
    #[inline]
    pub fn input_mut<T: 'static>(&self) -> Option<AtomicRefMut<T>> {
        let cell = unsafe { self.input.extract_dyn(TypeId::of::<T>()) };
        cell.map(|v| AtomicRefMut::map(v.borrow_mut(), unsafe { |v| v.cast().as_mut() }))
    }
}

pub struct WithInput<S, I> {
    system: S,
    input: I,
}

impl<'a, S, I> WithInput<S, I>
where
    S: DynSystem + InitState,
    I: IntoInput<'a>,
{
    pub fn with_input<I2>(self, input: I2) -> WithInput<S, I::PushRight>
    where
        I: TuplePush<I2>,
    {
        WithInput {
            system: self.system,
            input: self.input.push_right(input),
        }
    }

    pub fn boxed(self) -> BoxedSystem
    where
        S: Send + Sync + 'static,
    {
        BoxedSystem::new(self.system())
    }

    pub fn system(mut self) -> S {
        let input = self.input.into_input();
        let ctx = InitStateContext::new(&input);
        self.system.init_state(&ctx);
        self.system
    }
}

impl<F, Args, Ret> IntoSystem<Ret, ()> for System<F, Args, Ret>
where
    System<F, Args, Ret>: DynSystem,
    F: 'static,
    Args: 'static,
    Ret: 'static,
{
    type System = Self;

    fn into_system(self) -> Self::System {
        self
    }
}

#[cfg(test)]
mod test {
    use crate::error::Result;
    use crate::query::ResourceBorrow;
    use crate::system::into::IntoSystemExt;
    use crate::{Component, Entity, Mutable, Query, QueryBorrow, World};

    #[test]
    fn into_system() {
        component! {
            health: f32,
            resources,
        }

        let mut world = World::new();
        Entity::builder().set(health(), 5.0).spawn(&mut world);
        Entity::builder()
            .set(health(), 1.2)
            .spawn_at(&mut world, resources())
            .ok();

        fn regen_system(
            mut q: QueryBorrow<Mutable<f32>>,
            mut r: ResourceBorrow<Component<f32>>,
        ) -> Result<()> {
            let h = r.get()?;
            q.for_each(|health| {
                *health += *h;
            });
            Ok(())
        }

        let mut system = regen_system
            .with_input(Query::new(health().as_mut()))
            .system();
        system.run(&mut world).ok();
    }
}
