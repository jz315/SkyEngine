use crate::model::{
    DialogueTone, Ending, Flag, GameState, Location, RouteCostId, RouteWitnessId, StoryEvent,
};
use crate::route_cost;

pub const ROUTE_WITNESS_COUNT: usize = route_cost::ROUTE_COST_COUNT;

#[derive(Clone, Debug)]
pub struct RouteWitnessAction {
    pub witness: RouteWitnessId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteWitnessSummary {
    pub witness: RouteWitnessId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub visited: bool,
}

pub fn available_witnesses(state: &GameState) -> Vec<RouteWitnessAction> {
    RouteWitnessId::ALL
        .iter()
        .copied()
        .filter(|witness| !state.has_visited_route_witness(*witness))
        .filter(|witness| witness.location() == state.location)
        .filter(|witness| witness.visible(state))
        .map(|witness| {
            let missing = missing_requirements(state, witness);
            RouteWitnessAction {
                witness,
                label: witness.label(),
                detail: if missing.is_empty() {
                    "这条路线已经回到现场。现在可以让空间本身替终局作证。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn witness_summaries(state: &GameState) -> Vec<RouteWitnessSummary> {
    RouteWitnessId::ALL
        .iter()
        .copied()
        .map(|witness| {
            let visited = state.has_visited_route_witness(witness);
            let visible = visited || witness.visible(state);
            let missing = missing_requirements(state, witness);
            let ready = visible && missing.is_empty() && !visited;
            let status = if visited {
                "已见证"
            } else if ready {
                "可回访"
            } else if visible {
                "待现场"
            } else {
                "未显形"
            };
            let detail = if visited {
                witness.review().to_string()
            } else if ready {
                format!(
                    "{}已经能回到现场。前往{}，让终局路线在地点里留下证词。",
                    witness.title(),
                    witness.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "先完成路线回声：{}。人物回应之后，地点才会显出最后的见证位置。",
                    witness.cost().title()
                )
            };
            RouteWitnessSummary {
                witness,
                title: witness.title(),
                status,
                detail,
                progress: witness_progress(missing.len(), visible, visited),
                visible,
                ready,
                visited,
            }
        })
        .collect()
}

pub fn witness_objective_hint(state: &GameState) -> Option<String> {
    available_witnesses(state)
        .into_iter()
        .find(|witness| witness.enabled)
        .map(|witness| format!("路线现场可以回访：{}。", witness.label))
}

pub fn ending_note(ending: Ending, state: &GameState) -> Option<String> {
    let witness = RouteWitnessId::ALL
        .iter()
        .copied()
        .find(|witness| witness.ending() == ending && state.has_visited_route_witness(*witness));
    witness.map(|witness| {
        format!(
            "终局之前，你曾回到现场完成路线见证：{}。所以这个结局不是菜单上的一行字，而是已经被{}记住的事实。",
            witness.title(),
            witness.location().title()
        )
    })
}

pub fn visit(state: &mut GameState, witness: RouteWitnessId) -> StoryEvent {
    if state.has_visited_route_witness(witness) {
        return StoryEvent::new(
            "路线现场已经见证过",
            "你又回到这个位置。它没有给出新答案，只把已经留下的那道痕迹照得更清楚一点。",
        )
        .tag("路线现场");
    }

    let missing = missing_requirements(state, witness);
    if witness.location() != state.location || !witness.visible(state) || !missing.is_empty() {
        return StoryEvent::new(
            "路线现场还没有显形",
            format!(
                "你想让地点替这条路线作证，但现在还没有足够重量。{}",
                if witness.location() != state.location {
                    format!("这处现场不在这里，而在{}。", witness.location().title())
                } else if !witness.visible(state) {
                    "先完成对应路线回声，让人物回应过这项代价。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("路线现场");
    }

    state.visit_route_witness(witness);
    let mut event = witness.event(state.dialogue_tone);
    apply_witness_rewards(state, witness, &mut event);
    event
}

impl RouteWitnessId {
    pub const ALL: [Self; ROUTE_WITNESS_COUNT] = [
        Self::AloneSeatNotice,
        Self::ChildWindowSeat,
        Self::TimetableAshList,
        Self::BroadcastDryRun,
        Self::KeeperBoundaryLamp,
        Self::LastNoticeTicket,
    ];

    fn cost(self) -> RouteCostId {
        match self {
            Self::AloneSeatNotice => RouteCostId::AloneEmptySeat,
            Self::ChildWindowSeat => RouteCostId::ChildUnforgivenTomorrow,
            Self::TimetableAshList => RouteCostId::BurnedTimetableAftercare,
            Self::BroadcastDryRun => RouteCostId::BroadcastSecondName,
            Self::KeeperBoundaryLamp => RouteCostId::KeeperLightBoundary,
            Self::LastNoticeTicket => RouteCostId::LostPassengerNotice,
        }
    }

    fn location(self) -> Location {
        self.cost().location()
    }

    fn ending(self) -> Ending {
        self.cost().ending()
    }

    fn visible(self, state: &GameState) -> bool {
        state.has_completed_route_echo(self.cost()) || state.has_visited_route_witness(self)
    }

    fn label(self) -> &'static str {
        match self {
            Self::AloneSeatNotice => "路线现场：把说明贴在 07B 空座下",
            Self::ChildWindowSeat => "路线现场：让孩子先试坐靠窗座位",
            Self::TimetableAshList => "路线现场：在火前清点失物柜",
            Self::BroadcastDryRun => "路线现场：做一次不播出的试播",
            Self::KeeperBoundaryLamp => "路线现场：给雾灯画下照明边界",
            Self::LastNoticeTicket => "路线现场：把完整湿票压在月台灯下",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::AloneSeatNotice => "07B 空座下的说明",
            Self::ChildWindowSeat => "靠窗座位的试坐",
            Self::TimetableAshList => "火前失物清点",
            Self::BroadcastDryRun => "不播出的试播",
            Self::KeeperBoundaryLamp => "雾灯照明边界",
            Self::LastNoticeTicket => "月台灯下的完整湿票",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::AloneSeatNotice => "空座下面留下了说明，独自离开不能再把缺席伪装成无事发生。",
            Self::ChildWindowSeat => "孩子先试坐了靠窗座位，同行终于不是由你单方面宣布。",
            Self::TimetableAshList => "失物柜在火前被清点，烧掉规则以前，小事先被承认。",
            Self::BroadcastDryRun => "广播稿经过一次不播出的试播，警告不再急着占据所有人的声音。",
            Self::KeeperBoundaryLamp => "雾灯有了边界，守夜从自我牺牲变成能被检查的职责。",
            Self::LastNoticeTicket => "完整湿票压在月台灯下，后来者至少能读见选择的岔口。",
        }
    }

    fn event(self, tone: DialogueTone) -> StoryEvent {
        let (title, body) = match (self, tone) {
            (Self::ChildWindowSeat, DialogueTone::Gentle) => (
                "路线现场：靠窗座位的试坐",
                "你没有把孩子领上车，只是带他看那张已经被保留下来的 07B。他坐下又站起来，像在确认座位不会因为他不感谢你就消失。你说：它等你，但不催你。他把手放在窗沿上，终于问明天会不会有树。",
            ),
            (Self::BroadcastDryRun, DialogueTone::Listening) => (
                "路线现场：不播出的试播",
                "你按下录音键，却没有打开站内广播。空磁带转过一圈，你们只听见自己的呼吸。原来警告也需要练习不支配别人。红灯没有亮，旧钟却轻轻走了一格。",
            ),
            (Self::KeeperBoundaryLamp, DialogueTone::Direct) => (
                "路线现场：照明边界",
                "你把边界线画在雾灯能照到的最后一块地砖上：灯到这里为止，人的选择从这里开始。站务员看着那条线，说这比誓言难，因为它明天还要被检查。你说正是如此。",
            ),
            _ => (self.label(), self.review()),
        };
        StoryEvent::new(title, body)
            .tag("路线现场")
            .tag(format!("路线：{}", self.cost().title()))
            .tag(format!("地点：{}", self.location().title()))
    }
}

fn missing_requirements(state: &GameState, witness: RouteWitnessId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_completed_route_echo(witness.cost()),
        "先完成对应路线回声",
    );
    require(
        &mut missing,
        state.current_segment() >= 7,
        "进入第七段午夜以后",
    );
    missing
}

fn apply_witness_rewards(state: &mut GameState, witness: RouteWitnessId, event: &mut StoryEvent) {
    match witness {
        RouteWitnessId::AloneSeatNotice => {
            state.remember(Flag::TravelerTrusted);
            event.tags.push("见证：空座".to_string());
        }
        RouteWitnessId::ChildWindowSeat => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = 5;
            event.tags.push("见证：孩子座位".to_string());
        }
        RouteWitnessId::TimetableAshList => {
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("见证：火前善后".to_string());
        }
        RouteWitnessId::BroadcastDryRun => {
            state.remember(Flag::HeardBroadcastTape);
            event.tags.push("见证：试播".to_string());
        }
        RouteWitnessId::KeeperBoundaryLamp => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("见证：边界".to_string());
        }
        RouteWitnessId::LastNoticeTicket => {
            state.remember(Flag::UnderstoodFirstLoop);
            event.tags.push("见证：后来者".to_string());
        }
    }
}

fn witness_progress(missing_count: usize, visible: bool, visited: bool) -> u8 {
    if visited {
        100
    } else if !visible {
        0
    } else {
        (((2_usize.saturating_sub(missing_count)) * 100) / 2) as u8
    }
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MINUTES_PER_SEGMENT;

    #[test]
    fn route_witness_unlocks_after_route_echo_in_late_midnight() {
        let mut state = GameState::new();
        state.location = Location::WaitingHall;
        state.actions_used = MINUTES_PER_SEGMENT * 6;
        state.complete_route_echo(RouteCostId::AloneEmptySeat);

        let actions = available_witnesses(&state);
        assert!(actions
            .iter()
            .any(|action| action.witness == RouteWitnessId::AloneSeatNotice && action.enabled));
        assert!(witness_objective_hint(&state).is_some_and(|hint| hint.contains("路线现场")));

        let event = visit(&mut state, RouteWitnessId::AloneSeatNotice);
        assert!(state.has_visited_route_witness(RouteWitnessId::AloneSeatNotice));
        assert!(event.tags.iter().any(|tag| tag == "路线现场"));

        let summary = witness_summaries(&state)
            .into_iter()
            .find(|summary| summary.witness == RouteWitnessId::AloneSeatNotice)
            .expect("route witness summary should exist");
        assert_eq!(summary.status, "已见证");
        assert_eq!(summary.progress, 100);
    }
}
