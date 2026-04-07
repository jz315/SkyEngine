fn main() {
    // ── docs.rs builds have no access to the proprietary SDK ────────────
    if std::env::var_os("DOCS_RS").is_some() {
        return;
    }

    // Re-run triggers for all supported environment variables.
    println!("cargo:rerun-if-env-changed=CUBISM_CORE_LIB_DIR");
    println!("cargo:rerun-if-env-changed=CUBISM_CORE_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=LIVE2D_CUBISM_SDK_NATIVE_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    // ── Resolve library and include directories ─────────────────────────
    //
    // Priority:
    //   1. CUBISM_CORE_LIB_DIR / CUBISM_CORE_INCLUDE_DIR  (direct paths)
    //   2. LIVE2D_CUBISM_SDK_NATIVE_DIR                    (SDK root)
    //   3. ../CubismSdkForNative sibling directory         (local fallback)
    //   4. panic with instructions

    let direct_lib_dir = std::env::var("CUBISM_CORE_LIB_DIR").ok();
    let direct_include_dir = std::env::var("CUBISM_CORE_INCLUDE_DIR").ok();

    if let Some(ref lib_dir) = direct_lib_dir {
        // ── Path 1: direct lib/include dirs ─────────────────────────────
        let lib_path = std::path::PathBuf::from(lib_dir);
        if !lib_path.exists() {
            panic!(
                "CUBISM_CORE_LIB_DIR points to a non-existent directory: {}\n\
                 Please set it to the directory containing the Cubism Core static library.",
                lib_path.display()
            );
        }
        link_from_lib_dir(&lib_path);

        if let Some(ref inc_dir) = direct_include_dir {
            let inc_path = std::path::PathBuf::from(inc_dir);
            println!("cargo:include={}", inc_path.display());
        }
        return;
    }

    // ── Path 2/3: SDK root resolution ───────────────────────────────────
    let sdk_dir = std::env::var("LIVE2D_CUBISM_SDK_NATIVE_DIR")
        .ok()
        .or_else(|| {
            let fallback = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .unwrap()
                .join("CubismSdkForNative");
            if fallback.exists() {
                Some(fallback.to_string_lossy().to_string())
            } else {
                None
            }
        });

    let sdk_dir = sdk_dir.unwrap_or_else(|| {
        panic!(
            "Live2D Cubism SDK not found.\n\n\
             Set one of the following environment variables:\n\n\
             Option A — point to the full SDK root:\n\
               LIVE2D_CUBISM_SDK_NATIVE_DIR=C:\\CubismSdkForNative-5-r.5\n\n\
             Option B — point directly to lib and include directories:\n\
               CUBISM_CORE_LIB_DIR=<path to directory containing the Core static library>\n\
               CUBISM_CORE_INCLUDE_DIR=<path to Core include directory>\n\n\
             Download the SDK from: https://www.live2d.com/en/download/cubism-sdk/"
        );
    });

    let sdk_path = resolve_sdk_root(std::path::Path::new(&sdk_dir));

    // Derive platform-specific library path from SDK layout
    let lib_dir = resolve_sdk_lib_dir(&sdk_path);
    link_from_lib_dir(&lib_dir);

    // Export include path for downstream crates
    let include_dir = sdk_path.join("Core").join("include");
    println!("cargo:include={}", include_dir.display());
}

/// Resolve the effective SDK root.
///
/// Some workspaces vendor the official archive under a stable parent directory
/// such as `CubismSdkForNative/CubismSdkForNative-5-r.5`. Accept both shapes:
/// - `<path>/Core/...`
/// - `<path>/<version>/Core/...`
fn resolve_sdk_root(path: &std::path::Path) -> std::path::PathBuf {
    if path.join("Core").is_dir() {
        return path.to_path_buf();
    }

    let mut nested_candidates = std::fs::read_dir(path)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|child| child.is_dir() && child.join("Core").is_dir());

    match (nested_candidates.next(), nested_candidates.next()) {
        (Some(candidate), None) => candidate,
        _ => path.to_path_buf(),
    }
}

/// Emit linker directives given a directory containing the Core static library.
fn link_from_lib_dir(lib_dir: &std::path::Path) {
    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        // MD = multi-threaded DLL runtime (matches Rust default on MSVC)
        println!("cargo:rustc-link-lib=static=Live2DCubismCore_MD");
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    ))]
    {
        println!("cargo:rustc-link-lib=static=Live2DCubismCore");
    }
}

/// Resolve the platform-specific library directory from the standard SDK layout.
fn resolve_sdk_lib_dir(sdk_path: &std::path::Path) -> std::path::PathBuf {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let base = sdk_path
            .join("Core")
            .join("lib")
            .join("windows")
            .join("x86_64");
        if !base.exists() {
            panic!(
                "Cubism Core library directory not found: {}\n\
                 Expected directory structure: {{SDK}}/Core/lib/windows/x86_64/",
                base.display()
            );
        }
        // SDK v5+ uses MSVC version subdirectories (143, 142, 141).
        ["143", "142", "141"]
            .iter()
            .map(|v| base.join(v))
            .find(|p| p.join("Live2DCubismCore_MD.lib").exists())
            .unwrap_or(base)
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        sdk_path
            .join("Core")
            .join("lib")
            .join("macos")
            .join("x86_64")
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        sdk_path
            .join("Core")
            .join("lib")
            .join("macos")
            .join("arm64")
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        sdk_path
            .join("Core")
            .join("lib")
            .join("linux")
            .join("x86_64")
    }

    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
    )))]
    {
        panic!(
            "Unsupported platform for live2d-cubism-core-sys.\n\
             Supported targets: x86_64-pc-windows-msvc, x86_64-apple-darwin, \
             aarch64-apple-darwin, x86_64-unknown-linux-gnu.\n\
             If you have the Core library for this platform, set CUBISM_CORE_LIB_DIR directly."
        );
    }
}
