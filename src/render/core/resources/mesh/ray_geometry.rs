#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayTriangle {
    pub positions: [[f32; 3]; 3],
}

impl RayTriangle {
    #[inline]
    pub const fn new(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Self {
        Self {
            positions: [a, b, c],
        }
    }

    #[inline]
    fn centroid(self) -> [f32; 3] {
        [
            (self.positions[0][0] + self.positions[1][0] + self.positions[2][0]) / 3.0,
            (self.positions[0][1] + self.positions[1][1] + self.positions[2][1]) / 3.0,
            (self.positions[0][2] + self.positions[1][2] + self.positions[2][2]) / 3.0,
        ]
    }

    #[inline]
    fn bounds(self) -> RayAabb {
        RayAabb::from_points(&self.positions)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayAabb {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl RayAabb {
    pub const EMPTY: Self = Self {
        min: [f32::INFINITY; 3],
        max: [f32::NEG_INFINITY; 3],
    };

    fn from_points(points: &[[f32; 3]]) -> Self {
        let mut bounds = Self::EMPTY;
        for point in points {
            bounds.grow(*point);
        }
        bounds
    }

    fn grow(&mut self, point: [f32; 3]) {
        for ((min, max), value) in self.min.iter_mut().zip(&mut self.max).zip(point) {
            *min = min.min(value);
            *max = max.max(value);
        }
    }

    fn union(self, other: Self) -> Self {
        let mut bounds = self;
        bounds.grow(other.min);
        bounds.grow(other.max);
        bounds
    }

    fn extent(self) -> [f32; 3] {
        [
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        ]
    }

    fn intersects_ray(self, ray: Ray, t_max: f32) -> bool {
        let mut t_min = ray.t_min;
        let mut t_max = t_max;
        for axis in 0..3 {
            let inv_dir = 1.0 / ray.direction[axis];
            let mut t0 = (self.min[axis] - ray.origin[axis]) * inv_dir;
            let mut t1 = (self.max[axis] - ray.origin[axis]) * inv_dir;
            if inv_dir < 0.0 {
                std::mem::swap(&mut t0, &mut t1);
            }
            t_min = t_min.max(t0);
            t_max = t_max.min(t1);
            if t_max < t_min {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    pub direction: [f32; 3],
    pub t_min: f32,
}

impl Ray {
    #[inline]
    pub const fn new(origin: [f32; 3], direction: [f32; 3]) -> Self {
        Self {
            origin,
            direction,
            t_min: 0.0001,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub t: f32,
    pub triangle_index: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RayBlasNode {
    left_first: u32,
    count: u32,
    right_child: u32,
    _pad: u32,
}

impl RayBlasNode {
    #[inline]
    pub fn is_leaf(self) -> bool {
        self.count > 0
    }

    #[inline]
    pub fn left_first(self) -> u32 {
        self.left_first
    }

    #[inline]
    pub fn count(self) -> u32 {
        self.count
    }

    #[inline]
    pub fn right_child(self) -> u32 {
        self.right_child
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RayMesh {
    triangles: Vec<RayTriangle>,
    triangle_indices: Vec<u32>,
    nodes: Vec<RayBlasNode>,
    bounds: Vec<RayAabb>,
}

impl RayMesh {
    const LEAF_SIZE: usize = 4;

    pub fn new(triangles: Vec<RayTriangle>) -> Self {
        let mut mesh = Self {
            triangle_indices: (0..triangles.len() as u32).collect(),
            triangles,
            nodes: Vec::new(),
            bounds: Vec::new(),
        };
        if !mesh.triangles.is_empty() {
            let _ = mesh.build_node(0, mesh.triangles.len());
        }
        mesh
    }

    #[inline]
    pub fn triangles(&self) -> &[RayTriangle] {
        &self.triangles
    }

    #[inline]
    pub fn nodes(&self) -> &[RayBlasNode] {
        &self.nodes
    }

    #[inline]
    pub fn node_bounds(&self) -> &[RayAabb] {
        &self.bounds
    }

    #[inline]
    pub fn triangle_indices(&self) -> &[u32] {
        &self.triangle_indices
    }

    pub fn trace(&self, ray: Ray, t_max: f32) -> Option<RayHit> {
        if self.nodes.is_empty() {
            return None;
        }
        let mut stack = [0u32; 64];
        let mut stack_len = 1usize;
        stack[0] = 0;
        let mut best_t = t_max;
        let mut best_triangle = u32::MAX;

        while stack_len > 0 {
            stack_len -= 1;
            let node_index = stack[stack_len] as usize;
            let bounds = self.bounds[node_index];
            if !bounds.intersects_ray(ray, best_t) {
                continue;
            }
            let node = self.nodes[node_index];
            if node.is_leaf() {
                for offset in 0..node.count {
                    let index = self.triangle_indices[(node.left_first + offset) as usize];
                    let triangle = self.triangles[index as usize];
                    if let Some(t) = intersect_triangle(ray, triangle, best_t) {
                        best_t = t;
                        best_triangle = index;
                    }
                }
            } else {
                if stack_len + 2 <= stack.len() {
                    stack[stack_len] = node.right_child;
                    stack[stack_len + 1] = node.left_first;
                    stack_len += 2;
                }
            }
        }

        (best_triangle != u32::MAX).then_some(RayHit {
            t: best_t,
            triangle_index: best_triangle,
        })
    }

    fn build_node(&mut self, first: usize, count: usize) -> u32 {
        let node_index = self.nodes.len() as u32;
        self.nodes.push(RayBlasNode {
            left_first: first as u32,
            count: count as u32,
            right_child: u32::MAX,
            _pad: 0,
        });
        let bounds = self.bounds_for_range(first, count);
        self.bounds.push(bounds);

        if count <= Self::LEAF_SIZE {
            return node_index;
        }

        let centroid_bounds = self.centroid_bounds_for_range(first, count);
        let extent = centroid_bounds.extent();
        let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
            0
        } else if extent[1] >= extent[2] {
            1
        } else {
            2
        };
        if extent[axis] <= 1e-6 {
            return node_index;
        }

        self.triangle_indices[first..first + count].sort_by(|lhs, rhs| {
            let lhs_c = self.triangles[*lhs as usize].centroid()[axis];
            let rhs_c = self.triangles[*rhs as usize].centroid()[axis];
            lhs_c.total_cmp(&rhs_c)
        });

        let left_count = count / 2;
        let right_count = count - left_count;
        let left = self.build_node(first, left_count);
        let right = self.build_node(first + left_count, right_count);
        self.nodes[node_index as usize] = RayBlasNode {
            left_first: left,
            count: 0,
            right_child: right,
            _pad: 0,
        };
        node_index
    }

    fn bounds_for_range(&self, first: usize, count: usize) -> RayAabb {
        let mut bounds = RayAabb::EMPTY;
        for index in &self.triangle_indices[first..first + count] {
            bounds = bounds.union(self.triangles[*index as usize].bounds());
        }
        bounds
    }

    fn centroid_bounds_for_range(&self, first: usize, count: usize) -> RayAabb {
        let mut bounds = RayAabb::EMPTY;
        for index in &self.triangle_indices[first..first + count] {
            bounds.grow(self.triangles[*index as usize].centroid());
        }
        bounds
    }
}

fn intersect_triangle(ray: Ray, triangle: RayTriangle, t_max: f32) -> Option<f32> {
    let t_min = ray.t_min;
    let ray = crate::math::Ray3::new(
        crate::math::Vec3::from_array(ray.origin),
        crate::math::Vec3::from_array(ray.direction),
    );
    let triangle = crate::math::Triangle3::new(
        crate::math::Vec3::from_array(triangle.positions[0]),
        crate::math::Vec3::from_array(triangle.positions[1]),
        crate::math::Vec3::from_array(triangle.positions[2]),
    );
    ray.intersect_triangle(triangle, t_max)
        .map(|hit| hit.t)
        .filter(|t| *t >= t_min)
}
