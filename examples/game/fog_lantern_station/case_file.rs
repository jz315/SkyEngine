use crate::model::{CaseFileId, EvidenceId, Flag, GameState, Item, StoryEvent, TopicId};

pub const CASE_FILE_COUNT: usize = 6;

#[derive(Clone, Debug)]
pub struct CaseFileAction {
    pub case_file: CaseFileId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseFileSummary {
    pub case_file: CaseFileId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub resolved: bool,
    pub ready: bool,
}

pub fn available_case_files(state: &GameState) -> Vec<CaseFileAction> {
    CaseFileId::ALL
        .iter()
        .copied()
        .filter(|case_file| !state.has_resolved_case_file(*case_file))
        .filter(|case_file| case_file_visible(state, *case_file))
        .map(|case_file| {
            let missing = missing_requirements(state, case_file);
            CaseFileAction {
                case_file,
                label: case_file.label(),
                detail: if missing.is_empty() {
                    "证据已经够了，可以把这条线索归档。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn case_file_summaries(state: &GameState) -> Vec<CaseFileSummary> {
    CaseFileId::ALL
        .iter()
        .copied()
        .map(|case_file| {
            let visible = case_file_visible(state, case_file);
            let resolved = state.has_resolved_case_file(case_file);
            let missing = missing_requirements(state, case_file);
            let ready = visible && missing.is_empty() && !resolved;
            let progress = case_file_progress(case_file, missing.len(), visible, resolved);
            let status = if resolved {
                "已归档"
            } else if ready {
                "可归档"
            } else if visible {
                "缺页"
            } else {
                "未发现"
            };
            let detail = if resolved {
                case_file.review().to_string()
            } else if ready {
                "证据已经够了。把它归档后，车站会承认这条推理，并改变后续选择的重量。".to_string()
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                "还没有足够线索让这份档案显形。继续调查地点、追问人物，或把证据递给愿意沉默的人。"
                    .to_string()
            };

            CaseFileSummary {
                case_file,
                title: case_file.title(),
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

pub fn resolve(state: &mut GameState, case_file: CaseFileId) -> StoryEvent {
    if state.has_resolved_case_file(case_file) {
        return StoryEvent::new(
            "档案已经归位",
            "这份档案已经写进站内记录。再次翻开它，只会看见你当时终于愿意承认的那一行字。",
        )
        .tag("站内档案");
    }

    let missing = missing_requirements(state, case_file);
    if !case_file_visible(state, case_file) || !missing.is_empty() {
        return StoryEvent::new(
            "档案缺页",
            format!(
                "你试着把线索装订在一起，纸页却自动散开。{}",
                if missing.is_empty() {
                    "这条档案还没有被车站承认。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("站内档案");
    }

    state.resolve_case_file(case_file);
    let mut event = case_file_event(case_file);
    apply_case_file_rewards(state, case_file, &mut event);
    event
}

impl CaseFileId {
    pub const ALL: [Self; CASE_FILE_COUNT] = [
        Self::WetTicketProtocol,
        Self::ReturnProtocol,
        Self::ChildWitness,
        Self::BorrowedMinute,
        Self::BroadcastDoor,
        Self::KeeperContract,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::WetTicketProtocol => "归档：湿票警告不是禁令",
            Self::ReturnProtocol => "归档：返程票需要两个人承认",
            Self::ChildWitness => "归档：孩子不是你的免罪道具",
            Self::BorrowedMinute => "归档：最后一分钟是借来的",
            Self::BroadcastDoor => "归档：广播室会保留声音",
            Self::KeeperContract => "归档：站务员外套也是债务",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WetTicketProtocol => "湿票警告不是禁令",
            Self::ReturnProtocol => "返程票需要两个人承认",
            Self::ChildWitness => "孩子不是你的免罪道具",
            Self::BorrowedMinute => "最后一分钟是借来的",
            Self::BroadcastDoor => "广播室会保留声音",
            Self::KeeperContract => "站务员外套也是债务",
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WetTicketProtocol => {
                "湿票、电子屏与镜片互相校正：警告不是禁令，而是被剪短的请求。"
            }
            Self::ReturnProtocol => {
                "返程需要两个名字互相承认；一个人的逃离不能自动替另一个人抵达。"
            }
            Self::ChildWitness => {
                "孩子是证人，也是当事人。他可以同行，但不能被解释成你的免罪凭证。"
            }
            Self::BorrowedMinute => "午夜循环来自借来的最后一分钟；它救过人，也开始向人收利息。",
            Self::BroadcastDoor => "广播室能保存警告，也会逐渐用警告替代说话的人。",
            Self::KeeperContract => "站务员外套提供秩序，也索取影子；照路的人并不会因此无辜。",
        }
    }

    fn requirement_count(self) -> usize {
        match self {
            Self::WetTicketProtocol | Self::ReturnProtocol => 2,
            Self::ChildWitness
            | Self::BorrowedMinute
            | Self::BroadcastDoor
            | Self::KeeperContract => 3,
        }
    }
}

fn case_file_progress(
    case_file: CaseFileId,
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
    let total = case_file.requirement_count();
    if total == 0 {
        return 0;
    }
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn case_file_visible(state: &GameState, case_file: CaseFileId) -> bool {
    match case_file {
        CaseFileId::WetTicketProtocol => {
            state.has_flag(Flag::ExaminedTicket)
                || state.has_flag(Flag::ReadDepartureBoard)
                || state.has_item(Item::MirrorShard)
        }
        CaseFileId::ReturnProtocol => {
            state.has_item(Item::CoinToken)
                || state.ticket.name().contains("返程")
                || state.has_discussed(TopicId::ClerkSeats)
        }
        CaseFileId::ChildWitness => {
            state.has_flag(Flag::MetChild)
                || state.has_item(Item::NameTag)
                || state.has_item(Item::ChildHomework)
        }
        CaseFileId::BorrowedMinute => {
            state.has_item(Item::StationLog)
                || state.has_flag(Flag::HeardClockTruth)
                || state.has_flag(Flag::UnderstoodFirstLoop)
        }
        CaseFileId::BroadcastDoor => {
            state.has_item(Item::BroadcastTape)
                || state.has_flag(Flag::HeardBroadcastTape)
                || state.has_flag(Flag::RepairedFogLamp)
        }
        CaseFileId::KeeperContract => {
            state.has_item(Item::OldTimetable)
                || state.has_flag(Flag::UnderstoodStationMechanism)
                || state.has_discussed(TopicId::KeeperStay)
        }
    }
}

fn missing_requirements(state: &GameState, case_file: CaseFileId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    match case_file {
        CaseFileId::WetTicketProtocol => {
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
        }
        CaseFileId::ReturnProtocol => {
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
                "向售票员问清两个座位或递出铜筹",
            );
        }
        CaseFileId::ChildWitness => {
            require(
                &mut missing,
                state.has_flag(Flag::MetChild),
                "见到月台上的孩子",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodChildPromise)
                    || state.has_presented(EvidenceId::ChildTicket),
                "读懂湿票第二个名字",
            );
            require(
                &mut missing,
                state.has_item(Item::ChildHomework) || state.has_item(Item::NameTag),
                "取得作业本或姓名牌",
            );
        }
        CaseFileId::BorrowedMinute => {
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
                state.has_flag(Flag::UnderstoodFirstLoop),
                "理解第一次循环",
            );
        }
        CaseFileId::BroadcastDoor => {
            require(
                &mut missing,
                state.has_item(Item::BroadcastTape),
                "取得广播磁带",
            );
            require(
                &mut missing,
                state.has_flag(Flag::RepairedFogLamp),
                "修复雾灯",
            );
            require(
                &mut missing,
                state.has_flag(Flag::AlignedClock) || state.has_flag(Flag::HeardBroadcastTape),
                "校准旧钟或听见广播室线索",
            );
        }
        CaseFileId::KeeperContract => {
            require(
                &mut missing,
                state.has_item(Item::OldTimetable),
                "取得烧焦的旧时刻表",
            );
            require(
                &mut missing,
                state.has_flag(Flag::UnderstoodStationMechanism),
                "理解车站机制",
            );
            require(
                &mut missing,
                state.has_discussed(TopicId::KeeperStay)
                    || state.has_presented(EvidenceId::KeeperStationLog)
                    || state.keeper_trust >= 3,
                "追问站务员留下来的代价",
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

fn apply_case_file_rewards(state: &mut GameState, case_file: CaseFileId, event: &mut StoryEvent) {
    match case_file {
        CaseFileId::WetTicketProtocol => {
            state.remember(Flag::UnderstoodFirstLoop);
            event.tags.push("理解：湿票警告".to_string());
        }
        CaseFileId::ReturnProtocol => {
            state.remember(Flag::SynthesizedRoute);
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("合成：返程规则".to_string());
        }
        CaseFileId::ChildWitness => {
            state.remember(Flag::SynthesizedChildTruth);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("合成：孩子证词".to_string());
        }
        CaseFileId::BorrowedMinute => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("合成：循环机制".to_string());
        }
        CaseFileId::BroadcastDoor => {
            state.remember(Flag::SynthesizedStationTruth);
            event.tags.push("合成：广播室".to_string());
        }
        CaseFileId::KeeperContract => {
            state.remember(Flag::SynthesizedStationTruth);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("合成：站务员契约".to_string());
        }
    }
}

fn case_file_event(case_file: CaseFileId) -> StoryEvent {
    let (title, body) = match case_file {
        CaseFileId::WetTicketProtocol => (
            "站内档案：湿票警告",
            "你把湿票、电子屏和镜片上的水痕归到同一页。那句“别上车”不是禁令，而是被剪短的请求。车站喜欢剪短请求，因为禁令比较容易让人反抗，请求却要求人承担关系。",
        ),
        CaseFileId::ReturnProtocol => (
            "站内档案：返程协议",
            "退票铜筹、两个座位和售票员的停顿终于拼成规则：返程不是奖励，是互相承认。一个人可以逃离，两个名字才算返程。车站不查你是否痛苦，只查你是否又把别人省略。",
        ),
        CaseFileId::ChildWitness => (
            "站内档案：白线后的证人",
            "孩子、作业本和姓名牌不再只是你的回忆碎片。他是证人，也是当事人。你可以请求同行，但不能把他的同行解释成原谅。他有权上车，也有权带着害怕离开。",
        ),
        CaseFileId::BorrowedMinute => (
            "站内档案：借来的最后一分钟",
            "站务日志、旧钟和第一次循环互相咬合。午夜不是惩罚从天而降，是你曾经申请借来的一分钟。借来的东西救过人，也开始收利息。你终于看见慈悲和控制共用同一只表盘。",
        ),
        CaseFileId::BroadcastDoor => (
            "站内档案：广播室窄门",
            "磁带、雾灯和旧钟线路指向月台远端那扇窄门。广播室可以让警告活下来，但说话的人会逐渐被警告替代。留下声音不是错误，只是不能把它误认为活着。",
        ),
        CaseFileId::KeeperContract => (
            "站内档案：站务员契约",
            "旧时刻表、车站机制和站务员的沉默写成同一份契约：外套给人秩序，也向人索取影子。留下来可以照路，也可能把照路变成债。你若接过它，必须知道自己不是无辜的灯。",
        ),
    };
    StoryEvent::new(title, body).tag("站内档案").tag("组合推理")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summaries_track_hidden_ready_and_resolved_case_files() {
        let mut state = GameState::new();
        let initial = case_file_summaries(&state);
        let wet = initial
            .iter()
            .find(|summary| summary.case_file == CaseFileId::WetTicketProtocol)
            .expect("wet ticket case file should be listed");
        assert_eq!(wet.status, "未发现");
        assert_eq!(wet.progress, 0);

        state.remember(Flag::ExaminedTicket);
        let partial = case_file_summaries(&state);
        let wet = partial
            .iter()
            .find(|summary| summary.case_file == CaseFileId::WetTicketProtocol)
            .expect("wet ticket case file should be listed");
        assert_eq!(wet.status, "缺页");
        assert!(wet.visible);
        assert!(wet.progress > 0);

        state.remember(Flag::ReadDepartureBoard);
        let ready = case_file_summaries(&state);
        let wet = ready
            .iter()
            .find(|summary| summary.case_file == CaseFileId::WetTicketProtocol)
            .expect("wet ticket case file should be listed");
        assert_eq!(wet.status, "可归档");
        assert!(wet.ready);
        assert_eq!(wet.progress, 100);

        let event = resolve(&mut state, CaseFileId::WetTicketProtocol);
        assert!(event.tags.iter().any(|tag| tag == "组合推理"));
        let resolved = case_file_summaries(&state);
        let wet = resolved
            .iter()
            .find(|summary| summary.case_file == CaseFileId::WetTicketProtocol)
            .expect("wet ticket case file should be listed");
        assert_eq!(wet.status, "已归档");
        assert!(wet.resolved);
    }
}
