/// Tile cell coordinate in scene grid space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellCoord {
    pub x: i32,
    pub y: i32,
}

impl CellCoord {
    #[inline]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl From<[i32; 2]> for CellCoord {
    #[inline]
    fn from(value: [i32; 2]) -> Self {
        Self::new(value[0], value[1])
    }
}

impl From<(i32, i32)> for CellCoord {
    #[inline]
    fn from(value: (i32, i32)) -> Self {
        Self::new(value.0, value.1)
    }
}

/// Inclusive-exclusive cell rectangle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect {
    pub min: CellCoord,
    pub size: [u32; 2],
}

impl CellRect {
    #[inline]
    pub const fn new(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self {
            min: CellCoord::new(x, y),
            size: [width, height],
        }
    }

    #[inline]
    pub const fn from_min_size(min: CellCoord, size: [u32; 2]) -> Self {
        Self { min, size }
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        self.size[0] == 0 || self.size[1] == 0
    }

    pub fn contains(self, cell: CellCoord) -> bool {
        let max_x = self.min.x.saturating_add(self.size[0] as i32);
        let max_y = self.min.y.saturating_add(self.size[1] as i32);
        cell.x >= self.min.x && cell.y >= self.min.y && cell.x < max_x && cell.y < max_y
    }

    pub fn cells(self) -> impl Iterator<Item = CellCoord> {
        let min = self.min;
        let [width, height] = self.size;
        (0..height).flat_map(move |y| {
            (0..width).map(move |x| CellCoord::new(min.x + x as i32, min.y + y as i32))
        })
    }
}

impl From<([i32; 2], [u32; 2])> for CellRect {
    #[inline]
    fn from(value: ([i32; 2], [u32; 2])) -> Self {
        Self::from_min_size(value.0.into(), value.1)
    }
}

impl From<(CellCoord, [u32; 2])> for CellRect {
    #[inline]
    fn from(value: (CellCoord, [u32; 2])) -> Self {
        Self::from_min_size(value.0, value.1)
    }
}

/// Runtime tile coordinate semantics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridOrientation {
    #[default]
    Orthogonal,
    Isometric,
    Staggered,
    Hexagonal,
}

/// Axis shifted by staggered and hexagonal layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaggerAxis {
    X,
    Y,
}

/// Row or column parity shifted by staggered and hexagonal layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaggerIndex {
    Odd,
    Even,
}

/// Logical scene origin convention.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum GridOrigin {
    #[default]
    TopLeft,
    BottomLeft,
    Center,
}

/// Cardinal-ish object/tile orientation metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TileDirection {
    #[default]
    North,
    East,
    South,
    West,
}

/// Tile scene grid definition. Logical cell size and drawn image size stay
/// separate; image size lives on tile definitions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GridSpec {
    pub orientation: GridOrientation,
    pub cell_size: [u32; 2],
    pub origin: GridOrigin,
    pub render_order: crate::render::TilemapRenderOrder,
    pub stagger_axis: Option<StaggerAxis>,
    pub stagger_index: Option<StaggerIndex>,
    pub hex_side_length: Option<u32>,
}

impl GridSpec {
    pub fn new(orientation: GridOrientation, cell_size: [u32; 2]) -> Self {
        Self {
            orientation,
            cell_size: [cell_size[0].max(1), cell_size[1].max(1)],
            origin: GridOrigin::TopLeft,
            render_order: crate::render::TilemapRenderOrder::RightDown,
            stagger_axis: None,
            stagger_index: None,
            hex_side_length: None,
        }
    }

    #[inline]
    pub fn orthogonal(cell_size: [u32; 2]) -> Self {
        Self::new(GridOrientation::Orthogonal, cell_size)
    }

    #[inline]
    pub fn isometric(cell_size: [u32; 2]) -> Self {
        Self::new(GridOrientation::Isometric, cell_size)
    }
}

impl Default for GridSpec {
    fn default() -> Self {
        Self::orthogonal([1, 1])
    }
}
