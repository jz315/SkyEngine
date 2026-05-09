use sky_engine::ecs::EntityId;
use sky_engine::render::Color;

#[derive(Clone, Copy, Debug)]
pub struct Adventurer;

#[derive(Clone, Copy, Debug)]
pub struct ContractMarker;

#[derive(Clone, Copy, Debug)]
pub struct ContractSlot(pub usize);

#[derive(Clone, Copy, Debug)]
pub struct Fixture;

#[derive(Clone, Copy, Debug)]
pub struct StatusPip;

#[derive(Clone, Copy, Debug)]
pub struct Name(pub &'static str);

#[derive(Clone, Copy, Debug)]
pub struct GridPos {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug)]
pub struct PixelVisual {
    pub base: Color,
    pub hurt: Color,
    pub pulse: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct Follow {
    pub target: EntityId,
    pub offset_x: f32,
    pub offset_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Bunks,
    Infirmary,
    Stores,
    Training,
    Common,
    Board,
}

impl RoomKind {
    pub fn tile_id(self) -> u32 {
        match self {
            Self::Bunks => 1,
            Self::Infirmary => 2,
            Self::Stores => 3,
            Self::Training => 4,
            Self::Common => 5,
            Self::Board => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Vanguard,
    Scout,
    Medic,
    Occultist,
    Broker,
}

impl Role {
    pub fn color(self) -> Color {
        match self {
            Self::Vanguard => Color::rgb(0.88, 0.28, 0.25),
            Self::Scout => Color::rgb(0.33, 0.86, 0.42),
            Self::Medic => Color::rgb(0.35, 0.78, 0.94),
            Self::Occultist => Color::rgb(0.72, 0.42, 0.95),
            Self::Broker => Color::rgb(0.95, 0.76, 0.26),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Vanguard => "vanguard",
            Self::Scout => "scout",
            Self::Medic => "medic",
            Self::Occultist => "occultist",
            Self::Broker => "broker",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Stats {
    pub might: i32,
    pub finesse: i32,
    pub wits: i32,
    pub spirit: i32,
}

impl Stats {
    pub fn get(self, stat: PrimaryStat) -> i32 {
        match stat {
            PrimaryStat::Might => self.might,
            PrimaryStat::Finesse => self.finesse,
            PrimaryStat::Wits => self.wits,
            PrimaryStat::Spirit => self.spirit,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Condition {
    pub health: i32,
    pub stress: i32,
    pub fatigue: i32,
}

impl Condition {
    pub fn readiness(self) -> i32 {
        self.health * 2 - self.stress - self.fatigue
    }

    pub fn clamp(&mut self) {
        self.health = self.health.clamp(0, 10);
        self.stress = self.stress.clamp(0, 10);
        self.fatigue = self.fatigue.clamp(0, 10);
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Personality {
    pub courage: i32,
    pub caution: i32,
    pub empathy: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractKind {
    Hunt,
    Rescue,
    Salvage,
    Escort,
    Investigate,
}

impl ContractKind {
    pub fn color(self) -> Color {
        match self {
            Self::Hunt => Color::rgb(0.84, 0.18, 0.16),
            Self::Rescue => Color::rgb(0.26, 0.80, 0.94),
            Self::Salvage => Color::rgb(0.92, 0.70, 0.18),
            Self::Escort => Color::rgb(0.28, 0.74, 0.38),
            Self::Investigate => Color::rgb(0.70, 0.38, 0.90),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Hunt => "hunt",
            Self::Rescue => "rescue",
            Self::Salvage => "salvage",
            Self::Escort => "escort",
            Self::Investigate => "investigate",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryStat {
    Might,
    Finesse,
    Wits,
    Spirit,
}

impl PrimaryStat {
    pub fn label(self) -> &'static str {
        match self {
            Self::Might => "might",
            Self::Finesse => "finesse",
            Self::Wits => "wits",
            Self::Spirit => "spirit",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ContractSpec {
    pub kind: ContractKind,
    pub difficulty: i32,
    pub reward_gold: i32,
    pub reward_food: i32,
    pub reward_medicine: i32,
    pub reward_reputation: i32,
    pub supply_cost: i32,
    pub primary_stat: PrimaryStat,
    pub preferred_role: Role,
}

#[derive(Clone, Copy, Debug)]
pub struct Relationship {
    pub a: EntityId,
    pub b: EntityId,
    pub trust: i32,
    pub tension: i32,
}
