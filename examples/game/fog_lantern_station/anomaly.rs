use crate::model::{
    AnomalyId, AnomalyResponse, DepartureId, EvidenceId, Flag, GameState, Item, Location, MemoryId,
    PatrolId, StoryEvent, TopicId, VowId,
};

pub const ANOMALY_COUNT: usize = 7;

#[derive(Clone, Debug)]
pub struct AnomalyAction {
    pub anomaly: AnomalyId,
    pub response: AnomalyResponse,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnomalySummary {
    pub anomaly: AnomalyId,
    pub title: &'static str,
    pub status: String,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub resolved: bool,
    pub ready: bool,
}

pub fn available_anomalies(state: &GameState) -> Vec<AnomalyAction> {
    AnomalyId::ALL
        .iter()
        .copied()
        .filter(|anomaly| !state.has_resolved_anomaly(*anomaly))
        .filter(|anomaly| anomaly.location() == state.location)
        .filter(|anomaly| anomaly_visible(state, *anomaly))
        .flat_map(|anomaly| {
            [AnomalyResponse::Stabilize, AnomalyResponse::Follow]
                .into_iter()
                .map(move |response| {
                    let missing = missing_requirements(state, anomaly, response);
                    AnomalyAction {
                        anomaly,
                        response,
                        label: format!("{}：{}", response.name(), anomaly.short_label()),
                        detail: if missing.is_empty() {
                            anomaly.response_detail(response).to_string()
                        } else {
                            format!("还缺：{}。", missing.join("；"))
                        },
                        enabled: missing.is_empty(),
                    }
                })
        })
        .collect()
}

pub fn anomaly_summaries(state: &GameState) -> Vec<AnomalySummary> {
    AnomalyId::ALL
        .iter()
        .copied()
        .map(|anomaly| {
            let response = state.anomaly_response(anomaly);
            let resolved = response.is_some();
            let visible = resolved || anomaly_visible(state, anomaly);
            let stabilize_missing =
                missing_requirements(state, anomaly, AnomalyResponse::Stabilize);
            let follow_missing = missing_requirements(state, anomaly, AnomalyResponse::Follow);
            let ready =
                visible && !resolved && (stabilize_missing.is_empty() || follow_missing.is_empty());
            let progress = anomaly_progress(
                anomaly,
                stabilize_missing.len().min(follow_missing.len()),
                visible,
                resolved,
            );
            let status = if let Some(response) = response {
                format!("已{}", response.name().trim_end_matches("异象"))
            } else if ready {
                "可处理".to_string()
            } else if visible {
                "待补证".to_string()
            } else {
                "未显形".to_string()
            };
            let detail = if let Some(response) = response {
                format!(
                    "{}。处理方式：{}。",
                    anomaly.review(response),
                    response.name()
                )
            } else if ready {
                format!(
                    "{}正在{}出现。你可以选择稳住它，或追随它。",
                    anomaly.title(),
                    anomaly.location().title()
                )
            } else if visible {
                let mut parts = Vec::new();
                if !stabilize_missing.is_empty() {
                    parts.push(format!("稳住还缺：{}", stabilize_missing.join("；")));
                }
                if !follow_missing.is_empty() {
                    parts.push(format!("追随还缺：{}", follow_missing.join("；")));
                }
                parts.join("。")
            } else {
                format!(
                    "这场异象会在第 {} 段午夜以后显形，地点是{}。",
                    anomaly.segment(),
                    anomaly.location().title()
                )
            };

            AnomalySummary {
                anomaly,
                title: anomaly.title(),
                status,
                detail,
                progress,
                visible,
                resolved,
                ready,
            }
        })
        .collect()
}

pub fn handle(state: &mut GameState, anomaly: AnomalyId, response: AnomalyResponse) -> StoryEvent {
    if let Some(existing) = state.anomaly_response(anomaly) {
        return StoryEvent::new(
            "异象已经被处理",
            format!(
                "这场异象已经被你{}过。车站没有重演，只把那一次选择留在墙面水痕里。",
                existing.name()
            ),
        )
        .tag("车站异象");
    }

    let missing = missing_requirements(state, anomaly, response);
    if anomaly.location() != state.location
        || !anomaly_visible(state, anomaly)
        || !missing.is_empty()
    {
        return StoryEvent::new(
            "异象还没有被你接住",
            format!(
                "你感觉这一段午夜正在拧紧，但手里还缺少能让它成形的东西。{}",
                if anomaly.location() != state.location {
                    format!("它不在这里，而在{}。", anomaly.location().title())
                } else if !anomaly_visible(state, anomaly) {
                    format!("它要到第 {} 段午夜以后才会显形。", anomaly.segment())
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("车站异象");
    }

    state.resolve_anomaly(anomaly, response);
    let mut event = anomaly_event(anomaly, response);
    apply_anomaly_rewards(state, anomaly, response, &mut event);
    event
}

impl AnomalyId {
    pub const ALL: [Self; ANOMALY_COUNT] = [
        Self::ScreenKeepsScore,
        Self::RefundStampede,
        Self::RisingWaterline,
        Self::WhiteLineDrift,
        Self::StalledMinute,
        Self::BroadcastFeedback,
        Self::BrakeLightTrial,
    ];

    pub fn segment(self) -> u8 {
        match self {
            Self::ScreenKeepsScore => 2,
            Self::RefundStampede => 3,
            Self::RisingWaterline => 4,
            Self::WhiteLineDrift => 5,
            Self::StalledMinute => 6,
            Self::BroadcastFeedback => 7,
            Self::BrakeLightTrial => 8,
        }
    }

    pub fn location(self) -> Location {
        match self {
            Self::ScreenKeepsScore => Location::WaitingHall,
            Self::RefundStampede => Location::TicketOffice,
            Self::RisingWaterline => Location::Underpass,
            Self::WhiteLineDrift => Location::Platform,
            Self::StalledMinute | Self::BroadcastFeedback => Location::ClockTower,
            Self::BrakeLightTrial => Location::Platform,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::ScreenKeepsScore => "屏幕开始记账",
            Self::RefundStampede => "退票队列逆流",
            Self::RisingWaterline => "地下水线升高",
            Self::WhiteLineDrift => "月台白线漂移",
            Self::StalledMinute => "旧钟漏下一秒",
            Self::BroadcastFeedback => "广播室回授",
            Self::BrakeLightTrial => "雾灯号试刹",
        }
    }

    fn short_label(self) -> &'static str {
        match self {
            Self::ScreenKeepsScore => "屏幕记账",
            Self::RefundStampede => "退票逆流",
            Self::RisingWaterline => "水线升高",
            Self::WhiteLineDrift => "白线漂移",
            Self::StalledMinute => "旧钟漏秒",
            Self::BroadcastFeedback => "广播回授",
            Self::BrakeLightTrial => "试刹雾灯号",
        }
    }

    fn response_detail(self, response: AnomalyResponse) -> &'static str {
        match (self, response) {
            (Self::ScreenKeepsScore, AnomalyResponse::Stabilize) => {
                "把屏幕里的行动逐条核对，阻止车站把它们改写成债。"
            }
            (Self::ScreenKeepsScore, AnomalyResponse::Follow) => {
                "跟着屏幕闪烁的缺口走，让它暴露下一条被删掉的记录。"
            }
            (Self::RefundStampede, AnomalyResponse::Stabilize) => {
                "按住退票队列，替每张票补上同行栏。"
            }
            (Self::RefundStampede, AnomalyResponse::Follow) => {
                "顺着逆流退票走进窗口背面，看单程票最早如何成形。"
            }
            (Self::RisingWaterline, AnomalyResponse::Stabilize) => {
                "用雾灯玻璃压住水线，让回声把姓名留下。"
            }
            (Self::RisingWaterline, AnomalyResponse::Follow) => {
                "让水线漫过脚踝，跟着最深处那道孩子身高的痕迹走。"
            }
            (Self::WhiteLineDrift, AnomalyResponse::Stabilize) => {
                "把白线重新描回月台边缘，先保护等待的人。"
            }
            (Self::WhiteLineDrift, AnomalyResponse::Follow) => {
                "跟着漂移的白线走，看它究竟想把孩子带去哪里。"
            }
            (Self::StalledMinute, AnomalyResponse::Stabilize) => {
                "按住旧钟漏下的一秒，逼它承认借来的时间要归还。"
            }
            (Self::StalledMinute, AnomalyResponse::Follow) => {
                "钻进那一秒钟的缝里，看站务员第一次签字时隐藏了什么。"
            }
            (Self::BroadcastFeedback, AnomalyResponse::Stabilize) => {
                "把广播回授降下来，让警告重新能被人听懂。"
            }
            (Self::BroadcastFeedback, AnomalyResponse::Follow) => {
                "沿着回授里的第二个名字走向窄门后方。"
            }
            (Self::BrakeLightTrial, AnomalyResponse::Stabilize) => {
                "站在白线前稳住试刹，让最终选择不被恐惧提前替你做完。"
            }
            (Self::BrakeLightTrial, AnomalyResponse::Follow) => {
                "追着车灯进入雾里，提前看一眼每条路线的代价。"
            }
        }
    }

    fn requirement_count(self, response: AnomalyResponse) -> usize {
        match (self, response) {
            (Self::ScreenKeepsScore, AnomalyResponse::Stabilize) => 2,
            (Self::ScreenKeepsScore, AnomalyResponse::Follow) => 3,
            _ => 3,
        }
    }

    fn review(self, response: AnomalyResponse) -> &'static str {
        match (self, response) {
            (Self::ScreenKeepsScore, AnomalyResponse::Stabilize) => {
                "电子屏的账目被你按住，行动暂时不再被车站改写成单向债务"
            }
            (Self::ScreenKeepsScore, AnomalyResponse::Follow) => {
                "你追进电子屏缺口，看见被删掉的第二个座位闪了一下"
            }
            (Self::RefundStampede, AnomalyResponse::Stabilize) => {
                "退票队列被重新排好，每张单程票旁边都多出同行栏"
            }
            (Self::RefundStampede, AnomalyResponse::Follow) => {
                "你跟着逆流进入窗口背面，看见第一张单程票如何逃避了第二个名字"
            }
            (Self::RisingWaterline, AnomalyResponse::Stabilize) => {
                "地下水线被压回墙砖，回声留下了姓名"
            }
            (Self::RisingWaterline, AnomalyResponse::Follow) => {
                "你跟着最高水线走过通道，白线旧案有了孩子肩膀的高度"
            }
            (Self::WhiteLineDrift, AnomalyResponse::Stabilize) => {
                "漂移的白线被描回原位，孩子的等待先被保护下来"
            }
            (Self::WhiteLineDrift, AnomalyResponse::Follow) => {
                "你跟着白线看见它真正指向的不是轨道，而是同行的手"
            }
            (Self::StalledMinute, AnomalyResponse::Stabilize) => {
                "旧钟漏下的一秒被按回分针，站务员的借口少了一层"
            }
            (Self::StalledMinute, AnomalyResponse::Follow) => {
                "你钻进漏秒的缝里，看见申请停住午夜时被藏起的代价"
            }
            (Self::BroadcastFeedback, AnomalyResponse::Stabilize) => {
                "广播回授被降下来，警告重新变成人能听懂的句子"
            }
            (Self::BroadcastFeedback, AnomalyResponse::Follow) => {
                "你沿着回授找到第二个名字，广播室不再只会重复禁令"
            }
            (Self::BrakeLightTrial, AnomalyResponse::Stabilize) => {
                "试刹声被稳住，最终选择不会被恐惧提前盖章"
            }
            (Self::BrakeLightTrial, AnomalyResponse::Follow) => {
                "你追着车灯看见各条路线的影子，终点开始变得具体"
            }
        }
    }
}

fn anomaly_visible(state: &GameState, anomaly: AnomalyId) -> bool {
    state.current_segment() >= anomaly.segment()
}

fn missing_requirements(
    state: &GameState,
    anomaly: AnomalyId,
    response: AnomalyResponse,
) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match (anomaly, response) {
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
            require(
                &mut missing,
                state.discussed_topics.len() >= 1,
                "完成至少一次自由追问",
            );
        }
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
            require(
                &mut missing,
                state.has_item(Item::MirrorShard),
                "取得候车厅镜片",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain) || state.traveler_depth >= 1,
                "让老人谈过雨夜",
            );
        }
        (AnomalyId::RefundStampede, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(&mut missing, state.clerk_depth >= 2, "听售票员讲过退票规则");
            require(
                &mut missing,
                state.has_item(Item::StationLog) || state.has_flag(Flag::ReadStationLog),
                "取得或读过站务日志",
            );
        }
        (AnomalyId::RefundStampede, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkOneWay)
                    || state.has_discussed(TopicId::ClerkSeats),
                "追问过单程票或两个座位",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::TicketWindowReflection)
                    || state.has_completed_patrol(PatrolId::TicketWindowQueue),
                "走过窗口记忆或排查退票队列",
            );
        }
        (AnomalyId::RisingWaterline, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardUnderpassEcho),
                "听过地下回声",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_item(Item::LanternGlass) || state.has_flag(Flag::RepairedFogLamp),
                "带着雾灯玻璃或修好雾灯",
            );
        }
        (AnomalyId::RisingWaterline, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::EvacuationLine)
                    || state.has_completed_patrol(PatrolId::UnderpassWaterline),
                "走过疏散记忆或量过水线",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework) || state.has_flag(Flag::MetChild),
                "见过孩子或作业本",
            );
        }
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework),
                "拿到作业本",
            );
            require(&mut missing, state.child_trust >= 2, "让孩子开始信任你");
        }
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::WhiteLineMeasure)
                    || state.has_completed_patrol(PatrolId::PlatformBoundary),
                "走过白线记忆或重描月台白线",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ChildPromise) || state.child_trust >= 3,
                "谈过承诺或取得更多信任",
            );
        }
        (AnomalyId::StalledMinute, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth),
                "听过旧钟代价",
            );
            require(&mut missing, state.has_item(Item::BrassKey), "带着黄铜钥匙");
            require(
                &mut missing,
                state.has_item(Item::StationLog),
                "取得站务日志",
            );
        }
        (AnomalyId::StalledMinute, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_flag(Flag::HeardClockTruth),
                "听过旧钟代价",
            );
            require(
                &mut missing,
                state.has_memory(MemoryId::BorrowedClockMinute)
                    || state.has_completed_patrol(PatrolId::ClockTowerMinuteHand),
                "走过旧钟记忆或擦亮分针背面",
            );
            require(
                &mut missing,
                state.has_vow(VowId::TruthBeforeMercy) || state.keeper_trust >= 3,
                "写下真相锚点或取得站务员信任",
            );
        }
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape),
                "取得或听过广播磁带",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(&mut missing, state.has_flag(Flag::AlignedClock), "校准旧钟");
        }
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_memory(MemoryId::BroadcastPractice)
                    || state.has_prepared_departure(DepartureId::BroadcastScript),
                "走过广播练习室记忆或誊清广播稿",
            );
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape)
                    || state.has_presented(EvidenceId::KeeperBroadcastTape),
                "取得广播磁带或向站务员出示磁带",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RecoveredName),
                "找回自己的姓名",
            );
        }
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Stabilize) => {
            require(
                &mut missing,
                state.has_flag(Flag::InspectedRails),
                "检查过轨道尽头",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ChildJoined) || state.has_flag(Flag::RecoveredName),
                "找回姓名或让孩子同行",
            );
            require(
                &mut missing,
                state.resolved_anomalies.len() >= 2,
                "处理过至少两场异象",
            );
        }
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Follow) => {
            require(
                &mut missing,
                state.has_flag(Flag::InspectedRails),
                "检查过轨道尽头",
            );
            require(
                &mut missing,
                state.prepared_departures.len() >= 1,
                "完成至少一件路线准备",
            );
            require(
                &mut missing,
                state.synthesis_depth >= 5,
                "完成前五次回想整理",
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

fn anomaly_progress(anomaly: AnomalyId, missing_count: usize, visible: bool, resolved: bool) -> u8 {
    if resolved {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = anomaly
        .requirement_count(AnomalyResponse::Stabilize)
        .min(anomaly.requirement_count(AnomalyResponse::Follow));
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_anomaly_rewards(
    state: &mut GameState,
    anomaly: AnomalyId,
    response: AnomalyResponse,
    event: &mut StoryEvent,
) {
    match (anomaly, response) {
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::SynthesizedRoute, "异象：行动留痕");
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Follow) => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "异象：删改记录");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (AnomalyId::RefundStampede, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::SynthesizedRoute, "异象：返程规则");
            state.clerk_trust = (state.clerk_trust + 2).min(5);
        }
        (AnomalyId::RefundStampede, AnomalyResponse::Follow) => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "异象：同行栏");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (AnomalyId::RisingWaterline, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "异象：姓名留痕");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        (AnomalyId::RisingWaterline, AnomalyResponse::Follow) => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "异象：旧案高度",
            );
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::UnderstoodChildPromise, "异象：白线保护");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Follow) => {
            remember_tag(state, event, Flag::SynthesizedChildTruth, "异象：白线方向");
            state.child_trust = (state.child_trust + 2).min(5);
        }
        (AnomalyId::StalledMinute, AnomalyResponse::Stabilize) => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "异象：漏秒归还",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        (AnomalyId::StalledMinute, AnomalyResponse::Follow) => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "异象：最后一分钟",
            );
            state.keeper_trust = (state.keeper_trust + 2).min(5);
        }
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::HeardBroadcastTape, "异象：广播清晰");
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "异象：警告成形",
            );
        }
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Follow) => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "异象：第二个名字",
            );
            state.child_trust = (state.child_trust + 1).min(5);
        }
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Stabilize) => {
            remember_tag(state, event, Flag::SynthesizedRoute, "异象：选择稳住");
            event.tags.push("终段稳定".to_string());
        }
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Follow) => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "异象：路线代价",
            );
            event.tags.push("终段预视".to_string());
        }
    }
    event.tags.push("车站异象".to_string());
    event.tags.push(response.name().to_string());
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn anomaly_event(anomaly: AnomalyId, response: AnomalyResponse) -> StoryEvent {
    let (title, body) = match (anomaly, response) {
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Stabilize) => (
            "异象：稳住屏幕记账",
            "电子屏把你的行动一条条吐出来，又试图在每条后面加上欠款。你站到屏幕前，把能核对的事实逐项念出。念到第三条时，屏幕短暂黑掉，像终于承认行动不是债务，也可以是证词。",
        ),
        (AnomalyId::ScreenKeepsScore, AnomalyResponse::Follow) => (
            "异象：追随屏幕缺口",
            "你没有按住屏幕，而是盯着它擦掉文字的瞬间。缺口里闪过第七排长椅、两个座位和一个被水泡软的名字。你明白车站删掉的不是证据，是那些会迫使你把自己看成参与者的细节。",
        ),
        (AnomalyId::RefundStampede, AnomalyResponse::Stabilize) => (
            "异象：稳住退票逆流",
            "售票窗口前忽然排起一列倒着走的人，每个人都把票递回来，又把同行栏遮住。你用铜筹压住队首，把同行栏逐张补齐。票章声慢下来，售票员在玻璃后轻轻吸了一口气。",
        ),
        (AnomalyId::RefundStampede, AnomalyResponse::Follow) => (
            "异象：追随退票逆流",
            "你顺着倒退的人群走进窗口背面。那里堆满只写了一个名字的票，每张票都干净得像借口。最早的一张票角写着你的笔迹：先让我离开。你终于看见单程票从来不是车站强卖给你，是你亲手递过去的。",
        ),
        (AnomalyId::RisingWaterline, AnomalyResponse::Stabilize) => (
            "异象：稳住地下水线",
            "地下水线忽然升到胸口。你把雾灯玻璃压在墙砖上，逼回声说出完整姓名。水退下去时，墙上留下两个指印，一个是你的，一个小很多。回声第一次没有迟到。",
        ),
        (AnomalyId::RisingWaterline, AnomalyResponse::Follow) => (
            "异象：追随最高水线",
            "你让水漫过鞋面，跟着最深的那一道痕迹往前走。它不是通往出口，而是通往一处孩子肩膀高度的墙砖。那里有铅笔划过的浅痕：我站在线后，不是因为我不怕，是因为有人说会回来。",
        ),
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Stabilize) => (
            "异象：稳住漂移白线",
            "月台白线像活物一样往轨道边滑。你蹲下去，用作业本硬皮一点点把它推回安全距离。孩子站在后面看你，终于问：这次你是在保护线，还是在保护我？你说都不是，是先不让车站替你回答。",
        ),
        (AnomalyId::WhiteLineDrift, AnomalyResponse::Follow) => (
            "异象：追随漂移白线",
            "你跟着白线往雾里走。它绕开铁轨，绕开车门，最后停在孩子伸出的手边。原来白线不是边界的形状，而是一条迟来的路线，告诉你等待若没有人回来接住，就会自己长出方向。",
        ),
        (AnomalyId::StalledMinute, AnomalyResponse::Stabilize) => (
            "异象：稳住旧钟漏秒",
            "旧钟漏下一秒，整座钟楼都往那道缝里倾斜。你用黄铜钥匙抵住分针，念出站务日志上的日期。站务员的外套轻轻晃了一下，像终于承认守夜不是保存时间，而是把借来的时间还给具体的人。",
        ),
        (AnomalyId::StalledMinute, AnomalyResponse::Follow) => (
            "异象：追随旧钟漏秒",
            "你钻进漏下的一秒。里面不是黑暗，而是一张申请表、两处签名和站务员迟疑的手。你看见过去的自己说什么都可以，站务员却替你划掉了最重的一行：明天也会被抵押。",
        ),
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Stabilize) => (
            "异象：稳住广播回授",
            "广播声在钟楼里尖啸，所有警告互相覆盖，最后只剩别上车。你把磁带倒回去，调低回授，让第二个名字重新能被听见。声音安静下来时，警告不再像命令，而像一封迟来的说明。",
        ),
        (AnomalyId::BroadcastFeedback, AnomalyResponse::Follow) => (
            "异象：追随广播回授",
            "你沿着回授里的细小破音走向窄门。门后没有房间，只有一句被练习过无数次的广播：请不要把同行者留在白线后。你第一次听见这句话不是审判，而是路线。",
        ),
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Stabilize) => (
            "异象：稳住试刹声",
            "雾灯号在远处试刹，所有未完成的选择像乘客一样挤向白线。你站在它们前面，没有急着挑一扇门，只把已经确认的事实按顺序说完。车灯停了一停，像承认恐惧不能替你提前选择。",
        ),
        (AnomalyId::BrakeLightTrial, AnomalyResponse::Follow) => (
            "异象：追随试刹车灯",
            "你追着车灯进入雾里。每一扇车窗都映出一个结局：独自离开、带孩子返程、烧掉时刻表、走进广播室、接过外套。它们没有一个干净，但也没有一个只是按钮。你退回月台时，路线终于有了重量。",
        ),
    };
    StoryEvent::new(title, body)
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let resolved = state.resolved_anomalies.len();
    if resolved == 0 {
        return None;
    }

    let followed = state
        .resolved_anomalies
        .values()
        .filter(|response| **response == AnomalyResponse::Follow)
        .count();
    let stabilized = resolved.saturating_sub(followed);
    Some(format!(
        "你处理了 {resolved} 场车站异象，其中 {stabilized} 场被你稳住，{followed} 场被你追随。午夜因此不再只是倒计时，而是被你一次次具体处理过的压力。"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anomaly_summaries_track_visible_ready_and_resolved_state() {
        let mut state = GameState::new();
        let initial = anomaly_summaries(&state);
        let screen = initial
            .iter()
            .find(|summary| summary.anomaly == AnomalyId::ScreenKeepsScore)
            .expect("screen anomaly should be listed");
        assert_eq!(screen.status, "未显形");
        assert_eq!(screen.progress, 0);

        state.actions_used = crate::model::ACTIONS_PER_SEGMENT;
        state.remember(Flag::ReadDepartureBoard);
        let partial = anomaly_summaries(&state);
        let screen = partial
            .iter()
            .find(|summary| summary.anomaly == AnomalyId::ScreenKeepsScore)
            .expect("screen anomaly should be listed");
        assert_eq!(screen.status, "待补证");
        assert!(screen.visible);
        assert!(screen.progress > 0);

        state.discuss(TopicId::TravelerRain);
        let ready = anomaly_summaries(&state);
        let screen = ready
            .iter()
            .find(|summary| summary.anomaly == AnomalyId::ScreenKeepsScore)
            .expect("screen anomaly should be listed");
        assert_eq!(screen.status, "可处理");
        assert!(screen.ready);

        let event = handle(
            &mut state,
            AnomalyId::ScreenKeepsScore,
            AnomalyResponse::Stabilize,
        );
        assert!(event.tags.iter().any(|tag| tag == "车站异象"));
        assert_eq!(
            state.anomaly_response(AnomalyId::ScreenKeepsScore),
            Some(AnomalyResponse::Stabilize)
        );
    }

    #[test]
    fn anomaly_count_matches_midnight_pressure_segments() {
        assert_eq!(ANOMALY_COUNT, AnomalyId::ALL.len());
        assert_eq!(ANOMALY_COUNT, crate::model::MAX_SEGMENTS as usize - 1);
    }
}
