use super::super::{
    ButtonSkin, CheckboxSkin, Color, FontRef, HorizontalAlign, ImageRef, PanelSkin, Response, Size,
    SliderSkin, Ui, VerticalAlign,
};
use super::layout::WidgetLayout;
use super::{progress, slider, ProgressStyle, SliderStyle};

const DEFAULT_BUTTON_WIDTH: f32 = 240.0;
const DEFAULT_BUTTON_HEIGHT: f32 = 70.0;
const DEFAULT_PANEL_WIDTH: f32 = 360.0;
const DEFAULT_PANEL_HEIGHT: f32 = 240.0;
const DEFAULT_ROW_WIDTH: f32 = 360.0;
const DEFAULT_ROW_HEIGHT: f32 = 46.0;

#[derive(Debug, Clone)]
pub enum ButtonSkinSource {
    Key(String),
    Skin(ButtonSkin),
}

impl From<&str> for ButtonSkinSource {
    fn from(value: &str) -> Self {
        Self::Key(value.to_string())
    }
}

impl From<String> for ButtonSkinSource {
    fn from(value: String) -> Self {
        Self::Key(value)
    }
}

impl From<ButtonSkin> for ButtonSkinSource {
    fn from(value: ButtonSkin) -> Self {
        Self::Skin(value)
    }
}

impl From<&ButtonSkin> for ButtonSkinSource {
    fn from(value: &ButtonSkin) -> Self {
        Self::Skin(value.clone())
    }
}

pub struct SkinButtonBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    text: String,
    skin: Option<ButtonSkinSource>,
    layout: WidgetLayout,
    has_position: bool,
    x: f32,
    y: f32,
    font_size: f32,
    disabled: bool,
    on_click: Option<Box<dyn FnMut()>>,
}

impl<'ui> SkinButtonBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            text: String::new(),
            skin: None,
            layout: WidgetLayout::new(DEFAULT_BUTTON_WIDTH, DEFAULT_BUTTON_HEIGHT),
            has_position: false,
            x: 0.0,
            y: 0.0,
            font_size: 28.0,
            disabled: false,
            on_click: None,
        }
    }

    pub fn skin(mut self, value: impl Into<ButtonSkinSource>) -> Self {
        self.skin = Some(value.into());
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn width(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.width(value);
        self
    }

    pub fn height(mut self, value: impl Into<Size>) -> Self {
        self.layout = self.layout.height(value);
        self
    }

    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    pub fn grow(mut self, value: f32) -> Self {
        self.layout = self.layout.grow(value);
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn disabled(mut self, value: bool) -> Self {
        self.disabled = value;
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }

    pub fn build(mut self) -> Response {
        let id = self.id.clone();
        let bg_id = format!("{id}.bg");
        let content_id = format!("{id}.content");
        let text_id = format!("{id}.text");
        let response = self.ui.response(&bg_id);
        let skin = match self.skin.take() {
            Some(ButtonSkinSource::Skin(skin)) => skin,
            Some(ButtonSkinSource::Key(key)) => {
                self.ui.button_skin(&key).cloned().unwrap_or_default()
            }
            None => ButtonSkin::default(),
        };
        let image = skin.image_for(response, self.disabled);
        let text = self.text.clone();
        let font = skin.font.clone();
        let content_inset = skin.content_inset;
        let text_color = if self.disabled {
            skin.disabled_text_color
        } else {
            skin.text_color
        };
        let transition = crate::Transition::default()
            .duration(0.08)
            .easing(crate::Ease::OutCubic);
        let mut on_click = self.on_click.take();

        let mut root = self
            .layout
            .apply_to_size(self.ui.stack(id), self.layout.width, self.layout.height)
            .visual_state_from(&bg_id, skin.press_scale);
        if self.has_position {
            root = root.position(self.x, self.y);
        }
        root.content(|ui| {
            if image.is_empty() {
                let bg = ui
                    .rect(bg_id.as_str())
                    .fill()
                    .states(skin.tint, skin.hover_tint, skin.pressed_tint)
                    .disabled(self.disabled)
                    .transition(transition);
                let bg = if let Some(callback) = on_click.take() {
                    bg.on_click(callback)
                } else {
                    bg
                };
                bg.build();
            } else {
                let bg = ui
                    .nine_slice(bg_id.as_str())
                    .fill()
                    .source(image)
                    .slice(skin.slice)
                    .states(skin.tint, skin.hover_tint, skin.pressed_tint)
                    .disabled(self.disabled)
                    .transition(transition);
                let bg = if let Some(callback) = on_click.take() {
                    bg.on_click(callback)
                } else {
                    bg
                };
                bg.build();
            }

            ui.stack(content_id)
                .fill()
                .padding_each(
                    content_inset.left,
                    content_inset.top,
                    content_inset.right,
                    content_inset.bottom,
                )
                .content(|ui| {
                    ui.text(text_id)
                        .fill()
                        .text(text)
                        .font(font)
                        .font_size(self.font_size)
                        .line_height(self.font_size + 4.0)
                        .color(text_color)
                        .horizontal_align(HorizontalAlign::Center)
                        .vertical_align(VerticalAlign::Center)
                        .build();
                });
        });

        self.ui.response(&bg_id)
    }
}

pub struct SkinIconButtonBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    button: Option<ButtonSkinSource>,
    icon: ImageRef,
    layout: WidgetLayout,
    has_position: bool,
    x: f32,
    y: f32,
    icon_size: f32,
    on_click: Option<Box<dyn FnMut()>>,
}

impl<'ui> SkinIconButtonBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            button: None,
            icon: ImageRef::default(),
            layout: WidgetLayout::new(52.0, 52.0),
            has_position: false,
            x: 0.0,
            y: 0.0,
            icon_size: 26.0,
            on_click: None,
        }
    }

    pub fn skin(mut self, value: impl Into<ButtonSkinSource>) -> Self {
        self.button = Some(value.into());
        self
    }

    pub fn icon(mut self, value: impl Into<ImageRef>) -> Self {
        self.icon = value.into();
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn icon_size(mut self, value: f32) -> Self {
        self.icon_size = value.max(1.0);
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }

    pub fn build(mut self) -> Response {
        let id = self.id.clone();
        let icon_id = format!("{id}.icon");
        let icon = self.icon.clone();
        let mut on_click = self.on_click.take();
        let root_width = self.layout.fixed_width_or(52.0);
        let root_height = self.layout.fixed_height_or(52.0);

        let mut root =
            self.layout
                .apply_to_size(self.ui.stack(&id), self.layout.width, self.layout.height);
        if self.has_position {
            root = root.position(self.x, self.y);
        }
        root.content(|ui| {
            let mut button = skin_button(ui, format!("{icon_id}.button"))
                .skin(
                    self.button
                        .take()
                        .unwrap_or_else(|| ButtonSkinSource::Skin(ButtonSkin::default())),
                )
                .text("")
                .size(Size::fill(), Size::fill());
            if let Some(callback) = on_click.take() {
                button = button.on_click(callback);
            }
            button.build();
            let offset_x = (root_width - self.icon_size) * 0.5;
            let offset_y = (root_height - self.icon_size) * 0.5;
            ui.image(icon_id)
                .position(offset_x, offset_y)
                .size(self.icon_size, self.icon_size)
                .source(icon)
                .contain()
                .build();
        });

        self.ui.response(&format!("{id}.icon.button.bg"))
    }
}

#[derive(Debug, Clone)]
pub enum PanelSkinSource {
    Key(String),
    Skin(PanelSkin),
}

impl From<&str> for PanelSkinSource {
    fn from(value: &str) -> Self {
        Self::Key(value.to_string())
    }
}

impl From<String> for PanelSkinSource {
    fn from(value: String) -> Self {
        Self::Key(value)
    }
}

impl From<PanelSkin> for PanelSkinSource {
    fn from(value: PanelSkin) -> Self {
        Self::Skin(value)
    }
}

impl From<&PanelSkin> for PanelSkinSource {
    fn from(value: &PanelSkin) -> Self {
        Self::Skin(value.clone())
    }
}

pub struct SkinPanelBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    skin: Option<PanelSkinSource>,
    tint: Option<Color>,
    layout: WidgetLayout,
    has_position: bool,
    x: f32,
    y: f32,
}

impl<'ui> SkinPanelBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            skin: None,
            tint: None,
            layout: WidgetLayout::new(DEFAULT_PANEL_WIDTH, DEFAULT_PANEL_HEIGHT),
            has_position: false,
            x: 0.0,
            y: 0.0,
        }
    }

    pub fn skin(mut self, value: impl Into<PanelSkinSource>) -> Self {
        self.skin = Some(value.into());
        self
    }

    pub fn tint(mut self, value: Color) -> Self {
        self.tint = Some(value);
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn margin(mut self, value: f32) -> Self {
        self.layout = self.layout.margin(value);
        self
    }

    pub fn grow(mut self, value: f32) -> Self {
        self.layout = self.layout.grow(value);
        self
    }

    pub fn content(self, content: impl FnOnce(&mut Ui)) -> Response {
        let id = self.id.clone();
        let bg_id = format!("{id}.bg");
        let content_id = format!("{id}.content");
        let mut skin = match self.skin {
            Some(PanelSkinSource::Skin(skin)) => skin,
            Some(PanelSkinSource::Key(key)) => {
                self.ui.panel_skin(&key).cloned().unwrap_or_default()
            }
            None => PanelSkin::default(),
        };
        if let Some(tint) = self.tint {
            skin.tint = tint;
        }
        let mut root =
            self.layout
                .apply_to_size(self.ui.stack(id), self.layout.width, self.layout.height);
        if self.has_position {
            root = root.position(self.x, self.y);
        }
        root.content(|ui| {
            if skin.image.is_empty() {
                ui.rect(bg_id.as_str())
                    .fill()
                    .color(skin.tint)
                    .radius(8.0)
                    .build();
            } else {
                ui.nine_slice(bg_id.as_str())
                    .fill()
                    .source(skin.image)
                    .slice(skin.slice)
                    .tint(skin.tint)
                    .build();
            }
            ui.stack(content_id)
                .fill()
                .padding_each(
                    skin.content_inset.left,
                    skin.content_inset.top,
                    skin.content_inset.right,
                    skin.content_inset.bottom,
                )
                .content(content);
        });
        self.ui.response(&bg_id)
    }
}

#[derive(Debug, Clone)]
pub enum CheckboxSkinSource {
    Key(String),
    Skin(CheckboxSkin),
}

impl From<&str> for CheckboxSkinSource {
    fn from(value: &str) -> Self {
        Self::Key(value.to_string())
    }
}

impl From<String> for CheckboxSkinSource {
    fn from(value: String) -> Self {
        Self::Key(value)
    }
}

impl From<CheckboxSkin> for CheckboxSkinSource {
    fn from(value: CheckboxSkin) -> Self {
        Self::Skin(value)
    }
}

impl From<&CheckboxSkin> for CheckboxSkinSource {
    fn from(value: &CheckboxSkin) -> Self {
        Self::Skin(value.clone())
    }
}

pub struct SkinCheckboxBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    skin: Option<CheckboxSkinSource>,
    checked: bool,
    text: String,
    layout: WidgetLayout,
    has_position: bool,
    x: f32,
    y: f32,
    font_size: f32,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl<'ui> SkinCheckboxBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            skin: None,
            checked: false,
            text: String::new(),
            layout: WidgetLayout::new(DEFAULT_ROW_WIDTH, DEFAULT_ROW_HEIGHT),
            has_position: false,
            x: 0.0,
            y: 0.0,
            font_size: 18.0,
            on_change: None,
        }
    }

    pub fn skin(mut self, value: impl Into<CheckboxSkinSource>) -> Self {
        self.skin = Some(value.into());
        self
    }

    pub fn checked(mut self, value: bool) -> Self {
        self.checked = value;
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: impl Into<Size>, height: impl Into<Size>) -> Self {
        self.layout = self.layout.size(width, height);
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    pub fn build(mut self) -> Response {
        let id = self.id.clone();
        let box_id = format!("{id}.box");
        let label_id = format!("{id}.label");
        let text = self.text.clone();
        let checked = self.checked;
        let mut on_change = self.on_change.take();
        let skin = match self.skin.take() {
            Some(CheckboxSkinSource::Skin(skin)) => skin,
            Some(CheckboxSkinSource::Key(key)) => {
                self.ui.checkbox_skin(&key).cloned().unwrap_or_default()
            }
            None => CheckboxSkin::default(),
        };
        let image = if checked {
            skin.checked.clone()
        } else {
            skin.unchecked.clone()
        };
        let box_size = skin.box_size;
        let text_x = box_size + skin.gap;
        let root_width = self.layout.fixed_width_or(DEFAULT_ROW_WIDTH);
        let root_height = self.layout.fixed_height_or(DEFAULT_ROW_HEIGHT);

        let mut root =
            self.layout
                .apply_to_size(self.ui.stack(id), self.layout.width, self.layout.height);
        if self.has_position {
            root = root.position(self.x, self.y);
        }
        root.content(|ui| {
            let image = ui
                .image(box_id.as_str())
                .position(0.0, (root_height - box_size) * 0.5)
                .size(box_size, box_size)
                .source(image)
                .contain()
                .states(Color::WHITE, skin.hover_tint, skin.pressed_tint);
            let image = if let Some(mut callback) = on_change.take() {
                image.on_click(move || callback(!checked))
            } else {
                image
            };
            image.build();
            ui.text(label_id)
                .position(text_x, 0.0)
                .size((root_width - text_x).max(0.0), root_height)
                .text(text)
                .font(skin.font)
                .font_size(self.font_size)
                .color(skin.text_color)
                .vertical_align(VerticalAlign::Center)
                .build();
        });

        self.ui.response(&box_id)
    }
}

#[derive(Debug, Clone)]
pub enum SliderSkinSource {
    Key(String),
    Skin(SliderSkin),
}

impl From<&str> for SliderSkinSource {
    fn from(value: &str) -> Self {
        Self::Key(value.to_string())
    }
}

impl From<String> for SliderSkinSource {
    fn from(value: String) -> Self {
        Self::Key(value)
    }
}

impl From<SliderSkin> for SliderSkinSource {
    fn from(value: SliderSkin) -> Self {
        Self::Skin(value)
    }
}

impl From<&SliderSkin> for SliderSkinSource {
    fn from(value: &SliderSkin) -> Self {
        Self::Skin(value.clone())
    }
}

pub struct SkinSliderBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    skin: Option<SliderSkinSource>,
    value: f32,
    width: f32,
    height: f32,
    has_position: bool,
    x: f32,
    y: f32,
    on_change: Option<Box<dyn FnMut(f32)>>,
}

impl<'ui> SkinSliderBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            skin: None,
            value: 0.0,
            width: 300.0,
            height: 30.0,
            has_position: false,
            x: 0.0,
            y: 0.0,
            on_change: None,
        }
    }

    pub fn skin(mut self, value: impl Into<SliderSkinSource>) -> Self {
        self.skin = Some(value.into());
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width.max(0.0);
        self.height = height.max(0.0);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(f32) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }

    pub fn build(mut self) -> Response {
        let id = self.id.clone();
        let track_id = format!("{id}.track.skin");
        let slider_id = format!("{id}.slider");
        let skin = match self.skin.take() {
            Some(SliderSkinSource::Skin(skin)) => skin,
            Some(SliderSkinSource::Key(key)) => {
                self.ui.slider_skin(&key).cloned().unwrap_or_default()
            }
            None => SliderSkin::default(),
        };
        let mut on_change = self.on_change.take();
        let mut root = self.ui.stack(&id).size(self.width, self.height);
        if self.has_position {
            root = root.position(self.x, self.y);
        }
        root.content(|ui| {
            if !skin.track.is_empty() {
                ui.image(track_id)
                    .position(0.0, (self.height - skin.track_height) * 0.5)
                    .size(self.width, skin.track_height)
                    .source(skin.track)
                    .stretch()
                    .tint(skin.track_tint)
                    .opacity(skin.track_opacity)
                    .build();
            }
            let mut inner = slider(ui, slider_id)
                .size(self.width, self.height)
                .value(self.value)
                .style(SliderStyle {
                    track: skin.rail,
                    fill: skin.fill,
                    knob: skin.knob,
                });
            if let Some(callback) = on_change.take() {
                inner = inner.on_change(callback);
            }
            inner.build();
        });

        self.ui.response(&format!("{id}.slider.hit"))
    }
}

pub struct SkinStatusRibbonBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    panel: Option<PanelSkinSource>,
    text: String,
    width: f32,
    height: f32,
    has_position: bool,
    x: f32,
    y: f32,
    font: FontRef,
    font_size: f32,
    color: Color,
}

impl<'ui> SkinStatusRibbonBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            panel: None,
            text: String::new(),
            width: 280.0,
            height: 34.0,
            has_position: false,
            x: 0.0,
            y: 0.0,
            font: FontRef::DefaultText,
            font_size: 13.0,
            color: Color::BLACK,
        }
    }

    pub fn skin(mut self, value: impl Into<PanelSkinSource>) -> Self {
        self.panel = Some(value.into());
        self
    }

    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = value.into();
        self
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.has_position = true;
        self.x = x;
        self.y = y;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.width = width.max(0.0);
        self.height = height.max(0.0);
        self
    }

    pub fn font(mut self, value: impl Into<FontRef>) -> Self {
        self.font = value.into();
        self
    }

    pub fn font_size(mut self, value: f32) -> Self {
        self.font_size = value.max(1.0);
        self
    }

    pub fn color(mut self, value: Color) -> Self {
        self.color = value;
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let text_id = format!("{id}.text");
        let mut panel = skin_panel(self.ui, id)
            .skin(
                self.panel
                    .unwrap_or_else(|| PanelSkinSource::Skin(PanelSkin::default())),
            )
            .size(self.width, self.height);
        if self.has_position {
            panel = panel.position(self.x, self.y);
        }
        panel.content(|ui| {
            ui.text(text_id)
                .fill()
                .text(self.text)
                .font(self.font)
                .font_size(self.font_size)
                .color(self.color)
                .vertical_align(VerticalAlign::Center)
                .horizontal_align(HorizontalAlign::Center)
                .build();
        })
    }
}

pub struct SkinStatusBarBuilder<'ui> {
    ui: &'ui mut Ui,
    id: String,
    label: String,
    value: f32,
    fill: Color,
    font: FontRef,
    position: [f32; 2],
    width: f32,
}

impl<'ui> SkinStatusBarBuilder<'ui> {
    pub fn new(ui: &'ui mut Ui, id: impl Into<String>) -> Self {
        Self {
            ui,
            id: id.into(),
            label: String::new(),
            value: 0.0,
            fill: Color::GREEN,
            font: FontRef::DefaultText,
            position: [0.0, 0.0],
            width: 278.0,
        }
    }

    pub fn position(mut self, x: f32, y: f32) -> Self {
        self.position = [x, y];
        self
    }

    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = value.into();
        self
    }

    pub fn value(mut self, value: f32) -> Self {
        self.value = value.clamp(0.0, 1.0);
        self
    }

    pub fn fill(mut self, value: Color) -> Self {
        self.fill = value;
        self
    }

    pub fn font(mut self, value: impl Into<FontRef>) -> Self {
        self.font = value.into();
        self
    }

    pub fn width(mut self, value: f32) -> Self {
        self.width = value.max(0.0);
        self
    }

    pub fn build(self) -> Response {
        let id = self.id.clone();
        let x = self.position[0];
        let y = self.position[1];
        self.ui
            .text(format!("{id}.label"))
            .position(x, y - 22.0)
            .size(116.0, 20.0)
            .text(self.label)
            .font(self.font)
            .font_size(14.0)
            .color(Color::rgba8(228, 239, 242, 255))
            .build();
        let response = progress(self.ui, id.as_str())
            .size(self.width, 18.0)
            .value(self.value)
            .style(ProgressStyle {
                track: Color::rgba8(28, 42, 52, 235),
                fill: self.fill,
            })
            .transition_seconds(0.20, crate::Ease::OutCubic)
            .build();
        self.ui
            .text(format!("{id}.value"))
            .position(x + self.width + 8.0, y - 5.0)
            .size(48.0, 24.0)
            .text(format!("{:>3}%", (self.value * 100.0).round() as i32))
            .font_size(13.0)
            .color(Color::rgba8(202, 220, 225, 255))
            .build();
        response
    }
}

pub fn skin_button(ui: &mut Ui, id: impl Into<String>) -> SkinButtonBuilder<'_> {
    SkinButtonBuilder::new(ui, id)
}

pub fn skin_icon_button(ui: &mut Ui, id: impl Into<String>) -> SkinIconButtonBuilder<'_> {
    SkinIconButtonBuilder::new(ui, id)
}

pub fn skin_panel(ui: &mut Ui, id: impl Into<String>) -> SkinPanelBuilder<'_> {
    SkinPanelBuilder::new(ui, id)
}

pub fn skin_checkbox(ui: &mut Ui, id: impl Into<String>) -> SkinCheckboxBuilder<'_> {
    SkinCheckboxBuilder::new(ui, id)
}

pub fn skin_slider(ui: &mut Ui, id: impl Into<String>) -> SkinSliderBuilder<'_> {
    SkinSliderBuilder::new(ui, id)
}

pub fn skin_status_ribbon(ui: &mut Ui, id: impl Into<String>) -> SkinStatusRibbonBuilder<'_> {
    SkinStatusRibbonBuilder::new(ui, id)
}

pub fn skin_status_bar(ui: &mut Ui, id: impl Into<String>) -> SkinStatusBarBuilder<'_> {
    SkinStatusBarBuilder::new(ui, id)
}
