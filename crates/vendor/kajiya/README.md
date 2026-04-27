# Vendored Kajiya Runtime

This directory contains the subset of Embark's Kajiya renderer that SkyEngine
builds against for the experimental `kajiya-renderer` feature.

Kept:
- library crates needed by `sky_engine`: `kajiya`, `kajiya-asset`,
  `kajiya-backend`, `kajiya-rg`, and `rust-shaders-shared`
- runtime shader assets under `assets/shaders`
- precompiled rust-gpu shader metadata under `assets/rust-shaders-compiled`
- blue-noise images used by Kajiya's default world renderer
- the Windows DXC runtime DLL used by HLSL compilation

Intentionally not kept:
- Kajiya's example/viewer/baker binaries
- upstream docs, CI metadata, scripts, and build output
- upstream sample scenes and mesh packs
- generated cache files, which SkyEngine writes under `target/sky-kajiya-cache`

SkyEngine's adapter layer lives in `src/render/backend/kajiya*.rs`; code outside
that layer should use normal SkyEngine render components and
`RenderPipelineAsset::kajiya_3d()`.
