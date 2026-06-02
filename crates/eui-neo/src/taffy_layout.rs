use crate::text_measure::TextSystem;
use crate::{Align, Element, ElementKind, LayoutRect, Size};
use taffy::prelude::{
    AlignItems, AvailableSpace, Dimension, Display, FlexDirection, JustifyContent,
    LengthPercentage, LengthPercentageAuto, NodeId, Rect as TaffyRect, Size as TaffySize, Style,
    TaffyTree,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TaffyLayoutError {
    Unsupported(&'static str),
    Taffy(String),
}

impl From<taffy::TaffyError> for TaffyLayoutError {
    fn from(value: taffy::TaffyError) -> Self {
        Self::Taffy(value.to_string())
    }
}

#[derive(Debug, Clone)]
struct TaffyNode {
    id: NodeId,
    children: Vec<TaffyNode>,
}

pub(crate) fn layout_root_with_taffy_text_system(
    root: &mut Element,
    width: f32,
    height: f32,
    _text_system: &mut dyn TextSystem,
) -> Result<(), TaffyLayoutError> {
    layout_one(root, TaffyRootFrame::Screen { width, height })
}

pub(crate) fn layout_element_in_frame_with_taffy_text_system(
    element: &mut Element,
    frame: LayoutRect,
    _text_system: &mut dyn TextSystem,
) -> Result<(), TaffyLayoutError> {
    layout_one(element, TaffyRootFrame::Forced(frame))
}

fn layout_one(element: &mut Element, root_frame: TaffyRootFrame) -> Result<(), TaffyLayoutError> {
    let mut taffy = TaffyTree::new();
    let node = build_taffy_node(&mut taffy, element, true, root_frame)?;
    let available = match root_frame {
        TaffyRootFrame::Screen { width, height } => definite_size(width, height),
        TaffyRootFrame::Forced(frame) => definite_size(frame.width, frame.height),
    };
    taffy.compute_layout(node.id, available)?;
    let [origin_x, origin_y] = root_origin(element, root_frame);
    apply_taffy_layout(&taffy, element, &node, origin_x, origin_y)?;
    Ok(())
}

fn build_taffy_node(
    taffy: &mut TaffyTree,
    element: &Element,
    is_root: bool,
    root_frame: TaffyRootFrame,
) -> Result<TaffyNode, TaffyLayoutError> {
    validate_supported(element, is_root)?;
    let children = element
        .children
        .iter()
        .map(|child| build_taffy_node(taffy, child, false, root_frame))
        .collect::<Result<Vec<_>, _>>()?;
    let child_ids = children.iter().map(|child| child.id).collect::<Vec<_>>();
    let style = taffy_style(element, is_root.then_some(root_frame))?;
    let id = if child_ids.is_empty() {
        taffy.new_leaf(style)?
    } else {
        taffy.new_with_children(style, &child_ids)?
    };
    Ok(TaffyNode { id, children })
}

fn validate_supported(element: &Element, is_root: bool) -> Result<(), TaffyLayoutError> {
    if element.kind == ElementKind::Stack {
        return Err(TaffyLayoutError::Unsupported(
            "stack layout stays on the built-in runtime path",
        ));
    }
    if !is_root && (element.has_x || element.has_y) {
        return Err(TaffyLayoutError::Unsupported(
            "absolute child positioning stays on the built-in runtime path",
        ));
    }
    if element.kind == ElementKind::Text
        && (matches!(element.width, Size::WrapContent)
            || matches!(element.height, Size::WrapContent))
    {
        return Err(TaffyLayoutError::Unsupported(
            "natural text measurement stays on the built-in runtime path",
        ));
    }
    Ok(())
}

fn taffy_style(
    element: &Element,
    root_frame: Option<TaffyRootFrame>,
) -> Result<Style, TaffyLayoutError> {
    let is_leaf = element.children.is_empty();
    let mut style = Style {
        display: Display::Flex,
        flex_direction: match element.kind {
            ElementKind::Column => FlexDirection::Column,
            _ => FlexDirection::Row,
        },
        size: TaffySize {
            width: dimension_for_width(element, is_leaf),
            height: dimension_for_height(element, is_leaf),
        },
        min_size: TaffySize {
            width: min_dimension(element.min_width),
            height: min_dimension(element.min_height),
        },
        max_size: TaffySize {
            width: max_dimension(element.max_layout_width),
            height: max_dimension(element.max_height),
        },
        margin: TaffyRect {
            left: margin_length(element.margin.left),
            right: margin_length(element.margin.right),
            top: margin_length(element.margin.top),
            bottom: margin_length(element.margin.bottom),
        },
        padding: TaffyRect {
            left: padding_length(element.padding.left),
            right: padding_length(element.padding.right),
            top: padding_length(element.padding.top),
            bottom: padding_length(element.padding.bottom),
        },
        gap: TaffySize {
            width: padding_length(if element.kind == ElementKind::Row {
                element.spacing
            } else {
                0.0
            }),
            height: padding_length(if element.kind == ElementKind::Column {
                element.spacing
            } else {
                0.0
            }),
        },
        justify_content: Some(justify_content(element.main_align)),
        align_items: Some(align_items(element.cross_align)),
        align_content: Some(justify_content(element.cross_align)),
        flex_grow: element.grow.max(0.0),
        flex_shrink: 0.0,
        ..Default::default()
    };

    if let Some(TaffyRootFrame::Forced(frame)) = root_frame {
        style.size = TaffySize {
            width: Dimension::length(frame.width),
            height: Dimension::length(frame.height),
        };
    }

    Ok(style)
}

fn dimension_for_width(element: &Element, is_leaf: bool) -> Dimension {
    match element.width {
        Size::Fixed(value) => Dimension::length(value),
        Size::Fill => Dimension::percent(1.0),
        Size::WrapContent if is_leaf && element.kind != ElementKind::Text => {
            Dimension::percent(1.0)
        }
        Size::WrapContent => Dimension::auto(),
    }
}

fn dimension_for_height(element: &Element, is_leaf: bool) -> Dimension {
    match element.height {
        Size::Fixed(value) => Dimension::length(value),
        Size::Fill => Dimension::percent(1.0),
        Size::WrapContent if is_leaf && element.kind != ElementKind::Text => {
            Dimension::percent(1.0)
        }
        Size::WrapContent => Dimension::auto(),
    }
}

fn min_dimension(value: f32) -> Dimension {
    Dimension::length(value.max(0.0))
}

fn max_dimension(value: f32) -> Dimension {
    if value > 0.0 {
        Dimension::length(value)
    } else {
        Dimension::auto()
    }
}

fn margin_length(value: f32) -> LengthPercentageAuto {
    LengthPercentageAuto::length(value.max(0.0))
}

fn padding_length(value: f32) -> LengthPercentage {
    LengthPercentage::length(value.max(0.0))
}

fn justify_content(align: Align) -> JustifyContent {
    match align {
        Align::Start => JustifyContent::Start,
        Align::Center => JustifyContent::Center,
        Align::End => JustifyContent::End,
    }
}

fn align_items(align: Align) -> AlignItems {
    match align {
        Align::Start => AlignItems::Start,
        Align::Center => AlignItems::Center,
        Align::End => AlignItems::End,
    }
}

fn definite_size(width: f32, height: f32) -> TaffySize<AvailableSpace> {
    TaffySize {
        width: AvailableSpace::Definite(width),
        height: AvailableSpace::Definite(height),
    }
}

fn root_origin(element: &Element, root_frame: TaffyRootFrame) -> [f32; 2] {
    match root_frame {
        TaffyRootFrame::Screen { .. } => [
            if element.has_x { element.x } else { 0.0 },
            if element.has_y { element.y } else { 0.0 },
        ],
        TaffyRootFrame::Forced(frame) => [frame.x, frame.y],
    }
}

fn apply_taffy_layout(
    taffy: &TaffyTree,
    element: &mut Element,
    node: &TaffyNode,
    parent_x: f32,
    parent_y: f32,
) -> Result<(), TaffyLayoutError> {
    let layout = taffy.layout(node.id)?;
    let x = parent_x + layout.location.x;
    let y = parent_y + layout.location.y;
    element.frame = LayoutRect::new(x, y, layout.size.width, layout.size.height);
    for (child, child_node) in element.children.iter_mut().zip(&node.children) {
        apply_taffy_layout(taffy, child, child_node, x, y)?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum TaffyRootFrame {
    Screen { width: f32, height: f32 },
    Forced(LayoutRect),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::layout_roots_with_text_system;
    use crate::{DefaultTextSystem, Ui};

    #[test]
    fn taffy_row_matches_builtin_for_gap_margin_padding_and_fill() {
        let mut ui = Ui::new("test");
        ui.row("row")
            .size(300.0, 100.0)
            .gap(10.0)
            .padding_each(8.0, 6.0, 12.0, 4.0)
            .content(|ui| {
                ui.rect("fixed")
                    .size(50.0, 20.0)
                    .margin_each(2.0, 3.0, 4.0, 5.0)
                    .build();
                ui.rect("fill").size(Size::fill(), 40.0).build();
            });
        assert_taffy_matches_builtin(ui.into_roots(), 300.0, 100.0);
    }

    #[test]
    fn taffy_column_matches_builtin_for_grow_min_max_and_padding() {
        let mut ui = Ui::new("test");
        ui.column("panel")
            .size(140.0, 360.0)
            .gap(10.0)
            .padding_each(6.0, 8.0, 10.0, 12.0)
            .content(|ui| {
                ui.rect("fixed").size(80.0, 60.0).build();
                ui.rect("limited")
                    .size(80.0, 40.0)
                    .grow(1.0)
                    .max_height(120.0)
                    .build();
                ui.rect("flex").size(80.0, 40.0).grow(1.0).build();
            });
        assert_taffy_matches_builtin(ui.into_roots(), 140.0, 360.0);
    }

    #[test]
    fn taffy_reports_runtime_specific_stack_as_unsupported() {
        let mut ui = Ui::new("test");
        ui.stack("stack").size(200.0, 100.0).content(|ui| {
            ui.rect("child").size(40.0, 20.0).build();
        });
        let mut roots = ui.into_roots();
        let mut text_system = DefaultTextSystem::default();
        let err = layout_root_with_taffy_text_system(&mut roots[0], 200.0, 100.0, &mut text_system)
            .expect_err("stack layout should stay on the built-in path");
        assert!(matches!(err, TaffyLayoutError::Unsupported(reason) if reason.contains("stack")));
    }

    fn assert_taffy_matches_builtin(mut roots: Vec<Element>, width: f32, height: f32) {
        let mut expected = roots.clone();
        let mut text_system = DefaultTextSystem::default();
        layout_roots_with_text_system(&mut expected, width, height, &mut text_system);
        layout_root_with_taffy_text_system(&mut roots[0], width, height, &mut text_system)
            .expect("taffy layout should support this flex subtree");
        assert_element_frames_close(&roots[0], &expected[0]);
    }

    fn assert_element_frames_close(actual: &Element, expected: &Element) {
        assert_rect_close(actual.frame, expected.frame, &actual.id);
        assert_eq!(actual.children.len(), expected.children.len());
        for (actual_child, expected_child) in actual.children.iter().zip(&expected.children) {
            assert_element_frames_close(actual_child, expected_child);
        }
    }

    fn assert_rect_close(actual: LayoutRect, expected: LayoutRect, id: &str) {
        assert_close(actual.x, expected.x, id, "x");
        assert_close(actual.y, expected.y, id, "y");
        assert_close(actual.width, expected.width, id, "width");
        assert_close(actual.height, expected.height, id, "height");
    }

    fn assert_close(actual: f32, expected: f32, id: &str, field: &str) {
        assert!(
            (actual - expected).abs() <= 0.01,
            "{id}.{field}: expected {expected}, got {actual}"
        );
    }
}
