use log::Level;

use super::LogEntry;

/// Controls which captured log entries are mirrored to stderr.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LogConsole {
    Off,
    Errors,
    #[default]
    WarningsAndErrors,
    All,
}

impl LogConsole {
    #[inline]
    pub const fn allows(self, level: Level) -> bool {
        match self {
            Self::Off => false,
            Self::Errors => matches!(level, Level::Error),
            Self::WarningsAndErrors => matches!(level, Level::Error | Level::Warn),
            Self::All => true,
        }
    }
}

#[cfg_attr(not(feature = "app"), allow(dead_code))]
pub(crate) fn write_entry_to_stderr(entry: &LogEntry) {
    eprintln!("{}", format_console_entry(entry));
}

#[cfg_attr(not(feature = "app"), allow(dead_code))]
fn format_console_entry(entry: &LogEntry) -> String {
    let location = match (entry.short_file(), entry.line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.to_owned(),
        (None, Some(line)) => format!("line {line}"),
        (None, None) => entry
            .module_path
            .as_deref()
            .unwrap_or(entry.target.as_str())
            .to_owned(),
    };

    if entry.repeat_count > 1 {
        format!(
            "[SkyEngine][{}] {} {} (x{})",
            entry.level, location, entry.message, entry.repeat_count
        )
    } else {
        format!(
            "[SkyEngine][{}] {} {}",
            entry.level, location, entry.message
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn console_filter_matches_expected_levels() {
        assert!(!LogConsole::Off.allows(Level::Error));
        assert!(LogConsole::Errors.allows(Level::Error));
        assert!(!LogConsole::Errors.allows(Level::Warn));
        assert!(LogConsole::WarningsAndErrors.allows(Level::Warn));
        assert!(!LogConsole::WarningsAndErrors.allows(Level::Info));
        assert!(LogConsole::All.allows(Level::Debug));
    }
}
