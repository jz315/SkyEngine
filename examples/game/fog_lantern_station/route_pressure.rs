use crate::model::{
    ActiveDialogue, DepartureId, DialogueId, DialogueTone, Flag, GameState, RoutePressureId,
    RoutePressureResponseId, StoryEvent,
};

pub const ROUTE_PRESSURE_COUNT: usize = RoutePressureId::ALL.len();

#[derive(Clone, Debug)]
pub struct RoutePressureResponseAction {
    pub pressure: RoutePressureId,
    pub response: RoutePressureResponseId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutePressureSummary {
    pub pressure: RoutePressureId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub answered: bool,
}

pub fn available_responses(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<RoutePressureResponseAction> {
    RoutePressureId::ALL
        .iter()
        .copied()
        .filter(|pressure| pressure.dialogue() == active.dialogue)
        .filter(|pressure| pressure.visible(state))
        .filter(|pressure| !state.has_answered_route_pressure(*pressure))
        .flat_map(|pressure| {
            RoutePressureResponseId::ALL
                .iter()
                .copied()
                .map(move |response| RoutePressureResponseAction {
                    pressure,
                    response,
                    label: response.label_for(pressure).to_string(),
                    detail: response.detail_for(pressure),
                    enabled: true,
                })
        })
        .collect()
}

pub fn pressure_summaries(state: &GameState) -> Vec<RoutePressureSummary> {
    RoutePressureId::ALL
        .iter()
        .copied()
        .map(|pressure| {
            let answered = state.has_answered_route_pressure(pressure);
            let visible = answered || pressure.visible(state);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == pressure.dialogue())
                && visible
                && !answered;
            let status = if answered {
                "已回应"
            } else if ready {
                "正在追问"
            } else if visible {
                "待进入对话"
            } else {
                "未显形"
            };
            let detail = if let Some(response) = state.route_pressure_response(pressure) {
                pressure.review(response).to_string()
            } else if ready {
                format!(
                    "{}已经看出你倾向“{}”，正在逼你提前承认这条路会伤到哪里。",
                    pressure.dialogue().title(),
                    pressure.departure().title()
                )
            } else if visible {
                format!(
                    "进入{}的自由对话，回应这条路线的中段压力。",
                    pressure.dialogue().title()
                )
            } else {
                format!(
                    "先完成路线准备：{}。人物只有看见你开始偏向某条路，才会真正顶回来。",
                    pressure.departure().title()
                )
            };
            RoutePressureSummary {
                pressure,
                title: pressure.title(),
                status,
                detail,
                progress: pressure_progress(visible, ready, answered),
                visible,
                ready,
                answered,
            }
        })
        .collect()
}

pub fn active_pressure_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    RoutePressureId::ALL
        .iter()
        .copied()
        .find(|pressure| {
            pressure.dialogue() == active.dialogue
                && pressure.visible(state)
                && !state.has_answered_route_pressure(*pressure)
        })
        .map(|pressure| {
            format!(
                "{}正在质疑你的路线倾向：{}。",
                pressure.dialogue().title(),
                pressure.prompt()
            )
        })
}

pub fn pressure_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_pressure_objective_hint(state);
    }
    RoutePressureId::ALL
        .iter()
        .copied()
        .filter(|pressure| pressure.visible(state))
        .filter(|pressure| !state.has_answered_route_pressure(*pressure))
        .find(|pressure| pressure.dialogue().location() == state.location)
        .map(|pressure| {
            format!(
                "可以进入{}的对话，回应路线压力：{}。",
                pressure.dialogue().title(),
                pressure.title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.answered_route_pressures.is_empty() {
        return None;
    }

    let notes = state
        .answered_route_pressures
        .iter()
        .map(|(pressure, response)| format!("{}：{}", pressure.title(), pressure.review(*response)))
        .collect::<Vec<_>>();
    Some(format!(
        "这些路线倾向不是到终局才突然出现的。中段对话里，人物已经提前顶回过你：{}",
        notes.join(" ")
    ))
}

pub fn answer(
    state: &mut GameState,
    pressure: RoutePressureId,
    response: RoutePressureResponseId,
) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "路线压力没有对象",
            "这不是可以独自完成的内心独白。必须让相关人物在场，路线才会被关系质疑。",
        )
        .tag("路线争论");
    };

    if active.dialogue != pressure.dialogue() {
        return StoryEvent::new(
            "这不是当前人物的路线争论",
            format!(
                "你正和{}说话，但这条路线压力来自{}。",
                active.dialogue.title(),
                pressure.dialogue().title()
            ),
        )
        .tag("路线争论");
    }

    if !pressure.visible(state) {
        return StoryEvent::new(
            "路线压力还没有显形",
            format!(
                "先完成路线准备：{}。没有具体行动，人物只会听见空泛的决心。",
                pressure.departure().title()
            ),
        )
        .tag("路线争论");
    }

    if state.has_answered_route_pressure(pressure) {
        return StoryEvent::new(
            "路线争论已经回应过",
            "这个问题已经进入你们的关系里。再说一遍不会让路线更轻，只会让车站知道你害怕沉默。",
        )
        .tag("路线争论")
        .tag("复看");
    }

    state.answer_route_pressure(pressure, response);
    let mut event = response.event(pressure, state.dialogue_tone);
    apply_pressure_rewards(state, pressure, response, &mut event);
    event
}

impl RoutePressureId {
    pub const ALL: [Self; 5] = [
        Self::AloneTraveler,
        Self::ChildTomorrow,
        Self::TimetableClerk,
        Self::BroadcastKeeper,
        Self::KeeperDuty,
    ];

    fn dialogue(self) -> DialogueId {
        match self {
            Self::AloneTraveler => DialogueId::Traveler,
            Self::ChildTomorrow => DialogueId::Child,
            Self::TimetableClerk => DialogueId::Clerk,
            Self::BroadcastKeeper | Self::KeeperDuty => DialogueId::Keeper,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AloneTraveler => "独自离开会把空座留给谁",
            Self::ChildTomorrow => "带他离开是不是又一次替他安排",
            Self::TimetableClerk => "烧掉规则以后谁来收拾",
            Self::BroadcastKeeper => "成为广播会不会变成命令",
            Self::KeeperDuty => "留下守夜会不会变成占有",
        }
    }

    fn departure(self) -> DepartureId {
        match self {
            Self::AloneTraveler => DepartureId::SingleReturnPocket,
            Self::ChildTomorrow => DepartureId::ChildWindowSeat,
            Self::TimetableClerk => DepartureId::TimetableMatch,
            Self::BroadcastKeeper => DepartureId::BroadcastScript,
            Self::KeeperDuty => DepartureId::KeeperLedger,
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::AloneTraveler => {
                "你已经把自己的名字缝进票夹。老人问：那你是不是终于准备把空座丢给别人？"
            }
            Self::ChildTomorrow => {
                "孩子看见 07B 被保留下来，问：你是在给我座位，还是在给自己一张好人证明？"
            }
            Self::TimetableClerk => {
                "售票员看见火柴，问：烧掉时刻表以后，那些靠规则撑住的人怎么办？"
            }
            Self::BroadcastKeeper => {
                "站务员看见广播稿，问：你把名字交给广播，是提醒后来者，还是把自己变成新的规矩？"
            }
            Self::KeeperDuty => {
                "站务员看见值夜簿的新页，问：你说留下守夜，可你怎么证明那不是占有？"
            }
        }
    }

    fn visible(self, state: &GameState) -> bool {
        state.has_prepared_departure(self.departure())
            || state.has_rehearsed_departure(self.departure())
            || state.has_answered_route_pressure(self)
    }

    fn review(self, response: RoutePressureResponseId) -> &'static str {
        match (self, response) {
            (Self::AloneTraveler, RoutePressureResponseId::DefendRoute) => {
                "你坚持独自离开，但没有再把空座说成无关紧要。"
            }
            (Self::AloneTraveler, RoutePressureResponseId::AdmitRisk) => {
                "你承认独自离开会伤人，于是老人不再只把你当成逃票的人。"
            }
            (Self::AloneTraveler, RoutePressureResponseId::RevisePromise) => {
                "你把承诺改成不替缺席者发言，只替自己的离开承担。"
            }
            (Self::ChildTomorrow, RoutePressureResponseId::DefendRoute) => {
                "你坚持同行，但承认孩子到站后仍可以不感谢你。"
            }
            (Self::ChildTomorrow, RoutePressureResponseId::AdmitRisk) => {
                "你承认带他离开不是修好一切，明天仍可能继续疼。"
            }
            (Self::ChildTomorrow, RoutePressureResponseId::RevisePromise) => {
                "你把承诺改成听他继续说不，而不是收集他的原谅。"
            }
            (Self::TimetableClerk, RoutePressureResponseId::DefendRoute) => {
                "你坚持烧掉规则，但不再把火说成纯粹正义。"
            }
            (Self::TimetableClerk, RoutePressureResponseId::AdmitRisk) => {
                "你承认规则也曾挡风，所以撤销它必须带着善后。"
            }
            (Self::TimetableClerk, RoutePressureResponseId::RevisePromise) => {
                "你把承诺改成不立刻写一张新的表格替别人选择。"
            }
            (Self::BroadcastKeeper, RoutePressureResponseId::DefendRoute) => {
                "你坚持成为广播，同时承认提醒会被误听。"
            }
            (Self::BroadcastKeeper, RoutePressureResponseId::AdmitRisk) => {
                "你承认提醒也可能伤人，所以声音不能冒充答案。"
            }
            (Self::BroadcastKeeper, RoutePressureResponseId::RevisePromise) => {
                "你把承诺改成只保留迟疑，不把迟疑扩写成命令。"
            }
            (Self::KeeperDuty, RoutePressureResponseId::DefendRoute) => {
                "你坚持留下守夜，但承认守夜会诱惑人变成制度。"
            }
            (Self::KeeperDuty, RoutePressureResponseId::AdmitRisk) => {
                "你承认留下也会伤人，所以不把牺牲当成免罪。"
            }
            (Self::KeeperDuty, RoutePressureResponseId::RevisePromise) => {
                "你把承诺改成先照门口，再照自己的职责。"
            }
        }
    }
}

impl RoutePressureResponseId {
    pub const ALL: [Self; 3] = [Self::DefendRoute, Self::AdmitRisk, Self::RevisePromise];

    fn name(self) -> &'static str {
        match self {
            Self::DefendRoute => "坚持路线",
            Self::AdmitRisk => "承认风险",
            Self::RevisePromise => "改写承诺",
        }
    }

    fn label_for(self, pressure: RoutePressureId) -> &'static str {
        match (pressure, self) {
            (RoutePressureId::AloneTraveler, Self::DefendRoute) => "路线争论：我仍要离开",
            (RoutePressureId::AloneTraveler, Self::AdmitRisk) => "路线争论：离开也会伤人",
            (RoutePressureId::AloneTraveler, Self::RevisePromise) => "路线争论：只替自己承担",
            (RoutePressureId::ChildTomorrow, Self::DefendRoute) => "路线争论：我仍和你同行",
            (RoutePressureId::ChildTomorrow, Self::AdmitRisk) => "路线争论：明天不会立刻修好",
            (RoutePressureId::ChildTomorrow, Self::RevisePromise) => "路线争论：你可以继续说不",
            (RoutePressureId::TimetableClerk, Self::DefendRoute) => "路线争论：我仍要烧掉它",
            (RoutePressureId::TimetableClerk, Self::AdmitRisk) => "路线争论：规则也曾保护人",
            (RoutePressureId::TimetableClerk, Self::RevisePromise) => "路线争论：不写新的表格",
            (RoutePressureId::BroadcastKeeper, Self::DefendRoute) => "路线争论：我仍要播报",
            (RoutePressureId::BroadcastKeeper, Self::AdmitRisk) => "路线争论：提醒也会伤人",
            (RoutePressureId::BroadcastKeeper, Self::RevisePromise) => "路线争论：不把迟疑写成命令",
            (RoutePressureId::KeeperDuty, Self::DefendRoute) => "路线争论：我仍要留下",
            (RoutePressureId::KeeperDuty, Self::AdmitRisk) => "路线争论：留下也会伤人",
            (RoutePressureId::KeeperDuty, Self::RevisePromise) => "路线争论：先照门口",
        }
    }

    fn detail_for(self, pressure: RoutePressureId) -> String {
        format!("{} {}", pressure.prompt(), self.intent())
    }

    fn intent(self) -> &'static str {
        match self {
            Self::DefendRoute => "坚持路线，但不把它说成轻松胜利。",
            Self::AdmitRisk => "承认这条路会伤人，让人物知道你不是盲目奔向结局。",
            Self::RevisePromise => "把承诺从控制别人，改成约束自己。",
        }
    }

    fn event(self, pressure: RoutePressureId, tone: DialogueTone) -> StoryEvent {
        let (title, body) = match (pressure, self, tone) {
            (RoutePressureId::ChildTomorrow, Self::RevisePromise, DialogueTone::Gentle) => (
                "路线争论：你可以继续说不",
                "你把声音放轻：到站以后，你仍可以说不，仍可以后悔，仍可以不感谢我。孩子盯着你的脸，像在找这句话背后的绳结。最后他说：那我会记住你说过。不是原谅，是记住。",
            ),
            (RoutePressureId::ChildTomorrow, Self::AdmitRisk, _) => (
                "路线争论：明天不会立刻修好",
                "你没有承诺他会快乐。你说：明天可能仍然疼，而且疼的时候你可以怪我。孩子把作业本抱得松了一点，因为这次大人终于没有用保证堵住他的嘴。",
            ),
            (RoutePressureId::AloneTraveler, Self::AdmitRisk, _) => (
                "路线争论：离开也会伤人",
                "老人问你是不是准备把空座丢给别人。你说：是，离开也会伤人。老人把报纸折出一条新折痕：那就别把伤口叫作自由。",
            ),
            (RoutePressureId::TimetableClerk, Self::AdmitRisk, _) => (
                "路线争论：规则也曾保护人",
                "售票员问你火烧完以后谁来收拾。你承认规则曾经挡过风。她的手离开票章一寸：那你烧掉的就不是敌人，是一副已经勒进肉里的夹板。",
            ),
            (RoutePressureId::BroadcastKeeper, Self::RevisePromise, _) => (
                "路线争论：不把迟疑写成命令",
                "你说广播只保留迟疑，不替后来者下判决。站务员听完很久没有说话，旧钟在你们中间慢慢走过一格，像终于允许警告不是命令。",
            ),
            (RoutePressureId::KeeperDuty, Self::RevisePromise, _) => (
                "路线争论：先照门口",
                "你说第一条规矩是先照门口，再照自己的职责。站务员看着值夜簿的新页，像看见一个仍会犯错的人终于给错误留了栏位。",
            ),
            _ => (
                self.label_for(pressure),
                pressure.review(self),
            ),
        };
        StoryEvent::new(title, body)
            .tag("路线争论")
            .tag(format!("回应：{}", self.name()))
            .tag(format!("路线：{}", pressure.departure().title()))
    }
}

fn pressure_progress(visible: bool, ready: bool, answered: bool) -> u8 {
    if answered {
        100
    } else if ready {
        70
    } else if visible {
        45
    } else {
        0
    }
}

fn apply_pressure_rewards(
    state: &mut GameState,
    pressure: RoutePressureId,
    response: RoutePressureResponseId,
    event: &mut StoryEvent,
) {
    state.synthesis_depth = state.synthesis_depth.saturating_add(1).min(12);
    match pressure {
        RoutePressureId::AloneTraveler => {
            state.traveler_depth = state.traveler_depth.saturating_add(1).min(12);
            if response != RoutePressureResponseId::DefendRoute {
                state.remember(Flag::TravelerTrusted);
                event.tags.push("关系：老人承认".to_string());
            }
        }
        RoutePressureId::ChildTomorrow => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("关系：孩子保留反驳".to_string());
        }
        RoutePressureId::TimetableClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("关系：售票员追问善后".to_string());
        }
        RoutePressureId::BroadcastKeeper | RoutePressureId::KeeperDuty => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("关系：站务员确认边界".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DialogueNodeId, Location};

    #[test]
    fn route_pressure_unlocks_inside_active_dialogue_after_route_preparation() {
        let mut state = GameState::new();
        state.location = Location::Platform;
        state.prepare_departure(DepartureId::ChildWindowSeat);
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
        });
        let actions = available_responses(&state, state.active_dialogue.unwrap());
        assert!(actions
            .iter()
            .any(|action| action.pressure == RoutePressureId::ChildTomorrow));

        let event = answer(
            &mut state,
            RoutePressureId::ChildTomorrow,
            RoutePressureResponseId::RevisePromise,
        );
        assert!(state.has_answered_route_pressure(RoutePressureId::ChildTomorrow));
        assert!(event.tags.iter().any(|tag| tag == "路线争论"));
        assert!(state.child_trust > 0);
        let summary = pressure_summaries(&state)
            .into_iter()
            .find(|summary| summary.pressure == RoutePressureId::ChildTomorrow)
            .expect("route pressure summary should exist");
        assert_eq!(summary.status, "已回应");
        assert_eq!(summary.progress, 100);
    }
}
