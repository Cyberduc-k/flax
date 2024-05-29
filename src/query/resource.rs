use atomic_refcell::AtomicRef;

use crate::{
    components::resources,
    error::Result,
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
    entity: EntityBorrow<'w, Q, F>,
}

impl<'w, Q, F> ResourceBorrow<'w, Q, F>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    /// Returns the results of the fetch.
    ///
    /// Fails if the fetch isn't matched.
    pub fn get<'q>(&'q mut self) -> Result<<Q::Prepared as PreparedFetch<'q>>::Item>
    where
        'w: 'q,
    {
        self.entity.get()
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

    fn as_borrowed(&'a mut self) -> Self::Borrowed {
        let entity = self.query.borrow(&self.world);
        ResourceBorrow { entity }
    }
}
