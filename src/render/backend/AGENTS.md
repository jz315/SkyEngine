# Render Backend Contract

## Backend Boundary
- Backends consume `SceneSnapshot`; they must not query ECS directly.
- `SceneSnapshot` is the neutral scene payload shared by WGPU and Kajiya.
- Backend-specific state must remain inside `src/render/backend/`.

## Kajiya Boundary
- SkyEngine's Kajiya adapter lives in `src/render/backend/kajiya/`; vendored upstream code stays in `crates/vendor/kajiya/`.
- Kajiya types may only appear in `src/render/backend/kajiya/` and vendored Kajiya code.
- User code, examples, scenes, and public docs use SkyEngine components and assets.
- Public entry remains `RenderPipelineAsset::kajiya_3d()`; do not expose `kajiya::...`.

## Asset Flow
- SkyEngine owns user assets through `AssetServer`, `MeshAsset`, and material assets.
- Kajiya receives converted/baked backend cache payloads only.
- Kajiya must not directly load user scene assets or query user ECS state.

## Runtime Files
- `crates/vendor/kajiya` is read-only at runtime.
- Generated Kajiya runtime cache goes under `target/sky-kajiya-cache`.
- Kajiya VFS/cache paths and env-derived settings must flow through `KajiyaRendererConfig`.

## Error Handling
- Kajiya adapter code uses `KajiyaBackendError` internally.
- Convert Kajiya errors to `SceneRendererInitError` / `SceneRendererError` only at the outer scene-renderer boundary.
