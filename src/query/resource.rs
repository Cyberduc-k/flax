use core::ops::{Deref, DerefMut};

use atomic_refcell::AtomicRef;

use crate::{
    components::resources,
    fetch::PreparedFetch,
    filter::All,
    system::{AsBorrowed, InitStateContext, SystemContext, SystemParam},
    Entity, EntityBorrow, Fetch, Planar, Query, World,
};

/// Resource(*Query*)Borrow
///
/// A prepared query for the resources() entity. Holds the locks for the affected archetype and
/// components.
pub struct ResourceBorrow<'w, Q, F = All>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    _borrow: EntityBorrow<'w, Q, F>,
    data: <Q::Prepared as PreparedFetch<'w>>::Item,
}

impl<'w, Q, F> Deref for ResourceBorrow<'w, Q, F>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    type Target = <Q::Prepared as PreparedFetch<'w>>::Item;

    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

impl<'w, Q, F> DerefMut for ResourceBorrow<'w, Q, F>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data
    }
}

pub struct ResourceQueryData<'a, Q, F = All>
where
    Q: for<'x> Fetch<'x> + 'static,
    F: for<'x> Fetch<'x> + 'static,
{
    world: AtomicRef<'a, World>,
    query: &'a mut Query<Q, F, Entity>,
}

impl<'w, Q, F> SystemParam for ResourceBorrow<'w, Q, F>
where
    Q: for<'a> Fetch<'a> + Clone + 'static,
    F: for<'a> Fetch<'a> + Clone + 'static,
{
    type Value<'a> = ResourceQueryData<'a, Q, F>;
    type State = Query<Q, F, Entity>;

    fn init_state(ctx: &InitStateContext<'_, '_>) -> Self::State {
        let query = ctx.input::<Query<Q, F, Planar>>().unwrap();
        query.clone().entity(resources())
    }

    fn acquire<'a>(
        state: &'a mut Self::State,
        ctx: &'a SystemContext<'_, '_, '_>,
    ) -> Self::Value<'a> {
        ResourceQueryData {
            world: ctx.world(),
            query: state,
        }
    }

    fn describe(state: &Self::State, f: &mut alloc::fmt::Formatter<'_>) -> alloc::fmt::Result {
        f.write_str("QueryBorrow<")?;
        state.fetch.describe(f)?;
        f.write_str(">")
    }
}

impl<'a, 'w, Q, F> AsBorrowed<'a> for ResourceQueryData<'w, Q, F>
where
    Q: for<'x> Fetch<'x> + 'static,
    F: for<'x> Fetch<'x> + 'static,
{
    type Borrowed = ResourceBorrow<'a, Q, F>;

    #[allow(clippy::missing_transmute_annotations)]
    fn as_borrowed(&'a mut self) -> Self::Borrowed {
        let mut borrow = self.query.borrow(&self.world);
        let data = unsafe { core::mem::transmute(borrow.get().unwrap()) };
        ResourceBorrow {
            _borrow: borrow,
            data,
        }
    }
}
