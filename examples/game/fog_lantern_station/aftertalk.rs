use crate::model::{
    AftertalkId, DepartureId, EvidenceId, Flag, GameState, Item, Location, MemoryId, PatrolId,
    StoryEvent, TopicId,
};

pub const AFTERTALK_COUNT: usize = 9;

#[derive(Clone, Debug)]
pub struct AftertalkAction {
    pub aftertalk: AftertalkId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AftertalkSummary {
    pub aftertalk: AftertalkId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub ready: bool,
}

pub fn available_aftertalks(state: &GameState) -> Vec<AftertalkAction> {
    AftertalkId::ALL
        .iter()
        .copied()
        .filter(|aftertalk| !state.has_completed_aftertalk(*aftertalk))
        .filter(|aftertalk| aftertalk.location() == state.location)
        .filter(|aftertalk| aftertalk_visible(state, *aftertalk))
        .map(|aftertalk| {
            let missing = missing_requirements(state, aftertalk);
            AftertalkAction {
                aftertalk,
                label: aftertalk.label(),
                detail: if missing.is_empty() {
                    "你做过的事已经改变了这场谈话。现在可以把问题带回去。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn aftertalk_summaries(state: &GameState) -> Vec<AftertalkSummary> {
    AftertalkId::ALL
        .iter()
        .copied()
        .map(|aftertalk| {
            let completed = state.has_completed_aftertalk(aftertalk);
            let visible = completed || aftertalk_visible(state, aftertalk);
            let missing = missing_requirements(state, aftertalk);
            let ready = visible && missing.is_empty() && !completed;
            let progress = aftertalk_progress(aftertalk, missing.len(), visible, completed);
            let status = if completed {
                "已回应"
            } else if ready {
                "可追问"
            } else if visible {
                "待触发"
            } else {
                "未显形"
            };
            let detail = if completed {
                aftertalk.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以继续。前往{}，把行动的余波带回谈话。",
                    aftertalk.title(),
                    aftertalk.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这段回访对话还藏在{}。继续巡夜、进入记忆、准备路线或完成自由追问，它才会显形。",
                    aftertalk.location().title()
                )
            };

            AftertalkSummary {
                aftertalk,
                title: aftertalk.title(),
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

pub fn follow_up(state: &mut GameState, aftertalk: AftertalkId) -> StoryEvent {
    if state.has_completed_aftertalk(aftertalk) {
        return StoryEvent::new(
            "这段话已经被接住",
            "你又把同一个回访问题放到桌面上。对方没有厌烦，只是把它轻轻推回来：这句话已经变成行动，下一句要去别处找。",
        )
        .tag("回访对话");
    }

    let missing = missing_requirements(state, aftertalk);
    if aftertalk.location() != state.location
        || !aftertalk_visible(state, aftertalk)
        || !missing.is_empty()
    {
        return StoryEvent::new(
            "这段回访还没有入口",
            format!(
                "你已经感觉到新的谈话在附近，但它还缺少能落脚的事实。{}",
                if aftertalk.location() != state.location {
                    format!("它不在这里，而在{}。", aftertalk.location().title())
                } else if missing.is_empty() {
                    "这段话还没有被今晚承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("回访对话");
    }

    state.complete_aftertalk(aftertalk);
    let mut event = aftertalk_event(aftertalk);
    apply_aftertalk_rewards(state, aftertalk, &mut event);
    event
}

impl AftertalkId {
    pub const ALL: [Self; AFTERTALK_COUNT] = [
        Self::TravelerSecondSeat,
        Self::TravelerPatrolManifest,
        Self::ClerkRefundQueue,
        Self::LostFoundNamedShelf,
        Self::UnderpassMeasuredEcho,
        Self::ChildRedrawnLine,
        Self::ChildDepartureSeat,
        Self::KeeperMinuteHand,
        Self::KeeperBroadcastReply,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::TravelerSecondSeat | Self::TravelerPatrolManifest => Location::WaitingHall,
            Self::ClerkRefundQueue => Location::TicketOffice,
            Self::LostFoundNamedShelf => Location::LostAndFound,
            Self::UnderpassMeasuredEcho => Location::Underpass,
            Self::ChildRedrawnLine | Self::ChildDepartureSeat => Location::Platform,
            Self::KeeperMinuteHand | Self::KeeperBroadcastReply => Location::ClockTower,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::TravelerSecondSeat => "回访老人：第七张长椅为什么空着",
            Self::TravelerPatrolManifest => "回访老人：把清点记录给他看",
            Self::ClerkRefundQueue => "回访售票员：退票队列里的同行栏",
            Self::LostFoundNamedShelf => "回访失物架：把姓名牌放回正确标签",
            Self::UnderpassMeasuredEcho => "回访回声：两个水线高度",
            Self::ChildRedrawnLine => "回访孩子：新描过的白线",
            Self::ChildDepartureSeat => "回访孩子：07B 靠窗座位",
            Self::KeeperMinuteHand => "回访站务员：分针背面的字",
            Self::KeeperBroadcastReply => "回访站务员：誊清后的广播稿",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::TravelerSecondSeat => "老人和第二个座位",
            Self::TravelerPatrolManifest => "老人和清点记录",
            Self::ClerkRefundQueue => "售票员和同行栏",
            Self::LostFoundNamedShelf => "失物架和正确标签",
            Self::UnderpassMeasuredEcho => "回声和两个高度",
            Self::ChildRedrawnLine => "孩子和新白线",
            Self::ChildDepartureSeat => "孩子和 07B",
            Self::KeeperMinuteHand => "站务员和分针背面",
            Self::KeeperBroadcastReply => "站务员和广播稿",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::TravelerSecondSeat => "老人承认空座不是摆设，是你迟早要负责的第二个人。",
            Self::TravelerPatrolManifest => {
                "老人把清点记录收进报纸，像替你保管一份不再逃避的证词。"
            }
            Self::ClerkRefundQueue => "售票员承认同行栏不是优惠，而是返程票最硬的条件。",
            Self::LostFoundNamedShelf => "失物架终于把姓名牌从歉意里分出来，还给一个具体的人。",
            Self::UnderpassMeasuredEcho => {
                "回声用两个高度回答你，旧案从此不再只是午夜里一句含糊的话。"
            }
            Self::ChildRedrawnLine => "孩子看见新描过的白线，知道你这次没有把等待只留给他。",
            Self::ChildDepartureSeat => "孩子第一次把 07B 当作座位，而不是又一张迟来的保证。",
            Self::KeeperMinuteHand => "站务员承认最后一分钟不是他的慈悲，而是需要归还的债。",
            Self::KeeperBroadcastReply => "站务员听完广播稿，知道警告终于包含了第二个名字。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::TravelerSecondSeat
            | Self::TravelerPatrolManifest
            | Self::ClerkRefundQueue
            | Self::LostFoundNamedShelf
            | Self::UnderpassMeasuredEcho
            | Self::ChildRedrawnLine
            | Self::ChildDepartureSeat
            | Self::KeeperMinuteHand
            | Self::KeeperBroadcastReply => 3,
        }
    }
}

fn aftertalk_visible(state: &GameState, aftertalk: AftertalkId) -> bool {
    match aftertalk {
        AftertalkId::TravelerSecondSeat => {
            state.has_memory(MemoryId::SeventhBench)
                || state.has_completed_patrol(PatrolId::WaitingHallManifest)
        }
        AftertalkId::TravelerPatrolManifest => {
            state.has_completed_patrol(PatrolId::WaitingHallManifest)
        }
        AftertalkId::ClerkRefundQueue => {
            state.has_completed_patrol(PatrolId::TicketWindowQueue)
                || state.has_memory(MemoryId::TicketWindowReflection)
        }
        AftertalkId::LostFoundNamedShelf => {
            state.has_completed_patrol(PatrolId::LostFoundShelfAudit)
                || state.has_memory(MemoryId::RaincoatPocket)
        }
        AftertalkId::UnderpassMeasuredEcho => {
            state.has_completed_patrol(PatrolId::UnderpassWaterline)
                || state.has_memory(MemoryId::EvacuationLine)
        }
        AftertalkId::ChildRedrawnLine => {
            state.has_completed_patrol(PatrolId::PlatformBoundary)
                || state.has_memory(MemoryId::WhiteLineMeasure)
        }
        AftertalkId::ChildDepartureSeat => {
            state.has_prepared_departure(DepartureId::ChildWindowSeat)
                || state.has_memory(MemoryId::OrdinaryKitchen)
        }
        AftertalkId::KeeperMinuteHand => {
            state.has_completed_patrol(PatrolId::ClockTowerMinuteHand)
                || state.has_memory(MemoryId::BorrowedClockMinute)
        }
        AftertalkId::KeeperBroadcastReply => {
            state.has_prepared_departure(DepartureId::BroadcastScript)
                || state.has_memory(MemoryId::BroadcastPractice)
        }
    }
}

fn missing_requirements(state: &GameState, aftertalk: AftertalkId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match aftertalk {
        AftertalkId::TravelerSecondSeat => {
            require(
                &mut missing,
                state.has_memory(MemoryId::SeventhBench)
                    || state.has_completed_patrol(PatrolId::WaitingHallManifest),
                "走过第七张长椅记忆或完成候车厅巡夜",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain)
                    || state.has_discussed(TopicId::TravelerTicket),
                "和老人谈过雨夜或湿票",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ExaminedTicket) || state.has_item(Item::MirrorShard),
                "看清湿票或取得镜片",
            );
        }
        AftertalkId::TravelerPatrolManifest => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::WaitingHallManifest),
                "完成候车厅清点巡夜",
            );
            require(
                &mut missing,
                state.has_flag(Flag::TravelerTrusted) || state.traveler_depth >= 1,
                "让老人愿意信任你",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
        }
        AftertalkId::ClerkRefundQueue => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::TicketWindowQueue)
                    || state.has_memory(MemoryId::TicketWindowReflection),
                "排查退票队列或走过窗口记忆",
            );
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkSeats)
                    || state.has_flag(Flag::TicketRewritten)
                    || state.ticket.name().contains("返程"),
                "追问过两个座位或改签返程票",
            );
        }
        AftertalkId::LostFoundNamedShelf => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::LostFoundShelfAudit)
                    || state.has_memory(MemoryId::RaincoatPocket),
                "复核失物标签或走过雨衣口袋记忆",
            );
            require(
                &mut missing,
                state.has_item(Item::NameTag) || state.has_flag(Flag::ReturnedNameTag),
                "找到或归还姓名牌",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::LostFoundLabels)
                    || state.has_discussed(TopicId::LostFoundNameTag),
                "追问过失物标签或姓名牌",
            );
        }
        AftertalkId::UnderpassMeasuredEcho => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::UnderpassWaterline)
                    || state.has_memory(MemoryId::EvacuationLine),
                "量过地下通道水线或走过疏散记忆",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::UnderpassEcho)
                    || state.has_discussed(TopicId::UnderpassWaterline),
                "追问过回声或水线",
            );
        }
        AftertalkId::ChildRedrawnLine => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::PlatformBoundary)
                    || state.has_memory(MemoryId::WhiteLineMeasure),
                "重描月台白线或走过白线刻度记忆",
            );
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.child_trust >= 3
                    || state.has_discussed(TopicId::ChildWhiteLine)
                    || state.has_presented(EvidenceId::ChildHomework),
                "让孩子信任你，或谈过白线/作业本",
            );
        }
        AftertalkId::ChildDepartureSeat => {
            require(
                &mut missing,
                state.has_prepared_departure(DepartureId::ChildWindowSeat)
                    || state.has_memory(MemoryId::OrdinaryKitchen),
                "保留 07B 座位或走过普通清晨记忆",
            );
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined)
                    || state.child_trust >= 4
                    || state.has_discussed(TopicId::ChildLeaving),
                "让孩子愿意同行，或把信任推进到离开的话题",
            );
        }
        AftertalkId::KeeperMinuteHand => {
            require(
                &mut missing,
                state.has_completed_patrol(PatrolId::ClockTowerMinuteHand)
                    || state.has_memory(MemoryId::BorrowedClockMinute),
                "擦亮分针背面或走过借来的最后一分钟",
            );
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth) || state.keeper_depth >= 1,
                "听站务员说过旧钟代价",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.has_item(Item::OldTimetable),
                "取得站务日志或旧时刻表",
            );
        }
        AftertalkId::KeeperBroadcastReply => {
            require(
                &mut missing,
                state.has_prepared_departure(DepartureId::BroadcastScript)
                    || state.has_memory(MemoryId::BroadcastPractice),
                "誊清广播稿或走过广播练习室记忆",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得或听过广播磁带",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp)
                    || state.has_discussed(TopicId::KeeperBroadcast),
                "修复雾灯或追问广播室",
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

fn aftertalk_progress(
    aftertalk: AftertalkId,
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
    let total = aftertalk.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_aftertalk_rewards(state: &mut GameState, aftertalk: AftertalkId, event: &mut StoryEvent) {
    match aftertalk {
        AftertalkId::TravelerSecondSeat | AftertalkId::TravelerPatrolManifest => {
            remember_tag(state, event, Flag::TravelerTrusted, "老人信任");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("回访：老人".to_string());
        }
        AftertalkId::ClerkRefundQueue => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            remember_tag(state, event, Flag::SynthesizedRoute, "合成：同行栏");
            event.tags.push("回访：售票窗口".to_string());
        }
        AftertalkId::LostFoundNamedShelf => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "姓名牌复核");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("回访：失物招领".to_string());
        }
        AftertalkId::UnderpassMeasuredEcho => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "回声复核");
            remember_tag(state, event, Flag::UnderstoodStationMechanism, "水线复核");
            event.tags.push("回访：地下通道".to_string());
        }
        AftertalkId::ChildRedrawnLine | AftertalkId::ChildDepartureSeat => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "孩子真相");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("回访：孩子".to_string());
        }
        AftertalkId::KeeperMinuteHand => {
            remember_tag(state, event, Flag::UnderstoodStationMechanism, "旧钟复核");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("回访：旧钟楼".to_string());
        }
        AftertalkId::KeeperBroadcastReply => {
            remember_tag(state, event, Flag::SynthesizedStationTruth, "广播稿复核");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("回访：广播室".to_string());
        }
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn aftertalk_event(aftertalk: AftertalkId) -> StoryEvent {
    let (title, body) = match aftertalk {
        AftertalkId::TravelerSecondSeat => (
            "回访对话：老人和第二个座位",
            "你把第七张长椅的空座说给老人听。老人这次没有翻报纸，只问：现在你还觉得空座只是提醒你愧疚吗？他用指节敲了敲椅面，声音很轻。空着的座位不需要你跪在旁边，它需要你承认有人本来应该坐在那里，并且他有权决定还坐不坐回来。",
        ),
        AftertalkId::TravelerPatrolManifest => (
            "回访对话：老人和清点记录",
            "你把候车厅清点记录递过去。老人把它夹进报纸，像夹一张不会被雨泡坏的车票。他说：你终于不是来问我该不该走，而是来证明这里发生过什么。人能离开一座车站，但不能让车站替自己删掉见证。",
        ),
        AftertalkId::ClerkRefundQueue => (
            "回访对话：售票员和同行栏",
            "你把退票队列里的同行栏指给售票员看。她沉默很久，说这栏一直在，只是多数人急着签自己的名字，没发现旁边还有一格。她把票章拿起来，又放下：返程不是买两张票，是承认你抵达终点时仍会被另一个人的重量改变。",
        ),
        AftertalkId::LostFoundNamedShelf => (
            "回访对话：失物架和正确标签",
            "你把姓名牌从“歉意”那一格挪出来，放到“姓名”下面。失物架发出木头受潮的轻响，像有人终于松了一口气。你忽然明白，很多伤害之所以难以归还，是因为幸存者把人的东西错贴成自己的情绪，越保管越像占有。",
        ),
        AftertalkId::UnderpassMeasuredEcho => (
            "回访对话：回声和两个高度",
            "你对着地下通道说出两个水线高度。回声没有重复你的姓名，而是重复“肩膀”和“指节”。它第一次不再像谜语，倒像一份验伤记录。原来记忆变可靠的方式，不是变得宏大，而是变得可以被量、被指认、被另一个人检查。",
        ),
        AftertalkId::ChildRedrawnLine => (
            "回访对话：孩子和新白线",
            "你告诉孩子白线已经重新描过。他蹲下去看，手指悬在白漆上方，没有碰。他说：那以后如果我站在这里，就不是因为我还在等你回来救我。你说，是因为你愿意站在你自己觉得安全的地方。他点点头，这次点得很慢。",
        ),
        AftertalkId::ChildDepartureSeat => (
            "回访对话：孩子和 07B",
            "你说 07B 靠窗座位已经留好。孩子先问：靠窗是不是能看见明天？你说不一定，也可能只看见隧道。他想了想，说那也可以，至少这次你没有说一定会好。他把作业本抱紧一点：不保证的座位，听起来反而像真的。",
        ),
        AftertalkId::KeeperMinuteHand => (
            "回访对话：站务员和分针背面",
            "你把分针背面的字念给站务员听：借出一分钟，归还一个明天。他闭上眼，像被这句话从职业里叫回人间。他说我守了太久，差点忘了记录不是为了证明我尽责，而是为了防止我把别人的未来当作自己可以管理的库存。",
        ),
        AftertalkId::KeeperBroadcastReply => (
            "回访对话：站务员和广播稿",
            "你把誊清后的广播稿放在钟楼桌上。站务员读到第二个名字时停了一下，但这次没有跳过去。他说：警告若只会说别上车，就像把门关上。你这份稿子至少说出了为什么、为了谁、还有谁可以选择不被留下。",
        ),
    };
    StoryEvent::new(title, body).tag("回访对话").tag("自由对话")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let completed = state.completed_aftertalks.len();
    if completed == 0 {
        return None;
    }

    Some(format!(
        "你完成了 {completed} 段回访对话。那些对话不是新线索，而是你做过的事被人重新回应，于是选择不再孤零零地落下。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aftertalk_summaries_track_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = aftertalk_summaries(&state);
        let traveler = initial
            .iter()
            .find(|summary| summary.aftertalk == AftertalkId::TravelerSecondSeat)
            .expect("traveler aftertalk should be listed");
        assert_eq!(traveler.status, "未显形");
        assert_eq!(traveler.progress, 0);

        state.visit_memory(MemoryId::SeventhBench);
        state.remember(Flag::ExaminedTicket);
        let partial = aftertalk_summaries(&state);
        let traveler = partial
            .iter()
            .find(|summary| summary.aftertalk == AftertalkId::TravelerSecondSeat)
            .expect("traveler aftertalk should be listed");
        assert_eq!(traveler.status, "待触发");
        assert!(traveler.visible);
        assert!(traveler.progress > 0);

        state.discuss(TopicId::TravelerRain);
        let ready = aftertalk_summaries(&state);
        let traveler = ready
            .iter()
            .find(|summary| summary.aftertalk == AftertalkId::TravelerSecondSeat)
            .expect("traveler aftertalk should be listed");
        assert_eq!(traveler.status, "可追问");
        assert!(traveler.ready);

        let event = follow_up(&mut state, AftertalkId::TravelerSecondSeat);
        assert!(event.tags.iter().any(|tag| tag == "自由对话"));
        assert!(state.has_completed_aftertalk(AftertalkId::TravelerSecondSeat));
        assert!(state.has_flag(Flag::TravelerTrusted));
    }

    #[test]
    fn aftertalk_count_exceeds_core_character_threads() {
        assert_eq!(AFTERTALK_COUNT, AftertalkId::ALL.len());
        assert!(AFTERTALK_COUNT > 4);
    }
}
