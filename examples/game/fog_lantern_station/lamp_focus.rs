use crate::model::{
    AftertalkId, Flag, GameState, Item, LampFocusId, Location, MemoryId, PatrolId, StoryEvent,
    TopicId, VowId,
};

pub const LAMP_FOCUS_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct LampFocusAction {
    pub focus: LampFocusId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LampFocusSummary {
    pub focus: LampFocusId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub focused: bool,
    pub ready: bool,
}

pub fn available_focuses(state: &GameState) -> Vec<LampFocusAction> {
    LampFocusId::ALL
        .iter()
        .copied()
        .filter(|focus| !state.has_focused_lamp_trace(*focus))
        .filter(|focus| focus.location() == state.location)
        .filter(|focus| focus_visible(state, *focus))
        .map(|focus| {
            let missing = missing_requirements(state, focus);
            LampFocusAction {
                focus,
                label: focus.label(),
                detail: if missing.is_empty() {
                    "雾灯已经能照到这里的背面。调准光束，会把隐藏事实钉进地图。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn focus_summaries(state: &GameState) -> Vec<LampFocusSummary> {
    LampFocusId::ALL
        .iter()
        .copied()
        .map(|focus| {
            let focused = state.has_focused_lamp_trace(focus);
            let visible = focused || focus_visible(state, focus);
            let missing = missing_requirements(state, focus);
            let ready = visible && missing.is_empty() && !focused;
            let progress = focus_progress(focus, missing.len(), visible, focused);
            let status = if focused {
                "已照证"
            } else if ready {
                "可调光"
            } else if visible {
                "缺角度"
            } else {
                "未点亮"
            };
            let detail = if focused {
                focus.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以调光。前往{}，让雾灯照出隐藏事实。",
                    focus.title(),
                    focus.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这束雾灯还照不到{}。先修复雾灯，再把这里调查到足够具体。",
                    focus.location().title()
                )
            };

            LampFocusSummary {
                focus,
                title: focus.title(),
                status,
                detail,
                progress,
                visible,
                focused,
                ready,
            }
        })
        .collect()
}

pub fn focus(state: &mut GameState, focus: LampFocusId) -> StoryEvent {
    if state.has_focused_lamp_trace(focus) {
        return StoryEvent::new(
            "这束光已经留下证词",
            "你再次把雾灯转向同一个角度。墙面只泛起一层旧绿光，像提醒你：这处隐藏事实已经被地图承认。",
        )
        .tag("雾灯照证");
    }

    let missing = missing_requirements(state, focus);
    if focus.location() != state.location || !focus_visible(state, focus) || !missing.is_empty() {
        return StoryEvent::new(
            "雾灯还照不进这里",
            format!(
                "你试着调整雾灯角度，但光束只在雾里散开。{}",
                if focus.location() != state.location {
                    format!("这束光不在这里，而在{}。", focus.location().title())
                } else if !focus_visible(state, focus) {
                    "灯已经修好，但这里还没有显出可照的背面。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("雾灯照证");
    }

    state.focus_lamp_trace(focus);
    let mut event = focus_event(focus);
    apply_focus_rewards(state, focus, &mut event);
    event
}

impl LampFocusId {
    pub const ALL: [Self; LAMP_FOCUS_COUNT] = [
        Self::WaitingHallBenchTrace,
        Self::TicketOfficeReturnGrid,
        Self::LostFoundLabelShadow,
        Self::UnderpassWaterScript,
        Self::ClockTowerMinuteDebt,
        Self::PlatformBrakeLight,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::WaitingHallBenchTrace => Location::WaitingHall,
            Self::TicketOfficeReturnGrid => Location::TicketOffice,
            Self::LostFoundLabelShadow => Location::LostAndFound,
            Self::UnderpassWaterScript => Location::Underpass,
            Self::ClockTowerMinuteDebt => Location::ClockTower,
            Self::PlatformBrakeLight => Location::Platform,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingHallBenchTrace => "雾灯照证：照亮第七排长椅拖痕",
            Self::TicketOfficeReturnGrid => "雾灯照证：照亮返程票同行栏",
            Self::LostFoundLabelShadow => "雾灯照证：照亮失物标签背面",
            Self::UnderpassWaterScript => "雾灯照证：照亮水线下的字",
            Self::ClockTowerMinuteDebt => "雾灯照证：照亮分针背面的债",
            Self::PlatformBrakeLight => "雾灯照证：照亮试刹车灯",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WaitingHallBenchTrace => "第七排长椅拖痕",
            Self::TicketOfficeReturnGrid => "返程票同行栏",
            Self::LostFoundLabelShadow => "失物标签背面",
            Self::UnderpassWaterScript => "水线下的字",
            Self::ClockTowerMinuteDebt => "分针背面的债",
            Self::PlatformBrakeLight => "试刹车灯",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WaitingHallBenchTrace => "雾灯照出第七排长椅曾被推向月台，空座有了方向。",
            Self::TicketOfficeReturnGrid => "返程票同行栏被照亮，窗口规则不再能藏起第二个座位。",
            Self::LostFoundLabelShadow => "失物标签背面的真名显形，歉意不能再代替姓名。",
            Self::UnderpassWaterScript => "水线下的字被读出，循环开始处终于有了可核对的记录。",
            Self::ClockTowerMinuteDebt => "分针背面的债被照亮，守夜从慈悲变成需要归还的手续。",
            Self::PlatformBrakeLight => "试刹车灯被照见，雾灯号的返程轨迹从传闻变成事实。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::WaitingHallBenchTrace
            | Self::TicketOfficeReturnGrid
            | Self::LostFoundLabelShadow
            | Self::UnderpassWaterScript
            | Self::ClockTowerMinuteDebt
            | Self::PlatformBrakeLight => 3,
        }
    }
}

fn focus_visible(state: &GameState, focus: LampFocusId) -> bool {
    state.has_flag(Flag::RepairedFogLamp)
        && (state.has_focused_lamp_trace(focus)
            || state.investigation_depth(focus.location()) >= 2
            || state.has_item(Item::StationMap))
}

fn missing_requirements(state: &GameState, focus: LampFocusId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_flag(Flag::RepairedFogLamp),
        "修复雾灯",
    );
    match focus {
        LampFocusId::WaitingHallBenchTrace => {
            require(
                &mut missing,
                state.has_item(Item::StationMap) || state.has_flag(Flag::FoundStationMap),
                "取得折叠站内图",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard)
                    || state.has_completed_patrol(PatrolId::WaitingHallManifest),
                "读过时刻表，或完成候车厅巡夜",
            );
        }
        LampFocusId::TicketOfficeReturnGrid => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken) || state.has_flag(Flag::FoundCoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkSeats)
                    || state.has_completed_aftertalk(AftertalkId::ClerkRefundQueue),
                "追问两个座位，或回访同行栏",
            );
        }
        LampFocusId::LostFoundLabelShadow => {
            require(
                &mut missing,
                state.has_item(Item::NameTag) || state.has_flag(Flag::ReturnedNameTag),
                "找到或归还姓名牌",
            );
            require(
                &mut missing,
                state.has_flag(Flag::SearchedLostFound)
                    || state.has_completed_patrol(PatrolId::LostFoundShelfAudit),
                "翻过失物箱，或复核失物标签",
            );
        }
        LampFocusId::UnderpassWaterScript => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardUnderpassEcho) || state.has_flag(Flag::RecoveredName),
                "听过地下回声或找回姓名",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::EvacuationLine)
                    || state.has_completed_patrol(PatrolId::UnderpassWaterline),
                "走过疏散记忆，或量过地下水线",
            );
        }
        LampFocusId::ClockTowerMinuteDebt => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth) || state.keeper_depth >= 1,
                "听懂旧钟代价",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog)
                    || state.has_completed_patrol(PatrolId::ClockTowerMinuteHand),
                "取得站务日志，或擦亮分针背面",
            );
        }
        LampFocusId::PlatformBrakeLight => {
            require(
                &mut missing,
                state.has_flag(Flag::InspectedRails)
                    || state.has_completed_patrol(PatrolId::PlatformBoundary),
                "检查轨道，或重描月台白线",
            );
            require(
                &mut missing,
                state.has_flag(Flag::MetChild)
                    || state.has_vow(VowId::ReadTheWholeWarning)
                    || state.has_item(Item::ConductorRoster),
                "见到孩子、写下读完整句警告，或取得列车员名册",
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

fn focus_progress(focus: LampFocusId, missing_count: usize, visible: bool, focused: bool) -> u8 {
    if focused {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = focus.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_focus_rewards(state: &mut GameState, focus: LampFocusId, event: &mut StoryEvent) {
    match focus {
        LampFocusId::WaitingHallBenchTrace => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "照证：长椅拖痕");
            remember_tag(state, event, Flag::TravelerTrusted, "照证：空座方向");
        }
        LampFocusId::TicketOfficeReturnGrid => {
            remember_tag(state, event, Flag::SynthesizedRoute, "照证：同行栏");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        LampFocusId::LostFoundLabelShadow => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "照证：姓名归位");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        LampFocusId::UnderpassWaterScript => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "照证：水线记录");
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "照证：循环结构",
            );
        }
        LampFocusId::ClockTowerMinuteDebt => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "照证：守夜债务",
            );
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "照证：站务真相",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        LampFocusId::PlatformBrakeLight => {
            remember_tag(state, event, Flag::SynthesizedRoute, "照证：返程车灯");
            remember_tag(state, event, Flag::SynthesizedChildTruth, "照证：月台返程");
        }
    }
    event
        .tags
        .push(format!("照证地点：{}", focus.location().title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn focus_event(focus: LampFocusId) -> StoryEvent {
    let (title, body) = match focus {
        LampFocusId::WaitingHallBenchTrace => (
            "雾灯照证：第七排长椅拖痕",
            "你把雾灯光束压低，长椅脚边的拖痕忽然亮起。原来第七排不是凭空缺席，它曾被推向三号月台，又被人仓促推回阴影里。老人看着那道绿光，说：空座不是让你把账抹平的道具，它曾经真的朝他的方向移动过。地图上多出一条很细的铅笔线。",
        ),
        LampFocusId::TicketOfficeReturnGrid => (
            "雾灯照证：返程票同行栏",
            "绿光穿过售票窗口，座位图背面的暗格浮了出来。每一张返程票都有同行栏，只是平时被系统折进纸里。售票员没有解释，只把票章轻轻放下。规则被照见以后仍然冷，但它至少不能再假装自己从未给第二个人留过位置。",
        ),
        LampFocusId::LostFoundLabelShadow => (
            "雾灯照证：失物标签背面",
            "你把雾灯照向失物标签，纸背上的字一行行浮出：姓名不得归入歉意，孩童不得归入证物，雨衣不得归入幸存者的叙述。失物架轻轻摇晃，像终于从错误分类里醒来。你第一次觉得这里不是仓库，而是一个被迫练习公正的地方。",
        ),
        LampFocusId::UnderpassWaterScript => (
            "雾灯照证：水线下的字",
            "地下通道的水线在绿光里变成一行低处的记录：23:59，疏散失败；23:59，请求借出一分钟；23:59，同行者仍在白线后。回声没有重复你，它重复记录本身。被照亮的真相不再像梦，而像一份终于可以被签收的报告。",
        ),
        LampFocusId::ClockTowerMinuteDebt => (
            "雾灯照证：分针背面的债",
            "雾灯照过旧钟，分针背面显出第二层字：守夜者每借出一分钟，须归还一个不再被管理的明天。站务员别过脸。你忽然明白他不是不知道这条规矩，而是守得太久以后，把归还也误认为自己可以安排的事项。",
        ),
        LampFocusId::PlatformBrakeLight => (
            "雾灯照证：试刹车灯",
            "你把雾灯对准轨道尽头，远处红灯不再只是闪烁，而是在雾里留下试刹轨迹。雾灯号确实能返程，它每次都先试着停在白线外，再等待有人带着完整故事靠近。孩子站在你身后，没有说原谅，只说：原来车不是不回来，是大人没有把路带回来。",
        ),
    };
    StoryEvent::new(title, body)
        .tag("雾灯照证")
        .tag("自由探索")
        .tag("地图标注")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let focused = state.focused_lamp_traces.len();
    if focused == 0 {
        return None;
    }

    Some(format!(
        "你完成了 {focused} 处雾灯照证。修好的灯不再只是道具，而成了你重新勘验车站的方法；隐藏事实被光钉进地图，终点也少了一些仓促。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_summaries_track_ready_and_focused_state() {
        let mut state = GameState::new();
        let initial = focus_summaries(&state);
        let hall = initial
            .iter()
            .find(|summary| summary.focus == LampFocusId::WaitingHallBenchTrace)
            .expect("waiting hall focus should be listed");
        assert_eq!(hall.status, "未点亮");
        assert_eq!(hall.progress, 0);

        state.remember(Flag::RepairedFogLamp);
        state.add_item(Item::StationMap);
        state.remember(Flag::ReadDepartureBoard);
        let ready = focus_summaries(&state);
        let hall = ready
            .iter()
            .find(|summary| summary.focus == LampFocusId::WaitingHallBenchTrace)
            .expect("waiting hall focus should be listed");
        assert_eq!(hall.status, "可调光");
        assert!(hall.ready);

        let event = focus(&mut state, LampFocusId::WaitingHallBenchTrace);
        assert!(event.tags.iter().any(|tag| tag == "雾灯照证"));
        assert!(event.tags.iter().any(|tag| tag == "自由探索"));
        assert!(state.has_focused_lamp_trace(LampFocusId::WaitingHallBenchTrace));
        assert!(state.has_flag(Flag::UnderstoodFirstLoop));
    }

    #[test]
    fn focus_count_matches_station_locations() {
        assert_eq!(LAMP_FOCUS_COUNT, LampFocusId::ALL.len());
        assert_eq!(LAMP_FOCUS_COUNT, Location::ALL.len());
    }
}
