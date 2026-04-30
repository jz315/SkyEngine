use crate::ecs::{EntityId, World};
use crate::render::Color;
use crate::ui::{
    UiAlign, UiAnchor, UiButton, UiEventKind, UiEvents, UiId, UiImage, UiLayout, UiLength, UiNode,
    UiPanel, UiRect, UiState, UiText,
};
use crate::vn::action::VnAction;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::vn::components::{VnActorSprite, VnBackground, VnChoiceUi, VnDialogueUi, VnSceneLayer};
use crate::vn::dialogue::VnDialogueState;
use crate::vn::presentation::VnSpriteTextureMap;
use crate::vn::runtime::{VnRuntime, VnRuntimeEvent, VnRuntimeResult, VnStatus};
use crate::vn::scene::{VnActor, VnImageLayer, VnSceneState};
use crate::vn::ui::{VnUiMode, VnUiState};

static VN_UI_DEBUG_FRAMES: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VnUiImageFit {
    #[default]
    AutoFill,
    Contain,
    FillWidth,
    FillHeight,
}

#[derive(Clone, Debug)]
pub struct VnUiPresentationConfig {
    /// Scene rectangle in logical screen pixels, with the origin at the
    /// surface top-left and y increasing downward.
    pub scene_position: [f32; 2],
    pub scene_size: [f32; 2],
    pub background_fit: VnUiImageFit,
    pub cg_fit: VnUiImageFit,
    pub actor_height: f32,
    pub actor_scale: f32,
    pub actor_positions: BTreeMap<String, [f32; 2]>,
    /// Dialogue panel top-left in logical screen pixels.
    pub dialogue_position: [f32; 2],
    pub dialogue_size: [f32; 2],
    /// Choice panel top-left in logical screen pixels.
    pub choice_position: [f32; 2],
    pub choice_size: [f32; 2],
    pub choice_button_height: f32,
    pub panel_color: Color,
    pub choice_panel_color: Color,
    pub text_color: Color,
    pub speaker_color: Color,
    pub muted_text_color: Color,
    pub button_color: Color,
    pub button_hover_color: Color,
    pub button_pressed_color: Color,
    pub speaker_font_size: f32,
    pub line_font_size: f32,
}

impl Default for VnUiPresentationConfig {
    fn default() -> Self {
        let mut actor_positions = BTreeMap::new();
        actor_positions.insert("far_left".to_owned(), [0.18, 1.0]);
        actor_positions.insert("left".to_owned(), [0.32, 1.0]);
        actor_positions.insert("center".to_owned(), [0.5, 1.0]);
        actor_positions.insert("right".to_owned(), [0.68, 1.0]);
        actor_positions.insert("far_right".to_owned(), [0.82, 1.0]);
        Self {
            scene_position: [0.0, 0.0],
            scene_size: [1280.0, 720.0],
            background_fit: VnUiImageFit::AutoFill,
            cg_fit: VnUiImageFit::AutoFill,
            actor_height: 0.94,
            actor_scale: 1.0,
            actor_positions,
            dialogue_position: [50.0, 516.0],
            dialogue_size: [1180.0, 166.0],
            choice_position: [330.0, 148.0],
            choice_size: [620.0, 260.0],
            choice_button_height: 44.0,
            panel_color: Color::rgba8(12, 18, 28, 226),
            choice_panel_color: Color::rgba8(16, 22, 34, 216),
            text_color: Color::rgba8(244, 248, 252, 255),
            speaker_color: Color::rgba8(135, 210, 255, 255),
            muted_text_color: Color::rgba8(176, 188, 204, 255),
            button_color: Color::rgba8(38, 48, 64, 232),
            button_hover_color: Color::rgba8(58, 78, 104, 244),
            button_pressed_color: Color::rgba8(24, 34, 48, 246),
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
        config.scene_position = [0.0, 0.0];
        config.scene_size = [width, height];
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
            scene_position: [layout.x, layout.y],
            scene_size: [layout.width, layout.height],
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VnUiEntities {
    pub background_image: Option<EntityId>,
    pub cg_image: Option<EntityId>,
    pub actors: BTreeMap<String, EntityId>,
    pub dialogue_panel: Option<EntityId>,
    pub speaker_text: Option<EntityId>,
    pub line_text: Option<EntityId>,
    pub advance_button: Option<EntityId>,
    pub choice_panel: Option<EntityId>,
    pub choice_buttons: Vec<EntityId>,
}

pub fn sync_runtime_ui_to_world(world: &mut World) {
    let surface_size = world
        .get_resource::<UiState>()
        .map(UiState::surface_size)
        .filter(|size| size[0] > 0.0 && size[1] > 0.0)
        .unwrap_or_else(|| VnUiPresentationConfig::default().scene_size);
    sync_runtime_ui_to_world_with_surface(world, surface_size);
}

pub fn sync_runtime_ui_to_world_with_surface(world: &mut World, surface_size: [f32; 2]) {
    let Some((dialogue, scene)) = world
        .get_resource::<VnRuntime>()
        .map(|runtime| (runtime.dialogue().clone(), runtime.scene().clone()))
    else {
        return;
    };
    let mode = world
        .get_resource::<VnUiState>()
        .map(|ui| ui.mode.clone())
        .unwrap_or(VnUiMode::Reading);
    let config = world
        .get_resource::<VnUiPresentationConfig>()
        .cloned()
        .unwrap_or_else(|| VnUiPresentationConfig::for_surface(surface_size));
    let textures = world
        .get_resource::<VnSpriteTextureMap>()
        .cloned()
        .unwrap_or_default();
    let mut entities = world.remove_resource::<VnUiEntities>().unwrap_or_default();
    let debug = begin_vn_ui_debug(surface_size, &config, &scene);

    sync_scene_ui_to_world_debug(world, &mut entities, &scene, &config, &textures, debug);
    sync_dialogue_ui_to_world_debug(world, &mut entities, &dialogue, &mode, &config, debug);
    world.insert_resource(entities);
}

pub fn sync_scene_ui_to_world(
    world: &mut World,
    entities: &mut VnUiEntities,
    scene: &VnSceneState,
    config: &VnUiPresentationConfig,
    textures: &VnSpriteTextureMap,
) {
    sync_scene_ui_to_world_debug(world, entities, scene, config, textures, None);
}

fn sync_scene_ui_to_world_debug(
    world: &mut World,
    entities: &mut VnUiEntities,
    scene: &VnSceneState,
    config: &VnUiPresentationConfig,
    textures: &VnSpriteTextureMap,
    debug: Option<VnUiDebugFrame>,
) {
    let scene_bounds = (config.scene_position, config.scene_size);
    sync_image_ui_layer(
        world,
        &mut entities.background_image,
        scene.background.as_ref(),
        config.background_fit,
        scene_bounds,
        textures,
        "background",
        debug,
    );
    sync_image_ui_layer(
        world,
        &mut entities.cg_image,
        scene.cg.as_ref(),
        config.cg_fit,
        scene_bounds,
        textures,
        "cg",
        debug,
    );
    sync_actor_ui_layers(world, &mut entities.actors, scene, config, textures, debug);
}

fn sync_image_ui_layer(
    world: &mut World,
    slot: &mut Option<EntityId>,
    layer: Option<&VnImageLayer>,
    fit: VnUiImageFit,
    scene_bounds: ([f32; 2], [f32; 2]),
    textures: &VnSpriteTextureMap,
    layer_name: &str,
    debug: Option<VnUiDebugFrame>,
) {
    let Some(layer) = layer else {
        despawn_slot(world, slot);
        return;
    };
    let Some(texture) = textures.get(&layer.asset) else {
        despawn_slot(world, slot);
        return;
    };
    let (rect, uv_rect) = fitted_image_rect(
        scene_bounds.0,
        scene_bounds.1,
        textures.size(&layer.asset),
        fit,
    );
    debug_log_image_layer(
        debug,
        layer_name,
        &layer.asset,
        fit,
        scene_bounds,
        textures.size(&layer.asset),
        rect,
        uv_rect,
    );

    let node = UiNode::panel(rect[2], rect[3])
        .anchor(UiAnchor::TopLeft)
        .at(rect[0], rect[1])
        .z(layer.layer);
    let image = UiImage::new(texture).uv(uv_rect[0], uv_rect[1], uv_rect[2], uv_rect[3]);
    let entity = match live_entity(world, *slot) {
        Some(entity) => {
            world.insert(entity, node);
            world.insert(entity, image);
            entity
        }
        None => {
            let entity = world.spawn((node, image));
            *slot = Some(entity);
            entity
        }
    };
    world.insert(
        entity,
        VnBackground {
            asset: layer.asset.clone(),
            layer: layer.layer,
        },
    );
    world.insert(
        entity,
        VnSceneLayer {
            name: layer_name.to_owned(),
            order: layer.layer,
        },
    );
}

fn sync_actor_ui_layers(
    world: &mut World,
    entities: &mut BTreeMap<String, EntityId>,
    scene: &VnSceneState,
    config: &VnUiPresentationConfig,
    textures: &VnSpriteTextureMap,
    debug: Option<VnUiDebugFrame>,
) {
    let stale: Vec<_> = entities
        .keys()
        .filter(|id| {
            scene
                .actors
                .get(*id)
                .is_none_or(|actor| !actor.visible || actor_asset(actor).is_none())
        })
        .cloned()
        .collect();
    for id in stale {
        if let Some(entity) = entities.remove(&id) {
            world.despawn(entity);
        }
    }

    for actor in scene.actors.values().filter(|actor| actor.visible) {
        sync_actor_ui_layer(world, entities, actor, config, textures, debug);
    }
}

fn sync_actor_ui_layer(
    world: &mut World,
    entities: &mut BTreeMap<String, EntityId>,
    actor: &VnActor,
    config: &VnUiPresentationConfig,
    textures: &VnSpriteTextureMap,
    debug: Option<VnUiDebugFrame>,
) {
    let Some(asset) = actor_asset(actor) else {
        return;
    };
    let Some(texture) = textures.get(asset) else {
        if let Some(entity) = entities.remove(&actor.id) {
            world.despawn(entity);
        }
        return;
    };

    let [width, height] = actor_size(config, textures.visible_size(asset));
    let [anchor_x, anchor_y] = actor_anchor(actor, config);
    let x = config.scene_position[0] + config.scene_size[0] * anchor_x - width * 0.5;
    let y = config.scene_position[1] + config.scene_size[1] * anchor_y - height;
    debug_log_actor(
        debug,
        actor,
        asset,
        textures.size(asset),
        [anchor_x, anchor_y],
        [x, y, width, height],
        config.scene_size,
    );
    let node = UiNode::panel(width, height)
        .anchor(UiAnchor::TopLeft)
        .at(x, y)
        .z(actor.layer);
    let uv_rect = textures.visible_uv_rect(asset).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let image = UiImage::new(texture)
        .uv(uv_rect[0], uv_rect[1], uv_rect[2], uv_rect[3])
        .color(Color::new(1.0, 1.0, 1.0, actor.opacity));
    let entity = match entities
        .get(&actor.id)
        .copied()
        .filter(|entity| world.contains(*entity))
    {
        Some(entity) => {
            world.insert(entity, node);
            world.insert(entity, image);
            entity
        }
        None => {
            let entity = world.spawn((node, image));
            entities.insert(actor.id.clone(), entity);
            entity
        }
    };

    world.insert(
        entity,
        VnActorSprite {
            actor_id: actor.id.clone(),
            asset: actor.asset.clone(),
            expression: actor.expression.clone(),
            layer: actor.layer,
            z: actor.z,
            opacity: actor.opacity,
        },
    );
    world.insert(
        entity,
        VnSceneLayer {
            name: actor.id.clone(),
            order: actor.layer,
        },
    );
}

fn actor_asset(actor: &VnActor) -> Option<&str> {
    actor
        .asset
        .as_deref()
        .or(actor.expression.as_deref())
        .or(Some(actor.id.as_str()))
}

fn actor_anchor(actor: &VnActor, config: &VnUiPresentationConfig) -> [f32; 2] {
    let Some(position) = actor.position.as_deref() else {
        return config
            .actor_positions
            .get("center")
            .copied()
            .unwrap_or([0.5, 1.0]);
    };
    if let Some(named) = config.actor_positions.get(position) {
        return *named;
    }
    parse_xy(position).unwrap_or_else(|| {
        config
            .actor_positions
            .get("center")
            .copied()
            .unwrap_or([0.5, 1.0])
    })
}

fn actor_size(config: &VnUiPresentationConfig, image_size: Option<[u32; 2]>) -> [f32; 2] {
    let height = (config.scene_size[1] * config.actor_height * config.actor_scale).max(1.0);
    let aspect = image_size
        .filter(|size| size[0] > 0 && size[1] > 0)
        .map(|size| size[0] as f32 / size[1] as f32)
        .unwrap_or(0.55);
    [height * aspect, height]
}

fn fitted_image_rect(
    position: [f32; 2],
    bounds: [f32; 2],
    image_size: Option<[u32; 2]>,
    fit: VnUiImageFit,
) -> ([f32; 4], [f32; 4]) {
    let Some(image_size) = image_size.filter(|size| size[0] > 0 && size[1] > 0) else {
        return (
            [position[0], position[1], bounds[0], bounds[1]],
            [0.0, 0.0, 1.0, 1.0],
        );
    };
    let image_w = image_size[0] as f32;
    let image_h = image_size[1] as f32;
    let bounds_w = bounds[0].max(1.0);
    let bounds_h = bounds[1].max(1.0);

    match fit {
        VnUiImageFit::Contain => {
            let scale = (bounds_w / image_w).min(bounds_h / image_h);
            let width = image_w * scale;
            let height = image_h * scale;
            (
                [
                    position[0] + (bounds_w - width) * 0.5,
                    position[1] + (bounds_h - height) * 0.5,
                    width,
                    height,
                ],
                [0.0, 0.0, 1.0, 1.0],
            )
        }
        VnUiImageFit::AutoFill => {
            let target_aspect = bounds_w / bounds_h;
            let image_aspect = image_w / image_h;
            let uv = if image_aspect > target_aspect {
                let visible_width = target_aspect / image_aspect;
                let margin = (1.0 - visible_width) * 0.5;
                [margin, 0.0, 1.0 - margin, 1.0]
            } else {
                let visible_height = image_aspect / target_aspect;
                let margin = (1.0 - visible_height) * 0.5;
                [0.0, margin, 1.0, 1.0 - margin]
            };
            ([position[0], position[1], bounds_w, bounds_h], uv)
        }
        VnUiImageFit::FillWidth => {
            let scaled_h = bounds_w * image_h / image_w;
            if scaled_h <= bounds_h {
                let y = position[1] + (bounds_h - scaled_h) * 0.5;
                ([position[0], y, bounds_w, scaled_h], [0.0, 0.0, 1.0, 1.0])
            } else {
                let visible_height = bounds_h / scaled_h;
                let margin = (1.0 - visible_height) * 0.5;
                (
                    [position[0], position[1], bounds_w, bounds_h],
                    [0.0, margin, 1.0, 1.0 - margin],
                )
            }
        }
        VnUiImageFit::FillHeight => {
            let scaled_w = bounds_h * image_w / image_h;
            if scaled_w <= bounds_w {
                let x = position[0] + (bounds_w - scaled_w) * 0.5;
                ([x, position[1], scaled_w, bounds_h], [0.0, 0.0, 1.0, 1.0])
            } else {
                let visible_width = bounds_w / scaled_w;
                let margin = (1.0 - visible_width) * 0.5;
                (
                    [position[0], position[1], bounds_w, bounds_h],
                    [margin, 0.0, 1.0 - margin, 1.0],
                )
            }
        }
    }
}

fn parse_xy(raw: &str) -> Option<[f32; 2]> {
    let (x, y) = raw.split_once(',')?;
    Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
}

pub fn sync_dialogue_ui_to_world(
    world: &mut World,
    entities: &mut VnUiEntities,
    dialogue: &VnDialogueState,
    mode: &VnUiMode,
    config: &VnUiPresentationConfig,
) {
    sync_dialogue_ui_to_world_debug(world, entities, dialogue, mode, config, None);
}

fn sync_dialogue_ui_to_world_debug(
    world: &mut World,
    entities: &mut VnUiEntities,
    dialogue: &VnDialogueState,
    mode: &VnUiMode,
    config: &VnUiPresentationConfig,
    debug: Option<VnUiDebugFrame>,
) {
    let visible = matches!(mode, VnUiMode::Reading | VnUiMode::Debug);
    debug_log_dialogue(debug, config, dialogue, visible);
    ensure_dialogue_widgets(world, entities, config);
    ensure_choice_widgets(world, entities, dialogue.choices.len(), config);

    let speaker = dialogue
        .current_line
        .as_ref()
        .and_then(|line| line.speaker.clone())
        .unwrap_or_default();
    let line = if dialogue.line_complete {
        dialogue
            .current_line
            .as_ref()
            .map(|line| line.text.clone())
            .unwrap_or_default()
    } else {
        dialogue.visible_text()
    };

    set_visible(
        world,
        entities.dialogue_panel,
        visible && dialogue.current_line.is_some(),
    );
    set_visible(world, entities.speaker_text, visible && !speaker.is_empty());
    set_visible(
        world,
        entities.line_text,
        visible && dialogue.current_line.is_some(),
    );
    set_visible(
        world,
        entities.advance_button,
        visible && dialogue.current_line.is_some(),
    );
    set_text(world, entities.speaker_text, speaker);
    set_text(world, entities.line_text, line);

    let choices_visible = visible && !dialogue.choices.is_empty();
    set_visible(world, entities.choice_panel, choices_visible);
    for (index, entity) in entities.choice_buttons.iter().copied().enumerate() {
        let Some(choice) = dialogue.choices.get(index) else {
            set_visible(world, Some(entity), false);
            continue;
        };
        set_visible(world, Some(entity), choices_visible);
        if let Some(button) = world.get_mut::<UiButton>(entity) {
            button.label = choice.text.clone();
            if index == dialogue.selected_choice {
                button.normal_color = config.button_hover_color;
            } else {
                button.normal_color = config.button_color;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct VnUiDebugFrame {
    frame: usize,
}

fn begin_vn_ui_debug(
    surface_size: [f32; 2],
    config: &VnUiPresentationConfig,
    scene: &VnSceneState,
) -> Option<VnUiDebugFrame> {
    let raw = std::env::var("SKY_VN_UI_DEBUG").ok()?;
    if raw.is_empty() || raw == "0" || raw.eq_ignore_ascii_case("false") {
        return None;
    }
    let limit = raw.parse::<usize>().unwrap_or(12);
    let frame = VN_UI_DEBUG_FRAMES.fetch_add(1, Ordering::Relaxed);
    if frame >= limit {
        return None;
    }
    eprintln!(
        "[SkyEngine][VN UI][frame {frame}] surface={:?} scene_pos={:?} scene_size={:?} bg_fit={:?} cg_fit={:?} bg={:?} cg={:?} actors={}",
        surface_size,
        config.scene_position,
        config.scene_size,
        config.background_fit,
        config.cg_fit,
        scene.background.as_ref().map(|layer| layer.asset.as_str()),
        scene.cg.as_ref().map(|layer| layer.asset.as_str()),
        scene.actors.len(),
    );
    Some(VnUiDebugFrame { frame })
}

fn debug_log_image_layer(
    debug: Option<VnUiDebugFrame>,
    layer_name: &str,
    asset: &str,
    fit: VnUiImageFit,
    scene_bounds: ([f32; 2], [f32; 2]),
    image_size: Option<[u32; 2]>,
    rect: [f32; 4],
    uv: [f32; 4],
) {
    let Some(debug) = debug else {
        return;
    };
    eprintln!(
        "[SkyEngine][VN UI][frame {}] image layer={} asset={} fit={:?} bounds_pos={:?} bounds_size={:?} image_size={:?} rect={:?} uv={:?}",
        debug.frame, layer_name, asset, fit, scene_bounds.0, scene_bounds.1, image_size, rect, uv
    );
}

fn debug_log_actor(
    debug: Option<VnUiDebugFrame>,
    actor: &VnActor,
    asset: &str,
    image_size: Option<[u32; 2]>,
    anchor: [f32; 2],
    rect: [f32; 4],
    scene_size: [f32; 2],
) {
    let Some(debug) = debug else {
        return;
    };
    eprintln!(
        "[SkyEngine][VN UI][frame {}] actor id={} asset={} image_size={:?} anchor={:?} scene_size={:?} rect={:?} opacity={:.3}",
        debug.frame, actor.id, asset, image_size, anchor, scene_size, rect, actor.opacity
    );
}

fn debug_log_dialogue(
    debug: Option<VnUiDebugFrame>,
    config: &VnUiPresentationConfig,
    dialogue: &VnDialogueState,
    visible: bool,
) {
    let Some(debug) = debug else {
        return;
    };
    eprintln!(
        "[SkyEngine][VN UI][frame {}] dialogue visible={} has_line={} choices={} panel_pos={:?} panel_size={:?} choice_pos={:?} choice_size={:?} fonts=({:.1},{:.1})",
        debug.frame,
        visible,
        dialogue.current_line.is_some(),
        dialogue.choices.len(),
        config.dialogue_position,
        config.dialogue_size,
        config.choice_position,
        config.choice_size,
        config.speaker_font_size,
        config.line_font_size,
    );
}

pub fn apply_vn_ui_events(world: &mut World) -> VnRuntimeResult<Vec<VnRuntimeEvent>> {
    let mut actions = Vec::new();
    if let Some(events) = world.get_resource_mut::<UiEvents>() {
        let mut retained = Vec::new();
        for event in events.drain() {
            if event.kind == UiEventKind::Clicked {
                if let Some(id) = event.id.as_ref().and_then(parse_vn_ui_action) {
                    actions.push(id);
                    continue;
                }
            }
            retained.push(event);
        }
        for event in retained {
            events.push(event);
        }
    }

    let mut output = Vec::new();
    let Some(runtime) = world.get_resource_mut::<VnRuntime>() else {
        return Ok(output);
    };

    for action in actions {
        match action {
            VnUiAction::Advance => {
                if let Some(event) = runtime.apply_action(VnAction::Advance)? {
                    output.push(event);
                }
            }
            VnUiAction::Choice(index) => {
                if runtime.status() == &VnStatus::Choice {
                    runtime.dialogue_mut().selected_choice = index;
                    runtime.choose(index)?;
                    output.push(runtime.advance()?);
                }
            }
        }
    }

    Ok(output)
}

fn ensure_dialogue_widgets(
    world: &mut World,
    entities: &mut VnUiEntities,
    config: &VnUiPresentationConfig,
) {
    let panel = match live_entity(world, entities.dialogue_panel) {
        Some(entity) => entity,
        None => {
            let entity = world.spawn((
                UiNode::panel(config.dialogue_size[0], config.dialogue_size[1])
                    .anchor(UiAnchor::TopLeft)
                    .at(config.dialogue_position[0], config.dialogue_position[1])
                    .z(100),
                UiPanel::new(config.panel_color),
                VnDialogueUi,
            ));
            entities.dialogue_panel = Some(entity);
            entity
        }
    };
    if let Some(node) = world.get_mut::<UiNode>(panel) {
        node.position = config.dialogue_position;
        node.size = [
            UiLength::Px(config.dialogue_size[0]),
            UiLength::Px(config.dialogue_size[1]),
        ];
    }
    if let Some(panel_component) = world.get_mut::<UiPanel>(panel) {
        panel_component.color = config.panel_color;
    }

    let speaker = match live_entity(world, entities.speaker_text) {
        Some(entity) => entity,
        None => {
            let entity = world.spawn((
                UiNode::panel(320.0, 28.0)
                    .child_of(panel)
                    .at(28.0, 18.0)
                    .z(101),
                UiText::new("")
                    .size(config.speaker_font_size)
                    .color(config.speaker_color),
            ));
            entities.speaker_text = Some(entity);
            entity
        }
    };
    if let Some(text) = world.get_mut::<UiText>(speaker) {
        text.color = config.speaker_color;
        text.font_size = config.speaker_font_size;
    }

    let line = match live_entity(world, entities.line_text) {
        Some(entity) => entity,
        None => {
            let entity = world.spawn((
                UiNode::panel(config.dialogue_size[0] - 120.0, 82.0)
                    .child_of(panel)
                    .at(28.0, 56.0)
                    .z(101),
                UiText::new("")
                    .size(config.line_font_size)
                    .color(config.text_color),
            ));
            entities.line_text = Some(entity);
            entity
        }
    };
    if let Some(node) = world.get_mut::<UiNode>(line) {
        node.size = [
            UiLength::Px((config.dialogue_size[0] - 120.0).max(120.0)),
            UiLength::Px(82.0),
        ];
    }
    if let Some(text) = world.get_mut::<UiText>(line) {
        text.color = config.text_color;
        text.font_size = config.line_font_size;
    }

    let advance = match live_entity(world, entities.advance_button) {
        Some(entity) => entity,
        None => {
            let entity = world.spawn((
                UiNode::panel(86.0, 34.0)
                    .id("vn.advance")
                    .child_of(panel)
                    .at(
                        config.dialogue_size[0] - 112.0,
                        config.dialogue_size[1] - 50.0,
                    )
                    .z(101),
                styled_button("Next", config),
            ));
            entities.advance_button = Some(entity);
            entity
        }
    };
    if let Some(node) = world.get_mut::<UiNode>(advance) {
        node.position = [
            (config.dialogue_size[0] - 112.0).max(20.0),
            (config.dialogue_size[1] - 50.0).max(20.0),
        ];
    }
    if let Some(button) = world.get_mut::<UiButton>(advance) {
        apply_button_style(button, config);
    }
}

fn ensure_choice_widgets(
    world: &mut World,
    entities: &mut VnUiEntities,
    choice_count: usize,
    config: &VnUiPresentationConfig,
) {
    let panel = match live_entity(world, entities.choice_panel) {
        Some(entity) => entity,
        None => {
            let entity = world.spawn((
                UiNode::panel(config.choice_size[0], config.choice_size[1])
                    .anchor(UiAnchor::TopLeft)
                    .at(config.choice_position[0], config.choice_position[1])
                    .z(110)
                    .layout(UiLayout::column(
                        UiRect::new(18.0, 18.0, 18.0, 18.0),
                        10.0,
                        UiAlign::Stretch,
                    )),
                UiPanel::new(config.choice_panel_color),
                VnChoiceUi,
            ));
            entities.choice_panel = Some(entity);
            entity
        }
    };
    if let Some(node) = world.get_mut::<UiNode>(panel) {
        node.position = config.choice_position;
        node.size = [
            UiLength::Px(config.choice_size[0]),
            UiLength::Px(config.choice_size[1]),
        ];
    }
    if let Some(panel_component) = world.get_mut::<UiPanel>(panel) {
        panel_component.color = config.choice_panel_color;
    }

    while entities.choice_buttons.len() < choice_count {
        let index = entities.choice_buttons.len();
        let id = format!("vn.choice.{index}");
        let entity = world.spawn((
            UiNode::panel(1.0, config.choice_button_height)
                .id(UiId::new(id))
                .child_of(panel)
                .width(UiLength::Percent(1.0))
                .z(111),
            styled_button("", config),
        ));
        entities.choice_buttons.push(entity);
    }

    for entity in entities.choice_buttons.iter().copied() {
        if let Some(button) = world.get_mut::<UiButton>(entity) {
            apply_button_style(button, config);
        }
    }
}

fn live_entity(world: &World, entity: Option<EntityId>) -> Option<EntityId> {
    entity.filter(|entity| world.contains(*entity))
}

fn despawn_slot(world: &mut World, slot: &mut Option<EntityId>) {
    if let Some(entity) = slot.take() {
        world.despawn(entity);
    }
}

fn set_visible(world: &mut World, entity: Option<EntityId>, visible: bool) {
    if let Some(entity) = entity {
        if let Some(node) = world.get_mut::<UiNode>(entity) {
            node.visible = visible;
            node.enabled = visible;
        }
    }
}

fn set_text(world: &mut World, entity: Option<EntityId>, text: String) {
    if let Some(entity) = entity {
        if let Some(ui_text) = world.get_mut::<UiText>(entity) {
            ui_text.text = text;
        }
    }
}

fn styled_button(label: impl Into<String>, config: &VnUiPresentationConfig) -> UiButton {
    let mut button = UiButton::new(label);
    apply_button_style(&mut button, config);
    button
}

fn apply_button_style(button: &mut UiButton, config: &VnUiPresentationConfig) {
    button.normal_color = config.button_color;
    button.hover_color = config.button_hover_color;
    button.pressed_color = config.button_pressed_color;
    button.text_color = config.text_color;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VnUiAction {
    Advance,
    Choice(usize),
}

fn parse_vn_ui_action(id: &UiId) -> Option<VnUiAction> {
    match id.as_str() {
        "vn.advance" => Some(VnUiAction::Advance),
        value => value
            .strip_prefix("vn.choice.")
            .and_then(|index| index.parse().ok())
            .map(VnUiAction::Choice),
    }
}

#[cfg(test)]
mod tests {
    use crate::asset::{AssetConfig, AssetServer, TextureAsset};
    use crate::ui::{UiEvent, UiEvents};
    use crate::vn::script::{VnValue, YarnInstruction, YarnScript};

    use super::*;

    #[test]
    fn ui_sync_spawns_dialogue_and_choices() {
        let mut runtime = choice_runtime();
        runtime.advance().unwrap();
        runtime.dialogue_mut().complete_line();
        runtime.advance().unwrap();

        let mut world = World::new();
        world.insert_resource(runtime);
        sync_runtime_ui_to_world(&mut world);

        let entities = world.get_resource::<VnUiEntities>().unwrap().clone();
        assert!(world
            .get::<VnDialogueUi>(entities.dialogue_panel.unwrap())
            .is_some());
        assert!(world
            .get::<VnChoiceUi>(entities.choice_panel.unwrap())
            .is_some());
        assert_eq!(entities.choice_buttons.len(), 2);
        assert_eq!(
            world
                .get::<UiButton>(entities.choice_buttons[1])
                .unwrap()
                .label,
            "B"
        );
    }

    #[test]
    fn ui_events_choose_active_choice() {
        let mut runtime = choice_runtime();
        runtime.advance().unwrap();
        runtime.dialogue_mut().complete_line();
        runtime.advance().unwrap();

        let mut world = World::new();
        world.insert_resource(runtime);
        sync_runtime_ui_to_world(&mut world);
        let entities = world.get_resource::<VnUiEntities>().unwrap().clone();
        world.insert_resource(UiEvents::new());
        world
            .get_resource_mut::<UiEvents>()
            .unwrap()
            .push(UiEvent::clicked(
                entities.choice_buttons[1],
                Some(UiId::new("vn.choice.1")),
            ));

        let events = apply_vn_ui_events(&mut world).unwrap();
        assert!(matches!(events.first(), Some(VnRuntimeEvent::Line(_))));
        assert_eq!(
            world.get_resource::<VnRuntime>().unwrap().variable("route"),
            Some(&VnValue::String("b".to_owned()))
        );
    }

    #[test]
    fn dialogue_and_choice_positions_are_top_left_screen_pixels() {
        let mut runtime = choice_runtime();
        runtime.advance().unwrap();
        runtime.dialogue_mut().complete_line();
        runtime.advance().unwrap();

        let config = VnUiPresentationConfig {
            dialogue_position: [20.0, 300.0],
            dialogue_size: [400.0, 120.0],
            choice_position: [90.0, 80.0],
            choice_size: [300.0, 160.0],
            ..Default::default()
        };
        let mut world = World::new();
        world.insert_resource(runtime);
        world.insert_resource(config);

        sync_runtime_ui_to_world(&mut world);

        let entities = world.get_resource::<VnUiEntities>().unwrap();
        let dialogue = world
            .get::<UiNode>(entities.dialogue_panel.unwrap())
            .unwrap();
        let choice = world.get::<UiNode>(entities.choice_panel.unwrap()).unwrap();
        assert_eq!(dialogue.anchor, UiAnchor::TopLeft);
        assert_eq!(dialogue.position, [20.0, 300.0]);
        assert_eq!(choice.anchor, UiAnchor::TopLeft);
        assert_eq!(choice.position, [90.0, 80.0]);
    }

    #[test]
    fn scene_ui_sync_uses_image_fit_and_actor_layout() {
        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let background = asset_server.insert_runtime(TextureAsset::white_pixel());
        let cg = asset_server.insert_runtime(TextureAsset::white_pixel());
        let actor = asset_server.insert_runtime(TextureAsset::white_pixel());
        let mut textures = VnSpriteTextureMap::default();
        textures.insert_with_size("bg", background, [1000, 500]);
        textures.insert_with_size("cg", cg, [400, 800]);
        textures.insert_with_size("alice_pose", actor, [300, 600]);

        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "bg" layer=0>>
<<cg "cg" layer=40>>
<<show alice "alice_pose" at="right" layer=20 opacity=0.5>>
===
"#,
        )
        .unwrap();
        let mut scene = VnSceneState::default();
        for instruction in &script.node("Start").unwrap().body {
            if let YarnInstruction::Command(command) = instruction {
                scene.apply_command(command);
            }
        }

        let config = VnUiPresentationConfig {
            scene_position: [10.0, 20.0],
            scene_size: [800.0, 600.0],
            actor_height: 0.9,
            ..Default::default()
        };
        let mut world = World::new();
        let mut entities = VnUiEntities::default();

        sync_scene_ui_to_world(&mut world, &mut entities, &scene, &config, &textures);

        let background_entity = entities.background_image.unwrap();
        let background_node = world.get::<UiNode>(background_entity).unwrap();
        let background_image = world.get::<UiImage>(background_entity).unwrap();
        assert_eq!(background_node.position, [10.0, 20.0]);
        assert_eq!(
            background_node.size,
            [UiLength::Px(800.0), UiLength::Px(600.0)]
        );
        assert_slice_near(background_image.uv_rect, [0.16666666, 0.0, 0.8333333, 1.0]);

        let cg_entity = entities.cg_image.unwrap();
        let cg_node = world.get::<UiNode>(cg_entity).unwrap();
        let cg_image = world.get::<UiImage>(cg_entity).unwrap();
        assert_eq!(cg_node.position, [10.0, 20.0]);
        assert_eq!(cg_node.size, [UiLength::Px(800.0), UiLength::Px(600.0)]);
        assert_slice_near(cg_image.uv_rect, [0.0, 0.3125, 1.0, 0.6875]);

        let actor_entity = entities.actors["alice"];
        let actor_node = world.get::<UiNode>(actor_entity).unwrap();
        let actor_image = world.get::<UiImage>(actor_entity).unwrap();
        assert_eq!(actor_node.position, [419.0, 80.0]);
        assert_eq!(actor_node.size, [UiLength::Px(270.0), UiLength::Px(540.0)]);
        assert!((actor_image.color.a - 0.5).abs() < 0.001);
    }

    #[test]
    fn image_fit_modes_compute_expected_rects_and_uvs() {
        let (rect, uv) = fitted_image_rect(
            [10.0, 20.0],
            [800.0, 600.0],
            Some([400, 800]),
            VnUiImageFit::AutoFill,
        );
        assert_slice_near(rect, [10.0, 20.0, 800.0, 600.0]);
        assert_slice_near(uv, [0.0, 0.3125, 1.0, 0.6875]);

        let (rect, uv) = fitted_image_rect(
            [10.0, 20.0],
            [800.0, 600.0],
            Some([400, 800]),
            VnUiImageFit::Contain,
        );
        assert_slice_near(rect, [260.0, 20.0, 300.0, 600.0]);
        assert_slice_near(uv, [0.0, 0.0, 1.0, 1.0]);

        let (rect, uv) = fitted_image_rect(
            [10.0, 20.0],
            [800.0, 600.0],
            Some([400, 800]),
            VnUiImageFit::FillWidth,
        );
        assert_slice_near(rect, [10.0, 20.0, 800.0, 600.0]);
        assert_slice_near(uv, [0.0, 0.3125, 1.0, 0.6875]);

        let (rect, uv) = fitted_image_rect(
            [10.0, 20.0],
            [800.0, 600.0],
            Some([1600, 600]),
            VnUiImageFit::FillHeight,
        );
        assert_slice_near(rect, [10.0, 20.0, 800.0, 600.0]);
        assert_slice_near(uv, [0.25, 0.0, 0.75, 1.0]);
    }

    #[test]
    fn actor_layout_uses_visible_texture_rect() {
        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let actor = asset_server.insert_runtime(TextureAsset::white_pixel());
        let mut textures = VnSpriteTextureMap::default();
        textures.insert_with_visible_rect("wide_pose", actor, [1000, 500], [400, 0, 200, 500]);

        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<show alice "wide_pose" at="right" layer=20>>
===
"#,
        )
        .unwrap();
        let mut scene = VnSceneState::default();
        for instruction in &script.node("Start").unwrap().body {
            if let YarnInstruction::Command(command) = instruction {
                scene.apply_command(command);
            }
        }

        let config = VnUiPresentationConfig {
            scene_position: [10.0, 20.0],
            scene_size: [800.0, 600.0],
            actor_height: 0.9,
            ..Default::default()
        };
        let mut world = World::new();
        let mut entities = VnUiEntities::default();

        sync_scene_ui_to_world(&mut world, &mut entities, &scene, &config, &textures);

        let actor_entity = entities.actors["alice"];
        let actor_node = world.get::<UiNode>(actor_entity).unwrap();
        let actor_image = world.get::<UiImage>(actor_entity).unwrap();
        assert_slice_near(
            [
                actor_node.position[0],
                actor_node.position[1],
                ui_px(actor_node.size[0]),
                ui_px(actor_node.size[1]),
            ],
            [446.0, 80.0, 216.0, 540.0],
        );
        assert_slice_near(actor_image.uv_rect, [0.4, 0.0, 0.6, 1.0]);
    }

    #[test]
    fn surface_sync_uses_current_window_bounds_for_vn_layout() {
        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let background = asset_server.insert_runtime(TextureAsset::white_pixel());
        let actor = asset_server.insert_runtime(TextureAsset::white_pixel());
        let mut textures = VnSpriteTextureMap::default();
        textures.insert_with_size("bg", background, [1600, 900]);
        textures.insert_with_size("alice_pose", actor, [300, 600]);

        let script = YarnScript::parse_str(
            r#"
title: Start
---
<<scene "bg" layer=0>>
<<show alice "alice_pose" at="right" layer=20>>
Alice: Surface sized. #line:start.1
===
"#,
        )
        .unwrap();
        let mut runtime = VnRuntime::from_script(script, "Start").unwrap();
        while runtime.status() == &VnStatus::Ready {
            let event = runtime.advance().unwrap();
            if matches!(event, VnRuntimeEvent::Line(_)) {
                break;
            }
        }

        let mut world = World::new();
        world.insert_resource(runtime);
        world.insert_resource(textures);

        sync_runtime_ui_to_world_with_surface(&mut world, [1600.0, 900.0]);

        let entities = world.get_resource::<VnUiEntities>().unwrap();
        let background_node = world
            .get::<UiNode>(entities.background_image.unwrap())
            .unwrap();
        assert_eq!(background_node.position, [0.0, 0.0]);
        assert_eq!(
            background_node.size,
            [UiLength::Px(1600.0), UiLength::Px(900.0)]
        );

        let actor_node = world.get::<UiNode>(entities.actors["alice"]).unwrap();
        assert_slice_near(
            [
                actor_node.position[0],
                actor_node.position[1],
                ui_px(actor_node.size[0]),
                ui_px(actor_node.size[1]),
            ],
            [876.5, 54.0, 423.0, 846.0],
        );

        let dialogue_node = world
            .get::<UiNode>(entities.dialogue_panel.unwrap())
            .unwrap();
        assert_slice_near(
            [
                dialogue_node.position[0],
                dialogue_node.position[1],
                ui_px(dialogue_node.size[0]),
                ui_px(dialogue_node.size[1]),
            ],
            [64.0, 648.0, 1472.0, 207.0],
        );
    }

    #[test]
    fn texture_map_plain_insert_clears_stale_size() {
        let asset_server = AssetServer::with_empty_manifest(AssetConfig::default());
        let first = asset_server.insert_runtime(TextureAsset::white_pixel());
        let second = asset_server.insert_runtime(TextureAsset::white_pixel());
        let mut textures = VnSpriteTextureMap::default();

        textures.insert_with_size("asset", first, [320, 240]);
        assert_eq!(textures.size("asset"), Some([320, 240]));

        textures.insert("asset", second);
        assert_eq!(textures.size("asset"), None);
        assert_eq!(textures.get("asset"), Some(second));
    }

    fn choice_runtime() -> VnRuntime {
        let script = YarnScript::parse_str(
            r#"
title: Start
---
Hello. #line:start.1
-> A
    <<set $route = "a">>
    <<jump Ending>>
-> B
    <<set $route = "b">>
    <<jump Ending>>
===

title: Ending
---
Done. #line:end.1
===
"#,
        )
        .unwrap();
        VnRuntime::from_script(script, "Start").unwrap()
    }

    fn assert_slice_near(actual: [f32; 4], expected: [f32; 4]) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < 0.001,
                "expected {expected}, got {actual}"
            );
        }
    }

    fn ui_px(length: UiLength) -> f32 {
        match length {
            UiLength::Px(value) => value,
            other => panic!("expected px length, got {other:?}"),
        }
    }
}
