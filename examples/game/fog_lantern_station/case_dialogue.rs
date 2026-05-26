use crate::model::{
    ActiveDialogue, CaseDialogueId, CaseFileId, DialogueId, DialogueTone, Flag, GameState,
    StoryEvent,
};

pub const CASE_DIALOGUE_COUNT: usize = CaseDialogueId::ALL.len();

#[derive(Clone, Debug)]
pub struct CaseDialogueAction {
    pub dialogue: CaseDialogueId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseDialogueSummary {
    pub dialogue: CaseDialogueId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub completed: bool,
}

pub fn available_case_dialogues(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<CaseDialogueAction> {
    CaseDialogueId::ALL
        .iter()
        .copied()
        .filter(|dialogue| dialogue.dialogue() == active.dialogue)
        .filter(|dialogue| dialogue.visible(state))
        .filter(|dialogue| !state.has_completed_case_dialogue(*dialogue))
        .map(|dialogue| CaseDialogueAction {
            dialogue,
            label: dialogue.label(),
            detail: dialogue.detail(state),
            enabled: true,
        })
        .collect()
}

pub fn case_dialogue_summaries(state: &GameState) -> Vec<CaseDialogueSummary> {
    CaseDialogueId::ALL
        .iter()
        .copied()
        .map(|dialogue| {
            let completed = state.has_completed_case_dialogue(dialogue);
            let visible = completed || dialogue.visible(state);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == dialogue.dialogue())
                && visible
                && !completed;
            let status = if completed {
                "已回谈"
            } else if ready {
                "可回谈"
            } else if visible {
                "待对话"
            } else {
                "未归档"
            };
            let detail = if completed {
                dialogue.review().to_string()
            } else if ready {
                format!(
                    "{}正在听。把《{}》摊开给 TA 看，让推理重新进入关系。",
                    dialogue.dialogue().title(),
                    dialogue.case_file().title()
                )
            } else if visible {
                format!(
                    "进入{}的主动对话，继续回谈《{}》。",
                    dialogue.dialogue().title(),
                    dialogue.case_file().title()
                )
            } else {
                format!(
                    "先归档《{}》。档案只有写完，人物才会愿意被它反问。",
                    dialogue.case_file().title()
                )
            };
            CaseDialogueSummary {
                dialogue,
                title: dialogue.title(),
                status,
                detail,
                progress: case_dialogue_progress(visible, ready, completed),
                visible,
                ready,
                completed,
            }
        })
        .collect()
}

pub fn active_case_dialogue_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_case_dialogues(state, active)
        .into_iter()
        .find(|dialogue| dialogue.enabled)
        .map(|dialogue| format!("可以在当前对话里回谈档案：{}。", dialogue.label))
}

pub fn case_dialogue_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_case_dialogue_objective_hint(state);
    }
    CaseDialogueId::ALL
        .iter()
        .copied()
        .filter(|dialogue| dialogue.visible(state))
        .filter(|dialogue| !state.has_completed_case_dialogue(*dialogue))
        .find(|dialogue| dialogue.dialogue().location() == state.location)
        .map(|dialogue| {
            format!(
                "可以进入{}的对话，回谈档案：{}。",
                dialogue.dialogue().title(),
                dialogue.title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.completed_case_dialogues.is_empty() {
        return None;
    }
    let notes = state
        .completed_case_dialogues
        .iter()
        .map(|dialogue| dialogue.title())
        .collect::<Vec<_>>()
        .join(" / ");
    Some(format!(
        "被归档的真相没有只留在纸面上。你把这些档案重新带回人物面前：{notes}。"
    ))
}

pub fn discuss(state: &mut GameState, dialogue: CaseDialogueId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "档案无人接住",
            "这份档案不能只在你心里翻页。它需要回到一个具体的人面前，才会从推理变成关系。",
        )
        .tag("档案回谈");
    };

    if active.dialogue != dialogue.dialogue() {
        return StoryEvent::new(
            "档案递错了窗口",
            format!(
                "你正和{}说话，但这份档案应该先拿给{}看。",
                active.dialogue.title(),
                dialogue.dialogue().title()
            ),
        )
        .tag("档案回谈");
    }

    if !state.has_resolved_case_file(dialogue.case_file()) {
        return StoryEvent::new(
            "档案还没有写完",
            format!(
                "《{}》还没有归档。现在拿出来，只会让纸页重新散回雾里。",
                dialogue.case_file().title()
            ),
        )
        .tag("档案回谈");
    }

    if state.has_completed_case_dialogue(dialogue) {
        return StoryEvent::new(
            "档案已经回谈过",
            "对方记得这份档案，也记得你当时没有把结论当成胜利。真正还没发生的，是你下一步怎么承担。",
        )
        .tag("档案回谈")
        .tag("复谈");
    }

    state.complete_case_dialogue(dialogue);
    let mut event = dialogue.event(state.dialogue_tone);
    apply_case_dialogue_rewards(state, dialogue, &mut event);
    event
}

impl CaseDialogueId {
    pub const ALL: [Self; 6] = [
        Self::WetTicketTraveler,
        Self::ReturnProtocolClerk,
        Self::ChildWitnessChild,
        Self::BorrowedMinuteKeeper,
        Self::BroadcastDoorKeeper,
        Self::KeeperContractClerk,
    ];

    fn case_file(self) -> CaseFileId {
        match self {
            Self::WetTicketTraveler => CaseFileId::WetTicketProtocol,
            Self::ReturnProtocolClerk => CaseFileId::ReturnProtocol,
            Self::ChildWitnessChild => CaseFileId::ChildWitness,
            Self::BorrowedMinuteKeeper => CaseFileId::BorrowedMinute,
            Self::BroadcastDoorKeeper => CaseFileId::BroadcastDoor,
            Self::KeeperContractClerk => CaseFileId::KeeperContract,
        }
    }

    fn dialogue(self) -> DialogueId {
        match self {
            Self::WetTicketTraveler => DialogueId::Traveler,
            Self::ReturnProtocolClerk => DialogueId::Clerk,
            Self::ChildWitnessChild => DialogueId::Child,
            Self::BorrowedMinuteKeeper | Self::BroadcastDoorKeeper => DialogueId::Keeper,
            Self::KeeperContractClerk => DialogueId::Clerk,
        }
    }

    fn visible(self, state: &GameState) -> bool {
        state.has_resolved_case_file(self.case_file()) || state.has_completed_case_dialogue(self)
    }

    fn label(self) -> &'static str {
        match self {
            Self::WetTicketTraveler => "档案回谈：把湿票警告摊给老人",
            Self::ReturnProtocolClerk => "档案回谈：让售票员承认两个名字",
            Self::ChildWitnessChild => "档案回谈：告诉孩子他不是证物",
            Self::BorrowedMinuteKeeper => "档案回谈：问站务员借来的分钟",
            Self::BroadcastDoorKeeper => "档案回谈：把广播室说成一扇门",
            Self::KeeperContractClerk => "档案回谈：追问外套和手续的债",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::WetTicketTraveler => "湿票警告回到老人手里",
            Self::ReturnProtocolClerk => "返程规则回到窗口",
            Self::ChildWitnessChild => "孩子档案回到孩子面前",
            Self::BorrowedMinuteKeeper => "借来的分钟回到旧钟前",
            Self::BroadcastDoorKeeper => "广播室档案回到站务员手里",
            Self::KeeperContractClerk => "站务员外套回到手续里",
        }
    }

    fn detail(self, state: &GameState) -> String {
        if state.has_completed_case_dialogue(self) {
            return "这份档案已经回谈过，后续会进入结局余波。".to_string();
        }
        format!(
            "需要已归档《{}》。{}",
            self.case_file().title(),
            self.prompt()
        )
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::WetTicketTraveler => "老人也许能说出“别上车”后面被剪掉的半句。",
            Self::ReturnProtocolClerk => "窗口需要承认，两个名字不是备注，而是返程本身。",
            Self::ChildWitnessChild => "把孩子从档案里的“线索”位置请出来，让他亲自反驳。",
            Self::BorrowedMinuteKeeper => "旧钟前的站务员必须承认，慈悲借出了时间，也收走了影子。",
            Self::BroadcastDoorKeeper => "广播室既像出口，也像替人说话的陷阱。",
            Self::KeeperContractClerk => "售票手续和站务外套都在保护秩序，也都可能吃掉具体的人。",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WetTicketTraveler => "老人承认警告不是命令，而是过去的你不敢完整交出的请求。",
            Self::ReturnProtocolClerk => "售票员把两个名字写进同一张返程表，不再把同行者写进备注。",
            Self::ChildWitnessChild => {
                "孩子亲自修改档案标题：不是证物，不是奖励，也不是任何人把账抹平的证明。"
            }
            Self::BorrowedMinuteKeeper => {
                "站务员承认最后一分钟有利息，善意若不还账，也会变成机关。"
            }
            Self::BroadcastDoorKeeper => {
                "广播室被重新定义成门而不是王座：声音只能提醒，不能替人选择。"
            }
            Self::KeeperContractClerk => "售票员承认手续会保护人，也会把照路的人慢慢写成一件制服。",
        }
    }

    fn event(self, tone: DialogueTone) -> StoryEvent {
        let (title, body) = match (self, tone) {
            (Self::WetTicketTraveler, DialogueTone::Direct) => (
                "档案回谈：别上车不是命令",
                "你把湿票档案推到老人面前，直接问他：你早就知道这三个字不是禁令，对吗？老人没有否认。他把报纸折好，说：对。它原本还有半句，别上车，除非你愿意记住车上还有谁。",
            ),
            (Self::ChildWitnessChild, DialogueTone::Gentle) => (
                "档案回谈：他修改自己的标题",
                "你把档案放到白线边，只说：这里写得还不够准确。孩子拿起铅笔，把“孩子证人”划掉，写成“我”。他说：这样才像一个人，不像你们大人互相传递的证明材料。",
            ),
            (Self::BroadcastDoorKeeper, DialogueTone::Listening) => (
                "档案回谈：广播室不是答案",
                "你没有急着问站务员，只把广播室档案翻到最后一页。旧钟替你们沉默了很久。站务员说：如果你进去，记得每一句警告都该留下空白，让后来者还能把自己的话说完。",
            ),
            _ => (self.label(), self.review()),
        };
        StoryEvent::new(title, body)
            .tag("档案回谈")
            .tag(format!("档案：{}", self.case_file().title()))
            .tag(format!("对象：{}", self.dialogue().title()))
    }
}

fn apply_case_dialogue_rewards(
    state: &mut GameState,
    dialogue: CaseDialogueId,
    event: &mut StoryEvent,
) {
    match dialogue {
        CaseDialogueId::WetTicketTraveler => {
            state.remember(Flag::TravelerTrusted);
            state.remember(Flag::UnderstoodFirstLoop);
            event.tags.push("老人信任".to_string());
        }
        CaseDialogueId::ReturnProtocolClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("返程手续".to_string());
        }
        CaseDialogueId::ChildWitnessChild => {
            state.remember(Flag::UnderstoodChildPromise);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("孩子信任".to_string());
        }
        CaseDialogueId::BorrowedMinuteKeeper => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("旧钟代价".to_string());
        }
        CaseDialogueId::BroadcastDoorKeeper => {
            state.remember(Flag::HeardBroadcastTape);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("广播室线索".to_string());
        }
        CaseDialogueId::KeeperContractClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("手续债务".to_string());
        }
    }
}

fn case_dialogue_progress(visible: bool, ready: bool, completed: bool) -> u8 {
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
    fn case_dialogue_unlocks_inside_active_dialogue_after_case_file() {
        let mut state = GameState::new();
        state.resolve_case_file(CaseFileId::ChildWitness);
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
        });

        let actions = available_case_dialogues(&state, state.active_dialogue.unwrap());
        assert!(actions
            .iter()
            .any(|action| action.dialogue == CaseDialogueId::ChildWitnessChild));
        assert!(active_case_dialogue_objective_hint(&state)
            .is_some_and(|hint| hint.contains("档案回谈")));

        let event = discuss(&mut state, CaseDialogueId::ChildWitnessChild);
        assert!(state.has_completed_case_dialogue(CaseDialogueId::ChildWitnessChild));
        assert!(event.tags.iter().any(|tag| tag == "档案回谈"));

        let summary = case_dialogue_summaries(&state)
            .into_iter()
            .find(|summary| summary.dialogue == CaseDialogueId::ChildWitnessChild)
            .expect("case dialogue summary should exist");
        assert_eq!(summary.status, "已回谈");
        assert_eq!(summary.progress, 100);
    }
}
