use core::{fmt, marker::PhantomData};

use crate::{
    system::{Access, AsBorrowed, DynSystem, IntoInput, SystemAccess, SystemContext},
    BoxedSystem, CommandBuffer, IntoSystemExt, World,
};

use super::{param::SystemParam, InitState, InitStateContext, IntoSystem};

pub struct FunctionSystem<F, Ret, Marker>
where
    F: SystemParamFunction<Marker, Ret = Ret>,
{
    func: F,
    name: String,
    state: Option<<F::Args as SystemParam>::State>,
    marker: PhantomData<fn() -> Marker>,
}

pub struct IsFunctionSystem;

impl<F, Ret, Marker> IntoSystem<Ret, (IsFunctionSystem, Marker)> for F
where
    F: SystemParamFunction<Marker, Ret = Ret> + 'static,
    Ret: 'static,
    Marker: 'static,
{
    type System = FunctionSystem<F, Ret, Marker>;

    fn into_system(self) -> Self::System {
        FunctionSystem {
            func: self,
            name: tynm::type_name::<F>(),
            state: None,
            marker: PhantomData,
        }
    }
}

impl<F, Ret, Marker> InitState for FunctionSystem<F, Ret, Marker>
where
    F: SystemParamFunction<Marker, Ret = Ret>,
{
    fn init_state(&mut self, ctx: &InitStateContext) {
        self.state = Some(F::Args::init_state(ctx));
    }
}

impl<F, Err, Marker> DynSystem for FunctionSystem<F, Result<(), Err>, Marker>
where
    F: SystemParamFunction<Marker, Ret = Result<(), Err>>,
    Err: Into<anyhow::Error>,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fn ")?;
        f.write_str(&self.name)?;
        let state = self.state.as_ref().unwrap();
        F::Args::describe(state, f)?;
        Ok(())
    }

    fn execute(&mut self, ctx: &SystemContext<'_, '_, '_>) -> anyhow::Result<()> {
        profile_function!(self.name());
        let data = {
            profile_scope!("acquire_data");
            let state = self.state.as_mut().unwrap();
            F::Args::acquire(state, ctx)
        };
        let res: anyhow::Result<()> = self.func.execute(data).map_err(Into::into);
        if let Err(err) = res {
            return Err(err.context(format!("Failed to execute system: {:?}", self)));
        }
        Ok(())
    }

    fn access(&self, world: &World, dst: &mut Vec<Access>) {
        let state = self.state.as_ref().unwrap();
        state.access(world, dst);
    }
}

impl<F, Marker> DynSystem for FunctionSystem<F, (), Marker>
where
    F: SystemParamFunction<Marker, Ret = ()>,
{
    fn name(&self) -> &str {
        &self.name
    }

    fn describe(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("fn ")?;
        f.write_str(&self.name)?;
        let state = self.state.as_ref().unwrap();
        F::Args::describe(state, f)?;
        Ok(())
    }

    fn execute(&mut self, ctx: &SystemContext<'_, '_, '_>) -> anyhow::Result<()> {
        profile_function!(self.name());
        let data = {
            profile_scope!("acquire_data");
            let state = self.state.as_mut().unwrap();
            F::Args::acquire(state, ctx)
        };
        {
            profile_scope!("exec");
            self.func.execute(data);
        }
        Ok(())
    }

    fn access(&self, world: &World, dst: &mut Vec<Access>) {
        let state = self.state.as_ref().unwrap();
        state.access(world, dst);
    }
}

impl<F, Ret, Marker> fmt::Debug for FunctionSystem<F, Ret, Marker>
where
    Self: DynSystem,
    F: SystemParamFunction<Marker, Ret = Ret>,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.describe(f)
    }
}

impl<F, Ret, Marker> FunctionSystem<F, Ret, Marker>
where
    F: SystemParamFunction<Marker, Ret = Ret>,
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

impl<F, Ret, Marker> FunctionSystem<F, Ret, Marker>
where
    F: SystemParamFunction<Marker, Ret = Ret>,
    Ret: 'static,
{
    /// Run the system on the world. Any commands will be applied
    pub fn run<'a>(&'a mut self, world: &'a mut World) -> F::Ret {
        self.run_with(world, ())
    }

    /// Run the system on the world. Any commands will be applied
    pub fn run_with<'a>(&mut self, world: &mut World, input: impl IntoInput<'a>) -> F::Ret {
        #[cfg(feature = "tracing")]
        let _span = tracing::info_span!("run_on", name = self.name).entered();

        let mut cmd = CommandBuffer::new();
        let input = input.into_input();
        let ctx = SystemContext::new(world, &mut cmd, &input);
        let state = self.state.as_mut().unwrap();
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

            #[allow(clippy::unused_unit)]
            fn init_state(_ctx: &InitStateContext<'_, '_>) -> Self::State {
                ($($ty::init_state(_ctx),)*)
            }

            #[allow(clippy::unused_unit)]
            fn acquire<'a>(_state: &'a mut Self::State, _ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
                ($($ty::acquire(&mut _state.$idx, _ctx),)*)
            }

            fn describe(_state: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                core::fmt::Debug::fmt(&($(
                    FmtSystemParam::<$ty>(&_state.$idx),
                )*), f)
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        S::describe(self.0, f)
    }
}
