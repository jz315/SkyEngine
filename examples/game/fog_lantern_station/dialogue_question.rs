use crate::model::{
    ActiveDialogue, DialogueId, DialogueQuestionId, DialogueTone, Flag, GameState, Item,
    StoryEvent, NPC_THREAD_STEPS,
};

pub const DIALOGUE_QUESTION_COUNT: usize = DialogueQuestionId::ALL.len();

#[derive(Clone, Debug)]
pub struct DialogueQuestionAction {
    pub question: DialogueQuestionId,
    pub label: &'static str,
    pub detail: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueQuestionSummary {
    pub question: DialogueQuestionId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub ready: bool,
    pub answered: bool,
}

pub fn available_questions(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<DialogueQuestionAction> {
    DialogueQuestionId::ALL
        .iter()
        .copied()
        .filter(|question| question.dialogue() == active.dialogue)
        .filter(|question| question.visible(state))
        .filter(|question| !state.has_answered_dialogue_question(*question))
        .map(|question| DialogueQuestionAction {
            question,
            label: question.label(),
            detail: question.detail(),
            enabled: true,
        })
        .collect()
}

pub fn question_summaries(state: &GameState) -> Vec<DialogueQuestionSummary> {
    DialogueQuestionId::ALL
        .iter()
        .copied()
        .map(|question| {
            let answered = state.has_answered_dialogue_question(question);
            let visible = question.visible(state);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == question.dialogue())
                && visible
                && !answered;
            let status = if answered {
                "已问"
            } else if ready {
                "可询问"
            } else if visible {
                "待对话"
            } else {
                "未显形"
            };
            let detail = if answered {
                question.review().to_string()
            } else if ready {
                format!(
                    "{}正在听。现在可以把这个问题问出口，让对话跟随已发现的证词改变。",
                    question.dialogue().title()
                )
            } else if visible {
                format!(
                    "去{}进入{}的主动对话，再提出这个自由询问。",
                    question.dialogue().location().title(),
                    question.dialogue().title()
                )
            } else {
                question.missing_hint().to_string()
            };
            DialogueQuestionSummary {
                question,
                title: question.title(),
                status,
                detail,
                progress: question_progress(visible, ready, answered),
                visible,
                ready,
                answered,
            }
        })
        .collect()
}

pub fn active_question_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_questions(state, active)
        .into_iter()
        .find(|question| question.enabled)
        .map(|question| format!("可以在当前对话里自由询问：{}。", question.label))
}

pub fn question_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_question_objective_hint(state);
    }
    DialogueQuestionId::ALL
        .iter()
        .copied()
        .filter(|question| question.visible(state))
        .filter(|question| !state.has_answered_dialogue_question(*question))
        .find(|question| question.dialogue().location() == state.location)
        .map(|question| {
            format!(
                "可以进入{}的对话，提出自由询问：{}。",
                question.dialogue().title(),
                question.title()
            )
        })
}

pub fn ask(state: &mut GameState, question: DialogueQuestionId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "没有正在进行的自由询问",
            "这个问题需要一个正在听的人。雾灯站只保存问出口的话，不保存你在心里排练的句子。",
        )
        .tag("自由询问");
    };

    if question.dialogue() != active.dialogue {
        return StoryEvent::new(
            "问题问错了人",
            format!(
                "你正和{}说话，但这个问题应该留给{}。",
                active.dialogue.title(),
                question.dialogue().title()
            ),
        )
        .tag("自由询问");
    }

    if !question.visible(state) {
        return StoryEvent::new(
            "问题还没有证据支撑",
            "这句话还没有从车站里长出来。先去找能让它成立的物件、回声或证词。",
        )
        .tag("自由询问");
    }

    if state.has_answered_dialogue_question(question) {
        return StoryEvent::new(
            "问题已经问过",
            "对方没有阻止你重问，只是这次答案更短：真正还没发生的，是你怎样使用它。",
        )
        .tag("自由询问")
        .tag("复谈");
    }

    state.answer_dialogue_question(question);
    let mut event = question.event(state.dialogue_tone);
    apply_question_rewards(state, question, &mut event);
    event
}

impl DialogueQuestionId {
    pub const ALL: [Self; 8] = [
        Self::TravelerAboutChild,
        Self::TravelerAboutSecondSeat,
        Self::ClerkAboutWetTicket,
        Self::ClerkAboutName,
        Self::ChildAboutTraveler,
        Self::ChildAboutLeaving,
        Self::KeeperAboutBroadcastGap,
        Self::KeeperAboutStaying,
    ];

    fn dialogue(self) -> DialogueId {
        match self {
            Self::TravelerAboutChild | Self::TravelerAboutSecondSeat => DialogueId::Traveler,
            Self::ClerkAboutWetTicket | Self::ClerkAboutName => DialogueId::Clerk,
            Self::ChildAboutTraveler | Self::ChildAboutLeaving => DialogueId::Child,
            Self::KeeperAboutBroadcastGap | Self::KeeperAboutStaying => DialogueId::Keeper,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TravelerAboutChild => "自由询问：你认识白线后的孩子吗",
            Self::TravelerAboutSecondSeat => "自由询问：第二个座位到底是谁的",
            Self::ClerkAboutWetTicket => "自由询问：湿票为什么还能核验",
            Self::ClerkAboutName => "自由询问：姓名能不能重新登记",
            Self::ChildAboutTraveler => "自由询问：老人说的雨你听懂了吗",
            Self::ChildAboutLeaving => "自由询问：如果明天真的到了",
            Self::KeeperAboutBroadcastGap => "自由询问：广播里缺掉的一秒",
            Self::KeeperAboutStaying => "自由询问：留下守夜算不算逃避",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::TravelerAboutChild => "老人和白线后的孩子",
            Self::TravelerAboutSecondSeat => "第二个座位的归属",
            Self::ClerkAboutWetTicket => "湿票核验",
            Self::ClerkAboutName => "姓名重新登记",
            Self::ChildAboutTraveler => "孩子听见老人说雨",
            Self::ChildAboutLeaving => "孩子想象明天",
            Self::KeeperAboutBroadcastGap => "广播缺掉的一秒",
            Self::KeeperAboutStaying => "守夜和逃避",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::TravelerAboutChild => "把孩子的存在带回老人面前，问他是否一直知道孩子在等。",
            Self::TravelerAboutSecondSeat => "围绕空座、铜筹或返程手续追问老人保留 07B 的原因。",
            Self::ClerkAboutWetTicket => "在窗口继续追问湿票为何仍被系统承认。",
            Self::ClerkAboutName => "带着姓名牌或找回的名字，询问窗口能否把人重新登记。",
            Self::ChildAboutTraveler => "把老人关于雨和镜片的说法放到孩子面前，让他自己解释。",
            Self::ChildAboutLeaving => "在孩子已经开始信任你以后，问他怎样想象明天。",
            Self::KeeperAboutBroadcastGap => "有了磁带、广播稿或回声后，问站务员那一秒为什么缺席。",
            Self::KeeperAboutStaying => "在理解车站机制后，询问守夜是不是另一种逃离。",
        }
    }

    fn visible(self, state: &GameState) -> bool {
        match self {
            Self::TravelerAboutChild => {
                state.has_flag(Flag::MetChild) || state.has_flag(Flag::UnderstoodChildPromise)
            }
            Self::TravelerAboutSecondSeat => {
                state.has_item(Item::CoinToken)
                    || state.has_flag(Flag::SynthesizedRoute)
                    || state.has_completed_dialogue_relay(
                        crate::model::DialogueRelayId::EmptySeatToClerk,
                    )
            }
            Self::ClerkAboutWetTicket => {
                state.has_item(Item::WetTicket) || state.has_flag(Flag::ExaminedTicket)
            }
            Self::ClerkAboutName => {
                state.has_item(Item::NameTag) || state.has_flag(Flag::RecoveredName)
            }
            Self::ChildAboutTraveler => {
                state.has_completed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild)
                    || state.has_echoed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild)
            }
            Self::ChildAboutLeaving => {
                state.child_trust >= 2
                    || state.has_flag(Flag::ReturnedNameTag)
                    || state.has_prepared_departure(crate::model::DepartureId::ChildWindowSeat)
            }
            Self::KeeperAboutBroadcastGap => {
                state.has_item(Item::BroadcastTape)
                    || state.has_flag(Flag::HeardBroadcastTape)
                    || state.has_completed_dialogue_relay(
                        crate::model::DialogueRelayId::BroadcastDraftToTraveler,
                    )
            }
            Self::KeeperAboutStaying => {
                state.has_flag(Flag::UnderstoodStationMechanism)
                    || state.keeper_depth >= 3
                    || state.has_prepared_departure(crate::model::DepartureId::KeeperLedger)
            }
        }
    }

    fn event(self, tone: DialogueTone) -> StoryEvent {
        let approach = match tone {
            DialogueTone::Listening => "你先把问题放慢，让对方听见它不是审问。 ",
            DialogueTone::Gentle => "你把问题放得很轻，像怕碰碎刚刚露出的事实。 ",
            DialogueTone::Direct => "你没有绕路，直接把问题推到两人中间。 ",
        };
        let (title, body) = match self {
            Self::TravelerAboutChild => (
                "自由询问：老人和孩子",
                "老人把报纸折到一半，说他当然看见过那个孩子。只是大人有时把“看见”说成“不要打扰”，好让自己少承担一点。他说：别学我。",
            ),
            Self::TravelerAboutSecondSeat => (
                "自由询问：第二个座位",
                "老人说 07B 不是空座，是他每次逃跑时假装还有人会追上来的位置。现在它被写进手续，他反而不敢再把它当借口。",
            ),
            Self::ClerkAboutWetTicket => (
                "自由询问：湿票核验",
                "售票员把你的湿票平铺在玻璃下。她说：票面湿透以后，普通系统会拒收；雾灯站相反，只有被水泡过的字，才可能是真的。",
            ),
            Self::ClerkAboutName => (
                "自由询问：姓名登记",
                "售票员翻出一张空白旅客卡，说姓名不是贴回去就算完整。它要被人叫一次，被本人承认一次，再被车站允许离开一次。",
            ),
            Self::ChildAboutTraveler => (
                "自由询问：孩子听见雨",
                "孩子说他听懂了老人说的雨，但不想替谁擦干。雨落在地上就是证据，不是请求他马上原谅的理由。",
            ),
            Self::ChildAboutLeaving => (
                "自由询问：明天的形状",
                "孩子把作业本往怀里收了收，说如果明天真的到了，他想先买一支普通铅笔。不是纪念，不是补偿，只是上课会用到。",
            ),
            Self::KeeperAboutBroadcastGap => (
                "自由询问：缺掉的一秒",
                "站务员说广播缺掉的一秒不是故障，是当年每个人都觉得可以省略的名字。省略太久，车站就学会了吞字。",
            ),
            Self::KeeperAboutStaying => (
                "自由询问：守夜和逃避",
                "站务员看着无影的外套，说留下不是抵账，除非留下的人还愿意让后来者走。守夜如果只会扣住别人，也不过是换了制服的逃跑。",
            ),
        };
        StoryEvent::new(title, format!("{approach}{body}"))
            .tag("自由询问")
            .tag("主动对话")
            .tag(format!("语气：{}", tone.name()))
    }

    fn review(self) -> &'static str {
        match self {
            Self::TravelerAboutChild => {
                "老人承认他看见过孩子，也承认“看见”不能再被当作不打扰的借口。"
            }
            Self::TravelerAboutSecondSeat => {
                "老人把 07B 说成每次逃跑时保留的追赶位置，空座因此不再只是借口。"
            }
            Self::ClerkAboutWetTicket => {
                "售票员确认湿票在雾灯站仍可核验，因为被水泡过的字反而更接近真相。"
            }
            Self::ClerkAboutName => "售票员说明姓名需要被叫出、被承认、再被车站允许离开。",
            Self::ChildAboutTraveler => "孩子听懂了老人说的雨，但拒绝把证据立刻读成原谅。",
            Self::ChildAboutLeaving => "孩子把明天说成一支普通铅笔，而不是纪念或补偿。",
            Self::KeeperAboutBroadcastGap => {
                "站务员承认广播缺掉的一秒来自被省略的名字，不只是设备故障。"
            }
            Self::KeeperAboutStaying => {
                "站务员说留下只有在愿意放后来者离开时，才不算换了制服的逃跑。"
            }
        }
    }

    fn missing_hint(self) -> &'static str {
        match self {
            Self::TravelerAboutChild => "先在三号月台见到孩子，或让孩子的承诺进入其他证词。",
            Self::TravelerAboutSecondSeat => {
                "先找到退票铜筹、整理返程手续，或让空座页码进入转述链。"
            }
            Self::ClerkAboutWetTicket => "先检查湿票，让窗口问题有可以递过去的票面。",
            Self::ClerkAboutName => "先找到姓名牌，或在地下通道把遗失的名字找回来。",
            Self::ChildAboutTraveler => "先把老人关于镜片和雨的线索转述给孩子。",
            Self::ChildAboutLeaving => "先让孩子信任你，归还姓名，或准备一条带他离开的路线。",
            Self::KeeperAboutBroadcastGap => {
                "先找到广播磁带、听过广播回声，或推进广播缺口的转述链。"
            }
            Self::KeeperAboutStaying => "先理解车站机制、推进站务员对话，或准备守夜相关路线。",
        }
    }
}

fn question_progress(visible: bool, ready: bool, answered: bool) -> u8 {
    if answered {
        100
    } else if ready {
        72
    } else if visible {
        45
    } else {
        0
    }
}

fn apply_question_rewards(
    state: &mut GameState,
    question: DialogueQuestionId,
    event: &mut StoryEvent,
) {
    match question.dialogue() {
        DialogueId::Traveler => {
            state.traveler_depth = state.traveler_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.remember(Flag::TravelerTrusted);
        }
        DialogueId::Clerk => {
            state.clerk_depth = state.clerk_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            state.remember(Flag::ClerkMet);
        }
        DialogueId::Child => {
            state.child_depth = state.child_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.child_trust = (state.child_trust + 1).min(5);
            state.remember(Flag::MetChild);
        }
        DialogueId::Keeper => {
            state.keeper_depth = state.keeper_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
    }
    match question {
        DialogueQuestionId::TravelerAboutChild => {
            event.tags.push("询问：老人承认孩子".to_string());
        }
        DialogueQuestionId::TravelerAboutSecondSeat => {
            state.remember(Flag::SynthesizedRoute);
            event.tags.push("询问：空座有归属".to_string());
        }
        DialogueQuestionId::ClerkAboutWetTicket => {
            state.remember(Flag::ExaminedTicket);
            event.tags.push("询问：湿票可核验".to_string());
        }
        DialogueQuestionId::ClerkAboutName => {
            state.remember(Flag::RecoveredName);
            event.tags.push("询问：姓名可登记".to_string());
        }
        DialogueQuestionId::ChildAboutTraveler => {
            state.remember(Flag::UnderstoodChildPromise);
            event.tags.push("询问：雨不是原谅".to_string());
        }
        DialogueQuestionId::ChildAboutLeaving => {
            event.tags.push("询问：普通明天".to_string());
        }
        DialogueQuestionId::KeeperAboutBroadcastGap => {
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("询问：广播缺名".to_string());
        }
        DialogueQuestionId::KeeperAboutStaying => {
            state.remember(Flag::UnderstoodStationMechanism);
            event.tags.push("询问：守夜放人".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Location;

    #[test]
    fn questions_unlock_from_discovered_state_and_record_completion() {
        let mut state = GameState::new();
        state.location = Location::Platform;
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: crate::model::DialogueNodeId::Root,
        });
        assert!(available_questions(&state, state.active_dialogue.unwrap()).is_empty());

        state.complete_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild);
        let questions = available_questions(&state, state.active_dialogue.unwrap());
        assert!(questions
            .iter()
            .any(|question| question.question == DialogueQuestionId::ChildAboutTraveler));

        let event = ask(&mut state, DialogueQuestionId::ChildAboutTraveler);
        assert!(event.tags.iter().any(|tag| tag == "自由询问"));
        assert!(state.has_answered_dialogue_question(DialogueQuestionId::ChildAboutTraveler));
        assert!(state.has_flag(Flag::UnderstoodChildPromise));

        let summaries = question_summaries(&state);
        let child_question = summaries
            .iter()
            .find(|summary| summary.question == DialogueQuestionId::ChildAboutTraveler)
            .expect("child question summary should exist");
        assert_eq!(child_question.status, "已问");
        assert_eq!(child_question.progress, 100);
    }

    #[test]
    fn question_count_matches_declared_table() {
        assert_eq!(DIALOGUE_QUESTION_COUNT, DialogueQuestionId::ALL.len());
    }
}
