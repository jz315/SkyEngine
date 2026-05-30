use crate::diagnostics::{DiagnosticEvent, DiagnosticSeverity, Diagnostics};
use crate::ecs::World;
use crate::render::RenderStats;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RenderDiagnosticsState {
    last_snapshot: Option<RenderDiagnosticsSnapshot>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderDiagnosticsSnapshot {
    step_count: usize,
    view_count: usize,
    draw_calls: usize,
    passes: usize,
    resident_render_assets: usize,
    resident_render_asset_bytes: usize,
    uploaded_render_assets: usize,
    uploaded_render_asset_bytes: usize,
    evicted_render_assets: usize,
    evicted_render_asset_bytes: usize,
    cached_failed_render_assets: usize,
    queued_render_assets: usize,
    visible_queued_render_assets: usize,
    loading_render_assets: usize,
    fallback_render_assets: usize,
    missing_render_assets: usize,
    failed_render_assets: usize,
}

impl From<RenderStats> for RenderDiagnosticsSnapshot {
    fn from(stats: RenderStats) -> Self {
        Self {
            step_count: stats.step_count,
            view_count: stats.view_count,
            draw_calls: stats.draw_calls,
            passes: stats.passes,
            resident_render_assets: stats.resident_render_assets,
            resident_render_asset_bytes: stats.resident_render_asset_bytes,
            uploaded_render_assets: stats.uploaded_render_assets,
            uploaded_render_asset_bytes: stats.uploaded_render_asset_bytes,
            evicted_render_assets: stats.evicted_render_assets,
            evicted_render_asset_bytes: stats.evicted_render_asset_bytes,
            cached_failed_render_assets: stats.cached_failed_render_assets,
            queued_render_assets: stats.queued_render_assets,
            visible_queued_render_assets: stats.visible_queued_render_assets,
            loading_render_assets: stats.loading_render_assets,
            fallback_render_assets: stats.fallback_render_assets,
            missing_render_assets: stats.missing_render_assets,
            failed_render_assets: stats.failed_render_assets,
        }
    }
}

pub(crate) fn publish_render_diagnostics(world: &mut World, stats: RenderStats) {
    let diagnostics = ensure_diagnostics(world);
    let frame = Some(world.time.frame_count);
    let mut state = world
        .remove_resource::<RenderDiagnosticsState>()
        .unwrap_or_default();
    state.publish_stats(&diagnostics, frame, stats.into());
    world.insert_resource(state);
}

fn ensure_diagnostics(world: &mut World) -> Diagnostics {
    if let Some(diagnostics) = world.get_resource::<Diagnostics>() {
        return diagnostics.clone();
    }
    let diagnostics = Diagnostics::new();
    world.insert_resource(diagnostics.clone());
    diagnostics
}

impl RenderDiagnosticsState {
    fn publish_stats(
        &mut self,
        diagnostics: &Diagnostics,
        frame: Option<u64>,
        snapshot: RenderDiagnosticsSnapshot,
    ) {
        if self.last_snapshot == Some(snapshot) {
            return;
        }

        let previous_uploaded = self
            .last_snapshot
            .map(|snapshot| snapshot.uploaded_render_assets)
            .unwrap_or(0);
        if snapshot.uploaded_render_assets > 0 && previous_uploaded == 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "render",
                    "render.asset.uploaded",
                    DiagnosticSeverity::Info,
                    "Render asset residency cache uploaded backend-owned assets",
                )
                .with_frame(frame)
                .with_field("uploaded_render_assets", snapshot.uploaded_render_assets)
                .with_field(
                    "uploaded_render_asset_bytes",
                    snapshot.uploaded_render_asset_bytes,
                )
                .with_field("resident_render_assets", snapshot.resident_render_assets)
                .with_field(
                    "resident_render_asset_bytes",
                    snapshot.resident_render_asset_bytes,
                )
                .with_field("queued_render_assets", snapshot.queued_render_assets),
            );
        }

        let previous_evicted = self
            .last_snapshot
            .map(|snapshot| snapshot.evicted_render_assets)
            .unwrap_or(0);
        if snapshot.evicted_render_assets > 0 && previous_evicted == 0 {
            diagnostics.push(
                DiagnosticEvent::new(
                    "render",
                    "render.asset.evicted",
                    DiagnosticSeverity::Warning,
                    "Render asset residency cache evicted backend-owned assets",
                )
                .with_frame(frame)
                .with_field("evicted_render_assets", snapshot.evicted_render_assets)
                .with_field(
                    "evicted_render_asset_bytes",
                    snapshot.evicted_render_asset_bytes,
                )
                .with_field("resident_render_assets", snapshot.resident_render_assets)
                .with_field(
                    "resident_render_asset_bytes",
                    snapshot.resident_render_asset_bytes,
                )
                .with_field("queued_render_assets", snapshot.queued_render_assets),
            );
        }

        let previous_failed = self
            .last_snapshot
            .map(|snapshot| snapshot.failed_render_assets)
            .unwrap_or(0);
        if snapshot.failed_render_assets > 0 && previous_failed != snapshot.failed_render_assets {
            diagnostics.push(
                DiagnosticEvent::new(
                    "render",
                    "render.asset.failed",
                    DiagnosticSeverity::Warning,
                    "Render asset residency failures are present",
                )
                .with_frame(frame)
                .with_field(
                    "failed_render_asset_delta",
                    snapshot
                        .failed_render_assets
                        .saturating_sub(previous_failed),
                )
                .with_field("previous_failed_render_assets", previous_failed)
                .with_field("failed_render_assets", snapshot.failed_render_assets)
                .with_field(
                    "cached_failed_render_assets",
                    snapshot.cached_failed_render_assets,
                )
                .with_field("missing_render_assets", snapshot.missing_render_assets)
                .with_field("fallback_render_assets", snapshot.fallback_render_assets)
                .with_field("loading_render_assets", snapshot.loading_render_assets)
                .with_field("queued_render_assets", snapshot.queued_render_assets)
                .with_field(
                    "visible_queued_render_assets",
                    snapshot.visible_queued_render_assets,
                ),
            );
        }

        let previous_missing = self
            .last_snapshot
            .map(|snapshot| snapshot.missing_render_assets)
            .unwrap_or(0);
        if snapshot.missing_render_assets > 0 && previous_missing != snapshot.missing_render_assets
        {
            diagnostics.push(
                DiagnosticEvent::new(
                    "render",
                    "render.asset.missing",
                    DiagnosticSeverity::Warning,
                    "Render asset residency cache has missing backend-owned assets",
                )
                .with_frame(frame)
                .with_field("missing_render_assets", snapshot.missing_render_assets)
                .with_field("fallback_render_assets", snapshot.fallback_render_assets)
                .with_field("loading_render_assets", snapshot.loading_render_assets)
                .with_field("queued_render_assets", snapshot.queued_render_assets)
                .with_field(
                    "visible_queued_render_assets",
                    snapshot.visible_queued_render_assets,
                )
                .with_field("failed_render_assets", snapshot.failed_render_assets)
                .with_field(
                    "cached_failed_render_assets",
                    snapshot.cached_failed_render_assets,
                ),
            );
        }

        let previous_fallback = self
            .last_snapshot
            .map(|snapshot| snapshot.fallback_render_assets)
            .unwrap_or(0);
        if snapshot.fallback_render_assets > 0
            && previous_fallback != snapshot.fallback_render_assets
        {
            diagnostics.push(
                DiagnosticEvent::new(
                    "render",
                    "render.asset.fallback",
                    DiagnosticSeverity::Info,
                    "Render asset residency cache is using fallback backend-owned assets",
                )
                .with_frame(frame)
                .with_field("fallback_render_assets", snapshot.fallback_render_assets)
                .with_field("loading_render_assets", snapshot.loading_render_assets)
                .with_field("queued_render_assets", snapshot.queued_render_assets)
                .with_field(
                    "visible_queued_render_assets",
                    snapshot.visible_queued_render_assets,
                )
                .with_field("missing_render_assets", snapshot.missing_render_assets)
                .with_field("failed_render_assets", snapshot.failed_render_assets),
            );
        }

        diagnostics.push(
            DiagnosticEvent::new(
                "render",
                "render.stats",
                DiagnosticSeverity::Info,
                "Render backend stats changed",
            )
            .with_frame(frame)
            .with_field("step_count", snapshot.step_count)
            .with_field("view_count", snapshot.view_count)
            .with_field("draw_calls", snapshot.draw_calls)
            .with_field("passes", snapshot.passes)
            .with_field("resident_render_assets", snapshot.resident_render_assets)
            .with_field(
                "resident_render_asset_bytes",
                snapshot.resident_render_asset_bytes,
            )
            .with_field("uploaded_render_assets", snapshot.uploaded_render_assets)
            .with_field(
                "uploaded_render_asset_bytes",
                snapshot.uploaded_render_asset_bytes,
            )
            .with_field("evicted_render_assets", snapshot.evicted_render_assets)
            .with_field(
                "evicted_render_asset_bytes",
                snapshot.evicted_render_asset_bytes,
            )
            .with_field(
                "cached_failed_render_assets",
                snapshot.cached_failed_render_assets,
            )
            .with_field("queued_render_assets", snapshot.queued_render_assets)
            .with_field(
                "visible_queued_render_assets",
                snapshot.visible_queued_render_assets,
            )
            .with_field("loading_render_assets", snapshot.loading_render_assets)
            .with_field("fallback_render_assets", snapshot.fallback_render_assets)
            .with_field("missing_render_assets", snapshot.missing_render_assets)
            .with_field("failed_render_assets", snapshot.failed_render_assets),
        );
        self.last_snapshot = Some(snapshot);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field<'a>(event: &'a DiagnosticEvent, key: &str) -> Option<&'a str> {
        event
            .fields
            .iter()
            .find(|field| field.key == key)
            .map(|field| field.value.as_str())
    }

    #[test]
    fn publish_render_diagnostics_reports_backend_local_stats() {
        let mut world = World::new();
        let stats = RenderStats {
            step_count: 3,
            view_count: 2,
            draw_calls: 7,
            passes: 4,
            resident_render_assets: 5,
            resident_render_asset_bytes: 2048,
            uploaded_render_assets: 1,
            uploaded_render_asset_bytes: 512,
            evicted_render_assets: 1,
            evicted_render_asset_bytes: 256,
            cached_failed_render_assets: 2,
            queued_render_assets: 6,
            visible_queued_render_assets: 2,
            loading_render_assets: 1,
            fallback_render_assets: 1,
            missing_render_assets: 0,
            failed_render_assets: 0,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, stats);
        publish_render_diagnostics(&mut world, stats);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "render.stats")
            .expect("render stats event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "render.stats")
                .count(),
            1
        );
        assert_eq!(event.category, "render");
        assert_eq!(field(event, "step_count"), Some("3"));
        assert_eq!(field(event, "view_count"), Some("2"));
        assert_eq!(field(event, "draw_calls"), Some("7"));
        assert_eq!(field(event, "resident_render_assets"), Some("5"));
        assert_eq!(field(event, "resident_render_asset_bytes"), Some("2048"));
        assert_eq!(field(event, "evicted_render_assets"), Some("1"));
        assert_eq!(field(event, "evicted_render_asset_bytes"), Some("256"));
        assert_eq!(field(event, "cached_failed_render_assets"), Some("2"));
        assert_eq!(field(event, "queued_render_assets"), Some("6"));
        assert_eq!(field(event, "visible_queued_render_assets"), Some("2"));
        assert_eq!(field(event, "failed_render_assets"), Some("0"));
    }

    #[test]
    fn publish_render_diagnostics_reports_failed_render_assets() {
        let mut world = World::new();
        let stats = RenderStats {
            missing_render_assets: 2,
            failed_render_assets: 1,
            cached_failed_render_assets: 4,
            fallback_render_assets: 5,
            loading_render_assets: 6,
            queued_render_assets: 3,
            visible_queued_render_assets: 2,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, stats);
        publish_render_diagnostics(&mut world, stats);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let event = events
            .iter()
            .find(|event| event.code == "render.asset.failed")
            .expect("render failed-asset event should be emitted");

        assert_eq!(
            events
                .iter()
                .filter(|event| event.code == "render.asset.failed")
                .count(),
            1
        );
        assert_eq!(event.severity, DiagnosticSeverity::Warning);
        assert_eq!(field(event, "failed_render_asset_delta"), Some("1"));
        assert_eq!(field(event, "previous_failed_render_assets"), Some("0"));
        assert_eq!(field(event, "failed_render_assets"), Some("1"));
        assert_eq!(field(event, "cached_failed_render_assets"), Some("4"));
        assert_eq!(field(event, "missing_render_assets"), Some("2"));
        assert_eq!(field(event, "fallback_render_assets"), Some("5"));
        assert_eq!(field(event, "loading_render_assets"), Some("6"));
        assert_eq!(field(event, "queued_render_assets"), Some("3"));
        assert_eq!(field(event, "visible_queued_render_assets"), Some("2"));
    }

    #[test]
    fn publish_render_diagnostics_reports_missing_render_assets() {
        let mut world = World::new();
        let idle = RenderStats {
            queued_render_assets: 1,
            ..Default::default()
        };
        let missing = RenderStats {
            missing_render_assets: 2,
            fallback_render_assets: 1,
            loading_render_assets: 3,
            queued_render_assets: 5,
            visible_queued_render_assets: 4,
            cached_failed_render_assets: 1,
            ..Default::default()
        };
        let more_missing = RenderStats {
            missing_render_assets: 3,
            fallback_render_assets: 2,
            loading_render_assets: 1,
            queued_render_assets: 6,
            visible_queued_render_assets: 2,
            failed_render_assets: 1,
            cached_failed_render_assets: 2,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, missing);
        publish_render_diagnostics(&mut world, missing);
        publish_render_diagnostics(&mut world, more_missing);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let missing_events = events
            .iter()
            .filter(|event| event.code == "render.asset.missing")
            .collect::<Vec<_>>();
        assert_eq!(missing_events.len(), 2);
        assert_eq!(missing_events[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(field(missing_events[0], "missing_render_assets"), Some("2"));
        assert_eq!(
            field(missing_events[0], "fallback_render_assets"),
            Some("1")
        );
        assert_eq!(field(missing_events[0], "loading_render_assets"), Some("3"));
        assert_eq!(
            field(missing_events[0], "visible_queued_render_assets"),
            Some("4")
        );
        assert_eq!(
            field(missing_events[0], "cached_failed_render_assets"),
            Some("1")
        );
        assert_eq!(field(missing_events[1], "missing_render_assets"), Some("3"));
        assert_eq!(field(missing_events[1], "failed_render_assets"), Some("1"));
    }

    #[test]
    fn publish_render_diagnostics_reports_fallback_render_assets() {
        let mut world = World::new();
        let idle = RenderStats {
            queued_render_assets: 1,
            ..Default::default()
        };
        let fallback = RenderStats {
            fallback_render_assets: 2,
            loading_render_assets: 3,
            queued_render_assets: 4,
            visible_queued_render_assets: 1,
            ..Default::default()
        };
        let more_fallback = RenderStats {
            fallback_render_assets: 3,
            loading_render_assets: 1,
            queued_render_assets: 2,
            visible_queued_render_assets: 2,
            missing_render_assets: 1,
            failed_render_assets: 1,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, fallback);
        publish_render_diagnostics(&mut world, fallback);
        publish_render_diagnostics(&mut world, more_fallback);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let fallback_events = events
            .iter()
            .filter(|event| event.code == "render.asset.fallback")
            .collect::<Vec<_>>();
        assert_eq!(fallback_events.len(), 2);
        assert_eq!(fallback_events[0].severity, DiagnosticSeverity::Info);
        assert_eq!(
            field(fallback_events[0], "fallback_render_assets"),
            Some("2")
        );
        assert_eq!(
            field(fallback_events[0], "loading_render_assets"),
            Some("3")
        );
        assert_eq!(field(fallback_events[0], "queued_render_assets"), Some("4"));
        assert_eq!(
            field(fallback_events[0], "visible_queued_render_assets"),
            Some("1")
        );
        assert_eq!(
            field(fallback_events[1], "fallback_render_assets"),
            Some("3")
        );
        assert_eq!(
            field(fallback_events[1], "missing_render_assets"),
            Some("1")
        );
        assert_eq!(field(fallback_events[1], "failed_render_assets"), Some("1"));
    }

    #[test]
    fn publish_render_diagnostics_reports_eviction_bursts() {
        let mut world = World::new();
        let idle = RenderStats {
            resident_render_assets: 4,
            resident_render_asset_bytes: 4096,
            ..Default::default()
        };
        let evicted = RenderStats {
            resident_render_assets: 3,
            resident_render_asset_bytes: 3072,
            evicted_render_assets: 1,
            evicted_render_asset_bytes: 1024,
            queued_render_assets: 2,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, evicted);
        publish_render_diagnostics(&mut world, evicted);
        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, evicted);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let evictions = events
            .iter()
            .filter(|event| event.code == "render.asset.evicted")
            .collect::<Vec<_>>();
        assert_eq!(evictions.len(), 2);
        assert_eq!(evictions[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(field(evictions[0], "evicted_render_assets"), Some("1"));
        assert_eq!(
            field(evictions[0], "evicted_render_asset_bytes"),
            Some("1024")
        );
        assert_eq!(field(evictions[0], "resident_render_assets"), Some("3"));
        assert_eq!(field(evictions[0], "queued_render_assets"), Some("2"));
    }

    #[test]
    fn publish_render_diagnostics_reports_upload_bursts() {
        let mut world = World::new();
        let idle = RenderStats {
            resident_render_assets: 1,
            resident_render_asset_bytes: 512,
            queued_render_assets: 2,
            ..Default::default()
        };
        let uploaded = RenderStats {
            resident_render_assets: 2,
            resident_render_asset_bytes: 1536,
            uploaded_render_assets: 1,
            uploaded_render_asset_bytes: 1024,
            queued_render_assets: 1,
            ..Default::default()
        };

        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, uploaded);
        publish_render_diagnostics(&mut world, uploaded);
        publish_render_diagnostics(&mut world, idle);
        publish_render_diagnostics(&mut world, uploaded);

        let diagnostics = world.get_resource::<Diagnostics>().unwrap();
        let events = diagnostics.events();
        let uploads = events
            .iter()
            .filter(|event| event.code == "render.asset.uploaded")
            .collect::<Vec<_>>();
        assert_eq!(uploads.len(), 2);
        assert_eq!(uploads[0].severity, DiagnosticSeverity::Info);
        assert_eq!(field(uploads[0], "uploaded_render_assets"), Some("1"));
        assert_eq!(
            field(uploads[0], "uploaded_render_asset_bytes"),
            Some("1024")
        );
        assert_eq!(field(uploads[0], "resident_render_assets"), Some("2"));
        assert_eq!(field(uploads[0], "queued_render_assets"), Some("1"));
    }
}
