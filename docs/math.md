# SkyEngine Math

`sky_engine::math` 是引擎公开数学层。内部当前基于 `glam`，但公开 API 由 SkyEngine 拥有，避免业务代码直接绑死到底层数学库。

导出：

```rust
use sky_engine::math::{Mat4, Projection, Quat, Transform, Vec2, Vec3, Vec4};
```

## Vec2 / Vec3 / Vec4

构造：

```rust
Vec2::ZERO
Vec2::ONE
Vec2::new(x, y)
Vec2::splat(value)
Vec2::from_array([x, y])

Vec3::new(x, y, z)
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
v.dot(other)
v.abs()
v.min(other)
v.max(other)
v.clamp(min, max)
v.lerp(other, t)
v.try_normalized()
v.normalized()
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
- `/ f32`
- 对应 assign variants

## Quat

构造：

```rust
Quat::IDENTITY
Quat::from_xyzw(x, y, z, w)
Quat::from_xyzw_array([x, y, z, w])
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
quat.normalized()
quat.conjugate()
quat.rotate_vec3(vector)
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
Transform::default()
Transform::from_xy(x, y)
Transform::from_xyz(x, y, z)
Transform::from_position(position)
Transform::from_parts(position, rotation, scale)
```

Builder：

```rust
Transform::from_xy(0.0, 0.0)
    .with_z(10.0)
    .with_scale(2.0, 2.0)
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
transform.mul_transform(local_transform)
transform.to_matrix4()
transform.is_planar_2d()
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
Mat4::from_cols_array(value)
Mat4::from_quat(quat)
Mat4::from_translation(translation)
Mat4::from_scale(scale)
Mat4::from_scale_rotation_translation(scale, rotation, translation)
Mat4::orthographic_rh(left, right, bottom, top, near, far)
Mat4::perspective_rh(vertical_fov_radians, aspect, near, far)
Mat4::look_to_rh(origin, direction, up)
```

使用：

```rust
matrix.to_cols_array()
matrix.inverse()
matrix.transform_point3(point)
matrix.transform_vector3(vector)
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
projection.screen_to_world(transform, viewport_size, screen)
```

Render camera 会基于这层投影计算 view/projection。

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
cargo test math
```
