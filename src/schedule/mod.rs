use crate::{error::SystemResult, system::SystemContext, BoxedSystem, CommandBuffer, World};

/// A collection of systems to run on the world
pub struct Schedule {
    systems: Vec<BoxedSystem>,
}

impl Schedule {
    pub fn new() -> Self {
        Self {
            systems: Vec::new(),
        }
    }

    /// Add a new system to the schedule.
    /// Respects order.
    pub fn with_system(&mut self, system: impl Into<BoxedSystem>) -> &mut Self {
        self.systems.push(system.into());
        self
    }

    /// Execute all systems in the schedule sequentially on the world.
    /// Returns the first error and aborts if the execution fails.
    pub fn execute_seq(&mut self, world: &mut World) -> eyre::Result<()> {
        let mut cmd = CommandBuffer::new();
        let ctx = SystemContext::new(world, &mut cmd);
        self.systems
            .iter_mut()
            .try_for_each(|system| system.execute(&ctx))?;

        Ok(())
    }
}

impl Default for Schedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod test {

    use crate::{schedule::Schedule, system::System, EntityBuilder, Query, QueryData, World};

    #[test]
    fn schedule_seq() {
        component! {
            a: String,
            b: i32,
        };

        let mut world = World::new();

        let id = EntityBuilder::new()
            .set(a(), "Foo".to_string())
            .set(b(), 5)
            .spawn(&mut world);

        let mut prev_count: i32 = 0;
        let system_a = System::builder()
            .with(Query::new(a()))
            .build(move |mut a: QueryData<_>| {
                let count = a.prepare().iter().count() as i32;

                eprintln!("Change: {prev_count} -> {count}");
                prev_count = count;
            });

        let system_b = System::builder().with(Query::new(b())).build(
            move |mut query: QueryData<_>| -> eyre::Result<()> {
                let mut query = query.prepare();
                let item: &i32 = query.get(id)?;
                eprintln!("Item: {item}");

                Ok(())
            },
        );

        let mut schedule = Schedule::new();
        schedule.with_system(system_a).with_system(system_b);

        schedule.execute_seq(&mut world).unwrap();

        world.despawn(id).unwrap();
        let result: eyre::Result<()> = schedule.execute_seq(&mut world).map_err(Into::into);

        eprintln!("{result:?}");
        assert!(result.is_err());
    }
}
