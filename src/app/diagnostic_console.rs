//! App-runner bridge from structured diagnostics to console output.

use crate::diagnostics::{
    write_diagnostic_events, DiagnosticConsole, DiagnosticCursor, Diagnostics,
};
use crate::ecs::World;

pub(crate) fn write_new<W: std::io::Write>(
    world: &World,
    cursor: &mut DiagnosticCursor,
    console: DiagnosticConsole,
    writer: &mut W,
) -> std::io::Result<usize> {
    let Some(diagnostics) = world.get_resource::<Diagnostics>() else {
        return Ok(0);
    };
    let events = diagnostics.events_since(cursor);
    write_diagnostic_events(writer, events.iter(), console)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{DiagnosticEvent, DiagnosticSubsystem};

    #[test]
    fn bridge_writes_new_filtered_events() {
        let mut world = World::new();
        world.insert_resource(Diagnostics::default());
        let diagnostics = world
            .get_resource::<Diagnostics>()
            .expect("diagnostics should exist");
        diagnostics.report(
            DiagnosticEvent::info("engine.note", DiagnosticSubsystem::engine(), "note")
                .with_title("Note"),
        );
        diagnostics.report(
            DiagnosticEvent::warning("render.warning", DiagnosticSubsystem::render(), "warning")
                .with_title("Warning"),
        );
        diagnostics.report(
            DiagnosticEvent::error("asset.error", DiagnosticSubsystem::asset(), "error")
                .with_title("Error"),
        );

        let mut cursor = DiagnosticCursor::new();
        let mut output = Vec::new();
        let written = write_new(
            &world,
            &mut cursor,
            DiagnosticConsole::WarningsAndErrors,
            &mut output,
        )
        .expect("diagnostics should write to memory");

        let text = String::from_utf8(output).expect("diagnostics should be UTF-8");
        assert_eq!(written, 2);
        assert!(!text.contains("[SkyEngine][engine][info]"));
        assert!(text.contains("[SkyEngine][render][warning] Warning"));
        assert!(text.contains("[SkyEngine][asset][error] Error"));

        let mut second = Vec::new();
        let written = write_new(
            &world,
            &mut cursor,
            DiagnosticConsole::WarningsAndErrors,
            &mut second,
        )
        .expect("diagnostics should write to memory");
        assert_eq!(written, 0);
        assert!(second.is_empty());
    }
}
