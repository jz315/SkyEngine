use std::path::PathBuf;

use sky_engine::app::{AppState, FrameContext, SetupContext};
use sky_engine::input::KeyCode;
use sky_engine::render::{CameraMarker, MainCamera, Projection, RenderSettings, Transform};
use sky_engine::ui::neo::NeoState;

use crate::actions;
use crate::model::{
    AftertalkId, CaseDialogueId, CaseFileId, CompanionTalkId, DepartureId, DialogueChallengeId,
    DialogueChallengeResponseId, DialogueId, DialogueQuestionId, DialogueRelayId, DialogueTone,
    EndingPreludeId, FinalDebateId, FinalDebateResponseId, FinalInterviewId, Flag, GameMode,
    GameSession, InfoPanel, Item, LampFocusId, Location, MemoryId, PatrolId, RouteCostId,
    RoutePressureId, RoutePressureResponseId, RouteWitnessDebriefId, RouteWitnessId,
    StationRequestId, StationWhisperId, StoryEvent, TopicId, TruthSceneId, VowId,
};
use crate::save;
use crate::theme;
use crate::view;

#[derive(Debug)]
pub struct FogLanternStation {
    state: NeoState<GameSession>,
    screenshot: ScreenshotProbe,
}

impl Default for FogLanternStation {
    fn default() -> Self {
        let mut session = GameSession::new();
        if env_flag("SKY_FOG_LANTERN_START_PLAYING") {
            actions::start(&mut session);
        }
        configure_debug_session(&mut session);

        Self {
            state: NeoState::new(session),
            screenshot: ScreenshotProbe::default(),
        }
    }
}

impl AppState for FogLanternStation {
    fn setup(&mut self, ctx: &mut SetupContext<'_>) {
        let app_theme = theme::station_theme();
        ctx.world.insert_resource(RenderSettings {
            clear_color: app_theme.background_bottom.into(),
            ..Default::default()
        });
        ctx.world.spawn((
            Transform::default(),
            CameraMarker::new(),
            Projection::orthographic(860.0),
            MainCamera,
        ));
    }

    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        self.handle_keyboard(ctx);

        let snapshot = self.state.read(Clone::clone);
        let title_snapshot = snapshot.clone();
        let state = self.state.clone();
        sky_engine::ui::neo::compose(ctx, move |ui, screen| {
            view::render(ui, screen, &state, &snapshot);
        });

        ctx.set_title(&window_title(&title_snapshot));
        ctx.render();
        ctx.ui().render_overlays();
        self.screenshot.update(ctx);
        ctx.request_redraw();
    }
}

impl FogLanternStation {
    fn handle_keyboard(&mut self, ctx: &mut FrameContext<'_>) {
        if ctx.input.key_pressed(KeyCode::Escape) {
            ctx.request_exit();
            return;
        }
        if ctx.input.key_pressed(KeyCode::Space) {
            self.state.update(|session| {
                if session.mode == GameMode::Title {
                    actions::start(session);
                }
            });
        }
        if ctx.input.key_pressed(KeyCode::KeyR) {
            self.state.update(actions::restart);
        }
        if ctx.input.key_pressed(KeyCode::F5) {
            self.state.update(|session| match save::save(session) {
                Ok(path) => session.notice = Some(format!("已保存到 {}", path.display())),
                Err(error) => session.notice = Some(format!("保存失败：{error}")),
            });
        }
        if ctx.input.key_pressed(KeyCode::F9) {
            self.state.update(|session| match save::load() {
                Ok(mut loaded) => {
                    loaded.notice = Some("存档已读取。".to_string());
                    *session = loaded;
                }
                Err(error) => session.notice = Some(format!("读取失败：{error}")),
            });
        }
    }
}

fn window_title(session: &GameSession) -> String {
    match session.mode {
        GameMode::Title => "雾灯站".to_string(),
        GameMode::Playing => format!(
            "雾灯站 | {} | {} | 剩余 {} 分钟",
            session.state.location.title(),
            session.state.clock_text(),
            session.state.time_left()
        ),
        GameMode::Ending => session
            .state
            .ended
            .map(|ending| ending.title().to_string())
            .unwrap_or_else(|| "雾灯站 | 终局".to_string()),
    }
}

#[derive(Debug)]
struct ScreenshotProbe {
    path: Option<PathBuf>,
    frame: u32,
    frame_count: u32,
    taken: bool,
    exit_after: bool,
}

impl Default for ScreenshotProbe {
    fn default() -> Self {
        Self {
            path: std::env::var("SKY_NEO_SCREENSHOT_PATH")
                .ok()
                .map(PathBuf::from)
                .filter(|path| !path.as_os_str().is_empty()),
            frame: env_u32("SKY_NEO_SCREENSHOT_FRAME").unwrap_or(30),
            frame_count: 0,
            taken: false,
            exit_after: env_flag("SKY_NEO_EXIT_AFTER_SCREENSHOT"),
        }
    }
}

impl ScreenshotProbe {
    fn update(&mut self, ctx: &mut FrameContext<'_>) {
        if !self.taken && self.frame_count >= self.frame {
            if let Some(path) = self.path.as_ref() {
                ctx.request_screenshot(path);
                self.taken = true;
                if self.exit_after {
                    ctx.request_exit();
                }
            }
        }
        self.frame_count = self.frame_count.saturating_add(1);
    }
}

fn env_flag(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
}

fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key).ok()?.parse().ok()
}

fn configure_debug_session(session: &mut GameSession) {
    if env_flag("SKY_FOG_LANTERN_DEBUG_SCROLL_CONTENT") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        for item in [
            Item::WetTicket,
            Item::BrassKey,
            Item::LanternGlass,
            Item::OldTimetable,
            Item::StationLog,
            Item::NameTag,
            Item::SignalWhistle,
            Item::StationMap,
            Item::ChildHomework,
            Item::BroadcastTape,
            Item::ConductorRoster,
            Item::MirrorShard,
            Item::CoinToken,
        ] {
            session.state.inventory.insert(item);
        }
        for index in 0..14 {
            session.push_event(StoryEvent::new(
                format!("巡查记录 {}", index + 1),
                "雾灯在整点前闪烁，广播重复念出同一个缺字的姓名。你把这条记录写下，又发现上一页已经有同样的笔迹。",
            ));
        }
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_RICH_ENDING") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.actions_used = crate::model::MAX_ACTIONS;
        session.state.loop_count = crate::model::MAX_SEGMENTS;
        session.state.remember(Flag::FinalTrainArrived);
        session.state.remember(Flag::ChildJoined);
        session.state.remember(Flag::SynthesizedChildTruth);
        session.state.remember(Flag::ExaminedTicket);
        session.state.remember(Flag::RecoveredName);
        session.state.remember(Flag::UnderstoodStationMechanism);
        session.state.dialogue_tone = DialogueTone::Gentle;
        session.state.answer_dialogue_challenge(
            DialogueChallengeId::ChildAsksIfYouWillLeaveAgain,
            DialogueChallengeResponseId::Promise,
        );
        for truth in [
            TruthSceneId::WetTicketWarning,
            TruthSceneId::ChildIsNotCargo,
            TruthSceneId::ReturnTicketSignature,
        ] {
            session.state.reveal_truth_scene(truth);
        }
        for request in [
            StationRequestId::NewspaperCorrection,
            StationRequestId::RefundLedger,
            StationRequestId::HomeworkEnvelope,
        ] {
            session.state.complete_request(request);
        }
        for case_file in [
            CaseFileId::WetTicketProtocol,
            CaseFileId::ReturnProtocol,
            CaseFileId::ChildWitness,
        ] {
            session.state.resolve_case_file(case_file);
        }
        actions::apply(
            session,
            crate::model::ActionId::EnterEndingPrelude(EndingPreludeId::ChildWhiteLine),
        );
        actions::apply(
            session,
            crate::model::ActionId::AnswerEndingPrelude(
                EndingPreludeId::ChildWhiteLine,
                crate::model::EndingPreludeResponseId::ReturnChoice,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::AnswerFinalDebate(
                FinalDebateId::ChildWhiteLine,
                FinalDebateResponseId::RewritePromise,
            ),
        );
        actions::apply(session, crate::model::ActionId::BoardWithChild);
        session.active_panel = InfoPanel::Log;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_FINAL_PRELUDE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.actions_used = crate::model::MAX_ACTIONS;
        session.state.loop_count = crate::model::MAX_SEGMENTS;
        session.state.location = Location::Platform;
        session.state.ticket = crate::model::TicketKind::Return;
        for flag in [
            Flag::FinalTrainArrived,
            Flag::MetChild,
            Flag::ChildJoined,
            Flag::SynthesizedChildTruth,
            Flag::RecoveredName,
        ] {
            session.state.remember(flag);
        }
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::EnterEndingPrelude(EndingPreludeId::ChildWhiteLine)
            )
        }));
        session.active_panel = InfoPanel::Routes;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_FINAL_RESPONSE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.actions_used = crate::model::MAX_ACTIONS;
        session.state.loop_count = crate::model::MAX_SEGMENTS;
        session.state.location = Location::Platform;
        session.state.ticket = crate::model::TicketKind::Return;
        for flag in [
            Flag::FinalTrainArrived,
            Flag::MetChild,
            Flag::ChildJoined,
            Flag::SynthesizedChildTruth,
            Flag::RecoveredName,
        ] {
            session.state.remember(flag);
        }
        actions::apply(
            session,
            crate::model::ActionId::EnterEndingPrelude(EndingPreludeId::ChildWhiteLine),
        );
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::AnswerEndingPrelude(
                    EndingPreludeId::ChildWhiteLine,
                    crate::model::EndingPreludeResponseId::ReturnChoice
                )
            )
        }));
        session.active_panel = InfoPanel::Routes;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_FINAL_DEBATE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.actions_used = crate::model::MAX_ACTIONS;
        session.state.loop_count = crate::model::MAX_SEGMENTS;
        session.state.location = Location::Platform;
        session.state.ticket = crate::model::TicketKind::Return;
        for flag in [
            Flag::FinalTrainArrived,
            Flag::MetChild,
            Flag::ChildJoined,
            Flag::SynthesizedChildTruth,
            Flag::RecoveredName,
        ] {
            session.state.remember(flag);
        }
        actions::apply(
            session,
            crate::model::ActionId::EnterEndingPrelude(EndingPreludeId::ChildWhiteLine),
        );
        actions::apply(
            session,
            crate::model::ActionId::AnswerEndingPrelude(
                EndingPreludeId::ChildWhiteLine,
                crate::model::EndingPreludeResponseId::ReturnChoice,
            ),
        );
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::AnswerFinalDebate(
                    FinalDebateId::ChildWhiteLine,
                    FinalDebateResponseId::RewritePromise
                )
            )
        }));
        session.active_panel = InfoPanel::Routes;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_ROUTE_PRESSURE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::Platform;
        session.state.remember(Flag::MetChild);
        session.state.remember(Flag::ChildJoined);
        session
            .state
            .prepare_departure(DepartureId::ChildWindowSeat);
        session.state.active_dialogue = Some(crate::model::ActiveDialogue {
            dialogue: DialogueId::Child,
            node: crate::model::DialogueNodeId::Root,
        });
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::AnswerRoutePressure(
                    RoutePressureId::ChildTomorrow,
                    RoutePressureResponseId::RevisePromise
                )
            )
        }));
        session.active_panel = InfoPanel::Routes;
        session.routes_scroll = 2050.0;
        session.action_scroll = 380.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_CASE_DIALOGUE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.remember(Flag::ExaminedTicket);
        session.state.remember(Flag::ReadDepartureBoard);
        session
            .state
            .resolve_case_file(CaseFileId::WetTicketProtocol);
        session.state.active_dialogue = Some(crate::model::ActiveDialogue {
            dialogue: DialogueId::Traveler,
            node: crate::model::DialogueNodeId::Root,
        });
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::DiscussCaseDialogue(CaseDialogueId::WetTicketTraveler)
            )
        }));
        session.active_panel = InfoPanel::Cases;
        session.cases_scroll = 520.0;
        session.action_scroll = 150.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_TRUTH_SCENE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.remember(Flag::ExaminedTicket);
        session.state.remember(Flag::ReadDepartureBoard);
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::RevealTruth(TruthSceneId::WetTicketWarning)
            )
        }));
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 4300.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_COMPANION") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.child_trust = 5;
        for flag in [
            Flag::MetChild,
            Flag::ChildJoined,
            Flag::TravelerTrusted,
            Flag::ReadDepartureBoard,
            Flag::SynthesizedRoute,
            Flag::RecoveredName,
        ] {
            session.state.remember(flag);
        }
        session.state.visit_memory(MemoryId::SeventhBench);
        session.state.complete_patrol(PatrolId::WaitingHallManifest);
        session.state.discuss(TopicId::TravelerRain);
        session.state.discuss(TopicId::ChildTomorrow);
        session
            .state
            .complete_companion_talk(CompanionTalkId::PlatformWhiteLineTogether);
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 2300.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_DIALOGUE_QUESTION") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::TicketOffice;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Clerk),
        );
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::AskDialogueQuestion(
                    crate::model::DialogueQuestionId::ClerkAboutWetTicket
                )
            )
        }));
        session.active_panel = InfoPanel::Log;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_DIALOGUE_CHALLENGE") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::TicketOffice;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Clerk),
        );
        actions::apply(
            session,
            crate::model::ActionId::AskDialogueQuestion(DialogueQuestionId::ClerkAboutWetTicket),
        );
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::AnswerDialogueChallenge(
                    DialogueChallengeId::ClerkAsksWhoPaysForReturn,
                    DialogueChallengeResponseId::Promise
                )
            )
        }));
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 3300.0;
        session.action_scroll = 120.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_LAMP_FOCUS") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.remember(Flag::RepairedFogLamp);
        session.state.remember(Flag::ReadDepartureBoard);
        session.state.remember(Flag::SearchedLostFound);
        session.state.remember(Flag::RecoveredName);
        session.state.add_item(Item::StationMap);
        session.state.add_item(Item::NameTag);
        session
            .state
            .focus_lamp_trace(LampFocusId::PlatformBrakeLight);
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 6900.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_ROUTE_COST") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        session
            .state
            .rehearse_departure(DepartureId::SingleReturnPocket);
        session
            .state
            .rehearse_departure(DepartureId::ChildWindowSeat);
        session
            .state
            .complete_aftertalk(AftertalkId::TravelerSecondSeat);
        session.state.make_vow(VowId::ReturnWithoutErasing);
        session
            .state
            .mitigate_route_cost(RouteCostId::ChildUnforgivenTomorrow);
        session.active_panel = InfoPanel::Routes;
        session.routes_scroll = 1200.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_ROUTE_ECHO") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        session
            .state
            .mitigate_route_cost(RouteCostId::AloneEmptySeat);
        session
            .state
            .mitigate_route_cost(RouteCostId::ChildUnforgivenTomorrow);
        session
            .state
            .complete_route_echo(RouteCostId::ChildUnforgivenTomorrow);
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 4050.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_ROUTE_WITNESS") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 6;
        session
            .state
            .complete_route_echo(RouteCostId::AloneEmptySeat);
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::VisitRouteWitness(RouteWitnessId::AloneSeatNotice)
            )
        }));
        session.active_panel = InfoPanel::Routes;
        session.routes_scroll = 3900.0;
        session.action_scroll = 430.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_ROUTE_WITNESS_DEBRIEF") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 6;
        session
            .state
            .visit_route_witness(RouteWitnessId::AloneSeatNotice);
        session.state.active_dialogue = Some(crate::model::ActiveDialogue {
            dialogue: DialogueId::Traveler,
            node: crate::model::DialogueNodeId::Root,
        });
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::DebriefRouteWitness(
                    RouteWitnessDebriefId::AloneSeatTraveler
                )
            )
        }));
        session.active_panel = InfoPanel::Routes;
        session.routes_scroll = 4800.0;
        session.action_scroll = 420.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_FINAL_INTERVIEW") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::Platform;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 7;
        session
            .state
            .complete_route_witness_debrief(RouteWitnessDebriefId::ChildSeatChild);
        session.state.dialogue_tone = DialogueTone::Gentle;
        session.state.active_dialogue = Some(crate::model::ActiveDialogue {
            dialogue: DialogueId::Child,
            node: crate::model::DialogueNodeId::Root,
        });
        debug_assert!(actions::scene_actions(&session.state).iter().any(|action| {
            matches!(
                action.id,
                crate::model::ActionId::HoldFinalInterview(FinalInterviewId::ChildOrdinaryTomorrow)
            )
        }));
        actions::apply(
            session,
            crate::model::ActionId::HoldFinalInterview(FinalInterviewId::ChildOrdinaryTomorrow),
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 7600.0;
        session.action_scroll = 0.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_WHISPERS") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 4;
        session.state.location_depths = [5, 5, 5, 5, 5, 5];
        session
            .state
            .hear_station_whisper(StationWhisperId::TicketOfficeStampHumidity);
        session
            .state
            .hear_station_whisper(StationWhisperId::PlatformBrakeDust);
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 9000.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_DIALOGUE_SYSTEM") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session.state.remember(Flag::ReadDepartureBoard);
        session.state.add_item(Item::MirrorShard);
        session.state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 2;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        session.active_panel = InfoPanel::Intel;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_DIALOGUE_LEADS") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 900.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_LEAD_RETURN") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        actions::apply(
            session,
            crate::model::ActionId::FollowDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 3600.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_DIALOGUE_RELAY") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        actions::apply(
            session,
            crate::model::ActionId::FollowDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReturnDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::Platform;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Child),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );
        debug_assert!(
            crate::dialogue_relay::active_relay_objective_hint(&session.state).is_some()
                || session
                    .state
                    .has_completed_dialogue_relay(DialogueRelayId::MirrorToChild)
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 4200.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_RELAY_REFLECTION") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        actions::apply(
            session,
            crate::model::ActionId::FollowDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReturnDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::Platform;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Child),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );
        actions::apply(
            session,
            crate::model::ActionId::RelayDialogueLead(DialogueRelayId::MirrorToChild),
        );
        debug_assert!(
            crate::dialogue_relay::active_reflection_objective_hint(&session.state).is_some()
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 6100.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_RELAY_ECHO") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        actions::apply(
            session,
            crate::model::ActionId::FollowDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReturnDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::Platform;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Child),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );
        actions::apply(
            session,
            crate::model::ActionId::RelayDialogueLead(DialogueRelayId::MirrorToChild),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReflectDialogueRelay(DialogueRelayId::MirrorToChild),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        debug_assert!(crate::dialogue_relay::active_echo_objective_hint(&session.state).is_some());
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 6100.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_RELAY_ANCHOR") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::TravelerRain),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        actions::apply(
            session,
            crate::model::ActionId::FollowDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReturnDialogueLead(
                crate::model::DialogueLeadId::RainUnderBench,
            ),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::Platform;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Child),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::ChildWhiteLine),
        );
        actions::apply(
            session,
            crate::model::ActionId::RelayDialogueLead(DialogueRelayId::MirrorToChild),
        );
        actions::apply(
            session,
            crate::model::ActionId::ReflectDialogueRelay(DialogueRelayId::MirrorToChild),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        session.state.location = Location::WaitingHall;
        actions::apply(
            session,
            crate::model::ActionId::BeginDialogue(DialogueId::Traveler),
        );
        actions::apply(
            session,
            crate::model::ActionId::EchoDialogueRelay(DialogueRelayId::MirrorToChild),
        );
        actions::apply(
            session,
            crate::model::ActionId::ChooseDialogue(crate::model::DialogueChoiceId::Leave),
        );
        debug_assert!(crate::dialogue_relay::available_anchors(&session.state)
            .iter()
            .any(|anchor| anchor.relay == DialogueRelayId::MirrorToChild));
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 6100.0;
    }

    if env_flag("SKY_FOG_LANTERN_DEBUG_RELAY_REVIEW") {
        if session.mode == GameMode::Title {
            actions::start(session);
        }
        session.state.location = Location::WaitingHall;
        session
            .state
            .complete_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench);
        session
            .state
            .return_dialogue_lead(crate::model::DialogueLeadId::RainUnderBench);
        session
            .state
            .complete_dialogue_relay(DialogueRelayId::MirrorToChild);
        session
            .state
            .reflect_dialogue_relay(DialogueRelayId::MirrorToChild);
        session
            .state
            .echo_dialogue_relay(DialogueRelayId::MirrorToChild);
        session
            .state
            .anchor_dialogue_relay(DialogueRelayId::MirrorToChild);
        debug_assert!(
            crate::dialogue_relay::available_anchor_reviews(&session.state)
                .iter()
                .any(|review| review.relay == DialogueRelayId::MirrorToChild)
        );
        session.active_panel = InfoPanel::Intel;
        session.intel_scroll = 6100.0;
    }

    if let Ok(panel) = std::env::var("SKY_FOG_LANTERN_PANEL") {
        session.active_panel = match panel.to_ascii_lowercase().as_str() {
            "route" | "routes" | "ending" | "endings" => InfoPanel::Routes,
            "case" | "cases" | "archive" | "archives" | "file" | "files" => InfoPanel::Cases,
            "inventory" | "items" | "item" => InfoPanel::Inventory,
            "log" | "logs" => InfoPanel::Log,
            _ => InfoPanel::Intel,
        };
    }

    if let Some(scroll) = env_f32("SKY_FOG_LANTERN_PANEL_SCROLL") {
        match session.active_panel {
            InfoPanel::Intel => session.intel_scroll = scroll,
            InfoPanel::Routes => session.routes_scroll = scroll,
            InfoPanel::Cases => session.cases_scroll = scroll,
            InfoPanel::Inventory => session.inventory_scroll = scroll,
            InfoPanel::Log => session.log_scroll = scroll,
        }
    }

    if let Some(scroll) = env_f32("SKY_FOG_LANTERN_ACTION_SCROLL") {
        session.action_scroll = scroll;
    }

    if let Some(scroll) = env_f32("SKY_FOG_LANTERN_STORY_SCROLL") {
        session.story_scroll = scroll;
    }
}

fn env_f32(key: &str) -> Option<f32> {
    std::env::var(key).ok()?.parse().ok()
}
