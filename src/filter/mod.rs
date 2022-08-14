mod cmp;

use std::{
    iter::FusedIterator,
    ops::{BitAnd, Neg},
};

use atomic_refcell::AtomicRef;

use crate::{
    archetype::{Archetype, ChangeKind, Changes, Slice},
    Access, ArchetypeId, ComponentId, World,
};

pub use cmp::CmpExt;

macro_rules! gen_bitops {
    ($ty:ident[$($p: tt),*]) => {
        impl<R, $($p),*> std::ops::BitOr<R> for $ty<$($p),*>
        {
            type Output = Or<Self, R>;

            fn bitor(self, rhs: R) -> Self::Output {
                Or::new(self, rhs)
            }
        }

        impl<R, $($p),*> std::ops::BitAnd<R> for $ty<$($p),*>
        {
            type Output = And<Self, R>;

            fn bitand(self, rhs: R) -> Self::Output {
                And::new(self, rhs)
            }
        }

        impl<$($p),*> std::ops::Neg for $ty<$($p),*>
        {
            type Output = Not<Self>;

            fn neg(self) -> Self::Output {
                Not(self)
            }
        }
    };


    ($($ty:ident[$($p: tt),*];)*) => {
        $(
            gen_bitops!{ $ty[$($p),*] }
        )*
    }
}

gen_bitops! {
    ModifiedFilter[];
    InsertedFilter[];
    RemovedFilter[];
    And[A,B];
    Or[A,B];
    All[];
    Nothing[];
    With[];
    Without[];
}

/// A filter which does not depend upon any state, such as a `with` filter
pub trait StaticFilter {
    fn static_matches(&self, arch: &Archetype) -> bool;
}

/// A filter over a query which will be prepared for an archetype, yielding
/// subsets of slots.
///
/// A filter requires Debug for error messages for user conveniance
pub trait Filter<'w>
where
    Self: Sized + std::fmt::Debug,
{
    type Prepared: PreparedFilter + 'w;

    /// Prepare the filter for an archetype.
    /// `change_tick` refers to the last time this query was run. Useful for
    /// change detection.
    fn prepare(&'w self, archetype: &'w Archetype, change_tick: u32) -> Self::Prepared;

    /// Returns true if the filter will yield at least one entity from the
    /// archetype.
    ///
    /// Returns false if an entity will never yield, such as a mismatched
    /// archetype
    fn matches(&self, arch: &Archetype) -> bool;
    /// Returns which components and how will be accessed for an archetype.
    fn access(&self, id: ArchetypeId, arch: &Archetype) -> Vec<Access>;
}

pub trait PreparedFilter {
    /// Filters a slice of entity slots and returns a subset of the slice
    fn filter(&mut self, slots: Slice) -> Slice;
}

#[derive(Debug, Clone)]
pub struct ModifiedFilter {
    component: ComponentId,
}

impl ModifiedFilter {
    pub fn new(component: ComponentId) -> Self {
        Self { component }
    }
}

impl<'a> Filter<'a> for ModifiedFilter {
    type Prepared = PreparedOr<PreparedKindFilter<'a>, PreparedKindFilter<'a>>;

    fn prepare(&'a self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedOr {
            left: PreparedKindFilter::new(
                archetype,
                self.component,
                change_tick,
                ChangeKind::Modified,
            ),
            right: PreparedKindFilter::new(
                archetype,
                self.component,
                change_tick,
                ChangeKind::Inserted,
            ),
        }
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        archetype.has(self.component)
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        if self.matches(archetype) {
            vec![Access {
                kind: crate::AccessKind::Archetype {
                    id,
                    component: self.component,
                },
                mutable: false,
            }]
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone)]
pub struct InsertedFilter {
    component: ComponentId,
}

impl InsertedFilter {
    pub fn new(component: ComponentId) -> Self {
        Self { component }
    }
}

impl<'a> Filter<'a> for InsertedFilter {
    type Prepared = PreparedKindFilter<'a>;

    fn prepare(&self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedKindFilter::new(archetype, self.component, change_tick, ChangeKind::Inserted)
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        archetype.has(self.component)
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        if self.matches(archetype) {
            vec![Access {
                kind: crate::AccessKind::Archetype {
                    id,
                    component: self.component,
                },
                mutable: false,
            }]
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone)]
pub struct RemovedFilter {
    component: ComponentId,
}

impl RemovedFilter {
    pub fn new(component: ComponentId) -> Self {
        Self { component }
    }
}

impl<'a> Filter<'a> for RemovedFilter {
    type Prepared = PreparedKindFilter<'a>;

    fn prepare(&self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedKindFilter::new(archetype, self.component, change_tick, ChangeKind::Removed)
    }

    fn matches(&self, _: &Archetype) -> bool {
        true
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        if self.matches(archetype) {
            vec![Access {
                kind: crate::AccessKind::Archetype {
                    id,
                    component: self.component,
                },
                mutable: false,
            }]
        } else {
            vec![]
        }
    }
}

#[derive(Debug, Clone)]
pub struct And<L, R> {
    left: L,
    right: R,
}

impl<L, R> And<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<'a, L, R> Filter<'a> for And<L, R>
where
    L: Filter<'a>,
    R: Filter<'a>,
{
    type Prepared = PreparedAnd<L::Prepared, R::Prepared>;

    fn prepare(&'a self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedAnd {
            left: self.left.prepare(archetype, change_tick),
            right: self.right.prepare(archetype, change_tick),
        }
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        self.left.matches(archetype) && self.right.matches(archetype)
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        let mut res = self.left.access(id, archetype);
        res.append(&mut self.right.access(id, archetype));
        res
    }
}

impl<L, R> StaticFilter for And<L, R>
where
    L: StaticFilter,
    R: StaticFilter,
{
    fn static_matches(&self, archetype: &Archetype) -> bool {
        self.left.static_matches(archetype) && self.right.static_matches(archetype)
    }
}

#[derive(Debug, Clone)]
pub struct Or<L, R> {
    left: L,
    right: R,
}

impl<L, R> Or<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<'a, L, R> Filter<'a> for Or<L, R>
where
    L: Filter<'a>,
    R: Filter<'a>,
{
    type Prepared = PreparedOr<L::Prepared, R::Prepared>;

    fn prepare(&'a self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedOr {
            left: self.left.prepare(archetype, change_tick),
            right: self.right.prepare(archetype, change_tick),
        }
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        self.left.matches(archetype) || self.right.matches(archetype)
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        let mut accesses = self.left.access(id, archetype);
        accesses.append(&mut self.right.access(id, archetype));
        accesses
    }
}

impl<L, R> StaticFilter for Or<L, R>
where
    L: StaticFilter,
    R: StaticFilter,
{
    fn static_matches(&self, archetype: &Archetype) -> bool {
        self.left.static_matches(archetype) || self.right.static_matches(archetype)
    }
}

#[derive(Debug)]
pub struct PreparedKindFilter<'a> {
    changes: Option<AtomicRef<'a, Changes>>,
    cur: Option<Slice>,
    // The current change group.
    // Starts at the end and decrements
    index: usize,
    tick: u32,
    kind: ChangeKind,
}

impl<'a> PreparedKindFilter<'a> {
    pub fn new(
        archetype: &'a Archetype,
        component: ComponentId,
        tick: u32,
        kind: ChangeKind,
    ) -> Self {
        let changes = archetype.changes(component);
        Self {
            changes,
            cur: None,
            index: 0,
            tick,
            kind,
        }
    }

    #[cfg(test)]
    fn from_borrow(changes: AtomicRef<'a, Changes>, tick: u32, kind: ChangeKind) -> Self {
        Self {
            changes: Some(changes),
            cur: None,
            index: 0,
            tick,
            kind,
        }
    }

    pub fn current_slice(&mut self) -> Option<Slice> {
        match (self.cur, self.changes.as_mut()) {
            (Some(v), _) => Some(v),
            (None, Some(changes)) => loop {
                let v = changes.get(self.index);
                if let Some(change) = v {
                    self.index += 1;
                    if change.tick > self.tick && self.kind == change.kind {
                        break Some(*self.cur.get_or_insert(change.slice));
                    }
                } else {
                    // No more
                    return None;
                };
            },
            _ => None,
        }
    }
}

impl<'a> PreparedFilter for PreparedKindFilter<'a> {
    fn filter(&mut self, slots: Slice) -> Slice {
        loop {
            let cur = match self.current_slice() {
                Some(v) => v,
                None => return Slice::empty(),
            };

            let intersect = cur.intersect(&slots);
            // Try again with the next change group
            if intersect.is_empty() {
                self.cur = None;
                continue;
            } else {
                return intersect;
            }
        }
    }
}

/// Or filter combinator
pub struct PreparedOr<L, R> {
    left: L,
    right: R,
}

impl<L, R> PreparedOr<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R> PreparedFilter for PreparedOr<L, R>
where
    L: PreparedFilter,
    R: PreparedFilter,
{
    fn filter(&mut self, slots: Slice) -> Slice {
        let l = self.left.filter(slots);
        let r = self.right.filter(slots);
        let u = l.union(&r);
        match u {
            Some(v) => v,
            None => {
                // The slices where not contiguous
                // Return the left half for this run.
                // The right will be kept
                l
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Not<T>(pub T);

impl<'a, T> Filter<'a> for Not<T>
where
    T: Filter<'a>,
{
    type Prepared = PreparedNot<T::Prepared>;

    fn prepare(&'a self, archetype: &'a Archetype, change_tick: u32) -> Self::Prepared {
        PreparedNot(self.0.prepare(archetype, change_tick))
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        !self.0.matches(archetype)
    }

    fn access(&self, id: ArchetypeId, archetype: &Archetype) -> Vec<Access> {
        self.0.access(id, archetype)
    }
}

impl<T> StaticFilter for Not<T>
where
    T: StaticFilter,
{
    fn static_matches(&self, archetype: &Archetype) -> bool {
        !self.0.static_matches(archetype)
    }
}

impl<R, T> std::ops::BitOr<R> for Not<T>
where
    Self: for<'x> Filter<'x>,
    R: for<'x> Filter<'x>,
{
    type Output = Or<Self, R>;

    fn bitor(self, rhs: R) -> Self::Output {
        Or::new(self, rhs)
    }
}

impl<R, T> std::ops::BitAnd<R> for Not<T>
where
    Self: for<'x> Filter<'x>,
    R: for<'x> Filter<'x>,
{
    type Output = And<Self, R>;

    fn bitand(self, rhs: R) -> Self::Output {
        And::new(self, rhs)
    }
}

impl<T> Neg for Not<T>
where
    T: for<'x> Filter<'x>,
{
    type Output = T;

    fn neg(self) -> Self::Output {
        self.0
    }
}

pub struct PreparedNot<T>(T);

impl<T> PreparedFilter for PreparedNot<T>
where
    T: PreparedFilter,
{
    fn filter(&mut self, slots: Slice) -> Slice {
        let a = self.0.filter(slots);

        slots.difference(a).unwrap()
    }
}

/// And filter combinator
pub struct PreparedAnd<L, R> {
    left: L,
    right: R,
}

impl<L, R> PreparedAnd<L, R> {
    pub fn new(left: L, right: R) -> Self {
        Self { left, right }
    }
}

impl<L, R> PreparedFilter for PreparedAnd<L, R>
where
    L: PreparedFilter,
    R: PreparedFilter,
{
    fn filter(&mut self, slots: Slice) -> Slice {
        let l = self.left.filter(slots);
        let r = self.right.filter(slots);

        let i = l.intersect(&r);
        if i.is_empty() {
            // Go again but start with the highest bound
            // This is caused by one of the sides being past the end of the
            // other slice. As such, force the slice lagging behind to catch up
            // to the upper floor
            let max = l.start.max(r.start).min(slots.end);

            let slots = Slice::new(max, slots.end);
            let l = self.left.filter(slots);
            let r = self.right.filter(slots);
            l.intersect(&r)
        } else {
            i
        }
    }
}

#[derive(Debug, Clone)]
pub struct Nothing;

impl<'a> Filter<'a> for Nothing {
    type Prepared = BooleanFilter;

    fn prepare(&self, _: &'a Archetype, _: u32) -> Self::Prepared {
        BooleanFilter(false)
    }

    fn matches(&self, _: &Archetype) -> bool {
        false
    }

    fn access(&self, _: ArchetypeId, _: &Archetype) -> Vec<Access> {
        vec![]
    }
}

impl StaticFilter for Nothing {
    fn static_matches(&self, _: &Archetype) -> bool {
        false
    }
}

/// Filter all entities
#[derive(Debug, Clone)]
pub struct All;

impl<'a> Filter<'a> for All {
    type Prepared = BooleanFilter;

    fn prepare(&self, _: &Archetype, _: u32) -> Self::Prepared {
        BooleanFilter(true)
    }

    fn matches(&self, _: &Archetype) -> bool {
        true
    }

    fn access(&self, _: ArchetypeId, _: &Archetype) -> Vec<Access> {
        vec![]
    }
}

impl StaticFilter for All {
    fn static_matches(&self, _: &Archetype) -> bool {
        true
    }
}

#[derive(Debug, Clone)]
pub struct FilterIter<F> {
    slots: Slice,
    filter: F,
}

impl<F> FilterIter<F> {
    pub fn new(slots: Slice, filter: F) -> Self {
        Self { slots, filter }
    }
}

impl<F> Iterator for FilterIter<F>
where
    F: PreparedFilter,
{
    type Item = Slice;

    fn next(&mut self) -> Option<Self::Item> {
        let cur = self.filter.filter(self.slots);

        if cur.is_empty() {
            None
        } else {
            let (_l, m, r) = self
                .slots
                .split_with(&cur)
                .expect("Return value of filter must be a subset of `slots");

            self.slots = r;
            Some(m)
        }
    }
}

impl<F: PreparedFilter> FusedIterator for FilterIter<F> {}

#[derive(Debug, Clone)]
pub struct With {
    component: ComponentId,
}

impl With {
    pub fn new(component: ComponentId) -> Self {
        Self { component }
    }
}

impl StaticFilter for With {
    fn static_matches(&self, arch: &Archetype) -> bool {
        arch.has(self.component)
    }
}

impl<'a> Filter<'a> for With {
    type Prepared = BooleanFilter;

    fn prepare(&self, arch: &Archetype, _: u32) -> Self::Prepared {
        BooleanFilter(self.matches(arch))
    }

    fn matches(&self, arch: &Archetype) -> bool {
        if self.component.is_relation() {
            arch.matches_relation(self.component).next().is_some()
        } else {
            arch.has(self.component)
        }
    }

    fn access(&self, _: ArchetypeId, _: &Archetype) -> Vec<Access> {
        vec![]
    }
}

#[derive(Debug, Clone)]
pub struct Without {
    component: ComponentId,
}

impl Without {
    pub fn new(component: ComponentId) -> Self {
        Self { component }
    }
}

impl<'a> Filter<'a> for Without {
    type Prepared = BooleanFilter;

    fn prepare(&self, arch: &Archetype, _: u32) -> Self::Prepared {
        BooleanFilter(self.matches(arch))
    }

    fn matches(&self, archetype: &Archetype) -> bool {
        if self.component.is_relation() {
            archetype.matches_relation(self.component).next().is_none()
        } else {
            !archetype.has(self.component)
        }
    }

    fn access(&self, _: ArchetypeId, _: &Archetype) -> Vec<Access> {
        vec![]
    }
}

impl StaticFilter for Without {
    fn static_matches(&self, arch: &Archetype) -> bool {
        !arch.has(self.component)
    }
}

pub struct BooleanFilter(bool);

impl PreparedFilter for BooleanFilter {
    fn filter(&mut self, slots: Slice) -> Slice {
        if self.0 {
            slots
        } else {
            Slice::empty()
        }
    }
}

impl<'w, F> Filter<'w> for &F
where
    F: Filter<'w>,
{
    type Prepared = F::Prepared;

    fn prepare(&'w self, archetype: &'w Archetype, change_tick: u32) -> Self::Prepared {
        (*self).prepare(archetype, change_tick)
    }

    fn matches(&self, arch: &Archetype) -> bool {
        (*self).matches(arch)
    }

    fn access(&self, id: ArchetypeId, arch: &Archetype) -> Vec<Access> {
        (*self).access(id, arch)
    }
}

#[cfg(test)]
mod tests {

    use atomic_refcell::AtomicRefCell;
    use itertools::Itertools;

    use crate::{archetype::Change, component};

    use super::*;
    component! {
        a: (),
    }

    #[test]
    fn filter() {
        let mut changes = Changes::new(a().info());

        changes.set(Change::modified(Slice::new(40, 200), 1));
        changes.set(Change::modified(Slice::new(70, 349), 2));
        changes.set(Change::modified(Slice::new(560, 893), 5));
        changes.set(Change::modified(Slice::new(39, 60), 6));
        changes.set(Change::inserted(Slice::new(784, 800), 7));
        changes.set(Change::modified(Slice::new(945, 1139), 8));

        let changes = AtomicRefCell::new(changes);

        let filter = PreparedKindFilter::from_borrow(changes.borrow(), 2, ChangeKind::Modified);

        // The whole "archetype"
        let slots = Slice::new(0, 1238);

        let chunks = FilterIter::new(slots, filter).collect_vec();

        assert_eq!(
            chunks,
            [
                Slice::new(39, 60),
                Slice::new(560, 893),
                Slice::new(945, 1139)
            ]
        );
    }

    #[test]
    fn combinators() {
        let mut changes_1 = Changes::new(a().info());
        let mut changes_2 = Changes::new(a().info());

        changes_1.set(Change::modified(Slice::new(40, 65), 2));
        changes_1.set(Change::modified(Slice::new(59, 80), 3));
        changes_1.set(Change::modified(Slice::new(90, 234), 3));
        changes_2.set(Change::modified(Slice::new(50, 70), 3));
        changes_2.set(Change::modified(Slice::new(99, 210), 4));

        let a_map = changes_1.as_changed_set(1);
        let b_map = changes_2.as_changed_set(2);

        eprintln!("Changes: \n  {changes_1:?}\n  {changes_2:?}");
        let changes_1 = AtomicRefCell::new(changes_1);
        let changes_2 = AtomicRefCell::new(changes_2);

        let slots = Slice::new(0, 1000);

        // Or
        let a = PreparedKindFilter::from_borrow(changes_1.borrow(), 1, ChangeKind::Modified);
        let b = PreparedKindFilter::from_borrow(changes_2.borrow(), 2, ChangeKind::Modified);

        let filter = PreparedOr::new(a, b);

        // Use a brute force BTreeSet for solving it
        let chunks_set = slots
            .iter()
            .filter(|v| a_map.contains(v) || b_map.contains(v))
            .collect_vec();

        let chunks = FilterIter::new(slots, filter).flatten().collect_vec();

        assert_eq!(chunks, chunks_set);

        // And

        let a = PreparedKindFilter::from_borrow(changes_1.borrow(), 1, ChangeKind::Modified);
        let b = PreparedKindFilter::from_borrow(changes_2.borrow(), 2, ChangeKind::Modified);

        let filter = PreparedAnd::new(a, b);

        // Use a brute force BTreeSet for solving it
        let chunks_set = slots
            .iter()
            .filter(|v| a_map.contains(v) && b_map.contains(v))
            .collect_vec();

        let chunks = FilterIter::new(slots, filter).flatten().collect_vec();

        assert_eq!(chunks, chunks_set,);
    }

    #[test]
    fn archetypes() {
        component! {
            a: i32,
            b: String,
            c: u32,
        }

        let archetype = Archetype::new([a().info(), b().info(), c().info()]);

        let filter = ModifiedFilter::new(a().id()) & (ModifiedFilter::new(b().id()))
            | (ModifiedFilter::new(c().id()));

        // Mock changes
        let a_map = archetype
            .changes_mut(a().id())
            .unwrap()
            .set(Change::modified(Slice::new(9, 80), 2))
            .set(Change::modified(Slice::new(65, 83), 4))
            .as_changed_set(1);

        let b_map = archetype
            .changes_mut(b().id())
            .unwrap()
            .set(Change::modified(Slice::new(16, 45), 2))
            .set(Change::modified(Slice::new(68, 85), 2))
            .as_changed_set(1);

        let c_map = archetype
            .changes_mut(c().id())
            .unwrap()
            .set(Change::modified(Slice::new(96, 123), 3))
            .as_changed_set(1);

        // Brute force

        let slots = Slice::new(0, 1000);
        let chunks_set = slots
            .iter()
            .filter(|v| (a_map.contains(v) && b_map.contains(v)) || (c_map.contains(v)))
            .collect_vec();

        let chunks = FilterIter::new(slots, filter.prepare(&archetype, 1))
            .flatten()
            .collect_vec();

        assert_eq!(chunks, chunks_set);
    }
}
