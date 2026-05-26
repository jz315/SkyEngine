use crate::model::{
    ActiveDialogue, DialogueId, DialogueLeadId, DialogueRelayId, DialogueTone, Flag, GameState,
    Location, StoryEvent, NPC_THREAD_STEPS,
};

pub const DIALOGUE_RELAY_COUNT: usize = 8;

#[derive(Clone, Debug)]
pub struct DialogueRelayAction {
    pub relay: DialogueRelayId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueRelayReflectionAction {
    pub relay: DialogueRelayId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueRelayEchoAction {
    pub relay: DialogueRelayId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueRelayAnchorAction {
    pub relay: DialogueRelayId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueRelayAnchorReviewAction {
    pub relay: DialogueRelayId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueRelaySummary {
    pub relay: DialogueRelayId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub reflected: bool,
    pub echoed: bool,
    pub anchored: bool,
    pub reviewed: bool,
    pub ready: bool,
}

pub fn available_relays(state: &GameState, active: ActiveDialogue) -> Vec<DialogueRelayAction> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| !state.has_completed_dialogue_relay(*relay))
        .filter(|relay| relay.target_dialogue() == active.dialogue)
        .filter(|relay| relay_visible(state, *relay))
        .map(|relay| {
            let missing = missing_requirements(state, relay);
            DialogueRelayAction {
                relay,
                label: relay.label(),
                detail: if missing.is_empty() {
                    format!(
                        "把“{}”转述给{}，让另一条人物线接住这件事实。",
                        relay.source_lead().title(),
                        relay.target_dialogue().title()
                    )
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn available_reflections(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<DialogueRelayReflectionAction> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_completed_dialogue_relay(*relay))
        .filter(|relay| !state.has_reflected_dialogue_relay(*relay))
        .filter(|relay| relay.target_dialogue() == active.dialogue)
        .map(|relay| DialogueRelayReflectionAction {
            relay,
            label: relay.reflection_label(),
            detail: format!(
                "继续追问“{}”转述以后留下的余波，让{}给出更长期的回应。",
                relay.source_lead().title(),
                relay.target_dialogue().title()
            ),
            enabled: true,
        })
        .collect()
}

pub fn available_echoes(state: &GameState, active: ActiveDialogue) -> Vec<DialogueRelayEchoAction> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_reflected_dialogue_relay(*relay))
        .filter(|relay| !state.has_echoed_dialogue_relay(*relay))
        .filter(|relay| relay.source_dialogue() == active.dialogue)
        .map(|relay| DialogueRelayEchoAction {
            relay,
            label: relay.echo_label(),
            detail: format!(
                "把{}的回应带回{}，让这条转述真正形成双向关系。",
                relay.target_dialogue().title(),
                relay.source_dialogue().title()
            ),
            enabled: true,
        })
        .collect()
}

pub fn available_anchors(state: &GameState) -> Vec<DialogueRelayAnchorAction> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_echoed_dialogue_relay(*relay))
        .filter(|relay| !state.has_anchored_dialogue_relay(*relay))
        .filter(|relay| relay.anchor_location() == state.location)
        .map(|relay| DialogueRelayAnchorAction {
            relay,
            label: relay.anchor_label(),
            detail: format!(
                "把这段转述回声留在{}，让对话不只停在记录里。",
                relay.anchor_location().title()
            ),
            enabled: true,
        })
        .collect()
}

pub fn available_anchor_reviews(state: &GameState) -> Vec<DialogueRelayAnchorReviewAction> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_anchored_dialogue_relay(*relay))
        .filter(|relay| !state.has_reviewed_dialogue_anchor(*relay))
        .filter(|relay| relay.anchor_location() == state.location)
        .map(|relay| DialogueRelayAnchorReviewAction {
            relay,
            label: relay.review_anchor_label(),
            detail: "回到这个落点旁，看看车站怎样保存这段对话。".to_string(),
            enabled: true,
        })
        .collect()
}

pub fn relay_summaries(state: &GameState) -> Vec<DialogueRelaySummary> {
    DialogueRelayId::ALL
        .iter()
        .copied()
        .map(|relay| {
            let completed = state.has_completed_dialogue_relay(relay);
            let reflected = state.has_reflected_dialogue_relay(relay);
            let echoed = state.has_echoed_dialogue_relay(relay);
            let anchored = state.has_anchored_dialogue_relay(relay);
            let reviewed = state.has_reviewed_dialogue_anchor(relay);
            let visible = completed || relay_visible(state, relay);
            let missing = missing_requirements(state, relay);
            let ready = can_share_now(state, relay);
            let status = if reviewed {
                "已复看"
            } else if anchored {
                "已落点"
            } else if echoed {
                "已回声"
            } else if reflected {
                "已追问"
            } else if completed {
                "已转述"
            } else if ready {
                "可转述"
            } else if visible {
                "待转述"
            } else {
                "未显形"
            };
            let detail = if reviewed {
                relay.anchor_review_after().to_string()
            } else if anchored {
                relay.anchor_review().to_string()
            } else if echoed {
                relay.echo_review().to_string()
            } else if reflected {
                relay.reflection_review().to_string()
            } else if completed {
                relay.review().to_string()
            } else if ready {
                format!(
                    "{}正在听。现在可以把“{}”转述给对方，让对话从单线变成关系网。",
                    relay.target_dialogue().title(),
                    relay.source_lead().title()
                )
            } else if visible {
                let mut text = format!(
                    "带着“{}”去{}找{}，在主动对话里转述。",
                    relay.source_lead().title(),
                    relay.target_dialogue().location().title(),
                    relay.target_dialogue().title()
                );
                if !missing.is_empty() {
                    text.push_str(&format!(" 还缺：{}。", missing.join("；")));
                }
                text
            } else {
                format!(
                    "这条转述还没成形。先把“{}”追查完，并带回{}的对话里。",
                    relay.source_lead().title(),
                    relay.source_dialogue().title()
                )
            };

            DialogueRelaySummary {
                relay,
                title: relay.title(),
                status,
                detail,
                progress: relay_progress(
                    state,
                    relay,
                    visible,
                    completed,
                    reflected,
                    echoed,
                    anchored,
                    reviewed,
                    missing.len(),
                ),
                visible,
                completed,
                reflected,
                echoed,
                anchored,
                reviewed,
                ready,
            }
        })
        .collect()
}

pub fn share(state: &mut GameState, relay: DialogueRelayId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "还没有开始转述的对话",
            "这条线索已经离开原来的说话人，但它需要另一个正在听的人，才会变成真正的转述。",
        )
        .tag("线索转述");
    };

    if relay.target_dialogue() != active.dialogue {
        return StoryEvent::new(
            "这条线索不该转述给当前人物",
            format!(
                "你正和{}说话，而“{}”应该转述给{}。",
                active.dialogue.title(),
                relay.source_lead().title(),
                relay.target_dialogue().title()
            ),
        )
        .tag("线索转述");
    }

    if !relay_visible(state, relay) {
        return StoryEvent::new(
            "这条线索还没完成回谈",
            format!(
                "“{}”还停在原人物的对话里。先追查它，再带回{}那里回应。",
                relay.source_lead().title(),
                relay.source_dialogue().title()
            ),
        )
        .tag("线索转述");
    }

    if state.has_completed_dialogue_relay(relay) {
        return StoryEvent::new(
            "这条转述已经发生过",
            "对方记得你怎样把事实从另一个人那里带来。再重复一遍不会更真，只会让沉默变厚。",
        )
        .tag("线索转述");
    }

    let missing = missing_requirements(state, relay);
    if !missing.is_empty() {
        return StoryEvent::new(
            "转述还缺一个落点",
            format!("这条线索已经能被转述，但还缺：{}。", missing.join("；")),
        )
        .tag("线索转述");
    }

    state.complete_dialogue_relay(relay);
    let mut event = relay_event(relay, state.dialogue_tone);
    apply_relay_rewards(state, relay, &mut event);
    event
}

pub fn reflect(state: &mut GameState, relay: DialogueRelayId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "还没有人在继续这段转述",
            "转述已经发生过，但它需要在对话里被继续追问，才会从事件变成关系里的长期变化。",
        )
        .tag("转述余波");
    };

    if relay.target_dialogue() != active.dialogue {
        return StoryEvent::new(
            "这段余波不属于当前人物",
            format!(
                "你正和{}说话，而“{}”的余波应该继续问{}。",
                active.dialogue.title(),
                relay.source_lead().title(),
                relay.target_dialogue().title()
            ),
        )
        .tag("转述余波");
    }

    if !state.has_completed_dialogue_relay(relay) {
        return StoryEvent::new(
            "这条转述还没有发生",
            format!(
                "“{}”还没有真正转述给{}。先让对方接住这条线索，再追问它留下了什么。",
                relay.source_lead().title(),
                relay.target_dialogue().title()
            ),
        )
        .tag("转述余波");
    }

    if state.has_reflected_dialogue_relay(relay) {
        return StoryEvent::new(
            "这段转述余波已经追问过",
            "对方没有撤回刚才的回应。它已经留在这段关系里，接下来要看你怎样行动，而不是怎样重复确认。",
        )
        .tag("转述余波");
    }

    state.reflect_dialogue_relay(relay);
    let mut event = reflection_event(relay, state.dialogue_tone);
    apply_reflection_rewards(state, relay, &mut event);
    event
}

pub fn echo(state: &mut GameState, relay: DialogueRelayId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "还没有人能接住这段回声",
            "你已经知道对方怎样回应了转述，但这句话需要被带回最初说出线索的人那里，关系才会闭合。",
        )
        .tag("转述回声");
    };

    if relay.source_dialogue() != active.dialogue {
        return StoryEvent::new(
            "这段回声不属于当前人物",
            format!(
                "你正和{}说话，而“{}”的回声应该带回{}。",
                active.dialogue.title(),
                relay.source_lead().title(),
                relay.source_dialogue().title()
            ),
        )
        .tag("转述回声");
    }

    if !state.has_reflected_dialogue_relay(relay) {
        return StoryEvent::new(
            "这段转述还没有余波",
            format!(
                "先去{}那里继续追问“{}”造成的余波，再把答案带回来。",
                relay.target_dialogue().title(),
                relay.source_lead().title()
            ),
        )
        .tag("转述回声");
    }

    if state.has_echoed_dialogue_relay(relay) {
        return StoryEvent::new(
            "这段转述回声已经带回过",
            "最初说出线索的人已经听见了另一个人的回答。再重复一次不会让关系更完整，只会让你暂时逃开下一步行动。",
        )
        .tag("转述回声");
    }

    state.echo_dialogue_relay(relay);
    let mut event = echo_event(relay, state.dialogue_tone);
    apply_echo_rewards(state, relay, &mut event);
    event
}

pub fn anchor(state: &mut GameState, relay: DialogueRelayId) -> StoryEvent {
    if !state.has_echoed_dialogue_relay(relay) {
        return StoryEvent::new(
            "这段回声还没有带回原处",
            format!(
                "“{}”还没有形成完整回声。先完成转述、余波和回谈，再把它留在地点里。",
                relay.source_lead().title()
            ),
        )
        .tag("回声落点");
    }

    if state.has_anchored_dialogue_relay(relay) {
        return StoryEvent::new(
            "这段回声已经有了落点",
            "车站已经替它保存了一个位置。再摆一次，只会像把同一句话钉在同一块木板上。",
        )
        .tag("回声落点");
    }

    if relay.anchor_location() != state.location {
        return StoryEvent::new(
            "这段回声不适合留在这里",
            format!(
                "它应该落在{}，那里保存着最初说出这条线索的人和物。",
                relay.anchor_location().title()
            ),
        )
        .tag("回声落点");
    }

    state.anchor_dialogue_relay(relay);
    let mut event = anchor_event(relay);
    apply_anchor_rewards(state, relay, &mut event);
    event
}

pub fn review_anchor(state: &mut GameState, relay: DialogueRelayId) -> StoryEvent {
    if !state.has_anchored_dialogue_relay(relay) {
        return StoryEvent::new(
            "这里还没有这个回声落点",
            "你想复看一段关系留下的痕迹，但它还没有真正安放到车站里。",
        )
        .tag("回声复看");
    }

    if state.has_reviewed_dialogue_anchor(relay) {
        return StoryEvent::new(
            "这个落点已经复看过",
            "你已经确认过车站怎样保存它。现在该把这份保存带进下一次选择，而不是继续围着它打转。",
        )
        .tag("回声复看");
    }

    if relay.anchor_location() != state.location {
        return StoryEvent::new(
            "这个落点不在这里",
            format!("它留在{}。", relay.anchor_location().title()),
        )
        .tag("回声复看");
    }

    state.review_dialogue_anchor(relay);
    let mut event = review_anchor_event(relay);
    apply_anchor_review_rewards(state, relay, &mut event);
    event
}

pub fn location_anchor_note(state: &GameState, location: Location) -> Option<String> {
    let notes = DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_anchored_dialogue_relay(*relay))
        .filter(|relay| relay.anchor_location() == location)
        .map(|relay| {
            if state.has_reviewed_dialogue_anchor(relay) {
                relay.location_reviewed_note()
            } else {
                relay.location_anchor_note()
            }
        })
        .collect::<Vec<_>>();
    if notes.is_empty() {
        None
    } else {
        Some(format!(" {}", notes.join(" ")))
    }
}

pub fn active_relay_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_relays(state, active)
        .into_iter()
        .find(|relay| relay.enabled)
        .map(|relay| format!("可以在当前对话里转述线索：{}。", relay.label))
}

pub fn active_reflection_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_reflections(state, active)
        .into_iter()
        .find(|reflection| reflection.enabled)
        .map(|reflection| format!("可以继续追问转述余波：{}。", reflection.label))
}

pub fn active_echo_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_echoes(state, active)
        .into_iter()
        .find(|echo| echo.enabled)
        .map(|echo| format!("可以把转述回声带回原人物：{}。", echo.label))
}

pub fn relay_objective_hint(state: &GameState) -> Option<String> {
    if state.active_dialogue.is_some() {
        return active_relay_objective_hint(state)
            .or_else(|| active_reflection_objective_hint(state))
            .or_else(|| active_echo_objective_hint(state));
    }
    if let Some(relay) = DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_echoed_dialogue_relay(*relay))
        .filter(|relay| !state.has_anchored_dialogue_relay(*relay))
        .find(|relay| relay.anchor_location() == state.location)
    {
        return Some(format!(
            "可以把转述回声落到地点里：{}。",
            relay.anchor_label()
        ));
    }
    if let Some(relay) = DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_anchored_dialogue_relay(*relay))
        .filter(|relay| !state.has_reviewed_dialogue_anchor(*relay))
        .find(|relay| relay.anchor_location() == state.location)
    {
        return Some(format!(
            "可以复看回声落点：{}。",
            relay.review_anchor_label()
        ));
    }
    if let Some(relay) = DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_reflected_dialogue_relay(*relay))
        .filter(|relay| !state.has_echoed_dialogue_relay(*relay))
        .find(|relay| relay.source_dialogue().location() == state.location)
    {
        return Some(format!(
            "可以进入{}的对话，把{}的回应带回去。",
            relay.source_dialogue().title(),
            relay.target_dialogue().title()
        ));
    }
    if let Some(relay) = DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| state.has_completed_dialogue_relay(*relay))
        .filter(|relay| !state.has_reflected_dialogue_relay(*relay))
        .find(|relay| relay.target_dialogue().location() == state.location)
    {
        return Some(format!(
            "可以进入{}的对话，追问“{}”转述后的余波。",
            relay.target_dialogue().title(),
            relay.source_lead().title()
        ));
    }
    DialogueRelayId::ALL
        .iter()
        .copied()
        .filter(|relay| !state.has_completed_dialogue_relay(*relay))
        .filter(|relay| relay_visible(state, *relay))
        .find(|relay| {
            missing_requirements(state, *relay).is_empty()
                && relay.target_dialogue().location() == state.location
        })
        .map(|relay| {
            format!(
                "可以进入{}的对话，转述“{}”。",
                relay.target_dialogue().title(),
                relay.source_lead().title()
            )
        })
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let completed = state.completed_dialogue_relays.len();
    if completed == 0 {
        return None;
    }
    let reflected = state.reflected_dialogue_relays.len();
    let echoed = state.echoed_dialogue_relays.len();
    let anchored = state.anchored_dialogue_relays.len();
    let reviewed = state.reviewed_dialogue_anchors.len();
    Some(format!(
        "你完成了 {completed} 条线索转述，追问了 {reflected} 段转述余波，带回了 {echoed} 段转述回声，安放了 {anchored} 个回声落点，并复看了 {reviewed} 个地点余痕。今晚的对话不再只是一问一答，而是有人把别人的证词继续递给下一个人。"
    ))
}

impl DialogueRelayId {
    pub const ALL: [Self; DIALOGUE_RELAY_COUNT] = [
        Self::MirrorToChild,
        Self::EmptySeatToClerk,
        Self::ReturnStubToTraveler,
        Self::TwoSeatMapToChild,
        Self::HomeworkMarginToTraveler,
        Self::WhiteLineChalkToKeeper,
        Self::MinuteHandNoteToClerk,
        Self::BroadcastDraftToTraveler,
    ];

    fn source_lead(self) -> DialogueLeadId {
        match self {
            Self::MirrorToChild => DialogueLeadId::RainUnderBench,
            Self::EmptySeatToClerk => DialogueLeadId::EmptySeatLedger,
            Self::ReturnStubToTraveler => DialogueLeadId::ReturnStub,
            Self::TwoSeatMapToChild => DialogueLeadId::TwoSeatMap,
            Self::HomeworkMarginToTraveler => DialogueLeadId::HomeworkMargin,
            Self::WhiteLineChalkToKeeper => DialogueLeadId::WhiteLineChalk,
            Self::MinuteHandNoteToClerk => DialogueLeadId::MinuteHandNote,
            Self::BroadcastDraftToTraveler => DialogueLeadId::BroadcastDraft,
        }
    }

    fn source_dialogue(self) -> DialogueId {
        match self {
            Self::MirrorToChild | Self::EmptySeatToClerk => DialogueId::Traveler,
            Self::ReturnStubToTraveler | Self::TwoSeatMapToChild => DialogueId::Clerk,
            Self::HomeworkMarginToTraveler | Self::WhiteLineChalkToKeeper => DialogueId::Child,
            Self::MinuteHandNoteToClerk | Self::BroadcastDraftToTraveler => DialogueId::Keeper,
        }
    }

    fn target_dialogue(self) -> DialogueId {
        match self {
            Self::MirrorToChild | Self::TwoSeatMapToChild => DialogueId::Child,
            Self::EmptySeatToClerk | Self::MinuteHandNoteToClerk => DialogueId::Clerk,
            Self::ReturnStubToTraveler
            | Self::HomeworkMarginToTraveler
            | Self::BroadcastDraftToTraveler => DialogueId::Traveler,
            Self::WhiteLineChalkToKeeper => DialogueId::Keeper,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::MirrorToChild => "转述线索：让孩子看见镜片",
            Self::EmptySeatToClerk => "转述线索：把空座页码交给窗口",
            Self::ReturnStubToTraveler => "转述线索：问老人退票根",
            Self::TwoSeatMapToChild => "转述线索：把双座图说给孩子",
            Self::HomeworkMarginToTraveler => "转述线索：把页边日期说给老人",
            Self::WhiteLineChalkToKeeper => "转述线索：请站务员看新白线",
            Self::MinuteHandNoteToClerk => "转述线索：把分针借据带到窗口",
            Self::BroadcastDraftToTraveler => "转述线索：把广播缺口说给老人",
        }
    }

    fn reflection_label(self) -> &'static str {
        match self {
            Self::MirrorToChild => "追问转述余波：镜片以后",
            Self::EmptySeatToClerk => "追问转述余波：空座入账以后",
            Self::ReturnStubToTraveler => "追问转述余波：退票根以后",
            Self::TwoSeatMapToChild => "追问转述余波：双座图以后",
            Self::HomeworkMarginToTraveler => "追问转述余波：页边日期以后",
            Self::WhiteLineChalkToKeeper => "追问转述余波：新白线以后",
            Self::MinuteHandNoteToClerk => "追问转述余波：归还栏以后",
            Self::BroadcastDraftToTraveler => "追问转述余波：广播缺口以后",
        }
    }

    fn echo_label(self) -> &'static str {
        match self {
            Self::MirrorToChild => "带回转述回声：孩子说镜片以后",
            Self::EmptySeatToClerk => "带回转述回声：窗口承认空座",
            Self::ReturnStubToTraveler => "带回转述回声：老人听见旧手续",
            Self::TwoSeatMapToChild => "带回转述回声：孩子保留 07B",
            Self::HomeworkMarginToTraveler => "带回转述回声：老人听见页边日期",
            Self::WhiteLineChalkToKeeper => "带回转述回声：站务员登记新白线",
            Self::MinuteHandNoteToClerk => "带回转述回声：窗口写下归还栏",
            Self::BroadcastDraftToTraveler => "带回转述回声：老人承认广播缺口",
        }
    }

    fn anchor_label(self) -> &'static str {
        match self {
            Self::MirrorToChild => "安放回声：把镜片留在报纸雨痕旁",
            Self::EmptySeatToClerk => "安放回声：把空座页码贴进窗口账夹",
            Self::ReturnStubToTraveler => "安放回声：把退票根压回退票夹",
            Self::TwoSeatMapToChild => "安放回声：把 07B 虚线画到座位图上",
            Self::HomeworkMarginToTraveler => "安放回声：把页边日期夹回作业本",
            Self::WhiteLineChalkToKeeper => "安放回声：重描一寸可以自己开的白线",
            Self::MinuteHandNoteToClerk => "安放回声：把归还栏抄到分针背面",
            Self::BroadcastDraftToTraveler => "安放回声：给广播稿补录那一秒",
        }
    }

    fn review_anchor_label(self) -> &'static str {
        match self {
            Self::MirrorToChild => "复看回声落点：镜片和雨痕",
            Self::EmptySeatToClerk => "复看回声落点：空座账夹",
            Self::ReturnStubToTraveler => "复看回声落点：退票根",
            Self::TwoSeatMapToChild => "复看回声落点：07B 虚线",
            Self::HomeworkMarginToTraveler => "复看回声落点：页边日期",
            Self::WhiteLineChalkToKeeper => "复看回声落点：新白线",
            Self::MinuteHandNoteToClerk => "复看回声落点：分针归还栏",
            Self::BroadcastDraftToTraveler => "复看回声落点：广播缺口",
        }
    }

    fn anchor_location(self) -> Location {
        self.source_dialogue().location()
    }

    fn title(self) -> &'static str {
        match self {
            Self::MirrorToChild => "镜片转给孩子",
            Self::EmptySeatToClerk => "空座页码转给售票员",
            Self::ReturnStubToTraveler => "退票根转给老人",
            Self::TwoSeatMapToChild => "双座图转给孩子",
            Self::HomeworkMarginToTraveler => "页边日期转给老人",
            Self::WhiteLineChalkToKeeper => "新白线转给站务员",
            Self::MinuteHandNoteToClerk => "分针借据转给售票员",
            Self::BroadcastDraftToTraveler => "广播缺口转给老人",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::MirrorToChild => "孩子看过镜片，知道老人说的雨也会留下证据。",
            Self::EmptySeatToClerk => "售票员收下空座页码，窗口第一次承认第二个座位不是空想。",
            Self::ReturnStubToTraveler => "老人听完退票根，把你曾经排过的队也算进今晚。",
            Self::TwoSeatMapToChild => "孩子听见双座图，知道返程不是你一个人擅自安排的抵账。",
            Self::HomeworkMarginToTraveler => "老人听见页边日期，报纸下的沉默开始替孩子让座。",
            Self::WhiteLineChalkToKeeper => "站务员看过新白线，承认守夜也要守住人的边界。",
            Self::MinuteHandNoteToClerk => "售票员听见分针借据，窗口把返程规则从价格栏移到归还栏。",
            Self::BroadcastDraftToTraveler => {
                "老人听见广播缺口，终于不再把警告当作只属于自己的雨声。"
            }
        }
    }

    fn reflection_review(self) -> &'static str {
        match self {
            Self::MirrorToChild => "孩子不只看过镜片，也说出了镜片之后你该怎样少躲一点。",
            Self::EmptySeatToClerk => "售票员把空座页码留在窗口边，后续手续开始承认两个人。",
            Self::ReturnStubToTraveler => "老人把退票根之后的旧队伍说清，离开不再只像逃跑。",
            Self::TwoSeatMapToChild => "孩子确认 07B 不是奖品，双座图因此保留了他拒绝的余地。",
            Self::HomeworkMarginToTraveler => {
                "老人把页边日期留给雨声，后续对话不再替孩子催促原谅。"
            }
            Self::WhiteLineChalkToKeeper => "站务员把新白线写进巡夜规矩，边界开始被当作照顾。",
            Self::MinuteHandNoteToClerk => "售票员把分针借据写成归还栏，窗口后续不再只收价格。",
            Self::BroadcastDraftToTraveler => {
                "老人承认广播缺口不是别人的错误，而是他也参与过的沉默。"
            }
        }
    }

    fn echo_review(self) -> &'static str {
        match self {
            Self::MirrorToChild => {
                "老人听见孩子怎样回答镜片，雨痕终于从他的比喻里走到另一个人手上。"
            }
            Self::EmptySeatToClerk => "老人知道窗口接住了空座页码，第二个座位不再只夹在报纸里。",
            Self::ReturnStubToTraveler => {
                "售票员知道老人承认旧手续，退票根不再只是窗口内部的证据。"
            }
            Self::TwoSeatMapToChild => "售票员知道孩子保留拒绝 07B 的权利，双座图因此更接近承诺。",
            Self::HomeworkMarginToTraveler => {
                "孩子知道老人听见页边日期，日期从控诉变成能被别人保管的事实。"
            }
            Self::WhiteLineChalkToKeeper => {
                "孩子知道站务员会登记新白线，边界从私人坚持变成公共规矩。"
            }
            Self::MinuteHandNoteToClerk => {
                "站务员知道窗口写下归还栏，分针借据终于找到能办理它的人。"
            }
            Self::BroadcastDraftToTraveler => {
                "站务员知道老人承认广播缺口，警告里少掉的一秒开始有人补录。"
            }
        }
    }

    fn anchor_review(self) -> &'static str {
        match self {
            Self::MirrorToChild => "镜片留在报纸雨痕旁，孩子的回应从此能被候车厅看见。",
            Self::EmptySeatToClerk => "空座页码进入窗口账夹，第二个座位不再只是老人报纸里的保留。",
            Self::ReturnStubToTraveler => "退票根回到退票夹，旧手续第一次像能被重新办理。",
            Self::TwoSeatMapToChild => "07B 被画成虚线，座位图从安排变成允许。",
            Self::HomeworkMarginToTraveler => "页边日期回到作业本，孩子的时间不再只靠记恨保存。",
            Self::WhiteLineChalkToKeeper => "新白线被重描一寸，月台把边界当作照顾保存下来。",
            Self::MinuteHandNoteToClerk => "归还栏被抄到分针背面，旧钟终于有了一行手续文字。",
            Self::BroadcastDraftToTraveler => "广播稿补进那一秒，缺口不再完全留给沉默代管。",
        }
    }

    fn anchor_review_after(self) -> &'static str {
        match self {
            Self::MirrorToChild => {
                "你复看过镜片和雨痕。候车厅已经知道，证词可以从一个人手里传到另一个人眼前。"
            }
            Self::EmptySeatToClerk => "你复看过空座账夹。窗口已经把第二个座位从空想写成手续。",
            Self::ReturnStubToTraveler => {
                "你复看过退票根。旧手续不再只证明失败，也证明有人曾经想回来。"
            }
            Self::TwoSeatMapToChild => "你复看过 07B 虚线。座位图已经学会不把承诺说成命令。",
            Self::HomeworkMarginToTraveler => "你复看过页边日期。孩子保存的时间已经被车站承认。",
            Self::WhiteLineChalkToKeeper => "你复看过新白线。月台的边界不再只像旧伤，也像保护。",
            Self::MinuteHandNoteToClerk => {
                "你复看过分针归还栏。旧钟知道借出的一分钟仍能被办理归还。"
            }
            Self::BroadcastDraftToTraveler => "你复看过广播缺口。那一秒空白不再只属于沉默。",
        }
    }

    fn location_anchor_note(self) -> &'static str {
        match self {
            Self::MirrorToChild => "报纸雨痕旁压着一枚镜片，像有人把一句话钉在光里。",
            Self::EmptySeatToClerk => "窗口账夹里多出一张空座页码，边角还没有干。",
            Self::ReturnStubToTraveler => "退票夹深处有一张被压平的票根，不再像废纸。",
            Self::TwoSeatMapToChild => "座位图上的 07B 被画成虚线，线条细得像一次谨慎的允许。",
            Self::HomeworkMarginToTraveler => "作业本页边夹回了一串日期，纸页因此沉了一点。",
            Self::WhiteLineChalkToKeeper => "白线外侧多了一寸新粉笔，没有覆盖旧痕。",
            Self::MinuteHandNoteToClerk => "旧钟分针背面多了一行小字：归还明天。",
            Self::BroadcastDraftToTraveler => "广播稿缺口处留着一秒空白，像正在等名字落下。",
        }
    }

    fn location_reviewed_note(self) -> &'static str {
        match self {
            Self::MirrorToChild => "你已经复看过那枚镜片；它安静地照着报纸雨痕。",
            Self::EmptySeatToClerk => "你已经复看过空座账夹；窗口没有把页码取下。",
            Self::ReturnStubToTraveler => "你已经复看过退票根；它像一枚终于归档的旧证词。",
            Self::TwoSeatMapToChild => "你已经复看过 07B 虚线；那条线仍然没有逼谁坐下。",
            Self::HomeworkMarginToTraveler => "你已经复看过页边日期；它们没有再被读成审判。",
            Self::WhiteLineChalkToKeeper => "你已经复看过新白线；旧粉笔和新粉笔并排留着。",
            Self::MinuteHandNoteToClerk => "你已经复看过分针归还栏；旧钟经过那里时会稍微放轻。",
            Self::BroadcastDraftToTraveler => "你已经复看过广播缺口；那一秒空白有了等待的形状。",
        }
    }
}

fn can_share_now(state: &GameState, relay: DialogueRelayId) -> bool {
    state
        .active_dialogue
        .is_some_and(|active| active.dialogue == relay.target_dialogue())
        && relay_visible(state, relay)
        && missing_requirements(state, relay).is_empty()
        && !state.has_completed_dialogue_relay(relay)
}

fn relay_visible(state: &GameState, relay: DialogueRelayId) -> bool {
    state.has_returned_dialogue_lead(relay.source_lead())
}

fn missing_requirements(state: &GameState, relay: DialogueRelayId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        relay_visible(state, relay),
        "先完成对应线索的回谈",
    );
    match relay.target_dialogue() {
        DialogueId::Traveler => require(
            &mut missing,
            state.traveler_depth > 0 || state.has_flag(Flag::TravelerTrusted),
            "先让老人愿意听你把别人的话说完",
        ),
        DialogueId::Clerk => require(
            &mut missing,
            state.clerk_depth > 0 || state.has_flag(Flag::ClerkMet),
            "先让售票员承认你不是普通旅客",
        ),
        DialogueId::Child => require(
            &mut missing,
            state.child_depth > 0 || state.has_flag(Flag::MetChild),
            "先和孩子建立能说话的位置",
        ),
        DialogueId::Keeper => require(
            &mut missing,
            state.keeper_depth > 0
                || state.keeper_trust > 0
                || state.has_flag(Flag::HeardClockTruth),
            "先让站务员把守夜说成自己的事",
        ),
    }
    missing
}

fn relay_progress(
    state: &GameState,
    relay: DialogueRelayId,
    visible: bool,
    completed: bool,
    reflected: bool,
    echoed: bool,
    anchored: bool,
    reviewed: bool,
    missing_count: usize,
) -> u8 {
    if reviewed {
        100
    } else if anchored {
        97
    } else if echoed {
        96
    } else if reflected {
        88
    } else if completed {
        74
    } else if can_share_now(state, relay) {
        68
    } else if visible && missing_count == 0 && relay.target_dialogue().location() == state.location
    {
        60
    } else if visible && missing_count == 0 {
        48
    } else if visible {
        35
    } else {
        0
    }
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn apply_relay_rewards(state: &mut GameState, relay: DialogueRelayId, event: &mut StoryEvent) {
    match relay {
        DialogueRelayId::MirrorToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "转述：镜片照见承诺",
            );
        }
        DialogueRelayId::EmptySeatToClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "转述：空座入账");
        }
        DialogueRelayId::ReturnStubToTraveler => {
            remember_tag(state, event, Flag::TravelerTrusted, "转述：退票被承认");
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "转述：旧手续");
        }
        DialogueRelayId::TwoSeatMapToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "转述：双座不是奖品",
            );
        }
        DialogueRelayId::HomeworkMarginToTraveler => {
            remember_tag(state, event, Flag::TravelerTrusted, "转述：页边让座");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueRelayId::WhiteLineChalkToKeeper => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "转述：边界归档",
            );
        }
        DialogueRelayId::MinuteHandNoteToClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "转述：归还栏");
        }
        DialogueRelayId::BroadcastDraftToTraveler => {
            remember_tag(state, event, Flag::TravelerTrusted, "转述：警告分给后来者");
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "转述：广播补全",
            );
        }
    }
}

fn apply_reflection_rewards(state: &mut GameState, relay: DialogueRelayId, event: &mut StoryEvent) {
    advance_target_relationship(state, relay.target_dialogue());
    match relay {
        DialogueRelayId::MirrorToChild | DialogueRelayId::TwoSeatMapToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "余波：孩子保留选择",
            );
        }
        DialogueRelayId::EmptySeatToClerk | DialogueRelayId::MinuteHandNoteToClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "余波：手续继续生效");
        }
        DialogueRelayId::ReturnStubToTraveler
        | DialogueRelayId::HomeworkMarginToTraveler
        | DialogueRelayId::BroadcastDraftToTraveler => {
            remember_tag(state, event, Flag::TravelerTrusted, "余波：老人继续听");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueRelayId::WhiteLineChalkToKeeper => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "余波：边界写入守夜",
            );
        }
    }
}

fn apply_echo_rewards(state: &mut GameState, relay: DialogueRelayId, event: &mut StoryEvent) {
    advance_target_relationship(state, relay.source_dialogue());
    match relay.source_dialogue() {
        DialogueId::Traveler => {
            remember_tag(state, event, Flag::TravelerTrusted, "回声：老人接住回应");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueId::Clerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "回声：窗口接住回应");
        }
        DialogueId::Child => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "回声：孩子接住回应",
            );
        }
        DialogueId::Keeper => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "回声：站务接住回应",
            );
        }
    }
}

fn apply_anchor_rewards(state: &mut GameState, relay: DialogueRelayId, event: &mut StoryEvent) {
    state.synthesis_depth = state
        .synthesis_depth
        .saturating_add(1)
        .min(NPC_THREAD_STEPS);
    match relay {
        DialogueRelayId::MirrorToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(state, event, Flag::UnderstoodChildPromise, "落点：镜片留证");
        }
        DialogueRelayId::EmptySeatToClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "落点：空座入账");
        }
        DialogueRelayId::ReturnStubToTraveler => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "落点：旧手续留档");
        }
        DialogueRelayId::TwoSeatMapToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedChildTruth, "落点：座位允许");
        }
        DialogueRelayId::HomeworkMarginToTraveler => {
            state.child_trust = (state.child_trust + 1).min(5);
            remember_tag(state, event, Flag::TravelerTrusted, "落点：日期被保管");
        }
        DialogueRelayId::WhiteLineChalkToKeeper => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "落点：白线边界");
        }
        DialogueRelayId::MinuteHandNoteToClerk => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "落点：分针归还",
            );
        }
        DialogueRelayId::BroadcastDraftToTraveler => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "落点：广播补秒",
            );
        }
    }
}

fn apply_anchor_review_rewards(
    state: &mut GameState,
    relay: DialogueRelayId,
    event: &mut StoryEvent,
) {
    state.synthesis_depth = state
        .synthesis_depth
        .saturating_add(1)
        .min(NPC_THREAD_STEPS);
    advance_target_relationship(state, relay.source_dialogue());
    match relay {
        DialogueRelayId::MirrorToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("复看：镜片确认".to_string());
        }
        DialogueRelayId::EmptySeatToClerk => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("复看：空座入账".to_string());
        }
        DialogueRelayId::ReturnStubToTraveler => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "复看：旧手续归档");
        }
        DialogueRelayId::TwoSeatMapToChild => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("复看：座位仍可拒绝".to_string());
        }
        DialogueRelayId::HomeworkMarginToTraveler => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("复看：日期被保存".to_string());
        }
        DialogueRelayId::WhiteLineChalkToKeeper => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("复看：白线仍在".to_string());
        }
        DialogueRelayId::MinuteHandNoteToClerk => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("复看：分钟归还".to_string());
        }
        DialogueRelayId::BroadcastDraftToTraveler => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "复看：广播缺口有名",
            );
        }
    }
}

fn advance_target_relationship(state: &mut GameState, dialogue: DialogueId) {
    match dialogue {
        DialogueId::Traveler => {
            state.traveler_depth = state.traveler_depth.saturating_add(1).min(NPC_THREAD_STEPS);
        }
        DialogueId::Clerk => {
            state.clerk_depth = state.clerk_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.remember(Flag::ClerkMet);
        }
        DialogueId::Child => {
            state.child_depth = state.child_depth.saturating_add(1).min(NPC_THREAD_STEPS);
            state.remember(Flag::MetChild);
        }
        DialogueId::Keeper => {
            state.keeper_depth = state.keeper_depth.saturating_add(1).min(NPC_THREAD_STEPS);
        }
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn relay_event(relay: DialogueRelayId, tone: DialogueTone) -> StoryEvent {
    let tone_prefix = match tone {
        DialogueTone::Listening => "你先让对方把沉默放稳，再把这条线索递过去。 ",
        DialogueTone::Gentle => "你把转述说得很轻，像把一杯水推过桌面。 ",
        DialogueTone::Direct => "你没有绕开问题，直接把别人的证词放进当前对话。 ",
    };
    let (title, body) = match relay {
        DialogueRelayId::MirrorToChild => (
            "线索转述：孩子和镜片",
            "孩子接过镜片，没有照自己的脸，只照白线。他说：所以他说的雨不是用来躲开的，是会滴到地上的。你点头。他把镜片还给你，说那就别再只说你难过。",
        ),
        DialogueRelayId::EmptySeatToClerk => (
            "线索转述：售票员和空座页码",
            "你把空座页码报给窗口。售票员翻账册的速度慢下来，像第一次发现空座也需要手续。她说：如果账上有 07B，那返程就不能只按一个人收费。",
        ),
        DialogueRelayId::ReturnStubToTraveler => (
            "线索转述：老人和退票根",
            "老人听见退票根号码时，把报纸折出一道旧折痕。他说：你排过队，窗口也见过你。那你不是突然想走，是一直没有把离开和回来分清。",
        ),
        DialogueRelayId::TwoSeatMapToChild => (
            "线索转述：孩子和双座图",
            "你把两张座位图摊在白线外。孩子先看 07B，又看你。他说：如果我不上车，它也还在那里吗？你说在。他嗯了一声，像把座位从奖品里拿出来。",
        ),
        DialogueRelayId::HomeworkMarginToTraveler => (
            "线索转述：老人和页边日期",
            "老人听完那些页边日期，很久没有翻报纸。他说：小孩记日期，比大人记理由诚实。雨停以前，先别急着替他说原谅。",
        ),
        DialogueRelayId::WhiteLineChalkToKeeper => (
            "线索转述：站务员和新白线",
            "站务员跟你一起看那段新粉笔。他说：守夜的人总以为边界是用来拦人的。也许有些线只是为了让里面的人知道，他可以自己开门。",
        ),
        DialogueRelayId::MinuteHandNoteToClerk => (
            "线索转述：售票员和分针借据",
            "你把分针背面的字念给售票员。她在价格栏旁边写下“归还”。票章悬了一会儿，像终于明白不是所有手续都该收钱。",
        ),
        DialogueRelayId::BroadcastDraftToTraveler => (
            "线索转述：老人和广播缺口",
            "老人听见广播稿里少掉的名字，把报纸放低。那一刻雨声没有替他说话。他说：警告要是只记得我，就还是我在逃。",
        ),
    };
    StoryEvent::new(title, format!("{tone_prefix}{body}"))
        .tag("线索转述")
        .tag("交叉对话")
        .tag("自由对话")
        .tag(format!("语气：{}", tone.name()))
}

fn reflection_event(relay: DialogueRelayId, tone: DialogueTone) -> StoryEvent {
    let tone_prefix = match tone {
        DialogueTone::Listening => "你没有把转述当成结论，只等对方把第二句话说出来。 ",
        DialogueTone::Gentle => "你把问题放得很低，给对方留下不立刻回答的余地。 ",
        DialogueTone::Direct => "你直接问：听见这件事以后，你准备怎样改变？ ",
    };
    let (title, body) = match relay {
        DialogueRelayId::MirrorToChild => (
            "转述余波：镜片以后",
            "孩子把镜片放在掌心里，问你：如果它能照见你躲开的地方，那你以后是不是也要先看一眼，再说自己没办法？他没有逼你答应，只把镜片推回来，像把问题还给大人。",
        ),
        DialogueRelayId::EmptySeatToClerk => (
            "转述余波：空座入账以后",
            "售票员把 07B 的页码抄到窗口便笺上。她说：账不是为了惩罚人，是为了下一次有人排队时，不必从头证明另一个人也存在。",
        ),
        DialogueRelayId::ReturnStubToTraveler => (
            "转述余波：退票根以后",
            "老人说他以前也以为退票根只是废纸。后来才知道，有些废纸的用处，是证明你曾经想回来，只是没有找到能被承认的窗口。",
        ),
        DialogueRelayId::TwoSeatMapToChild => (
            "转述余波：双座图以后",
            "孩子用手指点着 07B，说：如果它是我的位置，那我可以暂时不坐。你说可以。他看你很久，像确认这一次“可以”不是催促的另一种说法。",
        ),
        DialogueRelayId::HomeworkMarginToTraveler => (
            "转述余波：页边日期以后",
            "老人把报纸折好，说他以前最怕孩子记日期，因为日期不会替大人修饰。现在他觉得也好，至少有人记得事情是一天一天被拖坏的。",
        ),
        DialogueRelayId::WhiteLineChalkToKeeper => (
            "转述余波：新白线以后",
            "站务员说他会把白线登记成“可由本人重画”。你听见这句业务话，反而比安慰更可靠：它把孩子的边界从情绪，写成了制度也必须承认的事。",
        ),
        DialogueRelayId::MinuteHandNoteToClerk => (
            "转述余波：归还栏以后",
            "售票员把价格表最下面空出一行，写上“归还明天”。她说：也许返程票不是买来的，是每次有人不再拿痛苦抵价时，慢慢补齐的。",
        ),
        DialogueRelayId::BroadcastDraftToTraveler => (
            "转述余波：广播缺口以后",
            "老人终于承认，警告里少掉的名字不是被雾吞了，是被他说话的人跳过了。他说：如果还要播，就把我跳过的那一秒也播进去。",
        ),
    };
    StoryEvent::new(title, format!("{tone_prefix}{body}"))
        .tag("转述余波")
        .tag("持续对话")
        .tag("自由对话")
        .tag(format!("语气：{}", tone.name()))
}

fn echo_event(relay: DialogueRelayId, tone: DialogueTone) -> StoryEvent {
    let tone_prefix = match tone {
        DialogueTone::Listening => "你把对方的回应原样放下，没有急着替它加意义。 ",
        DialogueTone::Gentle => "你把回声带得很轻，像怕最初说话的人被自己的句子撞疼。 ",
        DialogueTone::Direct => "你直接告诉对方：那句话已经被另一个人听见，并且回答了。 ",
    };
    let (title, body) = match relay {
        DialogueRelayId::MirrorToChild => (
            "转述回声：老人听见孩子和镜片",
            "老人听完孩子的话，把报纸合上。他说：原来雨滴到地上以后，还会被别人捡起来看。那我以前一直说雨，是不是也在等他替我证明我没有全在逃？",
        ),
        DialogueRelayId::EmptySeatToClerk => (
            "转述回声：老人听见窗口和空座",
            "你告诉老人，售票员把 07B 写进手续。他摸了摸报纸里的折痕，说：好，空座终于不必靠我这张破纸替它占位了。",
        ),
        DialogueRelayId::ReturnStubToTraveler => (
            "转述回声：售票员听见老人和退票根",
            "售票员知道老人承认旧队伍后，把退票根压平。她说：原来窗口记得的不是纸，是人反复回来时越来越不敢说出口的事。",
        ),
        DialogueRelayId::TwoSeatMapToChild => (
            "转述回声：售票员听见孩子和双座图",
            "你告诉售票员，孩子问 07B 可不可以暂时空着。她在图上没有盖章，只画了一道虚线：可保留，不强制登车。",
        ),
        DialogueRelayId::HomeworkMarginToTraveler => (
            "转述回声：孩子听见老人和页边日期",
            "孩子听见老人没有催他原谅，肩膀松了一点。他说：那他可以继续看报纸，但下次别把报纸当墙。",
        ),
        DialogueRelayId::WhiteLineChalkToKeeper => (
            "转述回声：孩子听见站务员和新白线",
            "你告诉孩子，站务员会把白线登记成他可以自己重画的边界。孩子低头看脚尖，说：那这条线以后也算我写的，不只是你当年命令留下的。",
        ),
        DialogueRelayId::MinuteHandNoteToClerk => (
            "转述回声：站务员听见窗口和归还栏",
            "站务员听见售票员写下归还栏，像终于从钟声里听见人声。他说：好，借出的分钟总算有地方办理归还。",
        ),
        DialogueRelayId::BroadcastDraftToTraveler => (
            "转述回声：站务员听见老人和广播缺口",
            "你告诉站务员，老人承认警告里跳过的一秒。站务员把磁带倒回去，说：那一秒不长，但够我们重新录进去了。",
        ),
    };
    StoryEvent::new(title, format!("{tone_prefix}{body}"))
        .tag("转述回声")
        .tag("双向对话")
        .tag("自由对话")
        .tag(format!("语气：{}", tone.name()))
}

fn anchor_event(relay: DialogueRelayId) -> StoryEvent {
    let (title, body) = match relay {
        DialogueRelayId::MirrorToChild => (
            "回声落点：镜片和报纸雨痕",
            "你把镜片压在报纸雨痕旁。它照不出完整的脸，只照见一小块候车厅灯光。老人和孩子的两句话都留在这里：雨会落地，人也该少躲一点。",
        ),
        DialogueRelayId::EmptySeatToClerk => (
            "回声落点：空座页码入账",
            "你把空座页码贴进窗口账夹。售票员没有撕掉它，只在旁边写了一行小字：第二个座位，保留至本人决定。",
        ),
        DialogueRelayId::ReturnStubToTraveler => (
            "回声落点：退票根回夹",
            "退票根被你压回退票夹深处。纸边还有旧水痕，但它终于不像废弃手续，而像某次回来没能办完的证明。",
        ),
        DialogueRelayId::TwoSeatMapToChild => (
            "回声落点：07B 虚线",
            "你在座位图上把 07B 画成虚线。虚线没有取消座位，只把它从命令里放出来，等一个真正愿意坐下的人。",
        ),
        DialogueRelayId::HomeworkMarginToTraveler => (
            "回声落点：页边日期归位",
            "你把页边日期夹回作业本。它们不再像控诉排成队，也不像赦免书，只是一串被车站承认真实存在过的日子。",
        ),
        DialogueRelayId::WhiteLineChalkToKeeper => (
            "回声落点：重描白线",
            "你沿着旧粉笔外侧重描一寸。新线不盖住旧线，只给它留出可以呼吸的边。月台因此安静了一点。",
        ),
        DialogueRelayId::MinuteHandNoteToClerk => (
            "回声落点：分针归还栏",
            "你把“归还明天”抄到分针背面。旧钟走过那一格时没有响，像终于学会不把每一分钟都宣判成债。",
        ),
        DialogueRelayId::BroadcastDraftToTraveler => (
            "回声落点：补录的一秒",
            "你在广播稿缺口处留下一秒空白，再把第二个名字写进去。磁带没有立刻播放，但它已经知道自己下次该怎样开口。",
        ),
    };
    StoryEvent::new(title, body)
        .tag("回声落点")
        .tag("自由探索")
        .tag("地点改变")
}

fn review_anchor_event(relay: DialogueRelayId) -> StoryEvent {
    let (title, body) = match relay {
        DialogueRelayId::MirrorToChild => (
            "回声复看：镜片照着雨痕",
            "你又回到候车厅。镜片仍压在报纸雨痕旁，没有被老人收回，也没有被孩子拿走。它只把灯光切成一小片，像提醒你：对话落到地点里以后，就不再完全属于任何一个说话的人。",
        ),
        DialogueRelayId::EmptySeatToClerk => (
            "回声复看：空座账夹",
            "窗口账夹还夹着那张空座页码。售票员没有把它整理进价格栏，而是让它留在最容易被翻到的位置。空座仍然空着，但它现在有了被承认的手续。",
        ),
        DialogueRelayId::ReturnStubToTraveler => (
            "回声复看：退票根归档",
            "你翻开退票夹，票根在旧水痕之间平整地躺着。它没有让任何人立刻回家，却让“曾经想回来”这件事不再像失败的附录。",
        ),
        DialogueRelayId::TwoSeatMapToChild => (
            "回声复看：07B 虚线",
            "座位图上的 07B 虚线还在。它比盖章更轻，也比空白更认真：它保存一个位置，同时保存一个人不马上接受这个位置的权利。",
        ),
        DialogueRelayId::HomeworkMarginToTraveler => (
            "回声复看：页边日期",
            "作业本页边的日期被夹回去以后，纸页并没有变轻。你看见那些日期仍在那里，像一些不请求原谅的证人，只要求以后不要再被跳过。",
        ),
        DialogueRelayId::WhiteLineChalkToKeeper => (
            "回声复看：新白线",
            "月台白线外侧那一寸新粉笔还没有散。旧线说明曾经发生过什么，新线说明以后可以怎样站立。你第一次觉得边界不是拒绝人，而是让人有地方开口。",
        ),
        DialogueRelayId::MinuteHandNoteToClerk => (
            "回声复看：分针归还栏",
            "旧钟分针走过那行小字时，没有发出多余的声响。归还明天。四个字贴在时间背面，像一张终于不再催款的收据。",
        ),
        DialogueRelayId::BroadcastDraftToTraveler => (
            "回声复看：广播缺口",
            "广播稿缺口处仍留着那一秒空白。你没有把它填满，只确认它已经能容下第二个名字。沉默如果被标出边界，也就不再完全替罪。",
        ),
    };
    StoryEvent::new(title, body)
        .tag("回声复看")
        .tag("自由探索")
        .tag("地点回访")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::DialogueNodeId;

    #[test]
    fn relay_summaries_track_returned_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = relay_summaries(&state);
        let mirror = initial
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "未显形");
        assert_eq!(mirror.progress, 0);

        state.return_dialogue_lead(DialogueLeadId::RainUnderBench);
        state.location = DialogueId::Child.location();
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
        });

        let visible = relay_summaries(&state);
        let mirror = visible
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "待转述");
        assert!(!mirror.ready);

        state.remember(Flag::MetChild);
        let ready = relay_summaries(&state);
        let mirror = ready
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "可转述");
        assert!(mirror.ready);

        let event = share(&mut state, DialogueRelayId::MirrorToChild);
        assert!(event.tags.iter().any(|tag| tag == "线索转述"));
        assert!(state.has_completed_dialogue_relay(DialogueRelayId::MirrorToChild));
        assert!(state.has_flag(Flag::UnderstoodChildPromise));

        let completed = relay_summaries(&state);
        let mirror = completed
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "已转述");
        assert_eq!(mirror.progress, 74);
        assert!(!mirror.reflected);
        assert!(!mirror.echoed);
        assert!(!mirror.anchored);

        let reflections = available_reflections(
            &state,
            ActiveDialogue {
                dialogue: DialogueId::Child,
                node: DialogueNodeId::Root,
            },
        );
        assert!(reflections
            .iter()
            .any(|action| action.relay == DialogueRelayId::MirrorToChild && action.enabled));
        let reflection = reflect(&mut state, DialogueRelayId::MirrorToChild);
        assert!(reflection.tags.iter().any(|tag| tag == "转述余波"));
        assert!(state.has_reflected_dialogue_relay(DialogueRelayId::MirrorToChild));

        let reflected = relay_summaries(&state);
        let mirror = reflected
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "已追问");
        assert_eq!(mirror.progress, 88);
        assert!(mirror.reflected);

        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Traveler,
            node: DialogueNodeId::Root,
        });
        let echoes = available_echoes(
            &state,
            ActiveDialogue {
                dialogue: DialogueId::Traveler,
                node: DialogueNodeId::Root,
            },
        );
        assert!(echoes
            .iter()
            .any(|action| action.relay == DialogueRelayId::MirrorToChild && action.enabled));
        let echo = echo(&mut state, DialogueRelayId::MirrorToChild);
        assert!(echo.tags.iter().any(|tag| tag == "转述回声"));
        assert!(state.has_echoed_dialogue_relay(DialogueRelayId::MirrorToChild));

        let echoed = relay_summaries(&state);
        let mirror = echoed
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "已回声");
        assert_eq!(mirror.progress, 96);
        assert!(mirror.echoed);

        state.active_dialogue = None;
        state.location = DialogueId::Traveler.location();
        let anchors = available_anchors(&state);
        assert!(anchors
            .iter()
            .any(|action| action.relay == DialogueRelayId::MirrorToChild && action.enabled));
        let anchor = anchor(&mut state, DialogueRelayId::MirrorToChild);
        assert!(anchor.tags.iter().any(|tag| tag == "回声落点"));
        assert!(state.has_anchored_dialogue_relay(DialogueRelayId::MirrorToChild));

        let anchored = relay_summaries(&state);
        let mirror = anchored
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "已落点");
        assert_eq!(mirror.progress, 97);
        assert!(mirror.anchored);
        assert!(!mirror.reviewed);

        let reviews = available_anchor_reviews(&state);
        assert!(reviews
            .iter()
            .any(|action| action.relay == DialogueRelayId::MirrorToChild && action.enabled));
        let review = review_anchor(&mut state, DialogueRelayId::MirrorToChild);
        assert!(review.tags.iter().any(|tag| tag == "回声复看"));
        assert!(state.has_reviewed_dialogue_anchor(DialogueRelayId::MirrorToChild));

        let reviewed = relay_summaries(&state);
        let mirror = reviewed
            .iter()
            .find(|summary| summary.relay == DialogueRelayId::MirrorToChild)
            .expect("mirror relay should be listed");
        assert_eq!(mirror.status, "已复看");
        assert_eq!(mirror.progress, 100);
        assert!(mirror.reviewed);
    }

    #[test]
    fn relay_count_matches_declared_table() {
        assert_eq!(DIALOGUE_RELAY_COUNT, DialogueRelayId::ALL.len());
    }
}
