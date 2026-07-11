use super::*;

fn triangle(p0: [f32; 3], p1: [f32; 3], p2: [f32; 3]) -> GpuGiTriangle {
    GpuGiTriangle {
        p0: [p0[0], p0[1], p0[2], 0.0],
        p1: [p1[0], p1[1], p1[2], 0.0],
        p2: [p2[0], p2[1], p2[2], 0.0],
        normal_emissive: [0.0, 1.0, 0.0, 0.0],
        albedo: [1.0, 1.0, 1.0, 0.0],
    }
}

#[test]
fn bvh_empty_scene_has_no_nodes() {
    assert!(build_gpu_bvh(&[]).is_empty());
}

#[test]
fn bvh_single_triangle_builds_leaf() {
    let nodes = build_gpu_bvh(&[triangle(
        [-1.0, 0.0, 2.0],
        [2.0, 0.5, 2.0],
        [0.0, 3.0, -1.0],
    )]);

    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].meta, [0, 0, 0, 1]);
    assert_eq!(nodes[0].bounds_min, [-1.0, 0.0, -1.0, 0.0]);
    assert_eq!(nodes[0].bounds_max, [2.0, 3.0, 2.0, 0.0]);
}

#[test]
fn bvh_multiple_triangles_builds_interior_nodes_and_leaves() {
    let triangles = [
        triangle([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        triangle([4.0, 0.0, 0.0], [5.0, 0.0, 0.0], [4.0, 1.0, 0.0]),
        triangle([-3.0, 0.0, 2.0], [-2.0, 0.0, 2.0], [-3.0, 1.0, 3.0]),
    ];
    let nodes = build_gpu_bvh(&triangles);

    assert_eq!(nodes.len(), triangles.len() * 2 - 1);
    assert_eq!(nodes[0].meta[3], 0);
    assert_eq!(nodes[0].bounds_min, [-3.0, 0.0, 0.0, 0.0]);
    assert_eq!(nodes[0].bounds_max, [5.0, 1.0, 3.0, 0.0]);

    let mut leaf_indices = nodes
        .iter()
        .filter(|node| node.meta[3] == 1)
        .map(|node| node.meta[2])
        .collect::<Vec<_>>();
    leaf_indices.sort_unstable();
    assert_eq!(leaf_indices, [0, 1, 2]);
}

#[test]
fn ddgi_atlas_size_uses_requested_probe_resolution() {
    assert_eq!(ddgi_atlas_size([3, 2, 4], 5), (21, 56));
}

#[test]
fn disabled_uniform_disables_both_ddgi_atlases() {
    let uniform = disabled_uniform();

    assert_eq!(uniform.counts_enabled, [1, 1, 1, 0]);
    assert_eq!(uniform.irradiance_atlas_params, [1, 1, 1, 0]);
    assert_eq!(uniform.visibility_atlas_params, [1, 1, 1, 0]);
}

#[test]
fn ddgi_symbols_stay_inside_provider_boundary() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![
        manifest_dir.join("src/render/core/runtime/frame_coordinator.rs"),
        manifest_dir.join("src/render/core/runtime/frame/assemble_frame.rs"),
        manifest_dir.join("src/render/core/runtime/frame/prepare_frame_resources.rs"),
        manifest_dir.join("src/render/core/runtime/frame/execute_frame.rs"),
        manifest_dir.join("src/render/core/runtime/frame/extract_frame.rs"),
        manifest_dir.join("src/render/core/runtime/frame/finish_frame.rs"),
        manifest_dir.join("src/render/core/runtime/frame/collect_frame_inputs.rs"),
        manifest_dir.join("src/render/core/runtime/frame/upload_scene.rs"),
        manifest_dir.join("src/render/core/runtime/executor.rs"),
        manifest_dir.join("src/render/core/pipeline/pipeline_asset.rs"),
        manifest_dir.join("src/render/core/resources/indirect_lighting.rs"),
        manifest_dir.join("src/render/shaders/materials/standard_material.wgsl"),
        manifest_dir.join("src/render/shaders/materials/standard_material_normal_mapped.wgsl"),
    ];

    let shadow_dir = manifest_dir.join("src/render/features/lighting/renderer/shadow");
    for entry in std::fs::read_dir(&shadow_dir).expect("shadow directory should exist") {
        let path = entry
            .expect("shadow directory entry should be readable")
            .path();
        if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }

    for path in files {
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        for symbol in ["Ddgi", "DDGI", "ddgi"] {
            assert!(
                !source.contains(symbol),
                "`{}` must not contain DDGI symbol `{symbol}`; keep DDGI provider-private",
                path.strip_prefix(&manifest_dir).unwrap_or(&path).display()
            );
        }
    }
}
