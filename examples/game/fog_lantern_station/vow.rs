use crate::model::{
    CaseFileId, EvidenceId, Flag, GameState, Item, ResonanceId, StationRequestId, StoryEvent,
    TicketKind, TopicId, VowId,
};

#[cfg(test)]
use crate::{case_file, resonance, station_request};

pub const VOW_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct VowAction {
    pub vow: VowId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VowSummary {
    pub vow: VowId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub chosen: bool,
    pub ready: bool,
}

pub fn available_vows(state: &GameState) -> Vec<VowAction> {
    VowId::ALL
        .iter()
        .copied()
        .filter(|vow| !state.has_vow(*vow))
        .filter(|vow| vow_visible(state, *vow))
        .map(|vow| {
            let missing = missing_requirements(state, vow);
            VowAction {
                vow,
                label: vow.label(),
                detail: if missing.is_empty() {
                    "你已经知道得够多，可以把这一点写成自己接下来要承担的立场。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn vow_summaries(state: &GameState) -> Vec<VowSummary> {
    VowId::ALL
        .iter()
        .copied()
        .map(|vow| {
            let chosen = state.has_vow(vow);
            let visible = chosen || vow_visible(state, vow);
            let missing = missing_requirements(state, vow);
            let ready = visible && missing.is_empty() && !chosen;
            let progress = vow_progress(vow, missing.len(), visible, chosen);
            let status = if chosen {
                "已写下"
            } else if ready {
                "可选择"
            } else if visible {
                "未成形"
            } else {
                "未触及"
            };
            let detail = if chosen {
                vow.review().to_string()
            } else if ready {
                "这不是答案，而是你愿意承担答案后果的方式。写下后，它会进入终局余波。".to_string()
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                "继续自由对话、证据追问、人物共鸣和旅客委托，某些立场才会变得足够具体。".to_string()
            };

            VowSummary {
                vow,
                title: vow.title(),
                status,
                detail,
                progress,
                visible,
                chosen,
                ready,
            }
        })
        .collect()
}

pub fn make(state: &mut GameState, vow: VowId) -> StoryEvent {
    if state.has_vow(vow) {
        return StoryEvent::new(
            "锚点已经写下",
            "你已经把这句话写进自己身上。重复它不会让它更正确，只会提醒你：真正要重复的是行动。",
        )
        .tag("内心锚点");
    }

    let missing = missing_requirements(state, vow);
    if !vow_visible(state, vow) || !missing.is_empty() {
        return StoryEvent::new(
            "锚点还没有重量",
            format!(
                "你试着把某个念头写下来，纸面却太轻，压不住雾。{}",
                if missing.is_empty() {
                    "这句话还没有被今晚承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("内心锚点");
    }

    state.make_vow(vow);
    let mut event = vow_event(vow);
    apply_vow_rewards(state, vow, &mut event);
    event
}

pub fn vow_titles(state: &GameState) -> Vec<&'static str> {
    VowId::ALL
        .iter()
        .copied()
        .filter(|vow| state.has_vow(*vow))
        .map(VowId::title)
        .collect()
}

impl VowId {
    pub const ALL: [Self; VOW_COUNT] = [
        Self::ReadTheWholeWarning,
        Self::DoNotOwnTheChild,
        Self::ReturnWithoutErasing,
        Self::TruthBeforeMercy,
        Self::LightWithoutDebt,
        Self::OrdinaryTomorrow,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::ReadTheWholeWarning => "写下锚点：读完整个警告",
            Self::DoNotOwnTheChild => "写下锚点：孩子不属于我的悔恨",
            Self::ReturnWithoutErasing => "写下锚点：离开不能擦掉留下",
            Self::TruthBeforeMercy => "写下锚点：真相先于慈悲",
            Self::LightWithoutDebt => "写下锚点：灯光不能变成债",
            Self::OrdinaryTomorrow => "写下锚点：允许明天普通",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ReadTheWholeWarning => "读完整个警告",
            Self::DoNotOwnTheChild => "孩子不属于我的悔恨",
            Self::ReturnWithoutErasing => "离开不能擦掉留下",
            Self::TruthBeforeMercy => "真相先于慈悲",
            Self::LightWithoutDebt => "灯光不能变成债",
            Self::OrdinaryTomorrow => "允许明天普通",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::ReadTheWholeWarning => "你承认“别上车”不是禁令，而是一句被你剪短太久的请求。",
            Self::DoNotOwnTheChild => "你承认孩子不是免罪材料。他可以同行，也可以带着害怕离开。",
            Self::ReturnWithoutErasing => "你承认离开不是清白证明；返程要带走人，也要留下事实。",
            Self::TruthBeforeMercy => "你承认善意若拒绝看见代价，就会慢慢长成新的机关。",
            Self::LightWithoutDebt => "你承认照路不是要求别人感谢的理由。灯光必须服务具体的人。",
            Self::OrdinaryTomorrow => {
                "你承认明天不必宏大。它可以只是热牛奶、靠窗座位和不被改写的清晨。"
            }
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::ReadTheWholeWarning | Self::ReturnWithoutErasing => 3,
            Self::DoNotOwnTheChild
            | Self::TruthBeforeMercy
            | Self::LightWithoutDebt
            | Self::OrdinaryTomorrow => 4,
        }
    }
}

fn vow_visible(state: &GameState, vow: VowId) -> bool {
    match vow {
        VowId::ReadTheWholeWarning => {
            state.has_flag(Flag::ExaminedTicket)
                || state.has_discussed(TopicId::TravelerRain)
                || state.has_resolved_resonance(ResonanceId::RainInTheMirror)
        }
        VowId::DoNotOwnTheChild => {
            state.has_flag(Flag::MetChild)
                || state.has_item(Item::ChildHomework)
                || state.has_item(Item::NameTag)
                || state.has_resolved_resonance(ResonanceId::WhiteLineHomework)
        }
        VowId::ReturnWithoutErasing => {
            state.has_item(Item::CoinToken)
                || state.ticket == TicketKind::Return
                || state.has_resolved_case_file(CaseFileId::ReturnProtocol)
                || state.has_resolved_resonance(ResonanceId::TwoReservedSeats)
        }
        VowId::TruthBeforeMercy => {
            state.has_item(Item::StationLog)
                || state.has_flag(Flag::HeardClockTruth)
                || state.has_resolved_case_file(CaseFileId::BorrowedMinute)
                || state.has_resolved_resonance(ResonanceId::BorrowedMinute)
        }
        VowId::LightWithoutDebt => {
            state.completed_requests.len() >= 1
                || state.has_flag(Flag::RepairedFogLamp)
                || state.has_discussed(TopicId::KeeperStay)
                || state.has_resolved_resonance(ResonanceId::DebtOfKeepingWatch)
        }
        VowId::OrdinaryTomorrow => {
            state.has_flag(Flag::MetChild)
                || state.has_flag(Flag::ChildJoined)
                || state.has_completed_request(StationRequestId::HomeworkEnvelope)
                || state.has_discussed(TopicId::ChildTomorrow)
        }
    }
}

fn missing_requirements(state: &GameState, vow: VowId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match vow {
        VowId::ReadTheWholeWarning => {
            require(
                &mut missing,
                state.has_flag(Flag::ExaminedTicket),
                "检查湿票",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard) || state.has_item(Item::MirrorShard),
                "读电子时刻表或取得候车厅镜片",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodFirstLoop)
                    || state.has_resolved_resonance(ResonanceId::RainInTheMirror),
                "理解第一次循环，或触发雨声与镜片的共鸣",
            );
        }
        VowId::DoNotOwnTheChild => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodChildPromise)
                    || state.has_presented(EvidenceId::ChildTicket)
                    || state.has_resolved_resonance(ResonanceId::WhiteLineHomework),
                "理解湿票里的第二个名字",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework) || state.has_discussed(TopicId::ChildHomework),
                "看过作业本",
            );
            require(
                &mut missing,
                state.child_trust >= 3 || state.has_discussed(TopicId::ChildAnger),
                "让孩子对你有足够具体的信任或愤怒",
            );
        }
        VowId::ReturnWithoutErasing => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken) || state.ticket == TicketKind::Return,
                "取得退票铜筹或改签返程票",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_resolved_case_file(CaseFileId::ReturnProtocol)
                    || state.has_resolved_resonance(ResonanceId::TwoReservedSeats)
                    || state.has_discussed(TopicId::ClerkSeats),
                "整理过返程协议、两个座位或窗口证词",
            );
        }
        VowId::TruthBeforeMercy => {
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
                state.has_flag(Flag::UnderstoodStationMechanism)
                    || state.has_resolved_case_file(CaseFileId::BorrowedMinute)
                    || state.has_resolved_resonance(ResonanceId::BorrowedMinute),
                "理解车站机制或借来的最后一分钟",
            );
            require(
                &mut missing,
                state.resolved_case_files.len() >= 1 || state.resolved_resonances.len() >= 1,
                "至少归档一份真相或触发一组人物共鸣",
            );
        }
        VowId::LightWithoutDebt => {
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.completed_requests.len() >= 2,
                "完成至少两件旅客委托",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::KeeperStay)
                    || state.has_resolved_resonance(ResonanceId::DebtOfKeepingWatch)
                    || state.keeper_trust >= 3,
                "让站务员或共鸣说明留下来的危险",
            );
            require(
                &mut missing,
                state.has_resolved_case_file(CaseFileId::KeeperContract)
                    || state.has_flag(Flag::SynthesizedStationTruth)
                    || state.resolved_case_files.len() >= 2,
                "整理过外套契约或车站真相",
            );
        }
        VowId::OrdinaryTomorrow => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.child_trust >= 4 || state.has_flag(Flag::ChildJoined),
                "让孩子愿意把问题交给你一部分",
            );
            require(
                &mut missing,
                state.has_completed_request(StationRequestId::HomeworkEnvelope)
                    || state.has_item(Item::ChildHomework),
                "完成作业本页角委托或取得作业本",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ChildTomorrow)
                    || state.has_flag(Flag::SynthesizedChildTruth)
                    || state.has_resolved_resonance(ResonanceId::WhiteLineHomework),
                "让明天变成可以说出口的具体愿望",
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

fn vow_progress(vow: VowId, missing_count: usize, visible: bool, chosen: bool) -> u8 {
    if chosen {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = vow.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_vow_rewards(state: &mut GameState, vow: VowId, event: &mut StoryEvent) {
    match vow {
        VowId::ReadTheWholeWarning => {
            state.remember(Flag::UnderstoodFirstLoop);
            state.remember(Flag::TravelerTrusted);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("锚点：湿票".to_string());
        }
        VowId::DoNotOwnTheChild => {
            state.remember(Flag::UnderstoodChildPromise);
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("锚点：孩子".to_string());
        }
        VowId::ReturnWithoutErasing => {
            state.remember(Flag::SynthesizedRoute);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("锚点：返程".to_string());
        }
        VowId::TruthBeforeMercy => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("锚点：真相".to_string());
        }
        VowId::LightWithoutDebt => {
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("锚点：雾灯".to_string());
        }
        VowId::OrdinaryTomorrow => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("锚点：明天".to_string());
        }
    }
}

fn vow_event(vow: VowId) -> StoryEvent {
    let (title, body) = match vow {
        VowId::ReadTheWholeWarning => (
            "内心锚点：读完整个警告",
            "你在湿票背面补上那半句：别一个人上车。字很小，却比广播更难否认。你终于承认自己过去不是没有看见请求，而是一次次把请求剪成比较容易反抗的禁令。",
        ),
        VowId::DoNotOwnTheChild => (
            "内心锚点：孩子不属于我的悔恨",
            "你把孩子的名字单独写在一页上，没有写在你的忏悔后面。他不是你故事里的证据，也不是奖赏。他可以选择离开，也可以选择继续害怕你。你的任务不是让他成全你，是不再省略他。",
        ),
        VowId::ReturnWithoutErasing => (
            "内心锚点：离开不能擦掉留下",
            "你写下：如果我走，也要让这里知道谁曾被留下。返程票不能把错误改成没发生，它只能证明你终于愿意带着事实走，而不是把事实交给雾灯站继续保管。",
        ),
        VowId::TruthBeforeMercy => (
            "内心锚点：真相先于慈悲",
            "你不再把温柔当作遮布。慈悲如果拒绝说出谁签了字、谁沉默、谁因此继续等候，就会变成另一种漂亮的命令。你写下这句时，旧钟轻轻顿了一下。",
        ),
        VowId::LightWithoutDebt => (
            "内心锚点：灯光不能变成债",
            "你把修灯记录和旅客委托放在一起，写下：照路不是索取感谢的理由。雾灯若要继续亮，就必须让后来者看清路，而不是逼他们永远承认有人替他们守过夜。",
        ),
        VowId::OrdinaryTomorrow => (
            "内心锚点：允许明天普通",
            "你写下的明天没有壮丽词语，只有靠窗座位、热牛奶、干净作业纸和一个不必证明自己被记得的早晨。普通不是变轻，普通是伤口终于不用负责定义全部人生。",
        ),
    };
    StoryEvent::new(title, body).tag("内心锚点").tag("立场选择")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let titles = vow_titles(state);
    if titles.is_empty() {
        return None;
    }

    let joined = titles.join(" / ");
    Some(format!(
        "你带到终点的内心锚点是：{joined}。它们不替你做选择，却让选择不再只是逃离、抵账或牺牲的旧词。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vow_summaries_track_ready_and_chosen_state() {
        let mut state = GameState::new();
        let initial = vow_summaries(&state);
        let warning = initial
            .iter()
            .find(|summary| summary.vow == VowId::ReadTheWholeWarning)
            .expect("warning vow should be listed");
        assert_eq!(warning.status, "未触及");
        assert_eq!(warning.progress, 0);

        state.remember(Flag::ExaminedTicket);
        state.remember(Flag::ReadDepartureBoard);
        let partial = vow_summaries(&state);
        let warning = partial
            .iter()
            .find(|summary| summary.vow == VowId::ReadTheWholeWarning)
            .expect("warning vow should be listed");
        assert_eq!(warning.status, "未成形");
        assert!(warning.visible);
        assert!(warning.progress > 0);

        state.resolve_resonance(ResonanceId::RainInTheMirror);
        let ready = vow_summaries(&state);
        let warning = ready
            .iter()
            .find(|summary| summary.vow == VowId::ReadTheWholeWarning)
            .expect("warning vow should be listed");
        assert_eq!(warning.status, "可选择");
        assert!(warning.ready);

        let event = make(&mut state, VowId::ReadTheWholeWarning);
        assert!(event.tags.iter().any(|tag| tag == "立场选择"));
        assert!(state.has_vow(VowId::ReadTheWholeWarning));
        assert!(state.has_flag(Flag::TravelerTrusted));
    }

    #[test]
    fn vow_count_stays_aligned_with_related_progression_systems() {
        assert_eq!(VOW_COUNT, VowId::ALL.len());
        assert!(VOW_COUNT <= case_file::CASE_FILE_COUNT + resonance::RESONANCE_COUNT);
        assert!(station_request::REQUEST_COUNT > 0);
    }
}
