#[derive(Clone, Debug)]
pub struct HudState {
    pub message: String,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            message: "finish the commission: reach target score with 3 of each style".to_string(),
        }
    }
}
