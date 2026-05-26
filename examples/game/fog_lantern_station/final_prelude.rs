use crate::model::{Ending, EndingPreludeId, EndingPreludeResponseId, Flag, GameState, StoryEvent};

pub const ENDING_PRELUDE_COUNT: usize = EndingPreludeId::ALL.len();

#[derive(Clone, Debug)]
pub struct EndingPreludeAction {
    pub prelude: EndingPreludeId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug)]
pub struct EndingPreludeResponseAction {
    pub prelude: EndingPreludeId,
    pub response: EndingPreludeResponseId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

pub fn available_preludes(state: &GameState) -> Vec<EndingPreludeAction> {
    EndingPreludeId::ALL
        .iter()
        .copied()
        .filter(|prelude| prelude.route_ready(state))
        .filter(|prelude| !state.has_completed_ending_prelude(*prelude))
        .map(|prelude| EndingPreludeAction {
            prelude,
            label: prelude.action_label().to_string(),
            detail: prelude.action_detail().to_string(),
            enabled: true,
        })
        .collect()
}

pub fn available_responses(state: &GameState) -> Vec<EndingPreludeResponseAction> {
    EndingPreludeId::ALL
        .iter()
        .copied()
        .filter(|prelude| prelude.route_ready(state))
        .filter(|prelude| state.has_completed_ending_prelude(*prelude))
        .filter(|prelude| !state.has_answered_ending_prelude(*prelude))
        .flat_map(|prelude| {
            EndingPreludeResponseId::ALL
                .iter()
                .copied()
                .map(move |response| EndingPreludeResponseAction {
                    prelude,
                    response,
                    label: response.label_for(prelude).to_string(),
                    detail: response.detail_for(prelude).to_string(),
                    enabled: true,
                })
        })
        .collect()
}

pub fn required_prelude(ending: Ending) -> Option<EndingPreludeId> {
    match ending {
        Ending::LostPassenger => None,
        Ending::EscapedAlone => Some(EndingPreludeId::AloneDoor),
        Ending::TookChildHome => Some(EndingPreludeId::ChildWhiteLine),
        Ending::BurnedTimetable => Some(EndingPreludeId::TimetablePyre),
        Ending::BecameTheVoice => Some(EndingPreludeId::BroadcastBooth),
        Ending::NewStationKeeper => Some(EndingPreludeId::KeeperCoat),
    }
}

pub fn route_ready_for_ending(state: &GameState, ending: Ending) -> bool {
    required_prelude(ending)
        .map(|prelude| prelude.route_ready(state))
        .unwrap_or(true)
}

pub fn final_choice_ready(state: &GameState, ending: Ending) -> bool {
    required_prelude(ending).map_or(true, |prelude| {
        prelude.route_ready(state)
            && state.has_completed_ending_prelude(prelude)
            && state.has_answered_ending_prelude(prelude)
    })
}

pub fn final_choice_detail(
    state: &GameState,
    ending: Ending,
    ready_detail: &'static str,
    missing_route_detail: &'static str,
) -> String {
    let Some(prelude) = required_prelude(ending) else {
        return ready_detail.to_string();
    };

    if !prelude.route_ready(state) {
        return missing_route_detail.to_string();
    }
    if !state.has_completed_ending_prelude(prelude) {
        return format!("先完成终局前场景：{}。", prelude.title());
    }
    if !state.has_answered_ending_prelude(prelude) {
        return format!("先回应最后一幕：{}。", prelude.title());
    }
    ready_detail.to_string()
}

pub fn missing_prelude_event(state: &GameState, ending: Ending) -> StoryEvent {
    let Some(prelude) = required_prelude(ending) else {
        return StoryEvent::new("还不能这样做", "这个选择没有对应的终局前场景。");
    };
    if state.has_completed_ending_prelude(prelude) && !state.has_answered_ending_prelude(prelude) {
        return StoryEvent::new(
            "还差最后一句回答",
            format!(
                "你已经经历“{}”，但还没有说出最后立场。先回应这幕，再作最终选择。",
                prelude.title()
            ),
        )
        .tag("终局前回应");
    }
    StoryEvent::new(
        "还差最后一幕",
        format!(
            "你已经具备这条结局的条件，但还没有真正面对它。先完成“{}”，再作最终选择。",
            prelude.title()
        ),
    )
    .tag("终局前场景")
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.completed_ending_preludes.is_empty() {
        return None;
    }

    let scenes = state
        .completed_ending_preludes
        .iter()
        .map(|prelude| prelude.title())
        .collect::<Vec<_>>();
    Some(format!(
        "最终选择前，你已经经历过最后一幕：{}。{}所以结局不是菜单上的按钮，而是你刚刚亲自走过并回应过的现场。",
        scenes.join("、"),
        ending_response_note(state)
    ))
}

pub fn ending_response_consequence(ending: Ending, state: &GameState) -> Option<String> {
    let prelude = required_prelude(ending)?;
    let response = state.ending_prelude_response(prelude)?;
    let note = match (prelude, response) {
        (EndingPreludeId::AloneDoor, EndingPreludeResponseId::AcceptCost) => {
            "你承认空座会留下，所以独自离开不再被写成干净的脱身。车窗里的 07B 没有消失，它像一枚小小的审判，提醒你自由也会占用空间。"
        }
        (EndingPreludeId::AloneDoor, EndingPreludeResponseId::ReturnChoice) => {
            "你没有替任何人坐下。车厢启动时，空座仍空着，却不再像惩罚；它更像一个终于归还给缺席者的选择。"
        }
        (EndingPreludeId::AloneDoor, EndingPreludeResponseId::RefuseControl) => {
            "你拒绝让车站替你命名逃离。广播叫你乘客，你在心里纠正它：不是乘客，是一个知道自己带走了什么的人。"
        }
        (EndingPreludeId::ChildWhiteLine, EndingPreludeResponseId::AcceptCost) => {
            "你承认那条命令来自你。孩子因此没有把跨线当成违抗，他把恐惧和车票一起收好，像收好一件终于被大人归还的事实。"
        }
        (EndingPreludeId::ChildWhiteLine, EndingPreludeResponseId::ReturnChoice) => {
            "你把最后一步交还给孩子。他自己跨过白线，所以到站以后，这段路不会被任何人说成你单方面完成的拯救。"
        }
        (EndingPreludeId::ChildWhiteLine, EndingPreludeResponseId::RefuseControl) => {
            "你拒绝把同行写成补偿。孩子坐在窗边，没有感谢你；他只是把窗帘拉开一点，允许明天先照到自己。"
        }
        (EndingPreludeId::TimetablePyre, EndingPreludeResponseId::AcceptCost) => {
            "你承认火会毁掉秩序。时刻表燃尽以后，有人短暂地慌了神，因为自由不是新的站名，而是没有人替他们排好的空白。"
        }
        (EndingPreludeId::TimetablePyre, EndingPreludeResponseId::ReturnChoice) => {
            "你把去处还给乘客。那些影子没有立刻奔向出口，他们先低头看自己的脚，像第一次发现目的地可以从脚下开始。"
        }
        (EndingPreludeId::TimetablePyre, EndingPreludeResponseId::RefuseControl) => {
            "你拒绝纸面慈悲。火光照见那些被规则安慰过也被规则困住过的人，他们终于能恨它，也能不再依赖它。"
        }
        (EndingPreludeId::BroadcastBooth, EndingPreludeResponseId::AcceptCost) => {
            "你接受自己会被误听。广播第一次响起时，仍有人把警告当作背景噪音，但你没有把沉默当成失败。"
        }
        (EndingPreludeId::BroadcastBooth, EndingPreludeResponseId::ReturnChoice) => {
            "你只播报，不命令。后来者听见的不是禁止，而是一句把选择交还给他们的话：请先确认自己为什么要上车。"
        }
        (EndingPreludeId::BroadcastBooth, EndingPreludeResponseId::RefuseControl) => {
            "你拒绝成为新规则。广播室因此没有诞生新的站务员，只留下一个会反复提醒、却不替任何人决定的声音。"
        }
        (EndingPreludeId::KeeperCoat, EndingPreludeResponseId::AcceptCost) => {
            "你承认留下也会腐烂。外套落在肩上时并不神圣，它只是沉；这份沉重反而让你不敢把守夜说成天然正确。"
        }
        (EndingPreludeId::KeeperCoat, EndingPreludeResponseId::ReturnChoice) => {
            "你把灯照向门口。后来每个旅客醒来，第一眼看见的不是规训，而是一条确实能离开的路。"
        }
        (EndingPreludeId::KeeperCoat, EndingPreludeResponseId::RefuseControl) => {
            "你拒绝替旅客决定。雾灯站仍需要守夜人，但从这一夜起，守夜人的第一职责是不把自己误认为命运。"
        }
    };
    Some(note.to_string())
}

pub fn ending_response_tag(ending: Ending, state: &GameState) -> Option<String> {
    let prelude = required_prelude(ending)?;
    let response = state.ending_prelude_response(prelude)?;
    Some(format!("终局回应：{}", response.name()))
}

pub fn enter(state: &mut GameState, prelude: EndingPreludeId) -> StoryEvent {
    if !state.final_train_due() {
        return StoryEvent::new(
            "列车还没有到站",
            "这不是可以提前排练的最后一幕。雾灯号必须先进站，选择才会显出重量。",
        )
        .tag("终局前场景");
    }

    if !prelude.route_ready(state) {
        return StoryEvent::new("最后一幕还搭不起来", prelude.missing_detail()).tag("终局前场景");
    }

    if state.has_completed_ending_prelude(prelude) {
        return StoryEvent::new(
            "最后一幕已经发生",
            format!("{}已经留在你身后。现在可以作最终选择。", prelude.title()),
        )
        .tag("终局前场景")
        .tag("复看");
    }

    state.complete_ending_prelude(prelude);
    let mut event = prelude.event();
    event
        .tags
        .push(format!("对应结局：{}", prelude.ending().short_title()));
    event
}

pub fn answer(
    state: &mut GameState,
    prelude: EndingPreludeId,
    response: EndingPreludeResponseId,
) -> StoryEvent {
    if !state.final_train_due() {
        return StoryEvent::new(
            "列车还没有到站",
            "最后回答不能提前写好。雾灯号必须先进站，你才知道自己在回答什么。",
        )
        .tag("终局前回应");
    }

    if !state.has_completed_ending_prelude(prelude) {
        return StoryEvent::new(
            "最后一幕还没有发生",
            format!("先经历“{}”，再回答它。", prelude.title()),
        )
        .tag("终局前回应");
    }

    if state.has_answered_ending_prelude(prelude) {
        return StoryEvent::new(
            "最后回应已经确定",
            format!("{}已经听见你的回答。结局正在等你。", prelude.title()),
        )
        .tag("终局前回应")
        .tag("复看");
    }

    state.answer_ending_prelude(prelude, response);
    response.event(prelude)
}

impl EndingPreludeId {
    pub const ALL: [Self; 5] = [
        Self::AloneDoor,
        Self::ChildWhiteLine,
        Self::TimetablePyre,
        Self::BroadcastBooth,
        Self::KeeperCoat,
    ];

    pub fn ending(self) -> Ending {
        match self {
            Self::AloneDoor => Ending::EscapedAlone,
            Self::ChildWhiteLine => Ending::TookChildHome,
            Self::TimetablePyre => Ending::BurnedTimetable,
            Self::BroadcastBooth => Ending::BecameTheVoice,
            Self::KeeperCoat => Ending::NewStationKeeper,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AloneDoor => "车门前的空座",
            Self::ChildWhiteLine => "白线前的撤销",
            Self::TimetablePyre => "旧时刻表的火",
            Self::BroadcastBooth => "广播室窄门",
            Self::KeeperCoat => "没有影子的外套",
        }
    }

    fn action_label(self) -> &'static str {
        match self {
            Self::AloneDoor => "终局前：站到空座前",
            Self::ChildWhiteLine => "终局前：撤销白线命令",
            Self::TimetablePyre => "终局前：点燃旧时刻表",
            Self::BroadcastBooth => "终局前：推开广播室门",
            Self::KeeperCoat => "终局前：披上外套以前",
        }
    }

    fn action_detail(self) -> &'static str {
        match self {
            Self::AloneDoor => "独自离开前，先确认你留下的是空座，不是又一个被你处理掉的人。",
            Self::ChildWhiteLine => {
                "带孩子返程前，先承认白线是你当年亲手加固的命令，并把是否跨线交还给他。"
            }
            Self::TimetablePyre => "烧掉规则前，先看清火会照亮什么，也会毁掉什么。",
            Self::BroadcastBooth => "成为声音前，先确认你愿意被误听、被忽略，也继续播报。",
            Self::KeeperCoat => "接任守夜前，先判断留下是不是又一次逃避上车。",
        }
    }

    fn missing_detail(self) -> &'static str {
        match self {
            Self::AloneDoor => {
                "你还缺姓名，或者缺返程票/轨道证据。空座不会替一个没有名字的人开门。"
            }
            Self::ChildWhiteLine => "孩子还没有愿意跨线，或者返程还没有被车站承认。",
            Self::TimetablePyre => "旧时刻表、修复后的雾灯和车站真相必须同时在场。",
            Self::BroadcastBooth => "姓名、站务日志、广播磁带和校准后的旧钟还没有接成一条线。",
            Self::KeeperCoat => "你还没有听懂旧钟真相，或没有理解雾灯站如何保存人。",
        }
    }

    fn route_ready(self, state: &GameState) -> bool {
        match self {
            Self::AloneDoor => {
                (state.ticket.name().contains("返程") || state.has_flag(Flag::InspectedRails))
                    && state.has_flag(Flag::RecoveredName)
            }
            Self::ChildWhiteLine => {
                state.has_flag(Flag::ChildJoined)
                    && (state.ticket.name().contains("返程")
                        || state.has_flag(Flag::SynthesizedChildTruth))
            }
            Self::TimetablePyre => {
                state.has_item(crate::model::Item::OldTimetable)
                    && state.has_flag(Flag::RepairedFogLamp)
                    && state.has_flag(Flag::SynthesizedStationTruth)
            }
            Self::BroadcastBooth => {
                state.has_flag(Flag::RecoveredName)
                    && state.has_item(crate::model::Item::StationLog)
                    && state.has_item(crate::model::Item::BroadcastTape)
                    && state.has_flag(Flag::AlignedClock)
            }
            Self::KeeperCoat => {
                state.has_flag(Flag::HeardClockTruth)
                    && state.has_flag(Flag::UnderstoodStationMechanism)
            }
        }
    }

    fn event(self) -> StoryEvent {
        let (title, body) = match self {
            Self::AloneDoor => (
                "终局前：车门前的空座",
                "雾灯号的车门打开，07B 空着，像从未被人使用过。你把手放到椅背上，终于承认独自离开不是清白，只是让那条旧命令继续替你站在月台上。",
            ),
            Self::ChildWhiteLine => (
                "终局前：白线前的撤销",
                "你蹲到白线外，没有伸手，也没有叫他过来。你说：六年前那句“不准动”现在作废。孩子看了你很久，像在检查这句话里有没有藏着新的命令。然后他说：那我可以先想一想。",
            ),
            Self::TimetablePyre => (
                "终局前：旧时刻表的火",
                "旧时刻表在雾灯下展开，每一班车都写着一个未完成的借口。你擦亮火柴时，许多乘客影子同时后退。烧掉它会让路变自由，也会让没人再能把痛苦交给一张表格保管。",
            ),
            Self::BroadcastBooth => (
                "终局前：广播室窄门",
                "广播室的门比想象中窄，只容得下一个人侧身进去。墙上挂着许多已经失真的姓名。你明白成为声音可能不是成为答案，而是永远重复那句曾经害了孩子的命令，直到你学会把命令改成提醒。",
            ),
            Self::KeeperCoat => (
                "终局前：没有影子的外套",
                "站务员把外套搭在椅背上，没有催你。你看见它没有影子，也没有退路。披上它不是抵账券；它只是一条很笨的规矩：每次想替别人决定时，先把灯照向门口。",
            ),
        };
        StoryEvent::new(title, body).tag("终局前场景")
    }
}

impl EndingPreludeResponseId {
    pub const ALL: [Self; 3] = [Self::AcceptCost, Self::ReturnChoice, Self::RefuseControl];

    fn name(self) -> &'static str {
        match self {
            Self::AcceptCost => "承认代价",
            Self::ReturnChoice => "交还选择",
            Self::RefuseControl => "拒绝控制",
        }
    }

    fn label_for(self, prelude: EndingPreludeId) -> &'static str {
        match (prelude, self) {
            (EndingPreludeId::AloneDoor, Self::AcceptCost) => "最后回应：承认空座会留下",
            (EndingPreludeId::AloneDoor, Self::ReturnChoice) => "最后回应：不替任何人坐下",
            (EndingPreludeId::AloneDoor, Self::RefuseControl) => "最后回应：拒绝让车站命名逃离",
            (EndingPreludeId::ChildWhiteLine, Self::AcceptCost) => "最后回应：承认命令来自你",
            (EndingPreludeId::ChildWhiteLine, Self::ReturnChoice) => "最后回应：让孩子自己跨线",
            (EndingPreludeId::ChildWhiteLine, Self::RefuseControl) => "最后回应：不把同行写成补偿",
            (EndingPreludeId::TimetablePyre, Self::AcceptCost) => "最后回应：承认火会毁掉秩序",
            (EndingPreludeId::TimetablePyre, Self::ReturnChoice) => "最后回应：把去处还给乘客",
            (EndingPreludeId::TimetablePyre, Self::RefuseControl) => "最后回应：拒绝纸面慈悲",
            (EndingPreludeId::BroadcastBooth, Self::AcceptCost) => "最后回应：接受被误听",
            (EndingPreludeId::BroadcastBooth, Self::ReturnChoice) => "最后回应：只播报不命令",
            (EndingPreludeId::BroadcastBooth, Self::RefuseControl) => "最后回应：拒绝成为新规则",
            (EndingPreludeId::KeeperCoat, Self::AcceptCost) => "最后回应：承认留下也会腐烂",
            (EndingPreludeId::KeeperCoat, Self::ReturnChoice) => "最后回应：把灯照向门口",
            (EndingPreludeId::KeeperCoat, Self::RefuseControl) => "最后回应：拒绝替旅客决定",
        }
    }

    fn detail_for(self, prelude: EndingPreludeId) -> &'static str {
        match (prelude, self) {
            (_, Self::AcceptCost) => "不把结局说成胜利，先承认这条路会留下些什么。",
            (_, Self::ReturnChoice) => "把选择还给关系里的另一个人，或还给后来者。",
            (_, Self::RefuseControl) => "拒绝让车站、职责或补偿欲替你解释这一切。",
        }
    }

    fn event(self, prelude: EndingPreludeId) -> StoryEvent {
        let (title, body) = match (prelude, self) {
            (EndingPreludeId::AloneDoor, Self::AcceptCost) => (
                "最后回应：承认空座会留下",
                "你对空座说：我走了，它仍会空着。承认这点没有让你更轻，却让离开不再像删除证据。",
            ),
            (EndingPreludeId::AloneDoor, Self::ReturnChoice) => (
                "最后回应：不替任何人坐下",
                "你把手从椅背上拿开。你不会替别人坐下，也不会再要求别人替你留在站台上。",
            ),
            (EndingPreludeId::AloneDoor, Self::RefuseControl) => (
                "最后回应：拒绝让车站命名逃离",
                "你不让广播替你定义这次离开。它可以叫你乘客，你知道自己是在承担一张空座。",
            ),
            (EndingPreludeId::ChildWhiteLine, Self::AcceptCost) => (
                "最后回应：承认命令来自你",
                "你对孩子说：那条白线不是车站画给你的，是我用害怕加重的。孩子点点头，像终于听见一句没有包装的实话。",
            ),
            (EndingPreludeId::ChildWhiteLine, Self::ReturnChoice) => (
                "最后回应：让孩子自己跨线",
                "你没有伸手，只把掌心摊开。孩子自己跨过白线，说：这次是我走过去的。",
            ),
            (EndingPreludeId::ChildWhiteLine, Self::RefuseControl) => (
                "最后回应：不把同行写成补偿",
                "你说：我不能用带你走来证明我是好人，也不能再用保护命令你。孩子看着你，终于没有把这句话退回去。",
            ),
            (EndingPreludeId::TimetablePyre, Self::AcceptCost) => (
                "最后回应：承认火会毁掉秩序",
                "你承认火不会把你洗干净。它会烧掉车站，也会烧掉许多人熟悉的借口。",
            ),
            (EndingPreludeId::TimetablePyre, Self::ReturnChoice) => (
                "最后回应：把去处还给乘客",
                "你把旧时刻表放低，让每个影子自己看见火。没有人再被一张纸安排目的地。",
            ),
            (EndingPreludeId::TimetablePyre, Self::RefuseControl) => (
                "最后回应：拒绝纸面慈悲",
                "你说：如果慈悲必须把人困在最后一分钟里，那它只是另一种管理。火光没有反驳。",
            ),
            (EndingPreludeId::BroadcastBooth, Self::AcceptCost) => (
                "最后回应：接受被误听",
                "你知道后来者可能听不懂你，甚至故意把警告当作挑衅。你仍然把嘴靠近话筒。",
            ),
            (EndingPreludeId::BroadcastBooth, Self::ReturnChoice) => (
                "最后回应：只播报不命令",
                "你把第一句广播改掉：不是不许上车，而是请先确认自己为什么要上车。",
            ),
            (EndingPreludeId::BroadcastBooth, Self::RefuseControl) => (
                "最后回应：拒绝成为新规则",
                "你拒绝把自己的声音变成新的车站制度。警告可以留下，命令不行。",
            ),
            (EndingPreludeId::KeeperCoat, Self::AcceptCost) => (
                "最后回应：承认留下也会腐烂",
                "你承认守夜会把人磨成制度的一部分。站务员第一次像是放心，又像是难过。",
            ),
            (EndingPreludeId::KeeperCoat, Self::ReturnChoice) => (
                "最后回应：把灯照向门口",
                "你给自己定下第一条规矩：雾灯必须照向出口，而不是照向留下来的理由。",
            ),
            (EndingPreludeId::KeeperCoat, Self::RefuseControl) => (
                "最后回应：拒绝替旅客决定",
                "你说：我可以守夜，但不能替他们选择。外套在椅背上轻轻落下，像终于有了重量。",
            ),
        };
        StoryEvent::new(title, body)
            .tag("终局前回应")
            .tag(format!("回应：{}", self.name()))
            .tag(format!("对应结局：{}", prelude.ending().short_title()))
    }
}

fn ending_response_note(state: &GameState) -> String {
    if state.ending_prelude_responses.is_empty() {
        return String::new();
    }

    let responses = state
        .ending_prelude_responses
        .iter()
        .map(|(prelude, response)| format!("{}：{}", prelude.title(), response.name()))
        .collect::<Vec<_>>();
    format!("你最后的回应是：{}。", responses.join(" / "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Item, TicketKind};

    #[test]
    fn prelude_requires_route_conditions_then_unlocks_final_choice() {
        let mut state = GameState::new();
        state.remember(Flag::FinalTrainArrived);
        state.remember(Flag::RecoveredName);
        state.ticket = TicketKind::Return;
        assert!(route_ready_for_ending(&state, Ending::EscapedAlone));
        assert!(!final_choice_ready(&state, Ending::EscapedAlone));

        let event = enter(&mut state, EndingPreludeId::AloneDoor);
        assert!(event.tags.iter().any(|tag| tag == "终局前场景"));
        assert!(state.has_completed_ending_prelude(EndingPreludeId::AloneDoor));
        assert!(!final_choice_ready(&state, Ending::EscapedAlone));

        let response = answer(
            &mut state,
            EndingPreludeId::AloneDoor,
            EndingPreludeResponseId::AcceptCost,
        );
        assert!(response.tags.iter().any(|tag| tag == "终局前回应"));
        assert!(state.has_answered_ending_prelude(EndingPreludeId::AloneDoor));
        assert!(final_choice_ready(&state, Ending::EscapedAlone));
    }

    #[test]
    fn prelude_count_matches_declared_table() {
        assert_eq!(ENDING_PRELUDE_COUNT, EndingPreludeId::ALL.len());
        assert_eq!(
            EndingPreludeId::TimetablePyre.ending(),
            Ending::BurnedTimetable
        );
        let mut state = GameState::new();
        state.add_item(Item::OldTimetable);
        assert!(!EndingPreludeId::TimetablePyre.route_ready(&state));
    }
}
