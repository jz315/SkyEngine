use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

pub const MINUTES_PER_SEGMENT: u8 = 30;
pub const MAX_SEGMENTS: u8 = 8;
pub const MAX_NIGHT_MINUTES: u8 = MINUTES_PER_SEGMENT * MAX_SEGMENTS;
pub const ACTIONS_PER_SEGMENT: u8 = MINUTES_PER_SEGMENT;
pub const MAX_ACTIONS: u8 = MAX_NIGHT_MINUTES;
pub const START_CLOCK_MINUTES: u16 = 23 * 60 + 48;
pub const LOCATION_INVESTIGATION_STEPS: u8 = 12;
pub const NPC_THREAD_STEPS: u8 = 12;
pub const MAX_LOG_EVENTS: usize = 220;
pub const MAX_DIALOGUE_TRANSCRIPT: usize = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Location {
    WaitingHall,
    TicketOffice,
    LostAndFound,
    Underpass,
    ClockTower,
    Platform,
}

impl Location {
    pub const ALL: [Self; 6] = [
        Self::WaitingHall,
        Self::TicketOffice,
        Self::LostAndFound,
        Self::Underpass,
        Self::ClockTower,
        Self::Platform,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::WaitingHall => "waiting_hall",
            Self::TicketOffice => "ticket_office",
            Self::LostAndFound => "lost_and_found",
            Self::Underpass => "underpass",
            Self::ClockTower => "clock_tower",
            Self::Platform => "platform",
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::WaitingHall => 0,
            Self::TicketOffice => 1,
            Self::LostAndFound => 2,
            Self::Underpass => 3,
            Self::ClockTower => 4,
            Self::Platform => 5,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WaitingHall => "候车厅",
            Self::TicketOffice => "售票窗口",
            Self::LostAndFound => "失物招领处",
            Self::Underpass => "地下通道",
            Self::ClockTower => "旧钟楼",
            Self::Platform => "三号月台",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Item {
    WetTicket,
    BrassKey,
    LanternGlass,
    OldTimetable,
    StationLog,
    NameTag,
    SignalWhistle,
    StationMap,
    ChildHomework,
    BroadcastTape,
    ConductorRoster,
    MirrorShard,
    CoinToken,
}

impl Item {
    pub fn name(self) -> &'static str {
        match self {
            Self::WetTicket => "湿透的车票",
            Self::BrassKey => "黄铜小钥匙",
            Self::LanternGlass => "雾灯玻璃",
            Self::OldTimetable => "烧焦的旧时刻表",
            Self::StationLog => "站务日志",
            Self::NameTag => "裂开的姓名牌",
            Self::SignalWhistle => "银色发车哨",
            Self::StationMap => "折叠站内图",
            Self::ChildHomework => "没有封面的作业本",
            Self::BroadcastTape => "广播室磁带",
            Self::ConductorRoster => "列车员名册",
            Self::MirrorShard => "候车厅镜片",
            Self::CoinToken => "退票铜筹",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Flag {
    ExaminedTicket,
    ReadDepartureBoard,
    TravelerTrusted,
    ClerkMet,
    TicketRewritten,
    SearchedLostFound,
    OpenedCabinet,
    ReadStationLog,
    HeardUnderpassEcho,
    RecoveredName,
    RepairedFogLamp,
    MetChild,
    ReturnedNameTag,
    ChildJoined,
    HeardClockTruth,
    AlignedClock,
    InspectedRails,
    SummonedConductor,
    FinalTrainArrived,
    FoundStationMap,
    ReadPlatformLedger,
    HeardBroadcastTape,
    FoundRoster,
    FoundMirrorShard,
    FoundCoinToken,
    UnderstoodFirstLoop,
    UnderstoodChildPromise,
    UnderstoodStationMechanism,
    SynthesizedRoute,
    SynthesizedChildTruth,
    SynthesizedStationTruth,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TicketKind {
    WetWarning,
    Return,
}

impl TicketKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::WetWarning => "写着“别上车”的湿票",
            Self::Return => "雾灯号返程联票",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Ending {
    LostPassenger,
    EscapedAlone,
    NewStationKeeper,
    BurnedTimetable,
    TookChildHome,
    BecameTheVoice,
}

impl Ending {
    pub const ALL: [Self; 6] = [
        Self::LostPassenger,
        Self::EscapedAlone,
        Self::NewStationKeeper,
        Self::BurnedTimetable,
        Self::TookChildHome,
        Self::BecameTheVoice,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::LostPassenger => "结局：遗失姓名的旅客",
            Self::EscapedAlone => "结局：单程逃离",
            Self::NewStationKeeper => "结局：新任站务员",
            Self::BurnedTimetable => "结局：烧掉时刻表",
            Self::TookChildHome => "结局：带孩子返程",
            Self::BecameTheVoice => "结局：广播员",
        }
    }

    pub fn short_title(self) -> &'static str {
        match self {
            Self::LostPassenger => "遗失姓名的旅客",
            Self::EscapedAlone => "单程逃离",
            Self::NewStationKeeper => "新任站务员",
            Self::BurnedTimetable => "烧掉时刻表",
            Self::TookChildHome => "带孩子返程",
            Self::BecameTheVoice => "广播员",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EndingPreludeId {
    AloneDoor,
    ChildWhiteLine,
    TimetablePyre,
    BroadcastBooth,
    KeeperCoat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EndingPreludeResponseId {
    AcceptCost,
    ReturnChoice,
    RefuseControl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FinalDebateId {
    AloneTraveler,
    ChildWhiteLine,
    TimetableClerk,
    BroadcastVoice,
    KeeperCoat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FinalInterviewId {
    TravelerEmptySeat,
    ClerkAfterRules,
    ChildOrdinaryTomorrow,
    KeeperBroadcastRoom,
    KeeperCoatBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum FinalDebateResponseId {
    Insist,
    AdmitWound,
    RewritePromise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TopicId {
    TravelerRain,
    TravelerKey,
    TravelerTicket,
    TravelerLeaving,
    TravelerChild,
    TravelerMercy,
    ClerkDestination,
    ClerkOneWay,
    ClerkLog,
    ClerkSeats,
    ClerkName,
    ClerkPrice,
    LostFoundLabels,
    LostFoundCabinet,
    LostFoundLantern,
    LostFoundNameTag,
    LostFoundApology,
    LostFoundLedger,
    UnderpassEcho,
    UnderpassWaterline,
    UnderpassExit,
    UnderpassLoop,
    UnderpassBroadcast,
    UnderpassStairs,
    ChildWhiteLine,
    ChildHomework,
    ChildPromise,
    ChildAnger,
    ChildTomorrow,
    ChildLeaving,
    KeeperClock,
    KeeperLantern,
    KeeperTimetable,
    KeeperBroadcast,
    KeeperStay,
    KeeperCoat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EvidenceId {
    TravelerMirror,
    TravelerCoinToken,
    TravelerRoster,
    ClerkCoinToken,
    ClerkRoster,
    ClerkMirror,
    ChildTicket,
    ChildHomework,
    ChildMirror,
    KeeperStationLog,
    KeeperBroadcastTape,
    KeeperMirror,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CaseFileId {
    WetTicketProtocol,
    ReturnProtocol,
    ChildWitness,
    BorrowedMinute,
    BroadcastDoor,
    KeeperContract,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CaseDialogueId {
    WetTicketTraveler,
    ReturnProtocolClerk,
    ChildWitnessChild,
    BorrowedMinuteKeeper,
    BroadcastDoorKeeper,
    KeeperContractClerk,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StationRequestId {
    NewspaperCorrection,
    RefundLedger,
    HomeworkEnvelope,
    LampMaintenance,
    LastNotice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ResonanceId {
    RainInTheMirror,
    TwoReservedSeats,
    WhiteLineHomework,
    BorrowedMinute,
    BroadcastAfterimage,
    DebtOfKeepingWatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VowId {
    ReadTheWholeWarning,
    DoNotOwnTheChild,
    ReturnWithoutErasing,
    TruthBeforeMercy,
    LightWithoutDebt,
    OrdinaryTomorrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum MemoryId {
    SeventhBench,
    TicketWindowReflection,
    RaincoatPocket,
    EvacuationLine,
    BorrowedClockMinute,
    WhiteLineMeasure,
    BroadcastPractice,
    OrdinaryKitchen,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PatrolId {
    WaitingHallManifest,
    TicketWindowQueue,
    LostFoundShelfAudit,
    UnderpassWaterline,
    ClockTowerMinuteHand,
    PlatformBoundary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AftertalkId {
    TravelerSecondSeat,
    TravelerPatrolManifest,
    ClerkRefundQueue,
    LostFoundNamedShelf,
    UnderpassMeasuredEcho,
    ChildRedrawnLine,
    ChildDepartureSeat,
    KeeperMinuteHand,
    KeeperBroadcastReply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueLeadId {
    RainUnderBench,
    EmptySeatLedger,
    ReturnStub,
    TwoSeatMap,
    HomeworkMargin,
    WhiteLineChalk,
    MinuteHandNote,
    BroadcastDraft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueRelayId {
    MirrorToChild,
    EmptySeatToClerk,
    ReturnStubToTraveler,
    TwoSeatMapToChild,
    HomeworkMarginToTraveler,
    WhiteLineChalkToKeeper,
    MinuteHandNoteToClerk,
    BroadcastDraftToTraveler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CompanionTalkId {
    WaitingHallEmptySeat,
    WaitingHallDepartureBoard,
    TicketOfficeTwoTickets,
    TicketOfficePriceQuestion,
    LostFoundNamedBox,
    LostFoundRaincoatSleeve,
    UnderpassEchoStep,
    UnderpassStairsTomorrow,
    ClockTowerBorrowedMinute,
    ClockTowerRulesForLight,
    PlatformWhiteLineTogether,
    PlatformDoorQuestion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LampFocusId {
    WaitingHallBenchTrace,
    TicketOfficeReturnGrid,
    LostFoundLabelShadow,
    UnderpassWaterScript,
    ClockTowerMinuteDebt,
    PlatformBrakeLight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum StationWhisperId {
    WaitingHallUmbrellaCount,
    WaitingHallBenchReturn,
    TicketOfficeStampHumidity,
    TicketOfficePriceList,
    LostFoundUmbrellaNames,
    LostFoundApologyBox,
    UnderpassReverseFootsteps,
    UnderpassSaltLine,
    ClockTowerGearPrayer,
    ClockTowerCoatShadow,
    PlatformBrakeDust,
    PlatformSuitcaseLine,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RouteCostId {
    AloneEmptySeat,
    ChildUnforgivenTomorrow,
    BurnedTimetableAftercare,
    BroadcastSecondName,
    KeeperLightBoundary,
    LostPassengerNotice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoutePressureId {
    AloneTraveler,
    ChildTomorrow,
    TimetableClerk,
    BroadcastKeeper,
    KeeperDuty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RouteWitnessId {
    AloneSeatNotice,
    ChildWindowSeat,
    TimetableAshList,
    BroadcastDryRun,
    KeeperBoundaryLamp,
    LastNoticeTicket,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RouteWitnessDebriefId {
    AloneSeatTraveler,
    ChildSeatChild,
    AshListClerk,
    BroadcastDryRunKeeper,
    BoundaryLampKeeper,
    LastNoticeTraveler,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RoutePressureResponseId {
    DefendRoute,
    AdmitRisk,
    RevisePromise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum TruthSceneId {
    WetTicketWarning,
    ChildIsNotCargo,
    ReturnTicketSignature,
    StationFeedsOnLastMinute,
    BroadcastIsAPerson,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnomalyId {
    ScreenKeepsScore,
    RefundStampede,
    RisingWaterline,
    WhiteLineDrift,
    StalledMinute,
    BroadcastFeedback,
    BrakeLightTrial,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnomalyResponse {
    Stabilize,
    Follow,
}

impl AnomalyResponse {
    pub fn name(self) -> &'static str {
        match self {
            Self::Stabilize => "稳住异象",
            Self::Follow => "追随异象",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DepartureId {
    SingleReturnPocket,
    ChildWindowSeat,
    TimetableMatch,
    BroadcastScript,
    KeeperLedger,
    LastNoticeOnPlatform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DialogueTone {
    Listening,
    Gentle,
    Direct,
}

impl DialogueTone {
    pub fn name(self) -> &'static str {
        match self {
            Self::Listening => "先听完沉默",
            Self::Gentle => "把问题放轻",
            Self::Direct => "直接逼近真相",
        }
    }
}

impl Default for DialogueTone {
    fn default() -> Self {
        Self::Listening
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueId {
    Traveler,
    Clerk,
    Child,
    Keeper,
}

impl DialogueId {
    pub fn title(self) -> &'static str {
        match self {
            Self::Traveler => "候车厅老人",
            Self::Clerk => "售票员",
            Self::Child => "白线后的孩子",
            Self::Keeper => "站务员",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueNodeId {
    Root,
    Memory,
    Proof,
    Tomorrow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveDialogue {
    pub dialogue: DialogueId,
    pub node: DialogueNodeId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueChoiceId {
    TravelerRain,
    TravelerEmptySeat,
    TravelerLoop,
    ClerkTicket,
    ClerkReturnRule,
    ClerkTomorrowPrice,
    ChildWhiteLine,
    ChildAnger,
    ChildTomorrowBag,
    KeeperDuty,
    KeeperBroadcast,
    KeeperCoat,
    DeepenTopic,
    ChallengeTopic,
    PromiseTopic,
    BackToRoot,
    Leave,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueQuestionId {
    TravelerAboutChild,
    TravelerAboutSecondSeat,
    ClerkAboutWetTicket,
    ClerkAboutName,
    ChildAboutTraveler,
    ChildAboutLeaving,
    KeeperAboutBroadcastGap,
    KeeperAboutStaying,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueChallengeId {
    TravelerAsksWhyYouKeptTheSeat,
    ClerkAsksWhoPaysForReturn,
    ChildAsksIfYouWillLeaveAgain,
    KeeperAsksIfStayingIsMercy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DialogueChallengeResponseId {
    Admit,
    Deflect,
    Promise,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DialogueBeatKey {
    pub dialogue: DialogueId,
    pub node: DialogueNodeId,
    pub choice: DialogueChoiceId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueTranscriptEntry {
    pub dialogue: DialogueId,
    pub node: DialogueNodeId,
    pub choice: Option<DialogueChoiceId>,
    pub title: String,
    pub body: String,
}

impl DialogueTranscriptEntry {
    pub fn new(
        dialogue: DialogueId,
        node: DialogueNodeId,
        choice: Option<DialogueChoiceId>,
        title: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            dialogue,
            node,
            choice,
            title: title.into(),
            body: body.into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionId {
    Move(Location),
    BeginDialogue(DialogueId),
    ChooseDialogue(DialogueChoiceId),
    AskDialogueQuestion(DialogueQuestionId),
    AnswerDialogueChallenge(DialogueChallengeId, DialogueChallengeResponseId),
    Discuss(TopicId),
    PresentEvidence(EvidenceId),
    ResolveCaseFile(CaseFileId),
    DiscussCaseDialogue(CaseDialogueId),
    CompleteRequest(StationRequestId),
    ResolveResonance(ResonanceId),
    MakeVow(VowId),
    EnterMemory(MemoryId),
    TakePatrol(PatrolId),
    FollowUpDialogue(AftertalkId),
    FollowDialogueLead(DialogueLeadId),
    ReturnDialogueLead(DialogueLeadId),
    RelayDialogueLead(DialogueRelayId),
    ReflectDialogueRelay(DialogueRelayId),
    EchoDialogueRelay(DialogueRelayId),
    AnchorDialogueRelay(DialogueRelayId),
    ReviewDialogueAnchor(DialogueRelayId),
    CompanionDialogue(CompanionTalkId),
    FocusFogLamp(LampFocusId),
    ListenStationWhisper(StationWhisperId),
    HandleAnomaly(AnomalyId, AnomalyResponse),
    PrepareDeparture(DepartureId),
    RehearseRoute(DepartureId),
    MitigateRouteCost(RouteCostId),
    DiscussRouteEcho(RouteCostId),
    VisitRouteWitness(RouteWitnessId),
    DebriefRouteWitness(RouteWitnessDebriefId),
    AnswerRoutePressure(RoutePressureId, RoutePressureResponseId),
    HoldFinalInterview(FinalInterviewId),
    RevealTruth(TruthSceneId),
    SetDialogueTone(DialogueTone),
    ExamineTicket,
    StudyTicket,
    ShowTicketToTraveler,
    InvestigateLocation,
    ReadDepartureBoard,
    TalkTraveler,
    TalkClerk,
    ShowLogToClerk,
    RewriteTicket,
    SearchLostFound,
    OpenCabinet,
    ListenUnderpass,
    RepairFogLamp,
    MeetChild,
    ShowNameTagToChild,
    ReturnNameTag,
    TalkStationKeeper,
    ShowTimetableToKeeper,
    AlignClock,
    InspectRails,
    BlowWhistle,
    SynthesizeClues,
    Wait,
    EnterEndingPrelude(EndingPreludeId),
    AnswerEndingPrelude(EndingPreludeId, EndingPreludeResponseId),
    AnswerFinalDebate(FinalDebateId, FinalDebateResponseId),
    BoardAlone,
    BoardWithChild,
    BurnTimetable,
    BroadcastName,
    TakeKeeperSeat,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameMode {
    Title,
    Playing,
    Ending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InfoPanel {
    Intel,
    Routes,
    Cases,
    Inventory,
    Log,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoryEvent {
    pub title: String,
    pub body: String,
    pub tags: Vec<String>,
}

impl StoryEvent {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            tags: Vec::new(),
        }
    }

    pub fn tagged(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    pub fn tag(self, tag: impl Into<String>) -> Self {
        self.tagged(tag)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameState {
    pub location: Location,
    pub actions_used: u8,
    pub loop_count: u8,
    pub ticket: TicketKind,
    pub inventory: BTreeSet<Item>,
    pub flags: BTreeSet<Flag>,
    #[serde(default)]
    pub discussed_topics: BTreeSet<TopicId>,
    #[serde(default)]
    pub presented_evidence: BTreeSet<EvidenceId>,
    #[serde(default)]
    pub resolved_case_files: BTreeSet<CaseFileId>,
    #[serde(default)]
    pub completed_case_dialogues: BTreeSet<CaseDialogueId>,
    #[serde(default)]
    pub completed_requests: BTreeSet<StationRequestId>,
    #[serde(default)]
    pub resolved_resonances: BTreeSet<ResonanceId>,
    #[serde(default)]
    pub chosen_vows: BTreeSet<VowId>,
    #[serde(default)]
    pub visited_memories: BTreeSet<MemoryId>,
    #[serde(default)]
    pub completed_patrols: BTreeSet<PatrolId>,
    #[serde(default)]
    pub completed_aftertalks: BTreeSet<AftertalkId>,
    #[serde(default)]
    pub completed_dialogue_leads: BTreeSet<DialogueLeadId>,
    #[serde(default)]
    pub returned_dialogue_leads: BTreeSet<DialogueLeadId>,
    #[serde(default)]
    pub completed_dialogue_relays: BTreeSet<DialogueRelayId>,
    #[serde(default)]
    pub reflected_dialogue_relays: BTreeSet<DialogueRelayId>,
    #[serde(default)]
    pub echoed_dialogue_relays: BTreeSet<DialogueRelayId>,
    #[serde(default)]
    pub anchored_dialogue_relays: BTreeSet<DialogueRelayId>,
    #[serde(default)]
    pub reviewed_dialogue_anchors: BTreeSet<DialogueRelayId>,
    #[serde(default)]
    pub completed_companion_talks: BTreeSet<CompanionTalkId>,
    #[serde(default)]
    pub focused_lamp_traces: BTreeSet<LampFocusId>,
    #[serde(default)]
    pub heard_station_whispers: BTreeSet<StationWhisperId>,
    #[serde(default)]
    pub resolved_anomalies: BTreeMap<AnomalyId, AnomalyResponse>,
    #[serde(default)]
    pub prepared_departures: BTreeSet<DepartureId>,
    #[serde(default)]
    pub rehearsed_departures: BTreeSet<DepartureId>,
    #[serde(default)]
    pub mitigated_route_costs: BTreeSet<RouteCostId>,
    #[serde(default)]
    pub completed_route_echoes: BTreeSet<RouteCostId>,
    #[serde(default)]
    pub visited_route_witnesses: BTreeSet<RouteWitnessId>,
    #[serde(default)]
    pub completed_route_witness_debriefs: BTreeSet<RouteWitnessDebriefId>,
    #[serde(default)]
    pub answered_route_pressures: BTreeMap<RoutePressureId, RoutePressureResponseId>,
    #[serde(default)]
    pub revealed_truth_scenes: BTreeSet<TruthSceneId>,
    #[serde(default)]
    pub completed_ending_preludes: BTreeSet<EndingPreludeId>,
    #[serde(default)]
    pub ending_prelude_responses: BTreeMap<EndingPreludeId, EndingPreludeResponseId>,
    #[serde(default)]
    pub final_debate_responses: BTreeMap<FinalDebateId, FinalDebateResponseId>,
    #[serde(default)]
    pub completed_final_interviews: BTreeSet<FinalInterviewId>,
    #[serde(default)]
    pub dialogue_tone: DialogueTone,
    #[serde(default)]
    pub completed_dialogue_beats: BTreeSet<DialogueBeatKey>,
    #[serde(default)]
    pub answered_dialogue_questions: BTreeSet<DialogueQuestionId>,
    #[serde(default)]
    pub answered_dialogue_challenges: BTreeMap<DialogueChallengeId, DialogueChallengeResponseId>,
    #[serde(default)]
    pub dialogue_transcript: Vec<DialogueTranscriptEntry>,
    #[serde(default)]
    pub active_dialogue: Option<ActiveDialogue>,
    #[serde(default)]
    pub location_depths: [u8; 6],
    #[serde(default)]
    pub traveler_depth: u8,
    #[serde(default)]
    pub clerk_depth: u8,
    #[serde(default)]
    pub child_depth: u8,
    #[serde(default)]
    pub keeper_depth: u8,
    #[serde(default)]
    pub synthesis_depth: u8,
    pub child_trust: i8,
    pub clerk_trust: i8,
    pub keeper_trust: i8,
    pub ended: Option<Ending>,
}

impl GameState {
    pub fn new() -> Self {
        let mut inventory = BTreeSet::new();
        inventory.insert(Item::WetTicket);
        Self {
            location: Location::WaitingHall,
            actions_used: 0,
            loop_count: 0,
            ticket: TicketKind::WetWarning,
            inventory,
            flags: BTreeSet::new(),
            discussed_topics: BTreeSet::new(),
            presented_evidence: BTreeSet::new(),
            resolved_case_files: BTreeSet::new(),
            completed_case_dialogues: BTreeSet::new(),
            completed_requests: BTreeSet::new(),
            resolved_resonances: BTreeSet::new(),
            chosen_vows: BTreeSet::new(),
            visited_memories: BTreeSet::new(),
            completed_patrols: BTreeSet::new(),
            completed_aftertalks: BTreeSet::new(),
            completed_dialogue_leads: BTreeSet::new(),
            returned_dialogue_leads: BTreeSet::new(),
            completed_dialogue_relays: BTreeSet::new(),
            reflected_dialogue_relays: BTreeSet::new(),
            echoed_dialogue_relays: BTreeSet::new(),
            anchored_dialogue_relays: BTreeSet::new(),
            reviewed_dialogue_anchors: BTreeSet::new(),
            completed_companion_talks: BTreeSet::new(),
            focused_lamp_traces: BTreeSet::new(),
            heard_station_whispers: BTreeSet::new(),
            resolved_anomalies: BTreeMap::new(),
            prepared_departures: BTreeSet::new(),
            rehearsed_departures: BTreeSet::new(),
            mitigated_route_costs: BTreeSet::new(),
            completed_route_echoes: BTreeSet::new(),
            visited_route_witnesses: BTreeSet::new(),
            completed_route_witness_debriefs: BTreeSet::new(),
            answered_route_pressures: BTreeMap::new(),
            revealed_truth_scenes: BTreeSet::new(),
            completed_ending_preludes: BTreeSet::new(),
            ending_prelude_responses: BTreeMap::new(),
            final_debate_responses: BTreeMap::new(),
            completed_final_interviews: BTreeSet::new(),
            dialogue_tone: DialogueTone::default(),
            completed_dialogue_beats: BTreeSet::new(),
            answered_dialogue_questions: BTreeSet::new(),
            answered_dialogue_challenges: BTreeMap::new(),
            dialogue_transcript: Vec::new(),
            active_dialogue: None,
            location_depths: [0; 6],
            traveler_depth: 0,
            clerk_depth: 0,
            child_depth: 0,
            keeper_depth: 0,
            synthesis_depth: 0,
            child_trust: 0,
            clerk_trust: 0,
            keeper_trust: 0,
            ended: None,
        }
    }

    pub fn has_flag(&self, flag: Flag) -> bool {
        self.flags.contains(&flag)
    }

    pub fn remember(&mut self, flag: Flag) -> bool {
        self.flags.insert(flag)
    }

    pub fn has_discussed(&self, topic: TopicId) -> bool {
        self.discussed_topics.contains(&topic)
    }

    pub fn discuss(&mut self, topic: TopicId) -> bool {
        self.discussed_topics.insert(topic)
    }

    pub fn has_presented(&self, evidence: EvidenceId) -> bool {
        self.presented_evidence.contains(&evidence)
    }

    pub fn present(&mut self, evidence: EvidenceId) -> bool {
        self.presented_evidence.insert(evidence)
    }

    pub fn has_resolved_case_file(&self, case_file: CaseFileId) -> bool {
        self.resolved_case_files.contains(&case_file)
    }

    pub fn resolve_case_file(&mut self, case_file: CaseFileId) -> bool {
        self.resolved_case_files.insert(case_file)
    }

    pub fn has_completed_case_dialogue(&self, dialogue: CaseDialogueId) -> bool {
        self.completed_case_dialogues.contains(&dialogue)
    }

    pub fn complete_case_dialogue(&mut self, dialogue: CaseDialogueId) -> bool {
        self.completed_case_dialogues.insert(dialogue)
    }

    pub fn has_completed_request(&self, request: StationRequestId) -> bool {
        self.completed_requests.contains(&request)
    }

    pub fn complete_request(&mut self, request: StationRequestId) -> bool {
        self.completed_requests.insert(request)
    }

    pub fn has_resolved_resonance(&self, resonance: ResonanceId) -> bool {
        self.resolved_resonances.contains(&resonance)
    }

    pub fn resolve_resonance(&mut self, resonance: ResonanceId) -> bool {
        self.resolved_resonances.insert(resonance)
    }

    pub fn has_vow(&self, vow: VowId) -> bool {
        self.chosen_vows.contains(&vow)
    }

    pub fn make_vow(&mut self, vow: VowId) -> bool {
        self.chosen_vows.insert(vow)
    }

    pub fn has_memory(&self, memory: MemoryId) -> bool {
        self.visited_memories.contains(&memory)
    }

    pub fn visit_memory(&mut self, memory: MemoryId) -> bool {
        self.visited_memories.insert(memory)
    }

    pub fn has_completed_patrol(&self, patrol: PatrolId) -> bool {
        self.completed_patrols.contains(&patrol)
    }

    pub fn complete_patrol(&mut self, patrol: PatrolId) -> bool {
        self.completed_patrols.insert(patrol)
    }

    pub fn has_completed_aftertalk(&self, aftertalk: AftertalkId) -> bool {
        self.completed_aftertalks.contains(&aftertalk)
    }

    pub fn complete_aftertalk(&mut self, aftertalk: AftertalkId) -> bool {
        self.completed_aftertalks.insert(aftertalk)
    }

    pub fn has_completed_dialogue_lead(&self, lead: DialogueLeadId) -> bool {
        self.completed_dialogue_leads.contains(&lead)
    }

    pub fn complete_dialogue_lead(&mut self, lead: DialogueLeadId) -> bool {
        self.completed_dialogue_leads.insert(lead)
    }

    pub fn has_returned_dialogue_lead(&self, lead: DialogueLeadId) -> bool {
        self.returned_dialogue_leads.contains(&lead)
    }

    pub fn return_dialogue_lead(&mut self, lead: DialogueLeadId) -> bool {
        self.returned_dialogue_leads.insert(lead)
    }

    pub fn has_completed_dialogue_relay(&self, relay: DialogueRelayId) -> bool {
        self.completed_dialogue_relays.contains(&relay)
    }

    pub fn complete_dialogue_relay(&mut self, relay: DialogueRelayId) -> bool {
        self.completed_dialogue_relays.insert(relay)
    }

    pub fn has_reflected_dialogue_relay(&self, relay: DialogueRelayId) -> bool {
        self.reflected_dialogue_relays.contains(&relay)
    }

    pub fn reflect_dialogue_relay(&mut self, relay: DialogueRelayId) -> bool {
        self.reflected_dialogue_relays.insert(relay)
    }

    pub fn has_echoed_dialogue_relay(&self, relay: DialogueRelayId) -> bool {
        self.echoed_dialogue_relays.contains(&relay)
    }

    pub fn echo_dialogue_relay(&mut self, relay: DialogueRelayId) -> bool {
        self.echoed_dialogue_relays.insert(relay)
    }

    pub fn has_anchored_dialogue_relay(&self, relay: DialogueRelayId) -> bool {
        self.anchored_dialogue_relays.contains(&relay)
    }

    pub fn anchor_dialogue_relay(&mut self, relay: DialogueRelayId) -> bool {
        self.anchored_dialogue_relays.insert(relay)
    }

    pub fn has_reviewed_dialogue_anchor(&self, relay: DialogueRelayId) -> bool {
        self.reviewed_dialogue_anchors.contains(&relay)
    }

    pub fn review_dialogue_anchor(&mut self, relay: DialogueRelayId) -> bool {
        self.reviewed_dialogue_anchors.insert(relay)
    }

    pub fn has_completed_companion_talk(&self, talk: CompanionTalkId) -> bool {
        self.completed_companion_talks.contains(&talk)
    }

    pub fn complete_companion_talk(&mut self, talk: CompanionTalkId) -> bool {
        self.completed_companion_talks.insert(talk)
    }

    pub fn has_focused_lamp_trace(&self, trace: LampFocusId) -> bool {
        self.focused_lamp_traces.contains(&trace)
    }

    pub fn focus_lamp_trace(&mut self, trace: LampFocusId) -> bool {
        self.focused_lamp_traces.insert(trace)
    }

    pub fn has_heard_station_whisper(&self, whisper: StationWhisperId) -> bool {
        self.heard_station_whispers.contains(&whisper)
    }

    pub fn hear_station_whisper(&mut self, whisper: StationWhisperId) -> bool {
        self.heard_station_whispers.insert(whisper)
    }

    pub fn has_resolved_anomaly(&self, anomaly: AnomalyId) -> bool {
        self.resolved_anomalies.contains_key(&anomaly)
    }

    pub fn anomaly_response(&self, anomaly: AnomalyId) -> Option<AnomalyResponse> {
        self.resolved_anomalies.get(&anomaly).copied()
    }

    pub fn resolve_anomaly(&mut self, anomaly: AnomalyId, response: AnomalyResponse) -> bool {
        self.resolved_anomalies.insert(anomaly, response).is_none()
    }

    pub fn has_prepared_departure(&self, departure: DepartureId) -> bool {
        self.prepared_departures.contains(&departure)
    }

    pub fn prepare_departure(&mut self, departure: DepartureId) -> bool {
        self.prepared_departures.insert(departure)
    }

    pub fn has_rehearsed_departure(&self, departure: DepartureId) -> bool {
        self.rehearsed_departures.contains(&departure)
    }

    pub fn rehearse_departure(&mut self, departure: DepartureId) -> bool {
        self.rehearsed_departures.insert(departure)
    }

    pub fn has_mitigated_route_cost(&self, cost: RouteCostId) -> bool {
        self.mitigated_route_costs.contains(&cost)
    }

    pub fn mitigate_route_cost(&mut self, cost: RouteCostId) -> bool {
        self.mitigated_route_costs.insert(cost)
    }

    pub fn has_completed_route_echo(&self, cost: RouteCostId) -> bool {
        self.completed_route_echoes.contains(&cost)
    }

    pub fn complete_route_echo(&mut self, cost: RouteCostId) -> bool {
        self.completed_route_echoes.insert(cost)
    }

    pub fn has_visited_route_witness(&self, witness: RouteWitnessId) -> bool {
        self.visited_route_witnesses.contains(&witness)
    }

    pub fn visit_route_witness(&mut self, witness: RouteWitnessId) -> bool {
        self.visited_route_witnesses.insert(witness)
    }

    pub fn has_completed_route_witness_debrief(&self, debrief: RouteWitnessDebriefId) -> bool {
        self.completed_route_witness_debriefs.contains(&debrief)
    }

    pub fn complete_route_witness_debrief(&mut self, debrief: RouteWitnessDebriefId) -> bool {
        self.completed_route_witness_debriefs.insert(debrief)
    }

    pub fn route_pressure_response(
        &self,
        pressure: RoutePressureId,
    ) -> Option<RoutePressureResponseId> {
        self.answered_route_pressures.get(&pressure).copied()
    }

    pub fn has_answered_route_pressure(&self, pressure: RoutePressureId) -> bool {
        self.route_pressure_response(pressure).is_some()
    }

    pub fn answer_route_pressure(
        &mut self,
        pressure: RoutePressureId,
        response: RoutePressureResponseId,
    ) -> bool {
        self.answered_route_pressures
            .insert(pressure, response)
            .is_none()
    }

    pub fn has_revealed_truth_scene(&self, truth: TruthSceneId) -> bool {
        self.revealed_truth_scenes.contains(&truth)
    }

    pub fn reveal_truth_scene(&mut self, truth: TruthSceneId) -> bool {
        self.revealed_truth_scenes.insert(truth)
    }

    pub fn has_completed_ending_prelude(&self, prelude: EndingPreludeId) -> bool {
        self.completed_ending_preludes.contains(&prelude)
    }

    pub fn complete_ending_prelude(&mut self, prelude: EndingPreludeId) -> bool {
        self.completed_ending_preludes.insert(prelude)
    }

    pub fn ending_prelude_response(
        &self,
        prelude: EndingPreludeId,
    ) -> Option<EndingPreludeResponseId> {
        self.ending_prelude_responses.get(&prelude).copied()
    }

    pub fn has_answered_ending_prelude(&self, prelude: EndingPreludeId) -> bool {
        self.ending_prelude_response(prelude).is_some()
    }

    pub fn answer_ending_prelude(
        &mut self,
        prelude: EndingPreludeId,
        response: EndingPreludeResponseId,
    ) -> bool {
        self.ending_prelude_responses
            .insert(prelude, response)
            .is_none()
    }

    pub fn final_debate_response(&self, debate: FinalDebateId) -> Option<FinalDebateResponseId> {
        self.final_debate_responses.get(&debate).copied()
    }

    pub fn has_answered_final_debate(&self, debate: FinalDebateId) -> bool {
        self.final_debate_response(debate).is_some()
    }

    pub fn answer_final_debate(
        &mut self,
        debate: FinalDebateId,
        response: FinalDebateResponseId,
    ) -> bool {
        self.final_debate_responses
            .insert(debate, response)
            .is_none()
    }

    pub fn has_completed_final_interview(&self, interview: FinalInterviewId) -> bool {
        self.completed_final_interviews.contains(&interview)
    }

    pub fn complete_final_interview(&mut self, interview: FinalInterviewId) -> bool {
        self.completed_final_interviews.insert(interview)
    }

    pub fn has_completed_dialogue_beat(&self, beat: DialogueBeatKey) -> bool {
        self.completed_dialogue_beats.contains(&beat)
    }

    pub fn complete_dialogue_beat(&mut self, beat: DialogueBeatKey) -> bool {
        self.completed_dialogue_beats.insert(beat)
    }

    pub fn has_answered_dialogue_question(&self, question: DialogueQuestionId) -> bool {
        self.answered_dialogue_questions.contains(&question)
    }

    pub fn answer_dialogue_question(&mut self, question: DialogueQuestionId) -> bool {
        self.answered_dialogue_questions.insert(question)
    }

    pub fn dialogue_challenge_response(
        &self,
        challenge: DialogueChallengeId,
    ) -> Option<DialogueChallengeResponseId> {
        self.answered_dialogue_challenges.get(&challenge).copied()
    }

    pub fn has_answered_dialogue_challenge(&self, challenge: DialogueChallengeId) -> bool {
        self.answered_dialogue_challenges.contains_key(&challenge)
    }

    pub fn answer_dialogue_challenge(
        &mut self,
        challenge: DialogueChallengeId,
        response: DialogueChallengeResponseId,
    ) -> bool {
        self.answered_dialogue_challenges
            .insert(challenge, response)
            .is_none()
    }

    pub fn record_dialogue_line(&mut self, entry: DialogueTranscriptEntry) {
        self.dialogue_transcript.push(entry);
        if self.dialogue_transcript.len() > MAX_DIALOGUE_TRANSCRIPT {
            let overflow = self.dialogue_transcript.len() - MAX_DIALOGUE_TRANSCRIPT;
            self.dialogue_transcript.drain(0..overflow);
        }
    }

    pub fn has_item(&self, item: Item) -> bool {
        self.inventory.contains(&item)
    }

    pub fn add_item(&mut self, item: Item) -> bool {
        self.inventory.insert(item)
    }

    pub fn remove_item(&mut self, item: Item) -> bool {
        self.inventory.remove(&item)
    }

    pub fn elapsed_minutes(&self) -> u8 {
        self.actions_used
    }

    pub fn time_left(&self) -> u8 {
        MAX_NIGHT_MINUTES.saturating_sub(self.actions_used)
    }

    pub fn clock_text(&self) -> String {
        let minute_of_day = (START_CLOCK_MINUTES + u16::from(self.actions_used)) % (24 * 60);
        format!("{:02}:{:02}", minute_of_day / 60, minute_of_day % 60)
    }

    pub fn current_segment(&self) -> u8 {
        (self.actions_used / MINUTES_PER_SEGMENT)
            .saturating_add(1)
            .min(MAX_SEGMENTS)
    }

    pub fn investigation_depth(&self, location: Location) -> u8 {
        self.location_depths[location.index()]
    }

    pub fn advance_investigation(&mut self, location: Location) -> u8 {
        let index = location.index();
        let current = self.location_depths[index];
        self.location_depths[index] = (current + 1).min(LOCATION_INVESTIGATION_STEPS);
        current
    }

    pub fn advance_time(&mut self, minutes: u8) {
        self.actions_used = self
            .actions_used
            .saturating_add(minutes)
            .min(MAX_NIGHT_MINUTES);
        self.loop_count = self.actions_used / MINUTES_PER_SEGMENT;
    }

    pub fn final_train_due(&self) -> bool {
        self.actions_used >= MAX_NIGHT_MINUTES || self.has_flag(Flag::FinalTrainArrived)
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameSession {
    pub mode: GameMode,
    pub state: GameState,
    pub log: Vec<StoryEvent>,
    #[serde(default)]
    pub endings_seen: BTreeSet<Ending>,
    pub active_panel: InfoPanel,
    #[serde(skip)]
    pub intel_scroll: f32,
    #[serde(skip)]
    pub routes_scroll: f32,
    #[serde(skip)]
    pub cases_scroll: f32,
    #[serde(skip)]
    pub inventory_scroll: f32,
    #[serde(skip)]
    pub log_scroll: f32,
    #[serde(skip)]
    pub story_scroll: f32,
    #[serde(skip)]
    pub action_scroll: f32,
    #[serde(skip)]
    pub notice: Option<String>,
}

impl GameSession {
    pub fn new() -> Self {
        Self {
            mode: GameMode::Title,
            state: GameState::new(),
            log: Vec::new(),
            endings_seen: BTreeSet::new(),
            active_panel: InfoPanel::Intel,
            intel_scroll: 0.0,
            routes_scroll: 0.0,
            cases_scroll: 0.0,
            inventory_scroll: 0.0,
            log_scroll: 0.0,
            story_scroll: 0.0,
            action_scroll: 0.0,
            notice: None,
        }
    }

    pub fn start_new_run(&mut self, opening: StoryEvent) {
        self.mode = GameMode::Playing;
        self.state = GameState::new();
        self.log.clear();
        self.log.push(opening);
        self.active_panel = InfoPanel::Intel;
        self.intel_scroll = 0.0;
        self.routes_scroll = 0.0;
        self.cases_scroll = 0.0;
        self.inventory_scroll = 0.0;
        self.log_scroll = 0.0;
        self.story_scroll = 0.0;
        self.action_scroll = 0.0;
        self.notice = None;
    }

    pub fn push_event(&mut self, event: StoryEvent) {
        self.log.push(event);
        self.story_scroll = 0.0;
        if self.log.len() > MAX_LOG_EVENTS {
            let overflow = self.log.len() - MAX_LOG_EVENTS;
            self.log.drain(0..overflow);
        }
    }

    pub fn latest(&self) -> Option<&StoryEvent> {
        self.log.last()
    }

    pub fn normalize_after_load(&mut self) {
        self.mode = if self.state.ended.is_some() {
            GameMode::Ending
        } else if self.log.is_empty() {
            GameMode::Title
        } else {
            GameMode::Playing
        };
        self.intel_scroll = 0.0;
        self.routes_scroll = 0.0;
        self.cases_scroll = 0.0;
        self.inventory_scroll = 0.0;
        self.log_scroll = 0.0;
        self.story_scroll = 0.0;
        self.action_scroll = 0.0;
        self.notice = None;
    }
}

impl Default for GameSession {
    fn default() -> Self {
        Self::new()
    }
}
