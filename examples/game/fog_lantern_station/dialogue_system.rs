use crate::model::{
    ActiveDialogue, DialogueBeatKey, DialogueChoiceId, DialogueId, DialogueNodeId, DialogueTone,
    DialogueTranscriptEntry, Flag, GameState, Item, Location, StoryEvent, TopicId,
    NPC_THREAD_STEPS,
};

pub const DIALOGUE_BEAT_COUNT: usize = 48;

#[derive(Clone, Debug)]
pub struct DialogueEntry {
    pub dialogue: DialogueId,
    pub label: &'static str,
    pub detail: &'static str,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueChoiceAction {
    pub choice: DialogueChoiceId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueThreadSummary {
    pub dialogue: DialogueId,
    pub title: &'static str,
    pub status: String,
    pub detail: String,
    pub progress: u8,
}

pub fn available_dialogues(state: &GameState) -> Vec<DialogueEntry> {
    if state.active_dialogue.is_some() {
        return Vec::new();
    }
    DialogueId::ALL
        .iter()
        .copied()
        .filter(|dialogue| dialogue.location() == state.location)
        .map(|dialogue| DialogueEntry {
            dialogue,
            label: dialogue.label(),
            detail: dialogue.detail(),
            enabled: dialogue_enabled(state, dialogue),
        })
        .collect()
}

pub fn available_choices(state: &GameState) -> Vec<DialogueChoiceAction> {
    let Some(active) = state.active_dialogue else {
        return Vec::new();
    };
    let mut choices = choices_for(active)
        .into_iter()
        .map(|choice| {
            let missing = missing_requirements(state, active.dialogue, choice);
            DialogueChoiceAction {
                choice,
                label: choice_label(active, choice),
                detail: choice_detail(state, active, choice, &missing),
                enabled: missing.is_empty(),
            }
        })
        .collect::<Vec<_>>();
    choices.push(DialogueChoiceAction {
        choice: DialogueChoiceId::Leave,
        label: "结束对话".to_string(),
        detail: "结束当前谈话，回到探索和行动列表。".to_string(),
        enabled: true,
    });
    choices
}

pub fn dialogue_thread_summaries(state: &GameState) -> Vec<DialogueThreadSummary> {
    DialogueId::ALL
        .iter()
        .copied()
        .map(|dialogue| {
            let completed = completed_thread_beats(state, dialogue);
            let total = thread_total_beats(dialogue);
            let active = state
                .active_dialogue
                .filter(|active| active.dialogue == dialogue);
            let opened_nodes = opened_node_names(state, dialogue);
            let status = if completed >= total {
                "完整".to_string()
            } else if active.is_some() {
                "对话中".to_string()
            } else if dialogue.location() == state.location {
                "可进入".to_string()
            } else if completed > 0 {
                "已展开".to_string()
            } else {
                "未展开".to_string()
            };
            let detail = if let Some(active) = active {
                format!(
                    "正在谈“{}”。已完成 {completed}/{total} 个对话节点；记录 {} 段。",
                    node_name(dialogue, active.node),
                    state.dialogue_transcript.len()
                )
            } else if opened_nodes.is_empty() {
                format!(
                    "尚未展开话题。到{}进入对话，先问出第一层问题。",
                    dialogue.location().title()
                )
            } else {
                format!(
                    "已展开：{}。已完成 {completed}/{total} 个对话节点；记录 {} 段。",
                    opened_nodes.join("、"),
                    state.dialogue_transcript.len()
                )
            };
            DialogueThreadSummary {
                dialogue,
                title: dialogue.title(),
                status,
                detail,
                progress: percent(completed, total),
            }
        })
        .collect()
}

pub fn begin(state: &mut GameState, dialogue: DialogueId) -> StoryEvent {
    if let Some(active) = state.active_dialogue {
        return StoryEvent::new(
            "已经在对话中",
            format!(
                "你正和{}说话。先把这段话说完，再转向别处。",
                active.dialogue.title()
            ),
        )
        .tag("对话系统");
    }

    if dialogue.location() != state.location || !dialogue_enabled(state, dialogue) {
        return StoryEvent::new(
            "对话还没有入口",
            format!(
                "{}现在还不能在这里展开。先确认地点、关系或已经发生过的事。",
                dialogue.title()
            ),
        )
        .tag("对话系统");
    }

    let active = ActiveDialogue {
        dialogue,
        node: DialogueNodeId::Root,
    };
    state.active_dialogue = Some(active);
    let event = StoryEvent::new(
        format!("进入对话：{}", dialogue.title()),
        dialogue.opening(state.dialogue_tone),
    )
    .tag("对话系统")
    .tag("自由对话");
    state.record_dialogue_line(DialogueTranscriptEntry::new(
        dialogue,
        DialogueNodeId::Root,
        None,
        event.title.clone(),
        event.body.clone(),
    ));
    event
}

pub fn choose(state: &mut GameState, choice: DialogueChoiceId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "没有正在进行的对话",
            "你把问题递向空气。车站替你保存了这个尴尬，但没有人回答。",
        )
        .tag("对话系统");
    };

    if choice == DialogueChoiceId::Leave {
        state.active_dialogue = None;
        let event = StoryEvent::new(
            format!("结束对话：{}", active.dialogue.title()),
            "你没有把所有话一次说完。雾灯站不喜欢完整答案，但它记得被暂时放下的问题。",
        )
        .tag("对话系统");
        state.record_dialogue_line(DialogueTranscriptEntry::new(
            active.dialogue,
            active.node,
            Some(choice),
            event.title.clone(),
            event.body.clone(),
        ));
        return event;
    }

    if !choice_valid_for(active, choice) {
        return StoryEvent::new(
            "这句话不属于当前对话",
            format!(
                "你正和{}谈{}，这个问题会把谈话撕到另一个方向。",
                active.dialogue.title(),
                node_title(active)
            ),
        )
        .tag("对话系统");
    }

    if choice == DialogueChoiceId::BackToRoot {
        state.active_dialogue = Some(ActiveDialogue {
            dialogue: active.dialogue,
            node: DialogueNodeId::Root,
        });
        let event = StoryEvent::new(
            format!("回到话题清单：{}", active.dialogue.title()),
            "你把刚才的话题轻轻合上。对方没有离开，只是把下一个问题的位置让出来。",
        )
        .tag("对话系统")
        .tag("话题节点");
        state.record_dialogue_line(DialogueTranscriptEntry::new(
            active.dialogue,
            active.node,
            Some(choice),
            event.title.clone(),
            event.body.clone(),
        ));
        return event;
    }

    let missing = missing_requirements(state, active.dialogue, choice);
    if !missing.is_empty() {
        return StoryEvent::new(
            "这句话还问不出口",
            format!(
                "你知道问题在那里，但还缺能支撑它的事实：{}。",
                missing.join("；")
            ),
        )
        .tag("对话系统");
    }

    let beat = DialogueBeatKey {
        dialogue: active.dialogue,
        node: active.node,
        choice,
    };
    let repeated = state.has_completed_dialogue_beat(beat);
    let mut event = if repeated {
        repeat_event(active, choice)
    } else if active.node == DialogueNodeId::Root {
        root_choice_event(active.dialogue, choice, state.dialogue_tone)
    } else {
        node_choice_event(active, choice, state.dialogue_tone)
    };
    if repeated {
        event.tags.push("复谈".to_string());
    } else {
        apply_choice_rewards(state, active, choice, &mut event);
        state.complete_dialogue_beat(beat);
    }
    let next_node = next_node(active, choice);
    state.active_dialogue = Some(ActiveDialogue {
        dialogue: active.dialogue,
        node: next_node,
    });
    state.record_dialogue_line(DialogueTranscriptEntry::new(
        active.dialogue,
        active.node,
        Some(choice),
        event.title.clone(),
        event.body.clone(),
    ));
    event
}

impl DialogueId {
    pub const ALL: [Self; 4] = [Self::Traveler, Self::Clerk, Self::Child, Self::Keeper];

    pub fn location(self) -> Location {
        match self {
            Self::Traveler => Location::WaitingHall,
            Self::Clerk => Location::TicketOffice,
            Self::Child => Location::Platform,
            Self::Keeper => Location::ClockTower,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Traveler => "进入对话：候车厅老人",
            Self::Clerk => "进入对话：售票窗口",
            Self::Child => "进入对话：白线后的孩子",
            Self::Keeper => "进入对话：旧钟楼站务员",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::Traveler => "进入一个可选择问题的对话，而不是只触发一段事件。",
            Self::Clerk => "隔着玻璃追问票、价格和返程规则。",
            Self::Child => "和孩子保持同一处场景，选择你要怎样问他。",
            Self::Keeper => "让守夜、广播和外套成为可追问的节点。",
        }
    }

    fn opening(self, tone: DialogueTone) -> &'static str {
        match (self, tone) {
            (Self::Traveler, DialogueTone::Listening) => {
                "老人没有抬头。你决定先听他说完，再问报纸、雨和六年前那晚。"
            }
            (Self::Traveler, DialogueTone::Gentle) => {
                "你坐到老人对面，放轻声音。他愿意谈报纸上的雨，也可能说出你以前来过的事。"
            }
            (Self::Traveler, DialogueTone::Direct) => {
                "你直接告诉老人：我需要知道发生过什么。老人压低报纸，准备回答。"
            }
            (Self::Clerk, DialogueTone::Listening) => {
                "售票员没有欢迎你。你先观察窗口、票章和抽屉，再问退票规则。"
            }
            (Self::Clerk, DialogueTone::Gentle) => {
                "你没有敲玻璃，只把车票放到窗口。售票员愿意解释一部分规则。"
            }
            (Self::Clerk, DialogueTone::Direct) => {
                "你直接站到窗口正中，要求售票员讲清单程票和返程票。"
            }
            (Self::Child, DialogueTone::Listening) => {
                "孩子看着白线，没有看你。你先不催他，只等他愿意开口。"
            }
            (Self::Child, DialogueTone::Gentle) => {
                "你蹲到他能平视的位置。他把作业本抱紧，但没有后退。"
            }
            (Self::Child, DialogueTone::Direct) => {
                "你告诉他：这次我不会替你回答。孩子抬起眼睛，等你证明。"
            }
            (Self::Keeper, DialogueTone::Listening) => {
                "站务员坐在旧钟旁，没有赶你。你先听他解释 23:59 为什么停住。"
            }
            (Self::Keeper, DialogueTone::Gentle) => {
                "你没有碰他的外套，只问旧钟和雾灯。站务员愿意慢慢回答。"
            }
            (Self::Keeper, DialogueTone::Direct) => {
                "你开门见山：守夜到底是职责，还是还债？站务员停顿了一会儿。"
            }
        }
    }
}

impl DialogueChoiceId {
    fn dialogue(self) -> Option<DialogueId> {
        match self {
            Self::TravelerRain | Self::TravelerEmptySeat | Self::TravelerLoop => {
                Some(DialogueId::Traveler)
            }
            Self::ClerkTicket | Self::ClerkReturnRule | Self::ClerkTomorrowPrice => {
                Some(DialogueId::Clerk)
            }
            Self::ChildWhiteLine | Self::ChildAnger | Self::ChildTomorrowBag => {
                Some(DialogueId::Child)
            }
            Self::KeeperDuty | Self::KeeperBroadcast | Self::KeeperCoat => Some(DialogueId::Keeper),
            Self::DeepenTopic
            | Self::ChallengeTopic
            | Self::PromiseTopic
            | Self::BackToRoot
            | Self::Leave => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::TravelerRain => "问：你为什么一直读同一页报纸？",
            Self::TravelerEmptySeat => "问：第七张长椅到底少了谁？",
            Self::TravelerLoop => "问：我是不是来过这里很多次？",
            Self::ClerkTicket => "问：这张湿票到底办什么业务？",
            Self::ClerkReturnRule => "问：返程票为什么要两个人？",
            Self::ClerkTomorrowPrice => "问：明天的价格是什么？",
            Self::ChildWhiteLine => "问：你为什么一直站在白线后？",
            Self::ChildAnger => "问：你可以继续生我的气吗？",
            Self::ChildTomorrowBag => "问：如果上车，明天要带什么？",
            Self::KeeperDuty => "问：守夜到底守的是什么？",
            Self::KeeperBroadcast => "问：广播为什么会用我的声音？",
            Self::KeeperCoat => "问：那件外套为什么没有影子？",
            Self::DeepenTopic => "继续听下去",
            Self::ChallengeTopic => "追问矛盾",
            Self::PromiseTopic => "接到行动",
            Self::BackToRoot => "回到话题清单",
            Self::Leave => "结束对话",
        }
    }

    fn detail(self) -> &'static str {
        match self {
            Self::TravelerRain => "了解老人为什么一直看同一页报纸。",
            Self::TravelerEmptySeat => "确认候车厅缺失的 07 号座位和谁有关。",
            Self::TravelerLoop => "确认你是否反复回到雾灯站。",
            Self::ClerkTicket => "让售票员解释湿票、退票和改签规则。",
            Self::ClerkReturnRule => "了解返程票为什么需要两个座位。",
            Self::ClerkTomorrowPrice => "了解离开车站之后你还要承担什么。",
            Self::ChildWhiteLine => "了解孩子为什么不越过白线。",
            Self::ChildAnger => "承认孩子可以继续生气，不必马上原谅你。",
            Self::ChildTomorrowBag => "把“明天”问成具体计划。",
            Self::KeeperDuty => "了解站务员为什么守着旧钟。",
            Self::KeeperBroadcast => "了解广播室、你的声音和完整姓名的关系。",
            Self::KeeperCoat => "了解站务员外套和留下来的代价。",
            Self::DeepenTopic => "继续听事实细节。",
            Self::ChallengeTopic => "追问对方没有说清的地方。",
            Self::PromiseTopic => "把谈话变成下一步行动。",
            Self::BackToRoot => "回到当前人物的主话题。",
            Self::Leave => "暂时离开当前谈话。",
        }
    }
}

fn dialogue_enabled(_state: &GameState, dialogue: DialogueId) -> bool {
    match dialogue {
        DialogueId::Traveler | DialogueId::Clerk | DialogueId::Keeper => true,
        DialogueId::Child => true,
    }
}

fn choices_for(active: ActiveDialogue) -> Vec<DialogueChoiceId> {
    if active.node != DialogueNodeId::Root {
        return vec![
            DialogueChoiceId::DeepenTopic,
            DialogueChoiceId::ChallengeTopic,
            DialogueChoiceId::PromiseTopic,
            DialogueChoiceId::BackToRoot,
        ];
    }

    match active.dialogue {
        DialogueId::Traveler => vec![
            DialogueChoiceId::TravelerRain,
            DialogueChoiceId::TravelerEmptySeat,
            DialogueChoiceId::TravelerLoop,
        ],
        DialogueId::Clerk => vec![
            DialogueChoiceId::ClerkTicket,
            DialogueChoiceId::ClerkReturnRule,
            DialogueChoiceId::ClerkTomorrowPrice,
        ],
        DialogueId::Child => vec![
            DialogueChoiceId::ChildWhiteLine,
            DialogueChoiceId::ChildAnger,
            DialogueChoiceId::ChildTomorrowBag,
        ],
        DialogueId::Keeper => vec![
            DialogueChoiceId::KeeperDuty,
            DialogueChoiceId::KeeperBroadcast,
            DialogueChoiceId::KeeperCoat,
        ],
    }
}

fn choice_valid_for(active: ActiveDialogue, choice: DialogueChoiceId) -> bool {
    match choice {
        DialogueChoiceId::Leave => true,
        DialogueChoiceId::BackToRoot => active.node != DialogueNodeId::Root,
        DialogueChoiceId::DeepenTopic
        | DialogueChoiceId::ChallengeTopic
        | DialogueChoiceId::PromiseTopic => active.node != DialogueNodeId::Root,
        _ => active.node == DialogueNodeId::Root && choice.dialogue() == Some(active.dialogue),
    }
}

fn choice_label(active: ActiveDialogue, choice: DialogueChoiceId) -> String {
    match choice {
        DialogueChoiceId::DeepenTopic => format!("继续听：{}", node_title(active)),
        DialogueChoiceId::ChallengeTopic => format!("追问矛盾：{}", node_title(active)),
        DialogueChoiceId::PromiseTopic => format!("接到行动：{}", node_title(active)),
        DialogueChoiceId::BackToRoot => "回到话题清单".to_string(),
        _ => choice.label().to_string(),
    }
}

fn choice_detail(
    state: &GameState,
    active: ActiveDialogue,
    choice: DialogueChoiceId,
    missing: &[&'static str],
) -> String {
    if !missing.is_empty() {
        return format!("还缺：{}。", missing.join("；"));
    }
    if choice == DialogueChoiceId::BackToRoot {
        return "不结束对话，只回到这个人的主话题。".to_string();
    }
    if choice == DialogueChoiceId::Leave {
        return choice.detail().to_string();
    }
    let beat = DialogueBeatKey {
        dialogue: active.dialogue,
        node: active.node,
        choice,
    };
    if state.has_completed_dialogue_beat(beat) {
        return "已问过：这次会得到复谈回应，不会重复增加关系数值。".to_string();
    }
    if active.node == DialogueNodeId::Root {
        return choice.detail().to_string();
    }
    match choice {
        DialogueChoiceId::DeepenTopic => "先不抢答案，让对方把这一层说完整。".to_string(),
        DialogueChoiceId::ChallengeTopic => "指出话里的回避处，但让对方还有余地回答。".to_string(),
        DialogueChoiceId::PromiseTopic => "把话题接到具体行动，而不是漂亮态度。".to_string(),
        _ => choice.detail().to_string(),
    }
}

fn completed_thread_beats(state: &GameState, dialogue: DialogueId) -> usize {
    state
        .completed_dialogue_beats
        .iter()
        .filter(|beat| beat.dialogue == dialogue)
        .count()
}

fn thread_total_beats(dialogue: DialogueId) -> usize {
    choices_for(ActiveDialogue {
        dialogue,
        node: DialogueNodeId::Root,
    })
    .len()
        + 3 * [
            DialogueChoiceId::DeepenTopic,
            DialogueChoiceId::ChallengeTopic,
            DialogueChoiceId::PromiseTopic,
        ]
        .len()
}

fn opened_node_names(state: &GameState, dialogue: DialogueId) -> Vec<&'static str> {
    [
        DialogueNodeId::Memory,
        DialogueNodeId::Proof,
        DialogueNodeId::Tomorrow,
    ]
    .into_iter()
    .filter(|node| {
        state
            .completed_dialogue_beats
            .iter()
            .any(|beat| beat.dialogue == dialogue && opened_node_for(beat) == Some(*node))
    })
    .map(|node| node_name(dialogue, node))
    .collect()
}

fn opened_node_for(beat: &DialogueBeatKey) -> Option<DialogueNodeId> {
    match (beat.node, beat.choice) {
        (DialogueNodeId::Root, choice) => Some(next_node(
            ActiveDialogue {
                dialogue: beat.dialogue,
                node: beat.node,
            },
            choice,
        )),
        (node @ (DialogueNodeId::Memory | DialogueNodeId::Proof | DialogueNodeId::Tomorrow), _) => {
            Some(node)
        }
    }
    .filter(|node| *node != DialogueNodeId::Root)
}

fn percent(value: usize, total: usize) -> u8 {
    if total == 0 {
        0
    } else {
        ((value.min(total) * 100) / total) as u8
    }
}

fn missing_requirements(
    state: &GameState,
    dialogue: DialogueId,
    choice: DialogueChoiceId,
) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        dialogue_enabled(state, dialogue),
        "先让这个人物进入场景",
    );
    match choice {
        DialogueChoiceId::TravelerEmptySeat => require(
            &mut missing,
            state.has_flag(Flag::ReadDepartureBoard) || state.current_segment() >= 2,
            "读过电子时刻表，或等到第二段午夜",
        ),
        DialogueChoiceId::TravelerLoop => require(
            &mut missing,
            state.has_flag(Flag::UnderstoodFirstLoop) || state.current_segment() >= 3,
            "理解过一次循环，或等到第三段午夜",
        ),
        DialogueChoiceId::ClerkReturnRule => require(
            &mut missing,
            state.has_flag(Flag::ExaminedTicket),
            "先检查湿票背面",
        ),
        DialogueChoiceId::ClerkTomorrowPrice => require(
            &mut missing,
            state.current_segment() >= 4 || state.ticket == crate::model::TicketKind::Return,
            "等到第四段午夜，或先拿到返程票",
        ),
        DialogueChoiceId::ChildAnger => require(
            &mut missing,
            state.child_depth >= 2 || state.child_trust >= 2,
            "先让孩子愿意继续听你说话",
        ),
        DialogueChoiceId::ChildTomorrowBag => require(
            &mut missing,
            state.has_item(crate::model::Item::ChildHomework) || state.child_trust >= 3,
            "拿到作业本，或建立足够信任",
        ),
        DialogueChoiceId::KeeperBroadcast => require(
            &mut missing,
            state.has_item(crate::model::Item::BroadcastTape)
                || state.has_flag(Flag::HeardBroadcastTape),
            "取得或听过广播磁带",
        ),
        DialogueChoiceId::KeeperCoat => require(
            &mut missing,
            state.current_segment() >= 5 || state.has_flag(Flag::HeardClockTruth),
            "等到第五段午夜，或先听懂旧钟真相",
        ),
        DialogueChoiceId::TravelerRain
        | DialogueChoiceId::ClerkTicket
        | DialogueChoiceId::ChildWhiteLine
        | DialogueChoiceId::KeeperDuty
        | DialogueChoiceId::DeepenTopic
        | DialogueChoiceId::ChallengeTopic
        | DialogueChoiceId::PromiseTopic
        | DialogueChoiceId::BackToRoot
        | DialogueChoiceId::Leave => {}
    }
    missing
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn next_node(active: ActiveDialogue, choice: DialogueChoiceId) -> DialogueNodeId {
    match choice {
        DialogueChoiceId::TravelerRain
        | DialogueChoiceId::ClerkTicket
        | DialogueChoiceId::ChildWhiteLine
        | DialogueChoiceId::KeeperDuty => DialogueNodeId::Memory,
        DialogueChoiceId::TravelerEmptySeat
        | DialogueChoiceId::ClerkReturnRule
        | DialogueChoiceId::ChildAnger
        | DialogueChoiceId::KeeperBroadcast => DialogueNodeId::Proof,
        DialogueChoiceId::TravelerLoop
        | DialogueChoiceId::ClerkTomorrowPrice
        | DialogueChoiceId::ChildTomorrowBag
        | DialogueChoiceId::KeeperCoat => DialogueNodeId::Tomorrow,
        DialogueChoiceId::BackToRoot => DialogueNodeId::Root,
        DialogueChoiceId::DeepenTopic
        | DialogueChoiceId::ChallengeTopic
        | DialogueChoiceId::PromiseTopic
        | DialogueChoiceId::Leave => active.node,
    }
}

pub fn node_name(dialogue: DialogueId, node: DialogueNodeId) -> &'static str {
    match (dialogue, node) {
        (_, DialogueNodeId::Root) => "主话题",
        (DialogueId::Traveler, DialogueNodeId::Memory) => "报纸与雨",
        (DialogueId::Traveler, DialogueNodeId::Proof) => "第七张长椅",
        (DialogueId::Traveler, DialogueNodeId::Tomorrow) => "循环里的逃离",
        (DialogueId::Clerk, DialogueNodeId::Memory) => "湿票核验",
        (DialogueId::Clerk, DialogueNodeId::Proof) => "返程双座",
        (DialogueId::Clerk, DialogueNodeId::Tomorrow) => "明天的价格",
        (DialogueId::Child, DialogueNodeId::Memory) => "白线以后",
        (DialogueId::Child, DialogueNodeId::Proof) => "可以继续的气",
        (DialogueId::Child, DialogueNodeId::Tomorrow) => "明天的行李",
        (DialogueId::Keeper, DialogueNodeId::Memory) => "守夜的债",
        (DialogueId::Keeper, DialogueNodeId::Proof) => "广播里的声音",
        (DialogueId::Keeper, DialogueNodeId::Tomorrow) => "无影的外套",
    }
}

fn node_title(active: ActiveDialogue) -> &'static str {
    node_name(active.dialogue, active.node)
}

fn root_choice_event(
    dialogue: DialogueId,
    choice: DialogueChoiceId,
    tone: DialogueTone,
) -> StoryEvent {
    let body = match (choice, tone) {
        (DialogueChoiceId::TravelerRain, DialogueTone::Listening) => {
            "你慢慢问报纸上有什么。老人说：六年前那晚下过雨。你每次醒来都带着同样的雨水味，所以我知道你又回来了。"
        }
        (DialogueChoiceId::TravelerRain, DialogueTone::Gentle) => {
            "你轻声问报纸日期。老人说日期会变，但水痕不会。你看见报纸上有一块孩子手掌大小的水印。"
        }
        (DialogueChoiceId::TravelerRain, DialogueTone::Direct) => {
            "你直接说：这不是新闻，是我的旧案。老人点头，把报纸翻到背面。纸上出现了你的车票编号。"
        }
        (DialogueChoiceId::TravelerEmptySeat, DialogueTone::Listening) => {
            "你问第七张长椅。老人让你先听那个空位。你听见很轻的衣料声，说明曾有人坐在那里等你。"
        }
        (DialogueChoiceId::TravelerEmptySeat, DialogueTone::Gentle) => {
            "你问空座是不是孩子的位置。老人说：它也代表你没说完的话。你可以坐近一点，但这不等于他已经从那条命令里出来。"
        }
        (DialogueChoiceId::TravelerEmptySeat, DialogueTone::Direct) => {
            "你直接问：我是不是把他留在这里？老人说：你当年用最像保护的口气下了命令，所以他只能留在这里等答案。"
        }
        (DialogueChoiceId::TravelerLoop, DialogueTone::Listening) => {
            "你问自己是不是来过很多次。老人说：是。你每次先问能不能上车，很少问谁还没上车。"
        }
        (DialogueChoiceId::TravelerLoop, DialogueTone::Gentle) => {
            "你问循环会不会让人累。老人说，最累的是被你反复要求作证的人。报纸边角写着很多次“再等等”。"
        }
        (DialogueChoiceId::TravelerLoop, DialogueTone::Direct) => {
            "你说：我不是第一次逃。老人回答：也不是第一次把逃跑说成选择。"
        }
        (DialogueChoiceId::ClerkTicket, DialogueTone::Listening) => {
            "你问湿票要办什么业务。售票员说：不是购票，是核验。车站要确认你是否还想一个人离开。"
        }
        (DialogueChoiceId::ClerkTicket, DialogueTone::Gentle) => {
            "你把湿票推近一点。售票员说：要改签，需要姓名、座位、同行人，以及车票背面完整的警告。"
        }
        (DialogueChoiceId::ClerkTicket, DialogueTone::Direct) => {
            "你让她别说业务话。售票员放下票章：直说吧，你是想撤销单程离开，还是补上被你忘掉的人？"
        }
        (DialogueChoiceId::ClerkReturnRule, DialogueTone::Listening) => {
            "你问返程票为什么要两个人。售票员说：因为当年你把恐惧变成命令，车站不允许你再把孩子当成能被安排的行李。"
        }
        (DialogueChoiceId::ClerkReturnRule, DialogueTone::Gentle) => {
            "你问两张票是不是补偿。售票员说不是。返程票只证明你承认：明天不是只给你一个人的。"
        }
        (DialogueChoiceId::ClerkReturnRule, DialogueTone::Direct) => {
            "你问规则是不是惩罚。售票员说不是。规则只是要求你别把另一个人从目的地栏里删掉。"
        }
        (DialogueChoiceId::ClerkTomorrowPrice, DialogueTone::Listening) => {
            "你问明天的价格。售票员说：离开后也要继续负责，不能用一次正确选择抵消以后所有逃避。"
        }
        (DialogueChoiceId::ClerkTomorrowPrice, DialogueTone::Gentle) => {
            "你问价格时没有急着递票。售票员说：明天的价格是持续承担，不是今晚表现好一次就结束。"
        }
        (DialogueChoiceId::ClerkTomorrowPrice, DialogueTone::Direct) => {
            "你问到底要付什么。售票员回答：付掉“我已经够痛，所以可以少负责”的想法。"
        }
        (DialogueChoiceId::ChildWhiteLine, DialogueTone::Listening) => {
            "你问白线。孩子说：站在线后，别人会以为我是听广播，其实我是听你。"
        }
        (DialogueChoiceId::ChildWhiteLine, DialogueTone::Gentle) => {
            "你问他为什么站在那里。他说：如果我先越过白线，就像我先承认自己当年不该听话。"
        }
        (DialogueChoiceId::ChildWhiteLine, DialogueTone::Direct) => {
            "你问白线是不是困住他。孩子说：有时我需要一条线，免得自己不知道听谁的话。"
        }
        (DialogueChoiceId::ChildAnger, DialogueTone::Listening) => {
            "你问他能不能继续生气，然后没有补充解释。孩子说：如果可以，我就不用为了你继续做听话的人。"
        }
        (DialogueChoiceId::ChildAnger, DialogueTone::Gentle) => {
            "你说他可以继续害怕。孩子问你会不会难过。你说会，但那不是他要替你解决的问题。"
        }
        (DialogueChoiceId::ChildAnger, DialogueTone::Direct) => {
            "你说：我不能再用你的原谅来救自己。孩子看着你，说这句话至少比普通道歉更接近事实。"
        }
        (DialogueChoiceId::ChildTomorrowBag, DialogueTone::Listening) => {
            "你问明天要带什么。孩子说：作业本、伞，还有我今天没说完的气话。你说都可以带。"
        }
        (DialogueChoiceId::ChildTomorrowBag, DialogueTone::Gentle) => {
            "你问他想带热牛奶还是雨衣。他想了想，说还想带一点“不知道”，因为明天他也不确定。"
        }
        (DialogueChoiceId::ChildTomorrowBag, DialogueTone::Direct) => {
            "你说别把明天说得太漂亮。孩子说：那就带账本。你欠我的，明天继续记。你说好。"
        }
        (DialogueChoiceId::KeeperDuty, DialogueTone::Listening) => {
            "你问守夜守什么。站务员说：守住别人还没准备好承认错误的最后一分钟。"
        }
        (DialogueChoiceId::KeeperDuty, DialogueTone::Gentle) => {
            "你问他累不累。他说累，但更怕没人提醒旅客：最后一分钟不能无限延长。"
        }
        (DialogueChoiceId::KeeperDuty, DialogueTone::Direct) => {
            "你问守夜是不是逃避。站务员没有否认：有时是。关键是别把逃避说成神圣。"
        }
        (DialogueChoiceId::KeeperBroadcast, DialogueTone::Listening) => {
            "你问广播为什么用你的声音。站务员说：因为你最熟悉那句警告，也最常把警告听到一半。"
        }
        (DialogueChoiceId::KeeperBroadcast, DialogueTone::Gentle) => {
            "你说听见自己的声音很害怕。站务员说：害怕是对的。广播的作用是让后来者多一秒犹豫。"
        }
        (DialogueChoiceId::KeeperBroadcast, DialogueTone::Direct) => {
            "你说这是不是拿我当工具。站务员回答：是，如果你不参与写稿；不是，如果你终于愿意把警告写完整。"
        }
        (DialogueChoiceId::KeeperCoat, DialogueTone::Listening) => {
            "你问外套为什么没有影子。站务员说：穿上它的人会慢慢把自己交给车站，久了就忘记自己也能离开。"
        }
        (DialogueChoiceId::KeeperCoat, DialogueTone::Gentle) => {
            "你说外套看起来很冷。站务员说：留下不是问题，问题是把留下说成唯一正确的事。"
        }
        (DialogueChoiceId::KeeperCoat, DialogueTone::Direct) => {
            "你说如果我穿上它，也会没有影子吗？站务员看向你：如果你把留下当成抵账，就会。如果你把留下当成工作，也许还能下班。"
        }
        (DialogueChoiceId::DeepenTopic, _)
        | (DialogueChoiceId::ChallengeTopic, _)
        | (DialogueChoiceId::PromiseTopic, _)
        | (DialogueChoiceId::BackToRoot, _) => "",
        (DialogueChoiceId::Leave, _) => "",
    };
    StoryEvent::new(format!("对话：{}", dialogue.title()), body)
        .tag("对话系统")
        .tag("对话选择")
        .tag(format!("语气：{}", tone.name()))
}

struct NodeScript {
    deepen: &'static str,
    challenge: &'static str,
    promise: &'static str,
}

fn node_choice_event(
    active: ActiveDialogue,
    choice: DialogueChoiceId,
    tone: DialogueTone,
) -> StoryEvent {
    let script = node_script(active);
    let approach = match tone {
        DialogueTone::Listening => "你先听完，再继续问。 ",
        DialogueTone::Gentle => "你放轻声音，继续问。 ",
        DialogueTone::Direct => "你直接追问重点。 ",
    };
    let body = match choice {
        DialogueChoiceId::DeepenTopic => format!("{approach}{}", script.deepen),
        DialogueChoiceId::ChallengeTopic => format!("{approach}{}", script.challenge),
        DialogueChoiceId::PromiseTopic => format!("{approach}{}", script.promise),
        _ => String::new(),
    };

    StoryEvent::new(
        format!("对话：{} / {}", active.dialogue.title(), node_title(active)),
        body,
    )
    .tag("对话系统")
    .tag("话题节点")
    .tag(format!("语气：{}", tone.name()))
}

fn repeat_event(active: ActiveDialogue, choice: DialogueChoiceId) -> StoryEvent {
    let body = match choice {
        DialogueChoiceId::DeepenTopic
        | DialogueChoiceId::ChallengeTopic
        | DialogueChoiceId::PromiseTopic => format!(
            "你又回到“{}”。{}没有重复旧答案，只提醒你：这个问题已经问过，下一步应该靠行动推进。",
            node_title(active),
            active.dialogue.title()
        ),
        _ => format!(
            "你又问了一遍。{}没有重复原话。这个问题已经记录在日志里。",
            active.dialogue.title()
        ),
    };
    StoryEvent::new(
        format!("复谈：{} / {}", active.dialogue.title(), node_title(active)),
        body,
    )
    .tag("对话系统")
    .tag("复谈")
}

fn node_script(active: ActiveDialogue) -> NodeScript {
    match (active.dialogue, active.node) {
        (DialogueId::Traveler, DialogueNodeId::Memory) => NodeScript {
            deepen: "老人说报纸是给每次醒来的你看的。水痕记录着六年前那晚的雨，也记录着那个孩子曾经和你在一起。",
            challenge: "你指出老人也在逃避。老人承认了：他坐在这里不是无辜，只是还没有离场。",
            promise: "你决定把报纸水痕当成证据，之后可以把它和车票、孩子线索放在一起判断。",
        },
        (DialogueId::Traveler, DialogueNodeId::Proof) => NodeScript {
            deepen: "老人说缺失的座位和孩子有关。它不是装饰线索，而是在提醒你：当年少了一个同行者。",
            challenge: "你说空座是在控诉你。老人同意，但也提醒你：控诉不能替代行动。",
            promise: "你把第七张长椅记成具体证据。长椅下滚出半枚旧票钉，可以作为后续线索。",
        },
        (DialogueId::Traveler, DialogueNodeId::Tomorrow) => NodeScript {
            deepen: "老人说循环看起来是机会，但也会让人一直拖延。下一次不一定更好，可能只是更熟练地逃避。",
            challenge: "你问这是不是车站的错。老人说车站有责任，但你不能把自己的责任全部推给车站。",
            promise: "你决定不再说“下次再处理”。这次先确认还有谁没上车。",
        },
        (DialogueId::Clerk, DialogueNodeId::Memory) => NodeScript {
            deepen: "售票员说湿票被退回过很多次。每次你只看见“别上车”，没看见后半句，它就会再次变湿。",
            challenge: "你指出窗口把痛苦变成流程。售票员承认流程会伤人，但没有流程时，你更容易漏掉别人。",
            promise: "你答应下一次交材料时，不只交自己的名字，也交同行者信息。",
        },
        (DialogueId::Clerk, DialogueNodeId::Proof) => NodeScript {
            deepen: "售票员摊开两张返程票：一张写目的地，一张写同行人。缺任何一张，都不能返程。",
            challenge: "你问为什么规则不早说清。她说：以前你只听能让自己离开的部分。",
            promise: "你承认双座是返程条件，不是奖励。之后要继续找 07A 和 07B 的证据。",
        },
        (DialogueId::Clerk, DialogueNodeId::Tomorrow) => NodeScript {
            deepen: "售票员说，明天的代价不是一次勇敢，而是以后持续负责。",
            challenge: "你问这是不是永远还不完。她说还不完也要开始还，至少不能假装没有欠账。",
            promise: "你把明天写成具体事：解释迟到、争吵后不消失、每天继续承担。",
        },
        (DialogueId::Child, DialogueNodeId::Memory) => NodeScript {
            deepen: "孩子说白线后面安全，因为只要他不过线，你就不能强迫他马上相信你。",
            challenge: "你说这条线是他能控制的东西。孩子提醒你：不要替他跨过去。",
            promise: "你答应先站在他允许的位置，再谈离开。",
        },
        (DialogueId::Child, DialogueNodeId::Proof) => NodeScript {
            deepen: "孩子说他还在生气，也不知道什么时候会好。你告诉他不需要现在变好。",
            challenge: "你承认自己怕他继续生气，因为那证明道歉不够。孩子回答：本来就不够。",
            promise: "你答应把他的愤怒也带进明天，不要求他在发车前原谅你。",
        },
        (DialogueId::Child, DialogueNodeId::Tomorrow) => NodeScript {
            deepen: "孩子列出明天要带的东西：作业本、伞、不确定，以及他可以反悔的权利。",
            challenge: "你说明天不会因为一张票自动变好。孩子要求你别把车票当奖状。",
            promise: "你答应把明天当成第一站，不当成一切问题的结局。",
        },
        (DialogueId::Keeper, DialogueNodeId::Memory) => NodeScript {
            deepen: "站务员说守夜守的是最后一分钟：人在那一分钟最容易把逃避说成没办法。",
            challenge: "你问他是不是把留下说得太高贵。他承认：岗位不是神坛。",
            promise: "你说如果接近雾灯，会记得它只是工具，不是赦免。",
        },
        (DialogueId::Keeper, DialogueNodeId::Proof) => NodeScript {
            deepen: "站务员说广播用你的声音，是因为完整警告必须由当年没说完的人补上。",
            challenge: "你说这很残忍。他承认，但认为只播半句会害后来者继续误解危险。",
            promise: "你答应如果进入广播室，会把姓名和后果都说完整。",
        },
        (DialogueId::Keeper, DialogueNodeId::Tomorrow) => NodeScript {
            deepen: "站务员说外套不是诅咒，而是一份会慢慢磨掉自我的工作。",
            challenge: "你问他为什么还不脱下外套。他说需要有人先证明：离开岗位不等于背叛。",
            promise: "你说如果选择留下，也会给自己设定边界和下班时间。",
        },
        (_, DialogueNodeId::Root) => NodeScript {
            deepen: "",
            challenge: "",
            promise: "",
        },
    }
}

fn apply_choice_rewards(
    state: &mut GameState,
    active: ActiveDialogue,
    choice: DialogueChoiceId,
    event: &mut StoryEvent,
) {
    if advances_relationship(choice) {
        bump_dialogue_depth(state, active.dialogue, 1);
        event
            .tags
            .push(format!("关系推进：{}", active.dialogue.title()));
    }

    match choice {
        DialogueChoiceId::TravelerRain => {
            state.discuss(TopicId::TravelerRain);
            grant_item_tag(state, event, Item::BrassKey, "获得：黄铜小钥匙");
        }
        DialogueChoiceId::TravelerEmptySeat => {
            state.discuss(TopicId::TravelerLeaving);
            remember_tag(state, event, Flag::TravelerTrusted, "对话：老人见证空座");
            state.child_trust = (state.child_trust + 1).min(5);
            grant_item_tag(state, event, Item::CoinToken, "获得：退票铜筹");
        }
        DialogueChoiceId::TravelerLoop => {
            state.discuss(TopicId::TravelerMercy);
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "对话：循环承认");
        }
        DialogueChoiceId::ClerkTicket => {
            state.discuss(TopicId::ClerkDestination);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        DialogueChoiceId::ClerkReturnRule => {
            state.discuss(TopicId::ClerkSeats);
            remember_tag(state, event, Flag::UnderstoodChildPromise, "对话：返程双座");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        DialogueChoiceId::ClerkTomorrowPrice => {
            state.discuss(TopicId::ClerkPrice);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        DialogueChoiceId::ChildWhiteLine => {
            state.discuss(TopicId::ChildWhiteLine);
            remember_tag(state, event, Flag::MetChild, "对话：白线相认");
            grant_item_tag(state, event, Item::ChildHomework, "获得：没有封面的作业本");
        }
        DialogueChoiceId::ChildAnger => {
            state.discuss(TopicId::ChildAnger);
            state.child_trust = (state.child_trust + 2).min(5);
        }
        DialogueChoiceId::ChildTomorrowBag => {
            state.discuss(TopicId::ChildTomorrow);
            remember_tag(state, event, Flag::UnderstoodChildPromise, "对话：两个名字");
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "对话：明天可带走",
            );
        }
        DialogueChoiceId::KeeperDuty => {
            state.discuss(TopicId::KeeperStay);
            remember_tag(state, event, Flag::HeardClockTruth, "对话：旧钟真相");
        }
        DialogueChoiceId::KeeperBroadcast => {
            state.discuss(TopicId::KeeperBroadcast);
            remember_tag(state, event, Flag::HeardBroadcastTape, "对话：广播回声");
        }
        DialogueChoiceId::KeeperCoat => {
            state.discuss(TopicId::KeeperCoat);
            remember_tag(state, event, Flag::HeardClockTruth, "对话：外套无影");
        }
        DialogueChoiceId::DeepenTopic
        | DialogueChoiceId::ChallengeTopic
        | DialogueChoiceId::PromiseTopic => {
            apply_node_reward(state, active, choice, event);
        }
        DialogueChoiceId::BackToRoot | DialogueChoiceId::Leave => {}
    }
}

fn advances_relationship(choice: DialogueChoiceId) -> bool {
    !matches!(
        choice,
        DialogueChoiceId::BackToRoot | DialogueChoiceId::Leave
    )
}

fn bump_dialogue_depth(state: &mut GameState, dialogue: DialogueId, amount: u8) {
    match dialogue {
        DialogueId::Traveler => {
            state.traveler_depth = state
                .traveler_depth
                .saturating_add(amount)
                .min(NPC_THREAD_STEPS)
        }
        DialogueId::Clerk => {
            state.clerk_depth = state
                .clerk_depth
                .saturating_add(amount)
                .min(NPC_THREAD_STEPS)
        }
        DialogueId::Child => {
            state.child_depth = state
                .child_depth
                .saturating_add(amount)
                .min(NPC_THREAD_STEPS)
        }
        DialogueId::Keeper => {
            state.keeper_depth = state
                .keeper_depth
                .saturating_add(amount)
                .min(NPC_THREAD_STEPS)
        }
    }
}

fn apply_node_reward(
    state: &mut GameState,
    active: ActiveDialogue,
    choice: DialogueChoiceId,
    event: &mut StoryEvent,
) {
    match (active.dialogue, active.node, choice) {
        (DialogueId::Traveler, DialogueNodeId::Proof, DialogueChoiceId::PromiseTopic) => {
            remember_tag(state, event, Flag::TravelerTrusted, "对话节点：空座被承认");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (DialogueId::Traveler, DialogueNodeId::Tomorrow, DialogueChoiceId::ChallengeTopic) => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodFirstLoop,
                "对话节点：逃离被点破",
            );
        }
        (DialogueId::Clerk, DialogueNodeId::Proof, DialogueChoiceId::PromiseTopic) => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodChildPromise,
                "对话节点：双座成为承诺",
            );
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        (DialogueId::Clerk, DialogueNodeId::Tomorrow, DialogueChoiceId::PromiseTopic) => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("对话节点：明天价格".to_string());
        }
        (DialogueId::Child, DialogueNodeId::Memory, DialogueChoiceId::PromiseTopic) => {
            remember_tag(state, event, Flag::MetChild, "对话节点：白线边界");
            grant_item_tag(state, event, Item::ChildHomework, "获得：没有封面的作业本");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (DialogueId::Child, DialogueNodeId::Proof, DialogueChoiceId::DeepenTopic) => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("对话节点：允许生气".to_string());
        }
        (DialogueId::Child, DialogueNodeId::Tomorrow, DialogueChoiceId::PromiseTopic) => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "对话节点：明天第一站",
            );
        }
        (DialogueId::Keeper, DialogueNodeId::Proof, DialogueChoiceId::PromiseTopic) => {
            remember_tag(state, event, Flag::HeardBroadcastTape, "对话节点：警告补全");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        (DialogueId::Keeper, DialogueNodeId::Tomorrow, DialogueChoiceId::PromiseTopic) => {
            remember_tag(state, event, Flag::HeardClockTruth, "对话节点：守夜边界");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            if state.has_item(Item::OldTimetable) && state.has_flag(Flag::RepairedFogLamp) {
                grant_item_tag(state, event, Item::SignalWhistle, "获得：银色发车哨");
            }
        }
        (_, _, DialogueChoiceId::PromiseTopic) => {
            event.tags.push("对话节点：行动承诺".to_string());
        }
        (_, _, DialogueChoiceId::ChallengeTopic) => {
            event.tags.push("对话节点：矛盾追问".to_string());
        }
        (_, _, DialogueChoiceId::DeepenTopic) => {
            event.tags.push("对话节点：继续倾听".to_string());
        }
        _ => {}
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn grant_item_tag(state: &mut GameState, event: &mut StoryEvent, item: Item, tag: &str) {
    if state.add_item(item) {
        event.tags.push(tag.to_string());
    }
}

pub fn condition_line(state: &GameState) -> Option<String> {
    state.active_dialogue.map(|active| {
        format!(
            "对话中：{} / {}",
            active.dialogue.title(),
            node_title(active)
        )
    })
}

pub fn objective_hint(state: &GameState) -> Option<String> {
    state.active_dialogue.map(|active| {
        format!(
            "正在和{}谈“{}”：选择追问、承诺或回到话题清单。",
            active.dialogue.title(),
            node_title(active)
        )
    })
}

pub fn active_thread_summary(state: &GameState) -> Option<String> {
    state.active_dialogue.map(|active| {
        format!(
            "当前对话：{} / {}；已记录 {} 段，完成 {} / {} 个对话节点。",
            active.dialogue.title(),
            node_title(active),
            state.dialogue_transcript.len(),
            state.completed_dialogue_beats.len(),
            DIALOGUE_BEAT_COUNT
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogue_session_exposes_choices_and_remembers_state() {
        let mut state = GameState::new();
        let entries = available_dialogues(&state);
        assert!(entries
            .iter()
            .any(|entry| entry.dialogue == DialogueId::Traveler));

        let begin = begin(&mut state, DialogueId::Traveler);
        assert!(begin.tags.iter().any(|tag| tag == "对话系统"));
        assert_eq!(
            state.active_dialogue,
            Some(ActiveDialogue {
                dialogue: DialogueId::Traveler,
                node: DialogueNodeId::Root
            })
        );

        let choices = available_choices(&state);
        assert!(choices
            .iter()
            .any(|choice| choice.choice == DialogueChoiceId::TravelerRain));

        let event = choose(&mut state, DialogueChoiceId::TravelerRain);
        assert!(event.tags.iter().any(|tag| tag == "对话选择"));
        assert!(state.has_discussed(TopicId::TravelerRain));
        assert_eq!(
            state.active_dialogue,
            Some(ActiveDialogue {
                dialogue: DialogueId::Traveler,
                node: DialogueNodeId::Memory
            })
        );
        assert!(state.has_completed_dialogue_beat(DialogueBeatKey {
            dialogue: DialogueId::Traveler,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::TravelerRain
        }));

        let node_choices = available_choices(&state);
        assert!(node_choices
            .iter()
            .any(|choice| choice.choice == DialogueChoiceId::DeepenTopic));
        let depth_after_root = state.traveler_depth;
        let node_event = choose(&mut state, DialogueChoiceId::DeepenTopic);
        assert!(node_event.tags.iter().any(|tag| tag == "话题节点"));
        assert_eq!(state.traveler_depth, depth_after_root + 1);
        assert_eq!(state.dialogue_transcript.len(), 3);

        let repeated = choose(&mut state, DialogueChoiceId::DeepenTopic);
        assert!(repeated.tags.iter().any(|tag| tag == "复谈"));
        assert_eq!(state.traveler_depth, depth_after_root + 1);

        choose(&mut state, DialogueChoiceId::Leave);
        assert!(state.active_dialogue.is_none());
    }

    #[test]
    fn every_dialogue_beat_advances_relationship_depth_once() {
        for dialogue in DialogueId::ALL {
            let mut state = GameState::new();
            seed_dialogue_requirements(&mut state, dialogue);

            begin(&mut state, dialogue);
            for root_choice in choices_for(ActiveDialogue {
                dialogue,
                node: DialogueNodeId::Root,
            }) {
                choose(&mut state, root_choice);
                for node_choice in [
                    DialogueChoiceId::DeepenTopic,
                    DialogueChoiceId::ChallengeTopic,
                    DialogueChoiceId::PromiseTopic,
                ] {
                    choose(&mut state, node_choice);
                }
                choose(&mut state, DialogueChoiceId::BackToRoot);
            }

            assert_eq!(
                completed_thread_beats(&state, dialogue),
                thread_total_beats(dialogue)
            );
            assert_eq!(relationship_depth(&state, dialogue), NPC_THREAD_STEPS);

            let depth_after_completion = relationship_depth(&state, dialogue);
            let repeated_root_choice = choices_for(ActiveDialogue {
                dialogue,
                node: DialogueNodeId::Root,
            })[0];
            choose(&mut state, repeated_root_choice);
            assert_eq!(relationship_depth(&state, dialogue), depth_after_completion);
        }
    }

    fn seed_dialogue_requirements(state: &mut GameState, dialogue: DialogueId) {
        state.location = dialogue.location();
        state.remember(Flag::ReadDepartureBoard);
        state.remember(Flag::UnderstoodFirstLoop);
        state.remember(Flag::ExaminedTicket);
        state.ticket = crate::model::TicketKind::Return;
        state.add_item(Item::ChildHomework);
        state.add_item(Item::BroadcastTape);
        state.remember(Flag::HeardClockTruth);
        state.child_trust = 3;
    }

    fn relationship_depth(state: &GameState, dialogue: DialogueId) -> u8 {
        match dialogue {
            DialogueId::Traveler => state.traveler_depth,
            DialogueId::Clerk => state.clerk_depth,
            DialogueId::Child => state.child_depth,
            DialogueId::Keeper => state.keeper_depth,
        }
    }

    #[test]
    fn dialogue_thread_summaries_track_topic_map_progress() {
        let mut state = GameState::new();
        let initial = dialogue_thread_summaries(&state);
        let traveler = initial
            .iter()
            .find(|thread| thread.dialogue == DialogueId::Traveler)
            .expect("traveler thread should be listed");
        assert_eq!(traveler.status, "可进入");
        assert_eq!(traveler.progress, 0);

        begin(&mut state, DialogueId::Traveler);
        choose(&mut state, DialogueChoiceId::TravelerRain);
        let after_root = dialogue_thread_summaries(&state);
        let traveler = after_root
            .iter()
            .find(|thread| thread.dialogue == DialogueId::Traveler)
            .expect("traveler thread should be listed");
        assert_eq!(traveler.status, "对话中");
        assert!(traveler.progress > 0);
        assert!(traveler.detail.contains("报纸与雨"));

        choose(&mut state, DialogueChoiceId::DeepenTopic);
        let after_node = dialogue_thread_summaries(&state);
        let traveler = after_node
            .iter()
            .find(|thread| thread.dialogue == DialogueId::Traveler)
            .expect("traveler thread should be listed");
        assert!(traveler.progress > after_root[0].progress);
        assert!(traveler.detail.contains("记录"));
    }
}
