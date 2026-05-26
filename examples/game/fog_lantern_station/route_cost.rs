use crate::departure;
use crate::model::{
    AftertalkId, CompanionTalkId, DepartureId, Ending, Flag, GameState, Item, LampFocusId,
    Location, ResonanceId, RouteCostId, StationRequestId, StoryEvent, VowId,
};

pub const ROUTE_COST_COUNT: usize = departure::DEPARTURE_COUNT;

#[derive(Clone, Debug)]
pub struct RouteCostAction {
    pub cost: RouteCostId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteCostSummary {
    pub cost: RouteCostId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub mitigated: bool,
    pub ready: bool,
}

pub fn available_costs(state: &GameState) -> Vec<RouteCostAction> {
    RouteCostId::ALL
        .iter()
        .copied()
        .filter(|cost| !state.has_mitigated_route_cost(*cost))
        .filter(|cost| cost.location() == state.location)
        .filter(|cost| cost_visible(state, *cost))
        .map(|cost| {
            let missing = missing_requirements(state, cost);
            RouteCostAction {
                cost,
                label: cost.label(),
                detail: if missing.is_empty() {
                    "这条路线的代价已经可以提前处理。它不会抹掉后果，只会让选择少一点伤人。"
                        .to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn cost_summaries(state: &GameState) -> Vec<RouteCostSummary> {
    RouteCostId::ALL
        .iter()
        .copied()
        .map(|cost| {
            let mitigated = state.has_mitigated_route_cost(cost);
            let visible = mitigated || cost_visible(state, cost);
            let missing = missing_requirements(state, cost);
            let ready = visible && missing.is_empty() && !mitigated;
            let progress = cost_progress(cost, missing.len(), visible, mitigated);
            let status = if mitigated {
                "已调停"
            } else if ready {
                "可调停"
            } else if visible {
                "缺承担"
            } else {
                "未显形"
            };
            let detail = if mitigated {
                cost.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以调停。前往{}，先处理这条路线会留下的伤口。",
                    cost.title(),
                    cost.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "先完成路线试炼：{}。只有被预演过的终点，才会暴露真正代价。",
                    cost.departure().title()
                )
            };

            RouteCostSummary {
                cost,
                title: cost.title(),
                status,
                detail,
                progress,
                visible,
                mitigated,
                ready,
            }
        })
        .collect()
}

pub fn mitigate(state: &mut GameState, cost: RouteCostId) -> StoryEvent {
    if state.has_mitigated_route_cost(cost) {
        return StoryEvent::new(
            "这项代价已经被看见",
            "你再次回到这条路线的阴影旁。它没有消失，只是不再躲在选择按钮背后。",
        )
        .tag("路线代价");
    }

    let missing = missing_requirements(state, cost);
    if cost.location() != state.location || !cost_visible(state, cost) || !missing.is_empty() {
        return StoryEvent::new(
            "代价还不能调停",
            format!(
                "你试着提前面对这条路线会伤到谁，但现在还没有足够承担。{}",
                if cost.location() != state.location {
                    format!("这项代价不在这里，而在{}。", cost.location().title())
                } else if !cost_visible(state, cost) {
                    "先完成对应路线试炼，代价才会显形。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("路线代价");
    }

    state.mitigate_route_cost(cost);
    let mut event = cost_event(cost);
    apply_cost_rewards(state, cost, &mut event);
    event
}

impl RouteCostId {
    pub const ALL: [Self; ROUTE_COST_COUNT] = [
        Self::AloneEmptySeat,
        Self::ChildUnforgivenTomorrow,
        Self::BurnedTimetableAftercare,
        Self::BroadcastSecondName,
        Self::KeeperLightBoundary,
        Self::LostPassengerNotice,
    ];

    pub fn departure(self) -> DepartureId {
        match self {
            Self::AloneEmptySeat => DepartureId::SingleReturnPocket,
            Self::ChildUnforgivenTomorrow => DepartureId::ChildWindowSeat,
            Self::BurnedTimetableAftercare => DepartureId::TimetableMatch,
            Self::BroadcastSecondName => DepartureId::BroadcastScript,
            Self::KeeperLightBoundary => DepartureId::KeeperLedger,
            Self::LostPassengerNotice => DepartureId::LastNoticeOnPlatform,
        }
    }

    pub fn location(self) -> Location {
        match self {
            Self::AloneEmptySeat => Location::WaitingHall,
            Self::ChildUnforgivenTomorrow => Location::Platform,
            Self::BurnedTimetableAftercare => Location::LostAndFound,
            Self::BroadcastSecondName => Location::ClockTower,
            Self::KeeperLightBoundary => Location::ClockTower,
            Self::LostPassengerNotice => Location::Platform,
        }
    }

    pub fn ending(self) -> Ending {
        self.departure().ending()
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AloneEmptySeat => "路线代价：给空座留下说明",
            Self::ChildUnforgivenTomorrow => "路线代价：允许孩子带着害怕上车",
            Self::BurnedTimetableAftercare => "路线代价：把善后写进火前清单",
            Self::BroadcastSecondName => "路线代价：给第二个名字留回声",
            Self::KeeperLightBoundary => "路线代价：给守夜立下边界",
            Self::LostPassengerNotice => "路线代价：把犹豫写成后来者能读懂的票",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AloneEmptySeat => "空座说明",
            Self::ChildUnforgivenTomorrow => "可以害怕的明天",
            Self::BurnedTimetableAftercare => "火前善后清单",
            Self::BroadcastSecondName => "第二个名字的回声",
            Self::KeeperLightBoundary => "守夜边界",
            Self::LostPassengerNotice => "后来者的湿票",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::AloneEmptySeat => "空座被写进说明，独自离开不再能假装没有留下别人。",
            Self::ChildUnforgivenTomorrow => "孩子的害怕被允许同行，明天不再以听话为票价。",
            Self::BurnedTimetableAftercare => "旧规则被烧毁前，小委托和档案先被夹进善后清单。",
            Self::BroadcastSecondName => "广播稿给第二个名字留下回声，警告不再只保存你的声音。",
            Self::KeeperLightBoundary => "守夜被写下边界，灯光不能继续把被照亮的人变成欠债人。",
            Self::LostPassengerNotice => "湿票被写完整，犹豫也能变成后来者可读的路标。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::AloneEmptySeat
            | Self::ChildUnforgivenTomorrow
            | Self::BurnedTimetableAftercare
            | Self::BroadcastSecondName
            | Self::KeeperLightBoundary
            | Self::LostPassengerNotice => 4,
        }
    }
}

fn cost_visible(state: &GameState, cost: RouteCostId) -> bool {
    state.has_mitigated_route_cost(cost) || state.has_rehearsed_departure(cost.departure())
}

fn missing_requirements(state: &GameState, cost: RouteCostId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_rehearsed_departure(cost.departure()),
        "完成对应路线试炼",
    );
    require(
        &mut missing,
        state.current_segment() >= 6,
        "进入第六段午夜以后",
    );
    match cost {
        RouteCostId::AloneEmptySeat => {
            require(
                &mut missing,
                state.has_completed_aftertalk(AftertalkId::TravelerSecondSeat)
                    || state.has_completed_companion_talk(CompanionTalkId::WaitingHallEmptySeat),
                "回访老人和第二个座位，或让孩子坐过第七张长椅",
            );
            require(
                &mut missing,
                state.has_vow(VowId::ReturnWithoutErasing)
                    || state.has_focused_lamp_trace(LampFocusId::WaitingHallBenchTrace),
                "写下不抹去的离开锚点，或照见长椅拖痕",
            );
        }
        RouteCostId::ChildUnforgivenTomorrow => {
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined) && state.child_trust >= 5,
                "让孩子真正愿意同行",
            );
            require(
                &mut missing,
                state.has_vow(VowId::DoNotOwnTheChild)
                    || state
                        .has_completed_companion_talk(CompanionTalkId::PlatformWhiteLineTogether),
                "承认孩子不是免罪材料，或并肩站过白线内侧",
            );
        }
        RouteCostId::BurnedTimetableAftercare => {
            require(
                &mut missing,
                state.completed_requests.len() >= 3
                    || state.has_completed_request(StationRequestId::LastNotice),
                "完成至少三件旅客委托，或贴好最后到站通知",
            );
            require(
                &mut missing,
                state.has_vow(VowId::TruthBeforeMercy) || state.resolved_case_files.len() >= 3,
                "写下真相锚点，或归档至少三份站内档案",
            );
        }
        RouteCostId::BroadcastSecondName => {
            require(
                &mut missing,
                state.has_completed_aftertalk(AftertalkId::KeeperBroadcastReply)
                    || state.has_completed_companion_talk(CompanionTalkId::PlatformDoorQuestion),
                "回访广播稿，或和孩子谈过广播室门",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得或听过广播磁带",
            );
        }
        RouteCostId::KeeperLightBoundary => {
            require(
                &mut missing,
                state.has_vow(VowId::LightWithoutDebt)
                    || state.has_resolved_resonance(ResonanceId::DebtOfKeepingWatch),
                "写下无债之光锚点，或触发守夜与欠债共鸣",
            );
            require(
                &mut missing,
                state.has_focused_lamp_trace(LampFocusId::ClockTowerMinuteDebt)
                    || state.has_completed_aftertalk(AftertalkId::KeeperMinuteHand),
                "照见分针背面的债，或回访分针背面",
            );
        }
        RouteCostId::LostPassengerNotice => {
            require(
                &mut missing,
                state.has_vow(VowId::ReadTheWholeWarning),
                "写下读完整个警告的锚点",
            );
            require(
                &mut missing,
                state.completed_requests.len() >= 4
                    || state.has_completed_request(StationRequestId::LastNotice)
                    || state.has_focused_lamp_trace(LampFocusId::PlatformBrakeLight),
                "完成足够多旅客委托、贴好最后通知，或照见试刹车灯",
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

fn cost_progress(cost: RouteCostId, missing_count: usize, visible: bool, mitigated: bool) -> u8 {
    if mitigated {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = cost.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_cost_rewards(state: &mut GameState, cost: RouteCostId, event: &mut StoryEvent) {
    match cost {
        RouteCostId::AloneEmptySeat => {
            remember_tag(state, event, Flag::TravelerTrusted, "代价：空座说明");
            remember_tag(state, event, Flag::SynthesizedRoute, "代价：不抹去地离开");
        }
        RouteCostId::ChildUnforgivenTomorrow => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "代价：可以害怕的明天",
            );
            state.child_trust = 5;
        }
        RouteCostId::BurnedTimetableAftercare => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "代价：火前善后",
            );
        }
        RouteCostId::BroadcastSecondName => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "代价：第二个名字",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        RouteCostId::KeeperLightBoundary => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "代价：守夜边界",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        RouteCostId::LostPassengerNotice => {
            remember_tag(state, event, Flag::SynthesizedRoute, "代价：后来者湿票");
        }
    }
    event.tags.push("路线代价".to_string());
    event
        .tags
        .push(format!("调停：{}", cost.ending().short_title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn cost_event(cost: RouteCostId) -> StoryEvent {
    let (title, body) = match cost {
        RouteCostId::AloneEmptySeat => (
            "路线代价：空座说明",
            "你在第七排长椅背面写下一段说明：如果我独自离开，这不是因为他不重要，也不是因为我终于清白。老人看完没有替你点头，只把报纸往空座那边推了一寸。那一寸很小，却让独自上车从逃避变成一种需要承担的选择。",
        ),
        RouteCostId::ChildUnforgivenTomorrow => (
            "路线代价：可以害怕的明天",
            "你告诉孩子，到站以后他可以继续害怕，可以生气，可以在某个普通清晨说你做得不够好。他盯着白线很久，说那我上车不是为了让你好受。你说是。风从轨道尽头吹来，明天忽然变得不再像奖赏，而像一个可以容纳坏心情的房间。",
        ),
        RouteCostId::BurnedTimetableAftercare => (
            "路线代价：火前善后清单",
            "你把几件旅客委托和归档标题夹进旧时刻表前页。若最后点火，火不能只替你泄愤，也必须照亮那些仍要被善后的名字。失物招领处的标签一张张安静下来，像承认规则被烧掉之前，仍有人认真把细小的事放回原位。",
        ),
        RouteCostId::BroadcastSecondName => (
            "路线代价：第二个名字的回声",
            "你在广播稿末尾多写一行：如果你听见我的声音，请确认身边是否还有人没被叫到。站务员读到这里时没有纠正格式。磁带空转一圈，吐出很轻的回声，像第二个名字终于有了不是被你占用的空间。",
        ),
        RouteCostId::KeeperLightBoundary => (
            "路线代价：守夜边界",
            "你把值夜簿翻到空白处，写下守夜的边界：灯只负责照路，不负责替人决定；值夜者可以疲惫，不可以用疲惫索取感激。站务员看着那几行字，像第一次发现留下来不是成为答案，而是接受自己也必须被规则约束。",
        ),
        RouteCostId::LostPassengerNotice => (
            "路线代价：后来者的湿票",
            "你把湿票压在月台灯下，背面写完整警告：别上车，如果你还没有读完自己为什么想逃；别一个人上车，如果有人仍被你的解释留在白线后。也许你最后仍会犹豫，但这一次，犹豫至少不再只留下空白。",
        ),
    };
    StoryEvent::new(title, body)
}

pub fn ending_note(ending: Ending, state: &GameState) -> Option<String> {
    let mitigated = RouteCostId::ALL
        .iter()
        .copied()
        .find(|cost| cost.ending() == ending && state.has_mitigated_route_cost(*cost));
    mitigated.map(|cost| {
        format!(
            "你曾提前调停路线代价：{}。终点仍有后果，但它不再把后果藏成按钮背后的阴影。",
            cost.title()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_summaries_track_ready_and_mitigated_state() {
        let mut state = GameState::new();
        let initial = cost_summaries(&state);
        let alone = initial
            .iter()
            .find(|summary| summary.cost == RouteCostId::AloneEmptySeat)
            .expect("alone cost should be listed");
        assert_eq!(alone.status, "未显形");
        assert_eq!(alone.progress, 0);

        state.rehearse_departure(DepartureId::SingleReturnPocket);
        state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        state.complete_aftertalk(AftertalkId::TravelerSecondSeat);
        let partial = cost_summaries(&state);
        let alone = partial
            .iter()
            .find(|summary| summary.cost == RouteCostId::AloneEmptySeat)
            .expect("alone cost should be listed");
        assert_eq!(alone.status, "缺承担");
        assert!(alone.visible);
        assert!(alone.progress > 0);

        state.make_vow(VowId::ReturnWithoutErasing);
        let ready = cost_summaries(&state);
        let alone = ready
            .iter()
            .find(|summary| summary.cost == RouteCostId::AloneEmptySeat)
            .expect("alone cost should be listed");
        assert_eq!(alone.status, "可调停");
        assert!(alone.ready);

        let event = mitigate(&mut state, RouteCostId::AloneEmptySeat);
        assert!(event.tags.iter().any(|tag| tag == "路线代价"));
        assert!(state.has_mitigated_route_cost(RouteCostId::AloneEmptySeat));
    }

    #[test]
    fn cost_count_matches_departure_routes() {
        assert_eq!(ROUTE_COST_COUNT, RouteCostId::ALL.len());
        assert_eq!(ROUTE_COST_COUNT, departure::DEPARTURE_COUNT);
    }
}
