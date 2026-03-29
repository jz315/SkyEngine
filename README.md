# SkyEngine

SkyEngine 当前还是一个以 ECS 为核心的引擎原型，重点在 simulation/runtime 基础，而不是完整图形或编辑器。

如果你想先理解现在这套 ECS 怎么用，先看：

- [ECS_GUIDE.md](/C:/Coding/SkyEngine/ECS_GUIDE.md)
- [ENGINE_PLAN.md](/C:/Coding/SkyEngine/ENGINE_PLAN.md)
- [BENCHMARKS.md](/C:/Coding/SkyEngine/BENCHMARKS.md)

当前 ECS 已经支持：

- `spawn((...))`
- `despawn`
- `get / get_mut`
- `insert / remove component`
- `resource`
- `Commands`
- `Schedule`
- `SystemContext`
- `SystemGroup`
- typed query
- `ecs::advanced`

代码入口主要在：

- [world.rs](/C:/Coding/SkyEngine/src/ecs/world.rs)
- [query.rs](/C:/Coding/SkyEngine/src/ecs/query.rs)
- [commands.rs](/C:/Coding/SkyEngine/src/ecs/commands.rs)
- [system.rs](/C:/Coding/SkyEngine/src/ecs/system.rs)
- [advanced.rs](/C:/Coding/SkyEngine/src/ecs/advanced.rs)
