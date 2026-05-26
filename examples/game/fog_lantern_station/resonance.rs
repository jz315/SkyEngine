use crate::model::{
    CaseFileId, EvidenceId, Flag, GameState, Item, ResonanceId, StoryEvent, TicketKind, TopicId,
};

#[cfg(test)]
use crate::case_file;
#[cfg(test)]
use crate::station_request;

pub const RESONANCE_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct ResonanceAction {
    pub resonance: ResonanceId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResonanceSummary {
    pub resonance: ResonanceId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub resolved: bool,
    pub ready: bool,
}

pub fn available_resonances(state: &GameState) -> Vec<ResonanceAction> {
    ResonanceId::ALL
        .iter()
        .copied()
        .filter(|resonance| !state.has_resolved_resonance(*resonance))
        .filter(|resonance| resonance_visible(state, *resonance))
        .map(|resonance| {
            let missing = missing_requirements(state, resonance);
            ResonanceAction {
                resonance,
                label: resonance.label(),
                detail: if missing.is_empty() {
                    "几句分散的证词正在互相照亮，可以把它们放到同一张长椅上。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn resonance_summaries(state: &GameState) -> Vec<ResonanceSummary> {
    ResonanceId::ALL
        .iter()
        .copied()
        .map(|resonance| {
            let resolved = state.has_resolved_resonance(resonance);
            let visible = resolved || resonance_visible(state, resonance);
            let missing = missing_requirements(state, resonance);
            let ready = visible && missing.is_empty() && !resolved;
            let progress = resonance_progress(resonance, missing.len(), visible, resolved);
            let status = if resolved {
                "已共鸣"
            } else if ready {
                "可触发"
            } else if visible {
                "待回声"
            } else {
                "未显形"
            };
            let detail = if resolved {
                resonance.review().to_string()
            } else if ready {
                "这组话已经可以互相回答。触发后会改变人物信任、路线理解或终局余波。".to_string()
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                "继续自由追问人物、出示证据和完成小委托，相关的证词才会彼此认出。".to_string()
            };

            ResonanceSummary {
                resonance,
                title: resonance.title(),
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

pub fn resolve(state: &mut GameState, resonance: ResonanceId) -> StoryEvent {
    if state.has_resolved_resonance(resonance) {
        return StoryEvent::new(
            "共鸣已经留下",
            "这组话已经在你的记录里互相回答。再把它们并排放好，只会看见当时亮过的一小块雾。",
        )
        .tag("人物共鸣");
    }

    let missing = missing_requirements(state, resonance);
    if !resonance_visible(state, resonance) || !missing.is_empty() {
        return StoryEvent::new(
            "共鸣还没有对上",
            format!(
                "你试着把几句证词摆在一起，它们却仍像隔着站台说话。{}",
                if missing.is_empty() {
                    "这组回声还没有被车站承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("人物共鸣");
    }

    state.resolve_resonance(resonance);
    let mut event = resonance_event(resonance);
    apply_resonance_rewards(state, resonance, &mut event);
    event
}

impl ResonanceId {
    pub const ALL: [Self; RESONANCE_COUNT] = [
        Self::RainInTheMirror,
        Self::TwoReservedSeats,
        Self::WhiteLineHomework,
        Self::BorrowedMinute,
        Self::BroadcastAfterimage,
        Self::DebtOfKeepingWatch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::RainInTheMirror => "共鸣：把雨声和镜片放在一起",
            Self::TwoReservedSeats => "共鸣：把两个座位说给窗口听",
            Self::WhiteLineHomework => "共鸣：把白线和作业本并排放好",
            Self::BorrowedMinute => "共鸣：把站务日志读给旧钟听",
            Self::BroadcastAfterimage => "共鸣：沿着广播线寻找回声",
            Self::DebtOfKeepingWatch => "共鸣：分辨守夜和欠债",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::RainInTheMirror => "雨声与镜片",
            Self::TwoReservedSeats => "两个保留座位",
            Self::WhiteLineHomework => "白线后的作业题",
            Self::BorrowedMinute => "借来的最后一分钟",
            Self::BroadcastAfterimage => "广播后的影子",
            Self::DebtOfKeepingWatch => "守夜与欠债",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::RainInTheMirror | Self::TwoReservedSeats => 3,
            Self::WhiteLineHomework
            | Self::BorrowedMinute
            | Self::BroadcastAfterimage
            | Self::DebtOfKeepingWatch => 4,
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::RainInTheMirror => {
                "老人谈过的雨和镜片里的旧脸互相照亮：警告不是天气，是迟到的请求。"
            }
            Self::TwoReservedSeats => {
                "铜筹、座位图和窗口停顿说明同一件事：返程不是一张票，是两个名字互相承认。"
            }
            Self::WhiteLineHomework => {
                "白线不是障碍，作业题也不是谜语。它们都在保护孩子仍能自己决定。"
            }
            Self::BorrowedMinute => {
                "站务日志与旧钟互相证明：最后一分钟曾被借出，慈悲因此开始拥有利息。"
            }
            Self::BroadcastAfterimage => {
                "广播能留下警告，也会留下说话者的影子。声音不是出口，只是另一种停留。"
            }
            Self::DebtOfKeepingWatch => {
                "守夜可以照路，也可能把别人锁进感谢。灯若要求被崇拜，就已经变成债。"
            }
        }
    }
}

fn resonance_visible(state: &GameState, resonance: ResonanceId) -> bool {
    match resonance {
        ResonanceId::RainInTheMirror => {
            state.has_discussed(TopicId::TravelerRain)
                || state.has_item(Item::MirrorShard)
                || state.has_flag(Flag::ReadDepartureBoard)
        }
        ResonanceId::TwoReservedSeats => {
            state.has_item(Item::CoinToken)
                || state.has_discussed(TopicId::ClerkSeats)
                || state.has_resolved_case_file(CaseFileId::ReturnProtocol)
                || state.ticket == TicketKind::Return
        }
        ResonanceId::WhiteLineHomework => {
            state.has_flag(Flag::MetChild)
                || state.has_item(Item::ChildHomework)
                || state.has_item(Item::NameTag)
        }
        ResonanceId::BorrowedMinute => {
            state.has_item(Item::StationLog)
                || state.has_flag(Flag::HeardClockTruth)
                || state.has_flag(Flag::UnderstoodFirstLoop)
                || state.has_resolved_case_file(CaseFileId::BorrowedMinute)
        }
        ResonanceId::BroadcastAfterimage => {
            state.has_item(Item::BroadcastTape)
                || state.has_flag(Flag::HeardBroadcastTape)
                || state.has_flag(Flag::RepairedFogLamp)
                || state.has_resolved_case_file(CaseFileId::BroadcastDoor)
        }
        ResonanceId::DebtOfKeepingWatch => {
            state.completed_requests.len() >= 1
                || state.has_discussed(TopicId::KeeperStay)
                || state.has_discussed(TopicId::TravelerMercy)
                || state.keeper_depth >= 6
        }
    }
}

fn missing_requirements(state: &GameState, resonance: ResonanceId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match resonance {
        ResonanceId::RainInTheMirror => {
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerRain) || state.traveler_depth >= 1,
                "让老人谈过雨或第一枚钥匙",
            );
            require(
                &mut missing,
                state.has_item(Item::MirrorShard)
                    || state.has_presented(EvidenceId::TravelerMirror),
                "取得候车厅镜片",
            );
            require(
                &mut missing,
                state.has_flag(Flag::ReadDepartureBoard),
                "读过电子时刻表",
            );
        }
        ResonanceId::TwoReservedSeats => {
            require(
                &mut missing,
                state.has_item(Item::CoinToken),
                "取得退票铜筹",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ClerkSeats)
                    || state.has_presented(EvidenceId::ClerkCoinToken)
                    || state.clerk_trust >= 3,
                "让售票员承认两个座位",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::TravelerLeaving)
                    || state.traveler_depth >= 5
                    || state.has_resolved_case_file(CaseFileId::ReturnProtocol),
                "让老人谈过上车又回来",
            );
        }
        ResonanceId::WhiteLineHomework => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到白线后的孩子",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework)
                    || state.has_discussed(TopicId::ChildHomework)
                    || state.has_presented(EvidenceId::ChildHomework),
                "看过作业本",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodChildPromise)
                    || state.has_discussed(TopicId::ChildPromise)
                    || state.has_presented(EvidenceId::ChildTicket),
                "理解那句旧承诺",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::ChildWhiteLine) || state.child_depth >= 3,
                "让孩子谈过白线",
            );
        }
        ResonanceId::BorrowedMinute => {
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
                state.has_flag(Flag::UnderstoodFirstLoop)
                    || state.has_resolved_case_file(CaseFileId::WetTicketProtocol),
                "理解第一次循环",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::KeeperClock) || state.keeper_depth >= 1,
                "问过站务员旧钟为什么停住",
            );
        }
        ResonanceId::BroadcastAfterimage => {
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
                    || state.has_discussed(TopicId::KeeperBroadcast)
                    || state.has_presented(EvidenceId::KeeperBroadcastTape),
                "让旧钟或站务员承认广播室代价",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::UnderpassBroadcast)
                    || state.investigation_depth(crate::model::Location::Underpass) >= 5,
                "在地下通道听见旧广播",
            );
        }
        ResonanceId::DebtOfKeepingWatch => {
            require(
                &mut missing,
                state.completed_requests.len() >= 2,
                "完成至少两件旅客委托",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::KeeperStay)
                    || state.has_discussed(TopicId::TravelerMercy)
                    || state.keeper_depth >= 6,
                "有人谈过留下来的危险",
            );
            require(
                &mut missing,
                state.has_resolved_case_file(CaseFileId::BorrowedMinute)
                    || state.has_resolved_case_file(CaseFileId::KeeperContract)
                    || state.has_flag(Flag::UnderstoodStationMechanism),
                "整理过车站机制或外套契约",
            );
            require(
                &mut missing,
                state.resolved_case_files.len() >= 2,
                "至少归档两份站内档案",
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

fn resonance_progress(
    resonance: ResonanceId,
    missing_count: usize,
    visible: bool,
    resolved: bool,
) -> u8 {
    if resolved {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = resonance.requirement_count();
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn apply_resonance_rewards(state: &mut GameState, resonance: ResonanceId, event: &mut StoryEvent) {
    match resonance {
        ResonanceId::RainInTheMirror => {
            state.remember(Flag::TravelerTrusted);
            state.remember(Flag::UnderstoodFirstLoop);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("共鸣：湿票警告".to_string());
        }
        ResonanceId::TwoReservedSeats => {
            state.remember(Flag::SynthesizedRoute);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("共鸣：返程座位".to_string());
        }
        ResonanceId::WhiteLineHomework => {
            state.remember(Flag::UnderstoodChildPromise);
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 2).min(5);
            event.tags.push("共鸣：孩子证词".to_string());
        }
        ResonanceId::BorrowedMinute => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("共鸣：循环机制".to_string());
        }
        ResonanceId::BroadcastAfterimage => {
            state.remember(Flag::HeardBroadcastTape);
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("共鸣：广播室".to_string());
        }
        ResonanceId::DebtOfKeepingWatch => {
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("共鸣：守夜代价".to_string());
        }
    }
}

fn resonance_event(resonance: ResonanceId) -> StoryEvent {
    let (title, body) = match resonance {
        ResonanceId::RainInTheMirror => (
            "人物共鸣：雨声与镜片",
            "你把老人说过的雨、电子屏上的空白和镜片里的旧脸放在一起。雨声忽然不像天气，倒像某个人在那晚以后反复练习开口。镜片里的人没有责备你，只把湿票推近一点：警告不是为了禁止你走，是为了让你终于读完整个请求。",
        ),
        ResonanceId::TwoReservedSeats => (
            "人物共鸣：两个保留座位",
            "退票铜筹在长椅上转了一圈，最后停在 07A 和 07B 之间。老人说回来不是勇敢，售票员说返程不是奖励。两句话互相咬合以后，你终于听懂：车站一直保留的不是座位，而是你承认另一个人也有目的地的能力。",
        ),
        ResonanceId::WhiteLineHomework => (
            "人物共鸣：白线后的作业题",
            "孩子谈过的白线、作业本上永远写不完的题、湿票里藏着的第二个名字并排放好。它们没有拼成一个谜底，只拼成一个很小的权利：他可以害怕，可以生气，也可以在害怕时仍然选择明天。",
        ),
        ResonanceId::BorrowedMinute => (
            "人物共鸣：借来的最后一分钟",
            "站务日志贴着旧钟，纸页被齿轮风吹得发抖。老人说好心也会把门反锁，站务员说最后一分钟需要利息。你忽然明白，午夜不是单纯惩罚你，它也曾经认真试图救人，只是救人的方式被你用得太久，开始像命令。",
        ),
        ResonanceId::BroadcastAfterimage => (
            "人物共鸣：广播后的影子",
            "地下通道的旧广播、钟楼里的磁带和雾灯照出的窄门终于接上同一条线。你听见自己的声音从另一端传来，比现在年轻，也比现在更急。它一直想警告后来者，却忘了声音若没有身体，也会慢慢学会把停留说成职责。",
        ),
        ResonanceId::DebtOfKeepingWatch => (
            "人物共鸣：守夜与欠债",
            "你把办完的小委托、归档的真相和站务员那件无影外套放在同一页。守夜当然可以是爱，可如果灯光要求所有人感谢它，爱就会变成债。你在页边写下：留下的人也要学会让别人不必欠他。",
        ),
    };
    StoryEvent::new(title, body).tag("人物共鸣").tag("交叉对话")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resonance_summaries_track_ready_and_resolved_state() {
        let mut state = GameState::new();
        let initial = resonance_summaries(&state);
        let rain = initial
            .iter()
            .find(|summary| summary.resonance == ResonanceId::RainInTheMirror)
            .expect("rain resonance should be listed");
        assert_eq!(rain.status, "未显形");
        assert_eq!(rain.progress, 0);

        state.discuss(TopicId::TravelerRain);
        state.remember(Flag::ReadDepartureBoard);
        let partial = resonance_summaries(&state);
        let rain = partial
            .iter()
            .find(|summary| summary.resonance == ResonanceId::RainInTheMirror)
            .expect("rain resonance should be listed");
        assert_eq!(rain.status, "待回声");
        assert!(rain.visible);
        assert!(rain.progress > 0);

        state.add_item(Item::MirrorShard);
        let ready = resonance_summaries(&state);
        let rain = ready
            .iter()
            .find(|summary| summary.resonance == ResonanceId::RainInTheMirror)
            .expect("rain resonance should be listed");
        assert_eq!(rain.status, "可触发");
        assert!(rain.ready);

        let event = resolve(&mut state, ResonanceId::RainInTheMirror);
        assert!(event.tags.iter().any(|tag| tag == "交叉对话"));
        assert!(state.has_resolved_resonance(ResonanceId::RainInTheMirror));
        assert!(state.has_flag(Flag::TravelerTrusted));
        assert!(state.has_flag(Flag::UnderstoodFirstLoop));
    }

    #[test]
    fn resonance_count_matches_related_system_totals() {
        assert_eq!(RESONANCE_COUNT, ResonanceId::ALL.len());
        assert!(RESONANCE_COUNT <= case_file::CASE_FILE_COUNT + station_request::REQUEST_COUNT);
    }
}
