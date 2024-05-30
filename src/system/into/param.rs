use core::fmt;

use atomic_refcell::{AtomicRef, AtomicRefMut};

use crate::{
    system::{
        traits::{WithCmd, WithCmdMut, WithWorld, WithWorldMut},
        AsBorrowed, SystemAccess, SystemContext,
    },
    CommandBuffer, World,
};

use super::InitStateContext;

/// Borrow state from the system execution data
pub trait SystemParam {
    /// The borrow from the system context
    type Value<'a>: for<'b> AsBorrowed<'b>;
    /// State for this [`SystemParam`]
    type State: SystemAccess;

    /// Initialize the system state
    fn init_state(ctx: &InitStateContext<'_, '_>) -> Self::State;

    /// Get the data from the system context
    fn acquire<'a>(
        state: &'a mut Self::State,
        ctx: &'a SystemContext<'_, '_, '_>,
    ) -> Self::Value<'a>;

    /// Human friendly debug description
    fn describe(state: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result;
}

impl SystemParam for &World {
    type Value<'a> = AtomicRef<'a, World>;
    type State = WithWorld;

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {
        WithWorld
    }

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.world()
    }

    fn describe(_: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("&World")
    }
}

impl SystemParam for &mut World {
    type Value<'a> = AtomicRefMut<'a, World>;
    type State = WithWorldMut;

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {
        WithWorldMut
    }

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.world_mut()
    }

    fn describe(_: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("&mut World")
    }
}

impl SystemParam for &CommandBuffer {
    type Value<'a> = AtomicRef<'a, CommandBuffer>;
    type State = WithCmd;

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {
        WithCmd
    }

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.cmd()
    }

    fn describe(_: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("&CommandBuffer")
    }
}

impl SystemParam for &mut CommandBuffer {
    type Value<'a> = AtomicRefMut<'a, CommandBuffer>;
    type State = WithCmdMut;

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {
        WithCmdMut
    }

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        ctx.cmd_mut()
    }

    fn describe(_: &Self::State, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("&mut CommandBuffer")
    }
}
