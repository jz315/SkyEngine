# live2d-cubism-core-sys

Raw Rust FFI bindings to the **Live2D Cubism SDK Native Core (v5)**.

> **This crate does not include the Live2D Cubism Core library.**
> You must download the SDK separately from the [Live2D official site](https://www.live2d.com/en/download/cubism-sdk/) and accept its license agreement before use.
>
> The MIT license of this crate covers **only** the Rust binding code, not the Cubism Core library itself.

## Quick Start

### 1. Install the crate

```toml
[dependencies]
live2d-cubism-core-sys = "0.1"
```

### 2. Download the Cubism SDK for Native

Get it from: <https://www.live2d.com/en/download/cubism-sdk/>

### 3. Set the SDK path and build

**Option A** — Point to the full SDK root:

```powershell
$env:LIVE2D_CUBISM_SDK_NATIVE_DIR = "C:\CubismSdkForNative-5-r.5"
cargo build
```

**Option B** — Point directly to the Core library and include directories (useful for CI, Nix, vcpkg, or custom layouts):

```powershell
$env:CUBISM_CORE_LIB_DIR = "C:\CubismSdkForNative-5-r.5\Core\lib\windows\x86_64\143"
$env:CUBISM_CORE_INCLUDE_DIR = "C:\CubismSdkForNative-5-r.5\Core\include"
cargo build
```

Option B takes priority over Option A when both are set.

## Environment Variables

| Variable | Description |
|----------|-------------|
| `CUBISM_CORE_LIB_DIR` | Direct path to the directory containing the Core static library. **Highest priority.** |
| `CUBISM_CORE_INCLUDE_DIR` | Direct path to the Core header directory. Used alongside `CUBISM_CORE_LIB_DIR`. |
| `LIVE2D_CUBISM_SDK_NATIVE_DIR` | Root of the Cubism SDK for Native installation. The build script derives lib/include paths from the standard SDK directory layout. |

## Platform Support

| Target | Library | Status |
|--------|---------|--------|
| `x86_64-pc-windows-msvc` | `Live2DCubismCore_MD.lib` (v143/142/141) | ✅ Tested |
| `x86_64-apple-darwin` | `libLive2DCubismCore.a` | ✅ Supported |
| `aarch64-apple-darwin` | `libLive2DCubismCore.a` | ✅ Supported |
| `x86_64-unknown-linux-gnu` | `libLive2DCubismCore.a` | ✅ Supported |

> **Note**: Windows builds require the MSVC toolchain. MinGW is not supported by the Cubism Core library.

## SDK Compatibility

Tested against: **Cubism SDK for Native 5-r.5**

This crate tracks the Cubism Core v5 ABI surface. The crate version follows semver independently of the SDK version. If upstream FFI symbols change, the crate minor or major version will be bumped accordingly.

## What's Included

- Opaque type declarations (`csmMoc`, `csmModel`)
- All public C function bindings (version, moc, model, parameters, parts, drawables, offscreens)
- SDK v5 constants and flag definitions
- Alignment constants (`ALIGN_OF_MOC = 64`, `ALIGN_OF_MODEL = 16`)

All bindings are hand-written (not bindgen-generated) to keep the published content minimal and the licensing boundary clear.

## License

This crate is licensed under the [MIT License](LICENSE).

The Live2D Cubism Core library is proprietary software licensed under the [Live2D Proprietary Software License Agreement](https://www.live2d.com/eula/live2d-proprietary-software-license-agreement_en.html). This crate does not distribute any part of the Cubism Core library.
