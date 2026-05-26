use crate::model::{
    ActiveDialogue, DialogueChallengeId, DialogueChallengeResponseId, DialogueId,
    DialogueQuestionId, DialogueTone, Flag, GameState, StoryEvent, NPC_THREAD_STEPS,
};

pub const DIALOGUE_CHALLENGE_COUNT: usize = DialogueChallengeId::ALL.len();

#[derive(Clone, Debug)]
pub struct DialogueChallengeResponseAction {
    pub challenge: DialogueChallengeId,
    pub response: DialogueChallengeResponseId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueChallengeSummary {
    pub challenge: DialogueChallengeId,
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
) -> Vec<DialogueChallengeResponseAction> {
    DialogueChallengeId::ALL
        .iter()
        .copied()
        .filter(|challenge| challenge.dialogue() == active.dialogue)
        .filter(|challenge| challenge.visible(state))
        .filter(|challenge| !state.has_answered_dialogue_challenge(*challenge))
        .flat_map(|challenge| {
            DialogueChallengeResponseId::ALL
                .iter()
                .copied()
                .map(move |response| DialogueChallengeResponseAction {
                    challenge,
                    response,
                    label: challenge.response_label(response),
                    detail: challenge.response_detail(response),
                    enabled: true,
                })
        })
        .collect()
}

pub fn challenge_summaries(state: &GameState) -> Vec<DialogueChallengeSummary> {
    DialogueChallengeId::ALL
        .iter()
        .copied()
        .map(|challenge| {
            let response = state.dialogue_challenge_response(challenge);
            let answered = response.is_some();
            let visible = challenge.visible(state);
            let ready = state
                .active_dialogue
                .is_some_and(|active| active.dialogue == challenge.dialogue())
                && visible
                && !answered;
            let status = if answered {
                "已回应"
            } else if ready {
                "等待回应"
            } else if visible {
                "待对话"
            } else {
                "未显形"
            };
            let detail = if let Some(response) = response {
                challenge.review(response).to_string()
            } else if ready {
                format!(
                    "{}正在把问题推回给你。选一种立场回应，而不是继续只让对方回答。",
                    challenge.dialogue().title()
                )
            } else if visible {
                format!(
                    "去{}进入{}的主动对话，听见这个反问。",
                    challenge.dialogue().location().title(),
                    challenge.dialogue().title()
                )
            } else {
                challenge.missing_hint().to_string()
            };
            DialogueChallengeSummary {
                challenge,
                title: challenge.title(),
                status,
                detail,
                progress: challenge_progress(visible, ready, answered),
                visible,
                ready,
                answered,
            }
        })
        .collect()
}

pub fn active_challenge_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    DialogueChallengeId::ALL
        .iter()
        .copied()
        .find(|challenge| {
            challenge.dialogue() == active.dialogue
                && challenge.visible(state)
                && !state.has_answered_dialogue_challenge(*challenge)
        })
        .map(|challenge| {
            format!(
                "{}反问你：{}。",
                challenge.dialogue().title(),
                challenge.prompt()
            )
        })
}

pub fn challenge_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_challenge_objective_hint(state);
    }
    DialogueChallengeId::ALL
        .iter()
        .copied()
        .filter(|challenge| challenge.visible(state))
        .filter(|challenge| !state.has_answered_dialogue_challenge(*challenge))
        .find(|challenge| challenge.dialogue().location() == state.location)
        .map(|challenge| {
            format!(
                "可以进入{}的对话，回应对方的反问：{}。",
                challenge.dialogue().title(),
                challenge.title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.answered_dialogue_challenges.is_empty() {
        return None;
    }

    let notes = state
        .answered_dialogue_challenges
        .iter()
        .map(|(challenge, response)| {
            format!("{}：{}", challenge.title(), challenge.review(*response))
        })
        .collect::<Vec<_>>();
    Some(format!("NPC 的反问没有停在对话框里。{}", notes.join(" ")))
}

pub fn answer(
    state: &mut GameState,
    challenge: DialogueChallengeId,
    response: DialogueChallengeResponseId,
) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "没有正在进行的反问",
            "这不是可以独自排练的回答。对方必须在场，你才需要承担自己的立场。",
        )
        .tag("NPC反问");
    };

    if challenge.dialogue() != active.dialogue {
        return StoryEvent::new(
            "这不是当前人物的反问",
            format!(
                "你正和{}说话，但这个反问来自{}。",
                active.dialogue.title(),
                challenge.dialogue().title()
            ),
        )
        .tag("NPC反问");
    }

    if !challenge.visible(state) {
        return StoryEvent::new(
            "反问还没有发生",
            "对方还没有被你的证词逼到需要反问你。继续推进自由询问、证据或转述。",
        )
        .tag("NPC反问");
    }

    if state.has_answered_dialogue_challenge(challenge) {
        return StoryEvent::new(
            "反问已经回应过",
            "对方没有再追问。你的回答已经留在这段关系里，接下来要看它如何改变行动。",
        )
        .tag("NPC反问")
        .tag("复谈");
    }

    state.answer_dialogue_challenge(challenge, response);
    let mut event = challenge.event(response, state.dialogue_tone);
    apply_response_rewards(state, challenge, response, &mut event);
    event
}

impl DialogueChallengeId {
    pub const ALL: [Self; 4] = [
        Self::TravelerAsksWhyYouKeptTheSeat,
        Self::ClerkAsksWhoPaysForReturn,
        Self::ChildAsksIfYouWillLeaveAgain,
        Self::KeeperAsksIfStayingIsMercy,
    ];

    fn dialogue(self) -> DialogueId {
        match self {
            Self::TravelerAsksWhyYouKeptTheSeat => DialogueId::Traveler,
            Self::ClerkAsksWhoPaysForReturn => DialogueId::Clerk,
            Self::ChildAsksIfYouWillLeaveAgain => DialogueId::Child,
            Self::KeeperAsksIfStayingIsMercy => DialogueId::Keeper,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::TravelerAsksWhyYouKeptTheSeat => "你为什么还替别人留座",
            Self::ClerkAsksWhoPaysForReturn => "返程的代价由谁承担",
            Self::ChildAsksIfYouWillLeaveAgain => "你会不会又一次离开",
            Self::KeeperAsksIfStayingIsMercy => "留下守夜是不是仁慈",
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::TravelerAsksWhyYouKeptTheSeat => "你一直替别人留着 07B，那你自己到底想不想上车？",
            Self::ClerkAsksWhoPaysForReturn => "返程票可以开，但谁来替那段明天付账？",
            Self::ChildAsksIfYouWillLeaveAgain => "如果车门又开了，你会不会又说这是为了我好？",
            Self::KeeperAsksIfStayingIsMercy => "你说留下来守夜，可你确定那不是另一种不肯离开？",
        }
    }

    fn visible(self, state: &GameState) -> bool {
        match self {
            Self::TravelerAsksWhyYouKeptTheSeat => {
                state.has_answered_dialogue_question(DialogueQuestionId::TravelerAboutSecondSeat)
                    || state.has_completed_dialogue_relay(
                        crate::model::DialogueRelayId::EmptySeatToClerk,
                    )
                    || state.has_flag(Flag::SynthesizedRoute)
            }
            Self::ClerkAsksWhoPaysForReturn => {
                state.has_answered_dialogue_question(DialogueQuestionId::ClerkAboutWetTicket)
                    || state.has_answered_dialogue_question(DialogueQuestionId::ClerkAboutName)
                    || state.ticket == crate::model::TicketKind::Return
            }
            Self::ChildAsksIfYouWillLeaveAgain => {
                state.has_answered_dialogue_question(DialogueQuestionId::ChildAboutLeaving)
                    || state.has_flag(Flag::ReturnedNameTag)
                    || state.has_prepared_departure(crate::model::DepartureId::ChildWindowSeat)
            }
            Self::KeeperAsksIfStayingIsMercy => {
                state.has_answered_dialogue_question(DialogueQuestionId::KeeperAboutStaying)
                    || state.has_flag(Flag::UnderstoodStationMechanism)
                    || state.has_prepared_departure(crate::model::DepartureId::KeeperLedger)
            }
        }
    }

    fn response_label(self, response: DialogueChallengeResponseId) -> String {
        format!("回应反问：{}", response.label_for(self))
    }

    fn response_detail(self, response: DialogueChallengeResponseId) -> String {
        match response {
            DialogueChallengeResponseId::Admit => {
                "承认问题刺中了你。关系会更近，但这句话会被记录成你必须承担的事实。"
            }
            DialogueChallengeResponseId::Deflect => {
                "把问题暂时挡开。对话仍会继续，但对方会记得你回避了重点。"
            }
            DialogueChallengeResponseId::Promise => {
                "给出一个具体承诺。它会推进路线理解，也会让后续选择更难装作无关。"
            }
        }
        .to_string()
    }

    fn event(self, response: DialogueChallengeResponseId, tone: DialogueTone) -> StoryEvent {
        let approach = match tone {
            DialogueTone::Listening => "你先听完反问，没有急着把它解释成误会。 ",
            DialogueTone::Gentle => "你把回答放得很轻，但没有把锋利处磨平。 ",
            DialogueTone::Direct => "你直接接住那个问题，让它停在你们中间。 ",
        };
        let (title, body) = match (self, response) {
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Admit) => (
                "回应反问：承认自己也想逃",
                "你说：我留着那个座位，是因为我想证明自己还不是一个人。老人没有笑。他说这比漂亮话诚实多了，也更危险。",
            ),
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Deflect) => (
                "回应反问：把空座说成手续",
                "你说 07B 只是返程规则的一部分。老人重新抬起报纸，说：那就让规则替你睡吧，人醒着会比较麻烦。",
            ),
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Promise) => (
                "回应反问：不再替别人上车",
                "你说你会保留座位，但不会替任何人坐上去。老人把报纸压低，像第一次听见保留和占有之间还有距离。",
            ),
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Admit) => (
                "回应反问：承认明天有代价",
                "你说返程不是免费的，至少要付出不再把痛苦当凭证的代价。售票员把票章停在半空，像在确认你听懂了价格。",
            ),
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Deflect) => (
                "回应反问：让窗口先开票",
                "你说先把票开出来，代价之后再算。售票员没有拒绝，只把“之后”两个字写得很重。",
            ),
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Promise) => (
                "回应反问：自己承担返程",
                "你说如果明天需要有人签名，你会签自己的名字。售票员看了你一会儿，把空白栏推近了一寸。",
            ),
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Admit) => (
                "回应反问：承认你害怕留下",
                "你说你怕车门关上，也怕它打开。孩子没有安慰你，只说：那你不要再把怕说成是为了我。",
            ),
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Deflect) => (
                "回应反问：说这次情况不同",
                "你说这次不一样。孩子低头看作业本，说大人每次离开以前，都很擅长把“这次”说得像新词。",
            ),
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Promise) => (
                "回应反问：让他决定距离",
                "你说如果车门打开，你会先问他要不要靠近，而不是把答案抱起来就走。孩子没有点头，但手指松开了一点。",
            ),
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Admit) => (
                "回应反问：承认留下也可能是逃",
                "你说留下也可能只是换一个地方躲。站务员看着旧钟，说能承认这一点的人，至少还没有把制服当成赦免书。",
            ),
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Deflect) => (
                "回应反问：把守夜说成职责",
                "你说总得有人守夜。站务员点头，却没有放过你：职责最容易被用来盖住不肯离开的私心。",
            ),
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Promise) => (
                "回应反问：留下是为了放人走",
                "你说如果你留下，第一条规矩就是不把后来者留下。站务员终于看向门外，像在确认雾有没有听见。",
            ),
        };
        StoryEvent::new(title, format!("{approach}{body}"))
            .tag("NPC反问")
            .tag("立场回应")
            .tag(format!("回应：{}", response.name()))
            .tag(format!("语气：{}", tone.name()))
    }

    fn review(self, response: DialogueChallengeResponseId) -> &'static str {
        match (self, response) {
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Admit) => {
                "你承认留座也有自己的逃意，老人因此更愿意相信你的诚实。"
            }
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Deflect) => {
                "你把空座说成手续，老人记住了你的回避。"
            }
            (Self::TravelerAsksWhyYouKeptTheSeat, DialogueChallengeResponseId::Promise) => {
                "你承诺保留座位但不替别人上车，07B 因此更像选择。"
            }
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Admit) => {
                "你承认返程要付代价，窗口开始把你当成能签字的人。"
            }
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Deflect) => {
                "你要求先开票，售票员把“之后”记进账页。"
            }
            (Self::ClerkAsksWhoPaysForReturn, DialogueChallengeResponseId::Promise) => {
                "你承诺自己承担返程，窗口留下了可签名的空白栏。"
            }
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Admit) => {
                "你承认害怕留下，孩子听见你没有再把害怕伪装成照顾。"
            }
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Deflect) => {
                "你说这次不同，孩子把这句话暂时放进不完全相信的格子里。"
            }
            (Self::ChildAsksIfYouWillLeaveAgain, DialogueChallengeResponseId::Promise) => {
                "你承诺让他决定距离，孩子的明天因此多了一点主动权。"
            }
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Admit) => {
                "你承认留下也可能是逃，站务员把这份警惕看得比牺牲可靠。"
            }
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Deflect) => {
                "你把守夜说成职责，站务员记住了你仍在躲开私心。"
            }
            (Self::KeeperAsksIfStayingIsMercy, DialogueChallengeResponseId::Promise) => {
                "你承诺留下是为了放人走，守夜第一次像一条能打开的规矩。"
            }
        }
    }

    fn missing_hint(self) -> &'static str {
        match self {
            Self::TravelerAsksWhyYouKeptTheSeat => "先让第二个座位、空座页码或返程路线进入对话。",
            Self::ClerkAsksWhoPaysForReturn => "先追问湿票、姓名登记，或真正拿到返程票。",
            Self::ChildAsksIfYouWillLeaveAgain => {
                "先让孩子谈到明天、归还姓名，或准备带他离开的路线。"
            }
            Self::KeeperAsksIfStayingIsMercy => "先理解车站机制，或把守夜路线推进到具体选择。",
        }
    }
}

impl DialogueChallengeResponseId {
    pub const ALL: [Self; 3] = [Self::Admit, Self::Deflect, Self::Promise];

    fn name(self) -> &'static str {
        match self {
            Self::Admit => "承认",
            Self::Deflect => "回避",
            Self::Promise => "承诺",
        }
    }

    fn label_for(self, challenge: DialogueChallengeId) -> &'static str {
        match (challenge, self) {
            (DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat, Self::Admit) => "承认你也想逃",
            (DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat, Self::Deflect) => "说那只是手续",
            (DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat, Self::Promise) => {
                "承诺不替别人上车"
            }
            (DialogueChallengeId::ClerkAsksWhoPaysForReturn, Self::Admit) => "承认返程有代价",
            (DialogueChallengeId::ClerkAsksWhoPaysForReturn, Self::Deflect) => "让窗口先开票",
            (DialogueChallengeId::ClerkAsksWhoPaysForReturn, Self::Promise) => "承诺自己签名",
            (DialogueChallengeId::ChildAsksIfYouWillLeaveAgain, Self::Admit) => "承认你害怕留下",
            (DialogueChallengeId::ChildAsksIfYouWillLeaveAgain, Self::Deflect) => "说这次不一样",
            (DialogueChallengeId::ChildAsksIfYouWillLeaveAgain, Self::Promise) => "承诺先问他",
            (DialogueChallengeId::KeeperAsksIfStayingIsMercy, Self::Admit) => "承认留下也可能是逃",
            (DialogueChallengeId::KeeperAsksIfStayingIsMercy, Self::Deflect) => "说守夜是职责",
            (DialogueChallengeId::KeeperAsksIfStayingIsMercy, Self::Promise) => {
                "承诺留下是为了放人走"
            }
        }
    }
}

fn challenge_progress(visible: bool, ready: bool, answered: bool) -> u8 {
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

fn apply_response_rewards(
    state: &mut GameState,
    challenge: DialogueChallengeId,
    response: DialogueChallengeResponseId,
    event: &mut StoryEvent,
) {
    match challenge.dialogue() {
        DialogueId::Traveler => {
            state.traveler_depth = state.traveler_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            if response != DialogueChallengeResponseId::Deflect {
                state.remember(Flag::TravelerTrusted);
            }
        }
        DialogueId::Clerk => {
            state.clerk_depth = state.clerk_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.remember(Flag::ClerkMet);
            if response != DialogueChallengeResponseId::Deflect {
                state.clerk_trust = (state.clerk_trust + 1).min(5);
            }
        }
        DialogueId::Child => {
            state.child_depth = state.child_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.remember(Flag::MetChild);
            if response != DialogueChallengeResponseId::Deflect {
                state.child_trust = (state.child_trust + 1).min(5);
            }
        }
        DialogueId::Keeper => {
            state.keeper_depth = state.keeper_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            if response != DialogueChallengeResponseId::Deflect {
                state.keeper_trust = (state.keeper_trust + 1).min(5);
            }
        }
    }

    match response {
        DialogueChallengeResponseId::Admit => {
            event.tags.push("反问：承认代价".to_string());
        }
        DialogueChallengeResponseId::Deflect => {
            event.tags.push("反问：暂时回避".to_string());
        }
        DialogueChallengeResponseId::Promise => {
            state.synthesis_depth = state
                .synthesis_depth
                .saturating_add(1)
                .min(NPC_THREAD_STEPS);
            match challenge {
                DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat => {
                    state.remember(Flag::SynthesizedRoute);
                }
                DialogueChallengeId::ClerkAsksWhoPaysForReturn => {
                    state.remember(Flag::SynthesizedRoute);
                }
                DialogueChallengeId::ChildAsksIfYouWillLeaveAgain => {
                    state.remember(Flag::UnderstoodChildPromise);
                }
                DialogueChallengeId::KeeperAsksIfStayingIsMercy => {
                    state.remember(Flag::UnderstoodStationMechanism);
                }
            }
            event.tags.push("反问：具体承诺".to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Location;

    #[test]
    fn challenges_unlock_after_free_questions_and_record_response() {
        let mut state = GameState::new();
        state.location = Location::TicketOffice;
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Clerk,
            node: crate::model::DialogueNodeId::Root,
        });
        assert!(available_responses(&state, state.active_dialogue.unwrap()).is_empty());

        state.answer_dialogue_question(DialogueQuestionId::ClerkAboutWetTicket);
        let responses = available_responses(&state, state.active_dialogue.unwrap());
        assert_eq!(responses.len(), 3);
        assert!(responses
            .iter()
            .any(|action| action.response == DialogueChallengeResponseId::Promise));

        let event = answer(
            &mut state,
            DialogueChallengeId::ClerkAsksWhoPaysForReturn,
            DialogueChallengeResponseId::Promise,
        );
        assert!(event.tags.iter().any(|tag| tag == "NPC反问"));
        assert_eq!(
            state.dialogue_challenge_response(DialogueChallengeId::ClerkAsksWhoPaysForReturn),
            Some(DialogueChallengeResponseId::Promise)
        );
        assert!(state.has_flag(Flag::SynthesizedRoute));

        let summary = challenge_summaries(&state)
            .into_iter()
            .find(|summary| summary.challenge == DialogueChallengeId::ClerkAsksWhoPaysForReturn)
            .expect("clerk challenge should exist");
        assert_eq!(summary.status, "已回应");
        assert_eq!(summary.progress, 100);
    }

    #[test]
    fn challenge_count_matches_declared_table() {
        assert_eq!(DIALOGUE_CHALLENGE_COUNT, DialogueChallengeId::ALL.len());
    }
}
