# Render Backend Contract

## Role
- `src/render/backend/` owns app-facing renderer selection and backend adapters.
- `create_scene_renderer(window, vsync, pipeline)` chooses a concrete `SceneRenderer` from `RenderPipelineAsset::backend_kind()`.
- `SceneRenderer` is the runner-facing trait used by `App`: begin/end frame, render world, resize, stats, backend names, and optional wgpu accessors.

## Current Backends
- `WgpuSceneRenderer` is the default backend. It owns `GpuContext`, optional `RenderRuntime`, and `WgpuRenderAssetCache`.
- `KajiyaSceneRenderer` is compiled behind `kajiya-renderer` and consumes `SceneSnapshot`.
- `RenderlingSceneRenderer` is compiled behind `renderling-renderer` and consumes `SceneSnapshot`.
- If no render pipeline is supplied, backend selection defaults to wgpu without a `RenderRuntime`; calling `FrameContext::render()` still requires an installed `RenderPlugin`.

## Wgpu Runtime Boundary
- The wgpu backend is allowed to call `RenderRuntime::render_world(gpu, world)`.
- `WgpuRenderAssetCache` adapts CPU `AssetServer` render assets into `RenderRuntime` mesh/material/texture resources.
- Keep wgpu-only escape hatches behind `SceneRenderer::wgpu*` and `SceneRenderer::wgpu_render_runtime*`.
- Do not move generic app-runner behavior into `WgpuSceneRenderer`; app lifecycle remains in `src/app/runner.rs`.

## SceneSnapshot Boundary
- `SceneSnapshot` is the neutral 3D scene payload shared by Kajiya and Renderling.
- Snapshot-backed backends should use `SceneSnapshotExtractor`; they must not add parallel ad-hoc ECS queries.
- `SceneSnapshot` currently covers cameras, mesh instances, directional/point/spot lights, render settings, layer masks, and shadow metadata.
- Do not use `SceneSnapshot` as the universal schema for the wgpu `RenderRuntime`; it is specifically the backend-neutral path for alternate scene renderers.

## Kajiya Boundary
- SkyEngine's Kajiya adapter lives in `src/render/backend/kajiya/`; vendored upstream code stays in `crates/vendor/kajiya/`.
- Kajiya types may only appear in `src/render/backend/kajiya/` and vendored Kajiya code.
- User code, examples, scenes, and public docs use SkyEngine components and assets.
- Public entry remains `RenderPipelineAsset::kajiya_3d()`; do not expose `kajiya::...`.

## Renderling Boundary
- SkyEngine's Renderling adapter lives in `src/render/backend/renderling.rs`.
- Renderling types should stay inside that file/module boundary and should not leak into public examples, components, or docs.
- Public entry remains `RenderPipelineAsset::renderling_3d()`; user-facing scene data stays SkyEngine ECS/assets.

## Asset Flow
- SkyEngine owns user assets through `AssetServer`, `MeshAsset`, texture assets, and material assets.
- Wgpu receives converted runtime resources through `WgpuRenderAssetCache`.
- Kajiya and Renderling receive converted/baked backend cache payloads only.
- Alternate backends must not directly load user scene assets or define their own public scene component set.

## Runtime Files
- `crates/vendor/kajiya` is read-only at runtime.
- Generated Kajiya runtime cache goes under `target/sky-kajiya-cache`.
- Kajiya VFS/cache paths and env-derived settings must flow through `KajiyaRendererConfig`.

## Error Handling
- Backend creation errors convert to `SceneRendererInitError`.
- Per-frame backend errors convert to `SceneRendererError`.
- Kajiya adapter code uses `KajiyaBackendError` internally and converts at the outer scene-renderer boundary.
- Renderling initialization/runtime errors should use `SceneRendererInitError::Other` or `SceneRendererError::Other` at the boundary until a dedicated error enum is warranted.
