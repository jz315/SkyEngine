use crate::model::{
    EvidenceId, Flag, GameState, Item, Location, MemoryId, PatrolId, StoryEvent, TopicId,
};

pub const PATROL_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct PatrolAction {
    pub patrol: PatrolId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatrolSummary {
    pub patrol: PatrolId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub ready: bool,
}

pub fn available_patrols(state: &GameState) -> Vec<PatrolAction> {
    PatrolId::ALL
        .iter()
        .copied()
        .filter(|patrol| !state.has_completed_patrol(*patrol))
        .filter(|patrol| patrol.location() == state.location)
        .filter(|patrol| patrol_visible(state, *patrol))
        .map(|patrol| {
            let missing = missing_requirements(state, patrol);
            PatrolAction {
                patrol,
                label: patrol.label(),
                detail: if missing.is_empty() {
                    "这处地点的夜巡可以开始。它会把零散证词钉回具体空间。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn patrol_summaries(state: &GameState) -> Vec<PatrolSummary> {
    PatrolId::ALL
        .iter()
        .copied()
        .map(|patrol| {
            let completed = state.has_completed_patrol(patrol);
            let visible = completed || patrol_visible(state, patrol);
            let missing = missing_requirements(state, patrol);
            let ready = visible && missing.is_empty() && !completed;
            let progress = patrol_progress(patrol, missing.len(), visible, completed);
            let status = if completed {
                "已记录"
            } else if ready {
                "可巡夜"
            } else if visible {
                "待补证"
            } else {
                "未显形"
            };
            let detail = if completed {
                patrol.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以巡夜。前往{}，把这处地点重新走一遍。",
                    patrol.title(),
                    patrol.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这份巡夜记录还藏在{}。继续调查地点、追问人物或出示证据，它才会出现。",
                    patrol.location().title()
                )
            };

            PatrolSummary {
                patrol,
                title: patrol.title(),
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

pub fn take(state: &mut GameState, patrol: PatrolId) -> StoryEvent {
    if state.has_completed_patrol(patrol) {
        return StoryEvent::new(
            "巡夜记录已经写好",
            "这段路线已经被你走进日志里。再次经过时，墙上的水痕只是轻轻亮了一下，像在确认你没有把它删掉。",
        )
        .tag("巡夜记录");
    }

    let missing = missing_requirements(state, patrol);
    if patrol.location() != state.location || !patrol_visible(state, patrol) || !missing.is_empty()
    {
        return StoryEvent::new(
            "巡夜还不能开始",
            format!(
                "这里有一份记录等着被补完，但今晚还没有把路让出来。{}",
                if patrol.location() != state.location {
                    format!("它不在这里，而在{}。", patrol.location().title())
                } else if missing.is_empty() {
                    "这份巡夜记录还没有被车站承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("巡夜记录");
    }

    state.complete_patrol(patrol);
    let mut event = patrol_event(patrol);
    apply_patrol_rewards(state, patrol, &mut event);
    event
}

impl PatrolId {
    pub const ALL: [Self; PATROL_COUNT] = [
        Self::WaitingHallManifest,
        Self::TicketWindowQueue,
        Self::LostFoundShelfAudit,
        Self::UnderpassWaterline,
        Self::ClockTowerMinuteHand,
        Self::PlatformBoundary,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::WaitingHallManifest => Location::WaitingHall,
            Self::TicketWindowQueue => Location::TicketOffice,
            Self::LostFoundShelfAudit => Location::LostAndFound,
            Self::UnderpassWaterline => Location::Underpass,
            Self::ClockTowerMinuteHand => Location::ClockTower,
            Self::PlatformBoundary => Location::Platform,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingHallManifest => "巡夜记录：清点第七排长椅",
            Self::TicketWindowQueue => "巡夜记录：排查退票队列",
            Self::LostFoundShelfAudit => "巡夜记录：重贴失物标签",
            Self::UnderpassWaterline => "巡夜记录：量地下通道水线",
            Self::ClockTowerMinuteHand => "巡夜记录：擦亮分针背面",
            Self::PlatformBoundary => "巡夜记录：重描月台白线",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WaitingHallManifest => "第七排长椅清点",
            Self::TicketWindowQueue => "退票队列排查",
            Self::LostFoundShelfAudit => "失物标签复核",
            Self::UnderpassWaterline => "地下通道水线",
            Self::ClockTowerMinuteHand => "旧钟分针背面",
            Self::PlatformBoundary => "月台白线重描",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WaitingHallManifest => "候车厅长椅被重新清点，老人终于不必独自守着那张湿票。",
            Self::TicketWindowQueue => "售票窗口的退票队列被排查，单程票不再能假装只是业务。",
            Self::LostFoundShelfAudit => "失物标签被重贴，姓名牌和雨衣不再被归成同一种沉默。",
            Self::UnderpassWaterline => "地下通道的水线被量过，回声从此有了可以对照的高度。",
            Self::ClockTowerMinuteHand => "旧钟分针背面被擦亮，借来的最后一分钟多了一条可查记录。",
            Self::PlatformBoundary => "月台白线被重新描过，等待不再只是孩子一个人的纪律。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::WaitingHallManifest
            | Self::TicketWindowQueue
            | Self::LostFoundShelfAudit
            | Self::UnderpassWaterline
            | Self::ClockTowerMinuteHand
            | Self::PlatformBoundary => 3,
        }
    }
}

fn patrol_visible(state: &GameState, patrol: PatrolId) -> bool {
    match patrol {
        PatrolId::WaitingHallManifest => {
            state.has_flag(Flag::ReadDepartureBoard)
                || state.investigation_depth(Location::WaitingHall) >= 2
                || state.has_memory(MemoryId::SeventhBench)
        }
        PatrolId::TicketWindowQueue => {
            state.has_item(Item::CoinToken)
                || state.clerk_depth >= 2
                || state.has_flag(Flag::TicketRewritten)
        }
        PatrolId::LostFoundShelfAudit => {
            state.has_flag(Flag::SearchedLostFound)
                || state.has_flag(Flag::OpenedCabinet)
                || state.investigation_depth(Location::LostAndFound) >= 2
        }
        PatrolId::UnderpassWaterline => {
            state.has_flag(Flag::HeardUnderpassEcho)
                || state.has_flag(Flag::RecoveredName)
                || state.investigation_depth(Location::Underpass) >= 2
        }
        PatrolId::ClockTowerMinuteHand => {
            state.has_flag(Flag::HeardClockTruth)
                || state.has_item(Item::StationLog)
                || state.investigation_depth(Location::ClockTower) >= 2
        }
        PatrolId::PlatformBoundary => {
            state.has_flag(Flag::MetChild)
                || state.has_flag(Flag::InspectedRails)
                || state.investigation_depth(Location::Platform) >= 1
        }
    }
}

fn missing_requirements(state: &GameState, patrol: PatrolId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match patrol {
        PatrolId::WaitingHallManifest => {
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
            require(
                &mut missing,
                state.has_item(Item::MirrorShard) || state.has_memory(MemoryId::SeventhBench),
                "取得候车厅镜片或走过第七张长椅记忆",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain) || state.traveler_depth >= 1,
                "让老人谈过雨夜",
            );
        }
        PatrolId::TicketWindowQueue => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.clerk_depth >= 2 || state.has_discussed(TopicId::ClerkOneWay),
                "听售票员讲过单程票",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.has_flag(Flag::ReadStationLog),
                "取得或读过站务日志",
            );
        }
        PatrolId::LostFoundShelfAudit => {
            require(
                &mut missing,
                state.has_flag(Flag::SearchedLostFound)
                    || state.investigation_depth(Location::LostAndFound) >= 2,
                "翻过失物箱或深入调查失物招领处",
            );
            require(
                &mut missing,
                state.has_item(Item::NameTag) || state.has_flag(Flag::ReturnedNameTag),
                "找到或归还姓名牌",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::LostFoundLabels)
                    || state.has_flag(Flag::OpenedCabinet),
                "追问失物标签或打开铁柜",
            );
        }
        PatrolId::UnderpassWaterline => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardUnderpassEcho)
                    || state.investigation_depth(Location::Underpass) >= 2,
                "听过地下通道回声或量过前两层水痕",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp) || state.has_item(Item::LanternGlass),
                "修复雾灯或带着雾灯玻璃",
            );
        }
        PatrolId::ClockTowerMinuteHand => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth) || state.keeper_depth >= 1,
                "听站务员说过旧钟代价",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog),
                "取得站务日志",
            );
            require(
                &mut missing,
                state.has_item(Item::OldTimetable) || state.has_discussed(TopicId::KeeperTimetable),
                "取得旧时刻表或追问时刻表",
            );
        }
        PatrolId::PlatformBoundary => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_flag(Flag::InspectedRails)
                    || state.investigation_depth(Location::Platform) >= 2,
                "检查轨道或调查月台白线",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework)
                    || state.has_discussed(TopicId::ChildWhiteLine)
                    || state.has_presented(EvidenceId::ChildHomework),
                "看过作业本或追问白线",
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

fn patrol_progress(patrol: PatrolId, missing_count: usize, visible: bool, completed: bool) -> u8 {
    if completed {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = patrol.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_patrol_rewards(state: &mut GameState, patrol: PatrolId, event: &mut StoryEvent) {
    match patrol {
        PatrolId::WaitingHallManifest => {
            remember_tag(state, event, Flag::TravelerTrusted, "老人信任");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("巡夜：候车厅".to_string());
        }
        PatrolId::TicketWindowQueue => {
            remember_tag(state, event, Flag::ReadStationLog, "读过站务日志");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("巡夜：售票窗口".to_string());
        }
        PatrolId::LostFoundShelfAudit => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "姓名牌复核");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("巡夜：失物招领".to_string());
        }
        PatrolId::UnderpassWaterline => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "水线记录");
            remember_tag(state, event, Flag::UnderstoodStationMechanism, "通道结构");
            event.tags.push("巡夜：地下通道".to_string());
        }
        PatrolId::ClockTowerMinuteHand => {
            remember_tag(state, event, Flag::UnderstoodStationMechanism, "旧钟记录");
            if state.has_item(Item::OldTimetable)
                && state.has_flag(Flag::RepairedFogLamp)
                && state.add_item(Item::SignalWhistle)
            {
                event
                    .tags
                    .push(format!("获得：{}", Item::SignalWhistle.name()));
            }
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("巡夜：旧钟楼".to_string());
        }
        PatrolId::PlatformBoundary => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "白线记录");
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("巡夜：三号月台".to_string());
        }
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn patrol_event(patrol: PatrolId) -> StoryEvent {
    let (title, body) = match patrol {
        PatrolId::WaitingHallManifest => (
            "巡夜记录：第七排长椅清点",
            "你从第一排长椅数到第七排，把每一道潮湿坐痕和报纸折角记进站内图。老人没有抬头，却把报纸往旁边挪了一寸，露出被压住的空座。那一寸像某种允许：你可以承认这里曾经等过两个人，而不是只等过一个更容易被原谅的人。",
        ),
        PatrolId::TicketWindowQueue => (
            "巡夜记录：退票队列排查",
            "售票窗口后的灯变成纸一样薄。你按铜筹编号核对退票队列，发现每一张单程票旁边都空着一个可以被手写补上的同行栏。售票员隔着玻璃看你，没有催促。她第一次像在等你办完一件正确但麻烦的小事。",
        ),
        PatrolId::LostFoundShelfAudit => (
            "巡夜记录：失物标签复核",
            "你把失物架上褪色的标签一张张揭下，又重新贴好：雨衣是雨衣，姓名牌是姓名牌，没来得及说出口的话也不能和歉意塞进同一个箱子。铁柜深处轻轻响了一声，像有人终于被从分类错误里放出来。",
        ),
        PatrolId::UnderpassWaterline => (
            "巡夜记录：地下通道水线",
            "你用雾灯玻璃的边沿去量墙砖上的水线。最深的一道正好到孩子肩膀，最浅的一道停在你的指节。回声这次没有抢先回答，只把两个高度重复给你听，像提醒你：旧案不是一个夜晚的代称，它有具体的身高和温度。",
        ),
        PatrolId::ClockTowerMinuteHand => (
            "巡夜记录：旧钟分针背面",
            "你爬上钟楼内侧，把分针背面的油污一点点擦掉。金属下面刻着很多小字：借出一分钟，归还一个明天。站务员站在楼梯口，忽然很疲惫地说：我一直怕你看见这行字，因为看见以后，留下和离开都不再像借口。",
        ),
        PatrolId::PlatformBoundary => (
            "巡夜记录：月台白线重描",
            "你蹲在三号月台边缘，把白线磨掉的地方重新描亮。孩子站在你身后，没有越线，也没有退后。你们都看见那条线不是为了惩罚等待的人，而是为了让后来的人知道：这里曾有人按约定站着，直到约定本身亏欠了他。",
        ),
    };
    StoryEvent::new(title, body).tag("巡夜记录").tag("自由探索")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let completed = state.completed_patrols.len();
    if completed == 0 {
        return None;
    }

    Some(format!(
        "你完成了 {completed} 份巡夜记录。车站因此不再只是一张谜题地图，而像一个被人重新走过、重新登记过的地方。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patrol_summaries_track_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = patrol_summaries(&state);
        let hall = initial
            .iter()
            .find(|summary| summary.patrol == PatrolId::WaitingHallManifest)
            .expect("waiting hall patrol should be listed");
        assert_eq!(hall.status, "未显形");
        assert_eq!(hall.progress, 0);

        state.remember(Flag::ReadDepartureBoard);
        state.add_item(Item::MirrorShard);
        let partial = patrol_summaries(&state);
        let hall = partial
            .iter()
            .find(|summary| summary.patrol == PatrolId::WaitingHallManifest)
            .expect("waiting hall patrol should be listed");
        assert_eq!(hall.status, "待补证");
        assert!(hall.visible);
        assert!(hall.progress > 0);

        state.discuss(TopicId::TravelerRain);
        let ready = patrol_summaries(&state);
        let hall = ready
            .iter()
            .find(|summary| summary.patrol == PatrolId::WaitingHallManifest)
            .expect("waiting hall patrol should be listed");
        assert_eq!(hall.status, "可巡夜");
        assert!(hall.ready);

        let event = take(&mut state, PatrolId::WaitingHallManifest);
        assert!(event.tags.iter().any(|tag| tag == "自由探索"));
        assert!(state.has_completed_patrol(PatrolId::WaitingHallManifest));
        assert!(state.has_flag(Flag::TravelerTrusted));
    }

    #[test]
    fn patrol_count_matches_station_locations() {
        assert_eq!(PATROL_COUNT, PatrolId::ALL.len());
        assert_eq!(PATROL_COUNT, Location::ALL.len());
    }
}
