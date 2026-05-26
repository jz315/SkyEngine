use crate::aftertalk;
use crate::anomaly;
use crate::case_dialogue;
use crate::case_file;
use crate::chapter;
use crate::companion;
use crate::departure;
use crate::dialogue_challenge;
use crate::dialogue_lead;
use crate::dialogue_question;
use crate::dialogue_relay;
use crate::dialogue_system;
use crate::ending_aftermath;
use crate::final_debate;
use crate::final_interview;
use crate::final_prelude;
use crate::lamp_focus;
use crate::memory;
use crate::model::{
    Ending, EvidenceId, Flag, GameSession, GameState, Item, Location, StoryEvent, TicketKind,
    LOCATION_INVESTIGATION_STEPS, MAX_NIGHT_MINUTES, MAX_SEGMENTS, NPC_THREAD_STEPS,
};
use crate::patrol;
use crate::resonance;
use crate::route_cost;
use crate::route_echo;
use crate::route_pressure;
use crate::route_witness;
use crate::route_witness_debrief;
use crate::station_request;
use crate::station_whisper;
use crate::trial;
use crate::truth;
use crate::vow;

pub fn opening_event() -> StoryEvent {
    StoryEvent::new(
        "你在雾灯站醒来",
        "你在候车厅醒来，手里攥着一张湿透的旧车票。票面被雨泡得发软，背面被指甲划出三个字：别上车。现在是 23:48，雾灯号会在 240 分钟后进站。你不知道自己为什么在这里，只知道车站不肯让你把这张票当成普通车票。先看清票面、时刻表、空座、售票窗口和旧钟楼，再决定零点以前该相信谁。",
    )
    .tag("八段午夜")
    .tag("悬疑开场")
}

pub fn title_copy() -> &'static str {
    "文字冒险 / 侦探式探索。你有 240 分钟调查雾灯站：检查湿票、空座、白线、失物、站务日志和旧钟，和站内人物自由对话，在每一次追问里分辨哪些话是真相，哪些话只是车站替你准备好的借口。"
}

pub fn location_description(state: &GameState) -> String {
    let mut text = match state.location {
        Location::WaitingHall => {
            let mut text =
                "候车厅是起点。这里有湿车票、电子时刻表、一排被挪空的长椅，以及一个一直读报纸的老人。"
                    .to_string();
            if state.has_flag(Flag::ReadDepartureBoard) {
                text.push_str(" 你已经知道：时刻表会把乘客姓名当成目的地。");
            }
            text
        }
        Location::TicketOffice => {
            let mut text =
                "售票窗口负责核验和改签。这里能查清湿票为什么还有效、返程规则为什么总差一点，以及售票员为什么总说“不够换”。"
                    .to_string();
            if state.ticket.name().contains("返程") {
                text.push_str(" 你已经拿到返程联票，可以继续准备最终离站路线。");
            }
            text
        }
        Location::LostAndFound => {
            let mut text =
                "失物招领处存放被雨水带回来的物件。这里能找到雾灯玻璃、裂开的姓名牌和站务档案。"
                    .to_string();
            if state.has_item(Item::StationLog) {
                text.push_str(" 你已经从铁柜里取得站务日志。");
            }
            text
        }
        Location::Underpass => {
            let mut text =
                "地下通道连接候车厅和月台。墙上的水线、疏散标志和回声会提示：同一句安全提醒，后来怎样被喊成不准动。".to_string();
            if state.has_flag(Flag::RecoveredName) {
                text.push_str(" 你已经在这里想起自己的名字。");
            }
            text
        }
        Location::ClockTower => {
            let mut text =
                "旧钟楼控制时间、广播线和雾灯。站务员在这里解释午夜为什么停在 23:59。".to_string();
            if state.has_flag(Flag::AlignedClock) {
                text.push_str(" 旧钟已经校准，列车可以被召回。");
            }
            text
        }
        Location::Platform => {
            let mut text = "三号月台是雾灯号进站的地方。白线、轨道轮痕和远处的人影都不像普通站台会留下的东西。"
                .to_string();
            if state.has_flag(Flag::MetChild) {
                text.push_str(" 白线后的人还没有靠近你；他像是在等一句你也没听懂的话。");
            }
            text
        }
    };
    if let Some(note) = dialogue_relay::location_anchor_note(state, state.location) {
        text.push_str(&note);
    }
    if let Some(shift) = chapter::location_shift(state.location, state.current_segment()) {
        text.push_str(shift);
    }
    text
}

pub fn pressure_text(state: &GameState) -> String {
    if state.final_train_due() {
        format!(
            "{}，雾灯号已经进站。调查时间结束，现在只能做最终选择。",
            state.clock_text()
        )
    } else {
        format!(
            "{}。全夜共 {} 段，现在是第 {} 段。雾灯号还没进站，还剩约 {} 分钟。{}",
            state.clock_text(),
            MAX_SEGMENTS,
            state.current_segment(),
            state.time_left(),
            chapter::pressure_note(state)
        )
    }
}

pub fn condition_summary(state: &GameState) -> Vec<String> {
    let mut lines = Vec::new();
    lines.push(format!("当前位置：{}", state.location.title()));
    lines.push(format!("车票：{}", state.ticket.name()));
    lines.push(format!(
        "时间：{} / 第 {} 段午夜",
        state.clock_text(),
        state.current_segment()
    ));
    lines.push(format!("说话方式：{}", state.dialogue_tone.name()));
    if let Some(line) = dialogue_system::condition_line(state) {
        lines.push(line);
    }
    if !state.dialogue_transcript.is_empty() {
        lines.push(format!(
            "对话记录：{} 段 / {} 节点，{} 个自由询问，{} 次立场回应",
            state.dialogue_transcript.len(),
            state.completed_dialogue_beats.len(),
            state.answered_dialogue_questions.len(),
            state.answered_dialogue_challenges.len()
        ));
    }
    if !state.completed_dialogue_leads.is_empty() {
        lines.push(format!(
            "对话线索：{} 条已追查",
            state.completed_dialogue_leads.len()
        ));
    }
    if !state.completed_case_dialogues.is_empty() {
        lines.push(format!(
            "档案回谈：{} / {} 份已带回人物面前",
            state.completed_case_dialogues.len(),
            case_dialogue::CASE_DIALOGUE_COUNT
        ));
    }
    if !state.completed_final_interviews.is_empty() {
        lines.push(format!(
            "终局前长谈：{} / {} 段已完成",
            state.completed_final_interviews.len(),
            final_interview::FINAL_INTERVIEW_COUNT
        ));
    }
    if !state.completed_dialogue_relays.is_empty() {
        lines.push(format!(
            "线索转述：{} 条已完成，{} 段余波，{} 段回声，{} 个落点，{} 个复看",
            state.completed_dialogue_relays.len(),
            state.reflected_dialogue_relays.len(),
            state.echoed_dialogue_relays.len(),
            state.anchored_dialogue_relays.len(),
            state.reviewed_dialogue_anchors.len()
        ));
    }
    lines.push(format!("列车：约 {} 分钟后进站", state.time_left()));
    let remaining =
        LOCATION_INVESTIGATION_STEPS.saturating_sub(state.investigation_depth(state.location));
    if remaining == 0 {
        lines.push("这里暂时没有新的细节愿意露面".to_string());
    } else {
        lines.push(format!("这里还有 {} 处调查点可继续推进", remaining));
    }
    if state.has_flag(Flag::RecoveredName) {
        lines.push("姓名：已经想起".to_string());
    } else {
        lines.push("姓名：遗失".to_string());
    }
    if state.has_flag(Flag::ChildJoined) {
        lines.push("同行者：月台上的孩子".to_string());
    }
    if state.has_flag(Flag::FinalTrainArrived) {
        lines.push("列车：已进站".to_string());
    }
    if state.synthesis_depth > 0 {
        lines.push("部分线索已经整理成可用结论".to_string());
    }
    if !state.revealed_truth_scenes.is_empty() {
        lines.push(format!(
            "中段真相：{} / {} 层已揭开",
            state.revealed_truth_scenes.len(),
            truth::TRUTH_SCENE_COUNT
        ));
    }
    lines
}

pub fn objective_summary(state: &GameState) -> Vec<String> {
    if let Some(hint) = dialogue_lead::active_return_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_relay::active_relay_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_relay::active_reflection_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_relay::active_echo_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = route_pressure::active_pressure_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = case_dialogue::active_case_dialogue_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = route_witness_debrief::active_debrief_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = final_interview::active_interview_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_challenge::active_challenge_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_question::active_question_objective_hint(state) {
        return vec![hint];
    }
    if let Some(hint) = dialogue_system::objective_hint(state) {
        return vec![hint];
    }

    if state.final_train_due() {
        let mut lines = vec!["列车已经进站：普通时间结束，只剩选择。".to_string()];
        if final_debate::final_choice_ready(state, Ending::EscapedAlone) {
            lines.push("可选：独自上车，保住自己的名字。".to_string());
        } else if final_prelude::route_ready_for_ending(state, Ending::EscapedAlone) {
            lines.push(final_debate::final_choice_detail(
                state,
                Ending::EscapedAlone,
                "可选：独自上车，保住自己的名字。",
                "独自上车还缺：姓名，以及返程票或轨道证据。",
            ));
        } else {
            lines.push("独自上车还缺：姓名，以及返程票或轨道证据。".to_string());
        }
        let child_route_known = state.has_flag(Flag::MetChild)
            || state.has_flag(Flag::ChildJoined)
            || state.child_depth > 0;
        if !child_route_known {
            lines.push("白线后的空位还缺：先确认三号月台远处的人影。".to_string());
        } else if final_debate::final_choice_ready(state, Ending::TookChildHome) {
            lines.push("可选：带孩子上车，让明天承认两个人。".to_string());
        } else if final_prelude::route_ready_for_ending(state, Ending::TookChildHome) {
            lines.push(final_debate::final_choice_detail(
                state,
                Ending::TookChildHome,
                "可选：带孩子上车，让明天承认两个人。",
                "带孩子返程还缺：同行信任，以及返程或孩子真相。",
            ));
        } else {
            lines.push("带孩子返程还缺：同行信任，以及返程或孩子真相。".to_string());
        }
        if final_debate::final_choice_ready(state, Ending::BurnedTimetable) {
            lines.push("可选：烧掉时刻表，把车站规则还给雾外的人。".to_string());
        } else if final_prelude::route_ready_for_ending(state, Ending::BurnedTimetable) {
            lines.push(final_debate::final_choice_detail(
                state,
                Ending::BurnedTimetable,
                "可选：烧掉时刻表，把车站规则还给雾外的人。",
                "烧掉时刻表还缺：旧时刻表、修复后的雾灯和车站真相。",
            ));
        }
        if final_debate::final_choice_ready(state, Ending::BecameTheVoice) {
            lines.push("可选：走进广播室，成为后来者会听见的警告。".to_string());
        } else if final_prelude::route_ready_for_ending(state, Ending::BecameTheVoice) {
            lines.push(final_debate::final_choice_detail(
                state,
                Ending::BecameTheVoice,
                "可选：走进广播室，成为后来者会听见的警告。",
                "走进广播室还缺：姓名、站务日志、旧钟和广播磁带。",
            ));
        }
        if final_debate::final_choice_ready(state, Ending::NewStationKeeper) {
            lines.push("可选：接过外套，替下一位旅客守夜。".to_string());
        } else if final_prelude::route_ready_for_ending(state, Ending::NewStationKeeper) {
            lines.push(final_debate::final_choice_detail(
                state,
                Ending::NewStationKeeper,
                "可选：接过外套，替下一位旅客守夜。",
                "接过外套还缺：旧钟真相和车站机制。",
            ));
        }
        lines.truncate(5);
        return lines;
    }

    let mut lines = Vec::new();
    if !state.has_flag(Flag::ExaminedTicket) {
        lines.push("先看清湿票背后的第一层笔迹。".to_string());
    }
    if let Some(hint) = evidence_objective_hint(state) {
        lines.push(hint.to_string());
    }
    if let Some(hint) = anomaly_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = request_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = resonance_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = vow_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = memory_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = patrol_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = dialogue_lead_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = dialogue_relay_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = dialogue_question_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = dialogue_challenge_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = aftertalk_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = companion_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = lamp_focus_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = station_whisper_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = departure_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = trial_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = route_cost_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = route_echo_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = route_pressure_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = route_witness_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = route_witness_debrief_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = final_interview_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = truth_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = case_file_objective_hint(state) {
        lines.push(hint);
    }
    if let Some(hint) = case_dialogue_objective_hint(state) {
        lines.push(hint);
    }
    if !state.has_item(Item::BrassKey) {
        lines.push("和候车厅老人谈谈，他可能知道黄铜钥匙为什么认得你。".to_string());
    }
    if !state.has_item(Item::StationLog) {
        lines.push("失物招领处最深的铁柜，保存着你曾经写下的站务日志。".to_string());
    }
    if !state.has_flag(Flag::RecoveredName) {
        lines.push("带着车票、镜片或姓名牌去地下通道，听回声把名字还回来。".to_string());
    }
    if let Some(hint) = chapter::objective_hint(state) {
        lines.push(hint.to_string());
    }
    if !state.has_flag(Flag::MetChild) {
        lines.push("三号月台白线后，有人还在等一个没有变成借口的回答。".to_string());
    } else if !state.has_flag(Flag::ChildJoined) {
        lines.push("孩子需要名字、承诺和足够具体的行动，而不是又一次道歉。".to_string());
    }
    if state.ticket != TicketKind::Return {
        lines.push("售票窗口只办理退票；返程票需要日志、铜筹和信任。".to_string());
    }
    if !state.has_flag(Flag::RepairedFogLamp) {
        lines.push("雾灯玻璃能让月台远端的路承认自己存在。".to_string());
    }
    if !state.has_flag(Flag::AlignedClock) {
        lines.push("旧钟停在 23:59；听懂代价以后，用黄铜钥匙让它继续。".to_string());
    } else if state.has_item(Item::SignalWhistle) && !state.has_flag(Flag::FinalTrainArrived) {
        lines.push("发车哨已经能响；吹响它，就会进入最终选择。".to_string());
    }
    if state.synthesis_depth < 5 {
        lines.push("当线索互相咬合时，整理它们会打开更深的结局条件。".to_string());
    }

    lines.truncate(8);
    lines
}

fn evidence_objective_hint(state: &GameState) -> Option<&'static str> {
    if state.has_item(Item::MirrorShard) && !state.has_presented(EvidenceId::TravelerMirror) {
        return Some("镜片可以带回给候车厅老人；旧脸会打开另一层循环证词。");
    }
    if state.has_item(Item::CoinToken) && !state.has_presented(EvidenceId::ClerkCoinToken) {
        return Some("退票铜筹不是收藏品，把它放到售票窗口，返程票才会变得具体。");
    }
    if state.has_item(Item::ConductorRoster) && !state.has_presented(EvidenceId::TravelerRoster) {
        return Some("列车员名册缺页可以拿给老人或售票员核对。");
    }
    if state.has_flag(Flag::MetChild)
        && state.has_flag(Flag::ExaminedTicket)
        && !state.has_presented(EvidenceId::ChildTicket)
    {
        return Some("把湿票摊给孩子看，不要替他解释那半句。");
    }
    if state.has_item(Item::StationLog) && !state.has_presented(EvidenceId::KeeperStationLog) {
        return Some("站务日志末页应带到旧钟楼，让站务员亲口承认申请。");
    }
    if state.has_item(Item::BroadcastTape) && !state.has_presented(EvidenceId::KeeperBroadcastTape)
    {
        return Some("广播磁带靠近旧钟时，会暴露广播室的代价。");
    }
    None
}

fn case_file_objective_hint(state: &GameState) -> Option<String> {
    case_file::available_case_files(state)
        .into_iter()
        .find(|case_file| case_file.enabled)
        .map(|case_file| format!("站内档案可以归档：{}。", case_file.label))
}

fn anomaly_objective_hint(state: &GameState) -> Option<String> {
    anomaly::available_anomalies(state)
        .into_iter()
        .find(|anomaly| anomaly.enabled)
        .map(|anomaly| format!("车站异象可以处理：{}。", anomaly.label))
}

fn request_objective_hint(state: &GameState) -> Option<String> {
    station_request::available_requests(state)
        .into_iter()
        .find(|request| request.enabled)
        .map(|request| format!("旅客委托可以完成：{}。", request.label))
}

fn resonance_objective_hint(state: &GameState) -> Option<String> {
    resonance::available_resonances(state)
        .into_iter()
        .find(|resonance| resonance.enabled)
        .map(|resonance| format!("人物共鸣可以触发：{}。", resonance.label))
}

fn vow_objective_hint(state: &GameState) -> Option<String> {
    vow::available_vows(state)
        .into_iter()
        .find(|vow| vow.enabled)
        .map(|vow| format!("内心锚点可以写下：{}。", vow.label))
}

fn memory_objective_hint(state: &GameState) -> Option<String> {
    memory::available_memories(state)
        .into_iter()
        .find(|memory| memory.enabled)
        .map(|memory| format!("记忆回廊可以进入：{}。", memory.label))
}

fn patrol_objective_hint(state: &GameState) -> Option<String> {
    patrol::available_patrols(state)
        .into_iter()
        .find(|patrol| patrol.enabled)
        .map(|patrol| format!("巡夜记录可以完成：{}。", patrol.label))
}

fn dialogue_lead_objective_hint(state: &GameState) -> Option<String> {
    dialogue_lead::available_leads(state)
        .into_iter()
        .find(|lead| lead.enabled)
        .map(|lead| format!("对话线索可以追查：{}。", lead.label))
}

fn dialogue_relay_objective_hint(state: &GameState) -> Option<String> {
    dialogue_relay::relay_objective_hint(state)
}

fn dialogue_question_objective_hint(state: &GameState) -> Option<String> {
    dialogue_question::question_objective_hint(state)
}

fn dialogue_challenge_objective_hint(state: &GameState) -> Option<String> {
    dialogue_challenge::challenge_objective_hint(state)
}

fn aftertalk_objective_hint(state: &GameState) -> Option<String> {
    aftertalk::available_aftertalks(state)
        .into_iter()
        .find(|aftertalk| aftertalk.enabled)
        .map(|aftertalk| format!("回访对话可以继续：{}。", aftertalk.label))
}

fn companion_objective_hint(state: &GameState) -> Option<String> {
    companion::available_companion_talks(state)
        .into_iter()
        .find(|talk| talk.enabled)
        .map(|talk| format!("同行对话可以继续：{}。", talk.label))
}

fn lamp_focus_objective_hint(state: &GameState) -> Option<String> {
    lamp_focus::available_focuses(state)
        .into_iter()
        .find(|focus| focus.enabled)
        .map(|focus| format!("雾灯照证可以完成：{}。", focus.label))
}

fn station_whisper_objective_hint(state: &GameState) -> Option<String> {
    station_whisper::available_whispers(state)
        .into_iter()
        .find(|whisper| whisper.enabled)
        .map(|whisper| format!("站内低语可以聆听：{}。", whisper.label))
}

fn departure_objective_hint(state: &GameState) -> Option<String> {
    departure::available_departures(state)
        .into_iter()
        .find(|departure| departure.enabled)
        .map(|departure| format!("路线准备可以完成：{}。", departure.label))
}

fn trial_objective_hint(state: &GameState) -> Option<String> {
    trial::available_trials(state)
        .into_iter()
        .find(|trial| trial.enabled)
        .map(|trial| format!("路线试炼可以进入：{}。", trial.label))
}

fn route_cost_objective_hint(state: &GameState) -> Option<String> {
    route_cost::available_costs(state)
        .into_iter()
        .find(|cost| cost.enabled)
        .map(|cost| format!("路线代价可以调停：{}。", cost.label))
}

fn route_echo_objective_hint(state: &GameState) -> Option<String> {
    route_echo::available_echoes(state)
        .into_iter()
        .find(|echo| echo.enabled)
        .map(|echo| format!("路线回声可以交谈：{}。", echo.label))
}

fn route_pressure_objective_hint(state: &GameState) -> Option<String> {
    route_pressure::pressure_objective_hint(state)
}

fn route_witness_objective_hint(state: &GameState) -> Option<String> {
    route_witness::witness_objective_hint(state)
}

fn route_witness_debrief_objective_hint(state: &GameState) -> Option<String> {
    route_witness_debrief::debrief_objective_hint(state)
}

fn final_interview_objective_hint(state: &GameState) -> Option<String> {
    final_interview::interview_objective_hint(state)
}

fn case_dialogue_objective_hint(state: &GameState) -> Option<String> {
    case_dialogue::case_dialogue_objective_hint(state)
}

fn truth_objective_hint(state: &GameState) -> Option<String> {
    truth::truth_objective_hint(state)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteSummary {
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RelationshipSummary {
    pub name: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRecapLine {
    pub label: &'static str,
    pub value: String,
    pub detail: String,
    pub progress: u8,
}

pub fn chapter_summary(state: &GameState) -> Vec<String> {
    let total_location_steps = Location::ALL.len() as u16 * LOCATION_INVESTIGATION_STEPS as u16;
    let explored = state
        .location_depths
        .iter()
        .copied()
        .map(u16::from)
        .sum::<u16>();
    let conversation_depth = u16::from(state.traveler_depth)
        + u16::from(state.clerk_depth)
        + u16::from(state.child_depth)
        + u16::from(state.keeper_depth);
    let max_conversation_depth = 4_u16 * crate::model::NPC_THREAD_STEPS as u16;

    vec![
        format!(
            "章节：第 {} / {} 段午夜",
            state.current_segment(),
            MAX_SEGMENTS
        ),
        format!("车站变化：{}", chapter::phase_label(state)),
        format!("地点调查：{} / {} 层", explored, total_location_steps),
        format!(
            "人物长谈：{} / {} 段",
            conversation_depth, max_conversation_depth
        ),
        format!(
            "自由追问：{} 个话题，{} 件证据，{} / {} 个询问，{} / {} 次反问",
            state.discussed_topics.len(),
            state.presented_evidence.len(),
            state.answered_dialogue_questions.len(),
            dialogue_question::DIALOGUE_QUESTION_COUNT,
            state.answered_dialogue_challenges.len(),
            dialogue_challenge::DIALOGUE_CHALLENGE_COUNT
        ),
        format!(
            "车站异象：{} / {} 场",
            state.resolved_anomalies.len(),
            anomaly::ANOMALY_COUNT
        ),
        format!(
            "人物共鸣：{} / {} 组",
            state.resolved_resonances.len(),
            resonance::RESONANCE_COUNT
        ),
        format!(
            "内心锚点：{} / {} 条",
            state.chosen_vows.len(),
            vow::VOW_COUNT
        ),
        format!(
            "记忆回廊：{} / {} 段",
            state.visited_memories.len(),
            memory::MEMORY_COUNT
        ),
        format!(
            "巡夜记录：{} / {} 份",
            state.completed_patrols.len(),
            patrol::PATROL_COUNT
        ),
        format!(
            "对话线索：{} / {} 条，回谈 {} 条",
            state.completed_dialogue_leads.len(),
            dialogue_lead::DIALOGUE_LEAD_COUNT,
            state.returned_dialogue_leads.len()
        ),
        format!(
            "线索转述：{} / {} 条，余波 {} 段，回声 {} 段，落点 {} 个，复看 {} 个",
            state.completed_dialogue_relays.len(),
            dialogue_relay::DIALOGUE_RELAY_COUNT,
            state.reflected_dialogue_relays.len(),
            state.echoed_dialogue_relays.len(),
            state.anchored_dialogue_relays.len(),
            state.reviewed_dialogue_anchors.len()
        ),
        format!(
            "回访对话：{} / {} 段",
            state.completed_aftertalks.len(),
            aftertalk::AFTERTALK_COUNT
        ),
        format!(
            "同行对话：{} / {} 段",
            state.completed_companion_talks.len(),
            companion::COMPANION_TALK_COUNT
        ),
        format!(
            "雾灯照证：{} / {} 处",
            state.focused_lamp_traces.len(),
            lamp_focus::LAMP_FOCUS_COUNT
        ),
        format!(
            "站内低语：{} / {} 段",
            state.heard_station_whispers.len(),
            station_whisper::WHISPER_COUNT
        ),
        format!(
            "路线准备：{} / {} 件",
            state.prepared_departures.len(),
            departure::DEPARTURE_COUNT
        ),
        format!(
            "路线试炼：{} / {} 场",
            state.rehearsed_departures.len(),
            trial::TRIAL_COUNT
        ),
        format!(
            "路线代价：{} / {} 项",
            state.mitigated_route_costs.len(),
            route_cost::ROUTE_COST_COUNT
        ),
        format!(
            "路线回声：{} / {} 段",
            state.completed_route_echoes.len(),
            route_echo::ROUTE_ECHO_COUNT
        ),
        format!(
            "路线争论：{} / {} 次",
            state.answered_route_pressures.len(),
            route_pressure::ROUTE_PRESSURE_COUNT
        ),
        format!(
            "路线现场：{} / {} 处",
            state.visited_route_witnesses.len(),
            route_witness::ROUTE_WITNESS_COUNT
        ),
        format!(
            "路线复盘：{} / {} 段",
            state.completed_route_witness_debriefs.len(),
            route_witness_debrief::ROUTE_WITNESS_DEBRIEF_COUNT
        ),
        format!(
            "终局前长谈：{} / {} 段",
            state.completed_final_interviews.len(),
            final_interview::FINAL_INTERVIEW_COUNT
        ),
        format!(
            "站内档案：{} / {} 份",
            state.resolved_case_files.len(),
            case_file::CASE_FILE_COUNT
        ),
        format!(
            "档案回谈：{} / {} 份",
            state.completed_case_dialogues.len(),
            case_dialogue::CASE_DIALOGUE_COUNT
        ),
        format!(
            "旅客委托：{} / {} 件",
            state.completed_requests.len(),
            station_request::REQUEST_COUNT
        ),
        format!(
            "回想整理：{} / {} 次",
            state.synthesis_depth,
            crate::model::NPC_THREAD_STEPS
        ),
        format!(
            "中段真相：{} / {} 层",
            state.revealed_truth_scenes.len(),
            truth::TRUTH_SCENE_COUNT
        ),
        format!(
            "终局前场景：{} / {} 幕",
            state.completed_ending_preludes.len(),
            final_prelude::ENDING_PRELUDE_COUNT
        ),
    ]
}

pub fn relationship_summaries(state: &GameState) -> Vec<RelationshipSummary> {
    vec![
        traveler_relationship(state),
        clerk_relationship(state),
        child_relationship(state),
        keeper_relationship(state),
    ]
}

pub fn run_recap(state: &GameState) -> Vec<RunRecapLine> {
    let total_location_steps = Location::ALL.len() as u16 * LOCATION_INVESTIGATION_STEPS as u16;
    let explored = state
        .location_depths
        .iter()
        .copied()
        .map(u16::from)
        .sum::<u16>();
    let conversation_depth = u16::from(state.traveler_depth)
        + u16::from(state.clerk_depth)
        + u16::from(state.child_depth)
        + u16::from(state.keeper_depth);
    let max_conversation_depth = 4_u16 * NPC_THREAD_STEPS as u16;

    vec![
        RunRecapLine {
            label: "结局",
            value: state
                .ended
                .map(|ending| ending.title().replace("结局：", ""))
                .unwrap_or_else(|| "尚未抵达".to_string()),
            detail: "本轮最终选择。不同路线会吸收你归档、委托和说话方式的余波。".to_string(),
            progress: if state.ended.is_some() { 100 } else { 0 },
        },
        RunRecapLine {
            label: "时间",
            value: format!("{} / {} 分钟", state.elapsed_minutes(), MAX_NIGHT_MINUTES),
            detail: format!(
                "午夜被切成 {} 段，你在 {}、第 {} 段抵达现在。",
                MAX_SEGMENTS,
                state.clock_text(),
                state.current_segment()
            ),
            progress: percent_u16(
                u16::from(state.elapsed_minutes()),
                u16::from(MAX_NIGHT_MINUTES),
            ),
        },
        RunRecapLine {
            label: "地点调查",
            value: format!("{explored} / {total_location_steps}"),
            detail: "车站里被翻开的细节。深层调查会改变路线和档案条件。".to_string(),
            progress: percent_u16(explored, total_location_steps),
        },
        RunRecapLine {
            label: "人物长谈",
            value: format!("{conversation_depth} / {max_conversation_depth}"),
            detail: format!(
                "自由话题 {} 个，证据追问 {} 件，人物共鸣 {} 组。",
                state.discussed_topics.len(),
                state.presented_evidence.len(),
                state.resolved_resonances.len()
            ),
            progress: percent_u16(conversation_depth, max_conversation_depth),
        },
        RunRecapLine {
            label: "对话系统",
            value: format!(
                "{} / {}",
                state.completed_dialogue_beats.len(),
                dialogue_system::DIALOGUE_BEAT_COUNT
            ),
            detail: format!(
                "当前对话会记录入口、话题节点、复谈、语气变化、自由询问和 NPC 反问；本轮 transcript 已保存 {} 段，自由询问已回答 {} / {} 个，立场回应 {} / {} 次。",
                state.dialogue_transcript.len(),
                state.answered_dialogue_questions.len(),
                dialogue_question::DIALOGUE_QUESTION_COUNT,
                state.answered_dialogue_challenges.len(),
                dialogue_challenge::DIALOGUE_CHALLENGE_COUNT
            ),
            progress: percent_usize(
                state.completed_dialogue_beats.len(),
                dialogue_system::DIALOGUE_BEAT_COUNT,
            ),
        },
        RunRecapLine {
            label: "中段真相",
            value: format!(
                "{} / {}",
                state.revealed_truth_scenes.len(),
                truth::TRUTH_SCENE_COUNT
            ),
            detail: "不是等结局解释一切，而是在探索中逐层揭开湿票、空座、返程、车站和广播室。"
                .to_string(),
            progress: percent_usize(state.revealed_truth_scenes.len(), truth::TRUTH_SCENE_COUNT),
        },
        RunRecapLine {
            label: "终局前场景",
            value: format!(
                "{} / {}",
                state.completed_ending_preludes.len(),
                final_prelude::ENDING_PRELUDE_COUNT
            ),
            detail: format!(
                "正向结局不再直接确认。每条路线都要先经历一段具体的最后现场，再作选择；本轮最后回应 {} 次。",
                state.ending_prelude_responses.len()
            ),
            progress: percent_usize(
                state.completed_ending_preludes.len(),
                final_prelude::ENDING_PRELUDE_COUNT,
            ),
        },
        RunRecapLine {
            label: "终局争辩",
            value: format!(
                "{} / {}",
                final_debate::completed_count(state),
                final_debate::FINAL_DEBATE_COUNT
            ),
            detail: "最终路线会被相关人物再次推回给你。回应争辩以后，结局才真正被关系承认。"
                .to_string(),
            progress: percent_usize(
                final_debate::completed_count(state),
                final_debate::FINAL_DEBATE_COUNT,
            ),
        },
        RunRecapLine {
            label: "结局余波",
            value: format!(
                "{} / {}",
                ending_aftermath::completed_count(state),
                ending_aftermath::ENDING_AFTERMATH_FRAGMENT_COUNT
            ),
            detail: "结局会继续回收第二天、留下的人、未归档缺口和最终回应，让终点不只是菜单按钮。"
                .to_string(),
            progress: percent_usize(
                ending_aftermath::completed_count(state),
                ending_aftermath::ENDING_AFTERMATH_FRAGMENT_COUNT,
            ),
        },
        RunRecapLine {
            label: "人物共鸣",
            value: format!(
                "{} / {}",
                state.resolved_resonances.len(),
                resonance::RESONANCE_COUNT
            ),
            detail: "把分散人物证词放到一起触发的交叉对话。它们让路线条件不只是清单，也像关系。"
                .to_string(),
            progress: percent_usize(state.resolved_resonances.len(), resonance::RESONANCE_COUNT),
        },
        RunRecapLine {
            label: "车站异象",
            value: format!(
                "{} / {}",
                state.resolved_anomalies.len(),
                anomaly::ANOMALY_COUNT
            ),
            detail: "午夜段落里显形的压力事件。你可以稳住它们，也可以追随它们进入更危险的真相。"
                .to_string(),
            progress: percent_usize(state.resolved_anomalies.len(), anomaly::ANOMALY_COUNT),
        },
        RunRecapLine {
            label: "内心锚点",
            value: format!("{} / {}", state.chosen_vows.len(), vow::VOW_COUNT),
            detail: "发现真相以后写下的立场。它们会进入终局余波，让选择更像由你承担。".to_string(),
            progress: percent_usize(state.chosen_vows.len(), vow::VOW_COUNT),
        },
        RunRecapLine {
            label: "记忆回廊",
            value: format!(
                "{} / {}",
                state.visited_memories.len(),
                memory::MEMORY_COUNT
            ),
            detail: "地点里保存的中后段回忆。它们把线索落回具体场景，并补强路线理解。"
                .to_string(),
            progress: percent_usize(state.visited_memories.len(), memory::MEMORY_COUNT),
        },
        RunRecapLine {
            label: "巡夜记录",
            value: format!("{} / {}", state.completed_patrols.len(), patrol::PATROL_COUNT),
            detail: "中后段回访地点写下的空间记录。它们让自由探索不只是翻找物品，也是在重新登记这个夜晚。"
                .to_string(),
            progress: percent_usize(state.completed_patrols.len(), patrol::PATROL_COUNT),
        },
        RunRecapLine {
            label: "对话线索",
            value: format!(
                "{} / {}",
                state.completed_dialogue_leads.len(),
                dialogue_lead::DIALOGUE_LEAD_COUNT
            ),
            detail: format!(
                "人物对话落到地点里的痕迹。已带回人物对话 {} 条，让证据重新变成回应。",
                state.returned_dialogue_leads.len()
            ),
            progress: percent_usize(
                state.completed_dialogue_leads.len(),
                dialogue_lead::DIALOGUE_LEAD_COUNT,
            ),
        },
        RunRecapLine {
            label: "线索转述",
            value: format!(
                "{} / {}",
                state.completed_dialogue_relays.len(),
                dialogue_relay::DIALOGUE_RELAY_COUNT
            ),
            detail: format!(
                "把回谈后的事实转述给另一位人物。它让人物不再只回答主角，而是开始互相影响。已继续追问 {} 段转述余波，带回 {} 段转述回声，安放 {} 个回声落点，并复看 {} 个地点余痕。",
                state.reflected_dialogue_relays.len(),
                state.echoed_dialogue_relays.len(),
                state.anchored_dialogue_relays.len(),
                state.reviewed_dialogue_anchors.len()
            ),
            progress: percent_usize(
                state.completed_dialogue_relays.len(),
                dialogue_relay::DIALOGUE_RELAY_COUNT,
            ),
        },
        RunRecapLine {
            label: "回访对话",
            value: format!(
                "{} / {}",
                state.completed_aftertalks.len(),
                aftertalk::AFTERTALK_COUNT
            ),
            detail: "完成巡夜、记忆或路线准备后回到人物身边触发的动态回应。它们让自由对话跟随行动变化。"
                .to_string(),
            progress: percent_usize(
                state.completed_aftertalks.len(),
                aftertalk::AFTERTALK_COUNT,
            ),
        },
        RunRecapLine {
            label: "同行对话",
            value: format!(
                "{} / {}",
                state.completed_companion_talks.len(),
                companion::COMPANION_TALK_COUNT
            ),
            detail:
                "孩子愿意同行后，在不同地点触发的自由对话。它们让探索不只是找到线索，也让同行者拥有自己的判断。"
                    .to_string(),
            progress: percent_usize(
                state.completed_companion_talks.len(),
                companion::COMPANION_TALK_COUNT,
            ),
        },
        RunRecapLine {
            label: "雾灯照证",
            value: format!(
                "{} / {}",
                state.focused_lamp_traces.len(),
                lamp_focus::LAMP_FOCUS_COUNT
            ),
            detail: "修复雾灯后用光重新勘验地点。它们把隐藏事实照进地图，让自由探索进入中后段。".to_string(),
            progress: percent_usize(state.focused_lamp_traces.len(), lamp_focus::LAMP_FOCUS_COUNT),
        },
        RunRecapLine {
            label: "站内低语",
            value: format!(
                "{} / {}",
                state.heard_station_whispers.len(),
                station_whisper::WHISPER_COUNT
            ),
            detail: "每处地点在不同午夜段露出的环境短场景。它们增加自由探索密度，也让车站像一群声音而不是谜题机关。".to_string(),
            progress: percent_usize(state.heard_station_whispers.len(), station_whisper::WHISPER_COUNT),
        },
        RunRecapLine {
            label: "路线准备",
            value: format!(
                "{} / {}",
                state.prepared_departures.len(),
                departure::DEPARTURE_COUNT
            ),
            detail: "最终选择前亲手做下的具体准备。它们让结局更像被铺出来，而不是临场按钮。"
                .to_string(),
            progress: percent_usize(state.prepared_departures.len(), departure::DEPARTURE_COUNT),
        },
        RunRecapLine {
            label: "路线试炼",
            value: format!("{} / {}", state.rehearsed_departures.len(), trial::TRIAL_COUNT),
            detail: "最终选择前的路线预演。它们把结局从按钮变成玩家已经承受过一次的压力。".to_string(),
            progress: percent_usize(state.rehearsed_departures.len(), trial::TRIAL_COUNT),
        },
        RunRecapLine {
            label: "路线代价",
            value: format!(
                "{} / {}",
                state.mitigated_route_costs.len(),
                route_cost::ROUTE_COST_COUNT
            ),
            detail: "路线试炼之后暴露出的后果处理。它们让终局选择先承担代价，而不是只满足条件。".to_string(),
            progress: percent_usize(state.mitigated_route_costs.len(), route_cost::ROUTE_COST_COUNT),
        },
        RunRecapLine {
            label: "路线回声",
            value: format!(
                "{} / {}",
                state.completed_route_echoes.len(),
                route_echo::ROUTE_ECHO_COUNT
            ),
            detail: "调停路线代价后回到人物或地点触发的自由谈话。它们让代价重新进入关系，而不只留在路线页。".to_string(),
            progress: percent_usize(state.completed_route_echoes.len(), route_echo::ROUTE_ECHO_COUNT),
        },
        RunRecapLine {
            label: "路线争论",
            value: format!(
                "{} / {}",
                state.answered_route_pressures.len(),
                route_pressure::ROUTE_PRESSURE_COUNT
            ),
            detail: "人物在中段就会察觉你的路线倾向并顶回来。回应以后，路线不再只是玩家脑内的选择。"
                .to_string(),
            progress: percent_usize(
                state.answered_route_pressures.len(),
                route_pressure::ROUTE_PRESSURE_COUNT,
            ),
        },
        RunRecapLine {
            label: "路线现场",
            value: format!(
                "{} / {}",
                state.visited_route_witnesses.len(),
                route_witness::ROUTE_WITNESS_COUNT
            ),
            detail: "路线回声之后回到具体地点完成的现场见证。它让终局路线落进空间，而不只停在人和菜单里。"
                .to_string(),
            progress: percent_usize(
                state.visited_route_witnesses.len(),
                route_witness::ROUTE_WITNESS_COUNT,
            ),
        },
        RunRecapLine {
            label: "路线复盘",
            value: format!(
                "{} / {}",
                state.completed_route_witness_debriefs.len(),
                route_witness_debrief::ROUTE_WITNESS_DEBRIEF_COUNT
            ),
            detail: "路线现场完成后带回人物主动对话里的复盘。它让地点见证重新进入关系，不让终局路线停成空间摆设。"
                .to_string(),
            progress: percent_usize(
                state.completed_route_witness_debriefs.len(),
                route_witness_debrief::ROUTE_WITNESS_DEBRIEF_COUNT,
            ),
        },
        RunRecapLine {
            label: "终局前长谈",
            value: format!(
                "{} / {}",
                state.completed_final_interviews.len(),
                final_interview::FINAL_INTERVIEW_COUNT
            ),
            detail: "列车进站前，把已见证、已复盘的路线再带回人物主动对话里。它让终局不只是地点和条件，也有人真的听完。"
                .to_string(),
            progress: percent_usize(
                state.completed_final_interviews.len(),
                final_interview::FINAL_INTERVIEW_COUNT,
            ),
        },
        RunRecapLine {
            label: "站内档案",
            value: format!(
                "{} / {}",
                state.resolved_case_files.len(),
                case_file::CASE_FILE_COUNT
            ),
            detail: "组合推理归档。越多真相被整理，终局越难把你说成只是情绪。".to_string(),
            progress: percent_usize(state.resolved_case_files.len(), case_file::CASE_FILE_COUNT),
        },
        RunRecapLine {
            label: "档案回谈",
            value: format!(
                "{} / {}",
                state.completed_case_dialogues.len(),
                case_dialogue::CASE_DIALOGUE_COUNT
            ),
            detail: "把已归档的真相带回人物自由对话里，让推理被当事人反问，而不是停在清单上。"
                .to_string(),
            progress: percent_usize(
                state.completed_case_dialogues.len(),
                case_dialogue::CASE_DIALOGUE_COUNT,
            ),
        },
        RunRecapLine {
            label: "旅客委托",
            value: format!(
                "{} / {}",
                state.completed_requests.len(),
                station_request::REQUEST_COUNT
            ),
            detail: "被你做完的小事。它们不会替你把账抹平，但会替后来者留下路标。".to_string(),
            progress: percent_usize(
                state.completed_requests.len(),
                station_request::REQUEST_COUNT,
            ),
        },
        RunRecapLine {
            label: "说话方式",
            value: state.dialogue_tone.name().to_string(),
            detail: "本轮最后选择的语气。它会影响自由对话中的信任和终局余波。".to_string(),
            progress: 100,
        },
    ]
}

pub fn route_summaries(state: &GameState) -> Vec<RouteSummary> {
    vec![
        lost_route(state),
        alone_route(state),
        child_route(state),
        burn_route(state),
        broadcast_route(state),
        keeper_route(state),
    ]
}

pub fn ending_archive_lines(session: &GameSession) -> Vec<String> {
    let seen_count = session.endings_seen.len();
    let mut lines = vec![format!("结局档案：{} / {}", seen_count, Ending::ALL.len())];

    if seen_count == 0 {
        lines.push("尚未通关；通关后会保留路线记录".to_string());
    } else {
        let seen = Ending::ALL
            .iter()
            .copied()
            .filter(|ending| session.endings_seen.contains(ending))
            .map(Ending::short_title)
            .collect::<Vec<_>>()
            .join(" / ");
        lines.push(format!("已见：{seen}"));
    }

    if let Some(current) = session.state.ended {
        lines.push(format!("本轮：{}", current.short_title()));
    } else if let Some(next) = Ending::ALL
        .iter()
        .copied()
        .find(|ending| !session.endings_seen.contains(ending))
    {
        lines.push(format!("未见路线提示：{}", next.short_title()));
    }

    if seen_count == Ending::ALL.len() {
        lines.push("全部结局已点亮".to_string());
    }

    lines
}

fn traveler_relationship(state: &GameState) -> RelationshipSummary {
    let progress = depth_progress(state.traveler_depth);
    RelationshipSummary {
        name: "候车厅老人",
        status: if state.traveler_depth >= NPC_THREAD_STEPS {
            "已摊牌"
        } else if state.traveler_depth >= 6 || state.has_flag(Flag::TravelerTrusted) {
            "认得你"
        } else if state.traveler_depth > 0 {
            "试探"
        } else {
            "陌生"
        },
        detail: if state.traveler_depth >= NPC_THREAD_STEPS {
            "他已经把钥匙、报纸和归来的理由都交给你；剩下的是你怎样使用这些旧证词。".to_string()
        } else if state.has_flag(Flag::ExaminedTicket) && !state.has_flag(Flag::TravelerTrusted) {
            "湿票可以递给老人。不是求证，而是让他确认你终于愿意读完那半句。".to_string()
        } else {
            "继续和他谈钥匙、雨和报纸日期；老人线会打开铜筹、旧循环与月台证词。".to_string()
        },
        progress,
    }
}

fn clerk_relationship(state: &GameState) -> RelationshipSummary {
    let progress = social_progress(state.clerk_depth, state.clerk_trust);
    RelationshipSummary {
        name: "售票员",
        status: if state.ticket == TicketKind::Return {
            "已改签"
        } else if state.clerk_trust >= 3 {
            "愿意破例"
        } else if state.clerk_depth > 0 || state.has_flag(Flag::ClerkMet) {
            "公事公办"
        } else {
            "未接近"
        },
        detail: if state.ticket == TicketKind::Return {
            "返程联票已经成立。售票窗口不会替你选择同行者，但已经承认两张座位。".to_string()
        } else if !state.has_item(Item::StationLog) {
            "她需要看到站务日志，才会停止把你当成普通乘客。".to_string()
        } else if !state.has_item(Item::CoinToken) {
            "日志已经够她开口，退票铜筹会让返程规则变得具体。".to_string()
        } else {
            "日志、铜筹和信任正在接近改签条件；继续追问座位与代价。".to_string()
        },
        progress,
    }
}

fn child_relationship(state: &GameState) -> RelationshipSummary {
    let progress = social_progress(state.child_depth, state.child_trust);
    RelationshipSummary {
        name: if state.has_flag(Flag::MetChild) {
            "白线后的孩子"
        } else {
            "白线后的人影"
        },
        status: if state.has_flag(Flag::ChildJoined) {
            "愿意同行"
        } else if state.child_trust >= 4 {
            "正在相信"
        } else if state.has_flag(Flag::MetChild) {
            "戒备"
        } else {
            "尚未相遇"
        },
        detail: if !state.has_flag(Flag::MetChild) {
            "三号月台白线后有个很小的轮廓。遇见他之前，很多关于离开的答案都还只是成人的独白。"
                .to_string()
        } else if state.has_flag(Flag::ChildJoined) {
            "他已经愿意同行，但这不是免罪券；返程仍需要两个人都被车站承认。".to_string()
        } else if !state.has_flag(Flag::UnderstoodChildPromise) {
            "他要的不是道歉表演。读懂湿票第二个名字，或让他说出那句旧命令。".to_string()
        } else {
            "姓名牌、作业本和足够具体的行动会让他判断这次是不是又一场保证。".to_string()
        },
        progress,
    }
}

fn keeper_relationship(state: &GameState) -> RelationshipSummary {
    let progress = social_progress(state.keeper_depth, state.keeper_trust);
    RelationshipSummary {
        name: "旧钟楼站务员",
        status: if state.has_flag(Flag::UnderstoodStationMechanism) {
            "承认真相"
        } else if state.keeper_trust >= 3 {
            "松口"
        } else if state.keeper_depth > 0 {
            "守口"
        } else {
            "值夜"
        },
        detail: if state.has_flag(Flag::AlignedClock) {
            "旧钟已经重新承认时间。站务员线现在会把问题推向发车、广播室和外套。".to_string()
        } else if !state.has_item(Item::OldTimetable) {
            "烧焦的旧时刻表能逼他承认车站规则不是命运。".to_string()
        } else if !state.has_flag(Flag::RepairedFogLamp) {
            "时刻表已经在手，修好雾灯后，他才会交出更深的责任。".to_string()
        } else {
            "证据足够逼近发车哨；继续追问旧钟、广播线和留下来的代价。".to_string()
        },
        progress,
    }
}

fn depth_progress(depth: u8) -> u8 {
    ((u16::from(depth.min(NPC_THREAD_STEPS)) * 100) / u16::from(NPC_THREAD_STEPS)) as u8
}

fn social_progress(depth: u8, trust: i8) -> u8 {
    let depth_part = (u16::from(depth.min(NPC_THREAD_STEPS)) * 60) / u16::from(NPC_THREAD_STEPS);
    let trust_part = u16::from(trust.clamp(0, 5) as u8) * 8;
    (depth_part + trust_part).min(100) as u8
}

fn lost_route(state: &GameState) -> RouteSummary {
    let chosen = state.ended == Some(Ending::LostPassenger);
    RouteSummary {
        title: Ending::LostPassenger.title(),
        status: if chosen { "已抵达" } else { "默认阴影" },
        detail: if chosen {
            "你已经让雾灯号替你决定。下一次醒来时，湿票会更淡一点。".to_string()
        } else {
            "如果最终什么都不选，车站会把犹豫写成结局。它不是失败提示，而是雾灯站最熟练的保存方式。"
                .to_string()
        },
        progress: if state.final_train_due() { 100 } else { 35 },
    }
}

fn alone_route(state: &GameState) -> RouteSummary {
    let recovered = state.has_flag(Flag::RecoveredName);
    let has_departure_proof =
        state.ticket == TicketKind::Return || state.has_flag(Flag::InspectedRails);
    let progress = percent([recovered, has_departure_proof]);
    RouteSummary {
        title: Ending::EscapedAlone.title(),
        status: route_status(state, Ending::EscapedAlone, progress),
        detail: missing_or_ready(
            state,
            progress,
            "你已经能独自上车：姓名回到你身上，返程或轨道证据足以让车门承认你。",
            &[
                (!recovered, "去地下通道找回姓名"),
                (!has_departure_proof, "取得返程票，或检查轨道尽头的返程轮痕"),
            ],
        ),
        progress,
    }
}

fn child_route(state: &GameState) -> RouteSummary {
    let met = state.has_flag(Flag::MetChild);
    let understands = state.has_flag(Flag::UnderstoodChildPromise);
    let joined = state.has_flag(Flag::ChildJoined);
    let has_return_truth =
        state.ticket == TicketKind::Return || state.has_flag(Flag::SynthesizedChildTruth);
    let progress = percent([met, understands, joined, has_return_truth]);
    RouteSummary {
        title: if met {
            Ending::TookChildHome.title()
        } else {
            "结局：白线后的空位"
        },
        status: route_status(state, Ending::TookChildHome, progress),
        detail: if !met {
            "这条路线还没有露出名字。先去三号月台，确认白线后那个很小的轮廓究竟是不是在等人。"
                .to_string()
        } else {
            missing_or_ready(
                state,
                progress,
                "你已经能带孩子返程：他不是结局奖励，而是愿意自己跨过白线、和你一起走向明天的人。",
                &[
                    (!understands, "读懂湿票第二个名字，或和孩子谈承诺"),
                    (!joined, "归还姓名牌，让他自己决定是否同行"),
                    (!has_return_truth, "拿到返程票，或整理孩子与姓名的真相"),
                ],
            )
        },
        progress,
    }
}

fn burn_route(state: &GameState) -> RouteSummary {
    let timetable = state.has_item(Item::OldTimetable);
    let lamp = state.has_flag(Flag::RepairedFogLamp);
    let truth = state.has_flag(Flag::SynthesizedStationTruth);
    let progress = percent([timetable, lamp, truth]);
    RouteSummary {
        title: Ending::BurnedTimetable.title(),
        status: route_status(state, Ending::BurnedTimetable, progress),
        detail: missing_or_ready(
            state,
            progress,
            "你已经能烧掉时刻表：雾灯照见路，旧纸承认规则可以被撤销。",
            &[
                (!timetable, "打开失物招领处铁柜，找到烧焦的旧时刻表"),
                (!lamp, "修复地下通道的雾灯"),
                (!truth, "整理广播室、旧钟和雾灯线索，理解车站真相"),
            ],
        ),
        progress,
    }
}

fn broadcast_route(state: &GameState) -> RouteSummary {
    let name = state.has_flag(Flag::RecoveredName);
    let log = state.has_item(Item::StationLog);
    let tape = state.has_item(Item::BroadcastTape);
    let clock = state.has_flag(Flag::AlignedClock);
    let progress = percent([name, log, tape, clock]);
    RouteSummary {
        title: Ending::BecameTheVoice.title(),
        status: route_status(state, Ending::BecameTheVoice, progress),
        detail: missing_or_ready(
            state,
            progress,
            "你已经能走进广播室：完整姓名会成为警告，也会把你留在声音里。",
            &[
                (!name, "找回自己的姓名"),
                (!log, "取得站务日志"),
                (!tape, "在钟楼或证据追问中找到广播磁带"),
                (!clock, "校准旧钟，让 23:59 之后真正发生"),
            ],
        ),
        progress,
    }
}

fn keeper_route(state: &GameState) -> RouteSummary {
    let clock = state.has_flag(Flag::HeardClockTruth);
    let mechanism = state.has_flag(Flag::UnderstoodStationMechanism);
    let progress = percent([clock, mechanism]);
    RouteSummary {
        title: Ending::NewStationKeeper.title(),
        status: route_status(state, Ending::NewStationKeeper, progress),
        detail: missing_or_ready(
            state,
            progress,
            "你已经能接过外套：留下不再只是惩罚，而是一种会继续伤人也继续照路的选择。",
            &[
                (!clock, "和站务员谈旧钟，听懂最后一分钟的代价"),
                (!mechanism, "用时刻表、日志或深层调查理解车站机制"),
            ],
        ),
        progress,
    }
}

fn route_status(state: &GameState, ending: Ending, progress: u8) -> &'static str {
    if state.ended == Some(ending) {
        "已抵达"
    } else if progress >= 100 {
        "可选择"
    } else if progress >= 50 {
        "接近"
    } else {
        "未成形"
    }
}

fn missing_or_ready(
    state: &GameState,
    progress: u8,
    ready: &'static str,
    missing: &[(bool, &'static str)],
) -> String {
    if state.ended.is_some() || progress >= 100 {
        return ready.to_string();
    }

    let missing = missing
        .iter()
        .filter_map(|(needed, text)| needed.then_some(*text))
        .take(3)
        .collect::<Vec<_>>();
    if missing.is_empty() {
        ready.to_string()
    } else {
        format!("还缺：{}。", missing.join("；"))
    }
}

fn percent<const N: usize>(checks: [bool; N]) -> u8 {
    if N == 0 {
        return 0;
    }
    let passed = checks.iter().filter(|passed| **passed).count();
    ((passed * 100) / N) as u8
}

fn percent_u16(value: u16, total: u16) -> u8 {
    if total == 0 {
        0
    } else {
        ((value.min(total) * 100) / total) as u8
    }
}

fn percent_usize(value: usize, total: usize) -> u8 {
    if total == 0 {
        0
    } else {
        ((value.min(total) * 100) / total) as u8
    }
}

pub fn item_description(item: Item) -> &'static str {
    match item {
        Item::WetTicket => "一张湿透的旧车票。背面写着“别上车”，更深的划痕还看不清。",
        Item::BrassKey => "黄铜钥匙，可以打开失物招领处深处的铁柜，也可能用于旧钟。",
        Item::LanternGlass => "雾灯缺失的玻璃。修好雾灯后，月台远端会出现更多线索。",
        Item::OldTimetable => "烧焦的时刻表。它能证明雾灯站的路线曾被人为改过。",
        Item::StationLog => {
            "站务日志。里面记录了午夜循环、站务员巡逻，以及你一次次试图跨过白线的失败。"
        }
        Item::NameTag => "裂开的姓名牌。它证明那不是比喻，而是当年和你一起逃到这里的人。",
        Item::SignalWhistle => "银色发车哨。旧钟校准后，吹响它可以召回雾灯号。",
        Item::StationMap => "站内图被折过很多次，背面用铅笔标着一条没有竣工的疏散通道。",
        Item::ChildHomework => {
            "孩子的作业本。里面画着两个人坐火车，也记录了他为什么一直不敢越过白线。"
        }
        Item::BroadcastTape => "广播磁带。它和广播室、当年那句命令、留下来的结局有关。",
        Item::ConductorRoster => "列车员名册缺了最后一页，可以追问雾灯号是否真的能返程。",
        Item::MirrorShard => "候车厅的镜片。它能帮助你读出车票水痕里的第二个名字。",
        Item::CoinToken => "退票铜筹。背面写着：返程需要两个人承认，也需要撤销一条错误命令。",
    }
}

pub fn investigation_label(location: Location, depth: u8) -> String {
    let label = match location {
        Location::WaitingHall => match depth {
            0 => "查看长椅下的站内图",
            1 => "检查售货机破裂镜面",
            2 => "核对缺失的座椅编号",
            3 => "查看扶手背面的刻字",
            4 => "监听广播报站前的停顿",
            5 => "检查老人报纸上的水痕",
            6 => "寻找 07 号寄存柜",
            7 => "追踪天花板上的广播线",
            8 => "观察售票窗口的倒影",
            9 => "记录电子屏异常车次",
            10 => "整理候车厅最后线索",
            _ => "复查候车厅",
        },
        Location::TicketOffice => match depth {
            0 => "检查退票口",
            1 => "打开抽屉寻找铜筹",
            2 => "检查柜台票章",
            3 => "查看窗口后的空椅子",
            4 => "阅读退票规则账本",
            5 => "检查玻璃内侧裂纹",
            6 => "研究湿票烘干机",
            7 => "查看被圈出的座位图",
            8 => "读取售票机系统提示",
            9 => "翻看单程票碎片",
            10 => "检查售票员白手套",
            _ => "复查售票窗口",
        },
        Location::LostAndFound => match depth {
            0 => "阅读第一排失物标签",
            1 => "检查童衣口袋",
            2 => "寻找雾灯玻璃",
            3 => "打开铁盒寻找姓名牌",
            4 => "查看无人领取的道歉信",
            5 => "阅读站务档案索引",
            6 => "检查少了一只鞋的鞋柜",
            7 => "打开没有编号的箱子",
            8 => "检查旧站务员外套",
            9 => "寻找广播磁带盒",
            10 => "阅读招领处账册",
            _ => "复查失物招领处",
        },
        Location::Underpass => match depth {
            0 => "听地下通道的回声",
            1 => "测量墙砖水线",
            2 => "检查相反方向的疏散标志",
            3 => "阅读循环申请记录",
            4 => "追问回声里的名字",
            5 => "播放地下广播旧录音",
            6 => "检查潮湿粉笔画",
            7 => "寻找地下风的来源",
            8 => "数没有尽头的台阶",
            9 => "监听墙后的候车室声音",
            10 => "校准脚步和回声",
            _ => "复查地下通道",
        },
        Location::ClockTower => match depth {
            0 => "检查停止的分针",
            1 => "读取齿轮编号",
            2 => "在钟楼抽屉里找磁带",
            3 => "检查广播线总闸",
            4 => "查看钟腹里的钥匙孔",
            5 => "阅读站务员值夜表",
            6 => "检查雾灯控制杆",
            7 => "从钟楼俯看月台",
            8 => "阅读钟声草稿",
            9 => "检查站务员茶杯",
            10 => "查看钟面背后的纸条",
            _ => "复查旧钟楼",
        },
        Location::Platform => match depth {
            0 => "检查月台白线",
            1 => "查看列车员名册",
            2 => "比较铁轨上的两组轮痕",
            3 => "测量车门边刻度",
            4 => "阅读三号月台值班记录",
            5 => "观察远端红灯",
            6 => "捡起作业本页角",
            7 => "寻找广播室的窄门",
            8 => "查看铁轨里的 07 号长椅倒影",
            9 => "观察雾里的乘客影子",
            10 => "确认列车是否接近",
            _ => "复查三号月台",
        },
    };
    label.to_string()
}

pub fn investigation_detail(location: Location, depth: u8) -> String {
    let hint = match (location, depth) {
        (Location::WaitingHall, 0) => "从长椅、镜面和电子屏找第一批线索。",
        (Location::WaitingHall, depth) if depth >= 3 => "候车厅开始回应你之前留下的安排。",
        (Location::TicketOffice, 0) => "查玻璃、抽屉和退票口的细节。",
        (Location::TicketOffice, depth) if depth >= 3 => "售票系统会暴露返程票的规则。",
        (Location::LostAndFound, 0) => "从失物标签里找出和你有关的物件。",
        (Location::LostAndFound, depth) if depth >= 3 => "深处铁柜会指向站务员的旧档案。",
        (Location::Underpass, 0) => "让回声慢慢拼出名字之前的事。",
        (Location::Underpass, depth) if depth >= 3 => "墙砖和疏散标志能解释循环怎么开始。",
        (Location::ClockTower, 0) => "齿轮、椅子和广播线都藏着站务员的选择。",
        (Location::ClockTower, depth) if depth >= 3 => "旧钟会说明列车为什么需要最后一分钟。",
        (Location::Platform, 0) => "查轨道、白线和远端车灯的真实方向。",
        (Location::Platform, depth) if depth >= 3 => "月台会把孩子和返程条件串起来。",
        _ => "继续把这个地点翻到下一层。",
    };
    hint.to_string()
}

pub fn investigation_event(location: Location, depth: u8) -> StoryEvent {
    let (title, body, tag) = match location {
        Location::WaitingHall => waiting_hall_step(depth),
        Location::TicketOffice => ticket_office_step(depth),
        Location::LostAndFound => lost_found_step(depth),
        Location::Underpass => underpass_step(depth),
        Location::ClockTower => clock_tower_step(depth),
        Location::Platform => platform_step(depth),
    };
    StoryEvent::new(title, body).tag(tag)
}

fn waiting_hall_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "长椅下的站内图",
            "你在长椅下找到一张折叠站内图。图上标出六个地点：候车厅、售票窗口、失物招领处、地下通道、旧钟楼和三号月台。",
            "候车厅",
        ),
        1 => (
            "破裂的镜面",
            "自动售货机的镜面裂成两半。一半映出你现在的脸，另一半映出更疲惫的你，正用口型说：去三号月台，不要先去售票口。",
            "镜片",
        ),
        2 => (
            "座椅编号",
            "候车厅座椅从 01 排到 47，却没有 07。地面上有拖痕，说明 07 号座椅曾被移走。",
            "缺席",
        ),
        3 => (
            "第一次循环的刻痕",
            "扶手背面刻着你的笔迹：如果还会醒来，先找第七排那个空出来的位置。这证明你以前知道自己会忘掉某个重要的人。",
            "循环",
        ),
        4 => (
            "广播前的停顿",
            "广播每次报站前都会停顿半秒。你终于听出那不是故障，而是有人把另一个名字剪掉了，只留下请遗失姓名的旅客。",
            "广播",
        ),
        5 => (
            "报纸里的旧雨",
            "老人读过的报纸边缘是湿的，湿痕却绕开日期。你意识到这场雨不是天气，而是六年前那晚被车站反复保存的一种证词。",
            "旧雨",
        ),
        6 => (
            "寄存柜 07",
            "一排寄存柜背后藏着 07 号小门，门缝里塞着半张儿童票。票面没有价格，只有一行小字：同行者不得遗忘同行者。",
            "儿童票",
        ),
        7 => (
            "天花板上的线路",
            "你顺着天花板裂缝看见广播线从候车厅穿过，分成两束：一束通向钟楼，一束通向月台远端那片最浓的雾。",
            "线路",
        ),
        8 => (
            "售票窗口的倒影",
            "从候车厅看售票窗口，玻璃后坐着的不只是售票员。倒影里还有你，正站在窗口前把一张单程票撕成两半。",
            "倒影",
        ),
        9 => (
            "不存在的车次",
            "电子屏闪出一班不存在的车：K000，始发雾灯站，终到明天。它只出现一秒，足够你明白车站并非没有出口，只是出口不承认独自抵达。",
            "车次",
        ),
        10 => (
            "候车厅最后线索",
            "你把候车厅线索整理到一起：湿票、老人、缺失的第七排长椅和广播停顿，都指向某个被车站反复省略的人。",
            "浅眠",
        ),
        _ => (
            "长椅尽头",
            "你复查候车厅，没有找到新的物件。这里的主要线索已经集中在车票、老人和第七排空位上。",
            "候车厅深层",
        ),
    }
}

fn ticket_office_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "退票口的冷光",
            "售票窗口下方没有售票口，只有退票口。绿灯照着一行细字：本窗口不出售未来，只受理未完成之事。",
            "售票窗口",
        ),
        1 => (
            "抽屉里的铜筹",
            "抽屉没有上锁，里面有一枚退票铜筹。铜筹背面写着：返程需要两个人承认。",
            "退票铜筹",
        ),
        2 => (
            "票章的声音",
            "你按下票章，发现它只会盖出“退票”和“返程”两种字样。这里不是普通售票处。",
            "票章",
        ),
        3 => (
            "窗口里的第二张椅子",
            "售票员身后有第二张空椅子，椅背上贴着临时工三个字。你突然知道，车站总会给留下的人准备一份看似体面的称呼。",
            "空椅",
        ),
        4 => (
            "退票规则",
            "柜台账本写着：退票者必须退还一件从六年前那晚带走的东西。有人退了影子，有人退了姓名，你曾经试图退还记忆。",
            "退票规则",
        ),
        5 => (
            "窗口裂纹",
            "玻璃裂纹从内侧开始，不是乘客砸的。售票员也曾想逃出去，只是发现窗口外的人比她更需要有人留在里面。",
            "裂纹",
        ),
        6 => (
            "湿票烘干机",
            "墙角有一台老式烘票机。说明书上说，湿票不能直接烘干，除非持票人已经承认车票为什么被雨淋湿。",
            "湿票",
        ),
        7 => (
            "座位图",
            "座位图上 07A 和 07B 被圈了很多次。这说明返程一直要求两个座位，不是临时规则。",
            "座位",
        ),
        8 => (
            "旧系统提示",
            "售票机屏幕弹出错误：同行者字段不能为空。你按取消，提示又出现：这不是技术错误。",
            "系统提示",
        ),
        9 => (
            "纸篓里的单程票",
            "纸篓装满撕碎的单程票。每一张都写着你的名字，每一张目的地都不同，只有背面的水痕完全一样。",
            "单程",
        ),
        10 => (
            "售票员的手套",
            "柜台上放着一副白手套，指尖磨破。她每天处理别人的选择，却不能用裸手触碰任何一张真正要离站的票。",
            "手套",
        ),
        _ => (
            "窗口之后",
            "你复查售票窗口。这里的主要线索已经集中在退票铜筹、返程规则、座位图和售票员的证词上。",
            "窗口深层",
        ),
    }
}

fn lost_found_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "失物标签",
            "第一排箱子是普通失物，后面的标签开始变成道歉信、旧承诺和白线旧物。这里保存的是那晚之后遗留的证据。",
            "失物招领",
        ),
        1 => (
            "童衣口袋",
            "童衣口袋里有糖纸和半截铅笔。它们和月台上的孩子有关。",
            "童衣",
        ),
        2 => (
            "雾灯玻璃",
            "你找到一片雾灯玻璃。这是修复雾灯需要的物件。",
            "雾灯玻璃",
        ),
        3 => (
            "裂开的姓名牌",
            "铁盒里有一块姓名牌，裂缝正好穿过两个名字之间。你握住它时，耳边响起孩子压低的声音：不要把我的名字也弄丢。",
            "姓名牌",
        ),
        4 => (
            "无人领取的道歉信",
            "抽屉里都是无人领取的道歉信。它们提醒你：道歉如果没有行动，就只会一直留在这里。",
            "道歉信",
        ),
        5 => (
            "站务档案索引",
            "柜底有一本索引，记录每个留下来的人换走了什么。你的条目被反复划掉又重写，最后只剩：请求保留最后一分钟。",
            "索引",
        ),
        6 => (
            "孩子的小鞋",
            "鞋柜里少了一只小鞋。留下的那只鞋属于孩子，鞋底边缘压着白粉，说明他曾在白线后站了很久。",
            "小鞋",
        ),
        7 => (
            "没有编号的箱子",
            "最里面有个没有编号的箱子，打开后却是空的。空箱底部写着：有些东西不是遗失，是被幸存者主动放下。",
            "空箱",
        ),
        8 => (
            "站务员的旧外套",
            "你摸到一件旧外套，领口有烧痕。它还没失去影子，说明它属于某个最终没有成为站务员的人。",
            "外套",
        ),
        9 => (
            "广播磁带盒",
            "磁带盒外壳裂开，里面空着。标签上是你的字：不要播完整姓名，除非你准备留下。",
            "磁带盒",
        ),
        10 => (
            "招领处的账",
            "账册显示，车站从不丢弃无人领取的东西。它们会被放回夜里，成为下一次循环可以捡到的证据。",
            "账册",
        ),
        _ => (
            "最深的柜门",
            "你复查失物招领处。这里的主要线索已经集中在雾灯玻璃、姓名牌、站务日志和旧时刻表上。",
            "招领处深层",
        ),
    }
}

fn underpass_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "慢半拍的脚步",
            "地下通道里的回声总比你的脚步慢半拍。这个异常会在之后帮助你找回姓名。",
            "地下通道",
        ),
        1 => (
            "墙砖上的水线",
            "墙砖水线停在孩子肩膀的高度。六年前那晚的雨并没有灌进通道，但车站仍把水位记在这里。",
            "水线",
        ),
        2 => (
            "疏散标志",
            "疏散标志指向两个相反方向。一边写出口，一边写三号月台。你终于明白，那天对孩子来说，月台才是出口。",
            "疏散",
        ),
        3 => (
            "循环开始的地方",
            "通道尽头刻着一段站务员记录：乘客请求保留 23:59 至 00:00，以便回收遗失同行者。申请人签名是你。",
            "循环机制",
        ),
        4 => (
            "回声叫出半个名字",
            "你对着墙问自己是谁。回声先说出孩子的姓，再说出你的名，中间缺了一段，说明两个名字被分开了。",
            "姓名",
        ),
        5 => (
            "通道里的广播",
            "地下广播比候车厅清晰。你听见旧录音里的自己反复试读同一句警告，每一次都在第二个名字前停住。",
            "录音",
        ),
        6 => (
            "潮湿的粉笔画",
            "墙角有一幅粉笔画：两个小人牵手站在列车门口。雨把其中一个冲淡，另一个被孩子用力描了很多遍。",
            "粉笔画",
        ),
        7 => (
            "风从地下吹来",
            "一阵风从更深处吹来，带着热牛奶和铁轨的味道。你意识到车站并非封闭，只是每条路都先经过记忆。",
            "风",
        ),
        8 => (
            "没有尽头的台阶",
            "台阶向上又向下，最后回到原处。你数到第十五级时，听见孩子说：你看，害怕的时候，路也会假装自己没有出口。",
            "台阶",
        ),
        9 => (
            "墙后的候车室",
            "砖缝后传来许多人的低语。他们都在等某个还没准备好的人回来。车站把等待做成墙，把墙命名为秩序。",
            "低语",
        ),
        10 => (
            "回声终于同步",
            "这一次，你的脚步和回声同时落下。地下通道的主要线索已经整理完毕。",
            "同步",
        ),
        _ => (
            "通道尽头",
            "地下通道的尽头不是门，而是一块干燥的墙。墙上写着：真相不会自动带人离开，它只负责让离开不再是逃跑。",
            "通道深层",
        ),
    }
}

fn clock_tower_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "停止的分针",
            "旧钟卡在 23:59。这个时间就是雾灯站循环的核心。",
            "旧钟楼",
        ),
        1 => (
            "齿轮编号",
            "齿轮内侧刻着许多编号，每个编号后面都有一行极短的备注：未上车、已遗忘、请求再等一分钟。",
            "齿轮",
        ),
        2 => (
            "广播磁带",
            "钟楼抽屉里有一卷广播室磁带。标签是你的字：不要播完整姓名，除非你准备留下。",
            "广播磁带",
        ),
        3 => (
            "广播线总闸",
            "总闸旁写着警告：完整姓名会打开广播室，也会使说话者成为车站声源的一部分。",
            "广播室",
        ),
        4 => (
            "钟腹里的钥匙孔",
            "钟腹里有一个黄铜钥匙孔。你还没插钥匙，就听见齿轮深处传来自己的声音：先想清楚，时间一走，就不能只要答案不要后果。",
            "钥匙孔",
        ),
        5 => (
            "站务员的值夜表",
            "值夜表上每一任站务员都没有离职日期。最后一栏空着，旁边摆着一支还没干的钢笔。",
            "值夜表",
        ),
        6 => (
            "雾灯控制杆",
            "控制杆锈住一半。标签写着：雾灯不是照亮道路，而是让道路承认自己曾经存在。",
            "雾灯",
        ),
        7 => (
            "窗外的月台",
            "从钟楼俯看三号月台，你能清楚看到孩子仍在线后等待。这条线索确认他不是幻觉，也不是车站编出的惩罚。",
            "俯视",
        ),
        8 => (
            "钟声草稿",
            "桌上有一页钟声草稿，写着每种结局对应的声音。独自上车是短音，带孩子返程是长音，留下来则没有钟声。",
            "钟声",
        ),
        9 => (
            "站务员的杯子",
            "杯底有茶渍，茶早凉了。你想到所谓永恒不是宏大的诅咒，有时只是一个人不断喝不到一杯热茶。",
            "茶渍",
        ),
        10 => (
            "午夜后的空白",
            "钟面背后藏着一格 00:00。它被纸条封住，纸条上写着：若打开，请不要再把重来误认为补偿。",
            "零点",
        ),
        _ => (
            "钟楼顶端",
            "你复查旧钟楼。这里的主要线索已经集中在旧钟、广播线、发车哨和站务员外套上。",
            "钟楼深层",
        ),
    }
}

fn platform_step(depth: u8) -> (&'static str, &'static str, &'static str) {
    match depth {
        0 => (
            "白线",
            "月台白线比普通白线更宽。孩子站在线后，不是因为他不想走，而是因为他还在执行你当年的命令。",
            "三号月台",
        ),
        1 => (
            "列车员名册",
            "月台值班柜里有一本列车员名册，最后一页缺失。空栏旁有退票铜筹留下的圆形压痕。",
            "名册",
        ),
        2 => (
            "两组轮痕",
            "铁轨上有两组方向相反的轮痕。雾灯号既来过，也回去过。它不是不能返程，只是很少有人带着完整故事上车。",
            "轮痕",
        ),
        3 => (
            "车门标尺",
            "月台边缘有一道小刻度，正好到孩子肩膀。你想起自己曾抓住他的肩膀，把他推回白线后，说不准动。",
            "刻度",
        ),
        4 => (
            "月台记录",
            "值班记录写着：23:59，成年人上车；00:00，儿童仍在白线后等待。记录员签名空缺，但笔迹是你的。",
            "月台记录",
        ),
        5 => (
            "远端的红灯",
            "远端红灯每亮一次，雾里就多出一节车厢轮廓。它不是在靠近，而是在等待你承认它一直在那里。",
            "红灯",
        ),
        6 => (
            "作业本页角",
            "风把一小片纸吹到你脚边。上面写着：如果大人说马上回来，我应该相信几个马上？",
            "页角",
        ),
        7 => (
            "广播室的窄门",
            "雾灯照过远端时，一扇窄门出现。门牌写着广播室，这是一个可能的终局方向。",
            "窄门",
        ),
        8 => (
            "候车长椅的倒影",
            "铁轨反出候车厅 07 号长椅。你把两个线索连起来：缺失的座位对应月台上被留下的人。",
            "倒影",
        ),
        9 => (
            "雾里的乘客",
            "雾里站着许多模糊的人影。他们没有催你，只是在看。你明白每个离开的人都曾经过这里，每个人都留下过某种未完成。",
            "乘客",
        ),
        10 => (
            "列车将至",
            "铁轨开始轻微震动，说明列车接近。孩子没有后退，只把作业本抱紧，等待你给出真正选择。",
            "将至",
        ),
        _ => (
            "月台尽头",
            "你走到月台尽头，雾向两边让开一寸。那里没有终点，只有一块小小的站牌：明天，本站未必到达，但允许出发。",
            "月台深层",
        ),
    }
}

pub fn synthesis_event(depth: u8) -> StoryEvent {
    let (title, body) = match depth {
        0 => (
            "车票规则整理完成",
            "你把湿票、铜筹和窗口账页并排放好。湿票上有一个被雨泡深的 07A，账页旁边留着另一格空栏。雾灯号不是不让你上车；它在等你说清，当年为什么只有一张票被攥在手里。",
        ),
        1 => (
            "姓名线索整理完成",
            "姓名牌裂成两半，回声先念他的姓，再念你的名。作业本第一页画着两个小人站在车门口，一个被雨水冲淡。你终于想起那晚不是手滑，也不是人群太挤；你曾抓住他的肩膀，让他站到白线后面。",
        ),
        2 => (
            "循环来源整理完成",
            "旧钟、站务日志和候车厅镜片说明：循环不是车站单方面惩罚你，而是你曾请求保留最后一分钟。",
        ),
        3 => (
            "返程座位整理完成",
            "座位图被摊平后，07A 和 07B 紧挨着。一个号码墨色发黑，一个号码干净得像从没等过人。售票员把红笔放在中间，没有替你圈。两个座位少任何一个，车门都会像那晚一样合上。",
        ),
        4 => (
            "广播室线索整理完成",
            "广播磁带、钟楼线路和雾灯玻璃说明：进入广播室可以让后来者听见完整警告，但你可能会永远留下。",
        ),
        5 => (
            "终局路线整理完成",
            "目前可理解的结局方向包括：独自上车、带孩子离开、烧掉规则、进入广播室，或留下守夜。",
        ),
        6 => (
            "老人线索整理完成",
            "老人、报纸和黄铜钥匙说明：善意也可能变成困住人的规则，必须有人承认这一点。",
        ),
        7 => (
            "售票规则整理完成",
            "售票窗口的规则来自许多人失败后的补救。它们能阻止逃避，也可能继续限制后来的人。",
        ),
        8 => (
            "孩子从白线里走出来",
            "孩子把作业本抱在胸前，先看白线，再看你的手。他没有扑过来，也没有说原谅。他只问：如果我走了，还算不算不听话？你听见雨落在站台边，终于没有用保证堵住这个问题。",
        ),
        9 => (
            "站务员线索整理完成",
            "站务员守住最后一分钟，确实帮助过人，但也让许多人依赖循环，不愿真正做决定。",
        ),
        10 => (
            "明天不是站名",
            "站牌上写着“明天”，漆还没干。它不像终点，更像清早厨房里一张没擦干的桌子：一杯热牛奶，一张晾干的票，两个人坐得很远，但都还在。",
        ),
        _ => (
            "最后一处空白整理完成",
            "你把所有线索收进同一个信封。白线的粉末沾在指腹上，湿票贴着掌心发凉。车站没有再给你新的谜语，只把月台灯亮起来，让你看清那个人还站在那里。",
        ),
    };
    StoryEvent::new(title, body).tag("线索整理").tag("回想")
}

pub fn ending_event(ending: Ending, state: &GameState) -> StoryEvent {
    let mut body = match ending {
        Ending::LostPassenger => {
            "你没有决定。雾灯号也不催促，只是打开车门，让站台和候车厅同时变得遥远。下一次醒来时，你仍会攥着那张湿票，只是背面的字迹会更淡一点。"
        }
        Ending::EscapedAlone => {
            "你登上返程车厢，没有回头。07B 仍然崭新，像从未等过任何人。你保住了自己的名字，也让那句“站在白线后等我”继续替你活着。"
        }
        Ending::NewStationKeeper => {
            "你坐到钟楼的木椅上，披上那件没有影子的外套。广播等你开口。你不再把守夜说成高贵，只承认自己还没有能力离开惩罚。"
        }
        Ending::BurnedTimetable => {
            "你用修好的雾灯点燃旧时刻表。车站规则失效，旧钟卡死在一声没有完成的咔哒里。雾灯站成了一间静止的候车厅：没有列车，也没有下一次逃避。"
        }
        Ending::TookChildHome => {
            "你没有命令孩子过来，而是自己跨过白线。那条线在脚下裂开，像一条终于失效的规矩。你把返程票塞进孩子口袋，问他要不要一起走。目的地显示为“明天”。"
        }
        Ending::BecameTheVoice => {
            "你走进广播室，抓住麦克风。门外的站务员撞击铁门，你在恐惧里喊出当年的旧话：退到白线以后。车站安静下来，随后用你的声音播报下一班车。你没有打破规则，而是成为规则的一部分。"
        }
    }
    .to_string();

    let notes = ending_consequence_notes(ending, state);
    if !notes.is_empty() {
        body.push_str("\n\n");
        body.push_str(&notes.join("\n\n"));
    }

    let mut event = StoryEvent::new(ending.title(), body).tag("终局");
    if state.completed_requests.len() >= 3 {
        event.tags.push("委托余波".to_string());
    }
    if state.resolved_case_files.len() >= 3 {
        event.tags.push("档案余波".to_string());
    }
    if !state.completed_case_dialogues.is_empty() {
        event.tags.push("档案回谈".to_string());
    }
    if state.chosen_vows.len() >= 2 {
        event.tags.push("锚点余波".to_string());
    }
    if state.visited_memories.len() >= 3 {
        event.tags.push("记忆余波".to_string());
    }
    if state.completed_patrols.len() >= 3 {
        event.tags.push("巡夜余波".to_string());
    }
    if state.completed_aftertalks.len() >= 3 {
        event.tags.push("回访余波".to_string());
    }
    if state.completed_companion_talks.len() >= 3 {
        event.tags.push("同行余波".to_string());
    }
    if state.focused_lamp_traces.len() >= 3 {
        event.tags.push("照证余波".to_string());
    }
    if state.resolved_anomalies.len() >= 3 {
        event.tags.push("异象余波".to_string());
    }
    if state.prepared_departures.len() >= 1 {
        event.tags.push("路线准备".to_string());
    }
    if state.rehearsed_departures.len() >= 1 {
        event.tags.push("路线试炼".to_string());
    }
    if state.mitigated_route_costs.len() >= 1 {
        event.tags.push("路线代价".to_string());
    }
    if state.completed_route_echoes.len() >= 1 {
        event.tags.push("路线回声".to_string());
    }
    if !state.answered_route_pressures.is_empty() {
        event.tags.push("路线争论".to_string());
    }
    if !state.visited_route_witnesses.is_empty() {
        event.tags.push("路线现场".to_string());
    }
    if !state.completed_route_witness_debriefs.is_empty() {
        event.tags.push("路线复盘".to_string());
    }
    if !state.completed_final_interviews.is_empty() {
        event.tags.push("终局前长谈".to_string());
    }
    if state.heard_station_whispers.len() >= 4 {
        event.tags.push("低语余波".to_string());
    }
    if !state.answered_dialogue_challenges.is_empty() {
        event.tags.push("反问余波".to_string());
    }
    if !state.completed_ending_preludes.is_empty() {
        event.tags.push("终局前场景".to_string());
    }
    if let Some(tag) = final_prelude::ending_response_tag(ending, state) {
        event.tags.push(tag);
    }
    if let Some(tag) = final_debate::ending_tag(ending, state) {
        event.tags.push(tag);
    }
    event.tags.push("结局余波".to_string());
    event
        .tags
        .push(format!("语气：{}", state.dialogue_tone.name()));
    event
}

fn ending_consequence_notes(ending: Ending, state: &GameState) -> Vec<String> {
    let mut notes = Vec::new();
    notes.push(ending_truth_note(state));
    notes.push(ending_relationship_note(state));
    notes.push(ending_price_note(ending, state).to_string());
    if let Some(note) = ending_specific_note(ending, state) {
        notes.push(note.to_string());
    }
    if let Some(note) = optional_progress_note(state) {
        notes.push(note);
    }
    if let Some(note) = vow::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = memory::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = patrol::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = dialogue_lead::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = dialogue_relay::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = dialogue_challenge::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = case_dialogue::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = truth::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = final_prelude::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = final_interview::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = final_prelude::ending_response_consequence(ending, state) {
        notes.push(note);
    }
    if let Some(note) = final_debate::ending_note(state) {
        notes.push(note);
    }
    notes.push(ending_aftermath::ending_note(ending, state));
    if let Some(note) = aftertalk::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = companion::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = lamp_focus::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = anomaly::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = departure::ending_note(ending, state) {
        notes.push(note);
    }
    if let Some(note) = trial::ending_note(ending, state) {
        notes.push(note);
    }
    if let Some(note) = route_cost::ending_note(ending, state) {
        notes.push(note);
    }
    if let Some(note) = route_echo::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = route_pressure::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = route_witness::ending_note(ending, state) {
        notes.push(note);
    }
    if let Some(note) = route_witness_debrief::ending_note(state) {
        notes.push(note);
    }
    if let Some(note) = station_whisper::ending_note(state) {
        notes.push(note);
    }
    notes.push(tone_ending_note(state).to_string());
    notes
}

fn ending_truth_note(state: &GameState) -> String {
    let mut truths = Vec::new();
    if state.has_flag(Flag::ExaminedTicket) || state.has_flag(Flag::ReadDepartureBoard) {
        truths.push("湿票不是警告纸条，而是上一轮的你留给自己的刹车");
    }
    if state.has_flag(Flag::UnderstoodFirstLoop) {
        truths.push("候车厅的雨声来自第一次没有完成的返程");
    }
    if state.has_flag(Flag::RecoveredName) {
        truths.push("姓名不是记忆装饰，而是车站承认一个人能否离开的凭据");
    }
    if state.has_flag(Flag::UnderstoodChildPromise)
        || state.has_flag(Flag::SynthesizedChildTruth)
        || state.has_flag(Flag::ChildJoined)
    {
        truths.push("孩子不是等你抱走的影子，他一直在看你敢不敢撤销当年那句命令");
    }
    if state.ticket == TicketKind::Return || state.has_flag(Flag::SynthesizedRoute) {
        truths.push("返程票买的不是车位，是愿意承担明天代价的签名");
    }
    if state.has_flag(Flag::UnderstoodStationMechanism)
        || state.has_flag(Flag::SynthesizedStationTruth)
    {
        truths.push("雾灯站靠未完成的承诺、未归档的姓名和最后一分钟维持循环");
    }
    if state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape) {
        truths.push("广播不是报站系统，里面还卡着一个人没能咽下去的呼吸");
    }
    if state.has_flag(Flag::AlignedClock) {
        truths.push("旧钟不是坏了，它是在等一个愿意让时间重新流动的人");
    }

    if truths.is_empty() {
        "你几乎没有让车站开口。雾灯站因此能把一切继续写成天气、误点和乘客自己的错；这个结局的空白不是谜底，而是你没有逼近谜底留下的惩罚。".to_string()
    } else {
        format!("你在这一夜确认了：{}。", truths.join("；"))
    }
}

fn ending_relationship_note(state: &GameState) -> String {
    let mut people = Vec::new();
    if state.traveler_depth > 0 || state.has_flag(Flag::TravelerTrusted) {
        people.push(if state.has_flag(Flag::TravelerTrusted) {
            "老人终于不再只做旁观者，他把报纸、黄铜钥匙和 07B 空座一起交还给你的选择"
        } else {
            "老人仍在报纸后面看你，他知道你来过，却还不知道你愿不愿意承认自己也想逃"
        });
    }
    if state.clerk_depth > 0 || state.has_flag(Flag::ClerkMet) || state.ticket == TicketKind::Return
    {
        people.push(if state.ticket == TicketKind::Return {
            "售票员已经替规则开了一次口，但她不会替你签下返程的代价"
        } else {
            "售票员仍把窗口开成一道审讯，她等你证明自己不是又一个把明天赊账的人"
        });
    }
    if state.has_flag(Flag::MetChild) || state.has_flag(Flag::ChildJoined) || state.child_depth > 0
    {
        people.push(if state.has_flag(Flag::ChildJoined) {
            "孩子愿意同行，但他带走的不是听话，而是一次他也能说不的明天"
        } else {
            "孩子还在白线后面，继续把每个保证拆开，看里面是不是又藏着离开"
        });
    }
    if state.keeper_depth > 0 || state.has_flag(Flag::HeardClockTruth) {
        people.push(if state.has_flag(Flag::UnderstoodStationMechanism) {
            "站务员承认守夜既是照路也是控制，他等你判断留下是不是另一种逃跑"
        } else {
            "站务员还守着最后一分钟，他的沉默说明你仍没有碰到车站最硬的骨头"
        });
    }

    if people.is_empty() {
        "四个核心人物仍像灯后的影子。你没有真正和他们交换过答案，所以终点也只能像一扇自动合上的门。"
            .to_string()
    } else {
        format!("这些人没有在结局里消失：{}。", people.join("；"))
    }
}

fn ending_price_note(ending: Ending, state: &GameState) -> &'static str {
    match ending {
        Ending::LostPassenger => {
            "这个结局最轻也最重：你什么都不必承担，于是什么都不会真的改变。"
        }
        Ending::EscapedAlone if state.answered_dialogue_challenges.is_empty() => {
            "你能离开，但离开像一张缺背面的票。没有被反问过的选择最干净，也最像旧循环允许你保留的借口。"
        }
        Ending::EscapedAlone => {
            "你能离开，是因为你已经被问过仍决定独自承担；这不是胜利，只是把空座留给它真正的主人。"
        }
        Ending::NewStationKeeper => {
            "留下的代价不是牺牲，而是每天抵抗牺牲变成姿态。外套会温暖人，也会让人误以为自己有权替别人决定明天。"
        }
        Ending::BurnedTimetable => {
            "烧毁规则以后，雾外的人会自由，也会重新面对没有车站替他们整理好的痛苦。"
        }
        Ending::TookChildHome => {
            "带孩子返程以后，你不能再扮演拯救者。到站以后，他仍可以生气、怀疑、改口，而你要学会不把这些当成失败。"
        }
        Ending::BecameTheVoice => {
            "成为广播以后，你会被拆成声音。你会帮助后来者少走弯路，却也要忍受他们不听、误解、甚至故意上车。"
        }
    }
}

fn ending_specific_note(ending: Ending, state: &GameState) -> Option<&'static str> {
    match ending {
        Ending::LostPassenger if state.completed_requests.len() > 0 => Some(
            "你没有离开，但那些被你办完的小事没有立刻消失。更正栏、账册和信封仍留在原处，像夜里几枚不肯熄灭的钉子，提醒后来者这里曾有人试图把含糊做成具体。",
        ),
        Ending::EscapedAlone if state.resolved_case_files.len() >= 3 => Some(
            "车窗映出你整理过的档案标题。它们没有阻止你独自离开，只让你明白：这不是清白的逃生，而是一次你终于不再假装无知的逃生。",
        ),
        Ending::TookChildHome if state.completed_requests.len() >= 3 => Some(
            "孩子把封好的作业本页角放进口袋。他没有说原谅，也没有说永远，只在车厢灯亮起时问你：到站以后，热牛奶会不会很甜？你说不知道。他说不知道也可以，至少这次不是保证。",
        ),
        Ending::BurnedTimetable if state.completed_requests.len() >= station_request::REQUEST_COUNT => {
            Some(
                "公告栏上的每件小委托都被做完了。旧时刻表燃烧时，火光没有吞掉这些记录，反而把它们照得更清楚。车站失去规则前，先承认这里还有许多不宏大的善后。",
            )
        }
        Ending::BecameTheVoice if state.resolved_case_files.len() >= case_file::CASE_FILE_COUNT => Some(
            "你归档过的六份真相在广播室墙上依次亮起。于是你的声音不再只会重复警告，它也能说出条件、代价和选择之间的区别。后来者听见的不是命令，而是一份尽可能完整的说明。",
        ),
        Ending::NewStationKeeper if state.completed_requests.len() >= 3 => Some(
            "你坐上木椅以前，先把几件旅客委托重新夹进值夜簿。守夜若只剩姿态，很快就会变成债；你至少给自己留下几条笨拙的规矩，提醒灯光必须服务于具体的人。",
        ),
        _ => None,
    }
}

fn optional_progress_note(state: &GameState) -> Option<String> {
    let requests = state.completed_requests.len();
    let cases = state.resolved_case_files.len();
    if requests == 0 && cases == 0 {
        return None;
    }

    if requests >= 3 && cases >= 3 {
        Some(format!(
            "你带到终点的不只是选择，还有 {} 件办完的委托和 {} 份归档的真相。雾灯站不会因此变得仁慈，但它不能再把你的行动说成只是一阵情绪。",
            requests, cases
        ))
    } else if requests > 0 {
        Some(format!(
            "{} 件旅客委托留在身后。它们小得不像结局，却正因为小，才没有被车站轻易改写。",
            requests
        ))
    } else {
        Some(format!(
            "{} 份站内档案被你归位。它们不替你辩护，只让每一道门都少一点装傻的余地。",
            cases
        ))
    }
}

fn tone_ending_note(state: &GameState) -> &'static str {
    match state.dialogue_tone {
        crate::model::DialogueTone::Listening => {
            "你最后仍保留着先听完沉默的习惯。也许这让你走得慢些，却让某些没有说完的话终于不必被推搡着上路。"
        }
        crate::model::DialogueTone::Gentle => {
            "你把问题放轻到最后。轻并不等于退让，它只是承认有些真相若被粗暴拿起，会再次割伤递出真相的人。"
        }
        crate::model::DialogueTone::Direct => {
            "你直接逼近真相到最后。锋利没有让你更无辜，却让那些长期躲在雾后的句子终于失去藏身处。"
        }
    }
}
