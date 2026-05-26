use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::model::GameSession;

const SAVE_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct SaveFile {
    version: u32,
    session: GameSession,
}

pub fn default_save_path() -> PathBuf {
    let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.join("target")
        .join("fog_lantern_station")
        .join("save.json")
}

pub fn save(session: &GameSession) -> io::Result<PathBuf> {
    let path = default_save_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let data = SaveFile {
        version: SAVE_VERSION,
        session: session.clone(),
    };
    let json = serde_json::to_string_pretty(&data)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load() -> io::Result<GameSession> {
    let path = default_save_path();
    let json = fs::read_to_string(path)?;
    let mut data: SaveFile = serde_json::from_str(&json)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if data.version != SAVE_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unsupported save version {}", data.version),
        ));
    }
    data.session.normalize_after_load();
    Ok(data.session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{actions, model::GameSession};

    #[test]
    fn save_round_trip_preserves_story_state() {
        let mut session = GameSession::new();
        actions::start(&mut session);
        actions::apply(&mut session, crate::model::ActionId::ExamineTicket);
        actions::apply(
            &mut session,
            crate::model::ActionId::Discuss(crate::model::TopicId::TravelerRain),
        );
        actions::apply(
            &mut session,
            crate::model::ActionId::SetDialogueTone(crate::model::DialogueTone::Direct),
        );
        session
            .state
            .present(crate::model::EvidenceId::TravelerMirror);
        session
            .state
            .resolve_case_file(crate::model::CaseFileId::WetTicketProtocol);
        session
            .state
            .complete_case_dialogue(crate::model::CaseDialogueId::WetTicketTraveler);
        session
            .state
            .complete_request(crate::model::StationRequestId::NewspaperCorrection);
        session
            .state
            .resolve_resonance(crate::model::ResonanceId::RainInTheMirror);
        session
            .state
            .make_vow(crate::model::VowId::ReadTheWholeWarning);
        session
            .state
            .visit_memory(crate::model::MemoryId::SeventhBench);
        session
            .state
            .complete_patrol(crate::model::PatrolId::WaitingHallManifest);
        session
            .state
            .complete_aftertalk(crate::model::AftertalkId::TravelerSecondSeat);
        session
            .state
            .complete_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench);
        session
            .state
            .return_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench);
        session
            .state
            .complete_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild);
        session
            .state
            .reflect_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild);
        session
            .state
            .echo_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild);
        session
            .state
            .anchor_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild);
        session
            .state
            .review_dialogue_anchor(crate::model::DialogueRelayId::MirrorToChild);
        session
            .state
            .complete_companion_talk(crate::model::CompanionTalkId::WaitingHallEmptySeat);
        session
            .state
            .focus_lamp_trace(crate::model::LampFocusId::WaitingHallBenchTrace);
        session
            .state
            .hear_station_whisper(crate::model::StationWhisperId::WaitingHallUmbrellaCount);
        session.state.resolve_anomaly(
            crate::model::AnomalyId::ScreenKeepsScore,
            crate::model::AnomalyResponse::Stabilize,
        );
        session
            .state
            .prepare_departure(crate::model::DepartureId::SingleReturnPocket);
        session
            .state
            .rehearse_departure(crate::model::DepartureId::SingleReturnPocket);
        session
            .state
            .mitigate_route_cost(crate::model::RouteCostId::AloneEmptySeat);
        session
            .state
            .complete_route_echo(crate::model::RouteCostId::AloneEmptySeat);
        session
            .state
            .visit_route_witness(crate::model::RouteWitnessId::AloneSeatNotice);
        session
            .state
            .complete_route_witness_debrief(crate::model::RouteWitnessDebriefId::AloneSeatTraveler);
        session.state.answer_route_pressure(
            crate::model::RoutePressureId::AloneTraveler,
            crate::model::RoutePressureResponseId::AdmitRisk,
        );
        session
            .state
            .reveal_truth_scene(crate::model::TruthSceneId::WetTicketWarning);
        session
            .state
            .complete_ending_prelude(crate::model::EndingPreludeId::AloneDoor);
        session.state.answer_ending_prelude(
            crate::model::EndingPreludeId::AloneDoor,
            crate::model::EndingPreludeResponseId::AcceptCost,
        );
        session.state.answer_final_debate(
            crate::model::FinalDebateId::AloneTraveler,
            crate::model::FinalDebateResponseId::AdmitWound,
        );
        session
            .state
            .complete_final_interview(crate::model::FinalInterviewId::TravelerEmptySeat);
        let dialogue_beat = crate::model::DialogueBeatKey {
            dialogue: crate::model::DialogueId::Traveler,
            node: crate::model::DialogueNodeId::Memory,
            choice: crate::model::DialogueChoiceId::DeepenTopic,
        };
        session.state.complete_dialogue_beat(dialogue_beat);
        session
            .state
            .answer_dialogue_question(crate::model::DialogueQuestionId::TravelerAboutChild);
        session.state.answer_dialogue_challenge(
            crate::model::DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat,
            crate::model::DialogueChallengeResponseId::Admit,
        );
        session
            .state
            .record_dialogue_line(crate::model::DialogueTranscriptEntry::new(
                crate::model::DialogueId::Traveler,
                crate::model::DialogueNodeId::Memory,
                Some(crate::model::DialogueChoiceId::DeepenTopic),
                "对话：候车厅老人 / 报纸与雨",
                "老人把雨声放回报纸里，等你下一次不要跳过它。",
            ));
        session.state.active_dialogue = Some(crate::model::ActiveDialogue {
            dialogue: crate::model::DialogueId::Traveler,
            node: crate::model::DialogueNodeId::Memory,
        });
        session
            .endings_seen
            .insert(crate::model::Ending::TookChildHome);
        let json = serde_json::to_string(&SaveFile {
            version: SAVE_VERSION,
            session: session.clone(),
        })
        .unwrap();
        let mut decoded: SaveFile = serde_json::from_str(&json).unwrap();
        decoded.session.normalize_after_load();
        assert_eq!(decoded.session.state.elapsed_minutes(), 10);
        assert_eq!(decoded.session.log.len(), 4);
        assert!(decoded
            .session
            .state
            .has_discussed(crate::model::TopicId::TravelerRain));
        assert!(decoded
            .session
            .state
            .has_presented(crate::model::EvidenceId::TravelerMirror));
        assert!(decoded
            .session
            .state
            .has_resolved_case_file(crate::model::CaseFileId::WetTicketProtocol));
        assert!(decoded
            .session
            .state
            .has_completed_case_dialogue(crate::model::CaseDialogueId::WetTicketTraveler));
        assert!(decoded
            .session
            .state
            .has_completed_request(crate::model::StationRequestId::NewspaperCorrection));
        assert!(decoded
            .session
            .state
            .has_resolved_resonance(crate::model::ResonanceId::RainInTheMirror));
        assert!(decoded
            .session
            .state
            .has_vow(crate::model::VowId::ReadTheWholeWarning));
        assert!(decoded
            .session
            .state
            .has_memory(crate::model::MemoryId::SeventhBench));
        assert!(decoded
            .session
            .state
            .has_completed_patrol(crate::model::PatrolId::WaitingHallManifest));
        assert!(decoded
            .session
            .state
            .has_completed_aftertalk(crate::model::AftertalkId::TravelerSecondSeat));
        assert!(decoded
            .session
            .state
            .has_completed_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench));
        assert!(decoded
            .session
            .state
            .has_returned_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench));
        assert!(decoded
            .session
            .state
            .has_completed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(decoded
            .session
            .state
            .has_reflected_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(decoded
            .session
            .state
            .has_echoed_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(decoded
            .session
            .state
            .has_anchored_dialogue_relay(crate::model::DialogueRelayId::MirrorToChild));
        assert!(decoded
            .session
            .state
            .has_reviewed_dialogue_anchor(crate::model::DialogueRelayId::MirrorToChild));
        assert!(decoded
            .session
            .state
            .has_completed_companion_talk(crate::model::CompanionTalkId::WaitingHallEmptySeat));
        assert!(decoded
            .session
            .state
            .has_focused_lamp_trace(crate::model::LampFocusId::WaitingHallBenchTrace));
        assert!(decoded
            .session
            .state
            .has_heard_station_whisper(crate::model::StationWhisperId::WaitingHallUmbrellaCount));
        assert_eq!(
            decoded
                .session
                .state
                .anomaly_response(crate::model::AnomalyId::ScreenKeepsScore),
            Some(crate::model::AnomalyResponse::Stabilize)
        );
        assert!(decoded
            .session
            .state
            .has_prepared_departure(crate::model::DepartureId::SingleReturnPocket));
        assert!(decoded
            .session
            .state
            .has_rehearsed_departure(crate::model::DepartureId::SingleReturnPocket));
        assert!(decoded
            .session
            .state
            .has_mitigated_route_cost(crate::model::RouteCostId::AloneEmptySeat));
        assert!(decoded
            .session
            .state
            .has_completed_route_echo(crate::model::RouteCostId::AloneEmptySeat));
        assert!(decoded
            .session
            .state
            .has_visited_route_witness(crate::model::RouteWitnessId::AloneSeatNotice));
        assert!(decoded.session.state.has_completed_route_witness_debrief(
            crate::model::RouteWitnessDebriefId::AloneSeatTraveler
        ));
        assert_eq!(
            decoded
                .session
                .state
                .route_pressure_response(crate::model::RoutePressureId::AloneTraveler),
            Some(crate::model::RoutePressureResponseId::AdmitRisk)
        );
        assert!(decoded
            .session
            .state
            .has_revealed_truth_scene(crate::model::TruthSceneId::WetTicketWarning));
        assert!(decoded
            .session
            .state
            .has_completed_ending_prelude(crate::model::EndingPreludeId::AloneDoor));
        assert_eq!(
            decoded
                .session
                .state
                .ending_prelude_response(crate::model::EndingPreludeId::AloneDoor),
            Some(crate::model::EndingPreludeResponseId::AcceptCost)
        );
        assert_eq!(
            decoded
                .session
                .state
                .final_debate_response(crate::model::FinalDebateId::AloneTraveler),
            Some(crate::model::FinalDebateResponseId::AdmitWound)
        );
        assert!(decoded
            .session
            .state
            .has_completed_final_interview(crate::model::FinalInterviewId::TravelerEmptySeat));
        assert_eq!(
            decoded.session.state.dialogue_tone,
            crate::model::DialogueTone::Direct
        );
        assert!(decoded
            .session
            .state
            .has_completed_dialogue_beat(dialogue_beat));
        assert!(decoded
            .session
            .state
            .has_answered_dialogue_question(crate::model::DialogueQuestionId::TravelerAboutChild));
        assert_eq!(
            decoded.session.state.dialogue_challenge_response(
                crate::model::DialogueChallengeId::TravelerAsksWhyYouKeptTheSeat
            ),
            Some(crate::model::DialogueChallengeResponseId::Admit)
        );
        assert_eq!(decoded.session.state.dialogue_transcript.len(), 1);
        assert_eq!(
            decoded.session.state.active_dialogue,
            Some(crate::model::ActiveDialogue {
                dialogue: crate::model::DialogueId::Traveler,
                node: crate::model::DialogueNodeId::Memory,
            })
        );
        assert!(decoded
            .session
            .endings_seen
            .contains(&crate::model::Ending::TookChildHome));
    }
}
