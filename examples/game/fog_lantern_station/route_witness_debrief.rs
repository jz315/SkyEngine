use crate::model::{
    ActiveDialogue, DialogueId, DialogueTone, Flag, GameState, RouteWitnessDebriefId,
    RouteWitnessId, StoryEvent,
};

pub const ROUTE_WITNESS_DEBRIEF_COUNT: usize = RouteWitnessDebriefId::ALL.len();

#[derive(Clone, Debug)]
pub struct RouteWitnessDebriefAction {
    pub debrief: RouteWitnessDebriefId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteWitnessDebriefSummary {
    pub debrief: RouteWitnessDebriefId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub completed: bool,
}

pub fn available_debriefs(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<RouteWitnessDebriefAction> {
    RouteWitnessDebriefId::ALL
        .iter()
        .copied()
        .filter(|debrief| debrief.dialogue() == active.dialogue)
        .filter(|debrief| debrief.visible(state))
        .filter(|debrief| !state.has_completed_route_witness_debrief(*debrief))
        .map(|debrief| RouteWitnessDebriefAction {
            debrief,
            label: debrief.label(),
            detail: debrief.detail(state),
            enabled: true,
        })
        .collect()
}

pub fn debrief_summaries(state: &GameState) -> Vec<RouteWitnessDebriefSummary> {
    RouteWitnessDebriefId::ALL
        .iter()
        .copied()
        .map(|debrief| {
            let completed = state.has_completed_route_witness_debrief(debrief);
            let visible = completed || debrief.visible(state);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == debrief.dialogue())
                && visible
                && !completed;
            let status = if completed {
                "已复盘"
            } else if ready {
                "可复盘"
            } else if visible {
                "待对话"
            } else {
                "未见证"
            };
            let detail = if completed {
                debrief.review().to_string()
            } else if ready {
                format!(
                    "{}正在听。把「{}」的现场结果带回对话里。",
                    debrief.dialogue().title(),
                    debrief.witness_title()
                )
            } else if visible {
                format!(
                    "进入{}的主动对话，复盘路线现场：{}。",
                    debrief.dialogue().title(),
                    debrief.witness_title()
                )
            } else {
                format!(
                    "先完成路线现场：{}。地点见证过以后，人物才会愿意谈它留下什么。",
                    debrief.witness_title()
                )
            };
            RouteWitnessDebriefSummary {
                debrief,
                title: debrief.title(),
                status,
                detail,
                progress: debrief_progress(visible, ready, completed),
                visible,
                ready,
                completed,
            }
        })
        .collect()
}

pub fn active_debrief_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_debriefs(state, active)
        .into_iter()
        .find(|debrief| debrief.enabled)
        .map(|debrief| format!("可以在当前对话里复盘路线现场：{}。", debrief.label))
}

pub fn debrief_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_debrief_objective_hint(state);
    }
    RouteWitnessDebriefId::ALL
        .iter()
        .copied()
        .filter(|debrief| debrief.visible(state))
        .filter(|debrief| !state.has_completed_route_witness_debrief(*debrief))
        .find(|debrief| debrief.dialogue().location() == state.location)
        .map(|debrief| {
            format!(
                "可以进入{}的对话，复盘路线现场：{}。",
                debrief.dialogue().title(),
                debrief.title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.completed_route_witness_debriefs.is_empty() {
        return None;
    }
    let notes = state
        .completed_route_witness_debriefs
        .iter()
        .map(|debrief| debrief.title())
        .collect::<Vec<_>>()
        .join(" / ");
    Some(format!(
        "这些路线现场又被带回人物面前复盘过：{notes}。终局因此不只是地点留下的痕迹，也有人愿意记住它。"
    ))
}

pub fn debrief(state: &mut GameState, debrief: RouteWitnessDebriefId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "复盘没有听众",
            "路线现场不能只在你心里反复播放。它需要被一个人听见，才会变成关系里的事实。",
        )
        .tag("路线复盘");
    };

    if active.dialogue != debrief.dialogue() {
        return StoryEvent::new(
            "复盘对象不对",
            format!(
                "你正和{}说话，但这段路线现场应该先带给{}。",
                active.dialogue.title(),
                debrief.dialogue().title()
            ),
        )
        .tag("路线复盘");
    }

    if !state.has_visited_route_witness(debrief.witness()) {
        return StoryEvent::new(
            "现场还没有发生",
            format!(
                "「{}」还没有被地点见证。现在复盘它，只会把未来说成已经发生。",
                debrief.witness_title()
            ),
        )
        .tag("路线复盘");
    }

    if state.has_completed_route_witness_debrief(debrief) {
        return StoryEvent::new(
            "路线现场已经复盘过",
            "对方记得你的现场行动，也记得你没有把它当成最后的免罪符。",
        )
        .tag("路线复盘")
        .tag("复谈");
    }

    state.complete_route_witness_debrief(debrief);
    let mut event = debrief.event(state.dialogue_tone);
    apply_debrief_rewards(state, debrief, &mut event);
    event
}

impl RouteWitnessDebriefId {
    pub const ALL: [Self; 6] = [
        Self::AloneSeatTraveler,
        Self::ChildSeatChild,
        Self::AshListClerk,
        Self::BroadcastDryRunKeeper,
        Self::BoundaryLampKeeper,
        Self::LastNoticeTraveler,
    ];

    fn witness(self) -> RouteWitnessId {
        match self {
            Self::AloneSeatTraveler => RouteWitnessId::AloneSeatNotice,
            Self::ChildSeatChild => RouteWitnessId::ChildWindowSeat,
            Self::AshListClerk => RouteWitnessId::TimetableAshList,
            Self::BroadcastDryRunKeeper => RouteWitnessId::BroadcastDryRun,
            Self::BoundaryLampKeeper => RouteWitnessId::KeeperBoundaryLamp,
            Self::LastNoticeTraveler => RouteWitnessId::LastNoticeTicket,
        }
    }

    fn dialogue(self) -> DialogueId {
        match self {
            Self::AloneSeatTraveler | Self::LastNoticeTraveler => DialogueId::Traveler,
            Self::ChildSeatChild => DialogueId::Child,
            Self::AshListClerk => DialogueId::Clerk,
            Self::BroadcastDryRunKeeper | Self::BoundaryLampKeeper => DialogueId::Keeper,
        }
    }

    fn visible(self, state: &GameState) -> bool {
        state.has_visited_route_witness(self.witness())
            || state.has_completed_route_witness_debrief(self)
    }

    fn label(self) -> &'static str {
        match self {
            Self::AloneSeatTraveler => "路线复盘：问老人空座说明有没有用",
            Self::ChildSeatChild => "路线复盘：问他靠窗座位像不像明天",
            Self::AshListClerk => "路线复盘：让售票员检查火前清单",
            Self::BroadcastDryRunKeeper => "路线复盘：问站务员试播是否太轻",
            Self::BoundaryLampKeeper => "路线复盘：确认雾灯边界能否执行",
            Self::LastNoticeTraveler => "路线复盘：问老人完整湿票会不会害人",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::AloneSeatTraveler => "空座说明的复盘",
            Self::ChildSeatChild => "靠窗座位的复盘",
            Self::AshListClerk => "火前清单的复盘",
            Self::BroadcastDryRunKeeper => "试播后的复盘",
            Self::BoundaryLampKeeper => "照明边界的复盘",
            Self::LastNoticeTraveler => "完整湿票的复盘",
        }
    }

    fn witness_title(self) -> &'static str {
        match self {
            Self::AloneSeatTraveler => "07B 空座下的说明",
            Self::ChildSeatChild => "靠窗座位的试坐",
            Self::AshListClerk => "火前失物清点",
            Self::BroadcastDryRunKeeper => "不播出的试播",
            Self::BoundaryLampKeeper => "雾灯照明边界",
            Self::LastNoticeTraveler => "月台灯下的完整湿票",
        }
    }

    fn detail(self, state: &GameState) -> String {
        if state.has_completed_route_witness_debrief(self) {
            return "这段路线现场已经复盘过，后续会进入结局余波。".to_string();
        }
        format!(
            "需要先完成路线现场「{}」。{}",
            self.witness_title(),
            self.prompt()
        )
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::AloneSeatTraveler => "老人会判断这份说明到底是在告别，还是又一次自我辩护。",
            Self::ChildSeatChild => "孩子会告诉你，靠窗座位是否真的给他保留了拒绝的余地。",
            Self::AshListClerk => "售票员会检查你是否把善后当成了火焰的装饰。",
            Self::BroadcastDryRunKeeper => "站务员会追问，沉默试播是否只是害怕开口。",
            Self::BoundaryLampKeeper => "站务员会要求边界明天也能被执行，而不是只在今晚好看。",
            Self::LastNoticeTraveler => "老人会提醒你，完整警告也可能变成另一种吓阻。",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::AloneSeatTraveler => "老人承认空座说明有用，但只在你不把它当成赦免时有用。",
            Self::ChildSeatChild => "孩子说靠窗座位像明天，因为它等他，也允许他不上车。",
            Self::AshListClerk => "售票员确认火前清单能保护小事，但不能替火焰证明清白。",
            Self::BroadcastDryRunKeeper => "站务员承认试播太轻，却正因为轻，才没有压住后来者。",
            Self::BoundaryLampKeeper => "站务员答应明天也检查边界，让照明不再偷偷扩大成管辖。",
            Self::LastNoticeTraveler => "老人承认完整湿票可能害怕人，但隐瞒岔口更残忍。",
        }
    }

    fn event(self, tone: DialogueTone) -> StoryEvent {
        let (title, body) = match (self, tone) {
            (Self::ChildSeatChild, DialogueTone::Gentle) => (
                "路线复盘：靠窗座位像不像明天",
                "你问他，刚才那张靠窗座位像不像明天。孩子想了很久，说像一点点，因为它没有逼我坐下。你说那就让它继续等。他点头：如果明天也是这样，也许我可以先把窗户打开。",
            ),
            (Self::BoundaryLampKeeper, DialogueTone::Direct) => (
                "路线复盘：边界明天也要有效",
                "你问站务员：这条雾灯边界，明天还算数吗？他没有立刻答应。你等着。最后他把粉笔线又描了一遍，说算数，而且如果我越线，你要把这页撕下来贴在门口。",
            ),
            (Self::LastNoticeTraveler, DialogueTone::Listening) => (
                "路线复盘：完整警告也会伤人",
                "老人听完月台灯下的完整湿票，没有马上评判。他只问：你知道有些人看见岔口会更害怕吗？你点头。他说那就好，警告不是为了让后来者勇敢，是为了让他们知道害怕时也还在选择。",
            ),
            _ => (self.label(), self.review()),
        };
        StoryEvent::new(title, body)
            .tag("路线复盘")
            .tag(format!("现场：{}", self.witness_title()))
            .tag(format!("对象：{}", self.dialogue().title()))
    }
}

fn apply_debrief_rewards(
    state: &mut GameState,
    debrief: RouteWitnessDebriefId,
    event: &mut StoryEvent,
) {
    match debrief {
        RouteWitnessDebriefId::AloneSeatTraveler => {
            state.remember(Flag::TravelerTrusted);
            event.tags.push("复盘：老人见证".to_string());
        }
        RouteWitnessDebriefId::ChildSeatChild => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = 5;
            event.tags.push("复盘：孩子明天".to_string());
        }
        RouteWitnessDebriefId::AshListClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("复盘：火前清单".to_string());
        }
        RouteWitnessDebriefId::BroadcastDryRunKeeper => {
            state.remember(Flag::HeardBroadcastTape);
            event.tags.push("复盘：广播试播".to_string());
        }
        RouteWitnessDebriefId::BoundaryLampKeeper => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("复盘：守夜边界".to_string());
        }
        RouteWitnessDebriefId::LastNoticeTraveler => {
            state.remember(Flag::UnderstoodFirstLoop);
            event.tags.push("复盘：完整警告".to_string());
        }
    }
}

fn debrief_progress(visible: bool, ready: bool, completed: bool) -> u8 {
    if completed {
        100
    } else if ready {
        70
    } else if visible {
        45
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ActiveDialogue, DialogueNodeId};

    #[test]
    fn debrief_unlocks_inside_active_dialogue_after_route_witness() {
        let mut state = GameState::new();
        state.visit_route_witness(RouteWitnessId::ChildWindowSeat);
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
        });

        let actions = available_debriefs(&state, state.active_dialogue.unwrap());
        assert!(actions
            .iter()
            .any(|action| action.debrief == RouteWitnessDebriefId::ChildSeatChild));
        assert!(active_debrief_objective_hint(&state).is_some_and(|hint| hint.contains("路线现场")));

        let event = debrief(&mut state, RouteWitnessDebriefId::ChildSeatChild);
        assert!(state.has_completed_route_witness_debrief(RouteWitnessDebriefId::ChildSeatChild));
        assert!(event.tags.iter().any(|tag| tag == "路线复盘"));

        let summary = debrief_summaries(&state)
            .into_iter()
            .find(|summary| summary.debrief == RouteWitnessDebriefId::ChildSeatChild)
            .expect("route witness debrief summary should exist");
        assert_eq!(summary.status, "已复盘");
        assert_eq!(summary.progress, 100);
    }
}
