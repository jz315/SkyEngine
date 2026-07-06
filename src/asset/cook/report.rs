#[derive(Debug, Default)]
pub struct VerifyReport {
    pub issues: Vec<String>,
}

impl VerifyReport {
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}
