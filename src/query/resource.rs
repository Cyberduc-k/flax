use core::{
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use atomic_refcell::AtomicRef;

use crate::{
    components::resources,
    error::Result,
    fetch::{FetchAccessData, PreparedFetch},
    filter::All,
    system::{
        Access, AccessKind, AsBorrowed, InitStateContext, SystemAccess, SystemContext, SystemParam,
    },
    ArchetypeSearcher, Component, Entity, EntityBorrow, Fetch, Planar, Query, RefMut, World,
};

/// A globally available resource.
pub trait Resource: Sized + Send + Sync + 'static {
    fn query() -> Component<Self>;
}

/// A [`SystemParam`] that represents a reference to a resource.
pub struct Res<'w, T: Resource> {
    borrow: AtomicRef<'w, T>,
}

pub struct ResData<'w, T: Resource> {
    world: AtomicRef<'w, World>,
    marker: PhantomData<fn() -> &'w T>,
}

impl<'w, T: Resource> Deref for Res<'w, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.borrow
    }
}

impl<'w, T: Resource> SystemAccess for Res<'w, T> {
    fn access(&self, world: &World, dst: &mut Vec<crate::system::Access>) {
        let mut searcher = ArchetypeSearcher::default();
        let fetch = T::query();
        fetch.searcher(&mut searcher);

        searcher.find_archetypes(&world.archetypes, |arch_id, arch| {
            if !fetch.filter_arch(FetchAccessData {
                world,
                arch,
                arch_id,
            }) {
                return;
            }

            let data = FetchAccessData {
                world,
                arch,
                arch_id,
            };

            fetch.access(data, dst)
        });

        dst.push(Access {
            kind: AccessKind::World,
            mutable: false,
        });
    }
}

impl<'w, T: Resource> SystemParam for Res<'w, T> {
    type Value<'a> = ResData<'a, T>;
    type State = ();

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        let world = ctx.world();
        ResData {
            world,
            marker: PhantomData,
        }
    }

    fn describe(_: &Self::State, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Res<")?;
        T::query().describe(f)?;
        f.write_str(">")
    }
}

impl<'w, 'a, T: Resource> AsBorrowed<'a> for ResData<'w, T> {
    type Borrowed = Res<'a, T>;

    fn as_borrowed(&'a mut self) -> Self::Borrowed {
        let borrow = self
            .world
            .get(resources(), T::query())
            .expect("resource not found");
        Res { borrow }
    }
}

/// A [`SystemParam`] that represents a mutable reference to a resource.
pub struct ResMut<'w, T: Resource> {
    borrow: RefMut<'w, T>,
}

pub struct ResMutData<'w, T: Resource> {
    world: AtomicRef<'w, World>,
    marker: PhantomData<fn() -> &'w mut T>,
}

impl<'w, T: Resource> Deref for ResMut<'w, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.borrow
    }
}

impl<'w, T: Resource> DerefMut for ResMut<'w, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.borrow
    }
}

impl<'w, T: Resource> SystemAccess for ResMut<'w, T> {
    fn access(&self, world: &World, dst: &mut Vec<crate::system::Access>) {
        let mut searcher = ArchetypeSearcher::default();
        let fetch = T::query().as_mut();
        fetch.searcher(&mut searcher);

        searcher.find_archetypes(&world.archetypes, |arch_id, arch| {
            if !fetch.filter_arch(FetchAccessData {
                world,
                arch,
                arch_id,
            }) {
                return;
            }

            let data = FetchAccessData {
                world,
                arch,
                arch_id,
            };

            fetch.access(data, dst)
        });

        dst.push(Access {
            kind: AccessKind::World,
            mutable: false,
        });
    }
}

impl<'w, T: Resource> SystemParam for ResMut<'w, T> {
    type Value<'a> = ResMutData<'a, T>;
    type State = ();

    fn init_state(_: &InitStateContext<'_, '_>) -> Self::State {}

    fn acquire<'a>(_: &'a mut Self::State, ctx: &'a SystemContext<'_, '_, '_>) -> Self::Value<'a> {
        let world = ctx.world();
        ResMutData {
            world,
            marker: PhantomData,
        }
    }

    fn describe(_: &Self::State, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ResMut<")?;
        T::query().describe(f)?;
        f.write_str(">")
    }
}

impl<'w, 'a, T: Resource> AsBorrowed<'a> for ResMutData<'w, T> {
    type Borrowed = ResMut<'a, T>;

    fn as_borrowed(&'a mut self) -> Self::Borrowed {
        let borrow = self
            .world
            .get_mut(resources(), T::query())
            .expect("resource not found");
        ResMut { borrow }
    }
}

/// Resource(*Query*)Borrow
///
/// A prepared query for the resources() entity. Holds the locks for the affected archetype and
/// components.
pub struct ResourceBorrow<'w, Q, F = All>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    borrow: EntityBorrow<'w, Q, F>,
}

impl<'w, Q, F> ResourceBorrow<'w, Q, F>
where
    Q: Fetch<'w>,
    F: Fetch<'w>,
{
    /// Returns the results of the fetch.
    ///
    /// Fails if the entity does not exist, or the fetch isn't matched.
    pub fn get<'q>(&'q mut self) -> Result<<Q::Prepared as PreparedFetch<'q>>::Item>
    where
        'w: 'q,
    {
        self.borrow.get()
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
        let borrow = self.query.borrow(&self.world);
        ResourceBorrow { borrow }
    }
}
