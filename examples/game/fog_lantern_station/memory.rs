use crate::model::{
    EvidenceId, Flag, GameState, Item, Location, MemoryId, ResonanceId, StationRequestId,
    StoryEvent, TopicId, VowId,
};

#[cfg(test)]
use crate::{resonance, vow};

pub const MEMORY_COUNT: usize = 8;

#[derive(Clone, Debug)]
pub struct MemoryAction {
    pub memory: MemoryId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemorySummary {
    pub memory: MemoryId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub visited: bool,
    pub ready: bool,
}

pub fn available_memories(state: &GameState) -> Vec<MemoryAction> {
    MemoryId::ALL
        .iter()
        .copied()
        .filter(|memory| !state.has_memory(*memory))
        .filter(|memory| memory.location() == state.location)
        .filter(|memory| memory_visible(state, *memory))
        .map(|memory| {
            let missing = missing_requirements(state, memory);
            MemoryAction {
                memory,
                label: memory.label(),
                detail: if missing.is_empty() {
                    "这处地点愿意把保存的那一晚放出来。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn memory_summaries(state: &GameState) -> Vec<MemorySummary> {
    MemoryId::ALL
        .iter()
        .copied()
        .map(|memory| {
            let visited = state.has_memory(memory);
            let visible = visited || memory_visible(state, memory);
            let missing = missing_requirements(state, memory);
            let ready = visible && missing.is_empty() && !visited;
            let progress = memory_progress(memory, missing.len(), visible, visited);
            let status = if visited {
                "已走过"
            } else if ready {
                "可进入"
            } else if visible {
                "未连通"
            } else {
                "未显形"
            };
            let detail = if visited {
                memory.review().to_string()
            } else if ready {
                format!(
                    "{}已经准备好。前往{}，可以进入这段记忆。",
                    memory.title(),
                    memory.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这段记忆还藏在{}。继续调查地点、追问人物或整理共鸣，它才会显形。",
                    memory.location().title()
                )
            };

            MemorySummary {
                memory,
                title: memory.title(),
                status,
                detail,
                progress,
                visible,
                visited,
                ready,
            }
        })
        .collect()
}

pub fn enter(state: &mut GameState, memory: MemoryId) -> StoryEvent {
    if state.has_memory(memory) {
        return StoryEvent::new(
            "记忆已经被走过",
            "这段回廊已经留在你的日志里。再次站到门口，只会听见里面的脚步声和你现在的呼吸重叠。",
        )
        .tag("记忆回廊");
    }

    let missing = missing_requirements(state, memory);
    if memory.location() != state.location || !memory_visible(state, memory) || !missing.is_empty()
    {
        return StoryEvent::new(
            "记忆还没有开门",
            format!(
                "这里确实有一段回廊，但门把手还冷着。{}",
                if memory.location() != state.location {
                    format!("它不在这里，而在{}。", memory.location().title())
                } else if missing.is_empty() {
                    "这段记忆还没有被今晚承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("记忆回廊");
    }

    state.visit_memory(memory);
    let mut event = memory_event(memory);
    apply_memory_rewards(state, memory, &mut event);
    event
}

impl MemoryId {
    pub const ALL: [Self; MEMORY_COUNT] = [
        Self::SeventhBench,
        Self::TicketWindowReflection,
        Self::RaincoatPocket,
        Self::EvacuationLine,
        Self::BorrowedClockMinute,
        Self::WhiteLineMeasure,
        Self::BroadcastPractice,
        Self::OrdinaryKitchen,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::SeventhBench | Self::OrdinaryKitchen => Location::WaitingHall,
            Self::TicketWindowReflection => Location::TicketOffice,
            Self::RaincoatPocket => Location::LostAndFound,
            Self::EvacuationLine => Location::Underpass,
            Self::BorrowedClockMinute | Self::BroadcastPractice => Location::ClockTower,
            Self::WhiteLineMeasure => Location::Platform,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SeventhBench => "进入记忆：第七张长椅",
            Self::TicketWindowReflection => "进入记忆：窗口里的单程票",
            Self::RaincoatPocket => "进入记忆：雨衣口袋",
            Self::EvacuationLine => "进入记忆：疏散演练白线",
            Self::BorrowedClockMinute => "进入记忆：借来的最后一分钟",
            Self::WhiteLineMeasure => "进入记忆：白线刻度",
            Self::BroadcastPractice => "进入记忆：广播练习室",
            Self::OrdinaryKitchen => "进入记忆：普通清晨的厨房",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::SeventhBench => "第七张长椅",
            Self::TicketWindowReflection => "窗口里的单程票",
            Self::RaincoatPocket => "雨衣口袋",
            Self::EvacuationLine => "疏散演练白线",
            Self::BorrowedClockMinute => "借来的最后一分钟",
            Self::WhiteLineMeasure => "白线刻度",
            Self::BroadcastPractice => "广播练习室",
            Self::OrdinaryKitchen => "普通清晨的厨房",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::SeventhBench => "候车厅让你看见 07 号长椅原本有两个湿座位。",
            Self::TicketWindowReflection => "售票窗口承认，单程票最可怕的地方是它很容易盖章。",
            Self::RaincoatPocket => "雨衣口袋里的姓名牌证明，孩子不是后来才被你想象出来的人。",
            Self::EvacuationLine => {
                "地下通道保存着疏散演练：同一条白线，后来被你喊成不准越过的命令。"
            }
            Self::BorrowedClockMinute => "旧钟让你看见自己申请停住午夜时，是想回到那句命令之前。",
            Self::WhiteLineMeasure => "三号月台的刻度说明，孩子那晚确实因为听话站在线后等你。",
            Self::BroadcastPractice => "广播练习室保存着你无数次停在那句旧命令前的声音。",
            Self::OrdinaryKitchen => "候车厅短暂变成厨房，让明天落回一杯热牛奶和一句可以拒绝的话。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::SeventhBench | Self::TicketWindowReflection => 3,
            Self::RaincoatPocket
            | Self::EvacuationLine
            | Self::BorrowedClockMinute
            | Self::WhiteLineMeasure
            | Self::BroadcastPractice
            | Self::OrdinaryKitchen => 4,
        }
    }
}

fn memory_visible(state: &GameState, memory: MemoryId) -> bool {
    match memory {
        MemoryId::SeventhBench => {
            state.has_flag(Flag::ReadDepartureBoard)
                || state.investigation_depth(Location::WaitingHall) >= 1
        }
        MemoryId::TicketWindowReflection => {
            state.has_item(Item::CoinToken) || state.clerk_depth >= 2
        }
        MemoryId::RaincoatPocket => {
            state.has_item(Item::NameTag) || state.investigation_depth(Location::LostAndFound) >= 3
        }
        MemoryId::EvacuationLine => {
            state.has_flag(Flag::RecoveredName)
                || state.investigation_depth(Location::Underpass) >= 3
        }
        MemoryId::BorrowedClockMinute => {
            state.has_item(Item::StationLog) || state.has_flag(Flag::HeardClockTruth)
        }
        MemoryId::WhiteLineMeasure => {
            state.has_flag(Flag::MetChild) || state.investigation_depth(Location::Platform) >= 1
        }
        MemoryId::BroadcastPractice => {
            state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape)
        }
        MemoryId::OrdinaryKitchen => {
            state.has_vow(VowId::OrdinaryTomorrow)
                || state.has_completed_request(StationRequestId::HomeworkEnvelope)
                || state.has_discussed(TopicId::ChildTomorrow)
        }
    }
}

fn missing_requirements(state: &GameState, memory: MemoryId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match memory {
        MemoryId::SeventhBench => {
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
            require(
                &mut missing,
                state.has_item(Item::MirrorShard) || state.has_flag(Flag::FoundMirrorShard),
                "取得候车厅镜片",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain) || state.traveler_depth >= 1,
                "让老人谈过雨或第一枚钥匙",
            );
        }
        MemoryId::TicketWindowReflection => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.clerk_depth >= 2 || state.has_discussed(TopicId::ClerkOneWay),
                "让售票员讲过退票规则",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.has_flag(Flag::ReadStationLog),
                "取得或读过站务日志",
            );
        }
        MemoryId::RaincoatPocket => {
            require(
                &mut missing,
                state.has_flag(Flag::SearchedLostFound)
                    || state.investigation_depth(Location::LostAndFound) >= 3,
                "翻过失物箱或深入调查失物招领处",
            );
            require(
                &mut missing,
                state.has_item(Item::NameTag) || state.has_flag(Flag::ReturnedNameTag),
                "找到或归还姓名牌",
            );
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodChildPromise)
                    || state.has_presented(EvidenceId::ChildTicket),
                "理解湿票里的第二个名字",
            );
        }
        MemoryId::EvacuationLine => {
            require(
                &mut missing,
                state.investigation_depth(Location::Underpass) >= 3,
                "调查地下通道前三层",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodFirstLoop)
                    || state.has_resolved_resonance(ResonanceId::RainInTheMirror),
                "理解第一次循环",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework) || state.has_flag(Flag::MetChild),
                "见过孩子或作业本",
            );
        }
        MemoryId::BorrowedClockMinute => {
            require(
                &mut missing,
                state.has_item(Item::StationLog),
                "取得站务日志",
            );
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth),
                "听站务员说出旧钟代价",
            );
            require(
                &mut missing,
                state.has_item(Item::OldTimetable) || state.has_discussed(TopicId::KeeperTimetable),
                "取得旧时刻表或让站务员看见它",
            );
            require(
                &mut missing,
                state.has_vow(VowId::TruthBeforeMercy)
                    || state.has_resolved_resonance(ResonanceId::BorrowedMinute),
                "写下真相锚点或触发借来的一分钟共鸣",
            );
        }
        MemoryId::WhiteLineMeasure => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.investigation_depth(Location::Platform) >= 2
                    || state.has_flag(Flag::InspectedRails),
                "调查月台轮痕或轨道",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework) || state.has_discussed(TopicId::ChildHomework),
                "看过作业本",
            );
            require(
                &mut missing,
                state.child_trust >= 3
                    || state.has_resolved_resonance(ResonanceId::WhiteLineHomework),
                "让孩子信任你，或触发白线后的作业题共鸣",
            );
        }
        MemoryId::BroadcastPractice => {
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得广播磁带或听见广播线索",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.has_flag(Flag::AlignedClock)
                    || state.has_presented(EvidenceId::KeeperBroadcastTape),
                "校准旧钟或向站务员出示广播磁带",
            );
            require(
                &mut missing,
                state.has_resolved_resonance(ResonanceId::BroadcastAfterimage)
                    || state.has_discussed(TopicId::KeeperBroadcast),
                "触发广播后的影子共鸣或追问广播室",
            );
        }
        MemoryId::OrdinaryKitchen => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_completed_request(StationRequestId::HomeworkEnvelope),
                "完成作业本页角委托",
            );
            require(
                &mut missing,
                state.has_vow(VowId::OrdinaryTomorrow),
                "写下允许明天普通的锚点",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined) || state.child_trust >= 5,
                "让孩子愿意同行，或把信任推到最高",
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

fn memory_progress(memory: MemoryId, missing_count: usize, visible: bool, visited: bool) -> u8 {
    if visited {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = memory.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_memory_rewards(state: &mut GameState, memory: MemoryId, event: &mut StoryEvent) {
    match memory {
        MemoryId::SeventhBench => {
            state.remember(Flag::UnderstoodFirstLoop);
            state.remember(Flag::TravelerTrusted);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("记忆：湿票".to_string());
        }
        MemoryId::TicketWindowReflection => {
            state.remember(Flag::SynthesizedRoute);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("记忆：返程".to_string());
        }
        MemoryId::RaincoatPocket => {
            state.remember(Flag::UnderstoodChildPromise);
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("记忆：孩子".to_string());
        }
        MemoryId::EvacuationLine => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("记忆：疏散".to_string());
        }
        MemoryId::BorrowedClockMinute => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("记忆：旧钟".to_string());
        }
        MemoryId::WhiteLineMeasure => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("记忆：白线".to_string());
        }
        MemoryId::BroadcastPractice => {
            state.remember(Flag::HeardBroadcastTape);
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("记忆：广播室".to_string());
        }
        MemoryId::OrdinaryKitchen => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = 5;
            event.tags.push("记忆：明天".to_string());
        }
    }
}

fn memory_event(memory: MemoryId) -> StoryEvent {
    let (title, body) = match memory {
        MemoryId::SeventhBench => (
            "记忆回廊：第七张长椅",
            "候车厅的长椅忽然向后退去，露出六年前的雨夜。第七张长椅上坐着两个浑身湿透的人：十六岁的你和更小的孩子。你们逃得太急，鞋底还带着家门口的泥。座位图上 07A 被雨水泡深，07B 的字迹却淡得像还没来得及生效。",
        ),
        MemoryId::TicketWindowReflection => (
            "记忆回廊：窗口里的单程票",
            "售票窗口的玻璃变成黑色水面。你看见过去的自己把口袋翻到发抖，只摸出一张皱烂的票。身后远处有狗叫和咒骂声。售票员说只够核 07A。过去的你没有哭，也没有解释，只把孩子往身后藏了藏，像这样就能把世界少算一份。",
        ),
        MemoryId::RaincoatPocket => (
            "记忆回廊：雨衣口袋",
            "失物箱深处亮起一件小雨衣。口袋里有糖纸、半截铅笔和一块干净到不合时宜的姓名牌。你终于想起，那天孩子把姓名牌交给你保管，说如果人太多，就用它叫我。后来你没有叫出声，只用最凶的声音叫他站住。",
        ),
        MemoryId::EvacuationLine => (
            "记忆回廊：疏散演练白线",
            "地下通道变成一次很久以前的疏散演练。白线被孩子们踩得发亮，老师说站在线后等待大人回来。你看见孩子认真地点头，把鞋尖收回线内。多年以后，你用了同样的口气喊他站住；他也用同样的动作，把鞋尖收了回去。",
        ),
        MemoryId::BorrowedClockMinute => (
            "记忆回廊：借来的最后一分钟",
            "旧钟楼的齿轮向两边让开。你看见自己在站务日志上签字，请求保留 23:59 至 00:00。站务员问你明白利息吗。过去的你说只要能回到那句“不准动”之前，什么都可以。现在的你终于听见这句话里最危险的部分：它把未来也一并抵押了。",
        ),
        MemoryId::WhiteLineMeasure => (
            "记忆回廊：白线刻度",
            "三号月台的白线升起一道小刻度，正好到孩子肩膀。记忆里的他站在线后，抱着作业本看向车门。雨水漫过鞋面，巡视员喊他走，他却摇头：哥哥说了，不准越线。他没有反抗任何人，所以被所有规则一起留下。",
        ),
        MemoryId::BroadcastPractice => (
            "记忆回廊：广播练习室",
            "钟楼墙后出现一间窄小的广播室。磁带转动，你听见自己练习同一句警告：别上车，别一个人上车，别再用命令保护你爱的人。每一次练到孩子的名字，声音都会停住。不是忘记，而是那时的你还不敢承认命令来自自己。",
        ),
        MemoryId::OrdinaryKitchen => (
            "记忆回廊：普通清晨的厨房",
            "候车厅短暂变成一间很小的厨房。窗外没有雾灯，只有早市的声音。孩子坐在桌边吹一杯热牛奶，问今天是不是不用听命令的一天。你没有回答得很漂亮，只说：是，今天你可以先说不。他点头，把杯子推近一点，却没有立刻原谅你。",
        ),
    };
    StoryEvent::new(title, body).tag("记忆回廊").tag("地点记忆")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let visited = state.visited_memories.len();
    if visited == 0 {
        return None;
    }

    Some(format!(
        "你走过 {visited} 段记忆回廊。雾灯站因此少了一些大词，多了一些具体的时间、地点、姓名和鞋印。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_summaries_track_ready_and_visited_state() {
        let mut state = GameState::new();
        let initial = memory_summaries(&state);
        let bench = initial
            .iter()
            .find(|summary| summary.memory == MemoryId::SeventhBench)
            .expect("bench memory should be listed");
        assert_eq!(bench.status, "未显形");
        assert_eq!(bench.progress, 0);

        state.remember(Flag::ReadDepartureBoard);
        state.add_item(Item::MirrorShard);
        let partial = memory_summaries(&state);
        let bench = partial
            .iter()
            .find(|summary| summary.memory == MemoryId::SeventhBench)
            .expect("bench memory should be listed");
        assert_eq!(bench.status, "未连通");
        assert!(bench.visible);
        assert!(bench.progress > 0);

        state.discuss(TopicId::TravelerRain);
        let ready = memory_summaries(&state);
        let bench = ready
            .iter()
            .find(|summary| summary.memory == MemoryId::SeventhBench)
            .expect("bench memory should be listed");
        assert_eq!(bench.status, "可进入");
        assert!(bench.ready);

        let event = enter(&mut state, MemoryId::SeventhBench);
        assert!(event.tags.iter().any(|tag| tag == "地点记忆"));
        assert!(state.has_memory(MemoryId::SeventhBench));
        assert!(state.has_flag(Flag::UnderstoodFirstLoop));
    }

    #[test]
    fn memory_count_stays_aligned_with_progression_systems() {
        assert_eq!(MEMORY_COUNT, MemoryId::ALL.len());
        assert!(MEMORY_COUNT >= resonance::RESONANCE_COUNT);
        assert!(MEMORY_COUNT >= vow::VOW_COUNT);
    }
}
