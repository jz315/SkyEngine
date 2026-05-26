use crate::model::{
    AftertalkId, CompanionTalkId, DepartureId, EvidenceId, Flag, GameState, Item, Location,
    MemoryId, PatrolId, StationRequestId, StoryEvent, TicketKind, TopicId, VowId,
};

pub const COMPANION_TALK_COUNT: usize = 12;

#[derive(Clone, Debug)]
pub struct CompanionAction {
    pub talk: CompanionTalkId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompanionSummary {
    pub talk: CompanionTalkId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub ready: bool,
}

pub fn available_companion_talks(state: &GameState) -> Vec<CompanionAction> {
    CompanionTalkId::ALL
        .iter()
        .copied()
        .filter(|talk| !state.has_completed_companion_talk(*talk))
        .filter(|talk| talk.location() == state.location)
        .filter(|talk| companion_visible(state, *talk))
        .map(|talk| {
            let missing = missing_requirements(state, talk);
            CompanionAction {
                talk,
                label: talk.label(),
                detail: if missing.is_empty() {
                    "这段同行对话已经能发生。它不会给出正确答案，只会让孩子把地点也说进来。"
                        .to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn companion_summaries(state: &GameState) -> Vec<CompanionSummary> {
    CompanionTalkId::ALL
        .iter()
        .copied()
        .map(|talk| {
            let completed = state.has_completed_companion_talk(talk);
            let visible = completed || companion_visible(state, talk);
            let missing = missing_requirements(state, talk);
            let ready = visible && missing.is_empty() && !completed;
            let progress = companion_progress(talk, missing.len(), visible, completed);
            let status = if completed {
                "已同行"
            } else if ready {
                "可交谈"
            } else if visible {
                "待同行"
            } else {
                "未显形"
            };
            let detail = if completed {
                talk.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以继续。前往{}，让孩子也看见这里。",
                    talk.title(),
                    talk.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这段同行对话还藏在{}。让孩子愿意同行后，地点会开始回答他的问题。",
                    talk.location().title()
                )
            };

            CompanionSummary {
                talk,
                title: talk.title(),
                status,
                detail,
                progress,
                visible,
                completed,
                ready,
            }
        })
        .collect()
}

pub fn talk(state: &mut GameState, talk: CompanionTalkId) -> StoryEvent {
    if state.has_completed_companion_talk(talk) {
        return StoryEvent::new(
            "这段同行已经记下",
            "孩子又看了一眼这里，像确认那句话仍在原处。他没有重复问题，因为今晚已经回答过一次。",
        )
        .tag("同行对话");
    }

    let missing = missing_requirements(state, talk);
    if talk.location() != state.location || !companion_visible(state, talk) || !missing.is_empty() {
        return StoryEvent::new(
            "同行对话还没有入口",
            format!(
                "你感觉这处地点会让孩子说些什么，但今晚还缺一块能站稳的事实。{}",
                if talk.location() != state.location {
                    format!("它不在这里，而在{}。", talk.location().title())
                } else if !companion_visible(state, talk) {
                    "你们还没有走到能一起看见这里的关系。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("同行对话");
    }

    state.complete_companion_talk(talk);
    let mut event = companion_event(talk);
    apply_companion_rewards(state, talk, &mut event);
    event
}

impl CompanionTalkId {
    pub const ALL: [Self; COMPANION_TALK_COUNT] = [
        Self::WaitingHallEmptySeat,
        Self::WaitingHallDepartureBoard,
        Self::TicketOfficeTwoTickets,
        Self::TicketOfficePriceQuestion,
        Self::LostFoundNamedBox,
        Self::LostFoundRaincoatSleeve,
        Self::UnderpassEchoStep,
        Self::UnderpassStairsTomorrow,
        Self::ClockTowerBorrowedMinute,
        Self::ClockTowerRulesForLight,
        Self::PlatformWhiteLineTogether,
        Self::PlatformDoorQuestion,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::WaitingHallEmptySeat | Self::WaitingHallDepartureBoard => Location::WaitingHall,
            Self::TicketOfficeTwoTickets | Self::TicketOfficePriceQuestion => {
                Location::TicketOffice
            }
            Self::LostFoundNamedBox | Self::LostFoundRaincoatSleeve => Location::LostAndFound,
            Self::UnderpassEchoStep | Self::UnderpassStairsTomorrow => Location::Underpass,
            Self::ClockTowerBorrowedMinute | Self::ClockTowerRulesForLight => Location::ClockTower,
            Self::PlatformWhiteLineTogether | Self::PlatformDoorQuestion => Location::Platform,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingHallEmptySeat => "同行对话：让孩子坐上第七张长椅",
            Self::WaitingHallDepartureBoard => "同行对话：和他一起读电子时刻表",
            Self::TicketOfficeTwoTickets => "同行对话：在窗口前核对两张票",
            Self::TicketOfficePriceQuestion => "同行对话：问售票员明天的价格",
            Self::LostFoundNamedBox => "同行对话：把姓名牌放进姓名那格",
            Self::LostFoundRaincoatSleeve => "同行对话：翻开雨衣袖口的线头",
            Self::UnderpassEchoStep => "同行对话：让他听同步的回声",
            Self::UnderpassStairsTomorrow => "同行对话：数通往明天的台阶",
            Self::ClockTowerBorrowedMinute => "同行对话：解释借来的最后一分钟",
            Self::ClockTowerRulesForLight => "同行对话：给雾灯写第二条规矩",
            Self::PlatformWhiteLineTogether => "同行对话：并肩站在白线内侧",
            Self::PlatformDoorQuestion => "同行对话：问他要不要看广播室门",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WaitingHallEmptySeat => "孩子和第七张长椅",
            Self::WaitingHallDepartureBoard => "孩子和电子时刻表",
            Self::TicketOfficeTwoTickets => "孩子和两张票",
            Self::TicketOfficePriceQuestion => "孩子和明天的价格",
            Self::LostFoundNamedBox => "孩子和正确的格子",
            Self::LostFoundRaincoatSleeve => "孩子和雨衣袖口",
            Self::UnderpassEchoStep => "孩子和同步回声",
            Self::UnderpassStairsTomorrow => "孩子和通往明天的台阶",
            Self::ClockTowerBorrowedMinute => "孩子和借来的分钟",
            Self::ClockTowerRulesForLight => "孩子和雾灯规矩",
            Self::PlatformWhiteLineTogether => "孩子和白线内侧",
            Self::PlatformDoorQuestion => "孩子和广播室门",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WaitingHallEmptySeat => "孩子坐过第七张长椅，空座从此不再只替你发言。",
            Self::WaitingHallDepartureBoard => "孩子读过时刻表，知道目的地也可以写成两个人。",
            Self::TicketOfficeTwoTickets => "两张票在窗口前被核对，返程开始像手续，也像关系。",
            Self::TicketOfficePriceQuestion => "售票员被问过明天的价格，规则终于没能只收取歉意。",
            Self::LostFoundNamedBox => "姓名牌被放进姓名那格，孩子不再被你的歉意代管。",
            Self::LostFoundRaincoatSleeve => "雨衣袖口的线头被翻出，旧案重新拥有了具体温度。",
            Self::UnderpassEchoStep => "孩子听见同步回声，知道这里不是只会重复旧错误。",
            Self::UnderpassStairsTomorrow => "你们一起数过台阶，明天变成可以慢慢走的路。",
            Self::ClockTowerBorrowedMinute => "孩子听懂最后一分钟，知道补偿不能再伪装成时间本身。",
            Self::ClockTowerRulesForLight => "雾灯多了一条规矩：照路不能要求被爱。",
            Self::PlatformWhiteLineTogether => "你们并肩站过白线内侧，界限从惩罚变成共同确认。",
            Self::PlatformDoorQuestion => "孩子问过广播室门，知道警告也必须说明它保护谁。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::WaitingHallEmptySeat
            | Self::WaitingHallDepartureBoard
            | Self::TicketOfficeTwoTickets
            | Self::TicketOfficePriceQuestion
            | Self::LostFoundNamedBox
            | Self::LostFoundRaincoatSleeve
            | Self::UnderpassEchoStep
            | Self::UnderpassStairsTomorrow
            | Self::ClockTowerBorrowedMinute
            | Self::ClockTowerRulesForLight
            | Self::PlatformWhiteLineTogether
            | Self::PlatformDoorQuestion => 3,
        }
    }
}

fn companion_visible(state: &GameState, talk: CompanionTalkId) -> bool {
    if state.has_completed_companion_talk(talk) {
        return true;
    }
    state.has_flag(Flag::MetChild)
        && (state.has_flag(Flag::ChildJoined)
            || state.child_trust >= 4
            || talk.location() == Location::Platform)
}

fn missing_requirements(state: &GameState, talk: CompanionTalkId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_flag(Flag::ChildJoined),
        "让孩子愿意同行",
    );
    match talk {
        CompanionTalkId::WaitingHallEmptySeat => {
            require(
                &mut missing,
                state.has_memory(MemoryId::SeventhBench)
                    || state.has_completed_patrol(PatrolId::WaitingHallManifest),
                "走过第七张长椅记忆或完成候车厅巡夜",
            );
            require(
                &mut missing,
                state.has_flag(Flag::TravelerTrusted) || state.has_discussed(TopicId::TravelerRain),
                "让老人承认雨夜或信任你",
            );
        }
        CompanionTalkId::WaitingHallDepartureBoard => {
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
            require(
                &mut missing,
                state.has_flag(Flag::SynthesizedRoute)
                    || state.has_discussed(TopicId::ChildTomorrow),
                "整理路线，或和孩子谈过明天",
            );
        }
        CompanionTalkId::TicketOfficeTwoTickets => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken) || state.ticket == TicketKind::Return,
                "取得退票铜筹或改签返程票",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkSeats)
                    || state.has_prepared_departure(DepartureId::ChildWindowSeat),
                "追问两个座位，或保留 07B",
            );
        }
        CompanionTalkId::TicketOfficePriceQuestion => {
            require(
                &mut missing,
                state.ticket == TicketKind::Return
                    || state.has_prepared_departure(DepartureId::ChildWindowSeat),
                "取得返程票，或让窗口保留 07B",
            );
            require(
                &mut missing,
                state.has_vow(VowId::DoNotOwnTheChild) || state.has_vow(VowId::OrdinaryTomorrow),
                "写下关于孩子或明天的锚点",
            );
        }
        CompanionTalkId::LostFoundNamedBox => {
            require(
                &mut missing,
                state.has_flag(Flag::ReturnedNameTag),
                "归还姓名牌",
            );
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::LostFoundShelfAudit)
                    || state.has_discussed(TopicId::LostFoundNameTag),
                "复核失物标签或追问姓名牌",
            );
        }
        CompanionTalkId::LostFoundRaincoatSleeve => {
            require(
                &mut missing,
                state.has_memory(MemoryId::RaincoatPocket)
                    || state.has_completed_request(StationRequestId::HomeworkEnvelope),
                "走过雨衣口袋记忆或封好作业本页角",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework)
                    || state.has_presented(EvidenceId::ChildHomework),
                "取得或出示孩子的作业本",
            );
        }
        CompanionTalkId::UnderpassEchoStep => {
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::UnderpassWaterline)
                    || state.has_memory(MemoryId::EvacuationLine),
                "量过地下水线或走过疏散记忆",
            );
        }
        CompanionTalkId::UnderpassStairsTomorrow => {
            require(
                &mut missing,
                state.has_flag(Flag::SynthesizedChildTruth)
                    || state.has_memory(MemoryId::OrdinaryKitchen),
                "整理孩子真相，或走过普通清晨记忆",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ChildLeaving)
                    || state.has_completed_aftertalk(AftertalkId::ChildDepartureSeat),
                "和孩子谈过离开，或回访 07B",
            );
        }
        CompanionTalkId::ClockTowerBorrowedMinute => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth)
                    || state.has_memory(MemoryId::BorrowedClockMinute),
                "听懂旧钟代价或走过借来的最后一分钟",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.keeper_depth >= 1,
                "取得站务日志或让站务员开口",
            );
        }
        CompanionTalkId::ClockTowerRulesForLight => {
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodStationMechanism),
                "理解车站机制",
            );
            require(
                &mut missing,
                state.has_vow(VowId::LightWithoutDebt)
                    || state.has_prepared_departure(DepartureId::KeeperLedger),
                "写下无债之光锚点或翻开值夜簿新页",
            );
        }
        CompanionTalkId::PlatformWhiteLineTogether => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::PlatformBoundary)
                    || state.has_memory(MemoryId::WhiteLineMeasure),
                "重描白线或走过白线刻度记忆",
            );
            require(
                &mut missing,
                state.child_trust >= 5,
                "让孩子完全愿意与你同行",
            );
        }
        CompanionTalkId::PlatformDoorQuestion => {
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp)
                    || state.has_prepared_departure(DepartureId::BroadcastScript),
                "修复雾灯或誊清广播稿",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape)
                    || state.has_item(Item::SignalWhistle)
                    || state.has_completed_aftertalk(AftertalkId::KeeperBroadcastReply),
                "取得广播磁带、发车哨，或回访广播稿",
            );
        }
    }
    missing
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn companion_progress(
    talk: CompanionTalkId,
    missing_count: usize,
    visible: bool,
    completed: bool,
) -> u8 {
    if completed {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = talk.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_companion_rewards(state: &mut GameState, talk: CompanionTalkId, event: &mut StoryEvent) {
    state.child_trust = (state.child_trust + 1).min(5);
    match talk {
        CompanionTalkId::WaitingHallEmptySeat => {
            remember_tag(state, event, Flag::TravelerTrusted, "同行：老人信任");
        }
        CompanionTalkId::WaitingHallDepartureBoard | CompanionTalkId::TicketOfficeTwoTickets => {
            remember_tag(state, event, Flag::SynthesizedRoute, "同行：路线具体化");
        }
        CompanionTalkId::TicketOfficePriceQuestion => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "同行：明天价格");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        CompanionTalkId::LostFoundNamedBox => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "同行：姓名归位");
        }
        CompanionTalkId::LostFoundRaincoatSleeve | CompanionTalkId::UnderpassStairsTomorrow => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "同行：孩子真相");
        }
        CompanionTalkId::UnderpassEchoStep => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "同行：同步回声");
        }
        CompanionTalkId::ClockTowerBorrowedMinute => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "同行：借来时间",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        CompanionTalkId::ClockTowerRulesForLight => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "同行：灯的规矩",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        CompanionTalkId::PlatformWhiteLineTogether => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "同行：白线内侧");
        }
        CompanionTalkId::PlatformDoorQuestion => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "同行：广播室门",
            );
        }
    }
    event
        .tags
        .push(format!("同行地点：{}", talk.location().title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn companion_event(talk: CompanionTalkId) -> StoryEvent {
    let (title, body) = match talk {
        CompanionTalkId::WaitingHallEmptySeat => (
            "同行对话：孩子和第七张长椅",
            "孩子坐上第七张长椅时，脚尖够不到地。他没有问这里是不是他的位置，只问：如果一个位置一直空着，是不是别人就会以为坐过它的人不重要？老人把报纸折低了一点，说空位不能替人说话。于是你第一次没有让空位替自己道歉，只把旁边那一寸留给他慢慢坐稳。",
        ),
        CompanionTalkId::WaitingHallDepartureBoard => (
            "同行对话：孩子和电子时刻表",
            "电子时刻表把目的地刷成许多姓名。孩子仰头看了一会儿，说如果目的地写两个人，会不会太挤？你说也许会，但至少不会只剩一个人抵达。他把手伸进袖口，像把这句话先藏起来，等真的害怕时再拿出来用。",
        ),
        CompanionTalkId::TicketOfficeTwoTickets => (
            "同行对话：孩子和两张票",
            "你把两张票在窗口前摊开。孩子没有碰票，只用指尖沿着边缘比了一下，说它们看起来一样，却有一张像在等谁补一句话。售票员低头盖章，章声比以前轻。返程票终于不再只是规则，而像一张要求你们互相确认的纸。",
        ),
        CompanionTalkId::TicketOfficePriceQuestion => (
            "同行对话：孩子和明天的价格",
            "孩子问售票员：明天要多少钱？玻璃后的绿灯闪了两下。售票员说，明天不卖，只退还被错误抵押的东西。孩子想了很久，说那我不要大人把自己赔给我。你站在旁边，忽然明白他问的不是价格，是你会不会又把补偿装扮成爱。",
        ),
        CompanionTalkId::LostFoundNamedBox => (
            "同行对话：孩子和正确的格子",
            "失物架上那只写着“歉意”的箱子仍然半开。孩子把姓名牌推到“姓名”那格下面，推得很认真。他说：如果放错地方，别人会不会以为我只是你的难过？你说不会了。他没有立刻相信，但这一次，标签至少站在他那边。",
        ),
        CompanionTalkId::LostFoundRaincoatSleeve => (
            "同行对话：孩子和雨衣袖口",
            "你们翻开那件小雨衣的袖口，里面有一截松开的蓝线。孩子捏着线头，说那天我是不是穿这个？你说是，但没有把后面的话说成审判。他把线绕在指尖，很小声地说：原来我不是只存在于你想起来的时候，我还有衣服、线头和冷。",
        ),
        CompanionTalkId::UnderpassEchoStep => (
            "同行对话：孩子和同步回声",
            "你们在地下通道同时迈出一步。回声没有抢先，也没有落后，像终于学会礼貌地跟随。孩子停住，问是不是代表这里可以重新开始。你说不代表，它只代表这一步没有再把谁落下。他点点头，说一步也可以，比很多保证听起来都稳。",
        ),
        CompanionTalkId::UnderpassStairsTomorrow => (
            "同行对话：孩子和通往明天的台阶",
            "你们数台阶，从一数到十五，又从十五数回一。孩子说，如果明天只是厨房和热牛奶，那它会不会太小？你说可能很小，小到没有任何广播愿意报道。他笑了一下：那就好。太大的明天听起来像又要我懂事。",
        ),
        CompanionTalkId::ClockTowerBorrowedMinute => (
            "同行对话：孩子和借来的分钟",
            "钟楼齿轮在你们头顶缓慢咬合。你告诉孩子，午夜是借来的最后一分钟。他问借来的东西是不是一定要还。你说是，但不能把人一起还掉。站务员在楼梯口沉默，像这句话同时放过了他，也给他划下边界。",
        ),
        CompanionTalkId::ClockTowerRulesForLight => (
            "同行对话：孩子和雾灯规矩",
            "你把值夜簿翻开，孩子用很慢的笔画写下第二条规矩：照路的人不可以要求被喜欢。他写完以后有些不好意思，问这样会不会太凶。你说不会，这条规矩很温柔，因为它先保护被照亮的人。雾灯在窗外轻轻亮了一下。",
        ),
        CompanionTalkId::PlatformWhiteLineTogether => (
            "同行对话：孩子和白线内侧",
            "你们并肩站在白线内侧。孩子看着脚尖，说以前我站在线后面，是因为只剩规则会陪我。现在规则还在，但你也在。他没有把手递过来，只把肩膀放松了一点。那一点比牵手更难，也更像真正的同意。",
        ),
        CompanionTalkId::PlatformDoorQuestion => (
            "同行对话：孩子和广播室门",
            "雾灯照出月台远端的广播室门。孩子问，如果你进去说话，我还能听见你吗？你说也许能，但听见不等于我还在你身边。他看着门牌，过了很久才说：那你说警告的时候，要把这个区别也说进去。别让后来的人把声音误认成陪伴。",
        ),
    };
    StoryEvent::new(title, body)
        .tag("同行对话")
        .tag("自由对话")
        .tag("自由探索")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let completed = state.completed_companion_talks.len();
    if completed == 0 {
        return None;
    }

    Some(format!(
        "你完成了 {completed} 段同行对话。孩子不再只是终局条件，而是在车站各处留下自己的问题；这些问题让离开更像两个人共同确认过的路。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn companion_summaries_track_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = companion_summaries(&state);
        let hall = initial
            .iter()
            .find(|summary| summary.talk == CompanionTalkId::WaitingHallEmptySeat)
            .expect("waiting hall companion talk should be listed");
        assert_eq!(hall.status, "未显形");
        assert_eq!(hall.progress, 0);

        state.remember(Flag::MetChild);
        state.child_trust = 4;
        state.visit_memory(MemoryId::SeventhBench);
        let partial = companion_summaries(&state);
        let hall = partial
            .iter()
            .find(|summary| summary.talk == CompanionTalkId::WaitingHallEmptySeat)
            .expect("waiting hall companion talk should be listed");
        assert_eq!(hall.status, "待同行");
        assert!(hall.visible);
        assert!(hall.progress > 0);

        state.remember(Flag::ChildJoined);
        state.discuss(TopicId::TravelerRain);
        let ready = companion_summaries(&state);
        let hall = ready
            .iter()
            .find(|summary| summary.talk == CompanionTalkId::WaitingHallEmptySeat)
            .expect("waiting hall companion talk should be listed");
        assert_eq!(hall.status, "可交谈");
        assert!(hall.ready);

        let event = talk(&mut state, CompanionTalkId::WaitingHallEmptySeat);
        assert!(event.tags.iter().any(|tag| tag == "同行对话"));
        assert!(event.tags.iter().any(|tag| tag == "自由探索"));
        assert!(state.has_completed_companion_talk(CompanionTalkId::WaitingHallEmptySeat));
        assert!(state.has_flag(Flag::TravelerTrusted));
    }

    #[test]
    fn companion_count_gives_each_location_two_talks() {
        assert_eq!(COMPANION_TALK_COUNT, CompanionTalkId::ALL.len());
        assert_eq!(COMPANION_TALK_COUNT, Location::ALL.len() * 2);
    }
}
