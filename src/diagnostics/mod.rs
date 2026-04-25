mod console;
mod engine;
mod event;
mod store;

pub use console::{write_diagnostic_events, DiagnosticConsole};
pub use engine::EngineDiagnosticKind;
pub use event::{
    DiagnosticEvent, DiagnosticField, DiagnosticId, DiagnosticKey, DiagnosticSeverity,
    DiagnosticSubsystem,
};
pub use store::{DiagnosticCursor, Diagnostics};

pub type EngineDiagnostic = DiagnosticEvent;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ecs::{EntityId, World};

    #[test]
    fn report_once_deduplicates_by_kind() {
        let diagnostics = Diagnostics::new();
        let entity = EntityId::new(7, 1);
        let kind = EngineDiagnosticKind::CameraMissingProjection { entity };

        assert!(diagnostics.report_once(kind).is_some());
        assert!(diagnostics.report_once(kind).is_none());
        assert_eq!(diagnostics.len(), 1);
    }

    #[test]
    fn cursor_returns_only_new_diagnostics() {
        let diagnostics = Diagnostics::new();
        let mut cursor = DiagnosticCursor::new();

        diagnostics.report(EngineDiagnosticKind::CameraMissingProjection {
            entity: EntityId::new(1, 0),
        });
        assert_eq!(diagnostics.events_since(&mut cursor).len(), 1);
        assert!(diagnostics.events_since(&mut cursor).is_empty());
    }

    #[test]
    fn generic_events_can_be_reported_once_without_engine_kind() {
        let diagnostics = Diagnostics::new();

        let event = DiagnosticEvent::warning(
            "live2d.parameter.missing",
            DiagnosticSubsystem::live2d(),
            "Live2D parameter ParamAngleX does not exist; animator binding ignored.",
        )
        .with_title("Live2D parameter is missing")
        .with_help("Check the Cubism parameter name used by the animator binding.")
        .with_field("parameter", "ParamAngleX")
        .with_once_key("live2d.parameter.missing:ParamAngleX");

        assert!(diagnostics.report_once(event.clone()).is_some());
        assert!(diagnostics.report_once(event).is_none());

        let entries = diagnostics.entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id.as_str(), "live2d.parameter.missing");
        assert_eq!(entries[0].subsystem.as_str(), "live2d");
        assert_eq!(entries[0].title, "Live2D parameter is missing");
        assert_eq!(
            entries[0].help.as_deref(),
            Some("Check the Cubism parameter name used by the animator binding.")
        );
        assert_eq!(entries[0].field("parameter"), Some("ParamAngleX"));
    }

    #[test]
    fn diagnostic_display_formats_human_readable_line() {
        let event = DiagnosticEvent::warning(
            "render.asset.texture.missing",
            DiagnosticSubsystem::render(),
            "A sprite referenced a texture asset that could not be resolved.",
        )
        .with_title("Texture asset is missing")
        .with_help("Check that the texture is registered in the asset manifest.")
        .with_field("asset_id", "asset-1")
        .with_field("asset_type", "texture");

        assert_eq!(
            event.to_string(),
            "[SkyEngine][render][warning] Texture asset is missing: A sprite referenced a texture \
             asset that could not be resolved. help: Check that the texture is registered in the \
             asset manifest. asset_id=asset-1 asset_type=texture"
        );
    }

    #[test]
    fn diagnostic_console_filters_events() {
        assert!(!DiagnosticConsole::Off.allows(DiagnosticSeverity::Error));
        assert!(DiagnosticConsole::WarningsAndErrors.allows(DiagnosticSeverity::Warning));
        assert!(DiagnosticConsole::WarningsAndErrors.allows(DiagnosticSeverity::Error));
        assert!(!DiagnosticConsole::WarningsAndErrors.allows(DiagnosticSeverity::Info));
        assert!(DiagnosticConsole::All.allows(DiagnosticSeverity::Info));
    }

    #[test]
    fn write_diagnostic_events_uses_console_filter() {
        let events = [
            DiagnosticEvent::info("engine.note", DiagnosticSubsystem::engine(), "note")
                .with_title("Note"),
            DiagnosticEvent::warning("render.warning", DiagnosticSubsystem::render(), "warning")
                .with_title("Warning"),
            DiagnosticEvent::error("asset.error", DiagnosticSubsystem::asset(), "error")
                .with_title("Error"),
        ];
        let mut output = Vec::new();

        let written = write_diagnostic_events(
            &mut output,
            events.iter(),
            DiagnosticConsole::WarningsAndErrors,
        )
        .expect("writing diagnostics should succeed");

        let text = String::from_utf8(output).expect("diagnostics should be UTF-8");
        assert_eq!(written, 2);
        assert!(!text.contains("[SkyEngine][engine][info]"));
        assert!(text.contains("[SkyEngine][render][warning] Warning"));
        assert!(text.contains("[SkyEngine][asset][error] Error"));
    }

    #[test]
    fn capacity_tracks_dropped_events_and_missed_cursor_events() {
        let diagnostics = Diagnostics::with_capacity(2);
        let cursor = DiagnosticCursor::new();

        for index in 0..3 {
            diagnostics.report(DiagnosticEvent::info(
                format!("test.event.{index}"),
                DiagnosticSubsystem::engine(),
                "test event",
            ));
        }

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics.dropped_count(), 1);
        assert_eq!(diagnostics.missed_since(cursor), 1);
    }

    #[test]
    fn resource_installs_diagnostics_into_world() {
        let mut world = World::new();
        assert!(!world.contains_resource::<Diagnostics>());

        Diagnostics::resource(&mut world).report(EngineDiagnosticKind::CameraMissingProjection {
            entity: EntityId::new(2, 0),
        });

        assert_eq!(
            world
                .get_resource::<Diagnostics>()
                .expect("diagnostics should be installed")
                .len(),
            1
        );
    }
}
