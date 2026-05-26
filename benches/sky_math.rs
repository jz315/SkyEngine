use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sky_engine::math::{Aabb3, Mat4, Quat, Ray3, Transform, Vec3};
use std::hint::black_box;
use std::time::Duration;

const POINT_COUNT: usize = 4096;

fn configure_group(group: &mut criterion::BenchmarkGroup<'_, criterion::measurement::WallTime>) {
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));
    group.sample_size(10);
}

fn sky_points() -> Vec<Vec3> {
    (0..POINT_COUNT)
        .map(|i| {
            let f = i as f32;
            Vec3::new(f.sin() * 8.0, f.cos() * 4.0, (f * 0.013).sin() * 2.0)
        })
        .collect()
}

fn glam_points() -> Vec<glam::Vec3> {
    sky_points()
        .into_iter()
        .map(|point| glam::Vec3::from_array(point.to_array()))
        .collect()
}

fn bench_vec3_normalize_dot(c: &mut Criterion) {
    let sky = sky_points();
    let glam = glam_points();
    let mut group = c.benchmark_group("sky_math_vec3_normalize_dot");
    configure_group(&mut group);

    group.bench_function(BenchmarkId::new("sky", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = 0.0;
            for point in &sky {
                let n = point.normalize_or_zero();
                acc += n.dot(Vec3::new(0.25, 0.5, 0.75));
            }
            black_box(acc)
        });
    });

    group.bench_function(BenchmarkId::new("glam", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = 0.0;
            let axis = glam::Vec3::new(0.25, 0.5, 0.75);
            for point in &glam {
                let n = point.normalize_or_zero();
                acc += n.dot(axis);
            }
            black_box(acc)
        });
    });

    group.finish();
}

fn bench_quat_rotate(c: &mut Criterion) {
    let sky = sky_points();
    let glam = glam_points();
    let sky_rotation = Quat::from_euler_angles(0.25, 0.5, 0.75);
    let glam_rotation = glam::Quat::from_euler(glam::EulerRot::ZYX, 0.75, 0.5, 0.25).normalize();
    let mut group = c.benchmark_group("sky_math_quat_rotate_vec3");
    configure_group(&mut group);

    group.bench_function(BenchmarkId::new("sky", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = Vec3::ZERO;
            for point in &sky {
                acc += sky_rotation.rotate_vec3(*point);
            }
            black_box(acc)
        });
    });

    group.bench_function(BenchmarkId::new("glam", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = glam::Vec3::ZERO;
            for point in &glam {
                acc += glam_rotation.mul_vec3(*point);
            }
            black_box(acc)
        });
    });

    group.finish();
}

fn bench_transform_to_matrix(c: &mut Criterion) {
    let sky_transform = Transform::from_xyz(3.0, 4.0, 5.0)
        .with_scale3(2.0, 3.0, 4.0)
        .with_euler_angles(0.25, 0.5, 0.75);
    let glam_scale = glam::Vec3::new(2.0, 3.0, 4.0);
    let glam_rotation = glam::Quat::from_euler(glam::EulerRot::ZYX, 0.75, 0.5, 0.25).normalize();
    let glam_translation = glam::Vec3::new(3.0, 4.0, 5.0);
    let mut group = c.benchmark_group("sky_math_transform_to_matrix");
    configure_group(&mut group);

    group.bench_function("sky", |b| {
        b.iter(|| black_box(sky_transform).to_matrix4().to_cols_array());
    });

    group.bench_function("glam", |b| {
        b.iter(|| {
            black_box(glam::Mat4::from_scale_rotation_translation(
                glam_scale,
                glam_rotation,
                glam_translation,
            ))
            .to_cols_array()
        });
    });

    group.finish();
}

fn bench_mat4_transform_points(c: &mut Criterion) {
    let sky = sky_points();
    let glam = glam_points();
    let sky_matrix = Mat4::from_scale_rotation_translation(
        Vec3::new(2.0, 3.0, 4.0),
        Quat::from_euler_angles(0.25, 0.5, 0.75),
        Vec3::new(3.0, 4.0, 5.0),
    );
    let glam_matrix = glam::Mat4::from_cols_array(&sky_matrix.to_cols_array());
    let mut group = c.benchmark_group("sky_math_mat4_transform_points");
    configure_group(&mut group);

    group.bench_function(BenchmarkId::new("sky", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = Vec3::ZERO;
            for point in &sky {
                acc += sky_matrix.transform_point3(*point);
            }
            black_box(acc)
        });
    });

    group.bench_function(BenchmarkId::new("glam", POINT_COUNT), |b| {
        b.iter(|| {
            let mut acc = glam::Vec3::ZERO;
            for point in &glam {
                acc += glam_matrix.transform_point3(*point);
            }
            black_box(acc)
        });
    });

    group.finish();
}

fn bench_ray_aabb(c: &mut Criterion) {
    let rays: Vec<_> = (0..POINT_COUNT)
        .map(|i| {
            let f = i as f32;
            Ray3::new(
                Vec3::new(-32.0, f.sin() * 3.0, f.cos() * 2.0),
                Vec3::new(1.0 + (f * 0.01).sin() * 0.2, 0.05, 0.025),
            )
        })
        .collect();
    let aabb = Aabb3::from_center_half_size(Vec3::ZERO, Vec3::new(8.0, 8.0, 8.0));
    let mut group = c.benchmark_group("sky_math_ray_aabb");
    configure_group(&mut group);

    group.bench_function(BenchmarkId::new("intersect", POINT_COUNT), |b| {
        b.iter(|| {
            let mut hits = 0usize;
            for ray in &rays {
                if ray.intersect_aabb(aabb).is_some() {
                    hits += 1;
                }
            }
            black_box(hits)
        });
    });

    group.finish();
}

criterion_group!(
    sky_math_benches,
    bench_vec3_normalize_dot,
    bench_quat_rotate,
    bench_transform_to_matrix,
    bench_mat4_transform_points,
    bench_ray_aabb
);
criterion_main!(sky_math_benches);
