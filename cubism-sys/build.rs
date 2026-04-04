fn main() {
    // Locate the Live2D Cubism SDK Native directory.
    // Set the environment variable LIVE2D_CUBISM_SDK_NATIVE_DIR to the SDK root,
    // e.g. C:\CubismSdkForNative-5-r.1
    let sdk_dir = std::env::var("LIVE2D_CUBISM_SDK_NATIVE_DIR").unwrap_or_else(|_| {
        // Fallback: check a sibling directory convention
        let fallback = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("CubismSdkForNative");
        if fallback.exists() {
            return fallback.to_string_lossy().to_string();
        }
        panic!(
            "LIVE2D_CUBISM_SDK_NATIVE_DIR is not set and no fallback SDK found.\n\
             Download the Cubism SDK for Native from:\n\
             https://www.live2d.com/en/download/cubism-sdk/\n\
             Then set LIVE2D_CUBISM_SDK_NATIVE_DIR to the SDK root directory."
        );
    });

    let sdk_path = std::path::PathBuf::from(&sdk_dir);

    // Platform-specific library path
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let base_lib_dir = sdk_path.join("Core").join("lib").join("windows").join("x86_64");
        if !base_lib_dir.exists() {
            panic!(
                "Cubism Core library directory not found: {}\n\
                 Expected directory structure: {{SDK}}/Core/lib/windows/x86_64/",
                base_lib_dir.display()
            );
        }

        // SDK v5+ uses MSVC version subdirectories (143, 142, 141).
        // Find the highest available version.
        let lib_dir = ["143", "142", "141"]
            .iter()
            .map(|v| base_lib_dir.join(v))
            .find(|p| p.join("Live2DCubismCore_MD.lib").exists())
            .unwrap_or_else(|| {
                // Fallback: flat layout (SDK v4 and earlier)
                base_lib_dir.clone()
            });

        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        // MD = multi-threaded DLL runtime (matches Rust default)
        println!("cargo:rustc-link-lib=static=Live2DCubismCore_MD");
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let lib_dir = sdk_path.join("Core").join("lib").join("macos").join("x86_64");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=Live2DCubismCore");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let lib_dir = sdk_path.join("Core").join("lib").join("macos").join("arm64");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=Live2DCubismCore");
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let lib_dir = sdk_path.join("Core").join("lib").join("linux").join("x86_64");
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=static=Live2DCubismCore");
    }

    // Re-run if SDK dir or env var changes
    println!("cargo:rerun-if-env-changed=LIVE2D_CUBISM_SDK_NATIVE_DIR");
    println!("cargo:rerun-if-changed=build.rs");

    // Export the include path for downstream crates
    let include_dir = sdk_path.join("Core").join("include");
    println!("cargo:include={}", include_dir.display());
}
