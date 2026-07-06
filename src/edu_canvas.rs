//! Coordinate-first drawing layer for teaching and physics visualizations.
//!
//! `EduCanvas` is intentionally a lightweight retained document, not a
//! SkyEngine scene tree.  It lets examples and tools describe 2D diagrams with
//! named rectangles, lines, and arrows, while [`EduCanvasRuntime`] owns the ECS
//! entities that render those elements through the existing sprite pipeline.

use std::collections::{HashMap, HashSet};
use std::f32::consts::PI;

use crate::ecs::{EntityId, World};
use crate::plugin::{Plugin, PluginResult};
use crate::render::{Color, SortingLayer, SpriteRenderer, Transform};

/// Installs the ECS-backed runtime used to render [`EduCanvas`] documents.
#[derive(Default)]
pub struct EduCanvasPlugin;

impl Plugin for EduCanvasPlugin {
    fn name(&self) -> &'static str {
        "EduCanvasPlugin"
    }

    fn install(self, world: &mut World) -> PluginResult {
        if !world.contains_resource::<EduCanvasRuntime>() {
            world.insert_resource(EduCanvasRuntime::default());
        }
        Ok(())
    }
}

/// A retained 2D diagram document addressed by stable element ids.
#[derive(Default)]
pub struct EduCanvas {
    elements: Vec<CanvasElement>,
    index: HashMap<String, usize>,
}

impl EduCanvas {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.index.clear();
    }

    pub fn rect(&mut self, id: impl Into<String>) -> RectBuilder<'_> {
        let element = self.upsert(id.into(), CanvasKind::Rect(RectPrim::default()));
        if !matches!(element.kind, CanvasKind::Rect(_)) {
            element.kind = CanvasKind::Rect(RectPrim::default());
        }
        RectBuilder { element }
    }

    pub fn line(&mut self, id: impl Into<String>) -> LineBuilder<'_> {
        let element = self.upsert(id.into(), CanvasKind::Line(LinePrim::default()));
        if !matches!(element.kind, CanvasKind::Line(_)) {
            element.kind = CanvasKind::Line(LinePrim::default());
        }
        LineBuilder { element }
    }

    pub fn arrow(&mut self, id: impl Into<String>) -> ArrowBuilder<'_> {
        let element = self.upsert(id.into(), CanvasKind::Arrow(ArrowPrim::default()));
        if !matches!(element.kind, CanvasKind::Arrow(_)) {
            element.kind = CanvasKind::Arrow(ArrowPrim::default());
        }
        ArrowBuilder { element }
    }

    pub fn rect_bounds(&self, id: &str) -> Option<Bounds> {
        let element = self.element(id)?;
        match &element.kind {
            CanvasKind::Rect(rect) => Some(rect.bounds()),
            CanvasKind::Line(_) | CanvasKind::Arrow(_) => None,
        }
    }

    pub fn element(&self, id: &str) -> Option<&CanvasElement> {
        self.index
            .get(id)
            .and_then(|index| self.elements.get(*index))
    }

    pub fn elements(&self) -> &[CanvasElement] {
        &self.elements
    }

    pub fn checks(&self) -> CanvasChecks<'_> {
        CanvasChecks {
            canvas: self,
            errors: Vec::new(),
        }
    }

    fn upsert(&mut self, id: String, kind: CanvasKind) -> &mut CanvasElement {
        if let Some(index) = self.index.get(&id).copied() {
            return &mut self.elements[index];
        }
        let index = self.elements.len();
        self.elements.push(CanvasElement {
            id: id.clone(),
            kind,
            color: Color::WHITE,
            layer: 0,
            visible: true,
        });
        self.index.insert(id, index);
        &mut self.elements[index]
    }
}

/// A named canvas element.
pub struct CanvasElement {
    pub id: String,
    pub kind: CanvasKind,
    pub color: Color,
    pub layer: i32,
    pub visible: bool,
}

pub enum CanvasKind {
    Rect(RectPrim),
    Line(LinePrim),
    Arrow(ArrowPrim),
}

#[derive(Clone, Copy, Debug)]
pub struct RectPrim {
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub rotation: f32,
}

impl RectPrim {
    #[inline]
    pub fn bounds(self) -> Bounds {
        Bounds::from_center_size(self.center, self.size)
    }
}

impl Default for RectPrim {
    fn default() -> Self {
        Self {
            center: [0.0, 0.0],
            size: [1.0, 1.0],
            rotation: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LinePrim {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub thickness: f32,
}

impl Default for LinePrim {
    fn default() -> Self {
        Self {
            from: [0.0, 0.0],
            to: [1.0, 0.0],
            thickness: 3.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ArrowPrim {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub thickness: f32,
    pub head_length: f32,
    pub head_angle: f32,
}

impl Default for ArrowPrim {
    fn default() -> Self {
        Self {
            from: [0.0, 0.0],
            to: [72.0, 0.0],
            thickness: 6.0,
            head_length: 24.0,
            head_angle: 0.62,
        }
    }
}

pub struct RectBuilder<'a> {
    element: &'a mut CanvasElement,
}

impl<'a> RectBuilder<'a> {
    pub fn center(mut self, x: f32, y: f32) -> Self {
        self.rect().center = [x, y];
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.rect().size = [width, height];
        self
    }

    pub fn rotation(mut self, radians: f32) -> Self {
        self.rect().rotation = radians;
        self
    }

    pub fn color(self, color: Color) -> Self {
        self.element.color = color;
        self
    }

    pub fn layer(self, layer: i32) -> Self {
        self.element.layer = layer;
        self
    }

    pub fn visible(self, visible: bool) -> Self {
        self.element.visible = visible;
        self
    }

    pub fn bounds(&self) -> Bounds {
        self.rect_ref().bounds()
    }

    fn rect(&mut self) -> &mut RectPrim {
        match &mut self.element.kind {
            CanvasKind::Rect(rect) => rect,
            CanvasKind::Line(_) | CanvasKind::Arrow(_) => unreachable!(),
        }
    }

    fn rect_ref(&self) -> RectPrim {
        match &self.element.kind {
            CanvasKind::Rect(rect) => *rect,
            CanvasKind::Line(_) | CanvasKind::Arrow(_) => unreachable!(),
        }
    }
}

pub struct LineBuilder<'a> {
    element: &'a mut CanvasElement,
}

impl<'a> LineBuilder<'a> {
    pub fn from(mut self, x: f32, y: f32) -> Self {
        self.line().from = [x, y];
        self
    }

    pub fn to(mut self, x: f32, y: f32) -> Self {
        self.line().to = [x, y];
        self
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.line().thickness = thickness;
        self
    }

    pub fn color(self, color: Color) -> Self {
        self.element.color = color;
        self
    }

    pub fn layer(self, layer: i32) -> Self {
        self.element.layer = layer;
        self
    }

    pub fn visible(self, visible: bool) -> Self {
        self.element.visible = visible;
        self
    }

    fn line(&mut self) -> &mut LinePrim {
        match &mut self.element.kind {
            CanvasKind::Line(line) => line,
            CanvasKind::Rect(_) | CanvasKind::Arrow(_) => unreachable!(),
        }
    }
}

pub struct ArrowBuilder<'a> {
    element: &'a mut CanvasElement,
}

impl<'a> ArrowBuilder<'a> {
    pub fn from(mut self, x: f32, y: f32) -> Self {
        self.arrow().from = [x, y];
        self
    }

    pub fn to(mut self, x: f32, y: f32) -> Self {
        self.arrow().to = [x, y];
        self
    }

    pub fn to_offset(self, dx: f32, dy: f32) -> Self {
        let from = self.arrow_ref().from;
        self.to(from[0] + dx, from[1] + dy)
    }

    pub fn thickness(mut self, thickness: f32) -> Self {
        self.arrow().thickness = thickness;
        self
    }

    pub fn head_length(mut self, length: f32) -> Self {
        self.arrow().head_length = length;
        self
    }

    pub fn color(self, color: Color) -> Self {
        self.element.color = color;
        self
    }

    pub fn layer(self, layer: i32) -> Self {
        self.element.layer = layer;
        self
    }

    pub fn visible(self, visible: bool) -> Self {
        self.element.visible = visible;
        self
    }

    fn arrow(&mut self) -> &mut ArrowPrim {
        match &mut self.element.kind {
            CanvasKind::Arrow(arrow) => arrow,
            CanvasKind::Rect(_) | CanvasKind::Line(_) => unreachable!(),
        }
    }

    fn arrow_ref(&self) -> ArrowPrim {
        match &self.element.kind {
            CanvasKind::Arrow(arrow) => *arrow,
            CanvasKind::Rect(_) | CanvasKind::Line(_) => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f32; 2],
    pub max: [f32; 2],
}

impl Bounds {
    pub fn from_center_size(center: [f32; 2], size: [f32; 2]) -> Self {
        let half = [size[0] * 0.5, size[1] * 0.5];
        Self {
            min: [center[0] - half[0], center[1] - half[1]],
            max: [center[0] + half[0], center[1] + half[1]],
        }
    }

    #[inline]
    pub fn left(self) -> f32 {
        self.min[0]
    }

    #[inline]
    pub fn right(self) -> f32 {
        self.max[0]
    }

    #[inline]
    pub fn bottom(self) -> f32 {
        self.min[1]
    }

    #[inline]
    pub fn top(self) -> f32 {
        self.max[1]
    }

    #[inline]
    pub fn center(self) -> [f32; 2] {
        [
            (self.min[0] + self.max[0]) * 0.5,
            (self.min[1] + self.max[1]) * 0.5,
        ]
    }

    #[inline]
    pub fn width(self) -> f32 {
        self.max[0] - self.min[0]
    }

    #[inline]
    pub fn height(self) -> f32 {
        self.max[1] - self.min[1]
    }

    pub fn edge_point(self, edge: Edge, offset: f32) -> [f32; 2] {
        let center = self.center();
        match edge {
            Edge::Top => [center[0] + offset, self.top()],
            Edge::Bottom => [center[0] + offset, self.bottom()],
            Edge::Left => [self.left(), center[1] + offset],
            Edge::Right => [self.right(), center[1] + offset],
        }
    }

    pub fn edge_value(self, edge: Edge) -> f32 {
        match edge {
            Edge::Top => self.top(),
            Edge::Bottom => self.bottom(),
            Edge::Left => self.left(),
            Edge::Right => self.right(),
        }
    }

    pub fn contains_x_range(self, other: Bounds, margin: f32) -> bool {
        other.left() >= self.left() + margin && other.right() <= self.right() - margin
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

pub struct CanvasChecks<'a> {
    canvas: &'a EduCanvas,
    errors: Vec<String>,
}

impl<'a> CanvasChecks<'a> {
    pub fn rect_edges_touch(
        mut self,
        first: &str,
        first_edge: Edge,
        second: &str,
        second_edge: Edge,
        tolerance: f32,
    ) -> Self {
        match (
            self.canvas.rect_bounds(first),
            self.canvas.rect_bounds(second),
        ) {
            (Some(a), Some(b)) => {
                let av = a.edge_value(first_edge);
                let bv = b.edge_value(second_edge);
                if (av - bv).abs() > tolerance {
                    self.errors.push(format!(
                        "{first}.{first_edge:?}={av:.3} does not touch {second}.{second_edge:?}={bv:.3} within {tolerance:.3}"
                    ));
                }
            }
            _ => self
                .errors
                .push(format!("missing rect for touch check: {first} or {second}")),
        }
        self
    }

    pub fn rect_inside_x(mut self, inner: &str, outer: &str, margin: f32) -> Self {
        match (
            self.canvas.rect_bounds(inner),
            self.canvas.rect_bounds(outer),
        ) {
            (Some(a), Some(b)) => {
                if !b.contains_x_range(a, margin) {
                    self.errors.push(format!(
                        "{inner} x-range [{:.3}, {:.3}] is outside {outer} x-range [{:.3}, {:.3}] with margin {:.3}",
                        a.left(),
                        a.right(),
                        b.left(),
                        b.right(),
                        margin,
                    ));
                }
            }
            _ => self.errors.push(format!(
                "missing rect for containment check: {inner} or {outer}"
            )),
        }
        self
    }

    pub fn finish(self) -> CanvasCheckReport {
        CanvasCheckReport {
            errors: self.errors,
        }
    }
}

pub struct CanvasCheckReport {
    errors: Vec<String>,
}

impl CanvasCheckReport {
    #[inline]
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    #[inline]
    pub fn errors(&self) -> &[String] {
        &self.errors
    }
}

/// ECS-backed renderer for a canvas document.
#[derive(Default)]
pub struct EduCanvasRuntime {
    elements: HashMap<String, RuntimeElement>,
}

impl EduCanvasRuntime {
    /// Synchronize a canvas through the runtime resource installed by
    /// [`EduCanvasPlugin`].
    pub fn sync_world(world: &mut World, canvas: &EduCanvas) {
        let mut runtime = world.remove_resource::<Self>().unwrap_or_default();
        runtime.sync(world, canvas);
        world.insert_resource(runtime);
    }

    pub fn sync(&mut self, world: &mut World, canvas: &EduCanvas) {
        let live_ids: HashSet<&str> = canvas
            .elements()
            .iter()
            .map(|element| element.id.as_str())
            .collect();
        let stale: Vec<String> = self
            .elements
            .keys()
            .filter(|id| !live_ids.contains(id.as_str()))
            .cloned()
            .collect();
        for id in stale {
            if let Some(runtime) = self.elements.remove(&id) {
                runtime.despawn(world);
            }
        }

        for element in canvas.elements() {
            let expected = RuntimeKind::from_canvas(&element.kind);
            let recreate = self
                .elements
                .get(&element.id)
                .map(|runtime| runtime.kind != expected)
                .unwrap_or(true);

            if recreate {
                if let Some(runtime) = self.elements.remove(&element.id) {
                    runtime.despawn(world);
                }
                self.elements
                    .insert(element.id.clone(), RuntimeElement::spawn(world, expected));
            }

            if let Some(runtime) = self.elements.get_mut(&element.id) {
                runtime.apply(world, element);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RuntimeKind {
    Rect,
    Line,
    Arrow,
}

impl RuntimeKind {
    fn from_canvas(kind: &CanvasKind) -> Self {
        match kind {
            CanvasKind::Rect(_) => Self::Rect,
            CanvasKind::Line(_) => Self::Line,
            CanvasKind::Arrow(_) => Self::Arrow,
        }
    }
}

struct RuntimeElement {
    kind: RuntimeKind,
    entities: Vec<EntityId>,
}

impl RuntimeElement {
    fn spawn(world: &mut World, kind: RuntimeKind) -> Self {
        let count = match kind {
            RuntimeKind::Rect | RuntimeKind::Line => 1,
            RuntimeKind::Arrow => 3,
        };
        let mut entities = Vec::with_capacity(count);
        for _ in 0..count {
            entities.push(spawn_sprite(world));
        }
        Self { kind, entities }
    }

    fn despawn(self, world: &mut World) {
        for entity in self.entities {
            let _ = world.despawn(entity);
        }
    }

    fn apply(&mut self, world: &mut World, element: &CanvasElement) {
        match &element.kind {
            CanvasKind::Rect(rect) => {
                apply_rect_entity(world, self.entities[0], *rect, element);
            }
            CanvasKind::Line(line) => {
                apply_line_entity(
                    world,
                    self.entities[0],
                    line.from,
                    line.to,
                    line.thickness,
                    element.color,
                    element.layer,
                    element.visible,
                );
            }
            CanvasKind::Arrow(arrow) => {
                apply_arrow_entities(world, &self.entities, *arrow, element);
            }
        }
    }
}

fn spawn_sprite(world: &mut World) -> EntityId {
    world.spawn((
        Transform::default(),
        SpriteRenderer::new(1.0, 1.0),
        SortingLayer(0),
    ))
}

fn apply_rect_entity(world: &mut World, entity: EntityId, rect: RectPrim, element: &CanvasElement) {
    if let Some(transform) = world.get_mut::<Transform>(entity) {
        transform.position[0] = rect.center[0];
        transform.position[1] = rect.center[1];
        transform.set_rotation_z(rect.rotation);
    }
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
        sprite.width = rect.size[0];
        sprite.height = rect.size[1];
        sprite.color = element.color;
        sprite.visible = element.visible;
    }
    if let Some(layer) = world.get_mut::<SortingLayer>(entity) {
        layer.0 = element.layer;
    }
}

fn apply_line_entity(
    world: &mut World,
    entity: EntityId,
    from: [f32; 2],
    to: [f32; 2],
    thickness: f32,
    color: Color,
    layer: i32,
    visible: bool,
) {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let length = (dx * dx + dy * dy).sqrt();
    let center = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];
    let rotation = dy.atan2(dx);

    if let Some(transform) = world.get_mut::<Transform>(entity) {
        transform.position[0] = center[0];
        transform.position[1] = center[1];
        transform.set_rotation_z(rotation);
    }
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
        sprite.width = length.max(0.0);
        sprite.height = thickness.max(0.0);
        sprite.color = color;
        sprite.visible = visible && length > 0.5 && thickness > 0.0;
    }
    if let Some(sort) = world.get_mut::<SortingLayer>(entity) {
        sort.0 = layer;
    }
}

fn apply_arrow_entities(
    world: &mut World,
    entities: &[EntityId],
    arrow: ArrowPrim,
    element: &CanvasElement,
) {
    let dx = arrow.to[0] - arrow.from[0];
    let dy = arrow.to[1] - arrow.from[1];
    let length = (dx * dx + dy * dy).sqrt();
    let visible = element.visible && length > arrow.head_length.max(1.0);
    if !visible {
        for entity in entities {
            set_entity_visible(world, *entity, false);
        }
        return;
    }

    let ux = dx / length;
    let uy = dy / length;
    let shaft_end = [
        arrow.to[0] - ux * arrow.head_length * 0.55,
        arrow.to[1] - uy * arrow.head_length * 0.55,
    ];
    apply_line_entity(
        world,
        entities[0],
        arrow.from,
        shaft_end,
        arrow.thickness,
        element.color,
        element.layer,
        true,
    );

    let angle = dy.atan2(dx);
    let left = angle + PI - arrow.head_angle;
    let right = angle + PI + arrow.head_angle;
    let left_end = [
        arrow.to[0] + left.cos() * arrow.head_length,
        arrow.to[1] + left.sin() * arrow.head_length,
    ];
    let right_end = [
        arrow.to[0] + right.cos() * arrow.head_length,
        arrow.to[1] + right.sin() * arrow.head_length,
    ];
    apply_line_entity(
        world,
        entities[1],
        arrow.to,
        left_end,
        arrow.thickness,
        element.color,
        element.layer,
        true,
    );
    apply_line_entity(
        world,
        entities[2],
        arrow.to,
        right_end,
        arrow.thickness,
        element.color,
        element.layer,
        true,
    );
}

fn set_entity_visible(world: &mut World, entity: EntityId, visible: bool) {
    if let Some(sprite) = world.get_mut::<SpriteRenderer>(entity) {
        sprite.visible = visible;
    }
}
