use std::borrow::Cow;

use rustc_hash::FxHashMap;

use crate::render::domains::{RenderDomain, SpriteDomain};
use crate::render::ecs::RenderSettings;
use crate::render::scene::{
    RenderInjectionPoint, RenderQueueDesc, RenderQueueSort, RenderStageKey, SCENE_HDR_FORMAT,
};

use super::{RenderFeature, SharedRenderFeature};

#[derive(Debug, Clone)]
pub struct OutputChainConfig {
    postfx_after_stage: Option<RenderStageKey>,
    pub(crate) bloom: bool,
    pub(crate) vignette: bool,
    pub(crate) tonemap: bool,
    pub(crate) color_resolve: bool,
}

impl OutputChainConfig {
    #[inline]
    pub fn none() -> Self {
        Self {
            postfx_after_stage: None,
            bloom: false,
            vignette: false,
            tonemap: false,
            color_resolve: false,
        }
    }

    #[inline]
    pub fn after_stage(stage: impl Into<RenderStageKey>) -> Self {
        Self {
            postfx_after_stage: Some(stage.into()),
            bloom: true,
            vignette: true,
            tonemap: true,
            color_resolve: true,
        }
    }

    #[inline]
    pub fn postfx_after_stage(&self) -> Option<&RenderStageKey> {
        self.postfx_after_stage.as_ref()
    }

    #[inline]
    pub fn bloom(mut self, enabled: bool) -> Self {
        self.bloom = enabled;
        self
    }

    #[inline]
    pub fn vignette(mut self, enabled: bool) -> Self {
        self.vignette = enabled;
        self
    }

    #[inline]
    pub fn tonemap(mut self, enabled: bool) -> Self {
        self.tonemap = enabled;
        self
    }

    #[inline]
    pub fn color_resolve(mut self, enabled: bool) -> Self {
        self.color_resolve = enabled;
        self
    }

    #[inline]
    pub(crate) fn has_enabled_nodes(&self) -> bool {
        self.bloom || self.vignette || self.tonemap || self.color_resolve
    }

    #[inline]
    pub(crate) fn output_format_after_settings(
        &self,
        settings: RenderSettings,
        mut current: wgpu::TextureFormat,
        surface: wgpu::TextureFormat,
    ) -> wgpu::TextureFormat {
        let bloom_active = self.bloom && settings.bloom.enabled;
        let vignette_active = self.vignette && settings.vignette.enabled;
        let tonemap_active = self.tonemap && settings.tonemap.enabled;
        let color_resolve_active = self.color_resolve && !tonemap_active;

        if bloom_active || vignette_active {
            current = SCENE_HDR_FORMAT;
        }
        if tonemap_active || color_resolve_active {
            current = surface;
        }

        current
    }
}

pub(crate) struct DomainEntry {
    pub(crate) queue: Cow<'static, str>,
    pub(crate) sort_policy: RenderQueueSort,
    pub(crate) domain: Box<dyn RenderDomain>,
}

pub(crate) struct FeatureEntry {
    pub(crate) injection_point: RenderInjectionPoint,
    pub(crate) feature: SharedRenderFeature,
}

pub(crate) struct CompiledRenderPipeline {
    pub(crate) stages: Vec<RenderStageKey>,
    pub(crate) queues: Vec<RenderQueueDesc>,
    pub(crate) queues_by_stage: Vec<Vec<usize>>,
    pub(crate) domains_by_queue: Vec<Vec<usize>>,
    pub(crate) before_stage: FxHashMap<String, Vec<usize>>,
    pub(crate) after_stage: FxHashMap<String, Vec<usize>>,
    pub(crate) before_queue: FxHashMap<String, Vec<usize>>,
    pub(crate) after_queue: FxHashMap<String, Vec<usize>>,
    pub(crate) before_present: Vec<usize>,
    pub(crate) output_chain: OutputChainConfig,
}

pub struct RenderPipelineBuilder {
    stages: Vec<RenderStageKey>,
    queues: Vec<RenderQueueDesc>,
    domains: Vec<DomainEntry>,
    features: Vec<FeatureEntry>,
    output_chain: OutputChainConfig,
}

impl RenderPipelineBuilder {
    pub fn new() -> Self {
        Self {
            stages: Vec::new(),
            queues: Vec::new(),
            domains: Vec::new(),
            features: Vec::new(),
            output_chain: OutputChainConfig::none(),
        }
    }

    pub fn add_stage(mut self, stage: impl Into<RenderStageKey>) -> Self {
        self.stages.push(stage.into());
        self
    }

    pub fn add_queue(mut self, queue: RenderQueueDesc) -> Self {
        self.queues.push(queue);
        self
    }

    pub fn add_domain<D>(mut self, domain: D, queue: impl Into<Cow<'static, str>>) -> Self
    where
        D: RenderDomain + 'static,
    {
        self.domains.push(DomainEntry {
            queue: queue.into(),
            sort_policy: RenderQueueSort::TransparentScene,
            domain: Box::new(domain),
        });
        self
    }

    pub fn add_boxed_domain(
        mut self,
        domain: Box<dyn RenderDomain>,
        queue: impl Into<Cow<'static, str>>,
    ) -> Self {
        self.domains.push(DomainEntry {
            queue: queue.into(),
            sort_policy: RenderQueueSort::TransparentScene,
            domain,
        });
        self
    }

    pub fn add_feature<F>(mut self, feature: F, injection_point: RenderInjectionPoint) -> Self
    where
        F: RenderFeature + 'static,
    {
        self.features.push(FeatureEntry {
            injection_point,
            feature: SharedRenderFeature::new(Box::new(feature)),
        });
        self
    }

    pub fn add_boxed_feature(
        mut self,
        feature: Box<dyn RenderFeature>,
        injection_point: RenderInjectionPoint,
    ) -> Self {
        self.features.push(FeatureEntry {
            injection_point,
            feature: SharedRenderFeature::new(feature),
        });
        self
    }

    pub fn output_chain(mut self, output_chain: OutputChainConfig) -> Self {
        self.output_chain = output_chain;
        self
    }

    pub fn build(self) -> RenderPipelineAsset {
        RenderPipelineAsset::from_builder(self)
    }
}

impl Default for RenderPipelineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

pub struct RenderPipelineAsset {
    pub(crate) compiled: CompiledRenderPipeline,
    pub(crate) domains: Vec<DomainEntry>,
    pub(crate) features: Vec<FeatureEntry>,
}

impl RenderPipelineAsset {
    #[inline]
    pub fn builder() -> RenderPipelineBuilder {
        RenderPipelineBuilder::new()
    }

    pub fn universal_2d() -> Self {
        Self::builder()
            .add_stage("Opaque")
            .add_stage("Transparent")
            .add_stage("Overlay")
            .add_queue(RenderQueueDesc::new(
                "transparent",
                "Transparent",
                RenderQueueSort::TransparentScene,
            ))
            .add_queue(RenderQueueDesc::new(
                "overlay",
                "Overlay",
                RenderQueueSort::OverlayStable,
            ))
            .add_domain(SpriteDomain::lit_hdr(), "transparent")
            .output_chain(OutputChainConfig::after_stage("Transparent"))
            .build()
    }

    pub fn universal_unlit() -> Self {
        Self::builder()
            .add_stage("Opaque")
            .add_stage("Transparent")
            .add_stage("Overlay")
            .add_queue(RenderQueueDesc::new(
                "transparent",
                "Transparent",
                RenderQueueSort::TransparentScene,
            ))
            .add_queue(RenderQueueDesc::new(
                "overlay",
                "Overlay",
                RenderQueueSort::OverlayStable,
            ))
            .add_domain(SpriteDomain::unlit(), "transparent")
            .output_chain(OutputChainConfig::none())
            .build()
    }

    pub fn overlay() -> Self {
        Self::builder()
            .add_stage("Overlay")
            .add_queue(RenderQueueDesc::new(
                "overlay",
                "Overlay",
                RenderQueueSort::OverlayStable,
            ))
            .output_chain(OutputChainConfig::none())
            .build()
    }

    fn from_builder(builder: RenderPipelineBuilder) -> Self {
        let mut queue_sort_lookup = FxHashMap::<String, RenderQueueSort>::default();
        for queue in &builder.queues {
            queue_sort_lookup.insert(queue.name().to_string(), queue.sort_policy());
        }
        let mut domains = builder.domains;
        for domain in &mut domains {
            domain.sort_policy = *queue_sort_lookup
                .get(domain.queue.as_ref())
                .unwrap_or(&RenderQueueSort::TransparentScene);
        }
        let compiled = CompiledRenderPipeline::compile(
            &builder.stages,
            &builder.queues,
            &domains,
            &builder.features,
            builder.output_chain,
        );
        Self {
            compiled,
            domains,
            features: builder.features,
        }
    }
}

impl CompiledRenderPipeline {
    fn compile(
        stages: &[RenderStageKey],
        queues: &[RenderQueueDesc],
        domains: &[DomainEntry],
        features: &[FeatureEntry],
        output_chain: OutputChainConfig,
    ) -> Self {
        let mut stage_lookup = FxHashMap::<String, usize>::default();
        for (index, stage) in stages.iter().enumerate() {
            let name = stage.as_str().to_string();
            assert!(
                stage_lookup.insert(name.clone(), index).is_none(),
                "duplicate render stage `{name}`"
            );
        }

        let mut queue_lookup = FxHashMap::<String, usize>::default();
        let mut queues_by_stage = vec![Vec::new(); stages.len()];
        for (index, queue) in queues.iter().enumerate() {
            let queue_name = queue.name().to_string();
            assert!(
                queue_lookup.insert(queue_name.clone(), index).is_none(),
                "duplicate render queue `{queue_name}`"
            );
            let stage_index = *stage_lookup.get(queue.stage().as_str()).unwrap_or_else(|| {
                panic!(
                    "queue `{queue_name}` references unknown stage `{}`",
                    queue.stage()
                )
            });
            queues_by_stage[stage_index].push(index);
        }

        let mut domains_by_queue = vec![Vec::new(); queues.len()];
        for (index, domain) in domains.iter().enumerate() {
            let queue_index = *queue_lookup.get(domain.queue.as_ref()).unwrap_or_else(|| {
                panic!(
                    "domain `{}` references unknown queue `{}`",
                    domain.domain.name(),
                    domain.queue
                )
            });
            domains_by_queue[queue_index].push(index);
        }

        let mut before_stage = FxHashMap::<String, Vec<usize>>::default();
        let mut after_stage = FxHashMap::<String, Vec<usize>>::default();
        let mut before_queue = FxHashMap::<String, Vec<usize>>::default();
        let mut after_queue = FxHashMap::<String, Vec<usize>>::default();
        let mut before_present = Vec::new();
        for (index, feature) in features.iter().enumerate() {
            let feature_name = feature.feature.name();
            match &feature.injection_point {
                RenderInjectionPoint::BeforeStage(stage) => {
                    assert!(
                        stage_lookup.contains_key(stage.as_str()),
                        "feature `{}` references unknown stage `{stage}`",
                        feature_name
                    );
                    before_stage
                        .entry(stage.as_str().to_string())
                        .or_default()
                        .push(index);
                }
                RenderInjectionPoint::AfterStage(stage) => {
                    assert!(
                        stage_lookup.contains_key(stage.as_str()),
                        "feature `{}` references unknown stage `{stage}`",
                        feature_name
                    );
                    after_stage
                        .entry(stage.as_str().to_string())
                        .or_default()
                        .push(index);
                }
                RenderInjectionPoint::BeforeQueue(queue) => {
                    assert!(
                        queue_lookup.contains_key(queue.as_ref()),
                        "feature `{}` references unknown queue `{queue}`",
                        feature_name
                    );
                    before_queue
                        .entry(queue.to_string())
                        .or_default()
                        .push(index);
                }
                RenderInjectionPoint::AfterQueue(queue) => {
                    assert!(
                        queue_lookup.contains_key(queue.as_ref()),
                        "feature `{}` references unknown queue `{queue}`",
                        feature_name
                    );
                    after_queue
                        .entry(queue.to_string())
                        .or_default()
                        .push(index);
                }
                RenderInjectionPoint::BeforePresent => before_present.push(index),
            }
        }

        if let Some(stage) = output_chain.postfx_after_stage() {
            assert!(
                stage_lookup.contains_key(stage.as_str()),
                "output chain references unknown stage `{stage}`"
            );
        }

        Self {
            stages: stages.to_vec(),
            queues: queues.to_vec(),
            queues_by_stage,
            domains_by_queue,
            before_stage,
            after_stage,
            before_queue,
            after_queue,
            before_present,
            output_chain,
        }
    }
}
