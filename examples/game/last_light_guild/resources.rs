use sky_engine::ecs::EntityId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GamePhase {
    Planning,
    Departing,
    Resolving,
    Returning,
    Results,
}

impl GamePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Planning => "planning",
            Self::Departing => "departing",
            Self::Resolving => "expedition",
            Self::Returning => "returning",
            Self::Results => "results",
        }
    }
}

#[derive(Clone, Debug)]
pub struct GameFlow {
    pub phase: GamePhase,
    pub selected_slot: usize,
    pub active_contract: Option<EntityId>,
    pub party: Vec<EntityId>,
    pub timer: f32,
}

impl Default for GameFlow {
    fn default() -> Self {
        Self {
            phase: GamePhase::Planning,
            selected_slot: 0,
            active_contract: None,
            party: Vec::new(),
            timer: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Calendar {
    pub day: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct SimClock {
    pub paused: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct GuildStock {
    pub gold: i32,
    pub food: i32,
    pub medicine: i32,
    pub supplies: i32,
    pub reputation: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct TownState {
    pub danger: i32,
    pub unrest: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct BoardCursor {
    pub next: usize,
}

#[derive(Clone, Debug)]
pub struct HudState {
    pub headline: String,
    pub party: String,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            headline: "The guild is choosing its next contract.".to_string(),
            party: "No expedition yet.".to_string(),
        }
    }
}
