use core::{
    any::TypeId,
    fmt::{self, Formatter},
    marker::PhantomData,
    mem::MaybeUninit,
};

use atomic_refcell::{AtomicRef, AtomicRefMut};

use crate::{BoxedSystem, CommandBuffer, System, World};

use super::{
    input::ExtractDyn,
    traits::{AsBorrowed, WithCmdMut, WithWorldMut},
    Access, AccessKind, DynSystem, IntoInput, SystemAccess,
};
use super::{
    traits::{WithCmd, WithWorld},
    SystemContext,
};

/// Transform into a system.
pub trait IntoSystem<Ret, Marker>: Sized {
    /// The concrete system type to transform into.
    type System: DynSystem;

    /// Transform into a system.
    fn into_system(self) -> Self::System;
}

impl<F, Args, Ret> IntoSystem<Ret, ()> for System<F, Args, Ret>
where
    System<F, Args, Ret>: DynSystem,
{
    type System = Self;

    fn into_system(self) -> Self::System {
        self
    }
}

/// Borrow state from the system execution data
pub trait SystemParam {
    /// The borrow from the system context
    type Value<'a>: for<'b> AsBorrowed<'b>;
    /// State for this [`SystemParam`]
    type State: SystemAccess;

    /// Initialize the system state
    fn init_state(state: &mut Self::State, ctx: &InitState<'_, '_>);

    /// Get the data from the system context
    fn acquire<'a>(
        state: &'a mut Self::State,
        ctx: &'a SystemContext<'_, '_, '_>,
    ) -> Self::Value<'a>;

    /// Human friendly debug description
    fn describe(state: &Self::State, f: &mut Formatter<'_>) -> fmt::Result;
}

/// Context for [`SystemParam::init_state`]
pub struct InitState<'b, 'input> {
    input: &'b dyn ExtractDyn<'b, 'input>,
}

impl<'b, 'input> InitState<'b, 'input> {
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

/// Describe an access to the world in terms of shared and unique accesses
pub trait SystemParamAccess {
    /// Local state for this [`SystemParam`]
    type State: Default;

    /// Returns all the accesses for a system
    fn access(state: &Self::State, world: &World, dst: &mut Vec<Access>);
}

impl SystemParam for &World {
    type Value<'a> = AtomicRef<'a, World>;
    type State = WithWorld;

    fn init_state(_: &mut Self::State, _: &InitState<'_, '_>) {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.world()
    }

    fn describe(_: &Self::State, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("&World")
    }
}

impl SystemParamAccess for &World {
    type State = ();

    fn access(_: &Self::State, _: &World, dst: &mut Vec<Access>) {
        dst.push(Access {
            kind: AccessKind::World,
            mutable: false,
        });
    }
}

impl SystemParam for &mut World {
    type Value<'a> = AtomicRefMut<'a, World>;
    type State = WithWorldMut;

    fn init_state(_: &mut Self::State, _: &InitState<'_, '_>) {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.world_mut()
    }

    fn describe(_: &Self::State, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("&mut World")
    }
}

impl SystemParamAccess for &mut World {
    type State = ();

    fn access(_: &Self::State, _: &World, dst: &mut Vec<Access>) {
        dst.push(Access {
            kind: AccessKind::World,
            mutable: true,
        });
    }
}

impl SystemParam for &CommandBuffer {
    type Value<'a> = AtomicRef<'a, CommandBuffer>;
    type State = WithCmd;

    fn init_state(_: &mut Self::State, _: &InitState<'_, '_>) {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.cmd()
    }

    fn describe(_: &Self::State, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("&CommandBuffer")
    }
}

impl SystemParamAccess for &CommandBuffer {
    type State = ();

    fn access(_: &Self::State, _: &World, dst: &mut Vec<Access>) {
        dst.push(Access {
            kind: AccessKind::CommandBuffer,
            mutable: false,
        });
    }
}

impl SystemParam for &mut CommandBuffer {
    type Value<'a> = AtomicRefMut<'a, CommandBuffer>;
    type State = WithCmdMut;

    fn init_state(_: &mut Self::State, _: &InitState<'_, '_>) {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.cmd_mut()
    }

    fn describe(_: &Self::State, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("&mut CommandBuffer")
    }
}

impl SystemParamAccess for &mut CommandBuffer {
    type State = ();

    fn access(_: &Self::State, _: &World, dst: &mut Vec<Access>) {
        dst.push(Access {
            kind: AccessKind::CommandBuffer,
            mutable: true,
        });
    }
}

pub struct IsFunctionSystem;

impl<Marker, F> IntoSystem<F::Ret, (IsFunctionSystem, Marker)> for F
where
    F: SystemParamFunction<Marker>,
{
    type System = FunctionSystem<F, (), Marker>;

    fn into_system(self) -> Self::System {
        FunctionSystem {
            func: self,
            name: tynm::type_name::<F>(),
            state: MaybeUninit::uninit(),
            input: (),
            marker: PhantomData,
        }
    }
}

pub struct FunctionSystem<F, Input, Marker>
where
    F: SystemParamFunction<Marker>,
{
    func: F,
    name: String,
    state: MaybeUninit<<F::Args as SystemParam>::State>,
    input: Input,
    marker: PhantomData<fn() -> Marker>,
}

impl<F, Input, Marker> DynSystem for FunctionSystem<F, Input, Marker>
where
    F: SystemParamFunction<Marker>,
    Input: IntoInput,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn describe(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("fn ")?;
        f.write_str(&self.name)?;
        let state = unsafe { self.state.assume_init_ref() };
        F::Args::describe(state, f)?;
        Ok(())
    }

    fn execute(&mut self, ctx: &SystemContext<'_, '_, '_>) -> anyhow::Result<()> {
        profile_function!(self.name());
        let data = {
            profile_scope!("acquire_data");
            let state = unsafe { self.state.assume_init_mut() };
            F::Args::acquire(state, ctx)
        };
        {
            profile_scope!("exec");
            self.func.execute(data);
        }
        Ok(())
    }

    fn access(&self, world: &World, dst: &mut Vec<Access>) {
        let state = unsafe { self.state.assume_init_ref() };
        state.access(world, dst);
    }
}

impl<F, Marker> FunctionSystem<F, Marker>
where
    F: SystemParamFunction<Marker>,
{
    /// Convert to a type erased Send + Sync system
    pub fn boxed(self) -> BoxedSystem
    where
        F::Ret: Send + Sync + 'static,
        F::Args: Send + Sync + 'static,
        <F::Args as SystemParam>::State: Send + Sync + 'static,
        F: Send + Sync + 'static,
        Self: DynSystem,
        Marker: 'static,
    {
        BoxedSystem::new(self)
    }
}

impl<F, Marker> FunctionSystem<F, Marker>
where
    F: SystemParamFunction<Marker>,
{
    /// Run the system on the world. Any commands will be applied
    pub fn run<'a>(&'a mut self, world: &'a mut World) -> F::Ret
    where
        F::Ret: 'static,
    {
        self.run_with(world, &mut ())
    }

    /// Run the system on the world. Any commands will be applied
    pub fn run_with<'a>(&mut self, world: &mut World, input: impl IntoInput<'a>) -> F::Ret
    where
        F::Ret: 'static,
    {
        #[cfg(feature = "tracing")]
        let _span = tracing::info_span!("run_on", name = self.name).entered();

        let mut cmd = CommandBuffer::new();
        let input = input.into_input();
        let ctx = SystemContext::new(world, &mut cmd, &input);
        let state = unsafe { self.state.assume_init_mut() };
        // F::Args::init_state(state);
        let data = F::Args::acquire(state, &ctx);
        let ret = self.func.execute(data);
        ctx.cmd_mut()
            .apply(&mut ctx.world.borrow_mut())
            .expect("Failed to apply commandbuffer");
        ret
    }
}

pub trait SystemParamFunction<Marker> {
    type Args: SystemParam;
    type Ret;

    fn execute(&mut self, args: <Self::Args as SystemParam>::Value<'_>) -> Self::Ret;
}

macro_rules! tuple_impl {
    ($($idx:tt => $ty:ident),*) => {
        impl<Func, Ret, $($ty,)*> SystemParamFunction<fn($($ty),*) -> Ret> for Func
        where
            $($ty: SystemParam,)*
            for<'a> &'a mut Func:
                FnMut($($ty),*) -> Ret +
                FnMut($(<<$ty as SystemParam>::Value<'_> as AsBorrowed<'_>>::Borrowed),*) -> Ret,
        {
            type Args = ($($ty,)*);
            type Ret = Ret;

            fn execute(&mut self, mut _args: <Self::Args as SystemParam>::Value<'_>) -> Self::Ret {
                #[inline(always)]
                fn call_inner<Ret, $($ty),*>(
                    mut f: impl FnMut($($ty),*) -> Ret,
                    _args: ($($ty,)*),
                ) -> Ret {
                    f($(_args.$idx),*)
                }
                call_inner(self, _args.as_borrowed())
            }
        }

        impl<$($ty),*> SystemParam for ($($ty,)*)
        where
            $($ty: SystemParam,)*
        {
            type Value<'a> = ($($ty::Value<'a>,)*);
            type State = ($($ty::State,)*);

            fn init_state(_state: &mut Self::State, _ctx: &InitState<'_, '_>) {
                $($ty::init_state(&mut _state.$idx, _ctx);)*
            }

            #[allow(clippy::unused_unit)]
            fn acquire<'a>(_state: &'a mut Self::State, _ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
                ($($ty::acquire(&mut _state.$idx, _ctx),)*)
            }

            fn describe(_state: &Self::State, f: &mut Formatter<'_>) -> fmt::Result {
                core::fmt::Debug::fmt(&($(
                    FmtSystemParam::<$ty>(&_state.$idx),
                )*), f)
            }
        }

        impl<$($ty),*> SystemParamAccess for ($($ty,)*)
        where
            $($ty: SystemParamAccess,)*
        {
            type State = ($($ty::State,)*);

            fn access(_state: &Self::State, _world: &World, _dst: &mut Vec<Access>) {
                $($ty::access(&_state.$idx, _world, _dst);)*
            }
        }

        impl<'a, $($ty),*> AsBorrowed<'a> for ($($ty,)*)
        where
            $($ty: AsBorrowed<'a>,)*
        {
            type Borrowed = ($($ty::Borrowed,)*);

            #[allow(clippy::unused_unit)]
            fn as_borrowed(&'a mut self) -> Self::Borrowed {
                ($(self.$idx.as_borrowed(),)*)
            }
        }
    };
}

tuple_impl! {}
tuple_impl! { 0 => A }
tuple_impl! { 0 => A, 1 => B }
tuple_impl! { 0 => A, 1 => B, 2 => C }
tuple_impl! { 0 => A, 1 => B, 2 => C, 3 => D }

struct FmtSystemParam<'a, S>(&'a S::State)
where
    S: SystemParam;
impl<'a, S> core::fmt::Debug for FmtSystemParam<'a, S>
where
    S: SystemParam,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        S::describe(self.0, f)
    }
}

#[cfg(test)]
mod test {
    use crate::system::IntoSystem;
    use crate::{Entity, Mutable, QueryBorrow, World};

    #[test]
    fn into_system() {
        component! {
            health: f32,
        }

        let mut world = World::new();
        Entity::builder().set(health(), 5.0).spawn(&mut world);

        fn regen_system(mut q: QueryBorrow<Mutable<f32>>) {
            q.for_each(|health| {
                *health += 1.2;
            })
        }

        let mut system = regen_system.into_system();
        system.run(&mut world);
    }
}
