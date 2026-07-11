use super::*;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuGiTriangle {
    pub(crate) p0: [f32; 4],
    pub(crate) p1: [f32; 4],
    pub(crate) p2: [f32; 4],
    pub(crate) normal_emissive: [f32; 4],
    pub(crate) albedo: [f32; 4],
}

impl GpuGiTriangle {
    pub(crate) fn from_triangle(
        triangle: RayTriangle,
        transform: Mat4,
        material: GiMaterial,
    ) -> Self {
        let p0 = transform.transform_point3(Vec3::from_array(triangle.positions[0]));
        let p1 = transform.transform_point3(Vec3::from_array(triangle.positions[1]));
        let p2 = transform.transform_point3(Vec3::from_array(triangle.positions[2]));
        let normal = (p1 - p0)
            .cross(p2 - p0)
            .try_normalized()
            .unwrap_or(Vec3::new(0.0, 1.0, 0.0));
        Self {
            p0: [p0.x(), p0.y(), p0.z(), 0.0],
            p1: [p1.x(), p1.y(), p1.z(), 0.0],
            p2: [p2.x(), p2.y(), p2.z(), 0.0],
            normal_emissive: [
                normal.x(),
                normal.y(),
                normal.z(),
                luminance(material.emissive),
            ],
            albedo: [
                material.albedo.r,
                material.albedo.g,
                material.albedo.b,
                material.metallic.clamp(0.0, 1.0),
            ],
        }
    }

    fn bounds(&self) -> GiBounds {
        let mut bounds = GiBounds::empty();
        bounds.include_point([self.p0[0], self.p0[1], self.p0[2]]);
        bounds.include_point([self.p1[0], self.p1[1], self.p1[2]]);
        bounds.include_point([self.p2[0], self.p2[1], self.p2[2]]);
        bounds
    }

    fn centroid(&self) -> [f32; 3] {
        [
            (self.p0[0] + self.p1[0] + self.p2[0]) / 3.0,
            (self.p0[1] + self.p1[1] + self.p2[1]) / 3.0,
            (self.p0[2] + self.p1[2] + self.p2[2]) / 3.0,
        ]
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuGiBvhNode {
    pub(crate) bounds_min: [f32; 4],
    pub(crate) bounds_max: [f32; 4],
    pub(crate) meta: [u32; 4],
}

impl GpuGiBvhNode {
    fn leaf(bounds: GiBounds, triangle_index: u32) -> Self {
        Self {
            bounds_min: [bounds.min[0], bounds.min[1], bounds.min[2], 0.0],
            bounds_max: [bounds.max[0], bounds.max[1], bounds.max[2], 0.0],
            meta: [0, 0, triangle_index, 1],
        }
    }

    fn inner(bounds: GiBounds, left: u32, right: u32) -> Self {
        Self {
            bounds_min: [bounds.min[0], bounds.min[1], bounds.min[2], 0.0],
            bounds_max: [bounds.max[0], bounds.max[1], bounds.max[2], 0.0],
            meta: [left, right, 0, 0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct GiBounds {
    min: [f32; 3],
    max: [f32; 3],
}

impl GiBounds {
    fn empty() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }

    fn include_point(&mut self, point: [f32; 3]) {
        for (axis, value) in point.into_iter().enumerate() {
            self.min[axis] = self.min[axis].min(value);
            self.max[axis] = self.max[axis].max(value);
        }
    }

    fn include_bounds(&mut self, bounds: GiBounds) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(bounds.min[axis]);
            self.max[axis] = self.max[axis].max(bounds.max[axis]);
        }
    }

    fn extent(&self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }
}

pub(crate) fn build_gpu_bvh(triangles: &[GpuGiTriangle]) -> Vec<GpuGiBvhNode> {
    if triangles.is_empty() {
        return Vec::new();
    }
    let capped_len = triangles.len().min(u32::MAX as usize);
    let mut indices = (0..capped_len)
        .map(|index| index as u32)
        .collect::<Vec<_>>();
    let mut nodes = Vec::with_capacity(capped_len.saturating_mul(2).saturating_sub(1));
    build_gpu_bvh_node(&mut nodes, triangles, &mut indices);
    nodes
}

pub(crate) fn build_gpu_bvh_node(
    nodes: &mut Vec<GpuGiBvhNode>,
    triangles: &[GpuGiTriangle],
    indices: &mut [u32],
) -> u32 {
    let node_index = nodes.len() as u32;
    nodes.push(GpuGiBvhNode::default());

    let mut bounds = GiBounds::empty();
    let mut centroid_bounds = GiBounds::empty();
    for index in indices.iter().copied() {
        let triangle = &triangles[index as usize];
        bounds.include_bounds(triangle.bounds());
        centroid_bounds.include_point(triangle.centroid());
    }

    if indices.len() == 1 {
        nodes[node_index as usize] = GpuGiBvhNode::leaf(bounds, indices[0]);
        return node_index;
    }

    let extent = centroid_bounds.extent();
    let split_axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
        0
    } else if extent[1] >= extent[2] {
        1
    } else {
        2
    };
    indices.sort_by(|left, right| {
        let left_centroid = triangles[*left as usize].centroid()[split_axis];
        let right_centroid = triangles[*right as usize].centroid()[split_axis];
        left_centroid.total_cmp(&right_centroid)
    });
    let mid = (indices.len() / 2).max(1);
    let (left_indices, right_indices) = indices.split_at_mut(mid);
    let left = build_gpu_bvh_node(nodes, triangles, left_indices);
    let right = build_gpu_bvh_node(nodes, triangles, right_indices);
    nodes[node_index as usize] = GpuGiBvhNode::inner(bounds, left, right);
    node_index
}

#[inline]
pub(crate) fn luminance(color: Color) -> f32 {
    color.r * 0.2126 + color.g * 0.7152 + color.b * 0.0722
}
