use criterion::{criterion_group, criterion_main, BenchmarkId, Throughput};
use criterion_cpu_time::PosixTime;

const NUM_ENTITIES: &[usize] = &[5_000, 10_000, 50_000, 100_000 /* 500_000, 1_000_000 */];

criterion_group!(name = world_benches; config = configure_criterion(); targets = world_spawn, world_spawn_sparse, world_many_components);
criterion_main!(world_benches);

type Criterion = criterion::Criterion<PosixTime>;
fn configure_criterion() -> Criterion {
    criterion::Criterion::default()
        .with_measurement(PosixTime::UserAndSystemTime)
        .warm_up_time(std::time::Duration::from_secs(1))
        .measurement_time(std::time::Duration::from_secs(4))
}

#[derive(Copy, Clone)]
struct A(usize);
#[derive(Copy, Clone)]
struct B(usize);
#[derive(Copy, Clone)]
struct C(usize);
#[derive(Copy, Clone)]
struct D(usize);
#[derive(Copy, Clone)]
struct E(usize);
#[derive(Copy, Clone)]
struct F(usize);
#[derive(Copy, Clone)]
struct G(usize);
#[derive(Copy, Clone)]
struct H(usize);
#[derive(Copy, Clone)]
struct I(usize);
#[derive(Copy, Clone)]
struct J(usize);
#[derive(Copy, Clone)]
struct K(usize);
#[derive(Copy, Clone)]
struct L(usize);

// TODO: big number of components / different bigger numbers of components

/// Span a number of entities and change their component configuration
pub fn world_spawn(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn_and_alter_dense");
    for &entity_count in NUM_ENTITIES {
        group.throughput(Throughput::Elements(entity_count as u64));
        group.bench_function(BenchmarkId::new("pulz", entity_count), |bencher| {
            use pulz_ecs::World;
            bencher.iter(|| {
                let mut world = World::new();
                let mut entities = Vec::new();
                for i in 0..entity_count {
                    entities.push(world.spawn().insert(A(i)).insert(B(i)).insert(C(i)).id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    world
                        .entity_mut(*entity)
                        .unwrap()
                        .insert(C(i))
                        .insert(D(i))
                        .insert(E(i));
                }
                for (i, entity) in entities.iter().enumerate() {
                    world
                        .entity_mut(*entity)
                        .unwrap()
                        .remove::<A>()
                        .remove::<C>()
                        .remove::<E>()
                        .insert(F(i))
                        .insert(G(i));
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
        group.bench_function(BenchmarkId::new("bevy", entity_count), |bencher| {
            use bevy_ecs::world::World;
            bencher.iter(|| {
                let mut world = World::new();
                let mut entities = Vec::new();
                for i in 0..entity_count {
                    entities.push(world.spawn().insert(A(i)).insert(B(i)).insert(C(i)).id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    world
                        .entity_mut(*entity)
                        .insert(C(i))
                        .insert(D(i))
                        .insert(E(i));
                }
                for (i, entity) in entities.iter().enumerate() {
                    world.entity_mut(*entity).remove::<A>();
                    world.entity_mut(*entity).remove::<C>();
                    world.entity_mut(*entity).remove::<E>();
                    world.entity_mut(*entity).insert(F(i)).insert(G(i));
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
    }
    group.finish()
}

/// Span a number of entities and change their sparse-component configuration
pub fn world_spawn_sparse(c: &mut Criterion) {
    let mut group = c.benchmark_group("spawn_and_alter_sparse");
    for &entity_count in NUM_ENTITIES {
        group.throughput(Throughput::Elements(entity_count as u64));
        group.bench_function(BenchmarkId::new("pulz", entity_count), |bencher| {
            use pulz_ecs::World;
            bencher.iter(|| {
                let mut world = World::new();
                world.components_mut().insert_sparse::<D>().unwrap();
                world.components_mut().insert_sparse::<E>().unwrap();
                world.components_mut().insert_sparse::<F>().unwrap();
                world.components_mut().insert_sparse::<G>().unwrap();
                let mut entities = Vec::new();
                for i in 0..entity_count {
                    entities.push(world.spawn().insert(A(i)).insert(B(i)).insert(C(i)).id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity).unwrap();
                    match i % 4 {
                        1 => {
                            e.insert(D(i));
                        }
                        2 => {
                            e.insert(E(i));
                        }
                        3 => {
                            e.insert(F(i));
                        }
                        _ => {}
                    }
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity).unwrap();
                    match i % 4 {
                        1 => {
                            e.remove::<D>();
                            e.insert(E(i));
                        }
                        2 => {
                            e.remove::<E>();
                            e.insert(F(i));
                        }
                        3 => {
                            e.remove::<F>();
                            e.insert(G(i));
                        }
                        _ => {}
                    }
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
        group.bench_function(BenchmarkId::new("bevy", entity_count), |bencher| {
            use bevy_ecs::{
                component::{ComponentDescriptor, StorageType},
                world::World,
            };
            bencher.iter(|| {
                let mut world = World::new();
                world
                    .register_component(ComponentDescriptor::new::<D>(StorageType::SparseSet))
                    .unwrap();
                world
                    .register_component(ComponentDescriptor::new::<E>(StorageType::SparseSet))
                    .unwrap();
                world
                    .register_component(ComponentDescriptor::new::<F>(StorageType::SparseSet))
                    .unwrap();
                world
                    .register_component(ComponentDescriptor::new::<G>(StorageType::SparseSet))
                    .unwrap();
                let mut entities = Vec::new();
                for i in 0..entity_count {
                    entities.push(world.spawn().insert(A(i)).insert(B(i)).insert(C(i)).id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity);
                    match i % 4 {
                        1 => {
                            e.insert(D(i));
                        }
                        2 => {
                            e.insert(E(i));
                        }
                        3 => {
                            e.insert(F(i));
                        }
                        _ => {}
                    }
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity);
                    match i % 4 {
                        1 => {
                            e.remove::<D>();
                            e.insert(E(i));
                        }
                        2 => {
                            e.remove::<E>();
                            e.insert(F(i));
                        }
                        3 => {
                            e.remove::<F>();
                            e.insert(G(i));
                        }
                        _ => {}
                    }
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
    }
    group.finish()
}

fn pulz_insert_many_components2<T>(e: &mut pulz_ecs::world::EntityMut, value: T)
where
    T: Send + Sync + Copy + 'static,
{
    e.insert((value, A(1)));
    e.insert((value, B(1)));
    e.insert((value, C(1)));
    e.insert((value, D(1)));
    e.insert((value, E(1)));
    e.insert((value, F(1)));
    e.insert((value, G(1)));
    e.insert((value, H(1)));
    e.insert((value, I(1)));
    e.insert((value, J(1)));
}

fn pulz_insert_many_components<T>(e: &mut pulz_ecs::world::EntityMut, value: T)
where
    T: Send + Sync + Copy + 'static,
{
    pulz_insert_many_components2(e, (value, A(2)));
    pulz_insert_many_components2(e, (value, B(2)));
    pulz_insert_many_components2(e, (value, C(2)));
    pulz_insert_many_components2(e, (value, D(2)));
    pulz_insert_many_components2(e, (value, E(2)));
    pulz_insert_many_components2(e, (value, F(2)));
    pulz_insert_many_components2(e, (value, G(2)));
    pulz_insert_many_components2(e, (value, H(2)));
    pulz_insert_many_components2(e, (value, I(2)));
    pulz_insert_many_components2(e, (value, J(2)));
}

fn bevy_insert_many_components2<T>(e: &mut bevy_ecs::world::EntityMut, value: T)
where
    T: bevy_ecs::component::Component + Copy,
{
    e.insert((value, A(1)));
    e.insert((value, B(1)));
    e.insert((value, C(1)));
    e.insert((value, D(1)));
    e.insert((value, E(1)));
    e.insert((value, F(1)));
    e.insert((value, G(1)));
    e.insert((value, H(1)));
    e.insert((value, I(1)));
    e.insert((value, J(1)));
}

fn bevy_insert_many_components<T>(e: &mut bevy_ecs::world::EntityMut, value: T)
where
    T: bevy_ecs::component::Component + Copy,
{
    bevy_insert_many_components2(e, (value, A(2)));
    bevy_insert_many_components2(e, (value, B(2)));
    bevy_insert_many_components2(e, (value, C(2)));
    bevy_insert_many_components2(e, (value, D(2)));
    bevy_insert_many_components2(e, (value, E(2)));
    bevy_insert_many_components2(e, (value, F(2)));
    bevy_insert_many_components2(e, (value, G(2)));
    bevy_insert_many_components2(e, (value, H(2)));
    bevy_insert_many_components2(e, (value, I(2)));
    bevy_insert_many_components2(e, (value, J(2)));
}

/// Span a number of entities and change their sparse-component configuration
pub fn world_many_components(c: &mut Criterion) {
    let mut group = c.benchmark_group("many_components");
    for component_count in [100, 200, 300] {
        group.throughput(Throughput::Elements(component_count * 1000 as u64));
        group.bench_function(BenchmarkId::new("pulz", component_count), |bencher| {
            use pulz_ecs::World;
            bencher.iter(|| {
                let mut world = World::new();
                let mut entities = Vec::new();
                for i in 0..1000 {
                    let mut e = world.spawn();
                    pulz_insert_many_components(&mut e, A(i));
                    entities.push(e.id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity).unwrap();
                    match i % 2 {
                        1 => {
                            pulz_insert_many_components(&mut e, B(i));
                        }
                        2 => {
                            pulz_insert_many_components(&mut e, C(i));
                        }
                        _ => {}
                    }
                }
                if component_count > 100 {
                    for (i, entity) in entities.iter().enumerate() {
                        let mut e = world.entity_mut(*entity).unwrap();
                        pulz_insert_many_components(&mut e, D(i));
                        // pulz_insert_many_components(&mut e, E(i));
                        // pulz_insert_many_components(&mut e, F(i));
                        // pulz_insert_many_components(&mut e, G(i));
                    }
                }
                if component_count > 200 {
                    for (i, entity) in entities.iter().enumerate() {
                        let mut e = world.entity_mut(*entity).unwrap();
                        pulz_insert_many_components(&mut e, H(i));
                        // pulz_insert_many_components(&mut e, I(i));
                        // pulz_insert_many_components(&mut e, J(i));
                        // pulz_insert_many_components(&mut e, K(i));
                        // pulz_insert_many_components(&mut e, L(i));
                    }
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
        group.bench_function(BenchmarkId::new("bevy", component_count), |bencher| {
            use bevy_ecs::world::World;
            bencher.iter(|| {
                let mut world = World::new();
                let mut entities = Vec::new();
                for i in 0..1000 {
                    let mut e = world.spawn();
                    bevy_insert_many_components(&mut e, A(i));
                    entities.push(e.id());
                }
                for (i, entity) in entities.iter().enumerate() {
                    let mut e = world.entity_mut(*entity);
                    match i % 2 {
                        1 => {
                            bevy_insert_many_components(&mut e, B(i));
                        }
                        2 => {
                            bevy_insert_many_components(&mut e, C(i));
                        }
                        _ => {}
                    }
                }
                if component_count > 100 {
                    for (i, entity) in entities.iter().enumerate() {
                        let mut e = world.entity_mut(*entity);
                        bevy_insert_many_components(&mut e, D(i));
                        // bevy_insert_many_components(&mut e, E(i));
                        // bevy_insert_many_components(&mut e, F(i));
                        // bevy_insert_many_components(&mut e, G(i));
                    }
                }
                if component_count > 200 {
                    for (i, entity) in entities.iter().enumerate() {
                        let mut e = world.entity_mut(*entity);
                        bevy_insert_many_components(&mut e, H(i));
                        // bevy_insert_many_components(&mut e, I(i));
                        // bevy_insert_many_components(&mut e, J(i));
                        // bevy_insert_many_components(&mut e, K(i));
                        // bevy_insert_many_components(&mut e, L(i));
                    }
                }
                for entity in entities {
                    world.despawn(entity);
                }
                drop(world)
            });
        });
    }
    group.finish()
}
