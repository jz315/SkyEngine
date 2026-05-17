//! Cross-platform OS helpers.

/// Open a URL with the user's default browser.
pub fn open_url(url: &str) -> bool {
    if url.trim().is_empty() {
        return false;
    }
    webbrowser::open(url).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_url_rejects_empty_input() {
        assert!(!open_url(""));
        assert!(!open_url("   "));
    }
}
