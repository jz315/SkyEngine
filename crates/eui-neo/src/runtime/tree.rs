use super::*;

impl Runtime {
    pub fn page_id(&self) -> &str {
        &self.tree.page_id
    }

    pub fn screen(&self) -> Screen {
        self.tree.screen
    }

    pub fn roots(&self) -> &[Element] {
        &self.tree.roots
    }

    pub(crate) fn element_count_hint(&self) -> usize {
        self.tree.structure.len().max(self.tree.roots.len())
    }

    pub fn find(&self, id: &str) -> Option<&Element> {
        let id = self.resolve_id_ref(id);
        self.find_resolved(id.as_ref())
    }

    pub(super) fn resolve_id_ref<'a>(&self, id: &'a str) -> Cow<'a, str> {
        if id.is_empty() || self.tree.page_id.is_empty() {
            return Cow::Borrowed(id);
        }
        if is_resolved_id(id, &self.tree.page_id) {
            Cow::Borrowed(id)
        } else {
            let mut resolved = String::with_capacity(self.tree.page_id.len() + 1 + id.len());
            resolved.push_str(&self.tree.page_id);
            resolved.push('.');
            resolved.push_str(id);
            Cow::Owned(resolved)
        }
    }

    pub(super) fn find_resolved(&self, id: &str) -> Option<&Element> {
        self.tree
            .roots
            .iter()
            .find_map(|root| find_element(root, id))
    }
}

pub(super) fn is_resolved_id(id: &str, page_id: &str) -> bool {
    id.len() > page_id.len()
        && id.as_bytes().get(page_id.len()) == Some(&b'.')
        && id.as_bytes().starts_with(page_id.as_bytes())
}

pub(super) fn sorted_z_indices(elements: &[Element]) -> SmallVec<[usize; 16]> {
    let mut order: SmallVec<[usize; 16]> = (0..elements.len()).collect();
    order.sort_by_key(|&index| (elements[index].z_index, index));
    order
}

pub(super) fn z_order_is_stable(elements: &[Element]) -> bool {
    elements
        .windows(2)
        .all(|pair| pair[0].z_index <= pair[1].z_index)
}

pub(super) fn collect_structure(roots: &[Element], previous_len: usize) -> Vec<ElementSnapshot> {
    let mut snapshots = Vec::with_capacity(previous_len);
    for root in roots {
        collect_element_structure(root, &mut snapshots);
    }
    snapshots
}

pub(super) fn collect_element_structure(element: &Element, snapshots: &mut Vec<ElementSnapshot>) {
    snapshots.push(ElementSnapshot {
        id: element.id.clone(),
        kind: element.kind,
        z_index: element.z_index,
        clip: element.clip,
        clip_radius_bits: element.clip_radius.to_bits(),
        child_count: element.children.len(),
        layout_signature: element_layout_signature(element),
        visual_signature: element_visual_signature(element),
    });
    for child in &element.children {
        collect_element_structure(child, snapshots);
    }
}

pub(super) fn layout_structures_match(
    next: &[ElementSnapshot],
    previous: &[ElementSnapshot],
) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.child_count == previous.child_count
                && next.layout_signature == previous.layout_signature
        })
}

pub(super) fn visual_structures_match(
    next: &[ElementSnapshot],
    previous: &[ElementSnapshot],
) -> bool {
    next.len() == previous.len()
        && next.iter().zip(previous).all(|(next, previous)| {
            next.id == previous.id
                && next.kind == previous.kind
                && next.z_index == previous.z_index
                && next.clip == previous.clip
                && next.clip_radius_bits == previous.clip_radius_bits
                && next.child_count == previous.child_count
                && next.visual_signature == previous.visual_signature
        })
}

pub(super) fn find_element<'a>(element: &'a Element, id: &str) -> Option<&'a Element> {
    if element.id == id {
        return Some(element);
    }
    element
        .children
        .iter()
        .find_map(|child| find_element(child, id))
}

pub(super) fn element_layout_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    hash_rect(element.frame, &mut hasher);
    element.has_x.hash(&mut hasher);
    element.has_y.hash(&mut hasher);
    hash_f32(element.x, &mut hasher);
    hash_f32(element.y, &mut hasher);
    hash_size(element.width, &mut hasher);
    hash_size(element.height, &mut hasher);
    hash_edge_insets(element.margin, &mut hasher);
    hash_edge_insets(element.padding, &mut hasher);
    hash_f32(element.min_width, &mut hasher);
    hash_f32(element.max_layout_width, &mut hasher);
    hash_f32(element.min_height, &mut hasher);
    hash_f32(element.max_height, &mut hasher);
    hash_f32(element.grow, &mut hasher);
    hash_f32(element.spacing, &mut hasher);
    element.main_align.hash(&mut hasher);
    element.cross_align.hash(&mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack | ElementKind::Rect => {}
        ElementKind::Polygon => {}
        ElementKind::Text => {
            if text_measure_affects_layout(element) {
                element.text.hash(&mut hasher);
                element.font.hash(&mut hasher);
                hash_f32(element.font_size, &mut hasher);
                element.font_weight.hash(&mut hasher);
                hash_f32(element.text_max_width, &mut hasher);
                element.wrap.hash(&mut hasher);
                element.horizontal_align.hash(&mut hasher);
                element.vertical_align.hash(&mut hasher);
                hash_f32(element.line_height, &mut hasher);
            }
        }
        ElementKind::Image | ElementKind::NineSlice => {}
    }

    hash_f32(element.timer_seconds, &mut hasher);
    hasher.finish()
}

pub(super) fn element_visual_signature(element: &Element) -> u64 {
    let mut hasher = FxHasher::default();
    element.z_index.hash(&mut hasher);
    element.clip.hash(&mut hasher);
    hash_f32(element.clip_radius, &mut hasher);

    match element.kind {
        ElementKind::Row | ElementKind::Column | ElementKind::Stack => {
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Rect => {
            hash_color(element.color, &mut hasher);
            hash_gradient(element.gradient, &mut hasher);
            hash_border(element.border, &mut hasher);
            hash_shadow(element.shadow, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.blur, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Polygon => {
            hash_color(element.color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
            for point in &element.polygon_points {
                hash_f32(point[0], &mut hasher);
                hash_f32(point[1], &mut hasher);
            }
        }
        ElementKind::Text => {
            element.text.hash(&mut hasher);
            element.font.hash(&mut hasher);
            hash_f32(element.font_size, &mut hasher);
            element.font_weight.hash(&mut hasher);
            hash_f32(element.text_max_width, &mut hasher);
            element.wrap.hash(&mut hasher);
            element.horizontal_align.hash(&mut hasher);
            element.vertical_align.hash(&mut hasher);
            hash_f32(element.line_height, &mut hasher);
            hash_color(element.text_color, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::Image => {
            element.image.hash(&mut hasher);
            element.image_fit.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.radius, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
        ElementKind::NineSlice => {
            element.image.hash(&mut hasher);
            hash_slice(element.slice, &mut hasher);
            hash_edge_insets(element.content_inset, &mut hasher);
            element.center_mode.hash(&mut hasher);
            element.edge_mode.hash(&mut hasher);
            hash_color(element.tint, &mut hasher);
            hash_transform(element.transform, &mut hasher);
            hash_f32(element.opacity, &mut hasher);
        }
    }

    element.interactive.hash(&mut hasher);
    element.focusable.hash(&mut hasher);
    element.disabled.hash(&mut hasher);
    element.cursor.hash(&mut hasher);
    element.has_ime_rect.hash(&mut hasher);
    hash_rect(element.ime_rect, &mut hasher);
    hash_color(element.hover_color, &mut hasher);
    hash_color(element.pressed_color, &mut hasher);
    element.has_state_colors.hash(&mut hasher);
    element.smooth_state_colors.hash(&mut hasher);
    element.visual_state_source_id.hash(&mut hasher);
    element.hover_opacity_source_id.hash(&mut hasher);
    hash_f32(element.pressed_scale, &mut hasher);
    hash_f32(element.hover_hidden_opacity, &mut hasher);
    hash_f32(element.hover_visible_opacity, &mut hasher);
    element.transition.enabled.hash(&mut hasher);
    hash_f32(element.transition.duration_seconds, &mut hasher);
    hash_f32(element.transition.delay_seconds, &mut hasher);
    element.transition.ease.hash(&mut hasher);
    element.transition.properties.hash(&mut hasher);
    hash_motion(element.transition.motion, &mut hasher);
    hash_f32(element.transition.damping_ratio, &mut hasher);
    element.explicit_frame_animation.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn text_measure_affects_layout(element: &Element) -> bool {
    matches!(element.width, crate::Size::WrapContent)
        || matches!(element.height, crate::Size::WrapContent)
}

pub(super) fn hash_rect(rect: LayoutRect, hasher: &mut impl Hasher) {
    hash_f32(rect.x, hasher);
    hash_f32(rect.y, hasher);
    hash_f32(rect.width, hasher);
    hash_f32(rect.height, hasher);
}

pub(super) fn hash_size(size: crate::Size, hasher: &mut impl Hasher) {
    match size {
        crate::Size::Fixed(value) => {
            0_u8.hash(hasher);
            hash_f32(value, hasher);
        }
        crate::Size::WrapContent => 1_u8.hash(hasher),
        crate::Size::Fill => 2_u8.hash(hasher),
    }
}

pub(super) fn hash_edge_insets(insets: crate::EdgeInsets, hasher: &mut impl Hasher) {
    hash_f32(insets.left, hasher);
    hash_f32(insets.top, hasher);
    hash_f32(insets.right, hasher);
    hash_f32(insets.bottom, hasher);
}

pub(super) fn hash_gradient(gradient: crate::Gradient, hasher: &mut impl Hasher) {
    gradient.enabled.hash(hasher);
    hash_color(gradient.start, hasher);
    hash_color(gradient.end, hasher);
    gradient.direction.hash(hasher);
}

pub(super) fn hash_border(border: Border, hasher: &mut impl Hasher) {
    hash_f32(border.width, hasher);
    hash_color(border.color, hasher);
}

pub(super) fn hash_shadow(shadow: Shadow, hasher: &mut impl Hasher) {
    shadow.enabled.hash(hasher);
    hash_f32(shadow.offset[0], hasher);
    hash_f32(shadow.offset[1], hasher);
    hash_f32(shadow.blur, hasher);
    hash_f32(shadow.spread, hasher);
    hash_color(shadow.color, hasher);
}

pub(super) fn hash_transform(transform: Transform, hasher: &mut impl Hasher) {
    hash_f32(transform.translate[0], hasher);
    hash_f32(transform.translate[1], hasher);
    hash_f32(transform.scale[0], hasher);
    hash_f32(transform.scale[1], hasher);
    hash_f32(transform.rotation, hasher);
    hash_f32(transform.origin[0], hasher);
    hash_f32(transform.origin[1], hasher);
}

pub(super) fn hash_slice(slice: crate::Slice, hasher: &mut impl Hasher) {
    hash_f32(slice.left, hasher);
    hash_f32(slice.top, hasher);
    hash_f32(slice.right, hasher);
    hash_f32(slice.bottom, hasher);
}

pub(super) fn hash_motion(motion: Motion, hasher: &mut impl Hasher) {
    match motion {
        Motion::Ease => 0_u8.hash(hasher),
        Motion::Spring => 1_u8.hash(hasher),
    }
}

pub(super) fn hash_color(color: Color, hasher: &mut impl Hasher) {
    hash_f32(color.r, hasher);
    hash_f32(color.g, hasher);
    hash_f32(color.b, hasher);
    hash_f32(color.a, hasher);
}

pub(super) fn hash_f32(value: f32, hasher: &mut impl Hasher) {
    value.to_bits().hash(hasher);
}
