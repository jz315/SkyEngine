use std::cell::RefCell;
use std::rc::Rc;

use crate::app::FrameContext;
use crate::ecs::World;
use crate::render::Color as RenderColor;
use crate::ui::neo::{
    self, widgets, Color as NeoColor, HorizontalAlign, Screen, Ui, VerticalAlign,
};
use crate::vn::action::VnAction;
use crate::vn::dialogue::VnDialogueState;
use crate::vn::resource::VnResource;
use crate::vn::ui::VnUiMode;

#[derive(Clone, Debug)]
pub struct VnUiPresentationConfig {
    /// Dialogue panel top-left in logical screen pixels.
    pub dialogue_position: [f32; 2],
    pub dialogue_size: [f32; 2],
    /// Choice panel top-left in logical screen pixels.
    pub choice_position: [f32; 2],
    pub choice_size: [f32; 2],
    pub choice_button_height: f32,
    pub panel_color: RenderColor,
    pub choice_panel_color: RenderColor,
    pub text_color: RenderColor,
    pub speaker_color: RenderColor,
    pub muted_text_color: RenderColor,
    pub button_color: RenderColor,
    pub button_hover_color: RenderColor,
    pub button_pressed_color: RenderColor,
    pub speaker_font_size: f32,
    pub line_font_size: f32,
}

impl Default for VnUiPresentationConfig {
    fn default() -> Self {
        Self {
            dialogue_position: [50.0, 516.0],
            dialogue_size: [1180.0, 166.0],
            choice_position: [330.0, 148.0],
            choice_size: [620.0, 260.0],
            choice_button_height: 44.0,
            panel_color: RenderColor::rgba8(12, 18, 28, 226),
            choice_panel_color: RenderColor::rgba8(16, 22, 34, 216),
            text_color: RenderColor::rgba8(244, 248, 252, 255),
            speaker_color: RenderColor::rgba8(135, 210, 255, 255),
            muted_text_color: RenderColor::rgba8(176, 188, 204, 255),
            button_color: RenderColor::rgba8(38, 48, 64, 232),
            button_hover_color: RenderColor::rgba8(58, 78, 104, 244),
            button_pressed_color: RenderColor::rgba8(24, 34, 48, 246),
            speaker_font_size: 22.0,
            line_font_size: 24.0,
        }
    }
}

impl VnUiPresentationConfig {
    pub fn for_surface(surface_size: [f32; 2]) -> Self {
        let width = surface_size[0].max(1.0);
        let height = surface_size[1].max(1.0);
        let mut config = Self::default();
        config.dialogue_size = [width * 0.92, (height * 0.23).clamp(136.0, 210.0)];
        config.dialogue_position = [
            (width - config.dialogue_size[0]) * 0.5,
            height - config.dialogue_size[1] - (height * 0.05).max(24.0),
        ];
        config.choice_size = [width.min(680.0), (height * 0.38).clamp(180.0, 340.0)];
        config.choice_position = [
            (width - config.choice_size[0]) * 0.5,
            (height - config.choice_size[1]) * 0.42,
        ];
        config.choice_button_height = (height * 0.061).clamp(38.0, 54.0);
        config.speaker_font_size = (height * 0.031).clamp(18.0, 26.0);
        config.line_font_size = (height * 0.033).clamp(18.0, 28.0);
        config
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VnUiLayoutPreset {
    pub content_size: [f32; 2],
    pub dialogue_x: f32,
    pub dialogue_bottom: f32,
    pub dialogue_width: f32,
    pub dialogue_height: f32,
    pub choice_y_offset: f32,
    pub choice_width: f32,
    pub choice_height: f32,
    pub choice_button_height: f32,
    pub speaker_font_size: f32,
    pub line_font_size: f32,
}

impl VnUiLayoutPreset {
    pub fn wide_16_9(content_size: [f32; 2]) -> Self {
        Self {
            content_size,
            dialogue_x: 50.0 / 1280.0,
            dialogue_bottom: 38.0 / 720.0,
            dialogue_width: 1180.0 / 1280.0,
            dialogue_height: 166.0 / 720.0,
            choice_y_offset: 82.0 / 720.0,
            choice_width: 620.0 / 1280.0,
            choice_height: 260.0 / 720.0,
            choice_button_height: 44.0 / 720.0,
            speaker_font_size: 22.0 / 720.0,
            line_font_size: 24.0 / 720.0,
        }
    }

    pub fn fit_config(self, surface_size: [f32; 2]) -> VnUiPresentationConfig {
        let layout = letterbox_layout(surface_size, self.content_size);
        VnUiPresentationConfig {
            dialogue_position: [
                layout.x + layout.width * self.dialogue_x,
                layout.bottom()
                    - layout.height * self.dialogue_bottom
                    - layout.height * self.dialogue_height,
            ],
            dialogue_size: [
                layout.width * self.dialogue_width,
                layout.height * self.dialogue_height,
            ],
            choice_position: [
                layout.center()[0] - layout.width * self.choice_width * 0.5,
                layout.center()[1]
                    - layout.height * self.choice_y_offset
                    - layout.height * self.choice_height * 0.5,
            ],
            choice_size: [
                layout.width * self.choice_width,
                layout.height * self.choice_height,
            ],
            choice_button_height: layout.height * self.choice_button_height,
            speaker_font_size: (layout.height * self.speaker_font_size).max(14.0),
            line_font_size: (layout.height * self.line_font_size).max(14.0),
            ..Default::default()
        }
    }

    pub fn initial_window_size(self, max_size: [f32; 2]) -> [u32; 2] {
        let layout = letterbox_layout(max_size, self.content_size);
        [
            layout.width.round().max(1.0) as u32,
            layout.height.round().max(1.0) as u32,
        ]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VnLetterboxLayout {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl VnLetterboxLayout {
    fn center(self) -> [f32; 2] {
        [self.x + self.width * 0.5, self.y + self.height * 0.5]
    }

    fn bottom(self) -> f32 {
        self.y + self.height
    }
}

fn letterbox_layout(surface_size: [f32; 2], content_size: [f32; 2]) -> VnLetterboxLayout {
    let surface_w = surface_size[0].max(1.0);
    let surface_h = surface_size[1].max(1.0);
    let content_w = content_size[0].max(1.0);
    let content_h = content_size[1].max(1.0);
    let scale = (surface_w / content_w).min(surface_h / content_h).max(0.01);
    let width = content_w * scale;
    let height = content_h * scale;
    let x = (surface_w - width).max(0.0) * 0.5;
    let y = (surface_h - height).max(0.0) * 0.5;

    VnLetterboxLayout {
        x,
        y,
        width,
        height,
    }
}

#[derive(Clone, Default)]
pub struct VnUiActionSink {
    actions: Rc<RefCell<Vec<VnAction>>>,
}

impl std::fmt::Debug for VnUiActionSink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VnUiActionSink")
            .field("queued", &self.actions.borrow().len())
            .finish()
    }
}

impl VnUiActionSink {
    pub fn push(&self, action: VnAction) {
        self.actions.borrow_mut().push(action);
    }

    pub fn drain(&self) -> Vec<VnAction> {
        self.actions.borrow_mut().drain(..).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.actions.borrow().is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct VnUiComposeContext {
    mode: VnUiMode,
    action_sink: VnUiActionSink,
}

impl VnUiComposeContext {
    pub fn mode(&self) -> &VnUiMode {
        &self.mode
    }

    pub fn action_sink(&self) -> VnUiActionSink {
        self.action_sink.clone()
    }

    pub fn push_action(&self, action: VnAction) {
        self.action_sink.push(action);
    }
}

#[derive(Clone, Debug)]
struct VnUiSnapshot {
    dialogue: Option<VnDialogueState>,
    mode: VnUiMode,
    config: VnUiPresentationConfig,
}

pub fn compose_vn_ui(ctx: &mut FrameContext<'_>) {
    let _ = compose_vn_ui_with(ctx, |_, _, _| {});
}

pub fn compose_vn_ui_with<R>(
    ctx: &mut FrameContext<'_>,
    extra: impl FnOnce(&mut Ui, Screen, &VnUiComposeContext) -> R,
) -> Option<R> {
    let logical_surface_size = ctx.logical_view_size().to_array();
    let snapshot = snapshot_from_world(ctx.world, logical_surface_size);
    let action_sink = ensure_vn_ui_action_sink(ctx.world);
    let compose_context = VnUiComposeContext {
        mode: snapshot
            .as_ref()
            .map(|snapshot| snapshot.mode.clone())
            .unwrap_or(VnUiMode::Reading),
        action_sink: action_sink.clone(),
    };
    let output = neo::compose(ctx, move |ui, screen| {
        if let Some(snapshot) = snapshot.as_ref() {
            draw_vn_ui(ui, screen, snapshot, &action_sink);
        }
        extra(ui, screen, &compose_context)
    });
    drain_vn_ui_actions_to_resource(ctx.world);
    output
}

pub fn drain_vn_ui_actions_to_resource(world: &mut World) -> Vec<VnAction> {
    let Some(sink) = world.get_resource::<VnUiActionSink>().cloned() else {
        return Vec::new();
    };
    let actions = sink.drain();
    if !actions.is_empty() {
        if let Some(vn) = world.get_resource_mut::<VnResource>() {
            for action in &actions {
                vn.push_action(*action);
            }
        }
    }
    actions
}

fn ensure_vn_ui_action_sink(world: &mut World) -> VnUiActionSink {
    if !world.contains_resource::<VnUiActionSink>() {
        world.insert_resource(VnUiActionSink::default());
    }
    world
        .get_resource::<VnUiActionSink>()
        .expect("VN UI action sink should be installed")
        .clone()
}

fn snapshot_from_world(world: &World, surface_size: [f32; 2]) -> Option<VnUiSnapshot> {
    let vn = world.get_resource::<VnResource>()?;
    let config = vn
        .ui_presentation_config()
        .cloned()
        .unwrap_or_else(|| VnUiPresentationConfig::for_surface(surface_size));
    Some(VnUiSnapshot {
        dialogue: vn.runtime().map(|runtime| runtime.dialogue().clone()),
        mode: vn.ui().mode.clone(),
        config,
    })
}

fn draw_vn_ui(ui: &mut Ui, _screen: Screen, snapshot: &VnUiSnapshot, action_sink: &VnUiActionSink) {
    let visible = matches!(snapshot.mode, VnUiMode::Reading | VnUiMode::Debug);
    if !visible {
        return;
    }
    let Some(dialogue) = snapshot.dialogue.as_ref() else {
        return;
    };

    draw_dialogue(ui, dialogue, &snapshot.config, action_sink);
    draw_choices(ui, dialogue, &snapshot.config, action_sink);
}

fn draw_dialogue(
    ui: &mut Ui,
    dialogue: &VnDialogueState,
    config: &VnUiPresentationConfig,
    action_sink: &VnUiActionSink,
) {
    let Some(line) = dialogue.current_line.as_ref() else {
        return;
    };
    let speaker = line.speaker.clone().unwrap_or_default();
    let text = if dialogue.line_complete {
        line.text.clone()
    } else {
        dialogue.visible_text()
    };
    let [x, y] = config.dialogue_position;
    let [w, h] = config.dialogue_size;

    ui.stack("vn.dialogue")
        .position(x, y)
        .size(w, h)
        .z(100)
        .content(|ui| {
            ui.rect("vn.dialogue.panel")
                .fill()
                .radius(10.0)
                .color(to_neo(config.panel_color))
                .border(1.0, NeoColor::rgba8(170, 206, 238, 88))
                .shadow(22.0, 0.0, 10.0, NeoColor::rgba8(0, 0, 0, 92))
                .build();

            if !speaker.is_empty() {
                ui.text("vn.dialogue.speaker")
                    .position(28.0, 17.0)
                    .size((w - 220.0).max(120.0), 30.0)
                    .text(speaker)
                    .font_size(config.speaker_font_size)
                    .line_height(config.speaker_font_size + 6.0)
                    .color(to_neo(config.speaker_color))
                    .build();
            }

            ui.text("vn.dialogue.line")
                .position(28.0, if line.speaker.is_some() { 56.0 } else { 30.0 })
                .size((w - 158.0).max(180.0), (h - 74.0).max(40.0))
                .text(text)
                .font_size(config.line_font_size)
                .line_height(config.line_font_size + 8.0)
                .wrap(true)
                .color(to_neo(config.text_color))
                .build();

            ui.stack("vn.advance.slot")
                .position((w - 112.0).max(20.0), (h - 50.0).max(20.0))
                .size(86.0, 34.0)
                .content(|ui| {
                    let button_sink = action_sink.clone();
                    widgets::button(ui, "vn.advance")
                        .size(86.0, 34.0)
                        .text("Next")
                        .font_size(15.0)
                        .radius(8.0)
                        .colors(
                            to_neo(config.button_color),
                            to_neo(config.button_hover_color),
                            to_neo(config.button_pressed_color),
                        )
                        .text_color(to_neo(config.text_color))
                        .on_click(move || button_sink.push(VnAction::Advance))
                        .build();
                });
        });
}

fn draw_choices(
    ui: &mut Ui,
    dialogue: &VnDialogueState,
    config: &VnUiPresentationConfig,
    action_sink: &VnUiActionSink,
) {
    if dialogue.choices.is_empty() {
        return;
    }
    let [x, y] = config.choice_position;
    let [w, h] = config.choice_size;
    ui.stack("vn.choices")
        .position(x, y)
        .size(w, h)
        .z(110)
        .content(|ui| {
            ui.rect("vn.choices.panel")
                .fill()
                .radius(12.0)
                .color(to_neo(config.choice_panel_color))
                .border(1.0, NeoColor::rgba8(170, 206, 238, 86))
                .shadow(24.0, 0.0, 12.0, NeoColor::rgba8(0, 0, 0, 96))
                .build();

            let pad = 18.0;
            let gap = 10.0;
            for (index, choice) in dialogue.choices.iter().enumerate() {
                let button_y = pad + index as f32 * (config.choice_button_height + gap);
                if button_y + config.choice_button_height > h - pad {
                    break;
                }
                let id = format!("vn.choice.{index}");
                let selected = index == dialogue.selected_choice;
                let normal = if selected {
                    config.button_hover_color
                } else {
                    config.button_color
                };
                let button_width = (w - pad * 2.0).max(80.0);
                ui.stack(format!("vn.choice.slot.{index}"))
                    .position(pad, button_y)
                    .size(button_width, config.choice_button_height)
                    .content(|ui| {
                        let choice_sink = action_sink.clone();
                        widgets::button(ui, id)
                            .size(button_width, config.choice_button_height)
                            .text(choice.text.clone())
                            .font_size((config.line_font_size - 3.0).max(15.0))
                            .radius(8.0)
                            .colors(
                                to_neo(normal),
                                to_neo(config.button_hover_color),
                                to_neo(config.button_pressed_color),
                            )
                            .text_color(to_neo(config.text_color))
                            .on_click(move || choice_sink.push(VnAction::Choice(index)))
                            .build();
                    });
            }
        });
}

fn to_neo(color: RenderColor) -> NeoColor {
    NeoColor::new(color.r, color.g, color.b, color.a)
}

#[allow(dead_code)]
fn draw_centered_text(
    ui: &mut Ui,
    id: impl Into<String>,
    text: impl Into<String>,
    color: NeoColor,
    font_size: f32,
) {
    ui.text(id)
        .size(1.0, 1.0)
        .text(text)
        .font_size(font_size)
        .line_height(font_size + 4.0)
        .horizontal_align(HorizontalAlign::Center)
        .vertical_align(VerticalAlign::Center)
        .color(color)
        .build();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::Plugin;
    use crate::ui::neo::{FrameInput, PointerEvent, Runtime};
    use crate::vn::dialogue::VnDialogueChoice;
    use crate::vn::script::YarnLine;
    use crate::vn::VnPlugin;

    #[test]
    fn action_sink_drains_actions_into_vn_resource() {
        let mut world = World::new();
        VnPlugin::default()
            .without_systems()
            .install(&mut world)
            .unwrap();
        let sink = ensure_vn_ui_action_sink(&mut world);
        sink.push(VnAction::Auto);
        sink.push(VnAction::Choice(2));

        let drained = drain_vn_ui_actions_to_resource(&mut world);

        assert_eq!(drained, vec![VnAction::Auto, VnAction::Choice(2)]);
        let pending: Vec<_> = world
            .get_resource::<VnResource>()
            .unwrap()
            .actions
            .iter()
            .copied()
            .collect();
        assert_eq!(pending, vec![VnAction::Auto, VnAction::Choice(2)]);
        assert!(world.get_resource::<VnUiActionSink>().unwrap().is_empty());
    }

    #[test]
    fn neo_choice_click_queues_vn_choice_action() {
        let mut dialogue = VnDialogueState::default();
        dialogue.set_choices(vec![
            VnDialogueChoice {
                text: "A".to_owned(),
                source_index: 0,
                condition: None,
            },
            VnDialogueChoice {
                text: "B".to_owned(),
                source_index: 1,
                condition: None,
            },
        ]);
        dialogue.selected_choice = 1;

        let sink = VnUiActionSink::default();
        let snapshot = VnUiSnapshot {
            dialogue: Some(dialogue),
            mode: VnUiMode::Reading,
            config: VnUiPresentationConfig {
                choice_position: [100.0, 100.0],
                choice_size: [320.0, 180.0],
                choice_button_height: 40.0,
                ..Default::default()
            },
        };
        let mut runtime = Runtime::new("vn-test");
        runtime.frame(
            FrameInput::new(Screen::new(640.0, 480.0), 0.0),
            |ui, screen| {
                draw_vn_ui(ui, screen, &snapshot, &sink);
            },
        );

        runtime.dispatch_frame_input(
            FrameInput::new(Screen::new(640.0, 480.0), 0.0).pointer_events([
                PointerEvent::pressed_at(124.0, 170.0),
                PointerEvent::released_at(124.0, 170.0),
            ]),
        );

        assert_eq!(sink.drain(), vec![VnAction::Choice(1)]);
    }

    #[test]
    fn dialogue_hidden_mode_draws_no_roots() {
        let mut dialogue = VnDialogueState::default();
        dialogue.present_line(YarnLine {
            speaker: Some("Alice".to_owned()),
            text: "Hello".to_owned(),
            line_id: None,
            span: Default::default(),
        });
        dialogue.complete_line();
        let snapshot = VnUiSnapshot {
            dialogue: Some(dialogue),
            mode: VnUiMode::Hidden,
            config: VnUiPresentationConfig::default(),
        };
        let sink = VnUiActionSink::default();
        let mut runtime = Runtime::new("vn-test");

        runtime.frame(
            FrameInput::new(Screen::new(1280.0, 720.0), 0.0),
            |ui, screen| {
                draw_vn_ui(ui, screen, &snapshot, &sink);
            },
        );

        assert!(runtime.diagnostics().roots().is_empty());
    }

    #[test]
    fn layout_preset_uses_letterboxed_dialogue_coordinates() {
        let config = VnUiLayoutPreset::wide_16_9([1280.0, 720.0]).fit_config([1600.0, 1000.0]);

        assert!((config.dialogue_position[0] - 62.5).abs() < 0.01);
        assert!((config.dialogue_position[1] - 695.0).abs() < 0.01);
        assert!((config.dialogue_size[0] - 1475.0).abs() < 0.01);
        assert!((config.dialogue_size[1] - 207.5).abs() < 0.01);
    }
}
