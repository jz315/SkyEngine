use crate::model::{
    ActiveDialogue, DialogueBeatKey, DialogueChoiceId, DialogueId, DialogueLeadId, DialogueNodeId,
    DialogueTone, Flag, GameState, Item, Location, StoryEvent,
};

pub const DIALOGUE_LEAD_COUNT: usize = 8;

#[derive(Clone, Debug)]
pub struct DialogueLeadAction {
    pub lead: DialogueLeadId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct DialogueLeadReturnAction {
    pub lead: DialogueLeadId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogueLeadSummary {
    pub lead: DialogueLeadId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub returned: bool,
    pub ready: bool,
}

pub fn available_leads(state: &GameState) -> Vec<DialogueLeadAction> {
    DialogueLeadId::ALL
        .iter()
        .copied()
        .filter(|lead| !state.has_completed_dialogue_lead(*lead))
        .filter(|lead| lead.location() == state.location)
        .filter(|lead| lead_visible(state, *lead))
        .map(|lead| {
            let missing = missing_requirements(state, lead);
            DialogueLeadAction {
                lead,
                label: lead.label(),
                detail: if missing.is_empty() {
                    format!(
                        "刚才的对话在{}留下了可追查的痕迹。",
                        lead.location().title()
                    )
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn available_returns(
    state: &GameState,
    active: ActiveDialogue,
) -> Vec<DialogueLeadReturnAction> {
    DialogueLeadId::ALL
        .iter()
        .copied()
        .filter(|lead| lead.source_dialogue() == active.dialogue)
        .filter(|lead| state.has_completed_dialogue_lead(*lead))
        .filter(|lead| !state.has_returned_dialogue_lead(*lead))
        .map(|lead| {
            let enabled = active.dialogue.location() == state.location;
            DialogueLeadReturnAction {
                lead,
                label: lead.return_label(),
                detail: if enabled {
                    format!(
                        "把“{}”带回{}，让证据重新进入谈话。",
                        lead.title(),
                        active.dialogue.title()
                    )
                } else {
                    format!(
                        "需要回到{}，才能把这条线索带回人物对话。",
                        active.dialogue.location().title()
                    )
                },
                enabled,
            }
        })
        .collect()
}

pub fn lead_summaries(state: &GameState) -> Vec<DialogueLeadSummary> {
    DialogueLeadId::ALL
        .iter()
        .copied()
        .map(|lead| {
            let completed = state.has_completed_dialogue_lead(lead);
            let returned = state.has_returned_dialogue_lead(lead);
            let visible = completed || lead_visible(state, lead);
            let missing = missing_requirements(state, lead);
            let ready =
                visible && missing.is_empty() && !completed && lead.location() == state.location;
            let status = if returned {
                "已回谈"
            } else if completed {
                "已追查"
            } else if ready {
                "可追查"
            } else if visible {
                "有线索"
            } else {
                "未显形"
            };
            let detail = if returned {
                lead.return_review().to_string()
            } else if completed {
                format!(
                    "{} 带回{}的对话里，会得到新的回应。",
                    lead.review(),
                    lead.source_dialogue().title()
                )
            } else if ready {
                format!(
                    "{}就在这里。把刚才的对话带回场景里，让它变成可处理的事实。",
                    lead.title()
                )
            } else if visible {
                format!(
                    "{}已经显形。前往{}；{}",
                    lead.title(),
                    lead.location().title(),
                    if missing.is_empty() {
                        "条件已经足够。".to_string()
                    } else {
                        format!("还缺：{}。", missing.join("；"))
                    }
                )
            } else {
                format!(
                    "这条对话线索还没露面。继续完成{}的相关对话节点。",
                    lead.source_dialogue().title()
                )
            };

            DialogueLeadSummary {
                lead,
                title: lead.title(),
                status,
                detail,
                progress: lead_progress(visible, completed, missing.len()),
                visible,
                completed,
                returned,
                ready,
            }
        })
        .collect()
}

pub fn follow(state: &mut GameState, lead: DialogueLeadId) -> StoryEvent {
    if state.has_completed_dialogue_lead(lead) {
        return StoryEvent::new(
            "这条对话线索已经追查过",
            "你又回到同一个痕迹前。它没有消失，只是已经从谜面变成了你手里的事实；下一步要把事实拿去问人，而不是让它继续站在原地。",
        )
        .tag("对话线索");
    }

    let missing = missing_requirements(state, lead);
    if lead.location() != state.location || !lead_visible(state, lead) || !missing.is_empty() {
        return StoryEvent::new(
            "对话线索还没有落点",
            format!(
                "这句话已经在车站里留下回声，但它还不能在这里被追查。{}",
                if lead.location() != state.location {
                    format!("它指向{}。", lead.location().title())
                } else if missing.is_empty() {
                    "你还没有真正问出那句能让它显形的话。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("对话线索");
    }

    state.complete_dialogue_lead(lead);
    let mut event = lead_event(lead);
    apply_lead_rewards(state, lead, &mut event);
    event
}

pub fn return_to_dialogue(state: &mut GameState, lead: DialogueLeadId) -> StoryEvent {
    let Some(active) = state.active_dialogue else {
        return StoryEvent::new(
            "还没有人在听这条线索",
            "你握着线索站在原地。它已经足够具体，但需要一个正在进行的对话，才会变成回应。",
        )
        .tag("线索回谈");
    };

    if lead.source_dialogue() != active.dialogue {
        return StoryEvent::new(
            "这条线索不属于当前对话",
            format!(
                "你正和{}说话，而“{}”应该带回{}那里。",
                active.dialogue.title(),
                lead.title(),
                lead.source_dialogue().title()
            ),
        )
        .tag("线索回谈");
    }

    if !state.has_completed_dialogue_lead(lead) {
        return StoryEvent::new(
            "线索还没有被追查",
            format!(
                "“{}”还只是对话里的回声。先去对应地点把它查成事实。",
                lead.title()
            ),
        )
        .tag("线索回谈");
    }

    if state.has_returned_dialogue_lead(lead) {
        return StoryEvent::new(
            "这条线索已经带回去过",
            "对方没有厌烦，只提醒你：证据被回应以后，下一步就不是重复举证，而是照着它行动。",
        )
        .tag("线索回谈");
    }

    state.return_dialogue_lead(lead);
    let mut event = return_event(lead, state.dialogue_tone);
    apply_return_rewards(state, lead, &mut event);
    event
}

impl DialogueLeadId {
    pub const ALL: [Self; DIALOGUE_LEAD_COUNT] = [
        Self::RainUnderBench,
        Self::EmptySeatLedger,
        Self::ReturnStub,
        Self::TwoSeatMap,
        Self::HomeworkMargin,
        Self::WhiteLineChalk,
        Self::MinuteHandNote,
        Self::BroadcastDraft,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::RainUnderBench => Location::WaitingHall,
            Self::EmptySeatLedger => Location::LostAndFound,
            Self::ReturnStub => Location::TicketOffice,
            Self::TwoSeatMap => Location::WaitingHall,
            Self::HomeworkMargin | Self::WhiteLineChalk => Location::Platform,
            Self::MinuteHandNote | Self::BroadcastDraft => Location::ClockTower,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::RainUnderBench => "追查对话线索：报纸雨痕滴到哪",
            Self::EmptySeatLedger => "追查对话线索：空座的登记页",
            Self::ReturnStub => "追查对话线索：湿票退票根",
            Self::TwoSeatMap => "追查对话线索：两张座位图",
            Self::HomeworkMargin => "追查对话线索：作业本页边",
            Self::WhiteLineChalk => "追查对话线索：被重描的白线",
            Self::MinuteHandNote => "追查对话线索：分针背面的字",
            Self::BroadcastDraft => "追查对话线索：广播稿的缺口",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::RainUnderBench => "雨痕滴到长椅下",
            Self::EmptySeatLedger => "空座登记页",
            Self::ReturnStub => "湿票退票根",
            Self::TwoSeatMap => "两张座位图",
            Self::HomeworkMargin => "作业本页边",
            Self::WhiteLineChalk => "重描白线",
            Self::MinuteHandNote => "分针背面的字",
            Self::BroadcastDraft => "广播稿缺口",
        }
    }

    fn return_label(self) -> &'static str {
        match self {
            Self::RainUnderBench => "带回线索：长椅下的镜片",
            Self::EmptySeatLedger => "带回线索：空座登记页",
            Self::ReturnStub => "带回线索：湿票退票根",
            Self::TwoSeatMap => "带回线索：两张座位图",
            Self::HomeworkMargin => "带回线索：作业本页边",
            Self::WhiteLineChalk => "带回线索：重描白线",
            Self::MinuteHandNote => "带回线索：分针背面的字",
            Self::BroadcastDraft => "带回线索：广播稿缺口",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::RainUnderBench => "你从报纸雨痕下找到镜片；老人说过的雨从此不只是比喻。",
            Self::EmptySeatLedger => "空座在失物账里有了页码；第二个座位开始变成可查的事实。",
            Self::ReturnStub => "退票根证明湿票不是警告而已，它曾经真的进入窗口流程。",
            Self::TwoSeatMap => "两张座位图让返程不再是口头承诺，而是一份必须被办理的关系。",
            Self::HomeworkMargin => "作业本页边把白线后的孩子拉回具体的明天。",
            Self::WhiteLineChalk => "重描过的白线证明边界可以被照顾，而不是被胜利地抹掉。",
            Self::MinuteHandNote => "分针背面的字把守夜从神圣职位拉回一笔需要归还的账。",
            Self::BroadcastDraft => "广播稿缺口让警告第一次包含第二个名字和后来者。",
        }
    }

    fn return_review(self) -> &'static str {
        match self {
            Self::RainUnderBench => "老人看过镜片，承认雨痕会留下物证，不只是诗意。",
            Self::EmptySeatLedger => "老人把空座登记页收进报纸，第二个座位不再只是你的愧疚。",
            Self::ReturnStub => "售票员看过退票根，窗口开始承认你曾经办过返程手续。",
            Self::TwoSeatMap => "售票员在两张座位图上盖章，返程规则更接近可执行的承诺。",
            Self::HomeworkMargin => "孩子听你读完页边日期，知道你没有把他的等待说成寓言。",
            Self::WhiteLineChalk => "孩子承认白线可以被照顾，边界从惩罚变成他自己的选择。",
            Self::MinuteHandNote => "站务员听见分针背面的字，守夜第一次像一笔能归还的账。",
            Self::BroadcastDraft => "站务员读过广播稿缺口，警告终于不再只保存一个人的声音。",
        }
    }

    fn source_dialogue(self) -> DialogueId {
        required_beat(self).dialogue
    }
}

fn lead_visible(state: &GameState, lead: DialogueLeadId) -> bool {
    state.has_completed_dialogue_beat(required_beat(lead))
}

fn missing_requirements(state: &GameState, lead: DialogueLeadId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        lead_visible(state, lead),
        "完成对应的人物对话节点",
    );
    match lead {
        DialogueLeadId::RainUnderBench => {}
        DialogueLeadId::EmptySeatLedger => require(
            &mut missing,
            state.has_item(Item::CoinToken) || state.has_flag(Flag::TravelerTrusted),
            "取得退票铜筹，或让老人承认空座",
        ),
        DialogueLeadId::ReturnStub => require(
            &mut missing,
            state.has_flag(Flag::ExaminedTicket),
            "先看清湿票背面",
        ),
        DialogueLeadId::TwoSeatMap => require(
            &mut missing,
            state.has_discussed(crate::model::TopicId::ClerkSeats)
                || state.has_flag(Flag::UnderstoodChildPromise),
            "让两张座位进入对话记录",
        ),
        DialogueLeadId::HomeworkMargin => require(
            &mut missing,
            state.has_item(Item::ChildHomework),
            "拿到没有封面的作业本",
        ),
        DialogueLeadId::WhiteLineChalk => require(
            &mut missing,
            state.child_trust >= 2,
            "让孩子相信你至少能听完一句气话",
        ),
        DialogueLeadId::MinuteHandNote => {}
        DialogueLeadId::BroadcastDraft => require(
            &mut missing,
            state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
            "取得或听过广播磁带",
        ),
    }
    missing
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn required_beat(lead: DialogueLeadId) -> DialogueBeatKey {
    match lead {
        DialogueLeadId::RainUnderBench => DialogueBeatKey {
            dialogue: DialogueId::Traveler,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::TravelerRain,
        },
        DialogueLeadId::EmptySeatLedger => DialogueBeatKey {
            dialogue: DialogueId::Traveler,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::TravelerEmptySeat,
        },
        DialogueLeadId::ReturnStub => DialogueBeatKey {
            dialogue: DialogueId::Clerk,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::ClerkTicket,
        },
        DialogueLeadId::TwoSeatMap => DialogueBeatKey {
            dialogue: DialogueId::Clerk,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::ClerkReturnRule,
        },
        DialogueLeadId::HomeworkMargin => DialogueBeatKey {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::ChildWhiteLine,
        },
        DialogueLeadId::WhiteLineChalk => DialogueBeatKey {
            dialogue: DialogueId::Child,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::ChildAnger,
        },
        DialogueLeadId::MinuteHandNote => DialogueBeatKey {
            dialogue: DialogueId::Keeper,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::KeeperDuty,
        },
        DialogueLeadId::BroadcastDraft => DialogueBeatKey {
            dialogue: DialogueId::Keeper,
            node: DialogueNodeId::Root,
            choice: DialogueChoiceId::KeeperBroadcast,
        },
    }
}

fn lead_progress(visible: bool, completed: bool, missing_count: usize) -> u8 {
    if completed {
        100
    } else if visible && missing_count == 0 {
        70
    } else if visible {
        40
    } else {
        0
    }
}

pub fn active_return_objective_hint(state: &GameState) -> Option<String> {
    let active = state.active_dialogue?;
    available_returns(state, active)
        .into_iter()
        .find(|lead| lead.enabled)
        .map(|lead| format!("可以把已追查的线索带回对话：{}。", lead.label))
}

fn apply_lead_rewards(state: &mut GameState, lead: DialogueLeadId, event: &mut StoryEvent) {
    match lead {
        DialogueLeadId::RainUnderBench => {
            grant_item_tag(state, event, Item::MirrorShard, "获得：候车厅镜片");
            remember_tag(state, event, Flag::FoundMirrorShard, "线索：镜片");
        }
        DialogueLeadId::EmptySeatLedger => {
            grant_item_tag(state, event, Item::StationMap, "获得：折叠站内图");
            remember_tag(state, event, Flag::FoundStationMap, "线索：空座页码");
        }
        DialogueLeadId::ReturnStub => {
            grant_item_tag(state, event, Item::CoinToken, "获得：退票铜筹");
            remember_tag(state, event, Flag::FoundCoinToken, "线索：退票根");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        DialogueLeadId::TwoSeatMap => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "线索：双座图");
            remember_tag(state, event, Flag::SynthesizedRoute, "线索：返程条件");
        }
        DialogueLeadId::HomeworkMargin => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "线索：作业页边");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueLeadId::WhiteLineChalk => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "线索：白线边界");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueLeadId::MinuteHandNote => {
            grant_item_tag(state, event, Item::OldTimetable, "获得：烧焦的旧时刻表");
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "线索：分针背面",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        DialogueLeadId::BroadcastDraft => {
            grant_item_tag(state, event, Item::BroadcastTape, "获得：广播室磁带");
            remember_tag(state, event, Flag::HeardBroadcastTape, "线索：广播稿");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
    }
}

fn apply_return_rewards(state: &mut GameState, lead: DialogueLeadId, event: &mut StoryEvent) {
    match lead {
        DialogueLeadId::RainUnderBench | DialogueLeadId::EmptySeatLedger => {
            remember_tag(state, event, Flag::TravelerTrusted, "回谈：老人信任");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DialogueLeadId::ReturnStub | DialogueLeadId::TwoSeatMap => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            if matches!(lead, DialogueLeadId::TwoSeatMap) {
                remember_tag(state, event, Flag::SynthesizedRoute, "回谈：返程可执行");
            }
        }
        DialogueLeadId::HomeworkMargin | DialogueLeadId::WhiteLineChalk => {
            state.child_trust = (state.child_trust + 1).min(5);
            if matches!(lead, DialogueLeadId::WhiteLineChalk) {
                remember_tag(
                    state,
                    event,
                    Flag::SynthesizedChildTruth,
                    "回谈：边界被承认",
                );
            }
        }
        DialogueLeadId::MinuteHandNote | DialogueLeadId::BroadcastDraft => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            if matches!(lead, DialogueLeadId::BroadcastDraft) {
                remember_tag(
                    state,
                    event,
                    Flag::SynthesizedStationTruth,
                    "回谈：广播补全",
                );
            } else {
                remember_tag(
                    state,
                    event,
                    Flag::UnderstoodStationMechanism,
                    "回谈：守夜账目",
                );
            }
        }
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

fn lead_event(lead: DialogueLeadId) -> StoryEvent {
    let (title, body) = match lead {
        DialogueLeadId::RainUnderBench => (
            "对话线索：雨痕滴到长椅下",
            "你顺着老人报纸边缘的水痕看下去，长椅底部卡着一枚细小镜片。它没有照出你的整张脸，只照出眼睛下方那一点疲惫。老人刚才说雨会记住落点；现在你明白，落点有时候是一件小得几乎能被扫走的证物。",
        ),
        DialogueLeadId::EmptySeatLedger => (
            "对话线索：空座登记页",
            "失物招领处的账册在“07B”那一页自己鼓起。页边写着：空座不是空缺，是保留。你摸到一张折叠站内图，背面用铅笔圈出候车厅、售票窗口和月台，像有人很早就知道你必须在三处之间来回。",
        ),
        DialogueLeadId::ReturnStub => (
            "对话线索：湿票退票根",
            "售票窗口抽屉深处压着一张退票根，号码和你湿票上的水痕对得上。售票员没有解释，只把票根推出来一点。你听见金属柜里有硬币滚动，像某种手续终于承认你不是第一次来办这件事。",
        ),
        DialogueLeadId::TwoSeatMap => (
            "对话线索：两张座位图",
            "候车厅时刻表短暂熄灭，随后浮出两张重叠的座位图。07A 和 07B 没有谁压住谁，它们只是并排亮着。你忽然知道，返程票最难的不是买到第二张，而是承认第二张不会因为你痛苦就自动属于你。",
        ),
        DialogueLeadId::HomeworkMargin => (
            "对话线索：作业本页边",
            "孩子把作业本递给你，只允许你看页边。那里写着一串很小的日期，每个日期后面都跟着“他又说下次”。你没有把它读成控诉，也没有读成赦免；它只是一份孩子保存明天的方式。",
        ),
        DialogueLeadId::WhiteLineChalk => (
            "对话线索：被重描的白线",
            "白线有一小段颜色比别处新。你蹲下去看，发现旧粉笔没有被抹掉，只是被新的边界盖住。孩子说：不是所有重新开始都要擦干净以前。你点头，把这句话当成规矩，而不是安慰。",
        ),
        DialogueLeadId::MinuteHandNote => (
            "对话线索：分针背面的字",
            "旧钟分针背面刻着一行小字：借出一分钟，归还一个明天。站务员看见你读出来，脸色像被灯照到。钟座下方落出一页烧焦的旧时刻表，所有车次都停在同一格，像一份迟到很久的认罪书。",
        ),
        DialogueLeadId::BroadcastDraft => (
            "对话线索：广播稿的缺口",
            "广播稿每一行都少一个名字。你把缺口念出来，磁带机忽然咔哒一声吐出黑色磁带。站务员没有阻止，只说：如果要播，就不要只播警告。后来者需要知道危险，也需要知道危险曾经属于谁。",
        ),
    };
    StoryEvent::new(title, body).tag("对话线索").tag("自由探索")
}

fn return_event(lead: DialogueLeadId, tone: DialogueTone) -> StoryEvent {
    let tone_prefix = match tone {
        DialogueTone::Listening => "你没有急着解释，只把线索放到对方面前。 ",
        DialogueTone::Gentle => "你把证据递得很轻，像怕它再次变成审判。 ",
        DialogueTone::Direct => "你直接把线索推到谈话中央，不再让它绕路。 ",
    };
    let (title, body) = match lead {
        DialogueLeadId::RainUnderBench => (
            "线索回谈：老人和镜片",
            "老人看见镜片时，报纸第一次没有挡住他的手。他说：你终于知道雨不是气氛了。能照出脸的东西都危险，因为它会让人不能继续假装自己只是路过。",
        ),
        DialogueLeadId::EmptySeatLedger => (
            "线索回谈：老人和空座页码",
            "你把空座登记页给老人看。他用指尖按住 07B，说：好，现在它不是你的梦了。账册里的座位不会原谅你，但它会阻止你再把另一个人说成雾。",
        ),
        DialogueLeadId::ReturnStub => (
            "线索回谈：售票员和退票根",
            "售票员读完退票根，业务口吻慢慢裂开。她说：原来你不是第一次排到这个窗口。那就别再装成第一次听懂返程的规则。",
        ),
        DialogueLeadId::TwoSeatMap => (
            "线索回谈：售票员和双座图",
            "两张座位图并排摊在窗口下。售票员没有立刻盖章，只问：如果他不坐 07B 呢？你说那也是他的位置，不是我的奖品。",
        ),
        DialogueLeadId::HomeworkMargin => (
            "线索回谈：孩子和页边日期",
            "孩子听你念完页边日期，没有纠正你。他说：你这次没有把它念得像检讨。它们就是日子，难过的、无聊的、等不到人的日子。",
        ),
        DialogueLeadId::WhiteLineChalk => (
            "线索回谈：孩子和新白线",
            "你说白线没有被擦掉，只是被重新描过。孩子低头看自己的鞋尖，说：那我以后越过它，也不是输给你。你说对，是你自己决定走到哪里。",
        ),
        DialogueLeadId::MinuteHandNote => (
            "线索回谈：站务员和分针字迹",
            "站务员听你念出那行字，像被旧钟从制服里叫出来。他说：原来我守着的不是一分钟，是一张没有归还日期的借据。",
        ),
        DialogueLeadId::BroadcastDraft => (
            "线索回谈：站务员和广播缺口",
            "站务员读到广播稿缺口，终于没有跳过第二个名字。他说：警告如果只保护说话的人，就还是旧命令的一部分。",
        ),
    };
    StoryEvent::new(title, format!("{tone_prefix}{body}"))
        .tag("线索回谈")
        .tag("自由对话")
        .tag(format!("语气：{}", tone.name()))
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let completed = state.completed_dialogue_leads.len();
    if completed == 0 {
        return None;
    }
    Some(format!(
        "你追查了 {completed} 条对话线索。今晚不是只有人会回答，地点也开始替那些回答留下证据。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lead_summaries_track_visible_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = lead_summaries(&state);
        let rain = initial
            .iter()
            .find(|summary| summary.lead == DialogueLeadId::RainUnderBench)
            .expect("rain lead should be listed");
        assert_eq!(rain.status, "未显形");
        assert_eq!(rain.progress, 0);

        state.complete_dialogue_beat(required_beat(DialogueLeadId::RainUnderBench));
        let ready = lead_summaries(&state);
        let rain = ready
            .iter()
            .find(|summary| summary.lead == DialogueLeadId::RainUnderBench)
            .expect("rain lead should be listed");
        assert_eq!(rain.status, "可追查");
        assert!(rain.ready);

        let event = follow(&mut state, DialogueLeadId::RainUnderBench);
        assert!(event.tags.iter().any(|tag| tag == "对话线索"));
        assert!(state.has_completed_dialogue_lead(DialogueLeadId::RainUnderBench));
        assert!(state.has_item(Item::MirrorShard));

        state.active_dialogue = Some(ActiveDialogue {
            dialogue: DialogueId::Traveler,
            node: DialogueNodeId::Root,
        });
        let returns = available_returns(
            &state,
            ActiveDialogue {
                dialogue: DialogueId::Traveler,
                node: DialogueNodeId::Root,
            },
        );
        assert!(returns
            .iter()
            .any(|action| action.lead == DialogueLeadId::RainUnderBench && action.enabled));
        let response = return_to_dialogue(&mut state, DialogueLeadId::RainUnderBench);
        assert!(response.tags.iter().any(|tag| tag == "线索回谈"));
        assert!(state.has_returned_dialogue_lead(DialogueLeadId::RainUnderBench));
        let returned = lead_summaries(&state);
        let rain = returned
            .iter()
            .find(|summary| summary.lead == DialogueLeadId::RainUnderBench)
            .expect("rain lead should be listed");
        assert_eq!(rain.status, "已回谈");
        assert!(rain.returned);
    }

    #[test]
    fn lead_count_matches_declared_table() {
        assert_eq!(DIALOGUE_LEAD_COUNT, DialogueLeadId::ALL.len());
    }
}
