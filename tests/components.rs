use flax::{buffer::ComponentBuffer, vtable::ComponentVTable, *};
use glam::{vec2, Vec2};

#[test]
fn custom_component() {
    let mut world = World::new();

    static VTABLE: &ComponentVTable<Vec2> = &ComponentVTable::new("position", |info| {
        let mut buf = ComponentBuffer::new();
        <Debuggable as Metadata<Vec2>>::attach(info, &mut buf);
        buf
    });

    let position = world.spawn_component(VTABLE);

    let id = Entity::builder()
        .set(position, vec2(1.0, 6.4))
        .spawn(&mut world);

    assert_eq!(world.get(id, position).as_deref(), Ok(&vec2(1.0, 6.4)));

    // When `position` is despawned, it is removed from all entities.
    // This ensured that dead components never exist
    world.despawn(position.id()).unwrap();

    assert_eq!(
        world.get(id, position).as_deref(),
        Err(&Error::MissingComponent(id, position.info())),
    );
}
