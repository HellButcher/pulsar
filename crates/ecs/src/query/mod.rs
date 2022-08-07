use std::sync::{
    atomic::{AtomicPtr, AtomicUsize, Ordering},
    Mutex,
};

pub use self::exec::Query;
use crate::{
    archetype::{Archetype, ArchetypeId, ArchetypeSet},
    component::{ComponentId, ComponentSet, Components},
    WorldInner,
};

// mostly based on `hecs` (https://github.com/Ralith/hecs/blob/9a2405c703ea0eb6481ad00d55e74ddd226c1494/src/query.rs)

/// A collection of component types to fetch from a [`World`](crate::World)
pub trait QueryParam {
    /// The type of the data which can be cached to speed up retrieving
    /// the relevant type states from a matching [`Archetype`]
    type Prepared: Send + Sync + Sized + Copy + 'static;

    type State: Sized + Copy + 'static;

    type Borrow: for<'w> QueryBorrow<'w, Prepared = Self::Prepared>;

    /// Looks up data that can be re-used between multiple query invocations
    fn prepare(res: &mut Resources, components: &mut Components) -> Self::Prepared;

    fn update_access(
        prepared: Self::Prepared,
        shared: &mut ComponentSet,
        exclusive: &mut ComponentSet,
    );

    /// Checks if the archetype matches the query
    fn matches_archetype(prepared: Self::Prepared, archetype: &Archetype) -> bool;

    fn state(prepared: Self::Prepared, archetype: &Archetype) -> Self::State;
}

pub trait QueryBorrow<'w>: QueryParam<Borrow = Self> {
    type Borrowed: Send;

    #[doc(hidden)]
    type Fetch: for<'a> QueryFetch<'w, 'a>;

    /// Acquire dynamic borrows from `archetype`
    fn borrow(res: &'w ResourcesSend, prepared: Self::Prepared) -> Self::Borrowed;
}

/// Type of values yielded by a query
pub type QueryItem<'w, 'a, Q> = <<Q as QueryBorrow<'w>>::Fetch as QueryFetch<'w, 'a>>::Item;

pub trait QueryFetch<'w, 'a>: QueryBorrow<'w, Fetch = Self> {
    /// Type of value to be fetched
    type Item;

    /// Access the given item in this archetype
    fn get(
        this: &'a mut Self::Borrowed,
        state: Self::State,
        archetype: &Archetype,
        index: usize,
    ) -> Self::Item
    where
        'w: 'a;
}

pub mod exec;
mod fetch;
mod filter;
pub use fetch::*;
pub use filter::*;
use pulz_schedule::resource::{FromResources, ResourceId, Resources, ResourcesSend};

struct QueryState<Q>
where
    Q: QueryParam,
{
    resource_id: ResourceId<WorldInner>,
    prepared: Q::Prepared,
    shared_access: ComponentSet,
    exclusive_access: ComponentSet,

    sparse_only: bool,
    last_archetype_index: AtomicUsize,
    updating_archetypes: Mutex<()>,
    matching_archetypes_p: AtomicPtr<ArchetypeSet>,
}

impl<Q> QueryState<Q>
where
    Q: QueryParam,
{
    pub fn new(resources: &mut Resources) -> Self {
        let world_id = resources.init::<WorldInner>();
        let mut world = resources.remove_id(world_id).unwrap();
        let result = Self::from_world(resources, &mut world, world_id);
        resources.insert_again(world);
        result
    }

    fn from_world(
        resources: &mut Resources,
        world: &mut WorldInner,
        resource_id: ResourceId<WorldInner>,
    ) -> Self {
        let prepared = Q::prepare(resources, &mut world.components);
        let mut shared_access = ComponentSet::new();
        let mut exclusive_access = ComponentSet::new();
        Q::update_access(prepared, &mut shared_access, &mut exclusive_access);

        let sparse_only = shared_access
            .iter(&world.components)
            .chain(exclusive_access.iter(&world.components))
            .all(ComponentId::is_sparse);

        let query = Self {
            resource_id,
            prepared,
            shared_access,
            exclusive_access,
            sparse_only,
            last_archetype_index: AtomicUsize::new(0),
            updating_archetypes: Mutex::new(()),
            matching_archetypes_p: AtomicPtr::new(std::ptr::null_mut()),
        };
        query.update_archetypes(world);
        query
    }

    fn update_archetypes(&self, world: &WorldInner) {
        let archetypes = &world.archetypes;
        let last_archetype_index = archetypes.len();
        let old_archetype_index = self.last_archetype_index.load(Ordering::Relaxed);
        if old_archetype_index >= last_archetype_index {
            // no new archetypes
            return;
        }
        let lock = self.updating_archetypes.lock();

        let mut archetypes_scratch: Option<Box<ArchetypeSet>> = None;

        for index in old_archetype_index..last_archetype_index {
            let id = ArchetypeId::new(index);
            let archetype = &archetypes[id];
            if Q::matches_archetype(self.prepared, archetype) {
                // init scratch
                let scratch = archetypes_scratch.get_or_insert_with(|| {
                    let ptr = self.matching_archetypes_p.load(Ordering::Relaxed);
                    if ptr.is_null() {
                        Default::default()
                    } else {
                        unsafe { Box::new((*ptr).clone()) }
                    }
                });
                // indert archetype
                scratch.insert(id);
            }
        }

        if let Some(new) = archetypes_scratch {
            // replace
            let old = self
                .matching_archetypes_p
                .swap(Box::into_raw(new), Ordering::Relaxed);
            if !old.is_null() {
                unsafe { drop(Box::from_raw(old)) }
            }
        }

        self.last_archetype_index
            .store(last_archetype_index, Ordering::Relaxed);

        drop(lock);
    }

    fn matching_archetypes(&self) -> &ArchetypeSet {
        static EMPTY: ArchetypeSet = ArchetypeSet::new();
        let ptr = self.matching_archetypes_p.load(Ordering::Relaxed);
        if ptr.is_null() {
            &EMPTY
        } else {
            unsafe { &*ptr }
        }
    }
}

impl<Q: QueryParam> FromResources for QueryState<Q> {
    fn from_resources(resources: &mut Resources) -> Self {
        Self::new(resources)
    }
}

#[cfg(test)]
mod test {

    use pulz_schedule::resource::Resources;

    use crate::{component::Component, prelude::Query, WorldExt};

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
    struct A(usize);

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
    #[component(storage = "crate::storage::DenseStorage")]
    struct B(usize);

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
    #[component(sparse)]
    struct C(usize);

    #[derive(Debug, Copy, Clone, PartialEq, Eq, Component)]
    #[component(storage = "DenseStorage")] // shortcut for `pulz_ecs::storage::DenseStorage`
    struct D(usize);

    #[test]
    fn test_query() {
        let mut resources = Resources::new();
        let mut entities = Vec::new();
        {
            let mut world = resources.world_mut();
            for i in 0..1000 {
                let entity = match i % 4 {
                    1 => world.spawn().insert(A(i)).id(),
                    2 => world.spawn().insert(B(i)).id(),
                    _ => world.spawn().insert(A(i)).insert(B(i)).id(),
                };
                entities.push(entity);
            }
        }

        let mut q1 = Query::<&A>::new(&mut resources);
        //let r = q1.one(&world, entities[1]).map(|mut o| *o.get());
        let r = q1.get(entities[1]).copied();
        assert_eq!(Some(A(1)), r);

        let mut counter1 = 0;
        let mut sum1 = 0;
        for a in q1.iter() {
            counter1 += 1;
            sum1 += a.0;
        }

        assert_eq!(750, counter1);
        assert_eq!(374500, sum1);
        drop(q1);

        let mut q2 = Query::<(&A, &B)>::new(&mut resources);
        let mut counter2 = 0;
        let mut sum2a = 0;
        let mut sum2b = 0;
        for (a, b) in q2.iter() {
            counter2 += 1;
            sum2a += a.0;
            sum2b += b.0;
        }
        assert_eq!(500, counter2);
        assert_eq!(249750, sum2a);
        assert_eq!(249750, sum2b);
        drop(q2);

        let mut q3 = Query::<(&B,)>::new(&mut resources);
        let mut counter3 = 0;
        let mut sum3 = 0;
        for (b,) in q3.iter() {
            counter3 += 1;
            sum3 += b.0;
        }
        assert_eq!(750, counter3);
        assert_eq!(374750, sum3);
        drop(q3);

        let mut q1 = Query::<&A>::new(&mut resources);
        let mut counter4 = 0;
        let mut sum4 = 0;
        for a in q1.iter() {
            counter4 += 1;
            sum4 += a.0;
        }
        assert_eq!(750, counter4);
        assert_eq!(374500, sum4);
    }
}
