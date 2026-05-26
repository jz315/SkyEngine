# SkyEngine Math

`sky_engine::math` 是引擎公开数学层。内部当前基于 `glam`，但公开 API 由 SkyEngine 拥有，避免业务代码直接绑死到底层数学库。

这层定位是“游戏引擎常用数学 + 稳定 API + 可验证性能”。它覆盖向量、矩阵、四元数、TRS、投影、屏幕坐标、颜色，以及小型几何辅助；它不是完整科学计算库。

导出：

```rust
use sky_engine::math::{Aabb2, Aabb3, Mat4, Projection, Quat, Ray2, Ray3, Transform, Vec2, Vec3, Vec4};
```

## Vec2 / Vec3 / Vec4

构造：

```rust
Vec2::ZERO
Vec2::ONE
Vec2::X
Vec2::Y
Vec2::new(x, y)
Vec2::splat(value)
Vec2::from_array([x, y])

Vec3::X
Vec3::Y
Vec3::Z
Vec3::new(x, y, z)

Vec4::X
Vec4::Y
Vec4::Z
Vec4::W
Vec4::new(x, y, z, w)
```

读取：

```rust
v.to_array()
v.x()
v.y()
v.z() // Vec3 / Vec4
v.w() // Vec4
```

常用运算：

```rust
v.length_squared()
v.length()
v.distance_squared(other)
v.distance(other)
v.dot(other)
v.abs()
v.min(other)
v.max(other)
v.clamp(min, max)
v.lerp(other, t)
v.floor()
v.ceil()
v.round()
v.is_finite()
v.try_normalized()
v.normalized()
v.normalize_or_zero()
```

维度转换：

```rust
Vec2::new(x, y).extend(z)       // -> Vec3
Vec3::new(x, y, z).truncate()   // -> Vec2
Vec4::new(x, y, z, w).truncate() // -> Vec3
```

`Vec2` 额外支持：

```rust
v.perp()
v.perp_dot(other)
```

`Vec3` 额外支持：

```rust
v.cross(other)
```

Index：

```rust
v[0]
v[1]
v[2]
v[3]
```

运算符：

- `+`
- `-`
- component-wise `*`
- scalar `* f32`
- scalar `f32 * vector`
- `/ f32`
- unary `-`
- 对应 assign variants

## Quat

构造：

```rust
Quat::IDENTITY
Quat::from_xyzw(x, y, z, w)
Quat::from_xyzw_array([x, y, z, w])
Quat::from_axis_angle(axis, radians)
Quat::from_rotation_x(radians)
Quat::from_rotation_y(radians)
Quat::from_rotation_z(radians)
Quat::from_euler_angles(pitch, yaw, roll)
```

读取和运算：

```rust
quat.to_xyzw_array()
quat.x()
quat.y()
quat.z()
quat.w()
quat.length_squared()
quat.dot(other)
quat.normalized()
quat.conjugate()
quat.inverse()
quat.rotate_vec3(vector)
quat.slerp(other, t)
quat.to_matrix4()
quat.roll()
quat.is_planar_2d()
```

2D 游戏常用：

```rust
let rotation = Quat::from_rotation_z(angle);
```

## Transform

`Transform` 是共享 TRS 组件，render、physics、scene 等模块都会用它。

字段：

```rust
pub position: Vec3
pub scale: Vec3
pub rotation: Quat
```

构造：

```rust
Transform::IDENTITY
Transform::default()
Transform::from_xy(x, y)
Transform::from_xyz(x, y, z)
Transform::from_position(position)
Transform::from_parts(position, rotation, scale)
Transform::from_matrix4(matrix)
```

Builder：

```rust
Transform::from_xy(0.0, 0.0)
    .with_z(10.0)
    .with_scale(2.0, 2.0)
    .with_scale_uniform(2.0)
    .with_rotation(std::f32::consts::FRAC_PI_2)
```

读取：

```rust
transform.x()
transform.y()
transform.z()
transform.scale_x()
transform.scale_y()
transform.scale_z()
transform.rotation_z()
```

变换：

```rust
transform.transform_vector(local_vector)
transform.transform_point(local_point)
transform.try_inverse_transform_vector(world_vector)
transform.try_inverse_transform_point(world_point)
transform.mul_transform(local_transform)
transform.to_matrix4()
transform.is_planar_2d()
```

方向向量：

```rust
transform.right()
transform.up()
transform.forward() // right-handed, local -Z
```

修改旋转：

```rust
transform.set_rotation_z(radians);
transform.rotate_z(delta_radians);
```

约定：

- 2D 坐标使用 `x/y`，`z` 通常用于排序或深度。
- `from_xy` 默认 `z = 0`、`scale = Vec3::ONE`、`rotation = Quat::IDENTITY`。
- `with_rotation` 是 z 轴旋转，适合 2D。

## Mat4

构造：

```rust
Mat4::ZERO
Mat4::IDENTITY
Mat4::from_cols_array(value)
Mat4::from_quat(quat)
Mat4::from_rotation_x(radians)
Mat4::from_rotation_y(radians)
Mat4::from_rotation_z(radians)
Mat4::from_translation(translation)
Mat4::from_scale(scale)
Mat4::from_rotation_translation(rotation, translation)
Mat4::from_scale_rotation_translation(scale, rotation, translation)
Mat4::orthographic_rh(left, right, bottom, top, near, far)
Mat4::perspective_rh(vertical_fov_radians, aspect, near, far)
Mat4::look_to_rh(origin, direction, up)
Mat4::look_at_rh(eye, target, up)
```

使用：

```rust
matrix.to_cols_array()
matrix.inverse()
matrix.transpose()
matrix.determinant()
matrix.is_finite()
matrix.to_scale_rotation_translation()
matrix.transform_point3(point)
matrix.transform_vector3(vector)
matrix.transform_vec4(vector)
```

GPU 侧通常使用 column-major array。

## Projection

`Projection` 用于 camera/view 计算。

常用方法：

```rust
projection.orthographic_size(viewport_size)
projection.projection_matrix(viewport_size)
projection.near_plane()
projection.far_plane()
projection.view_matrix(transform)
projection.world_to_view(transform, world)
projection.view_depth(transform, world)
projection.screen_to_world_in_viewport(transform, viewport_size, screen)
projection.screen_to_world_logical(transform, viewport_size, screen)
```

Render camera 会基于这层投影计算 view/projection。

## Geometry

小型几何辅助在 `sky_engine::math::geometry`，根模块也直接重导出：

```rust
use sky_engine::math::{Aabb2, Aabb3, Ray2, Ray3};
```

AABB：

```rust
Aabb2::from_min_max(min, max)
Aabb2::from_center_half_size(center, half_size)
aabb.center()
aabb.size()
aabb.half_size()
aabb.contains_point(point)
aabb.intersects(other)
aabb.union(other)
aabb.union_point(point)
aabb.expanded(padding)
aabb.translated(offset)
```

`Aabb3` 提供同名 3D API。

Ray：

```rust
Ray2::new(origin, direction)
Ray2::from_points(origin, target)
ray.at(t)
ray.normalized()
ray.intersect_aabb(aabb)
```

`Ray3` 提供同名 3D API。Ray 的 `direction` 不要求单位长度，`intersect_aabb` 返回的 `t` 与输入 direction 的尺度一致。

## Reflect / Scene 关系

Math 类型有内置 Inspector reflection：

- `Vec2`
- `Vec3`
- `Vec4`
- `Quat`
- `Transform`

Scene 的 `sky.Transform` 保存格式由 scene 模块单独定义，保持 serde-first，不依赖 `reflect-serde`。

## 测试

```bash
cargo test --manifest-path crates/sky_math/Cargo.toml
```

专项性能基线：

```bash
cargo bench --bench sky_math
```

这个 bench 会把 SkyEngine 包装类型和直接 `glam` 调用放在同组里比较，覆盖 `Vec3` normalize/dot、`Quat::rotate_vec3`、`Transform::to_matrix4`、`Mat4::transform_point3` 和 Ray/AABB 相交。
