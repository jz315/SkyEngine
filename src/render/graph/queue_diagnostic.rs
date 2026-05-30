//! Dry-run queue scheduling diagnostics.

use std::borrow::Cow;

use super::*;

/// Dry-run queue assignment report.
///
/// This does not alter render graph execution. It explains which passes would
/// be candidates for non-graphics queues if a future backend provided real
/// multi-queue scheduling.
#[derive(Debug, Clone)]
pub struct QueueScheduleDiagnostic {
    pub pass_assignments: Vec<QueueAssignmentDiagnostic>,
    pub async_compute_candidates: usize,
    pub copy_queue_candidates: usize,
    pub blockers: Vec<QueueScheduleBlocker>,
}

/// One pass's dry-run queue classification.
#[derive(Debug, Clone)]
pub struct QueueAssignmentDiagnostic {
    pub pass: PassHandle,
    pub pass_index: usize,
    pub name: Cow<'static, str>,
    pub pass_type: PassType,
    pub flags: PassFlags,
    pub dep_level: u32,
    pub execution_order: Option<usize>,
    pub class: QueueDiagnosticClass,
    pub shared_with_graphics: Vec<ResourceRef>,
    pub reasons: Vec<QueueScheduleReason>,
}

/// Queue class assigned by the dry-run diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueDiagnosticClass {
    Graphics,
    ComputeCandidate,
    CopyCandidate,
    GraphicsRequired,
}

/// Human-readable reason represented as data for tests and tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueScheduleReason {
    RenderPassUsesGraphics,
    SurfaceInteraction,
    PreferAsyncCompute,
    ComputePassWithoutAsyncPreference,
    CopyPass,
    ImportedResource(ResourceRef),
    PersistentResource(ResourceRef),
    SharesResourceWithGraphics(ResourceRef),
}

/// Blocking condition that prevents or constrains non-graphics scheduling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueScheduleBlocker {
    SurfaceInteraction {
        pass: PassHandle,
    },
    RenderPass {
        pass: PassHandle,
    },
    MissingAsyncComputePreference {
        pass: PassHandle,
    },
    ImportedResource {
        pass: PassHandle,
        resource: ResourceRef,
    },
    PersistentResource {
        pass: PassHandle,
        resource: ResourceRef,
    },
}

impl RenderGraph {
    /// Run a read-only queue scheduling diagnostic without changing execution.
    ///
    /// Call [`compile`](Self::compile) first if dependency levels and execution
    /// positions should reflect the current graph compilation.
    #[must_use]
    pub fn queue_schedule_diagnostic(&self) -> QueueScheduleDiagnostic {
        let mut execution_positions = FxHashMap::default();
        for (position, &pass_index) in self.order.iter().enumerate() {
            execution_positions.insert(pass_index, position);
        }

        let graphics_resources = self.graphics_resource_set();
        let mut pass_assignments = Vec::with_capacity(self.passes.len());
        let mut blockers = Vec::new();
        let mut async_compute_candidates = 0usize;
        let mut copy_queue_candidates = 0usize;

        for (idx, pass) in self.passes.iter().enumerate() {
            let access = pass.access_info(idx);
            let pass_handle = PassHandle(idx, self.handle_token);
            let pass_resources = pass_resource_set(access.reads, access.writes);
            let touches_surface = pass_resources.contains(&ResourceRef::Surface);
            let mut reasons = Vec::new();

            let class = if touches_surface {
                reasons.push(QueueScheduleReason::SurfaceInteraction);
                blockers.push(QueueScheduleBlocker::SurfaceInteraction { pass: pass_handle });
                QueueDiagnosticClass::GraphicsRequired
            } else {
                match access.pass_type {
                    PassType::Render => {
                        reasons.push(QueueScheduleReason::RenderPassUsesGraphics);
                        blockers.push(QueueScheduleBlocker::RenderPass { pass: pass_handle });
                        QueueDiagnosticClass::Graphics
                    }
                    PassType::Compute if access.flags.contains(PassFlags::PREFER_ASYNC_COMPUTE) => {
                        reasons.push(QueueScheduleReason::PreferAsyncCompute);
                        async_compute_candidates += 1;
                        QueueDiagnosticClass::ComputeCandidate
                    }
                    PassType::Compute => {
                        reasons.push(QueueScheduleReason::ComputePassWithoutAsyncPreference);
                        blockers.push(QueueScheduleBlocker::MissingAsyncComputePreference {
                            pass: pass_handle,
                        });
                        QueueDiagnosticClass::GraphicsRequired
                    }
                    PassType::Copy => {
                        reasons.push(QueueScheduleReason::CopyPass);
                        copy_queue_candidates += 1;
                        QueueDiagnosticClass::CopyCandidate
                    }
                }
            };

            for &resource in &pass_resources {
                if let Some(reason) = self.external_resource_reason(resource) {
                    reasons.push(reason.clone());
                    match reason {
                        QueueScheduleReason::ImportedResource(resource) => {
                            blockers.push(QueueScheduleBlocker::ImportedResource {
                                pass: pass_handle,
                                resource,
                            });
                        }
                        QueueScheduleReason::PersistentResource(resource) => {
                            blockers.push(QueueScheduleBlocker::PersistentResource {
                                pass: pass_handle,
                                resource,
                            });
                        }
                        _ => {}
                    }
                }
            }

            let shared_with_graphics = overlapping_resources(&pass_resources, &graphics_resources);
            for &resource in &shared_with_graphics {
                reasons.push(QueueScheduleReason::SharesResourceWithGraphics(resource));
            }

            pass_assignments.push(QueueAssignmentDiagnostic {
                pass: pass_handle,
                pass_index: idx,
                name: access.name.clone(),
                pass_type: access.pass_type,
                flags: access.flags,
                dep_level: pass.dep_level,
                execution_order: execution_positions.get(&idx).copied(),
                class,
                shared_with_graphics,
                reasons,
            });
        }

        QueueScheduleDiagnostic {
            pass_assignments,
            async_compute_candidates,
            copy_queue_candidates,
            blockers,
        }
    }

    fn graphics_resource_set(&self) -> Vec<ResourceRef> {
        let mut resources = Vec::new();
        for (idx, pass) in self.passes.iter().enumerate() {
            let access = pass.access_info(idx);
            let pass_resources = pass_resource_set(access.reads, access.writes);
            let touches_surface = pass_resources.contains(&ResourceRef::Surface);
            if access.pass_type != PassType::Render && !touches_surface {
                continue;
            }
            extend_resource_set(&mut resources, &pass_resources);
        }
        resources
    }

    fn external_resource_reason(&self, resource: ResourceRef) -> Option<QueueScheduleReason> {
        match resource {
            ResourceRef::Surface => None,
            ResourceRef::Texture(handle) => {
                let desc = self.textures.get(handle.0)?;
                if desc.imported.is_some() {
                    Some(QueueScheduleReason::ImportedResource(resource))
                } else if !desc.transient {
                    Some(QueueScheduleReason::PersistentResource(resource))
                } else {
                    None
                }
            }
            ResourceRef::TextureSubresource(subresource) => {
                let desc = self.textures.get(subresource.texture.0)?;
                if desc.imported.is_some() {
                    Some(QueueScheduleReason::ImportedResource(resource))
                } else if !desc.transient {
                    Some(QueueScheduleReason::PersistentResource(resource))
                } else {
                    None
                }
            }
            ResourceRef::Buffer(handle) => {
                let desc = self.buffers.get(handle.0)?;
                if desc.imported.is_some() {
                    Some(QueueScheduleReason::ImportedResource(resource))
                } else if !desc.transient {
                    Some(QueueScheduleReason::PersistentResource(resource))
                } else {
                    None
                }
            }
        }
    }
}

fn pass_resource_set(reads: &[ResourceRef], writes: &[ResourceRef]) -> Vec<ResourceRef> {
    let mut resources = Vec::new();
    extend_resource_set(&mut resources, reads);
    extend_resource_set(&mut resources, writes);
    resources
}

fn extend_resource_set(resources: &mut Vec<ResourceRef>, new_resources: &[ResourceRef]) {
    for &resource in new_resources {
        if !resources.contains(&resource) {
            resources.push(resource);
        }
    }
}

fn overlapping_resources(
    resources: &[ResourceRef],
    candidates: &[ResourceRef],
) -> Vec<ResourceRef> {
    let mut shared = Vec::new();
    for &resource in resources {
        if candidates
            .iter()
            .copied()
            .any(|candidate| resource_refs_overlap(resource, candidate))
            && !shared.contains(&resource)
        {
            shared.push(resource);
        }
    }
    shared
}
