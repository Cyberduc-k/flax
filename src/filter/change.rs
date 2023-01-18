use core::fmt::Formatter;
use core::ops::Deref;

use alloc::vec;
use alloc::vec::Vec;
use atomic_refcell::{AtomicRef, AtomicRefCell};
use itertools::Itertools;

use crate::archetype::{Archetype, Change};
use crate::fetch::{FetchPrepareData, PreparedFetch, ReadComponent};
use crate::{
    archetype::{ChangeList, Slice},
    Access, ChangeKind, Component, ComponentValue, Fetch, FetchItem,
};

static EMPTY_CHANGELIST_CELL: AtomicRefCell<ChangeList> = AtomicRefCell::new(ChangeList::new());
static EMPTY_CHANGELIST: ChangeList = ChangeList::new();

#[derive(Clone)]
/// Filter which only yields modified or inserted components
pub struct ChangeFilter<T> {
    component: Component<T>,
    kind: ChangeKind,
}

impl<T: ComponentValue> core::fmt::Debug for ChangeFilter<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ModifiedFilter")
            .field("component", &self.component)
            .field("kind", &self.kind)
            .finish()
    }
}

impl<T: ComponentValue> ChangeFilter<T> {
    /// Create a new modified filter
    pub(crate) fn new(component: Component<T>, kind: ChangeKind) -> Self {
        Self { component, kind }
    }
}

impl<'q, T> FetchItem<'q> for ChangeFilter<T>
where
    T: ComponentValue,
{
    type Item = &'q T;
}

impl<'w, T> Fetch<'w> for ChangeFilter<T>
where
    T: ComponentValue,
{
    const MUTABLE: bool = false;

    type Prepared = PreparedKindFilter<ReadComponent<'w, T>, AtomicRef<'w, [Change]>>;

    fn prepare(&'w self, data: crate::fetch::FetchPrepareData<'w>) -> Option<Self::Prepared> {
        let changes = data.arch.changes(self.component.key())?;

        // Make sure to enable modification tracking if it is actively used
        if self.kind.is_modified() {
            changes.set_track_modified()
        }

        let changes = AtomicRef::map(changes, |changes| changes.get(self.kind).as_slice());

        let fetch = self.component.prepare(data)?;
        Some(PreparedKindFilter::new(fetch, changes, data.old_tick))
    }

    fn filter_arch(&self, arch: &Archetype) -> bool {
        self.component.filter_arch(arch)
    }

    fn access(&self, data: crate::fetch::FetchPrepareData) -> Vec<Access> {
        let mut v = self.component.access(data);

        if data.arch.has(self.component.key()) {
            v.push(Access {
                kind: crate::AccessKind::ChangeEvent {
                    id: data.arch_id,
                    component: self.component.key(),
                },
                mutable: false,
            })
        }

        v
    }

    fn describe(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        write!(f, "{} {}", self.kind, self.component.name())
    }

    fn searcher(&self, searcher: &mut crate::ArchetypeSearcher) {
        searcher.add_required(self.component.key())
    }
}

#[derive(Debug)]
#[doc(hidden)]
pub struct PreparedKindFilter<Q, A> {
    fetch: Q,
    changes: A,
    cur: Option<Slice>,
    cursor: usize,
    old_tick: u32,
}

impl<Q, A> PreparedKindFilter<Q, A>
where
    A: Deref<Target = [Change]>,
{
    pub(crate) fn new(fetch: Q, changes: A, old_tick: u32) -> Self {
        Self {
            fetch,
            changes,
            cur: None,
            cursor: 0,
            old_tick,
        }
    }

    pub(crate) fn find_slice(&mut self, slots: Slice) -> Option<Slice> {
        // Short circuit
        if let Some(cur) = self.cur {
            if cur.overlaps(slots) {
                eprintln!("Found current {cur:?}");
                return Some(cur);
            }
        }

        let change = self.changes[self.cursor..]
            .iter()
            .filter(|v| v.tick > self.old_tick)
            .find_position(|change| change.slice.overlaps(slots));

        if let Some((idx, change)) = change {
            eprintln!("Found slice: {change:?} containing {slots:?}");
            self.cur = Some(change.slice);
            self.cursor = idx;
            return Some(change.slice);
        }

        let change = self.changes[..self.cursor]
            .iter()
            .filter(|v| v.tick > self.old_tick)
            .find_position(|change| change.slice.overlaps(slots));

        if let Some((_, change)) = change {
            eprintln!("Found slice before: {change:?} containing {slots:?}");
            return Some(change.slice);
        }

        None
    }
}

impl<'q, Q, A> PreparedFetch<'q> for PreparedKindFilter<Q, A>
where
    Q: PreparedFetch<'q>,
    A: Deref<Target = [Change]>,
{
    type Item = Q::Item;

    fn fetch(&'q mut self, slot: usize) -> Self::Item {
        self.fetch.fetch(slot)
    }

    fn filter_slots(&mut self, slots: Slice) -> Slice {
        eprintln!(
            "tick: {} changes: {:?} Looking for {slots:?}",
            self.old_tick, &*self.changes
        );

        let cur = match self.find_slice(slots) {
            Some(v) => v,
            None => return Slice::empty(),
        };

        let intersect = cur.intersect(&slots);
        // Try again with the next change group
        return intersect;
    }

    fn set_visited(&mut self, slots: Slice, change_tick: u32) {
        self.fetch.set_visited(slots, change_tick)
    }
}

#[derive(Clone)]
/// Filter which only yields removed `components
pub struct RemovedFilter<T> {
    component: Component<T>,
}

impl<T: ComponentValue> core::fmt::Debug for RemovedFilter<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RemovedFilter")
            .field("component", &self.component)
            .finish()
    }
}

impl<T: ComponentValue> RemovedFilter<T> {
    /// Create a new removed filter
    pub(crate) fn new(component: Component<T>) -> Self {
        Self { component }
    }
}

impl<'q, T: ComponentValue> FetchItem<'q> for RemovedFilter<T> {
    type Item = ();
}

impl<'a, T: ComponentValue> Fetch<'a> for RemovedFilter<T> {
    const MUTABLE: bool = false;

    type Prepared = PreparedKindFilter<(), &'a [Change]>;

    fn prepare(&self, data: FetchPrepareData<'a>) -> Option<Self::Prepared> {
        let changes = data
            .arch
            .removals(self.component.key())
            .unwrap_or(&EMPTY_CHANGELIST);

        Some(PreparedKindFilter::new((), changes, data.old_tick))
    }

    fn filter_arch(&self, arch: &Archetype) -> bool {
        true
    }

    fn access(&self, data: FetchPrepareData) -> Vec<Access> {
        vec![Access {
            kind: crate::AccessKind::ChangeEvent {
                id: data.arch_id,
                component: self.component.key(),
            },
            mutable: false,
        }]
    }

    fn describe(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "removed {}", self.component.name())
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::assert_eq;

    use crate::filter::FilterIter;

    use super::*;

    #[test]
    fn filter_slices() {
        let changes = [
            Change::new(Slice::new(10, 20), 3),
            Change::new(Slice::new(20, 22), 4),
            Change::new(Slice::new(30, 80), 3),
            Change::new(Slice::new(100, 200), 4),
        ];

        let mut filter = PreparedKindFilter::new((), &changes[..], 2);

        assert_eq!(filter.filter_slots(Slice::new(0, 10)), Slice::empty());
        assert_eq!(filter.filter_slots(Slice::new(0, 50)), Slice::new(10, 20));
        assert_eq!(filter.filter_slots(Slice::new(20, 50)), Slice::new(20, 22));
        assert_eq!(filter.filter_slots(Slice::new(22, 50)), Slice::new(30, 50));

        assert_eq!(filter.filter_slots(Slice::new(0, 10)), Slice::empty());
        // Due to modified state
        assert_eq!(filter.filter_slots(Slice::new(0, 50)), Slice::new(30, 50));

        assert_eq!(
            filter.filter_slots(Slice::new(120, 500)),
            Slice::new(120, 200)
        );
    }

    #[test]
    fn filter_slices_consume() {
        let changes = [
            Change::new(Slice::new(10, 20), 3),
            Change::new(Slice::new(20, 22), 4),
            Change::new(Slice::new(30, 80), 3),
            Change::new(Slice::new(100, 200), 4),
        ];

        let filter = PreparedKindFilter::new((), &changes[..], 2);

        let slices = FilterIter::new(Slice::new(0, 500), filter).collect_vec();

        assert_eq!(
            &[
                Slice::new(10, 20),
                Slice::new(20, 22),
                Slice::new(30, 80),
                Slice::new(100, 200),
            ],
            &slices[..]
        );
    }

    #[test]
    fn filter_slices_partial() {
        let changes = [
            Change::new(Slice::new(10, 20), 3),
            Change::new(Slice::new(20, 22), 4),
            Change::new(Slice::new(30, 80), 3),
            Change::new(Slice::new(100, 200), 4),
        ];

        let filter = PreparedKindFilter::new((), &changes[..], 2);

        let slices = FilterIter::new(Slice::new(25, 150), filter)
            .take(100)
            .collect_vec();

        assert_eq!(&[Slice::new(30, 80), Slice::new(100, 150),], &slices[..]);
    }
}
