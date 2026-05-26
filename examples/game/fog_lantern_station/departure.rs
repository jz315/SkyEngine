use crate::model::{
    DepartureId, Ending, Flag, GameState, Item, Location, MemoryId, ResonanceId, StationRequestId,
    StoryEvent, TicketKind, VowId,
};

pub const DEPARTURE_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct DepartureAction {
    pub departure: DepartureId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DepartureSummary {
    pub departure: DepartureId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub prepared: bool,
    pub ready: bool,
}

pub fn available_departures(state: &GameState) -> Vec<DepartureAction> {
    DepartureId::ALL
        .iter()
        .copied()
        .filter(|departure| !state.has_prepared_departure(*departure))
        .filter(|departure| departure.location() == state.location)
        .filter(|departure| departure_visible(state, *departure))
        .map(|departure| {
            let missing = missing_requirements(state, departure);
            DepartureAction {
                departure,
                label: departure.label(),
                detail: if missing.is_empty() {
                    "这条路线已经有足够重量，可以先把最后一步准备好。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn departure_summaries(state: &GameState) -> Vec<DepartureSummary> {
    DepartureId::ALL
        .iter()
        .copied()
        .map(|departure| {
            let prepared = state.has_prepared_departure(departure);
            let visible = prepared || departure_visible(state, departure);
            let missing = missing_requirements(state, departure);
            let ready = visible && missing.is_empty() && !prepared;
            let progress = departure_progress(departure, missing.len(), visible, prepared);
            let status = if prepared {
                "已准备"
            } else if ready {
                "可准备"
            } else if visible {
                "缺手续"
            } else {
                "未显形"
            };
            let detail = if prepared {
                departure.review().to_string()
            } else if ready {
                format!(
                    "{}已经可准备。前往{}，可以把这条路线提前落到手上。",
                    departure.title(),
                    departure.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这条路线准备还没有显形。继续推进{}路线、人物关系和记忆回廊。",
                    departure.ending().short_title()
                )
            };

            DepartureSummary {
                departure,
                title: departure.title(),
                status,
                detail,
                progress,
                visible,
                prepared,
                ready,
            }
        })
        .collect()
}

pub fn prepare(state: &mut GameState, departure: DepartureId) -> StoryEvent {
    if state.has_prepared_departure(departure) {
        return StoryEvent::new(
            "路线已经准备好",
            "这件事已经被你放在终点之前。再次确认它，只会让车站听见你终于不再把选择拖到最后一秒。",
        )
        .tag("路线准备");
    }

    let missing = missing_requirements(state, departure);
    if departure.location() != state.location
        || !departure_visible(state, departure)
        || !missing.is_empty()
    {
        return StoryEvent::new(
            "路线还缺一道手续",
            format!(
                "你想提前把结局拿稳，但车站不接受空泛的决心。{}",
                if departure.location() != state.location {
                    format!("这件事不在这里，而在{}。", departure.location().title())
                } else if missing.is_empty() {
                    "这条路线准备还没有被今晚承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("路线准备");
    }

    state.prepare_departure(departure);
    let mut event = departure_event(departure);
    apply_departure_rewards(state, departure, &mut event);
    event
}

pub fn ending_note(ending: Ending, state: &GameState) -> Option<String> {
    let prepared = DepartureId::ALL
        .iter()
        .copied()
        .find(|departure| departure.ending() == ending && state.has_prepared_departure(*departure));
    prepared.map(|departure| {
        format!(
            "你曾提前完成路线准备：{}。所以终点抵达时，它不像仓促决定，更像一条被你亲手铺过的路。",
            departure.title()
        )
    })
}

impl DepartureId {
    pub const ALL: [Self; DEPARTURE_COUNT] = [
        Self::SingleReturnPocket,
        Self::ChildWindowSeat,
        Self::TimetableMatch,
        Self::BroadcastScript,
        Self::KeeperLedger,
        Self::LastNoticeOnPlatform,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::SingleReturnPocket => Location::WaitingHall,
            Self::ChildWindowSeat => Location::TicketOffice,
            Self::TimetableMatch => Location::LostAndFound,
            Self::BroadcastScript | Self::KeeperLedger => Location::ClockTower,
            Self::LastNoticeOnPlatform => Location::Platform,
        }
    }

    pub fn ending(self) -> Ending {
        match self {
            Self::SingleReturnPocket => Ending::EscapedAlone,
            Self::ChildWindowSeat => Ending::TookChildHome,
            Self::TimetableMatch => Ending::BurnedTimetable,
            Self::BroadcastScript => Ending::BecameTheVoice,
            Self::KeeperLedger => Ending::NewStationKeeper,
            Self::LastNoticeOnPlatform => Ending::LostPassenger,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SingleReturnPocket => "路线准备：把自己的名字缝进票夹",
            Self::ChildWindowSeat => "路线准备：请售票员保留 07B",
            Self::TimetableMatch => "路线准备：把火柴夹进旧时刻表",
            Self::BroadcastScript => "路线准备：誊清广播稿",
            Self::KeeperLedger => "路线准备：把值夜簿翻到新页",
            Self::LastNoticeOnPlatform => "路线准备：把湿票留给后来者",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SingleReturnPocket => "自己的名字缝进票夹",
            Self::ChildWindowSeat => "保留 07B 靠窗座位",
            Self::TimetableMatch => "火柴夹进旧时刻表",
            Self::BroadcastScript => "誊清广播稿",
            Self::KeeperLedger => "值夜簿的新页",
            Self::LastNoticeOnPlatform => "留给后来者的湿票",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::SingleReturnPocket => "你把自己的名字缝进票夹，离开不再只是从雾里逃出去。",
            Self::ChildWindowSeat => {
                "售票员保留了 07B。孩子若上车，会先看见靠窗的位置，而不是你的愧疚。"
            }
            Self::TimetableMatch => "火柴夹进旧时刻表，规则终于有了被撤销前的最后一页。",
            Self::BroadcastScript => "广播稿已经誊清，完整姓名不再只是一阵临场的勇敢。",
            Self::KeeperLedger => "值夜簿翻到新页，守夜从姿态变成需要被约束的职责。",
            Self::LastNoticeOnPlatform => "湿票被留在月台边，犹豫也被写成后来者能读懂的警告。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::SingleReturnPocket | Self::KeeperLedger => 3,
            Self::ChildWindowSeat
            | Self::TimetableMatch
            | Self::BroadcastScript
            | Self::LastNoticeOnPlatform => 4,
        }
    }
}

fn departure_visible(state: &GameState, departure: DepartureId) -> bool {
    match departure {
        DepartureId::SingleReturnPocket => {
            state.has_flag(Flag::RecoveredName) || state.has_vow(VowId::ReturnWithoutErasing)
        }
        DepartureId::ChildWindowSeat => {
            state.has_flag(Flag::ChildJoined) || state.has_vow(VowId::DoNotOwnTheChild)
        }
        DepartureId::TimetableMatch => {
            state.has_item(Item::OldTimetable) || state.has_flag(Flag::SynthesizedStationTruth)
        }
        DepartureId::BroadcastScript => {
            state.has_item(Item::BroadcastTape) || state.has_memory(MemoryId::BroadcastPractice)
        }
        DepartureId::KeeperLedger => {
            state.has_flag(Flag::UnderstoodStationMechanism)
                || state.has_vow(VowId::LightWithoutDebt)
        }
        DepartureId::LastNoticeOnPlatform => {
            state.has_completed_request(StationRequestId::LastNotice)
                || state.completed_requests.len() >= 3
                || state.has_vow(VowId::ReadTheWholeWarning)
        }
    }
}

fn missing_requirements(state: &GameState, departure: DepartureId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match departure {
        DepartureId::SingleReturnPocket => {
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.ticket == TicketKind::Return || state.has_flag(Flag::InspectedRails),
                "取得返程票或检查返程轮痕",
            );
            require(
                &mut missing,
                state.has_vow(VowId::ReturnWithoutErasing)
                    || state.has_memory(MemoryId::SeventhBench),
                "写下离开锚点或走过第七张长椅",
            );
        }
        DepartureId::ChildWindowSeat => {
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined),
                "让孩子愿意同行",
            );
            require(
                &mut missing,
                state.ticket == TicketKind::Return || state.has_flag(Flag::SynthesizedChildTruth),
                "取得返程票或整理孩子真相",
            );
            require(
                &mut missing,
                state.has_vow(VowId::DoNotOwnTheChild) || state.has_vow(VowId::OrdinaryTomorrow),
                "写下关于孩子或明天的锚点",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::WhiteLineMeasure)
                    || state.has_completed_request(StationRequestId::HomeworkEnvelope),
                "走过白线刻度记忆或完成作业本页角委托",
            );
        }
        DepartureId::TimetableMatch => {
            require(
                &mut missing,
                state.has_item(Item::OldTimetable),
                "取得烧焦的旧时刻表",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.has_flag(Flag::SynthesizedStationTruth),
                "整理车站真相",
            );
            require(
                &mut missing,
                state.has_vow(VowId::TruthBeforeMercy)
                    || state.has_memory(MemoryId::BorrowedClockMinute),
                "写下真相锚点或走过借来的最后一分钟",
            );
        }
        DepartureId::BroadcastScript => {
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) && state.has_item(Item::BroadcastTape),
                "取得站务日志和广播磁带",
            );
            require(&mut missing, state.has_flag(Flag::AlignedClock), "校准旧钟");
            require(
                &mut missing,
                state.has_memory(MemoryId::BroadcastPractice)
                    || state.has_resolved_resonance(ResonanceId::BroadcastAfterimage),
                "走过广播练习室或触发广播后的影子共鸣",
            );
        }
        DepartureId::KeeperLedger => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth),
                "听懂旧钟代价",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodStationMechanism),
                "理解车站机制",
            );
            require(
                &mut missing,
                state.has_vow(VowId::LightWithoutDebt)
                    || state.has_resolved_resonance(ResonanceId::DebtOfKeepingWatch),
                "写下灯光锚点或触发守夜与欠债共鸣",
            );
        }
        DepartureId::LastNoticeOnPlatform => {
            require(
                &mut missing,
                state.has_vow(VowId::ReadTheWholeWarning),
                "写下读完整个警告的锚点",
            );
            require(
                &mut missing,
                state.has_completed_request(StationRequestId::LastNotice)
                    || state.completed_requests.len() >= 3,
                "完成最后到站通知或至少三件旅客委托",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::SeventhBench) || state.visited_memories.len() >= 2,
                "走过第七张长椅或至少两段记忆回廊",
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

fn departure_progress(
    departure: DepartureId,
    missing_count: usize,
    visible: bool,
    prepared: bool,
) -> u8 {
    if prepared {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = departure.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_departure_rewards(state: &mut GameState, departure: DepartureId, event: &mut StoryEvent) {
    match departure {
        DepartureId::SingleReturnPocket => {
            state.remember(Flag::SynthesizedRoute);
            event.tags.push("准备：单程逃离".to_string());
        }
        DepartureId::ChildWindowSeat => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = 5;
            event.tags.push("准备：带孩子返程".to_string());
        }
        DepartureId::TimetableMatch => {
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("准备：烧掉时刻表".to_string());
        }
        DepartureId::BroadcastScript => {
            state.remember(Flag::HeardBroadcastTape);
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("准备：广播员".to_string());
        }
        DepartureId::KeeperLedger => {
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("准备：新任站务员".to_string());
        }
        DepartureId::LastNoticeOnPlatform => {
            state.remember(Flag::UnderstoodFirstLoop);
            event.tags.push("准备：遗失旅客".to_string());
        }
    }
}

fn departure_event(departure: DepartureId) -> StoryEvent {
    let (title, body) = match departure {
        DepartureId::SingleReturnPocket => (
            "路线准备：自己的名字缝进票夹",
            "你把自己的名字写在票夹内侧，用候车厅镜片压平。若最后选择独自上车，至少不再把离开说成无事发生。票夹合上时，你听见湿票轻轻响了一声，像一扇很小的门。",
        ),
        DepartureId::ChildWindowSeat => (
            "路线准备：保留 07B 靠窗座位",
            "你请售票员把 07B 留给孩子。售票员没有问这是不是原谅，只在座位图上画了一个小小的窗。你忽然明白，有些准备不宏大，却比发誓更可靠：让他上车以后先看见外面，而不是先看见你的脸。",
        ),
        DepartureId::TimetableMatch => (
            "路线准备：火柴夹进旧时刻表",
            "你把一根火柴夹进烧焦的旧时刻表。火柴还没有点燃，却已经让纸页开始诚实。车站的规则可以被撤销，但撤销之前，你必须承认它曾经保护过一些人，也曾经困住另一些人。",
        ),
        DepartureId::BroadcastScript => (
            "路线准备：誊清广播稿",
            "你把广播稿誊清，连第二个名字也完整写下。纸面很轻，却比临场勇气重。若你走进广播室，后来者听见的不会只是一句惊慌的警告，而是一份尽量不再省略人的说明。",
        ),
        DepartureId::KeeperLedger => (
            "路线准备：值夜簿的新页",
            "你把值夜簿翻到空白页，在页角写下第一条规矩：灯光不能要求被感谢。站务员看见这句，沉默很久。他没有点头，但把钢笔推给你，像承认守夜若没有边界，很快就会长成债主。",
        ),
        DepartureId::LastNoticeOnPlatform => (
            "路线准备：留给后来者的湿票",
            "你把一张抄清的湿票压在月台边，背面写着完整警告。也许最后你仍会犹豫，也许雾灯号仍会替你决定。但这次，犹豫不会只留下空白；后来者至少能读到你终于读完的那半句。",
        ),
    };
    StoryEvent::new(title, body).tag("路线准备").tag("终局铺垫")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn departure_summaries_track_ready_and_prepared_state() {
        let mut state = GameState::new();
        let initial = departure_summaries(&state);
        let single = initial
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single return prep should be listed");
        assert_eq!(single.status, "未显形");

        state.remember(Flag::RecoveredName);
        state.remember(Flag::InspectedRails);
        let partial = departure_summaries(&state);
        let single = partial
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single return prep should be listed");
        assert_eq!(single.status, "缺手续");
        assert!(single.visible);
        assert!(single.progress > 0);

        state.visit_memory(MemoryId::SeventhBench);
        let ready = departure_summaries(&state);
        let single = ready
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single return prep should be listed");
        assert_eq!(single.status, "可准备");
        assert!(single.ready);

        let event = prepare(&mut state, DepartureId::SingleReturnPocket);
        assert!(event.tags.iter().any(|tag| tag == "终局铺垫"));
        assert!(state.has_prepared_departure(DepartureId::SingleReturnPocket));
        assert!(state.has_flag(Flag::SynthesizedRoute));
    }

    #[test]
    fn departure_count_matches_declared_routes() {
        assert_eq!(DEPARTURE_COUNT, DepartureId::ALL.len());
        assert_eq!(DEPARTURE_COUNT, Ending::ALL.len());
    }
}
