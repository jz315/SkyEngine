use crate::model::{EvidenceId, Flag, GameState, Item, StationRequestId, StoryEvent, TopicId};

pub const REQUEST_COUNT: usize = 5;

#[derive(Clone, Debug)]
pub struct RequestAction {
    pub request: StationRequestId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestSummary {
    pub request: StationRequestId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub ready: bool,
}

pub fn available_requests(state: &GameState) -> Vec<RequestAction> {
    StationRequestId::ALL
        .iter()
        .copied()
        .filter(|request| !state.has_completed_request(*request))
        .filter(|request| request_visible(state, *request))
        .map(|request| {
            let missing = missing_requirements(state, request);
            RequestAction {
                request,
                label: request.label(),
                detail: if missing.is_empty() {
                    "条件齐了，可以把这件小事做完。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn request_summaries(state: &GameState) -> Vec<RequestSummary> {
    StationRequestId::ALL
        .iter()
        .copied()
        .map(|request| {
            let visible = request_visible(state, request);
            let completed = state.has_completed_request(request);
            let missing = missing_requirements(state, request);
            let ready = visible && missing.is_empty() && !completed;
            let progress = request_progress(request, missing.len(), visible, completed);
            let status = if completed {
                "已完成"
            } else if ready {
                "可完成"
            } else if visible {
                "进行中"
            } else {
                "未接取"
            };
            let detail = if completed {
                request.review().to_string()
            } else if ready {
                "这件事已经有了足够的证词和物证。完成它会改变某个人对你的信任。".to_string()
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                "继续在车站里问问题、翻找物品，某个旅客迟早会把这件小事托付给你。".to_string()
            };

            RequestSummary {
                request,
                title: request.title(),
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

pub fn complete(state: &mut GameState, request: StationRequestId) -> StoryEvent {
    if state.has_completed_request(request) {
        return StoryEvent::new(
            "委托已经办完",
            "这件小事已经被你放回车站应有的位置。再摸一次，只会摸到一枚变温的回声。",
        )
        .tag("旅客委托");
    }

    let missing = missing_requirements(state, request);
    if !request_visible(state, request) || !missing.is_empty() {
        return StoryEvent::new(
            "委托还缺条件",
            format!(
                "你想把这件事做完，但车站不接受含糊的善意。{}",
                if missing.is_empty() {
                    "它还没有真正显形。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("旅客委托");
    }

    state.complete_request(request);
    let mut event = request_event(request);
    apply_request_rewards(state, request, &mut event);
    event
}

impl StationRequestId {
    pub const ALL: [Self; REQUEST_COUNT] = [
        Self::NewspaperCorrection,
        Self::RefundLedger,
        Self::HomeworkEnvelope,
        Self::LampMaintenance,
        Self::LastNotice,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::NewspaperCorrection => "完成委托：替旧报纸补一行更正",
            Self::RefundLedger => "完成委托：把退票账补到窗口",
            Self::HomeworkEnvelope => "完成委托：把作业本页角封好",
            Self::LampMaintenance => "完成委托：给雾灯做一次维护",
            Self::LastNotice => "完成委托：写下最后一张到站通知",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::NewspaperCorrection => "旧报纸的更正栏",
            Self::RefundLedger => "退票窗口的补账",
            Self::HomeworkEnvelope => "作业本页角",
            Self::LampMaintenance => "雾灯维护记录",
            Self::LastNotice => "最后到站通知",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::NewspaperCorrection => "报纸承认旧案里还有一个孩子，老人终于不用只读空白栏。",
            Self::RefundLedger => "售票窗口补上了那枚铜筹的去向，返程不再只是一句漂亮规则。",
            Self::HomeworkEnvelope => "作业本页角被封进信封，孩子终于有一份不用反复重写的今天。",
            Self::LampMaintenance => "雾灯维护记录写上你的名字，光不再假装自己没有代价。",
            Self::LastNotice => "最后到站通知被贴上公告栏，后来者会先看到路，而不是只看到雾。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::NewspaperCorrection | Self::RefundLedger | Self::LampMaintenance => 3,
            Self::HomeworkEnvelope => 4,
            Self::LastNotice => 5,
        }
    }
}

fn request_visible(state: &GameState, request: StationRequestId) -> bool {
    match request {
        StationRequestId::NewspaperCorrection => {
            state.has_discussed(TopicId::TravelerRain)
                || state.traveler_depth >= 1
                || state.has_flag(Flag::ReadDepartureBoard)
        }
        StationRequestId::RefundLedger => {
            state.has_item(Item::CoinToken)
                || state.has_discussed(TopicId::ClerkOneWay)
                || state.clerk_depth >= 2
        }
        StationRequestId::HomeworkEnvelope => {
            state.has_flag(Flag::MetChild)
                || state.has_item(Item::ChildHomework)
                || state.has_item(Item::NameTag)
        }
        StationRequestId::LampMaintenance => {
            state.has_item(Item::LanternGlass)
                || state.has_flag(Flag::RepairedFogLamp)
                || state.has_discussed(TopicId::KeeperLantern)
        }
        StationRequestId::LastNotice => {
            state.completed_requests.len() >= 2
                || state.has_flag(Flag::SynthesizedStationTruth)
                || state.final_train_due()
        }
    }
}

fn missing_requirements(state: &GameState, request: StationRequestId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match request {
        StationRequestId::NewspaperCorrection => {
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain) || state.traveler_depth >= 1,
                "让老人谈过雨或第一枚钥匙",
            );
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
        }
        StationRequestId::RefundLedger => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.has_flag(Flag::ReadStationLog),
                "取得或读过站务日志",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkSeats)
                    || state.has_presented(EvidenceId::ClerkCoinToken)
                    || state.clerk_trust >= 3,
                "让售票员承认两个座位",
            );
        }
        StationRequestId::HomeworkEnvelope => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework),
                "取得作业本",
            );
            require(
                &mut missing,
                state.has_item(Item::NameTag) || state.has_flag(Flag::ReturnedNameTag),
                "找到或归还姓名牌",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodChildPromise)
                    || state.has_discussed(TopicId::ChildPromise)
                    || state.has_presented(EvidenceId::ChildTicket),
                "理解那句旧承诺",
            );
        }
        StationRequestId::LampMaintenance => {
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.has_item(Item::StationLog),
                "取得站务日志",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::KeeperLantern)
                    || state.has_presented(EvidenceId::KeeperStationLog)
                    || state.keeper_trust >= 2,
                "让站务员承认雾灯照见的是责任",
            );
        }
        StationRequestId::LastNotice => {
            require(
                &mut missing,
                state.completed_requests.len() >= 3,
                "完成至少三件旅客委托",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得广播室磁带或听见广播线索",
            );
            require(
                &mut missing,
                state.has_flag(Flag::SynthesizedStationTruth),
                "整理出车站真相",
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

fn request_progress(
    request: StationRequestId,
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
    let total = request.requirement_count();
    if total == 0 {
        return 0;
    }
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_request_rewards(state: &mut GameState, request: StationRequestId, event: &mut StoryEvent) {
    match request {
        StationRequestId::NewspaperCorrection => {
            state.remember(Flag::TravelerTrusted);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("信任：老人".to_string());
        }
        StationRequestId::RefundLedger => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            if state.ticket == crate::model::TicketKind::Return {
                state.remember(Flag::SynthesizedRoute);
            }
            event.tags.push("信任：售票窗口".to_string());
        }
        StationRequestId::HomeworkEnvelope => {
            state.remember(Flag::UnderstoodChildPromise);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("信任：孩子".to_string());
        }
        StationRequestId::LampMaintenance => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("信任：站务员".to_string());
        }
        StationRequestId::LastNotice => {
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("车站公告".to_string());
        }
    }
}

fn request_event(request: StationRequestId) -> StoryEvent {
    let (title, body) = match request {
        StationRequestId::NewspaperCorrection => (
            "旅客委托：旧报纸的更正栏",
            "你把时刻表的空白、镜片里的水痕和老人说过的雨写进报纸边角。报纸没有变新，只是在六年前那条旧闻旁多出一行更正：失踪者不止一名。老人读完后很久没有翻页。那一刻，你知道有些事实不负责宽恕，只负责不再让人独自背负空白。",
        ),
        StationRequestId::RefundLedger => (
            "旅客委托：退票窗口的补账",
            "你把铜筹放在账册缺口上，又把站务日志压在旁边。售票员终于补上一行：07A 与 07B 不是两个座位号，是两个人互相承认过的离站资格。她盖章时没有说谢谢，只把绿灯调亮一点，像给某个迟到太久的人留了一条队伍。",
        ),
        StationRequestId::HomeworkEnvelope => (
            "旅客委托：作业本页角",
            "你用姓名牌的裂纹压住作业本页角，把那张反复写不完的题目封进旧信封。孩子没有马上接，他先问：这次封起来以后，明天会不会真的收？你说不知道，但至少今天不会再被午夜翻回第一页。他把信封抱到胸前，像抱住一小块不需要证明的现在。",
        ),
        StationRequestId::LampMaintenance => (
            "旅客委托：雾灯维护记录",
            "你在站务日志背面写下雾灯维护记录：玻璃已补，光线偏冷，照见责任时请勿谎称天气。站务员看了很久，低声说这句备注太不专业。可他没有划掉。他只是把记录夹回钟楼抽屉，像承认灯不只为别人亮，也会照见守灯的人。",
        ),
        StationRequestId::LastNotice => (
            "旅客委托：最后到站通知",
            "你把最后一张到站通知贴上公告栏。它没有写结局，只写：如果你读到这里，请先确认自己带走的是人，不是解释；留下的是灯，不是债。广播沉默几秒，随后把这句话读给整座车站听。雾没有散开，但雾里第一次有了可以慢慢走的边界。",
        ),
    };
    StoryEvent::new(title, body).tag("旅客委托").tag("支线")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_summaries_track_ready_and_completed_state() {
        let mut state = GameState::new();
        let hidden = request_summaries(&state);
        let correction = hidden
            .iter()
            .find(|summary| summary.request == StationRequestId::NewspaperCorrection)
            .expect("newspaper correction should be listed");
        assert_eq!(correction.status, "未接取");

        state.discuss(TopicId::TravelerRain);
        state.remember(Flag::ReadDepartureBoard);
        let partial = request_summaries(&state);
        let correction = partial
            .iter()
            .find(|summary| summary.request == StationRequestId::NewspaperCorrection)
            .expect("newspaper correction should be listed");
        assert_eq!(correction.status, "进行中");
        assert!(correction.progress > 0);

        state.add_item(Item::MirrorShard);
        let ready = request_summaries(&state);
        let correction = ready
            .iter()
            .find(|summary| summary.request == StationRequestId::NewspaperCorrection)
            .expect("newspaper correction should be listed");
        assert_eq!(correction.status, "可完成");
        assert!(correction.ready);

        let event = complete(&mut state, StationRequestId::NewspaperCorrection);
        assert!(event.tags.iter().any(|tag| tag == "旅客委托"));
        let completed = request_summaries(&state);
        let correction = completed
            .iter()
            .find(|summary| summary.request == StationRequestId::NewspaperCorrection)
            .expect("newspaper correction should be listed");
        assert_eq!(correction.status, "已完成");
        assert!(state.has_flag(Flag::TravelerTrusted));
    }
}
