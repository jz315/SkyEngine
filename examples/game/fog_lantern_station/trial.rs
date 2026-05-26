use crate::departure;
use crate::model::{
    AftertalkId, AnomalyId, DepartureId, Ending, Flag, GameState, Item, StoryEvent, VowId,
};

pub const TRIAL_COUNT: usize = departure::DEPARTURE_COUNT;

#[derive(Clone, Debug)]
pub struct TrialAction {
    pub departure: DepartureId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrialSummary {
    pub departure: DepartureId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub rehearsed: bool,
    pub ready: bool,
}

pub fn available_trials(state: &GameState) -> Vec<TrialAction> {
    DepartureId::ALL
        .iter()
        .copied()
        .filter(|departure| !state.has_rehearsed_departure(*departure))
        .filter(|departure| departure.location() == state.location)
        .filter(|departure| trial_visible(state, *departure))
        .map(|departure| {
            let missing = missing_requirements(state, departure);
            TrialAction {
                departure,
                label: format!("路线试炼：{}", departure.ending().short_title()),
                detail: if missing.is_empty() {
                    "这条路线已经准备好面对一次终点预演。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn trial_summaries(state: &GameState) -> Vec<TrialSummary> {
    DepartureId::ALL
        .iter()
        .copied()
        .map(|departure| {
            let rehearsed = state.has_rehearsed_departure(departure);
            let visible = rehearsed || trial_visible(state, departure);
            let missing = missing_requirements(state, departure);
            let ready = visible && missing.is_empty() && !rehearsed;
            let progress = trial_progress(departure, missing.len(), visible, rehearsed);
            let status = if rehearsed {
                "已演练"
            } else if ready {
                "可演练"
            } else if visible {
                "缺压力"
            } else {
                "需预备"
            };
            let detail = if rehearsed {
                review(departure).to_string()
            } else if ready {
                format!(
                    "{}已经可以试炼。前往{}，把这条终局路线先走过一遍。",
                    trial_title(departure),
                    departure.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "先完成路线准备：{}。试炼只接受已经落到行动里的结局。",
                    departure.title()
                )
            };

            TrialSummary {
                departure,
                title: trial_title(departure),
                status,
                detail,
                progress,
                visible,
                rehearsed,
                ready,
            }
        })
        .collect()
}

pub fn rehearse(state: &mut GameState, departure: DepartureId) -> StoryEvent {
    if state.has_rehearsed_departure(departure) {
        return StoryEvent::new(
            "路线已经演练过",
            "这条终点路线已经在你身上留下预演的重量。再次回想，只会让车站听见那一次选择还没有散。",
        )
        .tag("路线试炼");
    }

    let missing = missing_requirements(state, departure);
    if departure.location() != state.location
        || !trial_visible(state, departure)
        || !missing.is_empty()
    {
        return StoryEvent::new(
            "路线试炼还没有入口",
            format!(
                "你想提前面对终点，但这条路线还没有足够压力。{}",
                if departure.location() != state.location {
                    format!("它不在这里，而在{}。", departure.location().title())
                } else if !trial_visible(state, departure) {
                    "它需要先完成对应路线准备。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("路线试炼");
    }

    state.rehearse_departure(departure);
    let mut event = trial_event(departure);
    apply_trial_rewards(state, departure, &mut event);
    event
}

fn trial_visible(state: &GameState, departure: DepartureId) -> bool {
    state.has_prepared_departure(departure) || state.has_rehearsed_departure(departure)
}

fn missing_requirements(state: &GameState, departure: DepartureId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_prepared_departure(departure),
        "完成对应路线准备",
    );
    require(
        &mut missing,
        state.current_segment() >= 5,
        "进入第五段午夜以后",
    );
    match departure {
        DepartureId::SingleReturnPocket => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::ScreenKeepsScore)
                    || state.has_completed_aftertalk(AftertalkId::TravelerSecondSeat),
                "处理屏幕记账异象，或回访老人和第二个座位",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
        }
        DepartureId::ChildWindowSeat => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::WhiteLineDrift)
                    || state.has_completed_aftertalk(AftertalkId::ChildDepartureSeat),
                "处理白线漂移异象，或回访孩子和 07B",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined) || state.child_trust >= 4,
                "让孩子愿意同行，或把信任推到足够高",
            );
        }
        DepartureId::TimetableMatch => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::StalledMinute)
                    || state.has_flag(Flag::SynthesizedStationTruth),
                "处理旧钟漏秒异象，或理解车站真相",
            );
            require(
                &mut missing,
                state.has_item(Item::OldTimetable) && state.has_flag(Flag::RepairedFogLamp),
                "带着旧时刻表，并修复雾灯",
            );
        }
        DepartureId::BroadcastScript => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::BroadcastFeedback)
                    || state.has_completed_aftertalk(AftertalkId::KeeperBroadcastReply),
                "处理广播回授异象，或回访站务员和广播稿",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得或听过广播磁带",
            );
        }
        DepartureId::KeeperLedger => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::StalledMinute)
                    || state.has_completed_aftertalk(AftertalkId::KeeperMinuteHand),
                "处理旧钟漏秒异象，或回访分针背面",
            );
            require(
                &mut missing,
                state.has_vow(VowId::LightWithoutDebt)
                    || state.has_flag(Flag::UnderstoodStationMechanism),
                "理解守夜机制，或写下无债之光锚点",
            );
        }
        DepartureId::LastNoticeOnPlatform => {
            require(
                &mut missing,
                state.has_resolved_anomaly(AnomalyId::BrakeLightTrial)
                    || state.has_completed_request(crate::model::StationRequestId::LastNotice),
                "处理试刹异象，或完成最后到站通知",
            );
            require(
                &mut missing,
                state.completed_requests.len() >= 3 || state.has_vow(VowId::ReadTheWholeWarning),
                "完成足够多旅客委托，或写下读完整句警告的锚点",
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

fn trial_progress(
    departure: DepartureId,
    missing_count: usize,
    visible: bool,
    rehearsed: bool,
) -> u8 {
    if rehearsed {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = requirement_count(departure);
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn requirement_count(departure: DepartureId) -> usize {
    match departure {
        DepartureId::SingleReturnPocket
        | DepartureId::ChildWindowSeat
        | DepartureId::TimetableMatch
        | DepartureId::BroadcastScript
        | DepartureId::KeeperLedger
        | DepartureId::LastNoticeOnPlatform => 4,
    }
}

fn trial_title(departure: DepartureId) -> &'static str {
    match departure.ending() {
        Ending::LostPassenger => "试炼：把湿票留给后来者",
        Ending::EscapedAlone => "试炼：独自上车前的回头",
        Ending::NewStationKeeper => "试炼：接过外套前的规矩",
        Ending::BurnedTimetable => "试炼：点火前的撤销",
        Ending::TookChildHome => "试炼：带孩子返程前的座位",
        Ending::BecameTheVoice => "试炼：广播前的第二个名字",
    }
}

fn review(departure: DepartureId) -> &'static str {
    match departure {
        DepartureId::SingleReturnPocket => "你已经预演过独自离开，知道保住姓名不等于清白。",
        DepartureId::ChildWindowSeat => "你已经预演过带孩子返程，知道 07B 不是保证，而是具体位置。",
        DepartureId::TimetableMatch => "你已经预演过烧掉时刻表，知道撤销规则也要承担余火。",
        DepartureId::BroadcastScript => "你已经预演过走进广播室，知道警告必须带着第二个名字。",
        DepartureId::KeeperLedger => "你已经预演过接过外套，知道守夜需要规矩，不是姿态。",
        DepartureId::LastNoticeOnPlatform => {
            "你已经预演过不替自己选择，知道湿票也可以成为后来者的路标。"
        }
    }
}

fn apply_trial_rewards(state: &mut GameState, departure: DepartureId, event: &mut StoryEvent) {
    match departure {
        DepartureId::SingleReturnPocket => {
            remember_tag(state, event, Flag::SynthesizedRoute, "试炼：单程回头");
        }
        DepartureId::ChildWindowSeat => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "试炼：同行座位");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        DepartureId::TimetableMatch => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "试炼：撤销规则",
            );
        }
        DepartureId::BroadcastScript => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "试炼：广播第二名",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        DepartureId::KeeperLedger => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "试炼：守夜规矩",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        DepartureId::LastNoticeOnPlatform => {
            remember_tag(state, event, Flag::SynthesizedRoute, "试炼：后来者警告");
        }
    }
    event.tags.push("路线试炼".to_string());
    event
        .tags
        .push(format!("预演：{}", departure.ending().short_title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn trial_event(departure: DepartureId) -> StoryEvent {
    let (title, body) = match departure {
        DepartureId::SingleReturnPocket => (
            "路线试炼：独自上车前的回头",
            "你站在候车厅门口，手指按住缝进票夹的姓名。雾灯号的车门在想象里打开，温暖、干燥、没有任何人责问。你几乎走进去，又在最后一秒回头看见第七张长椅。试炼没有阻止你离开，只让你明白：独自上车不是失败，但它必须承认自己留下了什么。",
        ),
        DepartureId::ChildWindowSeat => (
            "路线试炼：带孩子返程前的座位",
            "售票窗口后的座位图亮起，07B 像一小块还没被雾碰过的玻璃。孩子坐上去，又马上站起来，问如果他到站以后仍然害怕怎么办。你没有说不会，只说那就把害怕也带下车。他看着窗外，像第一次允许明天不是奖品，而是空间。",
        ),
        DepartureId::TimetableMatch => (
            "路线试炼：点火前的撤销",
            "失物招领处的旧时刻表在你掌心变轻，火柴却迟迟擦不亮。你看见许多人的车次在纸上乱跑，像害怕规则被撤销以后连借口也失去。你终于擦亮火柴，但没有立刻点燃。试炼要你承认：烧掉时刻表不是发泄，是让后来者不必再按照这张纸受伤。",
        ),
        DepartureId::BroadcastScript => (
            "路线试炼：广播前的第二个名字",
            "钟楼墙后传来广播室的空响。你把誊清的稿子读到第二个名字，喉咙像被雨水按住。磁带继续转，你没有跳过。于是广播第一次不像审判，像一条写给陌生人的路：为什么别上车，为什么别一个人上车，以及如果已经太晚，怎样还能回头。",
        ),
        DepartureId::KeeperLedger => (
            "路线试炼：接过外套前的规矩",
            "旧钟楼的木椅在你面前长出影子。外套很轻，轻得像所有留下来的理由都想伪装成慈悲。你先翻开值夜簿，在第一页写下三条规矩：灯光不欠人情，守夜不替人决定，任何明天都不得再被抵押。写完以后，外套才有了真实重量。",
        ),
        DepartureId::LastNoticeOnPlatform => (
            "路线试炼：把湿票留给后来者",
            "三号月台的风把湿票吹到白线边缘。你没有上车，也没有把不上车说成伟大。你只是把票压在灯下，让背面的句子完整露出来。后来者也许仍会害怕，也许仍会误读，但至少他们拿到的不是你的沉默，而是一张终于愿意写完整的警告。",
        ),
    };
    StoryEvent::new(title, body)
}

pub fn ending_note(ending: Ending, state: &GameState) -> Option<String> {
    let rehearsed = DepartureId::ALL.iter().copied().find(|departure| {
        departure.ending() == ending && state.has_rehearsed_departure(*departure)
    });
    rehearsed.map(|departure| {
        format!(
            "你曾完成路线试炼：{}。因此终点不是第一次抵达，而是一次你已经预演过、仍愿意承担的选择。",
            trial_title(departure)
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trial_summaries_track_ready_and_rehearsed_state() {
        let mut state = GameState::new();
        let initial = trial_summaries(&state);
        let single = initial
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single route trial should be listed");
        assert_eq!(single.status, "需预备");
        assert_eq!(single.progress, 0);

        state.prepare_departure(DepartureId::SingleReturnPocket);
        state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 4;
        state.remember(Flag::RecoveredName);
        let partial = trial_summaries(&state);
        let single = partial
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single route trial should be listed");
        assert_eq!(single.status, "缺压力");
        assert!(single.visible);
        assert!(single.progress > 0);

        state.resolve_anomaly(
            AnomalyId::ScreenKeepsScore,
            crate::model::AnomalyResponse::Stabilize,
        );
        let ready = trial_summaries(&state);
        let single = ready
            .iter()
            .find(|summary| summary.departure == DepartureId::SingleReturnPocket)
            .expect("single route trial should be listed");
        assert_eq!(single.status, "可演练");
        assert!(single.ready);

        let event = rehearse(&mut state, DepartureId::SingleReturnPocket);
        assert!(event.tags.iter().any(|tag| tag == "路线试炼"));
        assert!(state.has_rehearsed_departure(DepartureId::SingleReturnPocket));
    }

    #[test]
    fn trial_count_matches_departure_routes() {
        assert_eq!(TRIAL_COUNT, DepartureId::ALL.len());
        assert_eq!(TRIAL_COUNT, departure::DEPARTURE_COUNT);
    }
}
