use crate::aftertalk;
use crate::anomaly;
use crate::case_dialogue;
use crate::case_file;
use crate::chapter;
use crate::companion;
use crate::content;
use crate::conversation;
use crate::departure;
use crate::dialogue;
use crate::dialogue_challenge;
use crate::dialogue_lead;
use crate::dialogue_question;
use crate::dialogue_relay;
use crate::dialogue_system;
use crate::evidence;
use crate::final_debate;
use crate::final_interview;
use crate::final_prelude;
use crate::inner_voice;
use crate::lamp_focus;
use crate::memory;
use crate::model::{
    ActionId, DialogueBeatKey, DialogueChoiceId, DialogueTone, DialogueTranscriptEntry, Ending,
    Flag, GameMode, GameSession, GameState, Item, Location, StoryEvent, TicketKind,
    LOCATION_INVESTIGATION_STEPS, NPC_THREAD_STEPS,
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

#[derive(Clone, Debug)]
pub struct ActionDefinition {
    pub id: ActionId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

impl ActionDefinition {
    fn new(
        id: ActionId,
        label: impl Into<String>,
        detail: impl Into<String>,
        enabled: bool,
    ) -> Self {
        Self::with_time_cost(id, label, detail, enabled, time_cost_for_action(id))
    }

    fn instant(
        id: ActionId,
        label: impl Into<String>,
        detail: impl Into<String>,
        enabled: bool,
    ) -> Self {
        Self::with_time_cost(id, label, detail, enabled, 0)
    }

    fn with_time_cost(
        id: ActionId,
        label: impl Into<String>,
        detail: impl Into<String>,
        enabled: bool,
        time_cost: u8,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            detail: format_action_detail(time_cost, detail.into()),
            enabled,
        }
    }
}

fn format_action_detail(time_cost: u8, detail: String) -> String {
    if time_cost == 0 {
        format!("不耗时：{detail}")
    } else {
        format!("耗时 {time_cost} 分钟：{detail}")
    }
}

pub fn start(session: &mut GameSession) {
    session.start_new_run(content::opening_event());
}

pub fn restart(session: &mut GameSession) {
    start(session);
}

pub fn apply(session: &mut GameSession, action: ActionId) {
    if session.mode == GameMode::Title {
        start(session);
        return;
    }
    if session.state.ended.is_some() {
        return;
    }

    let events = if session.state.final_train_due() {
        apply_final_choice(&mut session.state, action)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        apply_normal_action(&mut session.state, action)
    };

    if let Some(ending) = session.state.ended {
        session.mode = GameMode::Ending;
        session.endings_seen.insert(ending);
        session.push_event(content::ending_event(ending, &session.state));
    } else {
        for event in events {
            session.push_event(event);
        }
    }
}

pub fn scene_actions(state: &GameState) -> Vec<ActionDefinition> {
    if state.ended.is_some() {
        return Vec::new();
    }

    if state.final_train_due() {
        return final_actions(state);
    }

    if let Some(active) = state.active_dialogue {
        let choices = dialogue_system::available_choices(state);
        let mut actions = Vec::new();
        for choice in choices.iter().filter(|choice| {
            choice.choice != crate::model::DialogueChoiceId::Leave
                && dialogue_choice_visible(state, active, choice)
        }) {
            actions.push(ActionDefinition::new(
                ActionId::ChooseDialogue(choice.choice),
                choice.label.clone(),
                choice.detail.clone(),
                choice.enabled,
            ));
        }
        push_tone_actions(&mut actions, state);
        push_dialogue_topic_actions(&mut actions, state, active.dialogue.location());
        push_dialogue_evidence_actions(&mut actions, state, active.dialogue.location());
        push_dialogue_question_actions(&mut actions, state, active);
        push_dialogue_challenge_response_actions(&mut actions, state, active);
        push_route_pressure_response_actions(&mut actions, state, active);
        push_case_dialogue_actions(&mut actions, state, active);
        push_route_witness_debrief_actions(&mut actions, state, active);
        push_final_interview_actions(&mut actions, state, active);
        push_dialogue_lead_return_actions(&mut actions, state, active);
        push_dialogue_relay_actions(&mut actions, state, active);
        push_dialogue_relay_reflection_actions(&mut actions, state, active);
        push_dialogue_relay_echo_actions(&mut actions, state, active);
        for choice in choices
            .iter()
            .filter(|choice| choice.choice == crate::model::DialogueChoiceId::Leave)
        {
            actions.push(ActionDefinition::new(
                ActionId::ChooseDialogue(choice.choice),
                choice.label.clone(),
                choice.detail.clone(),
                choice.enabled,
            ));
        }
        return enabled_actions(actions);
    }

    let mut actions = Vec::new();
    push_dialogue_entry_actions(&mut actions, state);
    push_location_actions(&mut actions, state);
    push_dialogue_lead_actions(&mut actions, state);
    push_dialogue_relay_anchor_actions(&mut actions, state);
    push_dialogue_relay_anchor_review_actions(&mut actions, state);
    push_truth_scene_actions(&mut actions, state);
    push_anomaly_actions(&mut actions, state);
    push_patrol_actions(&mut actions, state);
    push_aftertalk_actions(&mut actions, state);
    push_companion_actions(&mut actions, state);
    push_lamp_focus_actions(&mut actions, state);
    push_station_whisper_actions(&mut actions, state);
    push_case_file_actions(&mut actions, state);
    push_request_actions(&mut actions, state);
    push_resonance_actions(&mut actions, state);
    push_vow_actions(&mut actions, state);
    push_memory_actions(&mut actions, state);
    push_departure_actions(&mut actions, state);
    push_trial_actions(&mut actions, state);
    push_route_cost_actions(&mut actions, state);
    push_route_echo_actions(&mut actions, state);
    push_route_witness_actions(&mut actions, state);
    push_tone_actions(&mut actions, state);
    push_common_actions(&mut actions, state);
    enabled_actions(actions)
}

fn enabled_actions(actions: Vec<ActionDefinition>) -> Vec<ActionDefinition> {
    actions
        .into_iter()
        .filter(|action| action.enabled)
        .collect()
}

fn dialogue_choice_visible(
    state: &GameState,
    active: crate::model::ActiveDialogue,
    choice: &dialogue_system::DialogueChoiceAction,
) -> bool {
    if !choice.enabled {
        return false;
    }

    let beat = DialogueBeatKey {
        dialogue: active.dialogue,
        node: active.node,
        choice: choice.choice,
    };
    !state.has_completed_dialogue_beat(beat)
}

fn push_dialogue_entry_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for entry in dialogue_system::available_dialogues(state) {
        actions.push(ActionDefinition::new(
            ActionId::BeginDialogue(entry.dialogue),
            entry.label,
            entry.detail,
            entry.enabled,
        ));
    }
}

fn push_dialogue_lead_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for lead in dialogue_lead::available_leads(state) {
        actions.push(ActionDefinition::new(
            ActionId::FollowDialogueLead(lead.lead),
            lead.label,
            lead.detail,
            lead.enabled,
        ));
    }
}

fn push_dialogue_lead_return_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for lead in dialogue_lead::available_returns(state, active) {
        actions.push(ActionDefinition::new(
            ActionId::ReturnDialogueLead(lead.lead),
            lead.label,
            lead.detail,
            lead.enabled,
        ));
    }
}

fn push_dialogue_relay_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for relay in dialogue_relay::available_relays(state, active) {
        actions.push(ActionDefinition::new(
            ActionId::RelayDialogueLead(relay.relay),
            relay.label,
            relay.detail,
            relay.enabled,
        ));
    }
}

fn push_dialogue_relay_reflection_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for reflection in dialogue_relay::available_reflections(state, active) {
        actions.push(ActionDefinition::new(
            ActionId::ReflectDialogueRelay(reflection.relay),
            reflection.label,
            reflection.detail,
            reflection.enabled,
        ));
    }
}

fn push_dialogue_relay_echo_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for echo in dialogue_relay::available_echoes(state, active) {
        actions.push(ActionDefinition::new(
            ActionId::EchoDialogueRelay(echo.relay),
            echo.label,
            echo.detail,
            echo.enabled,
        ));
    }
}

fn push_dialogue_question_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for question in dialogue_question::available_questions(state, active)
        .into_iter()
        .filter(|question| question.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::AskDialogueQuestion(question.question),
            question.label,
            question.detail,
            question.enabled,
        ));
    }
}

fn push_dialogue_challenge_response_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for response in dialogue_challenge::available_responses(state, active)
        .into_iter()
        .filter(|response| response.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::AnswerDialogueChallenge(response.challenge, response.response),
            response.label,
            response.detail,
            response.enabled,
        ));
    }
}

fn push_route_pressure_response_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for response in route_pressure::available_responses(state, active)
        .into_iter()
        .filter(|response| response.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::AnswerRoutePressure(response.pressure, response.response),
            response.label,
            response.detail,
            response.enabled,
        ));
    }
}

fn push_case_dialogue_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for dialogue in case_dialogue::available_case_dialogues(state, active)
        .into_iter()
        .filter(|dialogue| dialogue.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::DiscussCaseDialogue(dialogue.dialogue),
            dialogue.label,
            dialogue.detail,
            dialogue.enabled,
        ));
    }
}

fn push_route_witness_debrief_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for debrief in route_witness_debrief::available_debriefs(state, active)
        .into_iter()
        .filter(|debrief| debrief.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::DebriefRouteWitness(debrief.debrief),
            debrief.label,
            debrief.detail,
            debrief.enabled,
        ));
    }
}

fn push_final_interview_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    active: crate::model::ActiveDialogue,
) {
    for interview in final_interview::available_interviews(state, active)
        .into_iter()
        .filter(|interview| interview.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::HoldFinalInterview(interview.interview),
            interview.label,
            interview.detail,
            interview.enabled,
        ));
    }
}

fn push_dialogue_relay_anchor_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for anchor in dialogue_relay::available_anchors(state) {
        actions.push(ActionDefinition::new(
            ActionId::AnchorDialogueRelay(anchor.relay),
            anchor.label,
            anchor.detail,
            anchor.enabled,
        ));
    }
}

fn push_dialogue_relay_anchor_review_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
) {
    for review in dialogue_relay::available_anchor_reviews(state) {
        actions.push(ActionDefinition::new(
            ActionId::ReviewDialogueAnchor(review.relay),
            review.label,
            review.detail,
            review.enabled,
        ));
    }
}

fn push_truth_scene_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for truth in truth::available_truth_scenes(state) {
        actions.push(ActionDefinition::new(
            ActionId::RevealTruth(truth.truth),
            truth.label,
            truth.detail,
            truth.enabled,
        ));
    }
}

fn push_dialogue_evidence_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    location: Location,
) {
    for evidence in evidence::available_presentations(state, location)
        .into_iter()
        .filter(|evidence| evidence.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::PresentEvidence(evidence.evidence),
            format!("证据追问：{}", evidence.label),
            evidence.detail,
            evidence.enabled,
        ));
    }
}

fn push_dialogue_topic_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    location: Location,
) {
    for topic in conversation::available_topics(state, location)
        .into_iter()
        .filter(|topic| topic.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::Discuss(topic.topic),
            format!("话题追问：{}", topic.label),
            topic.detail,
            topic.enabled,
        ));
    }
}

fn push_location_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    let depth = state.investigation_depth(state.location);
    if depth < LOCATION_INVESTIGATION_STEPS {
        actions.push(ActionDefinition::new(
            ActionId::InvestigateLocation,
            content::investigation_label(state.location, depth),
            content::investigation_detail(state.location, depth),
            true,
        ));
    }

    match state.location {
        Location::WaitingHall => {
            actions.push(ActionDefinition::new(
                ActionId::ExamineTicket,
                "检查湿透的车票",
                "看清“别上车”背后第一层笔迹。",
                !state.has_flag(Flag::ExaminedTicket),
            ));
            actions.push(ActionDefinition::new(
                ActionId::StudyTicket,
                "对照车票水痕",
                "需要镜片或退票铜筹，才能读出车票更深的规则。",
                state.has_flag(Flag::ExaminedTicket)
                    && (state.has_item(Item::MirrorShard) || state.has_item(Item::CoinToken))
                    && !state.has_flag(Flag::UnderstoodChildPromise),
            ));
            actions.push(ActionDefinition::new(
                ActionId::ReadDepartureBoard,
                "读电子时刻表",
                "它会把目的地写成姓名，也会把空白写成危险。",
                !state.has_flag(Flag::ReadDepartureBoard),
            ));
            actions.push(ActionDefinition::new(
                ActionId::ShowTicketToTraveler,
                "把湿票递给老人，不先解释",
                "让老人直接看车票，可能提高他的信任并补全车票警告。",
                state.has_flag(Flag::ExaminedTicket)
                    && state.traveler_depth > 0
                    && !state.has_flag(Flag::TravelerTrusted),
            ));
            push_topic_actions(actions, state, Location::WaitingHall);
            push_evidence_actions(actions, state, Location::WaitingHall);
        }
        Location::TicketOffice => {
            actions.push(ActionDefinition::new(
                ActionId::ShowLogToClerk,
                "把站务日志从玻璃下推过去",
                "日志会让售票员停止扮演普通窗口。",
                state.has_item(Item::StationLog) && !state.has_flag(Flag::ReadStationLog),
            ));
            actions.push(ActionDefinition::new(
                ActionId::RewriteTicket,
                "请求改签返程票",
                "需要站务日志、退票铜筹和售票员的信任。",
                state.has_item(Item::StationLog)
                    && state.has_item(Item::CoinToken)
                    && state.clerk_trust >= 3
                    && state.ticket != TicketKind::Return,
            ));
            push_topic_actions(actions, state, Location::TicketOffice);
            push_evidence_actions(actions, state, Location::TicketOffice);
        }
        Location::LostAndFound => {
            actions.push(ActionDefinition::new(
                ActionId::SearchLostFound,
                "翻找标着“没来得及”的箱子",
                "寻找和雨夜有关的物件，尤其是雾灯玻璃和裂开的姓名牌。",
                !state.has_flag(Flag::SearchedLostFound),
            ));
            actions.push(ActionDefinition::new(
                ActionId::OpenCabinet,
                "打开最里面的铁柜",
                "需要黄铜小钥匙；铁柜里存着被反复封存的站务档案。",
                state.has_item(Item::BrassKey) && !state.has_flag(Flag::OpenedCabinet),
            ));
            push_topic_actions(actions, state, Location::LostAndFound);
            push_evidence_actions(actions, state, Location::LostAndFound);
        }
        Location::Underpass => {
            actions.push(ActionDefinition::new(
                ActionId::ListenUnderpass,
                "聆听慢半拍的回声",
                "回声会逐步拼出姓名、承诺和那场雨里缺掉的一段话。",
                !state.has_flag(Flag::RecoveredName) || !state.has_flag(Flag::UnderstoodFirstLoop),
            ));
            actions.push(ActionDefinition::new(
                ActionId::RepairFogLamp,
                "修复雾灯",
                "需要雾灯玻璃。修好后，雾会露出月台远端和广播线路。",
                state.has_item(Item::LanternGlass) && !state.has_flag(Flag::RepairedFogLamp),
            ));
            push_topic_actions(actions, state, Location::Underpass);
            push_evidence_actions(actions, state, Location::Underpass);
        }
        Location::ClockTower => {
            actions.push(ActionDefinition::new(
                ActionId::ShowTimetableToKeeper,
                "把烧焦的时刻表放到钟面下",
                "不是请他解释，而是让他承认这张纸。",
                state.has_item(Item::OldTimetable)
                    && !state.has_flag(Flag::UnderstoodStationMechanism),
            ));
            actions.push(ActionDefinition::new(
                ActionId::AlignClock,
                "校准旧钟",
                "需要听懂旧钟代价，并带着黄铜小钥匙。",
                state.has_flag(Flag::HeardClockTruth)
                    && state.has_item(Item::BrassKey)
                    && !state.has_flag(Flag::AlignedClock),
            ));
            push_topic_actions(actions, state, Location::ClockTower);
            push_evidence_actions(actions, state, Location::ClockTower);
        }
        Location::Platform => {
            actions.push(ActionDefinition::new(
                ActionId::InspectRails,
                "检查轨道尽头",
                "轨道上有两组方向相反的轮痕。",
                !state.has_flag(Flag::InspectedRails),
            ));
            actions.push(ActionDefinition::new(
                ActionId::ReturnNameTag,
                "把姓名牌交给孩子",
                "需要找回两个名字，并理解白线后那句旧命令。",
                state.has_flag(Flag::RecoveredName)
                    && state.has_flag(Flag::UnderstoodChildPromise)
                    && state.has_item(Item::NameTag)
                    && !state.has_flag(Flag::ReturnedNameTag),
            ));
            actions.push(ActionDefinition::new(
                ActionId::ShowNameTagToChild,
                "先把姓名牌摊在自己掌心",
                "你可以不急着递给他，先看看他愿不愿意看。",
                state.has_item(Item::NameTag)
                    && state.has_flag(Flag::MetChild)
                    && !state.has_flag(Flag::UnderstoodChildPromise),
            ));
            actions.push(ActionDefinition::new(
                ActionId::BlowWhistle,
                "吹响银色发车哨",
                "需要发车哨，并让旧钟重新承认时间。",
                state.has_item(Item::SignalWhistle) && state.has_flag(Flag::AlignedClock),
            ));
            push_topic_actions(actions, state, Location::Platform);
            push_evidence_actions(actions, state, Location::Platform);
        }
    }
}

fn push_anomaly_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for anomaly in anomaly::available_anomalies(state) {
        actions.push(ActionDefinition::new(
            ActionId::HandleAnomaly(anomaly.anomaly, anomaly.response),
            anomaly.label,
            anomaly.detail,
            anomaly.enabled,
        ));
    }
}

fn push_topic_actions(actions: &mut Vec<ActionDefinition>, state: &GameState, location: Location) {
    for topic in conversation::available_topics(state, location)
        .into_iter()
        .filter(|topic| topic.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::Discuss(topic.topic),
            topic.label,
            topic.detail,
            topic.enabled,
        ));
    }
}

fn push_evidence_actions(
    actions: &mut Vec<ActionDefinition>,
    state: &GameState,
    location: Location,
) {
    for evidence in evidence::available_presentations(state, location)
        .into_iter()
        .filter(|evidence| evidence.enabled)
    {
        actions.push(ActionDefinition::new(
            ActionId::PresentEvidence(evidence.evidence),
            evidence.label,
            evidence.detail,
            evidence.enabled,
        ));
    }
}

fn push_case_file_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for case_file in case_file::available_case_files(state) {
        actions.push(ActionDefinition::new(
            ActionId::ResolveCaseFile(case_file.case_file),
            case_file.label,
            case_file.detail,
            case_file.enabled,
        ));
    }
}

fn push_request_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for request in station_request::available_requests(state) {
        actions.push(ActionDefinition::new(
            ActionId::CompleteRequest(request.request),
            request.label,
            request.detail,
            request.enabled,
        ));
    }
}

fn push_resonance_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for resonance in resonance::available_resonances(state) {
        actions.push(ActionDefinition::new(
            ActionId::ResolveResonance(resonance.resonance),
            resonance.label,
            resonance.detail,
            resonance.enabled,
        ));
    }
}

fn push_vow_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for vow in vow::available_vows(state) {
        actions.push(ActionDefinition::new(
            ActionId::MakeVow(vow.vow),
            vow.label,
            vow.detail,
            vow.enabled,
        ));
    }
}

fn push_memory_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for memory in memory::available_memories(state) {
        actions.push(ActionDefinition::new(
            ActionId::EnterMemory(memory.memory),
            memory.label,
            memory.detail,
            memory.enabled,
        ));
    }
}

fn push_patrol_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for patrol in patrol::available_patrols(state) {
        actions.push(ActionDefinition::new(
            ActionId::TakePatrol(patrol.patrol),
            patrol.label,
            patrol.detail,
            patrol.enabled,
        ));
    }
}

fn push_aftertalk_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for aftertalk in aftertalk::available_aftertalks(state) {
        actions.push(ActionDefinition::new(
            ActionId::FollowUpDialogue(aftertalk.aftertalk),
            aftertalk.label,
            aftertalk.detail,
            aftertalk.enabled,
        ));
    }
}

fn push_companion_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for talk in companion::available_companion_talks(state) {
        actions.push(ActionDefinition::new(
            ActionId::CompanionDialogue(talk.talk),
            talk.label,
            talk.detail,
            talk.enabled,
        ));
    }
}

fn push_lamp_focus_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for focus in lamp_focus::available_focuses(state) {
        actions.push(ActionDefinition::new(
            ActionId::FocusFogLamp(focus.focus),
            focus.label,
            focus.detail,
            focus.enabled,
        ));
    }
}

fn push_station_whisper_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for whisper in station_whisper::available_whispers(state) {
        actions.push(ActionDefinition::new(
            ActionId::ListenStationWhisper(whisper.whisper),
            whisper.label,
            whisper.detail,
            whisper.enabled,
        ));
    }
}

fn push_departure_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for departure in departure::available_departures(state) {
        actions.push(ActionDefinition::new(
            ActionId::PrepareDeparture(departure.departure),
            departure.label,
            departure.detail,
            departure.enabled,
        ));
    }
}

fn push_trial_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for trial in trial::available_trials(state) {
        actions.push(ActionDefinition::new(
            ActionId::RehearseRoute(trial.departure),
            trial.label,
            trial.detail,
            trial.enabled,
        ));
    }
}

fn push_route_cost_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for cost in route_cost::available_costs(state) {
        actions.push(ActionDefinition::new(
            ActionId::MitigateRouteCost(cost.cost),
            cost.label,
            cost.detail,
            cost.enabled,
        ));
    }
}

fn push_route_echo_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for echo in route_echo::available_echoes(state) {
        actions.push(ActionDefinition::new(
            ActionId::DiscussRouteEcho(echo.cost),
            echo.label,
            echo.detail,
            echo.enabled,
        ));
    }
}

fn push_route_witness_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for witness in route_witness::available_witnesses(state) {
        actions.push(ActionDefinition::new(
            ActionId::VisitRouteWitness(witness.witness),
            witness.label,
            witness.detail,
            witness.enabled,
        ));
    }
}

fn push_tone_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    for tone in [
        DialogueTone::Listening,
        DialogueTone::Gentle,
        DialogueTone::Direct,
    ] {
        if tone != state.dialogue_tone {
            actions.push(ActionDefinition::new(
                ActionId::SetDialogueTone(tone),
                tone_action_label(tone),
                tone_action_detail(tone),
                true,
            ));
        }
    }
}

fn push_common_actions(actions: &mut Vec<ActionDefinition>, state: &GameState) {
    if state.synthesis_depth < NPC_THREAD_STEPS {
        actions.push(ActionDefinition::new(
            ActionId::SynthesizeClues,
            synthesis_label(state),
            synthesis_detail(state),
            can_synthesize_next(state),
        ));
    }

    actions.push(ActionDefinition::new(
        ActionId::Wait,
        "等待",
        "什么也不做同样会让列车靠近。",
        true,
    ));
}

fn synthesis_detail(state: &GameState) -> &'static str {
    match state.synthesis_depth {
        0 => "需要车票、时刻表或退票铜筹中的任意两件线索。",
        1 => "需要姓名牌、回声、作业本或镜片中的任意两件线索。",
        2 => "需要站务日志、旧钟真相和候车厅深层调查。",
        3 => "需要返程票规则、退票铜筹和孩子的信任。",
        4 => "需要广播磁带、旧钟线路和修复后的雾灯。",
        _ => "需要前五次整理，以及足够多的地点调查。",
    }
}

fn tone_action_label(tone: DialogueTone) -> &'static str {
    match tone {
        DialogueTone::Listening => "先听完，再追问",
        DialogueTone::Gentle => "把接下来的问题放轻",
        DialogueTone::Direct => "直接逼近真相",
    }
}

fn tone_action_detail(tone: DialogueTone) -> &'static str {
    match tone {
        DialogueTone::Listening => "倾听会让脆弱的证词更容易留下来。",
        DialogueTone::Gentle => "放轻语气更容易保护孩子和老人愿意说出的部分。",
        DialogueTone::Direct => "直接追问更容易迫使售票窗口和旧钟楼说出规则。",
    }
}

fn synthesis_label(state: &GameState) -> &'static str {
    match state.synthesis_depth {
        0 => "把湿票、时刻表和铜筹摊在长椅上",
        1 => "把姓名牌、作业本和回声对在一起",
        2 => "用站务日志复原午夜是怎么停住的",
        3 => "重新理解返程票上的两个座位",
        4 => "沿着广播线想象那扇窄门",
        5 => "给最后一分钟取一个不逃避的名字",
        6 => "回想老人为什么一直读同一页报纸",
        7 => "想清楚售票窗口到底在替谁办事",
        8 => "承认孩子不是你故事里的道具",
        9 => "把站务员、旧钟和最后一分钟放在一起",
        10 => "试着说出明天到底是什么",
        _ => "把所有东西放回同一个夜晚",
    }
}

fn can_synthesize_next(state: &GameState) -> bool {
    match state.synthesis_depth {
        0 => {
            state.has_flag(Flag::ExaminedTicket) as u8
                + state.has_flag(Flag::ReadDepartureBoard) as u8
                + state.has_item(Item::CoinToken) as u8
                >= 2
        }
        1 => {
            state.has_item(Item::NameTag) as u8
                + state.has_flag(Flag::RecoveredName) as u8
                + state.has_item(Item::ChildHomework) as u8
                + state.has_item(Item::MirrorShard) as u8
                >= 2
        }
        2 => {
            state.has_item(Item::StationLog)
                && state.has_flag(Flag::HeardClockTruth)
                && state.has_flag(Flag::UnderstoodFirstLoop)
        }
        3 => state.has_flag(Flag::SynthesizedRoute) && state.child_trust >= 3,
        4 => {
            state.has_item(Item::BroadcastTape)
                && state.has_flag(Flag::AlignedClock)
                && state.has_flag(Flag::RepairedFogLamp)
        }
        _ => state.synthesis_depth >= 5 && state.location_depths.iter().copied().sum::<u8>() >= 30,
    }
}

fn final_actions(state: &GameState) -> Vec<ActionDefinition> {
    let mut actions = Vec::new();
    for prelude in final_prelude::available_preludes(state) {
        actions.push(ActionDefinition::new(
            ActionId::EnterEndingPrelude(prelude.prelude),
            prelude.label,
            prelude.detail,
            prelude.enabled,
        ));
    }
    for response in final_prelude::available_responses(state) {
        actions.push(ActionDefinition::new(
            ActionId::AnswerEndingPrelude(response.prelude, response.response),
            response.label,
            response.detail,
            response.enabled,
        ));
    }
    for response in final_debate::available_responses(state) {
        actions.push(ActionDefinition::new(
            ActionId::AnswerFinalDebate(response.debate, response.response),
            response.label,
            response.detail,
            response.enabled,
        ));
    }

    actions.extend([
        ActionDefinition::new(
            ActionId::BoardAlone,
            "独自上车",
            final_debate::final_choice_detail(
                state,
                Ending::EscapedAlone,
                "保住自己的名字，离开这里。",
                "需要姓名，以及返程票或轨道证据。",
            ),
            final_debate::final_choice_ready(state, Ending::EscapedAlone),
        ),
        ActionDefinition::new(
            ActionId::BoardWithChild,
            "带孩子返程",
            final_debate::final_choice_detail(
                state,
                Ending::TookChildHome,
                "孩子已经亲口选择跨过白线，带他去一个不是保证书的明天。",
                "需要孩子愿意相信你，也需要返程的证据。",
            ),
            final_debate::final_choice_ready(state, Ending::TookChildHome),
        ),
        ActionDefinition::new(
            ActionId::BurnTimetable,
            "烧掉时刻表",
            final_debate::final_choice_detail(
                state,
                Ending::BurnedTimetable,
                "旧时刻表已经被火照过，烧掉它，让规则失去最后的纸面。",
                "需要旧时刻表、修复后的雾灯，以及对车站机制的理解。",
            ),
            final_debate::final_choice_ready(state, Ending::BurnedTimetable),
        ),
        ActionDefinition::new(
            ActionId::BroadcastName,
            "走进广播室",
            final_debate::final_choice_detail(
                state,
                Ending::BecameTheVoice,
                "窄门已经开过；把自己的名字交给广播，也承担被误听的代价。",
                "需要找回姓名、读过站务日志，校准旧钟，并找到广播磁带。",
            ),
            final_debate::final_choice_ready(state, Ending::BecameTheVoice),
        ),
        ActionDefinition::new(
            ActionId::TakeKeeperSeat,
            "接过站务员的外套",
            final_debate::final_choice_detail(
                state,
                Ending::NewStationKeeper,
                "你已经看清外套没有影子。留下来，为下一位旅客守夜。",
                "需要听懂旧钟真相，并理解雾灯站的机制。",
            ),
            final_debate::final_choice_ready(state, Ending::NewStationKeeper),
        ),
        ActionDefinition::instant(ActionId::Wait, "什么都不选", "让雾灯号替你决定。", true),
    ]);
    actions
}

fn apply_normal_action(state: &mut GameState, action: ActionId) -> Vec<StoryEvent> {
    let previous_segment = state.current_segment();
    let active_before = state.active_dialogue;
    let time_cost = time_cost_for_action(action);
    let event = match action {
        ActionId::BeginDialogue(dialogue_id) => dialogue_system::begin(state, dialogue_id),
        ActionId::ChooseDialogue(choice_id) => dialogue_system::choose(state, choice_id),
        ActionId::AskDialogueQuestion(question_id) => dialogue_question::ask(state, question_id),
        ActionId::AnswerDialogueChallenge(challenge_id, response_id) => {
            dialogue_challenge::answer(state, challenge_id, response_id)
        }
        ActionId::Move(location) => move_to(state, location),
        ActionId::Discuss(topic) => conversation::discuss(state, topic),
        ActionId::PresentEvidence(evidence_id) => evidence::present(state, evidence_id),
        ActionId::ResolveCaseFile(case_file_id) => case_file::resolve(state, case_file_id),
        ActionId::DiscussCaseDialogue(dialogue_id) => case_dialogue::discuss(state, dialogue_id),
        ActionId::CompleteRequest(request_id) => station_request::complete(state, request_id),
        ActionId::ResolveResonance(resonance_id) => resonance::resolve(state, resonance_id),
        ActionId::MakeVow(vow_id) => vow::make(state, vow_id),
        ActionId::EnterMemory(memory_id) => memory::enter(state, memory_id),
        ActionId::TakePatrol(patrol_id) => patrol::take(state, patrol_id),
        ActionId::FollowUpDialogue(aftertalk_id) => aftertalk::follow_up(state, aftertalk_id),
        ActionId::FollowDialogueLead(lead_id) => dialogue_lead::follow(state, lead_id),
        ActionId::ReturnDialogueLead(lead_id) => dialogue_lead::return_to_dialogue(state, lead_id),
        ActionId::RelayDialogueLead(relay_id) => dialogue_relay::share(state, relay_id),
        ActionId::ReflectDialogueRelay(relay_id) => dialogue_relay::reflect(state, relay_id),
        ActionId::EchoDialogueRelay(relay_id) => dialogue_relay::echo(state, relay_id),
        ActionId::AnchorDialogueRelay(relay_id) => dialogue_relay::anchor(state, relay_id),
        ActionId::ReviewDialogueAnchor(relay_id) => dialogue_relay::review_anchor(state, relay_id),
        ActionId::CompanionDialogue(talk_id) => companion::talk(state, talk_id),
        ActionId::FocusFogLamp(focus_id) => lamp_focus::focus(state, focus_id),
        ActionId::ListenStationWhisper(whisper_id) => station_whisper::listen(state, whisper_id),
        ActionId::HandleAnomaly(anomaly_id, response) => {
            anomaly::handle(state, anomaly_id, response)
        }
        ActionId::PrepareDeparture(departure_id) => departure::prepare(state, departure_id),
        ActionId::RehearseRoute(departure_id) => trial::rehearse(state, departure_id),
        ActionId::MitigateRouteCost(cost_id) => route_cost::mitigate(state, cost_id),
        ActionId::DiscussRouteEcho(cost_id) => route_echo::discuss(state, cost_id),
        ActionId::VisitRouteWitness(witness_id) => route_witness::visit(state, witness_id),
        ActionId::DebriefRouteWitness(debrief_id) => {
            route_witness_debrief::debrief(state, debrief_id)
        }
        ActionId::HoldFinalInterview(interview_id) => final_interview::hold(state, interview_id),
        ActionId::AnswerRoutePressure(pressure_id, response_id) => {
            route_pressure::answer(state, pressure_id, response_id)
        }
        ActionId::RevealTruth(truth_id) => truth::reveal(state, truth_id),
        ActionId::SetDialogueTone(tone) => set_dialogue_tone(state, tone),
        ActionId::InvestigateLocation => investigate_current_location(state),
        ActionId::ExamineTicket => examine_ticket(state),
        ActionId::StudyTicket => study_ticket(state),
        ActionId::ShowTicketToTraveler => show_ticket_to_traveler(state),
        ActionId::ReadDepartureBoard => read_departure_board(state),
        ActionId::TalkTraveler => talk_traveler(state),
        ActionId::TalkClerk => talk_clerk(state),
        ActionId::ShowLogToClerk => show_log_to_clerk(state),
        ActionId::RewriteTicket => rewrite_ticket(state),
        ActionId::SearchLostFound => search_lost_found(state),
        ActionId::OpenCabinet => open_cabinet(state),
        ActionId::ListenUnderpass => listen_underpass(state),
        ActionId::RepairFogLamp => repair_fog_lamp(state),
        ActionId::MeetChild => meet_child(state),
        ActionId::ShowNameTagToChild => show_name_tag_to_child(state),
        ActionId::ReturnNameTag => return_name_tag(state),
        ActionId::TalkStationKeeper => talk_station_keeper(state),
        ActionId::ShowTimetableToKeeper => show_timetable_to_keeper(state),
        ActionId::AlignClock => align_clock(state),
        ActionId::InspectRails => inspect_rails(state),
        ActionId::BlowWhistle => blow_whistle(state),
        ActionId::SynthesizeClues => synthesize_clues(state),
        ActionId::Wait => StoryEvent::new(
            "等待",
            "你坐了一会儿。广播报站、灯光闪烁、雾在门外加厚。什么都没有发生，除了时间真的少了一点。",
        ),
        ActionId::BoardAlone
        | ActionId::BoardWithChild
        | ActionId::BurnTimetable
        | ActionId::BroadcastName
        | ActionId::EnterEndingPrelude(_)
        | ActionId::AnswerEndingPrelude(_, _)
        | ActionId::AnswerFinalDebate(_, _)
        | ActionId::TakeKeeperSeat => return Vec::new(),
    };

    if let Some(active) = active_before {
        match action {
            ActionId::PresentEvidence(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("证据追问：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::SetDialogueTone(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("语气调整：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::Discuss(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("话题追问：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::AskDialogueQuestion(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("自由询问：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::AnswerDialogueChallenge(_, _) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("立场回应：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::AnswerRoutePressure(_, _) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("路线争论：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::DiscussCaseDialogue(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("档案回谈：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::DebriefRouteWitness(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("路线复盘：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::HoldFinalInterview(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("终局前长谈：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::ReturnDialogueLead(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("带回线索：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::RelayDialogueLead(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("线索转述：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::ReflectDialogueRelay(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("转述余波：{}", event.title),
                    event.body.clone(),
                ));
            }
            ActionId::EchoDialogueRelay(_) => {
                state.record_dialogue_line(DialogueTranscriptEntry::new(
                    active.dialogue,
                    active.node,
                    None,
                    format!("转述回声：{}", event.title),
                    event.body.clone(),
                ));
            }
            _ => {}
        }
    }

    if time_cost > 0 {
        state.advance_time(time_cost);
    }
    events_after_action(state, previous_segment, event)
}

fn time_cost_for_action(action: ActionId) -> u8 {
    match action {
        ActionId::BeginDialogue(_)
        | ActionId::ChooseDialogue(DialogueChoiceId::BackToRoot)
        | ActionId::ChooseDialogue(DialogueChoiceId::Leave)
        | ActionId::SetDialogueTone(_) => 0,
        ActionId::Move(_) => 0,
        ActionId::ChooseDialogue(
            DialogueChoiceId::DeepenTopic
            | DialogueChoiceId::ChallengeTopic
            | DialogueChoiceId::PromiseTopic,
        ) => 4,
        ActionId::ChooseDialogue(_) => 3,
        ActionId::AskDialogueQuestion(_) => 3,
        ActionId::AnswerDialogueChallenge(_, _) => 4,
        ActionId::AnswerRoutePressure(_, _) => 5,
        ActionId::DiscussCaseDialogue(_) => 5,
        ActionId::DebriefRouteWitness(_) => 5,
        ActionId::HoldFinalInterview(_) => 8,
        ActionId::Discuss(_) => 4,
        ActionId::PresentEvidence(_) => 5,
        ActionId::ReturnDialogueLead(_)
        | ActionId::RelayDialogueLead(_)
        | ActionId::ReflectDialogueRelay(_)
        | ActionId::EchoDialogueRelay(_) => 4,
        ActionId::AnchorDialogueRelay(_) | ActionId::ReviewDialogueAnchor(_) => 5,
        ActionId::FollowDialogueLead(_) => 8,
        ActionId::InvestigateLocation => 8,
        ActionId::EnterMemory(_) | ActionId::TakePatrol(_) | ActionId::RehearseRoute(_) => 12,
        ActionId::HandleAnomaly(_, _) => 10,
        ActionId::Wait => 10,
        ActionId::ResolveCaseFile(_)
        | ActionId::CompleteRequest(_)
        | ActionId::ResolveResonance(_)
        | ActionId::MakeVow(_)
        | ActionId::FollowUpDialogue(_)
        | ActionId::CompanionDialogue(_)
        | ActionId::FocusFogLamp(_)
        | ActionId::ListenStationWhisper(_)
        | ActionId::PrepareDeparture(_)
        | ActionId::MitigateRouteCost(_)
        | ActionId::DiscussRouteEcho(_) => 6,
        ActionId::VisitRouteWitness(_) => 8,
        ActionId::RevealTruth(_) => 10,
        ActionId::ExamineTicket
        | ActionId::StudyTicket
        | ActionId::ShowTicketToTraveler
        | ActionId::ReadDepartureBoard
        | ActionId::TalkTraveler
        | ActionId::TalkClerk
        | ActionId::ShowLogToClerk
        | ActionId::RewriteTicket
        | ActionId::SearchLostFound
        | ActionId::OpenCabinet
        | ActionId::ListenUnderpass
        | ActionId::RepairFogLamp
        | ActionId::MeetChild
        | ActionId::ShowNameTagToChild
        | ActionId::ReturnNameTag
        | ActionId::TalkStationKeeper
        | ActionId::ShowTimetableToKeeper
        | ActionId::AlignClock
        | ActionId::InspectRails
        | ActionId::BlowWhistle
        | ActionId::SynthesizeClues => 6,
        ActionId::BoardAlone
        | ActionId::BoardWithChild
        | ActionId::BurnTimetable
        | ActionId::BroadcastName
        | ActionId::EnterEndingPrelude(_)
        | ActionId::AnswerEndingPrelude(_, _)
        | ActionId::AnswerFinalDebate(_, _)
        | ActionId::TakeKeeperSeat => 0,
    }
}

fn set_dialogue_tone(state: &mut GameState, tone: DialogueTone) -> StoryEvent {
    if state.dialogue_tone == tone {
        return StoryEvent::new(
            "说话方式没有改变",
            format!("你仍决定{}。车站把这点记得很清楚。", tone.name()),
        )
        .tag("说话方式");
    }

    state.dialogue_tone = tone;
    let body = match tone {
        DialogueTone::Listening => {
            "你决定先听完对方的话，再继续追问。这样更容易让老人、孩子和站务员说出完整信息。"
        }
        DialogueTone::Gentle => "你决定把问题说得轻一点。这样更容易保护孩子和老人愿意继续谈下去。",
        DialogueTone::Direct => "你决定直接追问重点。这样更容易逼出售票员和站务员隐瞒的规则。",
    };
    StoryEvent::new(format!("说话方式：{}", tone.name()), body).tag("说话方式")
}

fn move_to(state: &mut GameState, location: Location) -> StoryEvent {
    if location == state.location {
        return StoryEvent::new("仍在原地", content::location_description(state));
    }
    state.location = location;
    StoryEvent::new(
        format!("前往{}", location.title()),
        content::location_description(state),
    )
}

fn investigate_current_location(state: &mut GameState) -> StoryEvent {
    let location = state.location;
    let depth = state.advance_investigation(location);
    let mut event = content::investigation_event(location, depth);
    apply_investigation_rewards(state, location, depth, &mut event);
    event
}

fn apply_investigation_rewards(
    state: &mut GameState,
    location: Location,
    depth: u8,
    event: &mut StoryEvent,
) {
    match (location, depth) {
        (Location::WaitingHall, 0) => {
            grant_item(state, event, Item::StationMap);
            remember_tag(state, event, Flag::FoundStationMap, "获得站内图");
        }
        (Location::WaitingHall, 1) => {
            grant_item(state, event, Item::MirrorShard);
            remember_tag(state, event, Flag::FoundMirrorShard, "候车厅镜片");
        }
        (Location::WaitingHall, 3) => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "理解：第一次循环")
        }
        (Location::TicketOffice, 1) => {
            grant_item(state, event, Item::CoinToken);
            remember_tag(state, event, Flag::FoundCoinToken, "退票铜筹");
        }
        (Location::TicketOffice, 4) => remember_tag(state, event, Flag::FoundCoinToken, "退票规则"),
        (Location::LostAndFound, 2) => grant_item(state, event, Item::LanternGlass),
        (Location::LostAndFound, 3) => grant_item(state, event, Item::NameTag),
        (Location::Underpass, 3) => remember_tag(
            state,
            event,
            Flag::UnderstoodStationMechanism,
            "理解：车站机制",
        ),
        (Location::ClockTower, 2) => grant_item(state, event, Item::BroadcastTape),
        (Location::ClockTower, 3) => {
            remember_tag(state, event, Flag::HeardBroadcastTape, "广播室线索")
        }
        (Location::Platform, 1) => {
            grant_item(state, event, Item::ConductorRoster);
            remember_tag(state, event, Flag::FoundRoster, "列车员名册");
        }
        (Location::Platform, 4) => remember_tag(state, event, Flag::ReadPlatformLedger, "月台记录"),
        _ => {}
    }
}

fn examine_ticket(state: &mut GameState) -> StoryEvent {
    state.remember(Flag::ExaminedTicket);
    StoryEvent::new(
        "车票背面",
        "你把车票举到灯下。票面有一角编号露出来，像座位号，又像被雨水咬掉了一半。背面只有三个清楚的字：别上车。水痕下面还有更深的划痕，但现在看不出来。",
    )
    .tag("记忆碎片")
}

fn study_ticket(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::MirrorShard) || state.has_item(Item::CoinToken) {
        state.remember(Flag::UnderstoodChildPromise);
        StoryEvent::new(
            "水痕里的第二个名字",
            "你用镜片或铜筹压住车票，终于读清水痕。07A 旁边原本还有 07B 的压痕；完整警告不是“别上车”，而是：别一个人上车，也别再命令他等。",
        )
        .tag("理解：孩子的承诺")
    } else {
        StoryEvent::new(
            "水痕太乱",
            "你需要能反光或能压住纸面的东西，才能读出更深的墨迹。",
        )
    }
}

fn show_ticket_to_traveler(state: &mut GameState) -> StoryEvent {
    if state.has_flag(Flag::ExaminedTicket) && state.traveler_depth > 0 {
        state.remember(Flag::TravelerTrusted);
        state.child_trust = (state.child_trust + 1).min(5);
        StoryEvent::new(
            "老人没有接票",
            "你把湿票递给老人。他看完后说：你每次都先给我看“别上车”这三个字，好像车害了你。可真正重要的东西不在这三个字里，在你还没读出来的那一半里。",
        )
        .tag("老人")
    } else {
        StoryEvent::new(
            "老人看着报纸",
            "你还没有把车票看明白，递过去也只是另一种逃避。",
        )
    }
}

fn read_departure_board(state: &mut GameState) -> StoryEvent {
    state.remember(Flag::ReadDepartureBoard);
    StoryEvent::new(
            "时刻表",
            "电子屏闪了三次。车次栏写着雾灯号，目的地栏却是一串姓名。你的名字短暂亮起，又被水痕一样的乱码盖住。旁边还有一格空栏，灯管坏了似的一明一暗。",
    )
}

fn talk_traveler(state: &mut GameState) -> StoryEvent {
    let depth = state.traveler_depth.min(NPC_THREAD_STEPS - 1);
    state.traveler_depth = (state.traveler_depth + 1).min(NPC_THREAD_STEPS);
    state.child_trust = (state.child_trust + (depth >= 2) as i8).min(4);
    let mut event = dialogue::traveler_event(depth);
    match depth {
        0 => grant_item(state, &mut event, Item::BrassKey),
        4 => grant_item(state, &mut event, Item::CoinToken),
        5 => remember_tag(
            state,
            &mut event,
            Flag::UnderstoodFirstLoop,
            "理解：借来的最后一分钟",
        ),
        _ => {}
    }
    event
}

fn talk_clerk(state: &mut GameState) -> StoryEvent {
    let depth = state.clerk_depth.min(NPC_THREAD_STEPS - 1);
    state.clerk_depth = (state.clerk_depth + 1).min(NPC_THREAD_STEPS);
    state.clerk_trust = (state.clerk_trust + 1).min(5);
    let mut event = dialogue::clerk_event(depth, state.has_item(Item::StationLog));
    if depth >= 2 {
        remember_tag(
            state,
            &mut event,
            Flag::UnderstoodChildPromise,
            "返程票规则",
        );
    }
    event
}

fn show_log_to_clerk(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::StationLog) {
        state.remember(Flag::ReadStationLog);
        state.clerk_trust = (state.clerk_trust + 2).min(5);
        StoryEvent::new(
            "玻璃后的停顿",
            "你把站务日志推给售票员。她读到你的旧笔迹后，票章悬在半空。账页翻到最后，07A 那栏被划过很多次，07B 那栏却只有指甲压出的白印。",
        )
        .tag("售票窗口")
    } else {
        StoryEvent::new("玻璃没有回应", "你手里没有能让她停止营业口吻的东西。")
    }
}

fn rewrite_ticket(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::StationLog) && state.has_item(Item::CoinToken) && state.clerk_trust >= 3
    {
        state.ticket = TicketKind::Return;
        state.remember(Flag::TicketRewritten);
        StoryEvent::new(
            "返程联票",
            "售票员盖下票章。湿票变成返程联票，背面警告也变完整：别一个人上车，别让听话的人替你的恐惧守规矩。现在你有资格准备返程，但孩子仍要自己决定是否跨线。",
        )
        .tag("车票已改签")
    } else {
        StoryEvent::new(
            "改签失败",
            "售票员摇头。她需要日志证明你曾经属于明天，也需要退票铜筹证明你愿意退掉旧执念。",
        )
    }
}

fn search_lost_found(state: &mut GameState) -> StoryEvent {
    state.remember(Flag::SearchedLostFound);
    let mut event = StoryEvent::new(
        "失物箱",
        "你在失物箱里找到一片雾灯玻璃，又在童衣口袋里找到裂开的姓名牌。姓名牌背面有一行铅笔字：哥哥叫我等，我就等。",
    );
    grant_item(state, &mut event, Item::LanternGlass);
    grant_item(state, &mut event, Item::NameTag);
    event
}

fn open_cabinet(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::BrassKey) {
        state.remember(Flag::OpenedCabinet);
        let mut event = StoryEvent::new(
            "铁柜里的档案",
            "黄铜钥匙打开了铁柜。里面有站务日志和一张烧焦的旧时刻表。日志末页是你的笔迹：我选择留下，是因为我还没有撤销那条命令。",
        );
        grant_item(state, &mut event, Item::StationLog);
        grant_item(state, &mut event, Item::OldTimetable);
        event
    } else {
        StoryEvent::new("打不开的柜门", "铁柜纹丝不动，锁孔里有黄铜磨痕。")
    }
}

fn listen_underpass(state: &mut GameState) -> StoryEvent {
    state.remember(Flag::HeardUnderpassEcho);
    if state.has_item(Item::NameTag) || state.has_flag(Flag::ExaminedTicket) {
        state.remember(Flag::RecoveredName);
        if state.has_item(Item::MirrorShard) || state.investigation_depth(Location::Underpass) >= 3
        {
            state.remember(Flag::UnderstoodFirstLoop);
        }
        StoryEvent::new(
            "慢半拍的回声",
            "你问自己是谁。回声说出了你的名字。你终于想起：六年前，你带着年幼的弟弟逃到雾灯站，想用一班夜车离开那个家。",
        )
        .tag("记忆碎片")
    } else {
        StoryEvent::new(
            "慢半拍的回声",
            "你听见回声里有一个名字，但还听不清。先找车票、姓名牌或其他身份线索。",
        )
    }
}

fn repair_fog_lamp(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::LanternGlass) {
        state.remove_item(Item::LanternGlass);
        state.remember(Flag::RepairedFogLamp);
        StoryEvent::new(
            "修复雾灯",
            "你把雾灯玻璃装回灯罩。雾被照开，月台远端出现一扇写着“广播室”的门；孩子袖口上露出和你同一批次的旧车票编号，只是他的号码停在 07B。",
        )
        .tag("雾灯已修复")
    } else {
        StoryEvent::new("缺少玻璃", "灯罩空着，雾只在里面打转。")
    }
}

fn meet_child(state: &mut GameState) -> StoryEvent {
    let depth = state.child_depth.min(NPC_THREAD_STEPS - 1);
    state.child_depth = (state.child_depth + 1).min(NPC_THREAD_STEPS);
    state.child_trust = (state.child_trust + 1).min(5);
    let mut event = dialogue::child_event(depth);
    match depth {
        0 => {
            state.remember(Flag::MetChild);
        }
        1 => grant_item(state, &mut event, Item::ChildHomework),
        4 => remember_tag(
            state,
            &mut event,
            Flag::UnderstoodChildPromise,
            "理解：两个名字",
        ),
        _ => {}
    }
    event
}

fn show_name_tag_to_child(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::NameTag) && state.has_flag(Flag::MetChild) {
        state.remember(Flag::UnderstoodChildPromise);
        state.child_trust = (state.child_trust + 2).min(5);
        StoryEvent::new(
            "他先看你的手",
            "你没有直接把姓名牌塞给孩子，只摊在掌心让他看。他愿意看牌子，也愿意继续听你说。你明白：姓名牌应该还给他自己，而不是拿来证明你的悔意。",
        )
        .tag("孩子")
    } else {
        StoryEvent::new(
            "白线后没有回答",
            "你还没有可以摊开的东西，或者他还没准备承认你在场。",
        )
    }
}

fn return_name_tag(state: &mut GameState) -> StoryEvent {
    if state.has_flag(Flag::RecoveredName)
        && state.has_flag(Flag::UnderstoodChildPromise)
        && state.has_item(Item::NameTag)
    {
        state.remove_item(Item::NameTag);
        state.remember(Flag::ReturnedNameTag);
        state.remember(Flag::ChildJoined);
        state.child_trust = 5;
        StoryEvent::new(
            "姓名牌",
            "你把姓名牌放进孩子掌心，说出他的名字，也说出自己的名字。孩子没有立刻跨线，只小声问：如果我动了，你还会骂我吗？你说不会。那条白线第一次像粉笔一样松动。",
        )
        .tag("同行者加入")
    } else {
        StoryEvent::new(
            "他还不相信",
            "你还没有足够的记忆证明自己是谁，也没有证明自己记得他是谁。",
        )
    }
}

fn talk_station_keeper(state: &mut GameState) -> StoryEvent {
    let depth = state.keeper_depth.min(NPC_THREAD_STEPS - 1);
    state.keeper_depth = (state.keeper_depth + 1).min(NPC_THREAD_STEPS);
    state.keeper_trust = (state.keeper_trust + 1).min(5);
    let mut event = dialogue::keeper_event(
        depth,
        state.has_item(Item::OldTimetable),
        state.has_flag(Flag::RepairedFogLamp),
    );
    match depth {
        0 => {
            state.remember(Flag::HeardClockTruth);
        }
        3 => remember_tag(
            state,
            &mut event,
            Flag::UnderstoodStationMechanism,
            "理解：旧钟代价",
        ),
        4 if state.has_item(Item::OldTimetable) && state.has_flag(Flag::RepairedFogLamp) => {
            grant_item(state, &mut event, Item::SignalWhistle);
        }
        _ => {}
    }
    event
}

fn show_timetable_to_keeper(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::OldTimetable) {
        state.remember(Flag::UnderstoodStationMechanism);
        state.keeper_trust = (state.keeper_trust + 2).min(5);
        StoryEvent::new(
            "烧焦的时刻表",
            "你把旧时刻表放到钟面下。站务员承认：这张纸证明车站规则被改过。那晚是你申请停住午夜，但不是只有你一个人签字。",
        )
        .tag("站务员")
    } else {
        StoryEvent::new("钟楼只剩齿轮声", "没有旧时刻表，他可以继续把一切说成必要。")
    }
}

fn align_clock(state: &mut GameState) -> StoryEvent {
    if state.has_flag(Flag::HeardClockTruth) && state.has_item(Item::BrassKey) {
        state.remember(Flag::AlignedClock);
        StoryEvent::new(
            "23:59 之后",
            "黄铜钥匙插入钟腹，齿轮重新咬合。旧钟可以继续走动，列车进站条件被推进。",
        )
        .tag("旧钟已校准")
    } else {
        StoryEvent::new("钟拒绝转动", "你还不知道它为什么停下。")
    }
}

fn inspect_rails(state: &mut GameState) -> StoryEvent {
    state.remember(Flag::InspectedRails);
    StoryEvent::new(
        "轨道尽头",
        "你检查铁轨，发现两组方向相反的轮痕：一组开进雾里，一组从雾里回来。雾灯号可以返程，只是返程票很难拿到。",
    )
}

fn blow_whistle(state: &mut GameState) -> StoryEvent {
    if state.has_item(Item::SignalWhistle) && state.has_flag(Flag::AlignedClock) {
        state.remember(Flag::SummonedConductor);
        state.remember(Flag::FinalTrainArrived);
        StoryEvent::new(
            "发车哨",
            "你吹响发车哨。雾灯号进站，广播提示：请作出最终选择。",
        )
        .tag("最终选择")
    } else {
        StoryEvent::new("哨声没有响", "旧钟还没有校准，发车哨暂时不能召回列车。")
    }
}

fn synthesize_clues(state: &mut GameState) -> StoryEvent {
    if !can_synthesize_next(state) {
        return StoryEvent::new("线索还缺一角", synthesis_detail(state));
    }

    let depth = state.synthesis_depth.min(NPC_THREAD_STEPS - 1);
    state.synthesis_depth = (state.synthesis_depth + 1).min(NPC_THREAD_STEPS);
    let mut event = content::synthesis_event(depth);
    match depth {
        0 => remember_tag(
            state,
            &mut event,
            Flag::SynthesizedRoute,
            "合成：车票与路线",
        ),
        1 => remember_tag(
            state,
            &mut event,
            Flag::SynthesizedChildTruth,
            "合成：孩子与姓名",
        ),
        2 => remember_tag(
            state,
            &mut event,
            Flag::UnderstoodStationMechanism,
            "合成：循环机制",
        ),
        3 => remember_tag(state, &mut event, Flag::SynthesizedRoute, "合成：返程规则"),
        4 => remember_tag(
            state,
            &mut event,
            Flag::SynthesizedStationTruth,
            "合成：广播室",
        ),
        5 => remember_tag(
            state,
            &mut event,
            Flag::SynthesizedStationTruth,
            "合成：最后一分钟",
        ),
        _ => {}
    }
    event
}

fn grant_item(state: &mut GameState, event: &mut StoryEvent, item: Item) {
    if state.add_item(item) {
        event.tags.push(format!("获得：{}", item.name()));
    }
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

fn events_after_action(
    state: &mut GameState,
    previous_segment: u8,
    mut event: StoryEvent,
) -> Vec<StoryEvent> {
    if state.final_train_due() && !state.has_flag(Flag::FinalTrainArrived) {
        state.remember(Flag::FinalTrainArrived);
        event
            .body
            .push_str(" 话音落下，三号月台传来刹车声。雾灯号到了。车站不再给出新的提示。");
        event.tags.push("最终选择".to_string());
    }

    let mut events = Vec::new();
    if let Some(interlude) = inner_voice::segment_event(state, previous_segment) {
        events.push(interlude);
    }
    if let Some(shift) = chapter::segment_shift_event(state, previous_segment) {
        events.push(shift);
    }
    events.push(event);
    events
}

fn apply_final_choice(state: &mut GameState, action: ActionId) -> Option<StoryEvent> {
    if let ActionId::EnterEndingPrelude(prelude) = action {
        return Some(final_prelude::enter(state, prelude));
    }
    if let ActionId::AnswerEndingPrelude(prelude, response) = action {
        return Some(final_prelude::answer(state, prelude, response));
    }
    if let ActionId::AnswerFinalDebate(debate, response) = action {
        return Some(final_debate::answer(state, debate, response));
    }

    let ending = match action {
        ActionId::BoardAlone if final_debate::final_choice_ready(state, Ending::EscapedAlone) => {
            Ending::EscapedAlone
        }
        ActionId::BoardWithChild
            if final_debate::final_choice_ready(state, Ending::TookChildHome) =>
        {
            Ending::TookChildHome
        }
        ActionId::BurnTimetable
            if final_debate::final_choice_ready(state, Ending::BurnedTimetable) =>
        {
            Ending::BurnedTimetable
        }
        ActionId::BroadcastName
            if final_debate::final_choice_ready(state, Ending::BecameTheVoice) =>
        {
            Ending::BecameTheVoice
        }
        ActionId::TakeKeeperSeat
            if final_debate::final_choice_ready(state, Ending::NewStationKeeper) =>
        {
            Ending::NewStationKeeper
        }
        ActionId::Wait => Ending::LostPassenger,
        ActionId::BoardAlone
            if final_prelude::route_ready_for_ending(state, Ending::EscapedAlone) =>
        {
            return Some(final_debate::missing_final_step_event(
                state,
                Ending::EscapedAlone,
            ));
        }
        ActionId::BoardWithChild
            if final_prelude::route_ready_for_ending(state, Ending::TookChildHome) =>
        {
            return Some(final_debate::missing_final_step_event(
                state,
                Ending::TookChildHome,
            ));
        }
        ActionId::BurnTimetable
            if final_prelude::route_ready_for_ending(state, Ending::BurnedTimetable) =>
        {
            return Some(final_debate::missing_final_step_event(
                state,
                Ending::BurnedTimetable,
            ));
        }
        ActionId::BroadcastName
            if final_prelude::route_ready_for_ending(state, Ending::BecameTheVoice) =>
        {
            return Some(final_debate::missing_final_step_event(
                state,
                Ending::BecameTheVoice,
            ));
        }
        ActionId::TakeKeeperSeat
            if final_prelude::route_ready_for_ending(state, Ending::NewStationKeeper) =>
        {
            return Some(final_debate::missing_final_step_event(
                state,
                Ending::NewStationKeeper,
            ));
        }
        _ => {
            return Some(StoryEvent::new(
                "还不能这样做",
                "你已经听见车门开启，但这个选择缺少关键条件。看看右侧行动说明，或者选择另一条路。",
            ));
        }
    };
    state.ended = Some(ending);
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_limit_summons_final_train() {
        let mut session = GameSession::new();
        start(&mut session);
        for _ in 0..(crate::model::MAX_NIGHT_MINUTES / 10) {
            apply(&mut session, ActionId::Wait);
        }
        assert!(session.state.has_flag(Flag::FinalTrainArrived));
        assert!(scene_actions(&session.state)
            .iter()
            .any(|action| matches!(action.id, ActionId::Wait)));
    }

    #[test]
    fn moving_between_locations_does_not_consume_time() {
        let mut session = GameSession::new();
        start(&mut session);

        apply(&mut session, ActionId::Move(Location::Platform));

        assert_eq!(session.state.location, Location::Platform);
        assert_eq!(session.state.elapsed_minutes(), 0);
        assert_eq!(
            time_cost_for_action(ActionId::Move(Location::WaitingHall)),
            0
        );
    }

    #[test]
    fn completed_one_shot_actions_leave_the_action_list() {
        let mut session = GameSession::new();
        start(&mut session);

        assert!(scene_actions(&session.state)
            .iter()
            .any(|action| matches!(action.id, ActionId::ExamineTicket)));
        apply(&mut session, ActionId::ExamineTicket);

        let actions = scene_actions(&session.state);
        assert!(!actions
            .iter()
            .any(|action| matches!(action.id, ActionId::ExamineTicket)));
        assert!(actions.iter().all(|action| action.enabled));
        assert!(actions
            .iter()
            .any(|action| action.detail.contains("耗时 10 分钟")));
    }

    #[test]
    fn completed_dialogue_choices_leave_the_active_dialogue_list() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        assert!(scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain)
            )
        }));
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::BackToRoot),
        );

        assert!(!scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain)
            )
        }));
    }

    #[test]
    fn segment_shift_emits_inner_voice_interlude() {
        let mut session = GameSession::new();
        start(&mut session);
        for _ in 0..3 {
            apply(&mut session, ActionId::Wait);
        }

        assert!(session.log.iter().any(|event| {
            event.title == "阶段提示：第二段午夜" && event.tags.iter().any(|tag| tag == "阶段提示")
        }));
        assert!(session.log.iter().any(|event| {
            event.title == "车站变化：屏幕开始记账"
                && event.tags.iter().any(|tag| tag == "车站变化")
        }));
    }

    #[test]
    fn station_anomaly_turns_chapter_pressure_into_playable_choice() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ReadDepartureBoard);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT;

        let anomaly_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::HandleAnomaly(_, _)))
            .collect::<Vec<_>>();
        let stabilize = anomaly_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::HandleAnomaly(
                        crate::model::AnomalyId::ScreenKeepsScore,
                        crate::model::AnomalyResponse::Stabilize
                    )
                )
            })
            .expect("screen anomaly should be visible in segment two");
        assert!(stabilize.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("车站异象可以处理")));

        apply(
            &mut session,
            ActionId::HandleAnomaly(
                crate::model::AnomalyId::ScreenKeepsScore,
                crate::model::AnomalyResponse::Stabilize,
            ),
        );
        assert_eq!(
            session
                .state
                .anomaly_response(crate::model::AnomalyId::ScreenKeepsScore),
            Some(crate::model::AnomalyResponse::Stabilize)
        );
        assert!(session.state.has_flag(Flag::SynthesizedRoute));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "车站异象")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("车站异象：1 / 7")));
    }

    #[test]
    fn chapter_pressure_changes_station_reading() {
        let mut state = GameState::new();
        state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 4;

        let hall = crate::content::location_description(&state);
        assert!(hall.contains("缺失的 07 号座位"));

        let objectives = crate::content::objective_summary(&state);
        assert!(objectives
            .iter()
            .any(|line| line.contains("地下通道会持续加压")));

        let chapter = crate::content::chapter_summary(&state);
        assert!(chapter.iter().any(|line| line.contains("车站变化")));
    }

    #[test]
    fn free_dialogue_topic_can_be_chosen_without_linear_thread() {
        let mut session = GameSession::new();
        start(&mut session);
        let topics = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::Discuss(_)))
            .collect::<Vec<_>>();
        assert!(topics.iter().any(|action| matches!(
            action.id,
            ActionId::Discuss(crate::model::TopicId::TravelerRain)
        )));

        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        assert!(session
            .state
            .has_discussed(crate::model::TopicId::TravelerRain));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "自由对话")));
    }

    #[test]
    fn dialogue_system_keeps_active_session_and_choice_actions() {
        let mut session = GameSession::new();
        start(&mut session);
        assert!(!scene_actions(&session.state)
            .iter()
            .any(|action| matches!(action.id, ActionId::TalkTraveler)));
        let entries = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::BeginDialogue(_)))
            .collect::<Vec<_>>();
        assert!(entries.iter().any(|action| {
            matches!(
                action.id,
                ActionId::BeginDialogue(crate::model::DialogueId::Traveler)
            )
        }));

        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        assert!(session.state.active_dialogue.is_some());
        assert!(crate::content::condition_summary(&session.state)
            .iter()
            .any(|line| line.contains("对话中：候车厅老人")));

        let choices = scene_actions(&session.state);
        assert!(choices.iter().all(|action| matches!(
            action.id,
            ActionId::ChooseDialogue(_)
                | ActionId::SetDialogueTone(_)
                | ActionId::Discuss(_)
                | ActionId::PresentEvidence(_)
        )));
        assert!(choices.iter().any(|action| {
            matches!(
                action.id,
                ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain)
            )
        }));
        assert!(choices
            .iter()
            .any(|action| matches!(action.id, ActionId::SetDialogueTone(DialogueTone::Gentle))));
        assert!(choices.iter().any(|action| {
            matches!(
                action.id,
                ActionId::Discuss(crate::model::TopicId::TravelerRain)
            ) && action.label.contains("话题追问")
        }));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("正在和候车厅老人谈")));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        assert!(session
            .state
            .has_discussed(crate::model::TopicId::TravelerRain));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "对话选择")));
        assert_eq!(
            session.state.active_dialogue,
            Some(crate::model::ActiveDialogue {
                dialogue: crate::model::DialogueId::Traveler,
                node: crate::model::DialogueNodeId::Memory,
            })
        );
        let node_choices = scene_actions(&session.state);
        assert!(node_choices.iter().any(|action| {
            matches!(
                action.id,
                ActionId::ChooseDialogue(crate::model::DialogueChoiceId::DeepenTopic)
            )
        }));
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::DeepenTopic),
        );
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "话题节点")));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        assert!(session.state.active_dialogue.is_none());
    }

    #[test]
    fn new_dialogue_replaces_legacy_talk_buttons_for_core_characters() {
        let mut session = GameSession::new();
        start(&mut session);
        let waiting_hall = scene_actions(&session.state);
        assert!(waiting_hall.iter().any(|action| matches!(
            action.id,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler)
        )));
        assert!(!waiting_hall
            .iter()
            .any(|action| matches!(action.id, ActionId::TalkTraveler)));

        apply(&mut session, ActionId::Move(Location::Platform));
        let platform = scene_actions(&session.state);
        assert!(platform.iter().any(|action| matches!(
            action.id,
            ActionId::BeginDialogue(crate::model::DialogueId::Child)
        )));
        assert!(!platform
            .iter()
            .any(|action| matches!(action.id, ActionId::MeetChild)));
    }

    #[test]
    fn dialogue_nodes_carry_main_progress_rewards() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        assert!(session.state.has_item(Item::BrassKey));
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::BackToRoot),
        );
        session.state.remember(Flag::ReadDepartureBoard);
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerEmptySeat),
        );
        assert!(session.state.has_item(Item::CoinToken));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(&mut session, ActionId::Move(Location::Platform));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Child),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );
        assert!(session.state.has_flag(Flag::MetChild));
        assert!(session.state.has_item(Item::ChildHomework));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(&mut session, ActionId::Move(Location::ClockTower));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Keeper),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::KeeperDuty),
        );
        assert!(session.state.has_flag(Flag::HeardClockTruth));
    }

    #[test]
    fn dialogue_leads_turn_finished_beats_into_exploration_actions() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );

        let lead_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::FollowDialogueLead(_)))
            .collect::<Vec<_>>();
        let rain = lead_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::FollowDialogueLead(crate::model::DialogueLeadId::RainUnderBench)
                )
            })
            .expect("rain dialogue lead should appear after traveler rain beat");
        assert!(rain.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("对话线索可以追查")));

        apply(
            &mut session,
            ActionId::FollowDialogueLead(crate::model::DialogueLeadId::RainUnderBench),
        );
        assert!(session
            .state
            .has_completed_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench));
        assert!(session.state.has_item(Item::MirrorShard));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "对话线索")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("对话线索：1 / 8")));
    }

    #[test]
    fn completed_dialogue_leads_can_return_to_active_dialogue() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(
            &mut session,
            ActionId::FollowDialogueLead(crate::model::DialogueLeadId::RainUnderBench),
        );
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::ReturnDialogueLead(crate::model::DialogueLeadId::RainUnderBench)
            ) && action.label.contains("带回线索")
        }));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("可以把已追查的线索带回对话")));

        apply(
            &mut session,
            ActionId::ReturnDialogueLead(crate::model::DialogueLeadId::RainUnderBench),
        );
        assert!(session
            .state
            .has_returned_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "线索回谈")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("带回线索")));
    }

    #[test]
    fn returned_dialogue_leads_can_be_relayed_to_other_active_dialogues() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(
            &mut session,
            ActionId::FollowDialogueLead(crate::model::DialogueLeadId::RainUnderBench),
        );
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        apply(
            &mut session,
            ActionId::ReturnDialogueLead(crate::model::DialogueLeadId::RainUnderBench),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(&mut session, ActionId::Move(Location::Platform));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Child),
        );
        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );

        let relay_action = scene_actions(&session.state)
            .into_iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::RelayDialogueLead(crate::model::DialogueRelayId::MirrorToChild)
                )
            })
            .expect("mirror relay should appear in child dialogue");
        assert!(relay_action.enabled);
        assert!(relay_action.label.contains("转述线索"));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("可以在当前对话里转述线索")));

        apply(
            &mut session,
            ActionId::RelayDialogueLead(crate::model::DialogueRelayId::MirrorToChild),
        );
        assert!(session
            .state
            .has_completed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "线索转述")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("线索转述")));

        let reflection_action = scene_actions(&session.state)
            .into_iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::ReflectDialogueRelay(crate::model::DialogueRelayId::MirrorToChild)
                )
            })
            .expect("mirror relay reflection should remain available after the relay");
        assert!(reflection_action.enabled);
        assert!(reflection_action.label.contains("追问转述余波"));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("可以继续追问转述余波")));

        apply(
            &mut session,
            ActionId::ReflectDialogueRelay(crate::model::DialogueRelayId::MirrorToChild),
        );
        assert!(session
            .state
            .has_reflected_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "转述余波")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("转述余波")));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        apply(&mut session, ActionId::Move(Location::WaitingHall));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        let echo_action = scene_actions(&session.state)
            .into_iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::EchoDialogueRelay(crate::model::DialogueRelayId::MirrorToChild)
                )
            })
            .expect("mirror relay echo should be available when returning to traveler");
        assert!(echo_action.enabled);
        assert!(echo_action.label.contains("带回转述回声"));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("可以把转述回声带回原人物")));

        apply(
            &mut session,
            ActionId::EchoDialogueRelay(crate::model::DialogueRelayId::MirrorToChild),
        );
        assert!(session
            .state
            .has_echoed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "转述回声")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("转述回声")));

        apply(
            &mut session,
            ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        let anchor_action = scene_actions(&session.state)
            .into_iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::AnchorDialogueRelay(crate::model::DialogueRelayId::MirrorToChild)
                )
            })
            .expect("mirror relay anchor should become a location action after echoing");
        assert!(anchor_action.enabled);
        assert!(anchor_action.label.contains("安放回声"));

        apply(
            &mut session,
            ActionId::AnchorDialogueRelay(crate::model::DialogueRelayId::MirrorToChild),
        );
        assert!(session
            .state
            .has_anchored_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "回声落点")));
        assert!(crate::content::location_description(&session.state).contains("镜片"));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("可以复看回声落点")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("线索转述：1 / 8 条，余波 1 段，回声 1 段，落点 1 个")));

        let review_action = scene_actions(&session.state)
            .into_iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::ReviewDialogueAnchor(crate::model::DialogueRelayId::MirrorToChild)
                )
            })
            .expect("mirror relay anchor review should become available after anchoring");
        assert!(review_action.enabled);
        assert!(review_action.label.contains("复看回声落点"));

        apply(
            &mut session,
            ActionId::ReviewDialogueAnchor(crate::model::DialogueRelayId::MirrorToChild),
        );
        assert!(session
            .state
            .has_reviewed_dialogue_anchor(crate::model::DialogueRelayId::MirrorToChild));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "回声复看")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("复看 1 个")));
        assert!(crate::content::location_description(&session.state).contains("复看过那枚镜片"));
    }

    #[test]
    fn active_dialogue_allows_evidence_follow_up_inside_session() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.add_item(Item::MirrorShard);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::PresentEvidence(crate::model::EvidenceId::TravelerMirror)
            ) && action.label.contains("证据追问")
        }));

        apply(
            &mut session,
            ActionId::PresentEvidence(crate::model::EvidenceId::TravelerMirror),
        );
        assert!(session
            .state
            .has_presented(crate::model::EvidenceId::TravelerMirror));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("证据追问")));
    }

    #[test]
    fn active_dialogue_allows_free_topic_follow_up_inside_session() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::Discuss(crate::model::TopicId::TravelerRain)
            ) && action.label.contains("话题追问")
        }));

        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        assert!(session
            .state
            .has_discussed(crate::model::TopicId::TravelerRain));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("话题追问")));
    }

    #[test]
    fn active_dialogue_allows_unlocked_free_questions_inside_session() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::Move(Location::TicketOffice));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Clerk),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::AskDialogueQuestion(
                    crate::model::DialogueQuestionId::ClerkAboutWetTicket
                )
            ) && action.label.contains("自由询问")
        }));

        apply(
            &mut session,
            ActionId::AskDialogueQuestion(crate::model::DialogueQuestionId::ClerkAboutWetTicket),
        );
        assert!(session
            .state
            .has_answered_dialogue_question(crate::model::DialogueQuestionId::ClerkAboutWetTicket));
        assert!(session.state.active_dialogue.is_some());
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "自由询问")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("自由询问")));
    }

    #[test]
    fn active_dialogue_allows_npc_challenge_responses() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::Move(Location::TicketOffice));
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Clerk),
        );
        apply(
            &mut session,
            ActionId::AskDialogueQuestion(crate::model::DialogueQuestionId::ClerkAboutWetTicket),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::AnswerDialogueChallenge(
                    crate::model::DialogueChallengeId::ClerkAsksWhoPaysForReturn,
                    crate::model::DialogueChallengeResponseId::Promise
                )
            ) && action.label.contains("回应反问")
        }));

        apply(
            &mut session,
            ActionId::AnswerDialogueChallenge(
                crate::model::DialogueChallengeId::ClerkAsksWhoPaysForReturn,
                crate::model::DialogueChallengeResponseId::Promise,
            ),
        );
        assert_eq!(
            session.state.dialogue_challenge_response(
                crate::model::DialogueChallengeId::ClerkAsksWhoPaysForReturn
            ),
            Some(crate::model::DialogueChallengeResponseId::Promise)
        );
        assert!(session.state.active_dialogue.is_some());
        assert!(session.state.has_flag(Flag::SynthesizedRoute));
        assert!(session.latest().is_some_and(|event| {
            event.tags.iter().any(|tag| tag == "NPC反问")
                && event.tags.iter().any(|tag| tag == "立场回应")
        }));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("立场回应")));
    }

    #[test]
    fn active_dialogue_allows_tone_shift_without_spending_action() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );
        let before_actions = session.state.actions_used;
        let actions = scene_actions(&session.state);
        assert!(actions
            .iter()
            .any(|action| matches!(action.id, ActionId::SetDialogueTone(DialogueTone::Direct))));

        apply(
            &mut session,
            ActionId::SetDialogueTone(DialogueTone::Direct),
        );
        assert_eq!(session.state.dialogue_tone, DialogueTone::Direct);
        assert_eq!(session.state.actions_used, before_actions);
        assert_eq!(
            session.state.active_dialogue,
            Some(crate::model::ActiveDialogue {
                dialogue: crate::model::DialogueId::Traveler,
                node: crate::model::DialogueNodeId::Root,
            })
        );
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("语气调整")));
    }

    #[test]
    fn dialogue_tone_changes_free_dialogue_consequences() {
        let mut session = GameSession::new();
        start(&mut session);
        let tone_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::SetDialogueTone(_)))
            .collect::<Vec<_>>();
        assert!(tone_actions
            .iter()
            .any(|action| matches!(action.id, ActionId::SetDialogueTone(DialogueTone::Gentle))));

        apply(
            &mut session,
            ActionId::SetDialogueTone(DialogueTone::Gentle),
        );
        assert_eq!(session.state.dialogue_tone, DialogueTone::Gentle);
        assert!(crate::content::condition_summary(&session.state)
            .iter()
            .any(|line| line.contains("说话方式：把问题放轻")));

        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "语气：放轻")));

        let mut direct_session = GameSession::new();
        start(&mut direct_session);
        apply(
            &mut direct_session,
            ActionId::SetDialogueTone(DialogueTone::Direct),
        );
        apply(&mut direct_session, ActionId::Move(Location::TicketOffice));
        apply(
            &mut direct_session,
            ActionId::Discuss(crate::model::TopicId::ClerkDestination),
        );
        assert_eq!(direct_session.state.clerk_trust, 1);
        assert!(direct_session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "语气：逼近")));
    }

    #[test]
    fn environmental_dialogue_extends_exploration_locations() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::Move(Location::LostAndFound));

        let lost_found_topics = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::Discuss(_)))
            .collect::<Vec<_>>();
        assert!(lost_found_topics.iter().any(|action| matches!(
            action.id,
            ActionId::Discuss(crate::model::TopicId::LostFoundLabels)
        )));

        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::LostFoundLabels),
        );
        assert!(session
            .state
            .has_discussed(crate::model::TopicId::LostFoundLabels));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "自由对话")));

        apply(&mut session, ActionId::Move(Location::Underpass));
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::UnderpassEcho),
        );
        assert!(session.state.has_flag(Flag::RecoveredName));
    }

    #[test]
    fn evidence_presentation_reacts_to_found_items() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);

        let evidence_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::PresentEvidence(_)))
            .collect::<Vec<_>>();
        assert!(evidence_actions.iter().any(|action| matches!(
            action.id,
            ActionId::PresentEvidence(crate::model::EvidenceId::TravelerMirror)
        )));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("镜片")));

        apply(
            &mut session,
            ActionId::PresentEvidence(crate::model::EvidenceId::TravelerMirror),
        );
        assert!(session
            .state
            .has_presented(crate::model::EvidenceId::TravelerMirror));
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "证据追问")));
    }

    #[test]
    fn case_file_resolution_combines_freeform_clues() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::ReadDepartureBoard);

        let case_file_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::ResolveCaseFile(_)))
            .collect::<Vec<_>>();
        let wet_ticket = case_file_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::ResolveCaseFile(crate::model::CaseFileId::WetTicketProtocol)
                )
            })
            .expect("wet ticket case file should be visible");
        assert!(wet_ticket.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("站内档案可以归档")));

        apply(
            &mut session,
            ActionId::ResolveCaseFile(crate::model::CaseFileId::WetTicketProtocol),
        );
        assert!(session
            .state
            .has_resolved_case_file(crate::model::CaseFileId::WetTicketProtocol));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "组合推理")));
    }

    #[test]
    fn case_dialogue_returns_archived_truth_to_active_dialogue() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(
            &mut session,
            ActionId::ResolveCaseFile(crate::model::CaseFileId::WetTicketProtocol),
        );
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| matches!(
            action.id,
            ActionId::DiscussCaseDialogue(crate::model::CaseDialogueId::WetTicketTraveler)
        )));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("档案回谈")));

        apply(
            &mut session,
            ActionId::DiscussCaseDialogue(crate::model::CaseDialogueId::WetTicketTraveler),
        );
        assert!(session
            .state
            .has_completed_case_dialogue(crate::model::CaseDialogueId::WetTicketTraveler));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "档案回谈")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("档案回谈")));
    }

    #[test]
    fn station_request_turns_optional_exploration_into_rewarded_side_quest() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);

        let request_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::CompleteRequest(_)))
            .collect::<Vec<_>>();
        let correction = request_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::CompleteRequest(crate::model::StationRequestId::NewspaperCorrection)
                )
            })
            .expect("newspaper correction request should be visible");
        assert!(correction.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("旅客委托可以完成")));

        apply(
            &mut session,
            ActionId::CompleteRequest(crate::model::StationRequestId::NewspaperCorrection),
        );
        assert!(session
            .state
            .has_completed_request(crate::model::StationRequestId::NewspaperCorrection));
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "旅客委托")));
    }

    #[test]
    fn resonance_turns_free_dialogue_into_cross_character_play() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);

        let resonance_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::ResolveResonance(_)))
            .collect::<Vec<_>>();
        let rain = resonance_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::ResolveResonance(crate::model::ResonanceId::RainInTheMirror)
                )
            })
            .expect("rain resonance should be visible");
        assert!(rain.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("人物共鸣可以触发")));

        apply(
            &mut session,
            ActionId::ResolveResonance(crate::model::ResonanceId::RainInTheMirror),
        );
        assert!(session
            .state
            .has_resolved_resonance(crate::model::ResonanceId::RainInTheMirror));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "交叉对话")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("人物共鸣：1 / 6")));
    }

    #[test]
    fn vow_turns_discovery_into_player_stance() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(
            &mut session,
            ActionId::ResolveResonance(crate::model::ResonanceId::RainInTheMirror),
        );

        let vow_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::MakeVow(_)))
            .collect::<Vec<_>>();
        let warning = vow_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::MakeVow(crate::model::VowId::ReadTheWholeWarning)
                )
            })
            .expect("warning vow should be visible");
        assert!(warning.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("内心锚点可以写下")));

        apply(
            &mut session,
            ActionId::MakeVow(crate::model::VowId::ReadTheWholeWarning),
        );
        assert!(session
            .state
            .has_vow(crate::model::VowId::ReadTheWholeWarning));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "立场选择")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("内心锚点：1 / 6")));
    }

    #[test]
    fn memory_corridor_adds_location_specific_midgame_scene() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);

        let memory_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::EnterMemory(_)))
            .collect::<Vec<_>>();
        let bench = memory_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::EnterMemory(crate::model::MemoryId::SeventhBench)
                )
            })
            .expect("seventh bench memory should be visible in waiting hall");
        assert!(bench.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("记忆回廊可以进入")));

        apply(
            &mut session,
            ActionId::EnterMemory(crate::model::MemoryId::SeventhBench),
        );
        assert!(session
            .state
            .has_memory(crate::model::MemoryId::SeventhBench));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "地点记忆")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("记忆回廊：1 / 8")));
    }

    #[test]
    fn night_patrol_rewards_revisiting_explored_locations() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);

        let patrol_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::TakePatrol(_)))
            .collect::<Vec<_>>();
        let hall = patrol_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::TakePatrol(crate::model::PatrolId::WaitingHallManifest)
                )
            })
            .expect("waiting hall patrol should be visible after revisiting clues");
        assert!(hall.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("巡夜记录可以完成")));

        apply(
            &mut session,
            ActionId::TakePatrol(crate::model::PatrolId::WaitingHallManifest),
        );
        assert!(session
            .state
            .has_completed_patrol(crate::model::PatrolId::WaitingHallManifest));
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "自由探索")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("巡夜记录：1 / 6")));
    }

    #[test]
    fn follow_up_dialogue_responds_to_completed_exploration() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(
            &mut session,
            ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        apply(&mut session, ActionId::ReadDepartureBoard);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(&mut session, ActionId::InvestigateLocation);
        apply(
            &mut session,
            ActionId::TakePatrol(crate::model::PatrolId::WaitingHallManifest),
        );

        let aftertalk_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::FollowUpDialogue(_)))
            .collect::<Vec<_>>();
        let traveler = aftertalk_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::FollowUpDialogue(crate::model::AftertalkId::TravelerSecondSeat)
                )
            })
            .expect("traveler follow-up should appear after the patrol");
        assert!(traveler.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("回访对话可以继续")));

        apply(
            &mut session,
            ActionId::FollowUpDialogue(crate::model::AftertalkId::TravelerSecondSeat),
        );
        assert!(session
            .state
            .has_completed_aftertalk(crate::model::AftertalkId::TravelerSecondSeat));
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "回访对话")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("回访对话：1 / 9")));
    }

    #[test]
    fn companion_dialogue_turns_child_joining_into_free_exploration() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::MetChild);
        session.state.remember(Flag::ChildJoined);
        session.state.child_trust = 5;
        session
            .state
            .visit_memory(crate::model::MemoryId::SeventhBench);
        session.state.discuss(crate::model::TopicId::TravelerRain);

        let companion_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::CompanionDialogue(_)))
            .collect::<Vec<_>>();
        let hall = companion_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::CompanionDialogue(
                        crate::model::CompanionTalkId::WaitingHallEmptySeat
                    )
                )
            })
            .expect("waiting hall companion talk should appear");
        assert!(hall.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("同行对话可以继续")));

        apply(
            &mut session,
            ActionId::CompanionDialogue(crate::model::CompanionTalkId::WaitingHallEmptySeat),
        );
        assert!(session
            .state
            .has_completed_companion_talk(crate::model::CompanionTalkId::WaitingHallEmptySeat));
        assert!(session.state.has_flag(Flag::TravelerTrusted));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "同行对话")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("同行对话：1 / 12")));
    }

    #[test]
    fn lamp_focus_turns_repaired_fog_lamp_into_exploration_tool() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::RepairedFogLamp);
        session.state.add_item(Item::StationMap);
        session.state.remember(Flag::ReadDepartureBoard);

        let focus_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::FocusFogLamp(_)))
            .collect::<Vec<_>>();
        let hall = focus_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::FocusFogLamp(crate::model::LampFocusId::WaitingHallBenchTrace)
                )
            })
            .expect("waiting hall lamp focus should appear");
        assert!(hall.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("雾灯照证可以完成")));

        apply(
            &mut session,
            ActionId::FocusFogLamp(crate::model::LampFocusId::WaitingHallBenchTrace),
        );
        assert!(session
            .state
            .has_focused_lamp_trace(crate::model::LampFocusId::WaitingHallBenchTrace));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "雾灯照证")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("雾灯照证：1 / 6")));
    }

    #[test]
    fn station_whisper_rewards_deeper_free_exploration() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT;
        session.state.location_depths[Location::WaitingHall.index()] = 2;

        let whisper_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::ListenStationWhisper(_)))
            .collect::<Vec<_>>();
        let umbrella = whisper_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::ListenStationWhisper(
                        crate::model::StationWhisperId::WaitingHallUmbrellaCount
                    )
                )
            })
            .expect("waiting hall whisper should appear");
        assert!(umbrella.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("站内低语可以聆听")));

        apply(
            &mut session,
            ActionId::ListenStationWhisper(
                crate::model::StationWhisperId::WaitingHallUmbrellaCount,
            ),
        );
        assert!(session
            .state
            .has_heard_station_whisper(crate::model::StationWhisperId::WaitingHallUmbrellaCount));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "站内低语")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("站内低语：1 / 12")));
    }

    #[test]
    fn midgame_truth_scene_becomes_playable_and_changes_story_state() {
        let mut session = GameSession::new();
        start(&mut session);
        apply(&mut session, ActionId::ExamineTicket);
        apply(&mut session, ActionId::ReadDepartureBoard);

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::RevealTruth(crate::model::TruthSceneId::WetTicketWarning)
            ) && action.label.contains("揭开真相")
        }));

        apply(
            &mut session,
            ActionId::RevealTruth(crate::model::TruthSceneId::WetTicketWarning),
        );
        assert!(session
            .state
            .has_revealed_truth_scene(crate::model::TruthSceneId::WetTicketWarning));
        assert!(session.state.has_flag(Flag::UnderstoodFirstLoop));
        assert!(session.latest().is_some_and(|event| {
            event.title.contains("湿票") && event.tags.iter().any(|tag| tag == "真相揭露")
        }));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("中段真相：1 /")));
    }

    #[test]
    fn route_preparation_turns_ending_route_into_concrete_setup() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::RecoveredName);
        session.state.remember(Flag::InspectedRails);
        session
            .state
            .visit_memory(crate::model::MemoryId::SeventhBench);

        let departure_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::PrepareDeparture(_)))
            .collect::<Vec<_>>();
        let single = departure_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::PrepareDeparture(crate::model::DepartureId::SingleReturnPocket)
                )
            })
            .expect("single return route prep should be visible in waiting hall");
        assert!(single.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线准备可以完成")));

        apply(
            &mut session,
            ActionId::PrepareDeparture(crate::model::DepartureId::SingleReturnPocket),
        );
        assert!(session
            .state
            .has_prepared_departure(crate::model::DepartureId::SingleReturnPocket));
        assert!(session.state.has_flag(Flag::SynthesizedRoute));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "终局铺垫")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线准备：1 / 6")));
    }

    #[test]
    fn route_trial_turns_prepared_route_into_playable_rehearsal() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 4;
        session.state.remember(Flag::RecoveredName);
        session
            .state
            .prepare_departure(crate::model::DepartureId::SingleReturnPocket);
        session.state.resolve_anomaly(
            crate::model::AnomalyId::ScreenKeepsScore,
            crate::model::AnomalyResponse::Stabilize,
        );

        let trial_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::RehearseRoute(_)))
            .collect::<Vec<_>>();
        let single = trial_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::RehearseRoute(crate::model::DepartureId::SingleReturnPocket)
                )
            })
            .expect("single return route trial should be visible");
        assert!(single.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线试炼可以进入")));

        apply(
            &mut session,
            ActionId::RehearseRoute(crate::model::DepartureId::SingleReturnPocket),
        );
        assert!(session
            .state
            .has_rehearsed_departure(crate::model::DepartureId::SingleReturnPocket));
        assert!(session.state.has_flag(Flag::SynthesizedRoute));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "路线试炼")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线试炼：1 / 6")));
    }

    #[test]
    fn route_cost_turns_rehearsed_route_into_mitigated_consequence() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        session
            .state
            .rehearse_departure(crate::model::DepartureId::SingleReturnPocket);
        session
            .state
            .complete_aftertalk(crate::model::AftertalkId::TravelerSecondSeat);
        session
            .state
            .make_vow(crate::model::VowId::ReturnWithoutErasing);

        let cost_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::MitigateRouteCost(_)))
            .collect::<Vec<_>>();
        let alone = cost_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::MitigateRouteCost(crate::model::RouteCostId::AloneEmptySeat)
                )
            })
            .expect("single return route cost should be visible in waiting hall");
        assert!(alone.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线代价可以调停")));

        apply(
            &mut session,
            ActionId::MitigateRouteCost(crate::model::RouteCostId::AloneEmptySeat),
        );
        assert!(session
            .state
            .has_mitigated_route_cost(crate::model::RouteCostId::AloneEmptySeat));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "路线代价")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线代价：1 / 6")));
    }

    #[test]
    fn route_echo_turns_mitigated_cost_back_into_free_dialogue() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        session.state.dialogue_tone = DialogueTone::Gentle;
        session
            .state
            .mitigate_route_cost(crate::model::RouteCostId::AloneEmptySeat);

        let echo_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::DiscussRouteEcho(_)))
            .collect::<Vec<_>>();
        let alone = echo_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::DiscussRouteEcho(crate::model::RouteCostId::AloneEmptySeat)
                )
            })
            .expect("single return route echo should be visible in waiting hall");
        assert!(alone.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线回声可以交谈")));

        apply(
            &mut session,
            ActionId::DiscussRouteEcho(crate::model::RouteCostId::AloneEmptySeat),
        );
        assert!(session
            .state
            .has_completed_route_echo(crate::model::RouteCostId::AloneEmptySeat));
        let latest = session.latest().expect("route echo event should be logged");
        assert!(latest.tags.iter().any(|tag| tag == "路线回声"));
        assert!(latest.tags.iter().any(|tag| tag == "语气：把问题放轻"));
        assert!(latest.body.contains("空座"));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线回声：1 / 6")));
    }

    #[test]
    fn route_witness_turns_route_echo_into_location_scene() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 6;
        session
            .state
            .complete_route_echo(crate::model::RouteCostId::AloneEmptySeat);

        let witness_actions = scene_actions(&session.state)
            .into_iter()
            .filter(|action| matches!(action.id, ActionId::VisitRouteWitness(_)))
            .collect::<Vec<_>>();
        let alone = witness_actions
            .iter()
            .find(|action| {
                matches!(
                    action.id,
                    ActionId::VisitRouteWitness(crate::model::RouteWitnessId::AloneSeatNotice)
                )
            })
            .expect("single return route witness should be visible in waiting hall");
        assert!(alone.enabled);
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线现场可以回访")));

        apply(
            &mut session,
            ActionId::VisitRouteWitness(crate::model::RouteWitnessId::AloneSeatNotice),
        );
        assert!(session
            .state
            .has_visited_route_witness(crate::model::RouteWitnessId::AloneSeatNotice));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "路线现场")));
        assert!(crate::content::chapter_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线现场：1 / 6")));
    }

    #[test]
    fn route_witness_debrief_returns_location_scene_to_dialogue() {
        let mut session = GameSession::new();
        start(&mut session);
        session
            .state
            .visit_route_witness(crate::model::RouteWitnessId::AloneSeatNotice);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Traveler),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| matches!(
            action.id,
            ActionId::DebriefRouteWitness(crate::model::RouteWitnessDebriefId::AloneSeatTraveler)
        )));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("路线现场")));

        apply(
            &mut session,
            ActionId::DebriefRouteWitness(crate::model::RouteWitnessDebriefId::AloneSeatTraveler),
        );
        assert!(session.state.has_completed_route_witness_debrief(
            crate::model::RouteWitnessDebriefId::AloneSeatTraveler
        ));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "路线复盘")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("路线复盘")));
    }

    #[test]
    fn final_interview_returns_late_route_progress_to_dialogue() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.location = crate::model::Location::Platform;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 7;
        session
            .state
            .complete_route_witness_debrief(crate::model::RouteWitnessDebriefId::ChildSeatChild);
        apply(
            &mut session,
            ActionId::BeginDialogue(crate::model::DialogueId::Child),
        );

        let actions = scene_actions(&session.state);
        assert!(actions.iter().any(|action| matches!(
            action.id,
            ActionId::HoldFinalInterview(crate::model::FinalInterviewId::ChildOrdinaryTomorrow)
        )));
        assert!(crate::content::objective_summary(&session.state)
            .iter()
            .any(|line| line.contains("终局前长谈")));

        apply(
            &mut session,
            ActionId::HoldFinalInterview(crate::model::FinalInterviewId::ChildOrdinaryTomorrow),
        );
        assert!(session
            .state
            .has_completed_final_interview(crate::model::FinalInterviewId::ChildOrdinaryTomorrow));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "终局前长谈")));
        assert!(session
            .state
            .dialogue_transcript
            .iter()
            .any(|entry| entry.title.contains("终局前长谈")));
    }

    #[test]
    fn objective_summary_points_to_next_playable_steps() {
        let mut session = GameSession::new();
        start(&mut session);
        let opening_objectives = crate::content::objective_summary(&session.state);
        assert!(opening_objectives.iter().any(|line| line.contains("湿票")));

        apply(&mut session, ActionId::ExamineTicket);
        let after_ticket = crate::content::objective_summary(&session.state);
        assert!(!after_ticket.iter().any(|line| line.contains("第一层笔迹")));
        assert!(after_ticket.iter().any(|line| line.contains("黄铜钥匙")));
    }

    #[test]
    fn final_objectives_explain_available_endings() {
        let mut state = GameState::new();
        state.remember(Flag::FinalTrainArrived);
        let blocked = crate::content::objective_summary(&state);
        assert!(blocked.iter().any(|line| line.contains("普通时间结束")));
        assert!(blocked.iter().any(|line| line.contains("独自上车还缺")));

        state.ticket = TicketKind::Return;
        state.remember(Flag::RecoveredName);
        state.remember(Flag::ChildJoined);
        state.remember(Flag::SynthesizedChildTruth);
        let available = crate::content::objective_summary(&state);
        assert!(available
            .iter()
            .any(|line| line.contains("先完成终局前场景：车门前的空座")));
        assert!(available
            .iter()
            .any(|line| line.contains("先完成终局前场景：白线前的撤销")));

        state.complete_ending_prelude(crate::model::EndingPreludeId::AloneDoor);
        state.complete_ending_prelude(crate::model::EndingPreludeId::ChildWhiteLine);
        state.answer_ending_prelude(
            crate::model::EndingPreludeId::AloneDoor,
            crate::model::EndingPreludeResponseId::AcceptCost,
        );
        state.answer_ending_prelude(
            crate::model::EndingPreludeId::ChildWhiteLine,
            crate::model::EndingPreludeResponseId::ReturnChoice,
        );
        let after_prelude = crate::content::objective_summary(&state);
        assert!(after_prelude
            .iter()
            .any(|line| line.contains("先回应终局争辩")));
        state.answer_final_debate(
            crate::model::FinalDebateId::AloneTraveler,
            crate::model::FinalDebateResponseId::AdmitWound,
        );
        state.answer_final_debate(
            crate::model::FinalDebateId::ChildWhiteLine,
            crate::model::FinalDebateResponseId::RewritePromise,
        );
        let after_debate = crate::content::objective_summary(&state);
        assert!(after_debate
            .iter()
            .any(|line| line.contains("可选：独自上车")));
        assert!(after_debate
            .iter()
            .any(|line| line.contains("可选：带孩子上车")));
    }

    #[test]
    fn final_choice_requires_route_specific_prelude() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::FinalTrainArrived);
        session.state.remember(Flag::RecoveredName);
        session.state.ticket = TicketKind::Return;

        let final_actions = scene_actions(&session.state);
        assert!(final_actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::EnterEndingPrelude(crate::model::EndingPreludeId::AloneDoor)
            )
        }));
        let board = final_actions
            .iter()
            .find(|action| matches!(action.id, ActionId::BoardAlone))
            .expect("board alone final action should be listed");
        assert!(!board.enabled);
        assert!(board.detail.contains("终局前场景"));

        apply(&mut session, ActionId::BoardAlone);
        assert!(session.state.ended.is_none());
        assert!(session
            .latest()
            .is_some_and(|event| event.title.contains("还差最后一幕")));

        apply(
            &mut session,
            ActionId::EnterEndingPrelude(crate::model::EndingPreludeId::AloneDoor),
        );
        assert!(session
            .state
            .has_completed_ending_prelude(crate::model::EndingPreludeId::AloneDoor));
        assert!(session
            .latest()
            .is_some_and(|event| event.tags.iter().any(|tag| tag == "终局前场景")));
        let response_actions = scene_actions(&session.state);
        assert!(response_actions.iter().any(|action| {
            matches!(
                action.id,
                ActionId::AnswerEndingPrelude(
                    crate::model::EndingPreludeId::AloneDoor,
                    crate::model::EndingPreludeResponseId::AcceptCost
                )
            )
        }));
        apply(&mut session, ActionId::BoardAlone);
        assert!(session.state.ended.is_none());
        assert!(session
            .latest()
            .is_some_and(|event| event.title.contains("还差最后一句回答")));
        apply(
            &mut session,
            ActionId::AnswerEndingPrelude(
                crate::model::EndingPreludeId::AloneDoor,
                crate::model::EndingPreludeResponseId::AcceptCost,
            ),
        );
        assert!(session
            .state
            .has_answered_ending_prelude(crate::model::EndingPreludeId::AloneDoor));
        apply(&mut session, ActionId::BoardAlone);
        assert!(session.state.ended.is_none());
        assert!(session
            .latest()
            .is_some_and(|event| event.title.contains("还差一次争辩")));
        apply(
            &mut session,
            ActionId::AnswerFinalDebate(
                crate::model::FinalDebateId::AloneTraveler,
                crate::model::FinalDebateResponseId::AdmitWound,
            ),
        );
        assert!(session
            .state
            .has_answered_final_debate(crate::model::FinalDebateId::AloneTraveler));
        apply(&mut session, ActionId::BoardAlone);
        assert_eq!(session.state.ended, Some(Ending::EscapedAlone));
    }

    #[test]
    fn ending_event_reflects_optional_progress_and_dialogue_tone() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::FinalTrainArrived);
        session.state.remember(Flag::ChildJoined);
        session.state.remember(Flag::SynthesizedChildTruth);
        session.state.remember(Flag::ExaminedTicket);
        session.state.remember(Flag::RecoveredName);
        session.state.remember(Flag::UnderstoodStationMechanism);
        session.state.dialogue_tone = DialogueTone::Gentle;
        session.state.answer_dialogue_challenge(
            crate::model::DialogueChallengeId::ChildAsksIfYouWillLeaveAgain,
            crate::model::DialogueChallengeResponseId::Promise,
        );
        for request in [
            crate::model::StationRequestId::NewspaperCorrection,
            crate::model::StationRequestId::RefundLedger,
            crate::model::StationRequestId::HomeworkEnvelope,
        ] {
            session.state.complete_request(request);
        }
        for case_file in [
            crate::model::CaseFileId::WetTicketProtocol,
            crate::model::CaseFileId::ReturnProtocol,
            crate::model::CaseFileId::ChildWitness,
        ] {
            session.state.resolve_case_file(case_file);
        }
        session
            .state
            .make_vow(crate::model::VowId::ReadTheWholeWarning);
        session
            .state
            .make_vow(crate::model::VowId::DoNotOwnTheChild);
        for memory in [
            crate::model::MemoryId::SeventhBench,
            crate::model::MemoryId::RaincoatPocket,
            crate::model::MemoryId::WhiteLineMeasure,
        ] {
            session.state.visit_memory(memory);
        }
        for patrol in [
            crate::model::PatrolId::WaitingHallManifest,
            crate::model::PatrolId::LostFoundShelfAudit,
            crate::model::PatrolId::PlatformBoundary,
        ] {
            session.state.complete_patrol(patrol);
        }
        for aftertalk in [
            crate::model::AftertalkId::TravelerSecondSeat,
            crate::model::AftertalkId::LostFoundNamedShelf,
            crate::model::AftertalkId::ChildRedrawnLine,
        ] {
            session.state.complete_aftertalk(aftertalk);
        }
        for talk in [
            crate::model::CompanionTalkId::WaitingHallEmptySeat,
            crate::model::CompanionTalkId::LostFoundNamedBox,
            crate::model::CompanionTalkId::PlatformWhiteLineTogether,
        ] {
            session.state.complete_companion_talk(talk);
        }
        for focus in [
            crate::model::LampFocusId::WaitingHallBenchTrace,
            crate::model::LampFocusId::LostFoundLabelShadow,
            crate::model::LampFocusId::PlatformBrakeLight,
        ] {
            session.state.focus_lamp_trace(focus);
        }
        for anomaly in [
            crate::model::AnomalyId::ScreenKeepsScore,
            crate::model::AnomalyId::WhiteLineDrift,
            crate::model::AnomalyId::BrakeLightTrial,
        ] {
            session
                .state
                .resolve_anomaly(anomaly, crate::model::AnomalyResponse::Stabilize);
        }
        session
            .state
            .prepare_departure(crate::model::DepartureId::ChildWindowSeat);
        session
            .state
            .rehearse_departure(crate::model::DepartureId::ChildWindowSeat);
        session
            .state
            .mitigate_route_cost(crate::model::RouteCostId::ChildUnforgivenTomorrow);
        session
            .state
            .complete_route_echo(crate::model::RouteCostId::ChildUnforgivenTomorrow);
        for whisper in [
            crate::model::StationWhisperId::WaitingHallUmbrellaCount,
            crate::model::StationWhisperId::TicketOfficeStampHumidity,
            crate::model::StationWhisperId::LostFoundUmbrellaNames,
            crate::model::StationWhisperId::PlatformBrakeDust,
        ] {
            session.state.hear_station_whisper(whisper);
        }

        apply(
            &mut session,
            ActionId::EnterEndingPrelude(crate::model::EndingPreludeId::ChildWhiteLine),
        );
        apply(
            &mut session,
            ActionId::AnswerEndingPrelude(
                crate::model::EndingPreludeId::ChildWhiteLine,
                crate::model::EndingPreludeResponseId::ReturnChoice,
            ),
        );
        apply(
            &mut session,
            ActionId::AnswerFinalDebate(
                crate::model::FinalDebateId::ChildWhiteLine,
                crate::model::FinalDebateResponseId::RewritePromise,
            ),
        );
        apply(&mut session, ActionId::BoardWithChild);
        assert_eq!(session.state.ended, Some(Ending::TookChildHome));
        let ending = session.latest().expect("ending event should be logged");
        assert!(ending.body.contains("你在这一夜确认了"));
        assert!(ending.body.contains("这些人没有在结局里消失"));
        assert!(ending.body.contains("带孩子返程以后"));
        assert!(ending.body.contains("NPC 的反问"));
        assert!(ending.body.contains("最终选择前"));
        assert!(ending.body.contains("他自己跨过白线"));
        assert!(ending.body.contains("终局不是没人反对的按钮"));
        assert!(ending.body.contains("允许他不听"));
        assert!(ending.body.contains("【余波：第二天】"));
        assert!(ending.body.contains("可以不听命令的房间"));
        assert!(ending.body.contains("【余波：仍未归档】"));
        assert!(ending.body.contains("没有替别人回答"));
        assert!(ending.body.contains("作业本页角"));
        assert!(ending.body.contains("办完的委托"));
        assert!(ending.body.contains("内心锚点"));
        assert!(ending.body.contains("记忆回廊"));
        assert!(ending.body.contains("巡夜记录"));
        assert!(ending.body.contains("回访对话"));
        assert!(ending.body.contains("同行对话"));
        assert!(ending.body.contains("雾灯照证"));
        assert!(ending.body.contains("车站异象"));
        assert!(ending.body.contains("保留 07B"));
        assert!(ending.body.contains("路线试炼"));
        assert!(ending.body.contains("带走的不是听话"));
        assert!(ending.body.contains("自由对话"));
        assert!(ending.body.contains("站内低语"));
        assert!(ending.tags.iter().any(|tag| tag == "委托余波"));
        assert!(ending.tags.iter().any(|tag| tag == "档案余波"));
        assert!(ending.tags.iter().any(|tag| tag == "锚点余波"));
        assert!(ending.tags.iter().any(|tag| tag == "记忆余波"));
        assert!(ending.tags.iter().any(|tag| tag == "巡夜余波"));
        assert!(ending.tags.iter().any(|tag| tag == "回访余波"));
        assert!(ending.tags.iter().any(|tag| tag == "同行余波"));
        assert!(ending.tags.iter().any(|tag| tag == "照证余波"));
        assert!(ending.tags.iter().any(|tag| tag == "异象余波"));
        assert!(ending.tags.iter().any(|tag| tag == "路线准备"));
        assert!(ending.tags.iter().any(|tag| tag == "路线试炼"));
        assert!(ending.tags.iter().any(|tag| tag == "路线代价"));
        assert!(ending.tags.iter().any(|tag| tag == "路线回声"));
        assert!(ending.tags.iter().any(|tag| tag == "低语余波"));
        assert!(ending.tags.iter().any(|tag| tag == "反问余波"));
        assert!(ending.tags.iter().any(|tag| tag == "终局前场景"));
        assert!(ending.tags.iter().any(|tag| tag == "终局回应：交还选择"));
        assert!(ending.tags.iter().any(|tag| tag == "终局争辩：改写承诺"));
        assert!(ending.tags.iter().any(|tag| tag == "结局余波"));
        assert!(ending.tags.iter().any(|tag| tag == "语气：把问题放轻"));
    }

    #[test]
    fn ending_archive_tracks_seen_endings_across_restart() {
        let mut session = GameSession::new();
        start(&mut session);
        session.state.remember(Flag::FinalTrainArrived);
        session.state.remember(Flag::RecoveredName);
        session.state.ticket = TicketKind::Return;

        apply(
            &mut session,
            ActionId::EnterEndingPrelude(crate::model::EndingPreludeId::AloneDoor),
        );
        apply(
            &mut session,
            ActionId::AnswerEndingPrelude(
                crate::model::EndingPreludeId::AloneDoor,
                crate::model::EndingPreludeResponseId::AcceptCost,
            ),
        );
        apply(
            &mut session,
            ActionId::AnswerFinalDebate(
                crate::model::FinalDebateId::AloneTraveler,
                crate::model::FinalDebateResponseId::AdmitWound,
            ),
        );
        apply(&mut session, ActionId::BoardAlone);
        assert!(session.endings_seen.contains(&Ending::EscapedAlone));
        let archived = crate::content::ending_archive_lines(&session);
        assert!(archived.iter().any(|line| line.contains("1 / 6")));
        assert!(archived.iter().any(|line| line.contains("单程逃离")));

        restart(&mut session);
        assert_eq!(session.mode, GameMode::Playing);
        assert!(session.state.ended.is_none());
        assert!(session.endings_seen.contains(&Ending::EscapedAlone));
        let after_restart = crate::content::ending_archive_lines(&session);
        assert!(after_restart
            .iter()
            .any(|line| line.contains("已见：单程逃离")));
    }

    #[test]
    fn run_recap_summarizes_finished_playthrough() {
        let mut state = GameState::new();
        state.actions_used = 80;
        state.ended = Some(Ending::BecameTheVoice);
        state.dialogue_tone = DialogueTone::Direct;
        state.location_depths = [3, 4, 5, 6, 7, 8];
        state.traveler_depth = 4;
        state.clerk_depth = 3;
        state.child_depth = 2;
        state.keeper_depth = 5;
        state.discuss(crate::model::TopicId::TravelerRain);
        state.present(crate::model::EvidenceId::KeeperBroadcastTape);
        state.complete_request(crate::model::StationRequestId::NewspaperCorrection);
        state.resolve_case_file(crate::model::CaseFileId::BroadcastDoor);
        state.resolve_resonance(crate::model::ResonanceId::BroadcastAfterimage);
        state.make_vow(crate::model::VowId::TruthBeforeMercy);
        state.visit_memory(crate::model::MemoryId::BroadcastPractice);
        state.complete_patrol(crate::model::PatrolId::ClockTowerMinuteHand);
        state.complete_aftertalk(crate::model::AftertalkId::KeeperBroadcastReply);
        state.complete_companion_talk(crate::model::CompanionTalkId::ClockTowerBorrowedMinute);
        state.focus_lamp_trace(crate::model::LampFocusId::ClockTowerMinuteDebt);
        state.hear_station_whisper(crate::model::StationWhisperId::ClockTowerGearPrayer);
        state.resolve_anomaly(
            crate::model::AnomalyId::BroadcastFeedback,
            crate::model::AnomalyResponse::Follow,
        );
        state.prepare_departure(crate::model::DepartureId::BroadcastScript);
        state.rehearse_departure(crate::model::DepartureId::BroadcastScript);
        state.mitigate_route_cost(crate::model::RouteCostId::BroadcastSecondName);
        state.complete_route_echo(crate::model::RouteCostId::BroadcastSecondName);
        state.complete_ending_prelude(crate::model::EndingPreludeId::BroadcastBooth);
        state.answer_ending_prelude(
            crate::model::EndingPreludeId::BroadcastBooth,
            crate::model::EndingPreludeResponseId::RefuseControl,
        );
        state.answer_final_debate(
            crate::model::FinalDebateId::BroadcastVoice,
            crate::model::FinalDebateResponseId::RewritePromise,
        );

        let recap = crate::content::run_recap(&state);
        assert!(recap
            .iter()
            .any(|line| line.label == "结局" && line.value == "广播员"));
        assert!(recap
            .iter()
            .any(|line| line.label == "时间" && line.value == "80 / 240 分钟"));
        assert!(recap
            .iter()
            .any(|line| line.label == "旅客委托" && line.value == "1 / 5"));
        assert!(recap
            .iter()
            .any(|line| line.label == "站内档案" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "人物共鸣" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "车站异象" && line.value == "1 / 7"));
        assert!(recap
            .iter()
            .any(|line| line.label == "内心锚点" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "记忆回廊" && line.value == "1 / 8"));
        assert!(recap
            .iter()
            .any(|line| line.label == "巡夜记录" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "回访对话" && line.value == "1 / 9"));
        assert!(recap
            .iter()
            .any(|line| line.label == "同行对话" && line.value == "1 / 12"));
        assert!(recap
            .iter()
            .any(|line| line.label == "雾灯照证" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "站内低语" && line.value == "1 / 12"));
        assert!(recap
            .iter()
            .any(|line| line.label == "路线准备" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "路线试炼" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "路线代价" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "路线回声" && line.value == "1 / 6"));
        assert!(recap
            .iter()
            .any(|line| line.label == "终局前场景" && line.value == "1 / 5"));
        assert!(recap
            .iter()
            .any(|line| line.label == "终局前场景" && line.detail.contains("最后回应 1 次")));
        assert!(recap
            .iter()
            .any(|line| line.label == "终局争辩" && line.value == "1 / 5"));
        assert!(recap
            .iter()
            .any(|line| line.label == "结局余波" && line.value == "4 / 4"));
        assert!(recap
            .iter()
            .any(|line| line.label == "说话方式" && line.value == "直接逼近真相"));
    }

    #[test]
    fn route_summary_tracks_ending_readiness() {
        let mut state = GameState::new();
        let early_routes = crate::content::route_summaries(&state);
        let child_route = early_routes
            .iter()
            .find(|route| route.title == "结局：白线后的空位")
            .expect("hidden child route should be listed as a mystery");
        assert!(child_route.progress < 100);
        assert!(child_route.detail.contains("轮廓"));

        state.remember(Flag::MetChild);
        state.remember(Flag::UnderstoodChildPromise);
        state.remember(Flag::ChildJoined);
        state.remember(Flag::SynthesizedChildTruth);
        let ready_routes = crate::content::route_summaries(&state);
        let child_route = ready_routes
            .iter()
            .find(|route| route.title == Ending::TookChildHome.title())
            .expect("child route should be listed");
        assert_eq!(child_route.progress, 100);
        assert_eq!(child_route.status, "可选择");

        state.ended = Some(Ending::TookChildHome);
        let ended_routes = crate::content::route_summaries(&state);
        let child_route = ended_routes
            .iter()
            .find(|route| route.title == Ending::TookChildHome.title())
            .expect("child route should be listed");
        assert_eq!(child_route.status, "已抵达");
    }

    #[test]
    fn relationship_summary_tracks_dialogue_progress() {
        let mut session = GameSession::new();
        start(&mut session);
        let opening = crate::content::relationship_summaries(&session.state);
        let traveler = opening
            .iter()
            .find(|relationship| relationship.name == "候车厅老人")
            .expect("traveler relationship should be listed");
        assert_eq!(traveler.status, "陌生");
        assert_eq!(traveler.progress, 0);

        apply(&mut session, ActionId::TalkTraveler);
        apply(&mut session, ActionId::TalkTraveler);
        let after_talk = crate::content::relationship_summaries(&session.state);
        let traveler = after_talk
            .iter()
            .find(|relationship| relationship.name == "候车厅老人")
            .expect("traveler relationship should be listed");
        assert_eq!(traveler.status, "试探");
        assert!(traveler.progress > 0);

        apply(&mut session, ActionId::Move(Location::Platform));
        apply(&mut session, ActionId::MeetChild);
        let after_child = crate::content::relationship_summaries(&session.state);
        let child = after_child
            .iter()
            .find(|relationship| relationship.name == "白线后的孩子")
            .expect("child relationship should be listed");
        assert_eq!(child.status, "戒备");
        assert!(child.progress > 0);
    }

    #[test]
    fn layered_child_route_can_reach_child_ending() {
        let mut session = GameSession::new();
        start(&mut session);
        for action in [
            ActionId::TalkTraveler,
            ActionId::Move(Location::WaitingHall),
            ActionId::InvestigateLocation,
            ActionId::InvestigateLocation,
            ActionId::ExamineTicket,
            ActionId::Move(Location::LostAndFound),
            ActionId::SearchLostFound,
            ActionId::OpenCabinet,
            ActionId::Move(Location::Underpass),
            ActionId::ListenUnderpass,
            ActionId::Move(Location::Platform),
            ActionId::MeetChild,
            ActionId::MeetChild,
            ActionId::MeetChild,
            ActionId::MeetChild,
            ActionId::MeetChild,
            ActionId::StudyTicket,
            ActionId::ReturnNameTag,
            ActionId::Move(Location::TicketOffice),
            ActionId::InvestigateLocation,
            ActionId::InvestigateLocation,
            ActionId::TalkClerk,
            ActionId::TalkClerk,
            ActionId::TalkClerk,
            ActionId::RewriteTicket,
        ] {
            apply(&mut session, action);
        }
        while !session.state.final_train_due() {
            apply(&mut session, ActionId::Wait);
        }
        apply(
            &mut session,
            ActionId::EnterEndingPrelude(crate::model::EndingPreludeId::ChildWhiteLine),
        );
        apply(
            &mut session,
            ActionId::AnswerEndingPrelude(
                crate::model::EndingPreludeId::ChildWhiteLine,
                crate::model::EndingPreludeResponseId::ReturnChoice,
            ),
        );
        apply(
            &mut session,
            ActionId::AnswerFinalDebate(
                crate::model::FinalDebateId::ChildWhiteLine,
                crate::model::FinalDebateResponseId::RewritePromise,
            ),
        );
        apply(&mut session, ActionId::BoardWithChild);
        assert_eq!(session.state.ended, Some(Ending::TookChildHome));
    }
}
