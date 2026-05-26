use sky_engine::ui::neo::widgets;
use sky_engine::ui::neo::{Binding, NeoState, Ui};

use crate::aftertalk;
use crate::anomaly;
use crate::case_dialogue;
use crate::case_file;
use crate::companion;
use crate::content;
use crate::departure;
use crate::dialogue_challenge;
use crate::dialogue_lead;
use crate::dialogue_question;
use crate::dialogue_relay;
use crate::dialogue_system;
use crate::final_interview;
use crate::lamp_focus;
use crate::memory;
use crate::model::{
    DialogueChoiceId, DialogueTranscriptEntry, GameSession, InfoPanel, Item, StoryEvent,
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
use crate::theme::{self, AppTheme};
use crate::trial;
use crate::truth;
use crate::view::components;
use crate::vow;

const SCROLLBAR_RESERVE: f32 = 20.0;
const LOG_GAP: f32 = 12.0;
const LOG_OVERSCAN: f32 = 180.0;

pub fn draw(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    ui.stack("right.info").size(width, height).content(|ui| {
        components::panel(ui, "right.info.bg", width, height, app_theme);
        ui.column("right.info.content")
            .x(18.0)
            .y(18.0)
            .size(width - 36.0, height - 36.0)
            .gap(12.0)
            .content(|ui| {
                draw_info_tabs(ui, width - 36.0, state, session, app_theme);
                let body_h = (height - 84.0).max(80.0);
                match session.active_panel {
                    InfoPanel::Intel => {
                        draw_intel(ui, width - 36.0, body_h, state, session, app_theme)
                    }
                    InfoPanel::Routes => {
                        draw_routes(ui, width - 36.0, body_h, state, session, app_theme)
                    }
                    InfoPanel::Cases => {
                        draw_cases(ui, width - 36.0, body_h, state, session, app_theme)
                    }
                    InfoPanel::Inventory => {
                        draw_inventory(ui, width - 36.0, body_h, state, session, app_theme)
                    }
                    InfoPanel::Log => draw_log(ui, width - 36.0, body_h, state, session, app_theme),
                }
            });
    });
}

fn draw_info_tabs(
    ui: &mut Ui,
    width: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let selected = match session.active_panel {
        InfoPanel::Intel => 0,
        InfoPanel::Routes => 1,
        InfoPanel::Cases => 2,
        InfoPanel::Inventory => 3,
        InfoPanel::Log => 4,
    };
    let tab_state = state.clone();
    widgets::segmented(ui, "right.info.tabs")
        .size(width, 36.0)
        .items(["线索", "路线", "档案", "物品", "日志"])
        .selected(selected)
        .theme(app_theme.tokens)
        .on_change(move |index| {
            tab_state.update(|session| {
                session.active_panel = match index {
                    1 => InfoPanel::Routes,
                    2 => InfoPanel::Cases,
                    3 => InfoPanel::Inventory,
                    4 => InfoPanel::Log,
                    _ => InfoPanel::Intel,
                };
            });
        })
        .build();
}

fn draw_intel(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let lines = content::condition_summary(&session.state);
    let objectives = content::objective_summary(&session.state);
    let anomalies = anomaly::anomaly_summaries(&session.state);
    let requests = station_request::request_summaries(&session.state);
    let resonances = resonance::resonance_summaries(&session.state);
    let vows = vow::vow_summaries(&session.state);
    let memories = memory::memory_summaries(&session.state);
    let patrols = patrol::patrol_summaries(&session.state);
    let dialogue_leads = dialogue_lead::lead_summaries(&session.state);
    let dialogue_relays = dialogue_relay::relay_summaries(&session.state);
    let dialogue_questions = dialogue_question::question_summaries(&session.state);
    let dialogue_challenges = dialogue_challenge::challenge_summaries(&session.state);
    let aftertalks = aftertalk::aftertalk_summaries(&session.state);
    let companions = companion::companion_summaries(&session.state);
    let route_echoes = route_echo::echo_summaries(&session.state);
    let final_interviews = final_interview::interview_summaries(&session.state);
    let truths = truth::truth_summaries(&session.state);
    let lamp_focuses = lamp_focus::focus_summaries(&session.state);
    let station_whispers = station_whisper::whisper_summaries(&session.state);
    let dialogue_threads = dialogue_system::dialogue_thread_summaries(&session.state);
    let relationships = content::relationship_summaries(&session.state);
    let content_w = scroll_content_width(width);
    let location = content::location_description(&session.state);
    let location_h = wrapped_height(&location, content_w, 14.0, 90.0);
    let objective_heights = objectives
        .iter()
        .map(|line| wrapped_height(line, content_w - 24.0, 12.0, 32.0) + 14.0)
        .collect::<Vec<_>>();
    let objectives_h = if objectives.is_empty() {
        0.0
    } else {
        30.0 + objective_heights.iter().sum::<f32>() + objectives.len() as f32 * 10.0
    };
    let anomaly_heights = anomalies
        .iter()
        .map(|anomaly| anomaly_card_height(anomaly, content_w))
        .collect::<Vec<_>>();
    let anomalies_h = if anomalies.is_empty() {
        0.0
    } else {
        30.0 + anomaly_heights.iter().sum::<f32>() + anomalies.len().saturating_sub(1) as f32 * 10.0
    };
    let request_heights = requests
        .iter()
        .map(|request| request_card_height(request, content_w))
        .collect::<Vec<_>>();
    let requests_h = if requests.is_empty() {
        0.0
    } else {
        30.0 + request_heights.iter().sum::<f32>() + requests.len().saturating_sub(1) as f32 * 10.0
    };
    let resonance_heights = resonances
        .iter()
        .map(|resonance| resonance_card_height(resonance, content_w))
        .collect::<Vec<_>>();
    let resonances_h = if resonances.is_empty() {
        0.0
    } else {
        30.0 + resonance_heights.iter().sum::<f32>()
            + resonances.len().saturating_sub(1) as f32 * 10.0
    };
    let vow_heights = vows
        .iter()
        .map(|vow| vow_card_height(vow, content_w))
        .collect::<Vec<_>>();
    let vows_h = if vows.is_empty() {
        0.0
    } else {
        30.0 + vow_heights.iter().sum::<f32>() + vows.len().saturating_sub(1) as f32 * 10.0
    };
    let memory_heights = memories
        .iter()
        .map(|memory| memory_card_height(memory, content_w))
        .collect::<Vec<_>>();
    let memories_h = if memories.is_empty() {
        0.0
    } else {
        30.0 + memory_heights.iter().sum::<f32>() + memories.len().saturating_sub(1) as f32 * 10.0
    };
    let patrol_heights = patrols
        .iter()
        .map(|patrol| patrol_card_height(patrol, content_w))
        .collect::<Vec<_>>();
    let patrols_h = if patrols.is_empty() {
        0.0
    } else {
        30.0 + patrol_heights.iter().sum::<f32>() + patrols.len().saturating_sub(1) as f32 * 10.0
    };
    let dialogue_lead_heights = dialogue_leads
        .iter()
        .map(|lead| dialogue_lead_card_height(lead, content_w))
        .collect::<Vec<_>>();
    let dialogue_leads_h = if dialogue_leads.is_empty() {
        0.0
    } else {
        30.0 + dialogue_lead_heights.iter().sum::<f32>()
            + dialogue_leads.len().saturating_sub(1) as f32 * 10.0
    };
    let dialogue_relay_heights = dialogue_relays
        .iter()
        .map(|relay| dialogue_relay_card_height(relay, content_w))
        .collect::<Vec<_>>();
    let dialogue_relays_h = if dialogue_relays.is_empty() {
        0.0
    } else {
        30.0 + dialogue_relay_heights.iter().sum::<f32>()
            + dialogue_relays.len().saturating_sub(1) as f32 * 10.0
    };
    let dialogue_question_heights = dialogue_questions
        .iter()
        .map(|question| dialogue_question_card_height(question, content_w))
        .collect::<Vec<_>>();
    let dialogue_questions_h = if dialogue_questions.is_empty() {
        0.0
    } else {
        30.0 + dialogue_question_heights.iter().sum::<f32>()
            + dialogue_questions.len().saturating_sub(1) as f32 * 10.0
    };
    let dialogue_challenge_heights = dialogue_challenges
        .iter()
        .map(|challenge| dialogue_challenge_card_height(challenge, content_w))
        .collect::<Vec<_>>();
    let dialogue_challenges_h = if dialogue_challenges.is_empty() {
        0.0
    } else {
        30.0 + dialogue_challenge_heights.iter().sum::<f32>()
            + dialogue_challenges.len().saturating_sub(1) as f32 * 10.0
    };
    let aftertalk_heights = aftertalks
        .iter()
        .map(|aftertalk| aftertalk_card_height(aftertalk, content_w))
        .collect::<Vec<_>>();
    let aftertalks_h = if aftertalks.is_empty() {
        0.0
    } else {
        30.0 + aftertalk_heights.iter().sum::<f32>()
            + aftertalks.len().saturating_sub(1) as f32 * 10.0
    };
    let companion_heights = companions
        .iter()
        .map(|companion| companion_card_height(companion, content_w))
        .collect::<Vec<_>>();
    let companions_h = if companions.is_empty() {
        0.0
    } else {
        30.0 + companion_heights.iter().sum::<f32>()
            + companions.len().saturating_sub(1) as f32 * 10.0
    };
    let route_echo_heights = route_echoes
        .iter()
        .map(|echo| route_echo_card_height(echo, content_w))
        .collect::<Vec<_>>();
    let route_echoes_h = if route_echoes.is_empty() {
        0.0
    } else {
        30.0 + route_echo_heights.iter().sum::<f32>()
            + route_echoes.len().saturating_sub(1) as f32 * 10.0
    };
    let final_interview_heights = final_interviews
        .iter()
        .map(|interview| final_interview_card_height(interview, content_w))
        .collect::<Vec<_>>();
    let final_interviews_h = if final_interviews.is_empty() {
        0.0
    } else {
        30.0 + final_interview_heights.iter().sum::<f32>()
            + final_interviews.len().saturating_sub(1) as f32 * 10.0
    };
    let truth_heights = truths
        .iter()
        .map(|truth| truth_card_height(truth, content_w))
        .collect::<Vec<_>>();
    let truths_h = if truths.is_empty() {
        0.0
    } else {
        30.0 + truth_heights.iter().sum::<f32>() + truths.len().saturating_sub(1) as f32 * 10.0
    };
    let lamp_focus_heights = lamp_focuses
        .iter()
        .map(|focus| lamp_focus_card_height(focus, content_w))
        .collect::<Vec<_>>();
    let lamp_focuses_h = if lamp_focuses.is_empty() {
        0.0
    } else {
        30.0 + lamp_focus_heights.iter().sum::<f32>()
            + lamp_focuses.len().saturating_sub(1) as f32 * 10.0
    };
    let station_whisper_heights = station_whispers
        .iter()
        .map(|whisper| station_whisper_card_height(whisper, content_w))
        .collect::<Vec<_>>();
    let station_whispers_h = if station_whispers.is_empty() {
        0.0
    } else {
        30.0 + station_whisper_heights.iter().sum::<f32>()
            + station_whispers.len().saturating_sub(1) as f32 * 10.0
    };
    let dialogue_thread_heights = dialogue_threads
        .iter()
        .map(|thread| dialogue_thread_card_height(thread, content_w))
        .collect::<Vec<_>>();
    let dialogue_threads_h = if dialogue_threads.is_empty() {
        0.0
    } else {
        30.0 + dialogue_thread_heights.iter().sum::<f32>()
            + dialogue_threads.len().saturating_sub(1) as f32 * 10.0
    };
    let relationship_heights = relationships
        .iter()
        .map(|relationship| relationship_card_height(relationship, content_w))
        .collect::<Vec<_>>();
    let relationships_h = if relationships.is_empty() {
        0.0
    } else {
        30.0 + relationship_heights.iter().sum::<f32>()
            + relationships.len().saturating_sub(1) as f32 * 10.0
    };
    let content_h = stacked_height(lines.len(), 28.0, 10.0)
        + 12.0
        + objectives_h
        + 12.0
        + anomalies_h
        + 12.0
        + requests_h
        + 12.0
        + resonances_h
        + 12.0
        + vows_h
        + 12.0
        + memories_h
        + 12.0
        + patrols_h
        + 12.0
        + dialogue_leads_h
        + 12.0
        + dialogue_relays_h
        + 12.0
        + dialogue_questions_h
        + 12.0
        + dialogue_challenges_h
        + 12.0
        + aftertalks_h
        + 12.0
        + companions_h
        + 12.0
        + route_echoes_h
        + 12.0
        + final_interviews_h
        + 12.0
        + truths_h
        + 12.0
        + lamp_focuses_h
        + 12.0
        + station_whispers_h
        + 12.0
        + dialogue_threads_h
        + 12.0
        + relationships_h
        + 12.0
        + location_h;
    let scroll_offset = panel_scroll_offset(session, InfoPanel::Intel, content_h, height);

    ui.scroll_y("right.info.intel")
        .size(width, height)
        .content_height(content_h.max(height))
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_bind(bind_panel_scroll(state, InfoPanel::Intel))
        .content(|ui| {
            let mut list =
                components::VirtualList::new("right.info.intel.virtual", scroll_offset, height);
            for (index, line) in lines.iter().enumerate() {
                let row_h = 28.0 + if index + 1 == lines.len() { 0.0 } else { 10.0 };
                list.row(
                    ui,
                    format!("right.info.intel.condition.row.{index}"),
                    content_w,
                    row_h,
                    |ui| {
                        components::badge(
                            ui,
                            format!("right.info.intel.{index}"),
                            content_w,
                            line,
                            if index % 2 == 0 {
                                app_theme.accent
                            } else {
                                app_theme.accent_warm
                            },
                            app_theme,
                        );
                    },
                );
            }
            list.spacer(12.0);
            if !objectives.is_empty() {
                list.row(
                    ui,
                    "right.info.intel.objectives.label.row",
                    content_w,
                    30.0,
                    |ui| {
                        components::section_label(
                            ui,
                            "right.info.intel.objectives.label",
                            "下一步",
                            content_w,
                        );
                    },
                );
                for (index, (line, height)) in
                    objectives.iter().zip(objective_heights.iter()).enumerate()
                {
                    list.row(
                        ui,
                        format!("right.info.intel.objective.row.{index}"),
                        content_w,
                        *height + 10.0,
                        |ui| draw_objective(ui, content_w, index, line, *height, app_theme),
                    );
                }
            }
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.anomalies",
                "right.info.intel.anomalies.label",
                "车站异象",
                content_w,
                &anomalies,
                &anomaly_heights,
                |ui, index, anomaly, height| {
                    draw_anomaly_card(ui, content_w, index, anomaly, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.requests",
                "right.info.intel.requests.label",
                "旅客委托",
                content_w,
                &requests,
                &request_heights,
                |ui, index, request, height| {
                    draw_request_card(ui, content_w, index, request, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.resonances",
                "right.info.intel.resonances.label",
                "人物共鸣",
                content_w,
                &resonances,
                &resonance_heights,
                |ui, index, resonance, height| {
                    draw_resonance_card(ui, content_w, index, resonance, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.vows",
                "right.info.intel.vows.label",
                "内心锚点",
                content_w,
                &vows,
                &vow_heights,
                |ui, index, vow, height| {
                    draw_vow_card(ui, content_w, index, vow, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.memories",
                "right.info.intel.memories.label",
                "记忆回廊",
                content_w,
                &memories,
                &memory_heights,
                |ui, index, memory, height| {
                    draw_memory_card(ui, content_w, index, memory, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.patrols",
                "right.info.intel.patrols.label",
                "巡夜记录",
                content_w,
                &patrols,
                &patrol_heights,
                |ui, index, patrol, height| {
                    draw_patrol_card(ui, content_w, index, patrol, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.dialogue_leads",
                "right.info.intel.dialogue_leads.label",
                "对话线索",
                content_w,
                &dialogue_leads,
                &dialogue_lead_heights,
                |ui, index, lead, height| {
                    draw_dialogue_lead_card(ui, content_w, index, lead, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.dialogue_relays",
                "right.info.intel.dialogue_relays.label",
                "线索转述",
                content_w,
                &dialogue_relays,
                &dialogue_relay_heights,
                |ui, index, relay, height| {
                    draw_dialogue_relay_card(ui, content_w, index, relay, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.dialogue_questions",
                "right.info.intel.dialogue_questions.label",
                "自由询问",
                content_w,
                &dialogue_questions,
                &dialogue_question_heights,
                |ui, index, question, height| {
                    draw_dialogue_question_card(ui, content_w, index, question, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.dialogue_challenges",
                "right.info.intel.dialogue_challenges.label",
                "NPC 反问",
                content_w,
                &dialogue_challenges,
                &dialogue_challenge_heights,
                |ui, index, challenge, height| {
                    draw_dialogue_challenge_card(ui, content_w, index, challenge, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.aftertalks",
                "right.info.intel.aftertalks.label",
                "回访对话",
                content_w,
                &aftertalks,
                &aftertalk_heights,
                |ui, index, aftertalk, height| {
                    draw_aftertalk_card(ui, content_w, index, aftertalk, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.companions",
                "right.info.intel.companions.label",
                "同行对话",
                content_w,
                &companions,
                &companion_heights,
                |ui, index, companion, height| {
                    draw_companion_card(ui, content_w, index, companion, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.route_echoes",
                "right.info.intel.route_echoes.label",
                "路线回声",
                content_w,
                &route_echoes,
                &route_echo_heights,
                |ui, index, echo, height| {
                    draw_route_echo_card(ui, content_w, index, echo, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.final_interviews",
                "right.info.intel.final_interviews.label",
                "终局前长谈",
                content_w,
                &final_interviews,
                &final_interview_heights,
                |ui, index, interview, height| {
                    draw_final_interview_card(ui, content_w, index, interview, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.truths",
                "right.info.intel.truths.label",
                "中段真相",
                content_w,
                &truths,
                &truth_heights,
                |ui, index, truth, height| {
                    draw_truth_card(ui, content_w, index, truth, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.lamp_focuses",
                "right.info.intel.lamp_focuses.label",
                "雾灯照证",
                content_w,
                &lamp_focuses,
                &lamp_focus_heights,
                |ui, index, focus, height| {
                    draw_lamp_focus_card(ui, content_w, index, focus, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.station_whispers",
                "right.info.intel.station_whispers.label",
                "站内低语",
                content_w,
                &station_whispers,
                &station_whisper_heights,
                |ui, index, whisper, height| {
                    draw_station_whisper_card(ui, content_w, index, whisper, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.dialogue_threads",
                "right.info.intel.dialogue_threads.label",
                "人物话题",
                content_w,
                &dialogue_threads,
                &dialogue_thread_heights,
                |ui, index, thread, height| {
                    draw_dialogue_thread_card(ui, content_w, index, thread, height, app_theme)
                },
            );
            list.spacer(12.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.intel.relationships",
                "right.info.intel.relationships.label",
                "人物关系",
                content_w,
                &relationships,
                &relationship_heights,
                |ui, index, relationship, height| {
                    draw_relationship_card(ui, content_w, index, relationship, height, app_theme)
                },
            );
            list.spacer(12.0);
            list.row(
                ui,
                "right.info.intel.location.row",
                content_w,
                location_h,
                |ui| {
                    components::body_text(
                        ui,
                        "right.info.intel.location",
                        location,
                        content_w,
                        location_h,
                        app_theme.text_soft,
                        14.0,
                    );
                },
            );
            list.finish(ui, content_w);
        });
}

fn request_card_height(request: &station_request::RequestSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&request.detail, width - 24.0, 12.0, 40.0)
}

fn anomaly_card_height(anomaly: &anomaly::AnomalySummary, width: f32) -> f32 {
    68.0 + wrapped_height(&anomaly.detail, width - 24.0, 12.0, 40.0)
}

fn draw_anomaly_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    anomaly: &anomaly::AnomalySummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(anomaly.progress.min(100)) / 100.0;
    let accent = if anomaly.resolved {
        app_theme.accent_warm
    } else if anomaly.ready {
        app_theme.accent
    } else if anomaly.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if anomaly.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.anomaly.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.anomaly.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.anomaly.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 118.0, 20.0)
                .text(anomaly.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if anomaly.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.anomaly.{index}.status"))
                .x(width - 102.0)
                .y(9.0)
                .size(90.0, 20.0)
                .text(anomaly.status.clone())
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.anomaly.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(anomaly.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.anomaly.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.anomaly.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn resonance_card_height(resonance: &resonance::ResonanceSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&resonance.detail, width - 24.0, 12.0, 40.0)
}

fn vow_card_height(vow: &vow::VowSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&vow.detail, width - 24.0, 12.0, 40.0)
}

fn memory_card_height(memory: &memory::MemorySummary, width: f32) -> f32 {
    68.0 + wrapped_height(&memory.detail, width - 24.0, 12.0, 40.0)
}

fn patrol_card_height(patrol: &patrol::PatrolSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&patrol.detail, width - 24.0, 12.0, 40.0)
}

fn dialogue_lead_card_height(lead: &dialogue_lead::DialogueLeadSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&lead.detail, width - 24.0, 12.0, 40.0)
}

fn dialogue_relay_card_height(relay: &dialogue_relay::DialogueRelaySummary, width: f32) -> f32 {
    68.0 + wrapped_height(&relay.detail, width - 24.0, 12.0, 40.0)
}

fn dialogue_question_card_height(
    question: &dialogue_question::DialogueQuestionSummary,
    width: f32,
) -> f32 {
    68.0 + wrapped_height(&question.detail, width - 24.0, 12.0, 40.0)
}

fn dialogue_challenge_card_height(
    challenge: &dialogue_challenge::DialogueChallengeSummary,
    width: f32,
) -> f32 {
    68.0 + wrapped_height(&challenge.detail, width - 24.0, 12.0, 40.0)
}

fn aftertalk_card_height(aftertalk: &aftertalk::AftertalkSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&aftertalk.detail, width - 24.0, 12.0, 40.0)
}

fn companion_card_height(companion: &companion::CompanionSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&companion.detail, width - 24.0, 12.0, 40.0)
}

fn route_echo_card_height(echo: &route_echo::RouteEchoSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&echo.detail, width - 24.0, 12.0, 40.0)
}

fn final_interview_card_height(
    interview: &final_interview::FinalInterviewSummary,
    width: f32,
) -> f32 {
    68.0 + wrapped_height(&interview.detail, width - 24.0, 12.0, 40.0)
}

fn truth_card_height(truth: &truth::TruthSceneSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&truth.detail, width - 24.0, 12.0, 40.0)
}

fn lamp_focus_card_height(focus: &lamp_focus::LampFocusSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&focus.detail, width - 24.0, 12.0, 40.0)
}

fn station_whisper_card_height(
    whisper: &station_whisper::StationWhisperSummary,
    width: f32,
) -> f32 {
    68.0 + wrapped_height(&whisper.detail, width - 24.0, 12.0, 40.0)
}

fn dialogue_thread_card_height(thread: &dialogue_system::DialogueThreadSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&thread.detail, width - 24.0, 12.0, 40.0)
}

fn draw_lamp_focus_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    focus: &lamp_focus::LampFocusSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(focus.progress.min(100)) / 100.0;
    let accent = if focus.focused {
        app_theme.accent_warm
    } else if focus.ready {
        app_theme.accent
    } else if focus.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if focus.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.lamp_focus.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.lamp_focus.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.lamp_focus.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(focus.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if focus.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.lamp_focus.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(focus.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.lamp_focus.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(focus.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.lamp_focus.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.lamp_focus.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_dialogue_thread_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    thread: &dialogue_system::DialogueThreadSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(thread.progress.min(100)) / 100.0;
    let id = format!("right.info.dialogue_thread.{:?}.{index}", thread.dialogue);
    let accent = if thread.progress >= 100 {
        app_theme.accent_warm
    } else if thread.status == "对话中" || thread.status == "可进入" {
        app_theme.accent
    } else {
        app_theme.text_muted
    };

    ui.stack(id.clone()).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(app_theme.panel_alt)
            .border(1.0, app_theme.border)
            .radius(6.0)
            .build();
        ui.text(format!("{id}.title"))
            .x(12.0)
            .y(9.0)
            .size(width - 118.0, 20.0)
            .text(thread.title)
            .font_size(14.0)
            .line_height(18.0)
            .color(app_theme.text)
            .build();
        ui.text(format!("{id}.status"))
            .x(width - 102.0)
            .y(9.0)
            .size(90.0, 20.0)
            .text(thread.status.clone())
            .font_size(12.0)
            .line_height(16.0)
            .color(accent)
            .build();
        ui.text(format!("{id}.detail"))
            .x(12.0)
            .y(34.0)
            .size(width - 24.0, height - 58.0)
            .text(thread.detail.clone())
            .font_size(12.0)
            .line_height(20.0)
            .wrap(true)
            .max_width(width - 24.0)
            .color(app_theme.text_muted)
            .build();
        ui.rect(format!("{id}.track"))
            .x(12.0)
            .y(height - 16.0)
            .size(bar_w, 6.0)
            .color(theme::alpha(app_theme.border, 0.72))
            .radius(3.0)
            .build();
        ui.rect(format!("{id}.fill"))
            .x(12.0)
            .y(height - 16.0)
            .size(fill_w, 6.0)
            .color(accent)
            .radius(3.0)
            .build();
    });
}

fn draw_station_whisper_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    whisper: &station_whisper::StationWhisperSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(whisper.progress.min(100)) / 100.0;
    let accent = if whisper.heard {
        app_theme.accent_warm
    } else if whisper.ready {
        app_theme.accent
    } else if whisper.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if whisper.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.station_whisper.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.station_whisper.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.station_whisper.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(whisper.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if whisper.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.station_whisper.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(whisper.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.station_whisper.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(whisper.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.station_whisper.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.station_whisper.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_companion_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    companion: &companion::CompanionSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(companion.progress.min(100)) / 100.0;
    let accent = if companion.completed {
        app_theme.accent_warm
    } else if companion.ready {
        app_theme.accent
    } else if companion.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if companion.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.companion.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.companion.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.companion.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(companion.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if companion.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.companion.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(companion.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.companion.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(companion.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.companion.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.companion.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_route_echo_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    echo: &route_echo::RouteEchoSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(echo.progress.min(100)) / 100.0;
    let accent = if echo.completed {
        app_theme.accent_warm
    } else if echo.ready {
        app_theme.accent
    } else if echo.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if echo.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.route_echo.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route_echo.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route_echo.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(echo.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if echo.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.route_echo.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(echo.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route_echo.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(echo.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route_echo.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route_echo.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_final_interview_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    interview: &final_interview::FinalInterviewSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(interview.progress.min(100)) / 100.0;
    let accent = if interview.completed {
        app_theme.accent_warm
    } else if interview.ready {
        app_theme.accent
    } else if interview.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if interview.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };
    let id = format!(
        "right.info.final_interview.{:?}.{index}",
        interview.interview
    );

    ui.stack(id.clone()).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(background)
            .border(1.0, app_theme.border)
            .radius(6.0)
            .build();
        ui.text(format!("{id}.title"))
            .x(12.0)
            .y(9.0)
            .size(width - 98.0, 20.0)
            .text(interview.title)
            .font_size(14.0)
            .line_height(18.0)
            .color(if interview.visible {
                app_theme.text
            } else {
                app_theme.text_muted
            })
            .build();
        ui.text(format!("{id}.status"))
            .x(width - 82.0)
            .y(9.0)
            .size(70.0, 20.0)
            .text(interview.status)
            .font_size(12.0)
            .line_height(16.0)
            .color(accent)
            .build();
        ui.text(format!("{id}.detail"))
            .x(12.0)
            .y(34.0)
            .size(width - 24.0, height - 58.0)
            .text(interview.detail.clone())
            .font_size(12.0)
            .line_height(20.0)
            .wrap(true)
            .max_width(width - 24.0)
            .color(app_theme.text_muted)
            .build();
        ui.rect(format!("{id}.track"))
            .x(12.0)
            .y(height - 16.0)
            .size(bar_w, 6.0)
            .color(theme::alpha(app_theme.border, 0.72))
            .radius(3.0)
            .build();
        ui.rect(format!("{id}.fill"))
            .x(12.0)
            .y(height - 16.0)
            .size(fill_w, 6.0)
            .color(accent)
            .radius(3.0)
            .build();
    });
}

fn draw_truth_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    truth: &truth::TruthSceneSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(truth.progress.min(100)) / 100.0;
    let accent = if truth.revealed {
        app_theme.accent_warm
    } else if truth.ready {
        app_theme.accent
    } else if truth.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if truth.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };
    let id = format!("right.info.truth.{:?}.{index}", truth.truth);

    ui.stack(id.clone()).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(background)
            .border(1.0, app_theme.border)
            .radius(6.0)
            .build();
        ui.text(format!("{id}.title"))
            .x(12.0)
            .y(9.0)
            .size(width - 98.0, 20.0)
            .text(truth.title)
            .font_size(14.0)
            .line_height(18.0)
            .color(if truth.visible {
                app_theme.text
            } else {
                app_theme.text_muted
            })
            .build();
        ui.text(format!("{id}.status"))
            .x(width - 82.0)
            .y(9.0)
            .size(70.0, 20.0)
            .text(truth.status)
            .font_size(12.0)
            .line_height(16.0)
            .color(accent)
            .build();
        ui.text(format!("{id}.detail"))
            .x(12.0)
            .y(34.0)
            .size(width - 24.0, height - 58.0)
            .text(truth.detail.clone())
            .font_size(12.0)
            .line_height(20.0)
            .wrap(true)
            .max_width(width - 24.0)
            .color(app_theme.text_muted)
            .build();
        ui.rect(format!("{id}.track"))
            .x(12.0)
            .y(height - 16.0)
            .size(bar_w, 6.0)
            .color(theme::alpha(app_theme.border, 0.72))
            .radius(3.0)
            .build();
        ui.rect(format!("{id}.fill"))
            .x(12.0)
            .y(height - 16.0)
            .size(fill_w, 6.0)
            .color(accent)
            .radius(3.0)
            .build();
    });
}

fn draw_aftertalk_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    aftertalk: &aftertalk::AftertalkSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(aftertalk.progress.min(100)) / 100.0;
    let accent = if aftertalk.completed {
        app_theme.accent_warm
    } else if aftertalk.ready {
        app_theme.accent
    } else if aftertalk.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if aftertalk.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.aftertalk.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.aftertalk.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.aftertalk.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(aftertalk.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if aftertalk.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.aftertalk.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(aftertalk.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.aftertalk.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(aftertalk.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.aftertalk.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.aftertalk.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_dialogue_lead_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    lead: &dialogue_lead::DialogueLeadSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(lead.progress.min(100)) / 100.0;
    let accent = if lead.completed {
        app_theme.accent_warm
    } else if lead.ready {
        app_theme.accent
    } else if lead.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if lead.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.dialogue_lead.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.dialogue_lead.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.dialogue_lead.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(lead.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if lead.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.dialogue_lead.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(lead.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.dialogue_lead.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(lead.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.dialogue_lead.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.dialogue_lead.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_dialogue_relay_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    relay: &dialogue_relay::DialogueRelaySummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(relay.progress.min(100)) / 100.0;
    let accent = if relay.completed {
        app_theme.accent_warm
    } else if relay.ready {
        app_theme.accent
    } else if relay.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if relay.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.dialogue_relay.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.dialogue_relay.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.dialogue_relay.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(relay.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if relay.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.dialogue_relay.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(relay.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.dialogue_relay.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(relay.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.dialogue_relay.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.dialogue_relay.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_dialogue_question_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    question: &dialogue_question::DialogueQuestionSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(question.progress.min(100)) / 100.0;
    let accent = if question.answered {
        app_theme.accent_warm
    } else if question.ready {
        app_theme.accent
    } else if question.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if question.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };
    let id = format!(
        "right.info.dialogue_question.{:?}.{index}",
        question.question
    );

    ui.stack(id.clone()).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(background)
            .border(1.0, app_theme.border)
            .radius(6.0)
            .build();
        ui.text(format!("{id}.title"))
            .x(12.0)
            .y(9.0)
            .size(width - 98.0, 20.0)
            .text(question.title)
            .font_size(14.0)
            .line_height(18.0)
            .color(if question.visible {
                app_theme.text
            } else {
                app_theme.text_muted
            })
            .build();
        ui.text(format!("{id}.status"))
            .x(width - 82.0)
            .y(9.0)
            .size(70.0, 20.0)
            .text(question.status)
            .font_size(12.0)
            .line_height(16.0)
            .color(accent)
            .build();
        ui.text(format!("{id}.detail"))
            .x(12.0)
            .y(34.0)
            .size(width - 24.0, height - 58.0)
            .text(question.detail.clone())
            .font_size(12.0)
            .line_height(20.0)
            .wrap(true)
            .max_width(width - 24.0)
            .color(app_theme.text_muted)
            .build();
        ui.rect(format!("{id}.track"))
            .x(12.0)
            .y(height - 16.0)
            .size(bar_w, 6.0)
            .color(theme::alpha(app_theme.border, 0.72))
            .radius(3.0)
            .build();
        ui.rect(format!("{id}.fill"))
            .x(12.0)
            .y(height - 16.0)
            .size(fill_w, 6.0)
            .color(accent)
            .radius(3.0)
            .build();
    });
}

fn draw_dialogue_challenge_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    challenge: &dialogue_challenge::DialogueChallengeSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(challenge.progress.min(100)) / 100.0;
    let accent = if challenge.answered {
        app_theme.accent_warm
    } else if challenge.ready {
        app_theme.accent
    } else if challenge.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if challenge.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };
    let id = format!(
        "right.info.dialogue_challenge.{:?}.{index}",
        challenge.challenge
    );

    ui.stack(id.clone()).size(width, height).content(|ui| {
        ui.rect(format!("{id}.bg"))
            .size(width, height)
            .color(background)
            .border(1.0, app_theme.border)
            .radius(6.0)
            .build();
        ui.text(format!("{id}.title"))
            .x(12.0)
            .y(9.0)
            .size(width - 98.0, 20.0)
            .text(challenge.title)
            .font_size(14.0)
            .line_height(18.0)
            .color(if challenge.visible {
                app_theme.text
            } else {
                app_theme.text_muted
            })
            .build();
        ui.text(format!("{id}.status"))
            .x(width - 82.0)
            .y(9.0)
            .size(70.0, 20.0)
            .text(challenge.status)
            .font_size(12.0)
            .line_height(16.0)
            .color(accent)
            .build();
        ui.text(format!("{id}.detail"))
            .x(12.0)
            .y(34.0)
            .size(width - 24.0, height - 58.0)
            .text(challenge.detail.clone())
            .font_size(12.0)
            .line_height(20.0)
            .wrap(true)
            .max_width(width - 24.0)
            .color(app_theme.text_muted)
            .build();
        ui.rect(format!("{id}.track"))
            .x(12.0)
            .y(height - 16.0)
            .size(bar_w, 6.0)
            .color(theme::alpha(app_theme.border, 0.72))
            .radius(3.0)
            .build();
        ui.rect(format!("{id}.fill"))
            .x(12.0)
            .y(height - 16.0)
            .size(fill_w, 6.0)
            .color(accent)
            .radius(3.0)
            .build();
    });
}

fn draw_patrol_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    patrol: &patrol::PatrolSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(patrol.progress.min(100)) / 100.0;
    let accent = if patrol.completed {
        app_theme.accent_warm
    } else if patrol.ready {
        app_theme.accent
    } else if patrol.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if patrol.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.patrol.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.patrol.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.patrol.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(patrol.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if patrol.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.patrol.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(patrol.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.patrol.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(patrol.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.patrol.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.patrol.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_memory_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    memory: &memory::MemorySummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(memory.progress.min(100)) / 100.0;
    let accent = if memory.visited {
        app_theme.accent_warm
    } else if memory.ready {
        app_theme.accent
    } else if memory.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if memory.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.memory.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.memory.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.memory.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(memory.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if memory.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.memory.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(memory.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.memory.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(memory.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.memory.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.memory.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_vow_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    vow: &vow::VowSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(vow.progress.min(100)) / 100.0;
    let accent = if vow.chosen {
        app_theme.accent_warm
    } else if vow.ready {
        app_theme.accent
    } else if vow.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if vow.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.vow.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.vow.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.vow.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(vow.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if vow.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.vow.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(vow.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.vow.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(vow.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.vow.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.vow.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_resonance_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    resonance: &resonance::ResonanceSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(resonance.progress.min(100)) / 100.0;
    let accent = if resonance.resolved {
        app_theme.accent_warm
    } else if resonance.ready {
        app_theme.accent
    } else if resonance.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if resonance.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.resonance.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.resonance.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.resonance.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(resonance.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if resonance.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.resonance.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(resonance.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.resonance.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(resonance.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.resonance.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.resonance.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_request_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    request: &station_request::RequestSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(request.progress.min(100)) / 100.0;
    let accent = if request.completed {
        app_theme.accent_warm
    } else if request.ready {
        app_theme.accent
    } else if request.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if request.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.request.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.request.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.request.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(request.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if request.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.request.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(request.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.request.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(request.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.request.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.request.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn relationship_card_height(relationship: &content::RelationshipSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&relationship.detail, width - 24.0, 12.0, 40.0)
}

fn draw_relationship_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    relationship: &content::RelationshipSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(relationship.progress.min(100)) / 100.0;
    let accent = if relationship.progress >= 80 {
        app_theme.accent_warm
    } else {
        app_theme.accent
    };

    ui.stack(format!("right.info.relationship.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.relationship.{index}.bg"))
                .size(width, height)
                .color(app_theme.panel_alt)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.relationship.{index}.name"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(relationship.name)
                .font_size(14.0)
                .line_height(18.0)
                .color(app_theme.text)
                .build();
            ui.text(format!("right.info.relationship.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(relationship.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.relationship.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(relationship.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.relationship.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.relationship.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_objective(
    ui: &mut Ui,
    width: f32,
    index: usize,
    text: &str,
    height: f32,
    app_theme: AppTheme,
) {
    ui.stack(format!("right.info.intel.objective.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.intel.objective.{index}.bg"))
                .size(width, height)
                .color(app_theme.panel_alt)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.intel.objective.{index}.text"))
                .x(12.0)
                .y(7.0)
                .size(width - 24.0, height - 14.0)
                .text(text)
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
        });
}

fn draw_routes(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let content_w = scroll_content_width(width);
    let archive = content::ending_archive_lines(session);
    let chapters = content::chapter_summary(&session.state);
    let departures = departure::departure_summaries(&session.state);
    let trials = trial::trial_summaries(&session.state);
    let route_costs = route_cost::cost_summaries(&session.state);
    let route_pressures = route_pressure::pressure_summaries(&session.state);
    let route_witnesses = route_witness::witness_summaries(&session.state);
    let route_witness_debriefs = route_witness_debrief::debrief_summaries(&session.state);
    let routes = content::route_summaries(&session.state);
    let departure_heights = departures
        .iter()
        .map(|departure| departure_card_height(departure, content_w))
        .collect::<Vec<_>>();
    let route_heights = routes
        .iter()
        .map(|route| route_card_height(route, content_w))
        .collect::<Vec<_>>();
    let trial_heights = trials
        .iter()
        .map(|trial| trial_card_height(trial, content_w))
        .collect::<Vec<_>>();
    let route_cost_heights = route_costs
        .iter()
        .map(|cost| route_cost_card_height(cost, content_w))
        .collect::<Vec<_>>();
    let route_pressure_heights = route_pressures
        .iter()
        .map(|pressure| route_pressure_card_height(pressure, content_w))
        .collect::<Vec<_>>();
    let route_witness_heights = route_witnesses
        .iter()
        .map(|witness| route_witness_card_height(witness, content_w))
        .collect::<Vec<_>>();
    let route_witness_debrief_heights = route_witness_debriefs
        .iter()
        .map(|debrief| route_witness_debrief_card_height(debrief, content_w))
        .collect::<Vec<_>>();
    let content_h = 30.0
        + stacked_height(archive.len(), 28.0, 10.0)
        + 18.0
        + 30.0
        + stacked_height(chapters.len(), 28.0, 10.0)
        + 18.0
        + 30.0
        + departure_heights.iter().sum::<f32>()
        + departures.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + trial_heights.iter().sum::<f32>()
        + trials.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + route_cost_heights.iter().sum::<f32>()
        + route_costs.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + route_pressure_heights.iter().sum::<f32>()
        + route_pressures.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + route_witness_heights.iter().sum::<f32>()
        + route_witnesses.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + route_witness_debrief_heights.iter().sum::<f32>()
        + route_witness_debriefs.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + route_heights.iter().sum::<f32>()
        + routes.len().saturating_sub(1) as f32 * 10.0;
    let scroll_offset = panel_scroll_offset(session, InfoPanel::Routes, content_h, height);

    ui.scroll_y("right.info.routes")
        .size(width, height)
        .content_height(content_h.max(height))
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_bind(bind_panel_scroll(state, InfoPanel::Routes))
        .content(|ui| {
            let mut list =
                components::VirtualList::new("right.info.routes.virtual", scroll_offset, height);
            list.row(
                ui,
                "right.info.routes.archive.label.row",
                content_w,
                30.0,
                |ui| {
                    components::section_label(
                        ui,
                        "right.info.routes.archive.label",
                        "结局档案",
                        content_w,
                    );
                },
            );
            for (index, line) in archive.iter().enumerate() {
                let row_h = 28.0
                    + if index + 1 == archive.len() {
                        0.0
                    } else {
                        10.0
                    };
                list.row(
                    ui,
                    format!("right.info.routes.archive.row.{index}"),
                    content_w,
                    row_h,
                    |ui| {
                        components::badge(
                            ui,
                            format!("right.info.routes.archive.{index}"),
                            content_w,
                            line,
                            if index == 0 {
                                app_theme.accent_warm
                            } else {
                                app_theme.accent
                            },
                            app_theme,
                        );
                    },
                );
            }
            list.spacer(18.0);
            list.row(
                ui,
                "right.info.routes.chapter.label.row",
                content_w,
                30.0,
                |ui| {
                    components::section_label(
                        ui,
                        "right.info.routes.chapter.label",
                        "午夜进度",
                        content_w,
                    );
                },
            );
            for (index, line) in chapters.iter().enumerate() {
                let row_h = 28.0
                    + if index + 1 == chapters.len() {
                        0.0
                    } else {
                        10.0
                    };
                list.row(
                    ui,
                    format!("right.info.routes.chapter.row.{index}"),
                    content_w,
                    row_h,
                    |ui| {
                        components::badge(
                            ui,
                            format!("right.info.routes.chapter.{index}"),
                            content_w,
                            line,
                            if index % 2 == 0 {
                                app_theme.accent
                            } else {
                                app_theme.accent_warm
                            },
                            app_theme,
                        );
                    },
                );
            }
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.departures",
                "right.info.routes.departures.label",
                "路线准备",
                content_w,
                &departures,
                &departure_heights,
                |ui, index, departure, height| {
                    draw_departure_card(ui, content_w, index, departure, height, app_theme)
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.trials",
                "right.info.routes.trials.label",
                "路线试炼",
                content_w,
                &trials,
                &trial_heights,
                |ui, index, trial, height| {
                    draw_trial_card(ui, content_w, index, trial, height, app_theme)
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.costs",
                "right.info.routes.costs.label",
                "路线代价",
                content_w,
                &route_costs,
                &route_cost_heights,
                |ui, index, cost, height| {
                    draw_route_cost_card(ui, content_w, index, cost, height, app_theme)
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.pressures",
                "right.info.routes.pressures.label",
                "路线争论",
                content_w,
                &route_pressures,
                &route_pressure_heights,
                |ui, index, pressure, height| {
                    draw_route_pressure_card(ui, content_w, index, pressure, height, app_theme)
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.witnesses",
                "right.info.routes.witnesses.label",
                "路线现场",
                content_w,
                &route_witnesses,
                &route_witness_heights,
                |ui, index, witness, height| {
                    draw_route_witness_card(ui, content_w, index, witness, height, app_theme)
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.witness_debriefs",
                "right.info.routes.witness_debriefs.label",
                "路线复盘",
                content_w,
                &route_witness_debriefs,
                &route_witness_debrief_heights,
                |ui, index, debrief, height| {
                    draw_route_witness_debrief_card(
                        ui, content_w, index, debrief, height, app_theme,
                    )
                },
            );
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.routes.endings",
                "right.info.routes.endings.label",
                "结局路线",
                content_w,
                &routes,
                &route_heights,
                |ui, index, route, height| {
                    draw_route_card(ui, content_w, index, route, height, app_theme)
                },
            );
            list.finish(ui, content_w);
        });
}

fn departure_card_height(departure: &departure::DepartureSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&departure.detail, width - 24.0, 12.0, 40.0)
}

fn trial_card_height(trial: &trial::TrialSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&trial.detail, width - 24.0, 12.0, 40.0)
}

fn route_cost_card_height(cost: &route_cost::RouteCostSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&cost.detail, width - 24.0, 12.0, 40.0)
}

fn route_pressure_card_height(pressure: &route_pressure::RoutePressureSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&pressure.detail, width - 24.0, 12.0, 40.0)
}

fn route_witness_card_height(witness: &route_witness::RouteWitnessSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&witness.detail, width - 24.0, 12.0, 40.0)
}

fn route_witness_debrief_card_height(
    debrief: &route_witness_debrief::RouteWitnessDebriefSummary,
    width: f32,
) -> f32 {
    68.0 + wrapped_height(&debrief.detail, width - 24.0, 12.0, 40.0)
}

fn route_card_height(route: &content::RouteSummary, width: f32) -> f32 {
    68.0 + wrapped_height(&route.detail, width - 24.0, 12.0, 40.0)
}

fn draw_route_cost_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    cost: &route_cost::RouteCostSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(cost.progress.min(100)) / 100.0;
    let accent = if cost.mitigated {
        app_theme.accent_warm
    } else if cost.ready {
        app_theme.accent
    } else if cost.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if cost.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.route_cost.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route_cost.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route_cost.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(cost.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if cost.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.route_cost.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(cost.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route_cost.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(cost.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route_cost.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route_cost.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_trial_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    trial: &trial::TrialSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(trial.progress.min(100)) / 100.0;
    let accent = if trial.rehearsed {
        app_theme.accent_warm
    } else if trial.ready {
        app_theme.accent
    } else if trial.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if trial.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.trial.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.trial.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.trial.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(trial.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if trial.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.trial.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(trial.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.trial.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(trial.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.trial.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.trial.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_route_pressure_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    pressure: &route_pressure::RoutePressureSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(pressure.progress.min(100)) / 100.0;
    let accent = if pressure.answered {
        app_theme.accent_warm
    } else if pressure.ready {
        app_theme.accent
    } else if pressure.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if pressure.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.route_pressure.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route_pressure.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route_pressure.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(pressure.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if pressure.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.route_pressure.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(pressure.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route_pressure.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(pressure.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route_pressure.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route_pressure.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_route_witness_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    witness: &route_witness::RouteWitnessSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(witness.progress.min(100)) / 100.0;
    let accent = if witness.visited {
        app_theme.accent_warm
    } else if witness.ready {
        app_theme.accent
    } else if witness.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if witness.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.route_witness.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route_witness.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route_witness.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(witness.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if witness.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.route_witness.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(witness.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route_witness.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(witness.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route_witness.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route_witness.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_route_witness_debrief_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    debrief: &route_witness_debrief::RouteWitnessDebriefSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(debrief.progress.min(100)) / 100.0;
    let accent = if debrief.completed {
        app_theme.accent_warm
    } else if debrief.ready {
        app_theme.accent
    } else if debrief.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if debrief.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.route_witness_debrief.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route_witness_debrief.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route_witness_debrief.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(debrief.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if debrief.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.route_witness_debrief.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(debrief.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route_witness_debrief.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(debrief.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route_witness_debrief.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route_witness_debrief.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_departure_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    departure: &departure::DepartureSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(departure.progress.min(100)) / 100.0;
    let accent = if departure.prepared {
        app_theme.accent_warm
    } else if departure.ready {
        app_theme.accent
    } else if departure.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.50)
    };
    let background = if departure.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.70)
    };

    ui.stack(format!("right.info.departure.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.departure.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.departure.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(departure.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if departure.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.departure.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(departure.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.departure.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(departure.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.departure.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.departure.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_route_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    route: &content::RouteSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(route.progress.min(100)) / 100.0;
    let accent = if route.progress >= 100 {
        app_theme.accent_warm
    } else {
        app_theme.accent
    };

    ui.stack(format!("right.info.route.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.route.{index}.bg"))
                .size(width, height)
                .color(app_theme.panel_alt)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.route.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 98.0, 20.0)
                .text(route.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(app_theme.text)
                .build();
            ui.text(format!("right.info.route.{index}.status"))
                .x(width - 82.0)
                .y(9.0)
                .size(70.0, 20.0)
                .text(route.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.route.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(route.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.route.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.route.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_cases(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let content_w = scroll_content_width(width);
    let summaries = case_file::case_file_summaries(&session.state);
    let case_dialogues = case_dialogue::case_dialogue_summaries(&session.state);
    let ready_count = summaries.iter().filter(|summary| summary.ready).count();
    let ready_dialogues = case_dialogues
        .iter()
        .filter(|summary| summary.ready)
        .count();
    let overview = format!(
        "已归档 {} / {} 份，当前可归档 {} 份；档案回谈 {} / {} 份，可回谈 {} 份",
        session.state.resolved_case_files.len(),
        case_file::CASE_FILE_COUNT,
        ready_count,
        session.state.completed_case_dialogues.len(),
        case_dialogue::CASE_DIALOGUE_COUNT,
        ready_dialogues
    );
    let card_heights = summaries
        .iter()
        .map(|summary| case_card_height(summary, content_w))
        .collect::<Vec<_>>();
    let dialogue_heights = case_dialogues
        .iter()
        .map(|summary| case_dialogue_card_height(summary, content_w))
        .collect::<Vec<_>>();
    let content_h = 30.0
        + 28.0
        + 18.0
        + card_heights.iter().sum::<f32>()
        + summaries.len().saturating_sub(1) as f32 * 10.0
        + 18.0
        + 30.0
        + dialogue_heights.iter().sum::<f32>()
        + case_dialogues.len().saturating_sub(1) as f32 * 10.0;
    let scroll_offset = panel_scroll_offset(session, InfoPanel::Cases, content_h, height);

    ui.scroll_y("right.info.cases")
        .size(width, height)
        .content_height(content_h.max(height))
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_bind(bind_panel_scroll(state, InfoPanel::Cases))
        .content(|ui| {
            let mut list =
                components::VirtualList::new("right.info.cases.virtual", scroll_offset, height);
            list.row(ui, "right.info.cases.label.row", content_w, 30.0, |ui| {
                components::section_label(ui, "right.info.cases.label", "站内档案", content_w);
            });
            list.row(ui, "right.info.cases.overview.row", content_w, 28.0, |ui| {
                components::badge(
                    ui,
                    "right.info.cases.overview",
                    content_w,
                    overview,
                    app_theme.accent_warm,
                    app_theme,
                );
            });
            list.spacer(18.0);
            for (index, (summary, height)) in summaries.iter().zip(card_heights.iter()).enumerate()
            {
                let row_h = *height
                    + if index + 1 == summaries.len() {
                        0.0
                    } else {
                        10.0
                    };
                list.row(
                    ui,
                    format!("right.info.cases.case.row.{index}"),
                    content_w,
                    row_h,
                    |ui| draw_case_card(ui, content_w, index, summary, *height, app_theme),
                );
            }
            list.spacer(18.0);
            virtual_section(
                ui,
                &mut list,
                "right.info.cases.dialogue",
                "right.info.cases.dialogue.label",
                "档案回谈",
                content_w,
                &case_dialogues,
                &dialogue_heights,
                |ui, index, summary, height| {
                    draw_case_dialogue_card(ui, content_w, index, summary, height, app_theme)
                },
            );
            list.finish(ui, content_w);
        });
}

fn case_card_height(summary: &case_file::CaseFileSummary, width: f32) -> f32 {
    76.0 + wrapped_height(&summary.detail, width - 24.0, 12.0, 40.0)
}

fn case_dialogue_card_height(summary: &case_dialogue::CaseDialogueSummary, width: f32) -> f32 {
    76.0 + wrapped_height(&summary.detail, width - 24.0, 12.0, 40.0)
}

fn draw_case_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    summary: &case_file::CaseFileSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(summary.progress.min(100)) / 100.0;
    let accent = if summary.resolved {
        app_theme.accent_warm
    } else if summary.ready {
        app_theme.accent
    } else if summary.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.52)
    };
    let background = if summary.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.72)
    };

    ui.stack(format!("right.info.case.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.case.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.case.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 88.0, 20.0)
                .text(summary.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if summary.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.case.{index}.status"))
                .x(width - 72.0)
                .y(9.0)
                .size(60.0, 20.0)
                .text(summary.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.case.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(summary.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.case.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.case.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_case_dialogue_card(
    ui: &mut Ui,
    width: f32,
    index: usize,
    summary: &case_dialogue::CaseDialogueSummary,
    height: f32,
    app_theme: AppTheme,
) {
    let bar_w = (width - 24.0).max(0.0);
    let fill_w = bar_w * f32::from(summary.progress.min(100)) / 100.0;
    let accent = if summary.completed {
        app_theme.accent_warm
    } else if summary.ready {
        app_theme.accent
    } else if summary.visible {
        app_theme.text_muted
    } else {
        theme::alpha(app_theme.text_muted, 0.52)
    };
    let background = if summary.visible {
        app_theme.panel_alt
    } else {
        theme::alpha(app_theme.panel_alt, 0.72)
    };

    ui.stack(format!("right.info.case_dialogue.{index}"))
        .size(width, height)
        .content(|ui| {
            ui.rect(format!("right.info.case_dialogue.{index}.bg"))
                .size(width, height)
                .color(background)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.case_dialogue.{index}.title"))
                .x(12.0)
                .y(9.0)
                .size(width - 88.0, 20.0)
                .text(summary.title)
                .font_size(14.0)
                .line_height(18.0)
                .color(if summary.visible {
                    app_theme.text
                } else {
                    app_theme.text_muted
                })
                .build();
            ui.text(format!("right.info.case_dialogue.{index}.status"))
                .x(width - 72.0)
                .y(9.0)
                .size(60.0, 20.0)
                .text(summary.status)
                .font_size(12.0)
                .line_height(16.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.case_dialogue.{index}.detail"))
                .x(12.0)
                .y(34.0)
                .size(width - 24.0, height - 58.0)
                .text(summary.detail.clone())
                .font_size(12.0)
                .line_height(20.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
            ui.rect(format!("right.info.case_dialogue.{index}.track"))
                .x(12.0)
                .y(height - 16.0)
                .size(bar_w, 6.0)
                .color(theme::alpha(app_theme.border, 0.72))
                .radius(3.0)
                .build();
            ui.rect(format!("right.info.case_dialogue.{index}.fill"))
                .x(12.0)
                .y(height - 16.0)
                .size(fill_w, 6.0)
                .color(accent)
                .radius(3.0)
                .build();
        });
}

fn draw_inventory(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let content_w = scroll_content_width(width);
    let item_count = session.state.inventory.len();
    let content_h = if item_count == 0 {
        80.0
    } else {
        stacked_height(item_count, 74.0, 9.0)
    };
    let scroll_offset = panel_scroll_offset(session, InfoPanel::Inventory, content_h, height);

    ui.scroll_y("right.info.inventory")
        .size(width, height)
        .content_height(content_h.max(height))
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_bind(bind_panel_scroll(state, InfoPanel::Inventory))
        .content(|ui| {
            let mut list =
                components::VirtualList::new("right.info.inventory.virtual", scroll_offset, height);
            if session.state.inventory.is_empty() {
                list.row(
                    ui,
                    "right.info.inventory.empty.row",
                    content_w,
                    80.0,
                    |ui| {
                        components::body_text(
                            ui,
                            "right.info.inventory.empty",
                            "你什么也没有带来。也许这就是车站最喜欢的旅客。",
                            content_w,
                            80.0,
                            app_theme.text_soft,
                            14.0,
                        );
                    },
                );
                list.finish(ui, content_w);
                return;
            }
            for (index, item) in session.state.inventory.iter().copied().enumerate() {
                let row_h = 74.0 + if index + 1 == item_count { 0.0 } else { 9.0 };
                list.row(
                    ui,
                    format!("right.info.inventory.item.row.{index}"),
                    content_w,
                    row_h,
                    |ui| draw_item(ui, content_w, index, item, app_theme),
                );
            }
            list.finish(ui, content_w);
        });
}

fn draw_item(ui: &mut Ui, width: f32, index: usize, item: Item, app_theme: AppTheme) {
    ui.stack(format!("right.info.item.{index}"))
        .size(width, 74.0)
        .content(|ui| {
            ui.rect(format!("right.info.item.{index}.bg"))
                .size(width, 74.0)
                .color(app_theme.panel_alt)
                .border(1.0, app_theme.border)
                .radius(6.0)
                .build();
            ui.text(format!("right.info.item.{index}.name"))
                .x(12.0)
                .y(9.0)
                .size(width - 24.0, 20.0)
                .text(item.name())
                .font_size(15.0)
                .line_height(18.0)
                .color(app_theme.text)
                .build();
            ui.text(format!("right.info.item.{index}.desc"))
                .x(12.0)
                .y(32.0)
                .size(width - 24.0, 36.0)
                .text(content::item_description(item))
                .font_size(12.0)
                .line_height(18.0)
                .wrap(true)
                .max_width(width - 24.0)
                .color(app_theme.text_muted)
                .build();
        });
}

fn draw_log(
    ui: &mut Ui,
    width: f32,
    height: f32,
    state: &NeoState<GameSession>,
    session: &GameSession,
    app_theme: AppTheme,
) {
    let content_w = scroll_content_width(width);
    let entries = log_rows(session, content_w);
    let content_h = entries
        .last()
        .map(|entry| entry.top + entry.row_height)
        .unwrap_or(64.0);
    let max_scroll = (content_h - height).max(0.0);
    let scroll_offset = session.log_scroll.clamp(0.0, max_scroll);
    let visible_top = (scroll_offset - LOG_OVERSCAN).max(0.0);
    let visible_bottom = scroll_offset + height + LOG_OVERSCAN;
    let first_visible = entries
        .iter()
        .position(|entry| entry.bottom() >= visible_top)
        .unwrap_or(entries.len());
    let last_visible = entries
        .iter()
        .rposition(|entry| entry.top <= visible_bottom)
        .map(|index| index + 1)
        .unwrap_or(first_visible);
    let top_spacer = entries
        .get(first_visible)
        .map(|entry| entry.top)
        .unwrap_or(0.0);
    let bottom_spacer = entries
        .get(last_visible.saturating_sub(1))
        .map(|entry| (content_h - entry.top - entry.row_height).max(0.0))
        .unwrap_or(0.0);

    ui.scroll_y("right.info.log")
        .size(width, height)
        .content_height(content_h.max(height))
        .gap(0.0)
        .theme(app_theme.tokens)
        .scrollbar_gap(10.0)
        .offset_bind(bind_panel_scroll(state, InfoPanel::Log))
        .content(|ui| {
            if session.log.is_empty() && session.state.dialogue_transcript.is_empty() {
                components::body_text(
                    ui,
                    "right.info.log.empty",
                    "还没有记录。车站暂时保持沉默。",
                    content_w,
                    64.0,
                    app_theme.text_soft,
                    14.0,
                );
                return;
            }

            spacer(ui, "right.info.log.top-spacer", content_w, top_spacer);
            for entry in &entries[first_visible..last_visible] {
                draw_log_entry(
                    ui,
                    content_w,
                    entry.index,
                    entry.kind,
                    entry.entry_height,
                    app_theme,
                );
                spacer(
                    ui,
                    format!("right.info.log.{}.gap", entry.index),
                    content_w,
                    entry.gap,
                );
            }
            spacer(ui, "right.info.log.bottom-spacer", content_w, bottom_spacer);
        });
}

#[derive(Clone, Copy)]
struct LogRow<'a> {
    index: usize,
    kind: LogKind<'a>,
    top: f32,
    entry_height: f32,
    row_height: f32,
    gap: f32,
}

#[derive(Clone, Copy)]
enum LogKind<'a> {
    Dialogue {
        transcript_index: usize,
        entry: &'a DialogueTranscriptEntry,
    },
    Event {
        event_index: usize,
        event: &'a StoryEvent,
    },
}

impl LogRow<'_> {
    fn bottom(self) -> f32 {
        self.top + self.row_height
    }
}

fn log_rows(session: &GameSession, width: f32) -> Vec<LogRow<'_>> {
    let mut top = 0.0;
    let mut kinds = Vec::new();
    let transcript_total = session.state.dialogue_transcript.len();
    kinds.extend(
        session
            .state
            .dialogue_transcript
            .iter()
            .rev()
            .enumerate()
            .map(|(index, entry)| LogKind::Dialogue {
                transcript_index: transcript_total.saturating_sub(index + 1),
                entry,
            }),
    );
    let event_total = session.log.len();
    kinds.extend(
        session
            .log
            .iter()
            .rev()
            .enumerate()
            .map(|(index, event)| LogKind::Event {
                event_index: event_total.saturating_sub(index + 1),
                event,
            }),
    );
    let total = kinds.len();
    kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let entry_height = log_kind_height(*kind, width);
            let gap = if index + 1 == total { 0.0 } else { LOG_GAP };
            let row = LogRow {
                index,
                kind: *kind,
                top,
                entry_height,
                row_height: entry_height + gap,
                gap,
            };
            top += row.row_height;
            row
        })
        .collect()
}

fn spacer(ui: &mut Ui, id: impl Into<String>, width: f32, height: f32) {
    if height > 0.0 {
        ui.stack(id).size(width, height).content(|_| {});
    }
}

fn draw_log_entry(
    ui: &mut Ui,
    width: f32,
    index: usize,
    kind: LogKind<'_>,
    entry_height: f32,
    app_theme: AppTheme,
) {
    let body_h = (entry_height - 44.0).max(52.0);
    ui.column(format!("right.info.log.{index}"))
        .size(width, entry_height)
        .gap(4.0)
        .content(|ui| {
            let (title, meta, body, accent) = match kind {
                LogKind::Dialogue {
                    transcript_index,
                    entry,
                } => (
                    format!("对话记录 {:02} · {}", transcript_index + 1, entry.title),
                    transcript_meta(entry),
                    entry.body.clone(),
                    app_theme.accent,
                ),
                LogKind::Event { event_index, event } => (
                    format!("事件记录 {:02} · {}", event_index + 1, event.title),
                    event
                        .tags
                        .first()
                        .map(|tag| format!("标签：{tag}"))
                        .unwrap_or_else(|| "车站日志".to_string()),
                    event.body.clone(),
                    app_theme.accent_warm,
                ),
            };
            ui.text(format!("right.info.log.{index}.title"))
                .size(width, 20.0)
                .text(title)
                .font_size(14.0)
                .line_height(18.0)
                .color(accent)
                .build();
            ui.text(format!("right.info.log.{index}.meta"))
                .size(width, 16.0)
                .text(meta)
                .font_size(10.0)
                .line_height(14.0)
                .color(app_theme.text_muted)
                .build();
            components::body_text(
                ui,
                format!("right.info.log.{index}.body"),
                body,
                width,
                body_h,
                app_theme.text_soft,
                12.0,
            );
        });
}

fn transcript_meta(entry: &DialogueTranscriptEntry) -> String {
    let choice = entry.choice.map(dialogue_choice_name).unwrap_or("进入对话");
    format!(
        "{} / {} / {}",
        entry.dialogue.title(),
        dialogue_system::node_name(entry.dialogue, entry.node),
        choice
    )
}

fn dialogue_choice_name(choice: DialogueChoiceId) -> &'static str {
    match choice {
        DialogueChoiceId::TravelerRain => "报纸与雨",
        DialogueChoiceId::TravelerEmptySeat => "第七张长椅",
        DialogueChoiceId::TravelerLoop => "循环里的逃离",
        DialogueChoiceId::ClerkTicket => "湿票核验",
        DialogueChoiceId::ClerkReturnRule => "返程双座",
        DialogueChoiceId::ClerkTomorrowPrice => "明天的价格",
        DialogueChoiceId::ChildWhiteLine => "白线以后",
        DialogueChoiceId::ChildAnger => "可以继续的气",
        DialogueChoiceId::ChildTomorrowBag => "明天的行李",
        DialogueChoiceId::KeeperDuty => "守夜的债",
        DialogueChoiceId::KeeperBroadcast => "广播里的声音",
        DialogueChoiceId::KeeperCoat => "无影的外套",
        DialogueChoiceId::DeepenTopic => "继续听",
        DialogueChoiceId::ChallengeTopic => "追问矛盾",
        DialogueChoiceId::PromiseTopic => "接到行动",
        DialogueChoiceId::BackToRoot => "回到话题清单",
        DialogueChoiceId::Leave => "结束对话",
    }
}

fn bind_panel_scroll(state: &NeoState<GameSession>, panel: InfoPanel) -> Binding<GameSession, f32> {
    state.bind(
        move |session| match panel {
            InfoPanel::Intel => session.intel_scroll,
            InfoPanel::Routes => session.routes_scroll,
            InfoPanel::Cases => session.cases_scroll,
            InfoPanel::Inventory => session.inventory_scroll,
            InfoPanel::Log => session.log_scroll,
        },
        move |session, value| {
            let value = value.max(0.0);
            match panel {
                InfoPanel::Intel => session.intel_scroll = value,
                InfoPanel::Routes => session.routes_scroll = value,
                InfoPanel::Cases => session.cases_scroll = value,
                InfoPanel::Inventory => session.inventory_scroll = value,
                InfoPanel::Log => session.log_scroll = value,
            }
        },
    )
}

fn panel_scroll_offset(
    session: &GameSession,
    panel: InfoPanel,
    content_h: f32,
    viewport_h: f32,
) -> f32 {
    let raw = match panel {
        InfoPanel::Intel => session.intel_scroll,
        InfoPanel::Routes => session.routes_scroll,
        InfoPanel::Cases => session.cases_scroll,
        InfoPanel::Inventory => session.inventory_scroll,
        InfoPanel::Log => session.log_scroll,
    };
    raw.clamp(0.0, (content_h - viewport_h).max(0.0))
}

fn virtual_section<T>(
    ui: &mut Ui,
    list: &mut components::VirtualList,
    prefix: &str,
    label_id: &str,
    label: &str,
    width: f32,
    items: &[T],
    heights: &[f32],
    mut draw: impl FnMut(&mut Ui, usize, &T, f32),
) {
    if items.is_empty() {
        return;
    }
    list.row(ui, format!("{prefix}.label.row"), width, 30.0, |ui| {
        components::section_label(ui, label_id, label, width);
    });
    for (index, item) in items.iter().enumerate() {
        let height = heights.get(index).copied().unwrap_or(0.0);
        let row_height = height + if index + 1 == items.len() { 0.0 } else { 10.0 };
        list.row(
            ui,
            format!("{prefix}.row.{index}"),
            width,
            row_height,
            |ui| draw(ui, index, item, height),
        );
    }
}

fn scroll_content_width(width: f32) -> f32 {
    (width - SCROLLBAR_RESERVE).max(0.0)
}

fn stacked_height(count: usize, item_height: f32, gap: f32) -> f32 {
    if count == 0 {
        0.0
    } else {
        count as f32 * item_height + count.saturating_sub(1) as f32 * gap
    }
}

fn log_kind_height(kind: LogKind<'_>, width: f32) -> f32 {
    let body = match kind {
        LogKind::Dialogue { entry, .. } => &entry.body,
        LogKind::Event { event, .. } => &event.body,
    };
    44.0 + wrapped_height(body, width, 12.0, 52.0)
}

fn wrapped_height(text: &str, width: f32, font_size: f32, min_height: f32) -> f32 {
    let line_height = font_size + 8.0;
    let chars_per_line = (width / (font_size * 0.92)).floor().max(10.0);
    let lines = (text.chars().count() as f32 / chars_per_line)
        .ceil()
        .max(1.0);
    (lines * line_height).max(min_height)
}
