use crate::model::{
    CaseFileId, DialogueQuestionId, Flag, GameState, Item, Location, StoryEvent, TopicId,
    TruthSceneId, NPC_THREAD_STEPS,
};

pub const TRUTH_SCENE_COUNT: usize = TruthSceneId::ALL.len();

#[derive(Clone, Debug)]
pub struct TruthSceneAction {
    pub truth: TruthSceneId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct TruthSceneSummary {
    pub truth: TruthSceneId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub revealed: bool,
}

pub fn available_truth_scenes(state: &GameState) -> Vec<TruthSceneAction> {
    TruthSceneId::ALL
        .iter()
        .copied()
        .filter(|truth| truth.location() == state.location)
        .filter(|truth| truth.unlocked(state))
        .filter(|truth| !state.has_revealed_truth_scene(*truth))
        .map(|truth| TruthSceneAction {
            truth,
            label: format!("揭开真相：{}", truth.title()),
            detail: truth.action_detail().to_string(),
            enabled: true,
        })
        .collect()
}

pub fn truth_summaries(state: &GameState) -> Vec<TruthSceneSummary> {
    TruthSceneId::ALL
        .iter()
        .copied()
        .map(|truth| {
            let revealed = state.has_revealed_truth_scene(truth);
            let visible = truth.unlocked(state);
            let ready = visible && truth.location() == state.location && !revealed;
            let status = if revealed {
                "已揭开"
            } else if ready {
                "可揭开"
            } else if visible {
                "待前往"
            } else {
                "未成形"
            };
            let detail = if revealed {
                truth.review().to_string()
            } else if ready {
                truth.ready_detail().to_string()
            } else if visible {
                format!("去{}，让这层真相真正发生。", truth.location().title())
            } else {
                truth.missing_hint(state).to_string()
            };
            TruthSceneSummary {
                truth,
                title: truth.title(),
                status,
                detail,
                progress: truth_progress(visible, ready, revealed),
                visible,
                ready,
                revealed,
            }
        })
        .collect()
}

pub fn truth_objective_hint(state: &GameState) -> Option<String> {
    TruthSceneId::ALL
        .iter()
        .copied()
        .filter(|truth| truth.unlocked(state))
        .filter(|truth| !state.has_revealed_truth_scene(*truth))
        .find(|truth| truth.location() == state.location)
        .map(|truth| format!("这里可以揭开一层真相：{}。", truth.title()))
        .or_else(|| {
            TruthSceneId::ALL
                .iter()
                .copied()
                .filter(|truth| truth.unlocked(state))
                .filter(|truth| !state.has_revealed_truth_scene(*truth))
                .next()
                .map(|truth| {
                    format!(
                        "去{}揭开中段真相：{}。",
                        truth.location().title(),
                        truth.title()
                    )
                })
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.revealed_truth_scenes.is_empty() {
        return None;
    }

    let truths = state
        .revealed_truth_scenes
        .iter()
        .map(|truth| truth.title())
        .collect::<Vec<_>>();
    Some(format!(
        "这些真相不是在终点才突然出现的。你已经亲手揭开：{}。",
        truths.join("、")
    ))
}

pub fn reveal(state: &mut GameState, truth: TruthSceneId) -> StoryEvent {
    if state.has_revealed_truth_scene(truth) {
        return StoryEvent::new(
            "真相已经揭开",
            format!("{}已经留在你的记录里，不会再伪装成巧合。", truth.title()),
        )
        .tag("真相揭露")
        .tag("复看");
    }

    if !truth.unlocked(state) {
        return StoryEvent::new("真相还没有形状", truth.missing_hint(state)).tag("真相揭露");
    }

    if truth.location() != state.location {
        return StoryEvent::new(
            "地点不对",
            format!(
                "这层真相必须回到{}才能发生。雾灯站不允许谜底脱离它的现场。",
                truth.location().title()
            ),
        )
        .tag("真相揭露");
    }

    state.reveal_truth_scene(truth);
    let mut event = truth.event();
    apply_truth_rewards(state, truth, &mut event);
    event
}

impl TruthSceneId {
    pub const ALL: [Self; 5] = [
        Self::WetTicketWarning,
        Self::ChildIsNotCargo,
        Self::ReturnTicketSignature,
        Self::StationFeedsOnLastMinute,
        Self::BroadcastIsAPerson,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::WetTicketWarning => Location::WaitingHall,
            Self::ChildIsNotCargo => Location::Platform,
            Self::ReturnTicketSignature => Location::TicketOffice,
            Self::StationFeedsOnLastMinute => Location::ClockTower,
            Self::BroadcastIsAPerson => Location::ClockTower,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::WetTicketWarning => "湿票是你写给自己的警告",
            Self::ChildIsNotCargo => "白线是你留下的命令",
            Self::ReturnTicketSignature => "返程票需要撤销旧命令",
            Self::StationFeedsOnLastMinute => "车站靠最后一分钟维持循环",
            Self::BroadcastIsAPerson => "广播室里留下的是一个人",
        }
    }

    fn action_detail(self) -> &'static str {
        match self {
            Self::WetTicketWarning => "把湿票、电子屏和候车厅缺席的 07 号座位放到同一张桌上。",
            Self::ChildIsNotCargo => "在三号月台重读孩子、姓名牌和作业本，确认白线为什么困住他。",
            Self::ReturnTicketSignature => {
                "让售票窗口承认：返程不是买票，而是撤销那条以保护为名的命令。"
            }
            Self::StationFeedsOnLastMinute => {
                "把站务日志、旧钟和第一次循环合在一起，逼近车站机制。"
            }
            Self::BroadcastIsAPerson => "沿着广播磁带和钟楼线路，确认那个声音曾经是谁。",
        }
    }

    fn ready_detail(self) -> &'static str {
        match self {
            Self::WetTicketWarning => "线索已经足够。湿票背面的“别上车”可以不再只是恐吓。",
            Self::ChildIsNotCargo => {
                "孩子线索已经聚齐。现在要承认：白线不是天生的规则，是你当年亲口加固的命令。"
            }
            Self::ReturnTicketSignature => "窗口、铜筹和返程规则已经能互相作证。代价可以被说清楚。",
            Self::StationFeedsOnLastMinute => "旧钟和日志正在互相指认。车站不再只是梦境布景。",
            Self::BroadcastIsAPerson => "广播磁带接上钟楼线路以后，声音背后的人快要显形。",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WetTicketWarning => {
                "你确认湿票是上一轮的你留下的刹车，而不是命运随手塞来的纸条。它阻止的不是列车，而是你再次重演 07A 的逃离。"
            }
            Self::ChildIsNotCargo => "你确认孩子不是结局奖励；他被困住，是因为他把你的命令当成了最后的安全。",
            Self::ReturnTicketSignature => "你确认返程票不是逃生券，它需要你承认 07A 的逃离，也需要孩子不再被 07B 的空白管理。",
            Self::StationFeedsOnLastMinute => {
                "你确认雾灯站靠最后一分钟、未归档姓名和未完成承诺喂养循环。"
            }
            Self::BroadcastIsAPerson => {
                "你确认广播室里的警告来自曾经留下的人，而不是冷冰冰的系统。"
            }
        }
    }

    fn unlocked(self, state: &GameState) -> bool {
        match self {
            Self::WetTicketWarning => {
                state.has_flag(Flag::ExaminedTicket)
                    && (state.has_flag(Flag::ReadDepartureBoard)
                        || state.has_item(Item::CoinToken)
                        || state.has_resolved_case_file(CaseFileId::WetTicketProtocol))
            }
            Self::ChildIsNotCargo => {
                state.has_flag(Flag::MetChild)
                    && (state.has_item(Item::NameTag)
                        || state.has_item(Item::ChildHomework)
                        || state.has_flag(Flag::RecoveredName)
                        || state
                            .has_answered_dialogue_question(DialogueQuestionId::ChildAboutLeaving))
            }
            Self::ReturnTicketSignature => {
                (state.has_item(Item::CoinToken)
                    && (state.has_flag(Flag::ClerkMet)
                        || state.has_answered_dialogue_question(
                            DialogueQuestionId::ClerkAboutWetTicket,
                        )
                        || state
                            .has_answered_dialogue_question(DialogueQuestionId::ClerkAboutName)
                        || state.has_discussed(TopicId::ClerkPrice)))
                    || state.ticket.name().contains("返程")
            }
            Self::StationFeedsOnLastMinute => {
                state.has_item(Item::StationLog)
                    && (state.has_flag(Flag::HeardClockTruth)
                        || state.has_flag(Flag::UnderstoodStationMechanism))
                    && (state.has_flag(Flag::UnderstoodFirstLoop)
                        || state.has_flag(Flag::AlignedClock))
            }
            Self::BroadcastIsAPerson => {
                state.has_item(Item::BroadcastTape)
                    && (state.has_flag(Flag::AlignedClock)
                        || state.has_flag(Flag::HeardBroadcastTape)
                        || state.has_resolved_case_file(CaseFileId::BroadcastDoor))
            }
        }
    }

    fn missing_hint(self, state: &GameState) -> &'static str {
        match self {
            Self::WetTicketWarning if !state.has_flag(Flag::ExaminedTicket) => {
                "先检查湿票。警告只有被读完以后，才会承认作者。"
            }
            Self::WetTicketWarning => "还需要电子屏、退票铜筹或湿票档案来把警告固定住。",
            Self::ChildIsNotCargo if !state.has_flag(Flag::MetChild) => {
                "先去三号月台见到孩子。他不能只活在你的猜测里。"
            }
            Self::ChildIsNotCargo => "还需要姓名牌、作业本、回声或孩子亲口谈离开。",
            Self::ReturnTicketSignature => "还需要退票铜筹，以及售票员或返程代价的明确证词。",
            Self::StationFeedsOnLastMinute => {
                "还需要站务日志、旧钟真相，以及第一次循环或校准旧钟的证据。"
            }
            Self::BroadcastIsAPerson => "还需要广播磁带，并让旧钟或广播室线路回应它。",
        }
    }

    fn event(self) -> StoryEvent {
        let (title, body) = match self {
            Self::WetTicketWarning => (
                "真相：湿票是你写给自己的警告",
                "你把湿票摊在长椅上。电子屏那班不存在的车从水痕里晃过去，像一尾黑鱼。背面的“别上车”划得很浅，末笔发抖。票角只露出 07A；另一处编号被雨泡开，看不清了。",
            ),
            Self::ChildIsNotCargo => (
                "真相：白线是你留下的命令",
                "三号月台的白线在你脚边起了皮，雨水把白漆泡软。姓名牌、作业本和孩子一直没问完的问题挤到同一秒里：你听见自己的声音说，站在这里，不准动。白线后的鞋尖仍压着漆边。你欠他的不是把人抱走，而是当面说：这句话作废了。",
            ),
            Self::ReturnTicketSignature => (
                "真相：返程票需要撤销旧命令",
                "售票窗口的玻璃映出两张并排的座位。退票铜筹滚到票章旁边，发出很轻的一声。07A 的墨迹被雨泡深，07B 那一栏却干净得刺眼。售票员没有催你，只把打孔机推近一点，等你先说出那张空位属于谁。",
            ),
            Self::StationFeedsOnLastMinute => (
                "真相：车站靠最后一分钟维持循环",
                "站务日志摊在旧钟下，纸页边缘被齿轮阴影切成一格一格。每一页都停在 23:59。有人在页脚写过申请，有人按过手印，有人把名字划掉又重新写上。旧钟没有审判谁，它只是一次次把最后一分钟借出去，直到借据堆满抽屉。",
            ),
            Self::BroadcastIsAPerson => (
                "真相：广播室里留下的是一个人",
                "广播磁带在钟楼线路旁轻轻转动。杂音底下不是系统提示，而是一口练习太多遍的气：别上车，别命令别人替你害怕。那声音曾经有名字，后来只剩喇叭里的电流声。它每晚都响一次，像有人还在门里敲墙。",
            ),
        };
        StoryEvent::new(title, body)
            .tag("真相揭露")
            .tag(self.title())
    }
}

fn truth_progress(visible: bool, ready: bool, revealed: bool) -> u8 {
    if revealed {
        100
    } else if ready {
        70
    } else if visible {
        45
    } else {
        0
    }
}

fn apply_truth_rewards(state: &mut GameState, truth: TruthSceneId, event: &mut StoryEvent) {
    state.synthesis_depth = state
        .synthesis_depth
        .saturating_add(1)
        .min(NPC_THREAD_STEPS);
    match truth {
        TruthSceneId::WetTicketWarning => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "真相：第一次循环");
            remember_tag(state, event, Flag::SynthesizedRoute, "真相：湿票作者");
        }
        TruthSceneId::ChildIsNotCargo => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "真相：孩子也能说不",
            );
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "真相：孩子与姓名",
            );
        }
        TruthSceneId::ReturnTicketSignature => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "真相：返程签名");
        }
        TruthSceneId::StationFeedsOnLastMinute => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "真相：车站机制",
            );
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "真相：最后一分钟",
            );
        }
        TruthSceneId::BroadcastIsAPerson => {
            remember_tag(state, event, Flag::HeardBroadcastTape, "真相：广播人格");
            remember_tag(state, event, Flag::SynthesizedStationTruth, "真相：广播室");
        }
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truth_scenes_unlock_from_midgame_evidence_and_reveal_flags() {
        let mut state = GameState::new();
        state.location = Location::WaitingHall;
        state.remember(Flag::ExaminedTicket);
        state.remember(Flag::ReadDepartureBoard);

        let actions = available_truth_scenes(&state);
        assert!(actions
            .iter()
            .any(|action| action.truth == TruthSceneId::WetTicketWarning));

        let event = reveal(&mut state, TruthSceneId::WetTicketWarning);
        assert!(state.has_revealed_truth_scene(TruthSceneId::WetTicketWarning));
        assert!(state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(event.tags.iter().any(|tag| tag == "真相揭露"));

        let summary = truth_summaries(&state)
            .into_iter()
            .find(|summary| summary.truth == TruthSceneId::WetTicketWarning)
            .expect("truth summary should exist");
        assert_eq!(summary.status, "已揭开");
        assert_eq!(summary.progress, 100);
    }

    #[test]
    fn truth_scene_count_matches_declared_table() {
        assert_eq!(TRUTH_SCENE_COUNT, TruthSceneId::ALL.len());
    }

    #[test]
    fn core_truth_names_the_white_line_as_the_old_command() {
        let event = TruthSceneId::ChildIsNotCargo.event();
        assert_eq!(event.title, "真相：白线是你留下的命令");
        assert!(event.body.contains("站在这里，不准动"));
        assert!(event.body.contains("这句话作废了"));

        let return_event = TruthSceneId::ReturnTicketSignature.event();
        assert!(return_event.body.contains("07A"));
        assert!(return_event.body.contains("07B"));
        assert!(return_event.body.contains("空位属于谁"));
    }
}
