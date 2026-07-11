use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::render::graph::{BufferHandle, TextureHandle};
use crate::render::resources::SceneShadowResources;
use crate::render::view::ViewportRect;

use super::payload::PreparedView;
use super::TextureFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextureSlot {
    handle: TextureHandle,
    format: TextureFormat,
}

impl TextureSlot {
    #[inline]
    pub const fn new(handle: TextureHandle, format: TextureFormat) -> Self {
        Self { handle, format }
    }

    #[inline]
    pub const fn handle(self) -> TextureHandle {
        self.handle
    }

    #[inline]
    pub const fn format(self) -> TextureFormat {
        self.format
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneTexture {
    Color,
    Depth,
    Normal,
    Velocity,
    Albedo,
    Material,
    Emissive,
    Light,
    IndirectDiffuse,
}

impl SceneTexture {
    pub const ALL: [Self; 9] = [
        Self::Color,
        Self::Depth,
        Self::Normal,
        Self::Velocity,
        Self::Albedo,
        Self::Material,
        Self::Emissive,
        Self::Light,
        Self::IndirectDiffuse,
    ];
    pub const MATERIAL_ROUGHNESS_CHANNEL: usize = 0;
    pub const MATERIAL_METALLIC_CHANNEL: usize = 1;
    pub const MATERIAL_AO_CHANNEL: usize = 2;
    pub const MATERIAL_FLAGS_CHANNEL: usize = 3;

    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            SceneTexture::Color => "scene color",
            SceneTexture::Depth => "scene depth",
            SceneTexture::Normal => "scene normal",
            SceneTexture::Velocity => "scene velocity",
            SceneTexture::Albedo => "scene albedo",
            SceneTexture::Material => "scene material",
            SceneTexture::Emissive => "scene emissive",
            SceneTexture::Light => "scene light",
            SceneTexture::IndirectDiffuse => "scene indirect diffuse",
        }
    }

    #[inline]
    pub const fn debug_name(self) -> &'static str {
        match self {
            SceneTexture::Color => "scene_color",
            SceneTexture::Depth => "scene_depth",
            SceneTexture::Normal => "scene_normal",
            SceneTexture::Velocity => "scene_velocity",
            SceneTexture::Albedo => "scene_albedo",
            SceneTexture::Material => "scene_material",
            SceneTexture::Emissive => "scene_emissive",
            SceneTexture::Light => "scene_light",
            SceneTexture::IndirectDiffuse => "scene_indirect_diffuse",
        }
    }

    #[inline]
    pub const fn modern_3d_format(self) -> TextureFormat {
        match self {
            SceneTexture::Color
            | SceneTexture::Emissive
            | SceneTexture::Light
            | SceneTexture::IndirectDiffuse => TextureFormat::Rgba16Float,
            SceneTexture::Depth => TextureFormat::Depth32Float,
            SceneTexture::Normal | SceneTexture::Albedo | SceneTexture::Material => {
                TextureFormat::Rgba8Unorm
            }
            // The current velocity target keeps a vec4 render-target contract:
            // xy = motion, zw = reset/validity space for temporal passes.
            SceneTexture::Velocity => TextureFormat::Rgba16Float,
        }
    }

    #[inline]
    pub const fn is_modern_3d_gbuffer(self) -> bool {
        match self {
            SceneTexture::Depth
            | SceneTexture::Normal
            | SceneTexture::Velocity
            | SceneTexture::Albedo
            | SceneTexture::Material
            | SceneTexture::Emissive => true,
            SceneTexture::Color | SceneTexture::Light | SceneTexture::IndirectDiffuse => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SceneGBufferSlots {
    color: Option<TextureSlot>,
    depth: Option<TextureSlot>,
    normal: Option<TextureSlot>,
    velocity: Option<TextureSlot>,
    albedo: Option<TextureSlot>,
    material: Option<TextureSlot>,
    emissive: Option<TextureSlot>,
    light: Option<TextureSlot>,
    indirect_diffuse: Option<TextureSlot>,
}

impl SceneGBufferSlots {
    #[inline]
    pub fn get(&self, texture: SceneTexture) -> Option<TextureSlot> {
        match texture {
            SceneTexture::Color => self.color(),
            SceneTexture::Depth => self.depth(),
            SceneTexture::Normal => self.normal(),
            SceneTexture::Velocity => self.velocity(),
            SceneTexture::Albedo => self.albedo(),
            SceneTexture::Material => self.material(),
            SceneTexture::Emissive => self.emissive(),
            SceneTexture::Light => self.light(),
            SceneTexture::IndirectDiffuse => self.indirect_diffuse(),
        }
    }

    #[inline]
    pub fn set(&mut self, texture: SceneTexture, slot: TextureSlot) -> Option<TextureSlot> {
        match texture {
            SceneTexture::Color => self.set_color(slot),
            SceneTexture::Depth => self.set_depth(slot),
            SceneTexture::Normal => self.set_normal(slot),
            SceneTexture::Velocity => self.set_velocity(slot),
            SceneTexture::Albedo => self.set_albedo(slot),
            SceneTexture::Material => self.set_material(slot),
            SceneTexture::Emissive => self.set_emissive(slot),
            SceneTexture::Light => self.set_light(slot),
            SceneTexture::IndirectDiffuse => self.set_indirect_diffuse(slot),
        }
    }

    #[inline]
    pub fn color(&self) -> Option<TextureSlot> {
        self.color
    }

    #[inline]
    pub fn depth(&self) -> Option<TextureSlot> {
        self.depth
    }

    #[inline]
    pub fn normal(&self) -> Option<TextureSlot> {
        self.normal
    }

    #[inline]
    pub fn velocity(&self) -> Option<TextureSlot> {
        self.velocity
    }

    #[inline]
    pub fn albedo(&self) -> Option<TextureSlot> {
        self.albedo
    }

    #[inline]
    pub fn material(&self) -> Option<TextureSlot> {
        self.material
    }

    #[inline]
    pub fn emissive(&self) -> Option<TextureSlot> {
        self.emissive
    }

    #[inline]
    pub fn light(&self) -> Option<TextureSlot> {
        self.light
    }

    #[inline]
    pub fn indirect_diffuse(&self) -> Option<TextureSlot> {
        self.indirect_diffuse
    }

    #[inline]
    pub fn set_color(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.color;
        self.color = Some(slot);
        previous
    }

    #[inline]
    pub fn set_depth(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.depth;
        self.depth = Some(slot);
        previous
    }

    #[inline]
    pub fn set_normal(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.normal;
        self.normal = Some(slot);
        previous
    }

    #[inline]
    pub fn set_velocity(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.velocity;
        self.velocity = Some(slot);
        previous
    }

    #[inline]
    pub fn set_albedo(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.albedo;
        self.albedo = Some(slot);
        previous
    }

    #[inline]
    pub fn set_material(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.material;
        self.material = Some(slot);
        previous
    }

    #[inline]
    pub fn set_emissive(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.emissive;
        self.emissive = Some(slot);
        previous
    }

    #[inline]
    pub fn set_light(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.light;
        self.light = Some(slot);
        previous
    }

    #[inline]
    pub fn set_indirect_diffuse(&mut self, slot: TextureSlot) -> Option<TextureSlot> {
        let previous = self.indirect_diffuse;
        self.indirect_diffuse = Some(slot);
        previous
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotResource {
    Texture(TextureSlot),
    Buffer(BufferHandle),
}

const INLINE_RESOURCE_SLOT_CAPACITY: usize = 8;

#[derive(Debug, Clone)]
struct InlineResourceSlots {
    inline: [Option<(Cow<'static, str>, SlotResource)>; INLINE_RESOURCE_SLOT_CAPACITY],
    inline_len: usize,
    overflow: FxHashMap<Cow<'static, str>, SlotResource>,
}

impl Default for InlineResourceSlots {
    fn default() -> Self {
        Self {
            inline: std::array::from_fn(|_| None),
            inline_len: 0,
            overflow: FxHashMap::default(),
        }
    }
}

impl InlineResourceSlots {
    fn insert(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        resource: SlotResource,
    ) -> Option<SlotResource> {
        let name = name.into();
        for entry in self.inline[..self.inline_len].iter_mut().flatten() {
            if entry.0 == name {
                return Some(std::mem::replace(&mut entry.1, resource));
            }
        }

        if self.inline_len < INLINE_RESOURCE_SLOT_CAPACITY {
            self.inline[self.inline_len] = Some((name, resource));
            self.inline_len += 1;
            return None;
        }

        self.overflow.insert(name, resource)
    }

    fn get(&self, name: &str) -> Option<SlotResource> {
        self.inline[..self.inline_len]
            .iter()
            .flatten()
            .find_map(|(candidate, resource)| (candidate.as_ref() == name).then_some(*resource))
            .or_else(|| self.overflow.get(name).copied())
    }

    fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

#[derive(Debug, Clone, Default)]
pub struct ResourceSlotMap {
    slots: InlineResourceSlots,
}

impl ResourceSlotMap {
    pub const CURRENT_COLOR: &'static str = "current_color";

    #[inline]
    pub fn insert_texture(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots
            .insert(
                name.into(),
                SlotResource::Texture(TextureSlot::new(handle, format)),
            )
            .and_then(|previous| match previous {
                SlotResource::Texture(slot) => Some(slot),
                SlotResource::Buffer(_) => None,
            })
    }

    #[inline]
    pub fn insert_buffer(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: BufferHandle,
    ) -> Option<BufferHandle> {
        self.slots
            .insert(name.into(), SlotResource::Buffer(handle))
            .and_then(|previous| match previous {
                SlotResource::Buffer(handle) => Some(handle),
                SlotResource::Texture(_) => None,
            })
    }

    #[inline]
    pub fn texture(&self, name: &str) -> Option<TextureSlot> {
        self.slots.get(name).and_then(|resource| match resource {
            SlotResource::Texture(slot) => Some(slot),
            SlotResource::Buffer(_) => None,
        })
    }

    #[inline]
    pub fn buffer(&self, name: &str) -> Option<BufferHandle> {
        self.slots.get(name).and_then(|resource| match resource {
            SlotResource::Buffer(handle) => Some(handle),
            SlotResource::Texture(_) => None,
        })
    }

    #[inline]
    pub fn resource(&self, name: &str) -> Option<SlotResource> {
        self.slots.get(name)
    }

    #[inline]
    pub fn contains(&self, name: &str) -> bool {
        self.slots.contains(name)
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let _ = self.insert_texture(Self::CURRENT_COLOR, handle, format);
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.texture(Self::CURRENT_COLOR)
    }
}

#[derive(Debug, Clone)]
pub struct PhaseState {
    surface_format: TextureFormat,
    has_surface: bool,
    slots: ResourceSlotMap,
    scene_gbuffer: SceneGBufferSlots,
    scene_shadows: Option<SceneShadowResources>,
}

impl PhaseState {
    #[inline]
    pub fn new(surface_format: TextureFormat, has_surface: bool) -> Self {
        Self {
            surface_format,
            has_surface,
            slots: ResourceSlotMap::default(),
            scene_gbuffer: SceneGBufferSlots::default(),
            scene_shadows: None,
        }
    }

    #[inline]
    pub fn with_slots(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: ResourceSlotMap,
        scene_gbuffer: SceneGBufferSlots,
        scene_shadows: Option<SceneShadowResources>,
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
            scene_gbuffer,
            scene_shadows,
        }
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.surface_format
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.has_surface
    }

    #[inline]
    pub fn slots(&self) -> &ResourceSlotMap {
        &self.slots
    }

    #[inline]
    pub fn slots_mut(&mut self) -> &mut ResourceSlotMap {
        &mut self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        &self.scene_gbuffer
    }

    #[inline]
    pub fn scene_gbuffer_mut(&mut self) -> &mut SceneGBufferSlots {
        &mut self.scene_gbuffer
    }

    #[inline]
    pub fn scene_shadows(&self) -> Option<&SceneShadowResources> {
        self.scene_shadows.as_ref()
    }

    #[inline]
    pub fn set_scene_shadows(
        &mut self,
        resources: SceneShadowResources,
    ) -> Option<SceneShadowResources> {
        self.scene_shadows.replace(resources)
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.slots.current_color()
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.slots.set_current_color(handle, format);
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.scene_gbuffer.get(texture)
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }

    #[inline]
    pub fn scene_light(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.light()
    }

    #[inline]
    pub fn scene_indirect_diffuse(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.indirect_diffuse()
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set(texture, slot)
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_color(slot);
    }

    #[inline]
    pub fn set_scene_depth(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_depth(slot);
    }

    #[inline]
    pub fn set_scene_normal(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_normal(slot);
    }

    #[inline]
    pub fn set_scene_velocity(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_velocity(slot);
    }

    #[inline]
    pub fn set_scene_albedo(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_albedo(slot);
    }

    #[inline]
    pub fn set_scene_material(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_material(slot);
    }

    #[inline]
    pub fn set_scene_emissive(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_emissive(slot);
    }

    #[inline]
    pub fn set_scene_light(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_light(slot);
    }

    #[inline]
    pub fn set_scene_indirect_diffuse(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_indirect_diffuse(slot);
    }

    #[inline]
    pub fn texture_slot(&self, name: &str) -> Option<TextureSlot> {
        self.slots.texture(name)
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots.insert_texture(name, handle, format)
    }

    #[inline]
    pub fn into_slots(self) -> ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn into_parts(
        self,
    ) -> (
        ResourceSlotMap,
        SceneGBufferSlots,
        Option<SceneShadowResources>,
    ) {
        (self.slots, self.scene_gbuffer, self.scene_shadows)
    }
}

#[derive(Debug, Clone)]
pub struct CompletedViewState {
    view_index: usize,
    order: i32,
    viewport: ViewportRect,
    target_size: [u32; 2],
    clear_surface: bool,
    slots: ResourceSlotMap,
    scene_gbuffer: SceneGBufferSlots,
    scene_shadows: Option<SceneShadowResources>,
}

impl CompletedViewState {
    pub(crate) fn new(
        view_index: usize,
        view: &PreparedView<'_>,
        slots: ResourceSlotMap,
        scene_gbuffer: SceneGBufferSlots,
        scene_shadows: Option<SceneShadowResources>,
    ) -> Self {
        Self {
            view_index,
            order: view.order(),
            viewport: view.viewport(),
            target_size: view.target_size(),
            clear_surface: view.clear_surface(),
            slots,
            scene_gbuffer,
            scene_shadows,
        }
    }

    #[inline]
    pub fn view_index(&self) -> usize {
        self.view_index
    }

    #[inline]
    pub fn order(&self) -> i32 {
        self.order
    }

    #[inline]
    pub fn viewport(&self) -> ViewportRect {
        self.viewport
    }

    #[inline]
    pub fn target_size(&self) -> [u32; 2] {
        self.target_size
    }

    #[inline]
    pub fn clear_surface(&self) -> bool {
        self.clear_surface
    }

    #[inline]
    pub fn slots(&self) -> &ResourceSlotMap {
        &self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        &self.scene_gbuffer
    }

    #[inline]
    pub fn scene_shadows(&self) -> Option<&SceneShadowResources> {
        self.scene_shadows.as_ref()
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.scene_gbuffer.get(texture)
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }

    #[inline]
    pub fn scene_light(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.light()
    }

    #[inline]
    pub fn scene_indirect_diffuse(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.indirect_diffuse()
    }
}

pub struct FinalizePhaseState<'a> {
    surface_format: TextureFormat,
    has_surface: bool,
    slots: &'a mut ResourceSlotMap,
    scene_gbuffer: &'a mut SceneGBufferSlots,
    completed_views: &'a [CompletedViewState],
}

impl<'a> FinalizePhaseState<'a> {
    pub(crate) fn new(
        surface_format: TextureFormat,
        has_surface: bool,
        slots: &'a mut ResourceSlotMap,
        scene_gbuffer: &'a mut SceneGBufferSlots,
        completed_views: &'a [CompletedViewState],
    ) -> Self {
        Self {
            surface_format,
            has_surface,
            slots,
            scene_gbuffer,
            completed_views,
        }
    }

    #[inline]
    pub fn surface_format(&self) -> TextureFormat {
        self.surface_format
    }

    #[inline]
    pub fn has_surface(&self) -> bool {
        self.has_surface
    }

    #[inline]
    pub fn slots(&self) -> &ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn slots_mut(&mut self) -> &mut ResourceSlotMap {
        self.slots
    }

    #[inline]
    pub fn scene_gbuffer(&self) -> &SceneGBufferSlots {
        self.scene_gbuffer
    }

    #[inline]
    pub fn scene_gbuffer_mut(&mut self) -> &mut SceneGBufferSlots {
        self.scene_gbuffer
    }

    #[inline]
    pub fn completed_views(&self) -> &'a [CompletedViewState] {
        self.completed_views
    }

    #[inline]
    pub fn current_color(&self) -> Option<TextureSlot> {
        self.slots.current_color()
    }

    #[inline]
    pub fn scene_color(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.color()
    }

    #[inline]
    pub fn scene_texture(&self, texture: SceneTexture) -> Option<TextureSlot> {
        self.scene_gbuffer.get(texture)
    }

    #[inline]
    pub fn scene_depth(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.depth()
    }

    #[inline]
    pub fn scene_normal(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.normal()
    }

    #[inline]
    pub fn scene_velocity(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.velocity()
    }

    #[inline]
    pub fn scene_albedo(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.albedo()
    }

    #[inline]
    pub fn scene_material(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.material()
    }

    #[inline]
    pub fn scene_emissive(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.emissive()
    }

    #[inline]
    pub fn scene_light(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.light()
    }

    #[inline]
    pub fn scene_indirect_diffuse(&self) -> Option<TextureSlot> {
        self.scene_gbuffer.indirect_diffuse()
    }

    #[inline]
    pub fn set_current_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        self.slots.set_current_color(handle, format);
    }

    #[inline]
    pub fn set_scene_color(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_color(slot);
    }

    #[inline]
    pub fn set_scene_texture(
        &mut self,
        texture: SceneTexture,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set(texture, slot)
    }

    #[inline]
    pub fn set_scene_depth(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_depth(slot);
    }

    #[inline]
    pub fn set_scene_normal(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_normal(slot);
    }

    #[inline]
    pub fn set_scene_velocity(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_velocity(slot);
    }

    #[inline]
    pub fn set_scene_albedo(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_albedo(slot);
    }

    #[inline]
    pub fn set_scene_material(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_material(slot);
    }

    #[inline]
    pub fn set_scene_emissive(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_emissive(slot);
    }

    #[inline]
    pub fn set_scene_light(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_light(slot);
    }

    #[inline]
    pub fn set_scene_indirect_diffuse(&mut self, handle: TextureHandle, format: TextureFormat) {
        let slot = TextureSlot::new(handle, format);
        self.scene_gbuffer.set_indirect_diffuse(slot);
    }

    #[inline]
    pub fn set_texture_slot(
        &mut self,
        name: impl Into<Cow<'static, str>>,
        handle: TextureHandle,
        format: TextureFormat,
    ) -> Option<TextureSlot> {
        self.slots.insert_texture(name, handle, format)
    }
}

#[cfg(test)]
mod inline_slot_tests {
    use super::*;

    #[test]
    fn resource_slots_preserve_replacement_and_overflow_semantics() {
        let mut slots = ResourceSlotMap::default();
        for index in 0..INLINE_RESOURCE_SLOT_CAPACITY {
            let handle = TextureHandle(index, 1);
            assert!(slots
                .insert_texture(format!("slot_{index}"), handle, TextureFormat::Rgba8Unorm,)
                .is_none());
        }

        let overflow = TextureHandle(99, 1);
        assert!(slots
            .insert_texture("overflow", overflow, TextureFormat::Rgba16Float)
            .is_none());
        assert_eq!(
            slots.texture("overflow").map(TextureSlot::handle),
            Some(overflow)
        );

        let replacement = TextureHandle(100, 1);
        let previous = slots.insert_texture("slot_0", replacement, TextureFormat::Bgra8Unorm);
        assert_eq!(previous.map(TextureSlot::handle), Some(TextureHandle(0, 1)));
        assert_eq!(
            slots.texture("slot_0").map(TextureSlot::handle),
            Some(replacement)
        );
    }
}
