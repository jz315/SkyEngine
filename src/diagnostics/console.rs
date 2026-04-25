use std::io::{self, Write};

use super::{DiagnosticEvent, DiagnosticSeverity};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiagnosticConsole {
    Off,
    #[default]
    WarningsAndErrors,
    All,
}

impl DiagnosticConsole {
    #[inline]
    pub const fn allows(self, severity: DiagnosticSeverity) -> bool {
        match self {
            Self::Off => false,
            Self::WarningsAndErrors => {
                matches!(
                    severity,
                    DiagnosticSeverity::Warning | DiagnosticSeverity::Error
                )
            }
            Self::All => true,
        }
    }
}

pub fn write_diagnostic_events<'a, W, I>(
    writer: &mut W,
    events: I,
    console: DiagnosticConsole,
) -> io::Result<usize>
where
    W: Write + ?Sized,
    I: IntoIterator<Item = &'a DiagnosticEvent>,
{
    let mut written = 0;
    for event in events {
        if console.allows(event.severity) {
            writeln!(writer, "{event}")?;
            written += 1;
        }
    }
    Ok(written)
}
