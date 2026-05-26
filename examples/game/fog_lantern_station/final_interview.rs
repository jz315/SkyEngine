use crate::model::{
    ActiveDialogue, DialogueId, DialogueTone, FinalInterviewId, Flag, GameState,
    RouteWitnessDebriefId, StoryEvent,
};

pub const FINAL_INTERVIEW_COUNT: usize = FinalInterviewId::ALL.len();

#[derive(Clone, Debug)]
pub struct FinalInterviewAction {
    pub interview: FinalInterviewId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FinalInterviewSummary {
    pub interview: FinalInterviewId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub completed: bool,
}

pub fn available_interviews(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<FinalInterviewAction> {
    FinalInterviewId::ALL
        .iter()
        .copied()
        .filter(|interview| interview.dialogue() == active.dialogue)
        .filter(|interview| interview.visible(state))
        .filter(|interview| !state.has_completed_final_interview(*interview))
        .map(|interview| {
            let missing = missing_requirements(state, interview);
            FinalInterviewAction {
                interview,
                label: interview.label(),
                detail: if missing.is_empty() {
                    interview.detail().to_string()
                } else {
                    format!("还缺：{}。{}", missing.join("；"), interview.detail())
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn interview_summaries(state: &GameState) -> Vec<FinalInterviewSummary> {
    FinalInterviewId::ALL
        .iter()
        .copied()
        .map(|interview| {
            let completed = state.has_completed_final_interview(interview);
            let visible = completed || interview.visible(state);
            let missing = missing_requirements(state, interview);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == interview.dialogue())
                && visible
                && missing.is_empty()
                && !completed;
            let status = if completed {
                "已长谈"
            } else if ready {
                "可长谈"
            } else if visible {
                "待对话"
            } else {
                "未抵达"
            };
            let detail = if completed {
                interview.review().to_string()
            } else if ready {
                format!(
                    "{}正在等你把今晚真正谈完：{}",
                    interview.dialogue().title(),
                    interview.detail()
                )
            } else if visible && missing.is_empty() {
                format!(
                    "进入{}的主动对话，进行终局前长谈：{}。",
                    interview.dialogue().title(),
                    interview.title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                "第八段午夜会把已见证、已复盘的路线推回人物身边。继续推进路线现场、路线复盘或核心真相。"
                    .to_string()
            };
            FinalInterviewSummary {
                interview,
                title: interview.title(),
                status,
                detail,
                progress: interview_progress(interview, missing.len(), visible, completed),
                visible,
                ready,
                completed,
            }
        })
        .collect()
}

pub fn active_interview_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_interviews(state, active)
        .into_iter()
        .find(|interview| interview.enabled)
        .map(|interview| format!("可以在当前对话里进入终局前长谈：{}。", interview.label))
}

pub fn interview_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_interview_objective_hint(state);
    }
    FinalInterviewId::ALL
        .iter()
        .copied()
        .filter(|interview| interview.visible(state))
        .filter(|interview| !state.has_completed_final_interview(*interview))
        .filter(|interview| missing_requirements(state, *interview).is_empty())
        .find(|interview| interview.dialogue().location() == state.location)
        .map(|interview| {
            format!(
                "可以进入{}的对话，进行终局前长谈：{}。",
                interview.dialogue().title(),
                interview.title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.completed_final_interviews.is_empty() {
        return None;
    }
    let notes = state
        .completed_final_interviews
        .iter()
        .map(|interview| interview.title())
        .collect::<Vec<_>>()
        .join(" / ");
    Some(format!(
        "列车进站前，你没有把人留在系统清单里，而是完成了这些终局前长谈：{notes}。"
    ))
}

pub fn hold(state: &mut GameState, interview: FinalInterviewId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "长谈没有对象",
            "终局前的长谈不是独白。它必须发生在一个仍然愿意听你把话说完的人面前。",
        )
        .tag("终局前长谈");
    };

    if active.dialogue != interview.dialogue() {
        return StoryEvent::new(
            "长谈对象不对",
            format!(
                "你正和{}说话，但这场长谈应该留给{}。",
                active.dialogue.title(),
                interview.dialogue().title()
            ),
        )
        .tag("终局前长谈");
    }

    let missing = missing_requirements(state, interview);
    if !interview.visible(state) || !missing.is_empty() {
        return StoryEvent::new(
            "长谈还没有抵达",
            format!(
                "这句话太早了。{}",
                if missing.is_empty() {
                    "继续把路线、现场和真相推到第八段午夜。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("终局前长谈");
    }

    if state.has_completed_final_interview(interview) {
        return StoryEvent::new(
            "长谈已经发生",
            "这场谈话已经留在你们之间。重复它不会让结局更轻，只会提醒你：最后一步仍然要走。 ",
        )
        .tag("终局前长谈")
        .tag("复谈");
    }

    state.complete_final_interview(interview);
    let mut event = interview.event(state.dialogue_tone);
    apply_interview_rewards(state, interview, &mut event);
    event
}

impl FinalInterviewId {
    pub const ALL: [Self; 5] = [
        Self::TravelerEmptySeat,
        Self::ClerkAfterRules,
        Self::ChildOrdinaryTomorrow,
        Self::KeeperBroadcastRoom,
        Self::KeeperCoatBoundary,
    ];

    fn dialogue(self) -> DialogueId {
        match self {
            Self::TravelerEmptySeat => DialogueId::Traveler,
            Self::ClerkAfterRules => DialogueId::Clerk,
            Self::ChildOrdinaryTomorrow => DialogueId::Child,
            Self::KeeperBroadcastRoom | Self::KeeperCoatBoundary => DialogueId::Keeper,
        }
    }

    fn visible(self, state: &GameState) -> bool {
        state.has_completed_final_interview(self)
            || state.current_segment() >= 8
            || self.has_anchor(state)
    }

    fn has_anchor(self, state: &GameState) -> bool {
        match self {
            Self::TravelerEmptySeat => {
                state.has_completed_route_witness_debrief(RouteWitnessDebriefId::AloneSeatTraveler)
                    || state.has_completed_route_witness_debrief(
                        RouteWitnessDebriefId::LastNoticeTraveler,
                    )
            }
            Self::ClerkAfterRules => {
                state.has_completed_route_witness_debrief(RouteWitnessDebriefId::AshListClerk)
                    || state.has_flag(Flag::SynthesizedStationTruth)
            }
            Self::ChildOrdinaryTomorrow => {
                state.has_completed_route_witness_debrief(RouteWitnessDebriefId::ChildSeatChild)
                    || state.has_flag(Flag::SynthesizedChildTruth)
            }
            Self::KeeperBroadcastRoom => {
                state.has_completed_route_witness_debrief(
                    RouteWitnessDebriefId::BroadcastDryRunKeeper,
                ) || state.has_flag(Flag::HeardBroadcastTape)
            }
            Self::KeeperCoatBoundary => {
                state.has_completed_route_witness_debrief(RouteWitnessDebriefId::BoundaryLampKeeper)
                    || state.has_flag(Flag::UnderstoodStationMechanism)
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TravelerEmptySeat => "终局前长谈：老人和空座",
            Self::ClerkAfterRules => "终局前长谈：售票员和规则之后",
            Self::ChildOrdinaryTomorrow => "终局前长谈：孩子和普通明天",
            Self::KeeperBroadcastRoom => "终局前长谈：站务员和广播室",
            Self::KeeperCoatBoundary => "终局前长谈：站务员和外套边界",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::TravelerEmptySeat => "老人和空座",
            Self::ClerkAfterRules => "规则之后的窗口",
            Self::ChildOrdinaryTomorrow => "普通明天",
            Self::KeeperBroadcastRoom => "广播室的空白",
            Self::KeeperCoatBoundary => "外套的边界",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::TravelerEmptySeat => "把空座、湿票和你仍想离开的事实一次说完。",
            Self::ClerkAfterRules => "问她如果规则被撤销，窗口还能不能继续保护人。",
            Self::ChildOrdinaryTomorrow => "不谈拯救，只谈明天早晨他想先看见什么。",
            Self::KeeperBroadcastRoom => "确认警告、沉默和广播室之间还剩多少人的余地。",
            Self::KeeperCoatBoundary => "在披上外套或拒绝外套以前，谈清楚守夜的边界。",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::TravelerEmptySeat => "老人没有替你宽恕空座，只确认你终于不再把它写没。",
            Self::ClerkAfterRules => "售票员承认规则之后仍要有人守窗口，但窗口不能再冒充命运。",
            Self::ChildOrdinaryTomorrow => "孩子把明天说得很小：窗户、热水、可以发脾气的一天。",
            Self::KeeperBroadcastRoom => "站务员承认广播室需要空白，提醒不能把后来者的句子填满。",
            Self::KeeperCoatBoundary => "站务员把外套边界说成职责，而不是牺牲的奖章。",
        }
    }

    fn event(self, tone: DialogueTone) -> StoryEvent {
        let (title, body) = match (self, tone) {
            (Self::ChildOrdinaryTomorrow, DialogueTone::Gentle) => (
                "终局前长谈：普通明天",
                "你问他明天想先看见什么。孩子没有说学校、家、原谅，也没有说宏大的东西。他说想看见窗户上有没有雾，想喝一杯不烫嘴的水，想有一天可以发脾气而不被送回这里。你说好，明天可以先从这些小事开始。",
            ),
            (Self::KeeperBroadcastRoom, DialogueTone::Listening) => (
                "终局前长谈：广播室的空白",
                "你没有问站务员该不该走进广播室，只听旧钟和磁带之间那段空白。他终于说：如果你留下声音，请不要把每个沉默都填满。后来者需要警告，也需要误解、迟疑和自己说错话的空间。",
            ),
            (Self::TravelerEmptySeat, DialogueTone::Direct) => (
                "终局前长谈：老人和空座",
                "你对老人说：我可能还是想离开。老人把报纸合上，说这次你至少没有把想离开说成替所有人好。空座在那里，仍然空着，但它第一次不像证据消失的地方，更像一个你不再敢随便解释的位置。",
            ),
            _ => (self.label(), self.review()),
        };
        StoryEvent::new(title, body)
            .tag("终局前长谈")
            .tag(format!("对象：{}", self.dialogue().title()))
    }
}

fn missing_requirements(state: &GameState, interview: FinalInterviewId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(&mut missing, state.current_segment() >= 8, "进入第八段午夜");
    require(
        &mut missing,
        interview.has_anchor(state),
        "完成相关路线复盘或核心真相",
    );
    missing
}

fn apply_interview_rewards(
    state: &mut GameState,
    interview: FinalInterviewId,
    event: &mut StoryEvent,
) {
    match interview {
        FinalInterviewId::TravelerEmptySeat => {
            state.remember(Flag::TravelerTrusted);
            event.tags.push("长谈：空座".to_string());
        }
        FinalInterviewId::ClerkAfterRules => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("长谈：规则之后".to_string());
        }
        FinalInterviewId::ChildOrdinaryTomorrow => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = 5;
            event.tags.push("长谈：普通明天".to_string());
        }
        FinalInterviewId::KeeperBroadcastRoom => {
            state.remember(Flag::HeardBroadcastTape);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("长谈：广播室".to_string());
        }
        FinalInterviewId::KeeperCoatBoundary => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("长谈：外套边界".to_string());
        }
    }
}

fn interview_progress(
    interview: FinalInterviewId,
    missing_count: usize,
    visible: bool,
    completed: bool,
) -> u8 {
    if completed {
        100
    } else if !visible {
        0
    } else {
        let total = 2;
        let base = ((total - missing_count.min(total)) * 100) / total;
        if interview == FinalInterviewId::KeeperCoatBoundary && base > 0 {
            base.max(50) as u8
        } else {
            base as u8
        }
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
    use crate::model::{ActiveDialogue, DialogueNodeId, MINUTES_PER_SEGMENT};

    #[test]
    fn final_interview_unlocks_inside_active_dialogue_in_eighth_segment() {
        let mut state = GameState::new();
        state.actions_used = MINUTES_PER_SEGMENT * 7;
        state.complete_route_witness_debrief(RouteWitnessDebriefId::ChildSeatChild);
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
        });

        let actions = available_interviews(&state, state.active_dialogue.unwrap());
        assert!(actions
            .iter()
            .any(|action| action.interview == FinalInterviewId::ChildOrdinaryTomorrow));
        assert!(
            active_interview_objective_hint(&state).is_some_and(|hint| hint.contains("终局前长谈"))
        );

        let event = hold(&mut state, FinalInterviewId::ChildOrdinaryTomorrow);
        assert!(state.has_completed_final_interview(FinalInterviewId::ChildOrdinaryTomorrow));
        assert!(event.tags.iter().any(|tag| tag == "终局前长谈"));

        let summary = interview_summaries(&state)
            .into_iter()
            .find(|summary| summary.interview == FinalInterviewId::ChildOrdinaryTomorrow)
            .expect("final interview summary should exist");
        assert_eq!(summary.status, "已长谈");
        assert_eq!(summary.progress, 100);
    }
}
