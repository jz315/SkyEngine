use crate::final_prelude;
use crate::model::{Ending, FinalDebateId, FinalDebateResponseId, GameState, StoryEvent};

pub const FINAL_DEBATE_COUNT: usize = FinalDebateId::ALL.len();

#[derive(Clone, Debug)]
pub struct FinalDebateResponseAction {
    pub debate: FinalDebateId,
    pub response: FinalDebateResponseId,
    pub label: String,
    pub detail: String,
    pub enabled: bool,
}

pub fn available_responses(state: &GameState) -> Vec<FinalDebateResponseAction> {
    FinalDebateId::ALL
        .iter()
        .copied()
        .filter(|debate| debate.ready(state))
        .filter(|debate| !state.has_answered_final_debate(*debate))
        .flat_map(|debate| {
            FinalDebateResponseId::ALL
                .iter()
                .copied()
                .map(move |response| FinalDebateResponseAction {
                    debate,
                    response,
                    label: response.label_for(debate).to_string(),
                    detail: response.detail_for(debate).to_string(),
                    enabled: true,
                })
        })
        .collect()
}

pub fn required_debate(ending: Ending) -> Option<FinalDebateId> {
    match ending {
        Ending::LostPassenger => None,
        Ending::EscapedAlone => Some(FinalDebateId::AloneTraveler),
        Ending::TookChildHome => Some(FinalDebateId::ChildWhiteLine),
        Ending::BurnedTimetable => Some(FinalDebateId::TimetableClerk),
        Ending::BecameTheVoice => Some(FinalDebateId::BroadcastVoice),
        Ending::NewStationKeeper => Some(FinalDebateId::KeeperCoat),
    }
}

pub fn final_choice_ready(state: &GameState, ending: Ending) -> bool {
    final_prelude::final_choice_ready(state, ending)
        && required_debate(ending).map_or(true, |debate| state.has_answered_final_debate(debate))
}

pub fn final_choice_detail(
    state: &GameState,
    ending: Ending,
    ready_detail: &'static str,
    missing_route_detail: &'static str,
) -> String {
    if !final_prelude::route_ready_for_ending(state, ending) {
        return missing_route_detail.to_string();
    }
    if !final_prelude::final_choice_ready(state, ending) {
        return final_prelude::final_choice_detail(
            state,
            ending,
            ready_detail,
            missing_route_detail,
        );
    }
    if let Some(debate) = required_debate(ending) {
        if !state.has_answered_final_debate(debate) {
            return format!("先回应终局争辩：{}。", debate.title());
        }
    }
    ready_detail.to_string()
}

pub fn missing_final_step_event(state: &GameState, ending: Ending) -> StoryEvent {
    if !final_prelude::final_choice_ready(state, ending) {
        return final_prelude::missing_prelude_event(state, ending);
    }

    let Some(debate) = required_debate(ending) else {
        return StoryEvent::new("还不能这样做", "这个选择没有对应的终局争辩。");
    };
    StoryEvent::new(
        "还差一次争辩",
        format!(
            "你已经走到结局门口，但{}还没有放你轻易通过。先回应“{}”，再作最终选择。",
            debate.speaker(),
            debate.title()
        ),
    )
    .tag("终局争辩")
}

pub fn answer(
    state: &mut GameState,
    debate: FinalDebateId,
    response: FinalDebateResponseId,
) -> StoryEvent {
    if !state.final_train_due() {
        return StoryEvent::new(
            "列车还没有到站",
            "终局争辩不能提前排练。只有车门真正打开时，人物才会把你的结局推回给你。",
        )
        .tag("终局争辩");
    }

    if !debate.ready(state) {
        return StoryEvent::new(
            "争辩还没有成立",
            format!(
                "先完成对应路线的最后现场和回应，再让{}提出这个问题。",
                debate.speaker()
            ),
        )
        .tag("终局争辩");
    }

    if state.has_answered_final_debate(debate) {
        return StoryEvent::new(
            "争辩已经回应过",
            format!(
                "{}已经听见你的回答。现在可以继续走向结局。",
                debate.speaker()
            ),
        )
        .tag("终局争辩")
        .tag("复看");
    }

    state.answer_final_debate(debate, response);
    response.event(debate)
}

pub fn ending_note(state: &GameState) -> Option<String> {
    if state.final_debate_responses.is_empty() {
        return None;
    }

    let notes = state
        .final_debate_responses
        .iter()
        .map(|(debate, response)| format!("{}：{}", debate.title(), debate.review(*response)))
        .collect::<Vec<_>>();
    Some(format!(
        "终局不是没人反对的按钮。最后这些争辩留下了回声：{}",
        notes.join(" ")
    ))
}

pub fn ending_tag(ending: Ending, state: &GameState) -> Option<String> {
    let debate = required_debate(ending)?;
    let response = state.final_debate_response(debate)?;
    Some(format!("终局争辩：{}", response.name()))
}

pub fn completed_count(state: &GameState) -> usize {
    state.final_debate_responses.len()
}

impl FinalDebateId {
    pub const ALL: [Self; 5] = [
        Self::AloneTraveler,
        Self::ChildWhiteLine,
        Self::TimetableClerk,
        Self::BroadcastVoice,
        Self::KeeperCoat,
    ];

    fn ending(self) -> Ending {
        match self {
            Self::AloneTraveler => Ending::EscapedAlone,
            Self::ChildWhiteLine => Ending::TookChildHome,
            Self::TimetableClerk => Ending::BurnedTimetable,
            Self::BroadcastVoice => Ending::BecameTheVoice,
            Self::KeeperCoat => Ending::NewStationKeeper,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::AloneTraveler => "老人问你是否又留下一个空座",
            Self::ChildWhiteLine => "孩子问你是否又在替他决定",
            Self::TimetableClerk => "售票员问谁来收拾自由",
            Self::BroadcastVoice => "旧广播问提醒会不会变成命令",
            Self::KeeperCoat => "站务员问留下是不是占有",
        }
    }

    fn speaker(self) -> &'static str {
        match self {
            Self::AloneTraveler => "候车厅老人",
            Self::ChildWhiteLine => "白线后的孩子",
            Self::TimetableClerk => "售票员",
            Self::BroadcastVoice => "广播里的旧声音",
            Self::KeeperCoat => "站务员",
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::AloneTraveler => {
                "你说独自离开，可那个空座以后归谁？你是真的尊重缺席，还是只是学会不回头？"
            }
            Self::ChildWhiteLine => {
                "你说让我自己跨线。那如果到站以后我后悔、发脾气、不感谢你，你还会说这是我的选择吗？"
            }
            Self::TimetableClerk => {
                "你要烧掉时刻表。很好。等他们不知道该去哪、开始互相责怪时，谁来收拾自由？"
            }
            Self::BroadcastVoice => {
                "你想成为警告。可警告说久了也会像命令。你凭什么保证自己不会变成新的车站？"
            }
            Self::KeeperCoat => {
                "你说留下守夜。留下的人最容易把自己说成必要。你怎么知道你不是在占有这盏灯？"
            }
        }
    }

    fn ready(self, state: &GameState) -> bool {
        final_prelude::final_choice_ready(state, self.ending())
    }

    fn review(self, response: FinalDebateResponseId) -> &'static str {
        match (self, response) {
            (Self::AloneTraveler, FinalDebateResponseId::Insist) => {
                "你坚持离开，但承认空座不会因此变轻。"
            }
            (Self::AloneTraveler, FinalDebateResponseId::AdmitWound) => {
                "你承认独自离开也会伤人，所以不再把它伪装成清白。"
            }
            (Self::AloneTraveler, FinalDebateResponseId::RewritePromise) => {
                "你把承诺改成不再替缺席者发言，只替自己承担。"
            }
            (Self::ChildWhiteLine, FinalDebateResponseId::Insist) => {
                "你坚持同行，却承认他保留讨厌这段路的权利。"
            }
            (Self::ChildWhiteLine, FinalDebateResponseId::AdmitWound) => {
                "你承认旧命令来自你，不再把带走包装成修复。"
            }
            (Self::ChildWhiteLine, FinalDebateResponseId::RewritePromise) => {
                "你把承诺改成允许他不听，而不是收集他的感谢。"
            }
            (Self::TimetableClerk, FinalDebateResponseId::Insist) => {
                "你坚持烧掉规则，同时承认自由不会自动善后。"
            }
            (Self::TimetableClerk, FinalDebateResponseId::AdmitWound) => {
                "你承认时刻表也曾保护过人，所以火不是纯粹胜利。"
            }
            (Self::TimetableClerk, FinalDebateResponseId::RewritePromise) => {
                "你把承诺改成不再制造新的表格来替代旧表格。"
            }
            (Self::BroadcastVoice, FinalDebateResponseId::Insist) => {
                "你坚持播报警告，但接受有人仍会误听。"
            }
            (Self::BroadcastVoice, FinalDebateResponseId::AdmitWound) => {
                "你承认提醒也会伤人，所以不把声音说成答案。"
            }
            (Self::BroadcastVoice, FinalDebateResponseId::RewritePromise) => {
                "你把承诺改成只保留迟疑，不把迟疑扩写成命令。"
            }
            (Self::KeeperCoat, FinalDebateResponseId::Insist) => {
                "你坚持留下，但承认守夜会诱惑你成为制度。"
            }
            (Self::KeeperCoat, FinalDebateResponseId::AdmitWound) => {
                "你承认留下也可能伤人，所以不把牺牲当成免罪。"
            }
            (Self::KeeperCoat, FinalDebateResponseId::RewritePromise) => {
                "你把承诺改成先照门口，再照自己的职责。"
            }
        }
    }
}

impl FinalDebateResponseId {
    pub const ALL: [Self; 3] = [Self::Insist, Self::AdmitWound, Self::RewritePromise];

    fn name(self) -> &'static str {
        match self {
            Self::Insist => "坚持此路",
            Self::AdmitWound => "承认伤口",
            Self::RewritePromise => "改写承诺",
        }
    }

    fn label_for(self, debate: FinalDebateId) -> &'static str {
        match (debate, self) {
            (FinalDebateId::AloneTraveler, Self::Insist) => "争辩：我仍要离开",
            (FinalDebateId::AloneTraveler, Self::AdmitWound) => "争辩：离开也会伤人",
            (FinalDebateId::AloneTraveler, Self::RewritePromise) => "争辩：不再替缺席者说话",
            (FinalDebateId::ChildWhiteLine, Self::Insist) => "争辩：我仍要跨过白线",
            (FinalDebateId::ChildWhiteLine, Self::AdmitWound) => "争辩：命令来自我",
            (FinalDebateId::ChildWhiteLine, Self::RewritePromise) => "争辩：以后你可以不听",
            (FinalDebateId::TimetableClerk, Self::Insist) => "争辩：我仍要烧掉它",
            (FinalDebateId::TimetableClerk, Self::AdmitWound) => "争辩：规则也曾保护人",
            (FinalDebateId::TimetableClerk, Self::RewritePromise) => "争辩：不再制造新表格",
            (FinalDebateId::BroadcastVoice, Self::Insist) => "争辩：我仍要播报",
            (FinalDebateId::BroadcastVoice, Self::AdmitWound) => "争辩：提醒也会伤人",
            (FinalDebateId::BroadcastVoice, Self::RewritePromise) => "争辩：不把迟疑写成命令",
            (FinalDebateId::KeeperCoat, Self::Insist) => "争辩：我仍要留下",
            (FinalDebateId::KeeperCoat, Self::AdmitWound) => "争辩：留下也会伤人",
            (FinalDebateId::KeeperCoat, Self::RewritePromise) => "争辩：先照门口",
        }
    }

    fn detail_for(self, debate: FinalDebateId) -> String {
        format!("{}。{}", debate.prompt(), self.intent())
    }

    fn intent(self) -> &'static str {
        match self {
            Self::Insist => "坚持路线，但不把它说成轻松胜利。",
            Self::AdmitWound => "承认这条路会伤人，仍决定承担它。",
            Self::RewritePromise => "把承诺从控制别人，改成约束自己。",
        }
    }

    fn event(self, debate: FinalDebateId) -> StoryEvent {
        let (title, body) = match (debate, self) {
            (FinalDebateId::AloneTraveler, Self::Insist) => (
                "终局争辩：我仍要离开",
                "老人把报纸折到 07B 那一栏。你说：我仍要离开，但这个空座不会被我说成不存在。老人点头，像终于听见一个不干净却诚实的答案。",
            ),
            (FinalDebateId::AloneTraveler, Self::AdmitWound) => (
                "终局争辩：离开也会伤人",
                "你承认独自上车会伤人。老人没有安慰你，只说：那就别把伤口叫作自由。你把这句话收进票夹。车门因此更重，也更真实。",
            ),
            (FinalDebateId::AloneTraveler, Self::RewritePromise) => (
                "终局争辩：不再替缺席者说话",
                "你说：我不再替缺席者解释，也不再让他们替我留座。老人把报纸递给你一角，像把沉默的边界交还给沉默本身。",
            ),
            (FinalDebateId::ChildWhiteLine, Self::Insist) => (
                "终局争辩：我仍要跨过白线",
                "孩子问如果跨过去以后还是害怕怎么办。你说：那我也要听。同行不是把你带到我想要的明天，而是到站以后仍承认你能讨厌我。",
            ),
            (FinalDebateId::ChildWhiteLine, Self::AdmitWound) => (
                "终局争辩：命令来自我",
                "你没有把白线说成车站的错。你说：那句不准动，是我说的。孩子反而松开一点肩膀，因为这次大人终于没有用规则遮住伤口。",
            ),
            (FinalDebateId::ChildWhiteLine, Self::RewritePromise) => (
                "终局争辩：以后你可以不听",
                "你把承诺改短：到站以后，你可以不听我的。孩子看着你，像在判断这句话能不能经得起明天。最后他说：那你先记住。",
            ),
            (FinalDebateId::TimetableClerk, Self::Insist) => (
                "终局争辩：我仍要烧掉它",
                "售票员问谁来收拾自由。你说：我不知道，但继续让纸替人活着更糟。她没有笑，只把票章扣回抽屉，让你听见规则合上的声音。",
            ),
            (FinalDebateId::TimetableClerk, Self::AdmitWound) => (
                "终局争辩：规则也曾保护人",
                "你承认时刻表曾经挡过风。售票员的眼神因此软了一点：那就别把火说成正义。你点头，火柴在指尖变得不那么英雄。",
            ),
            (FinalDebateId::TimetableClerk, Self::RewritePromise) => (
                "终局争辩：不再制造新表格",
                "你说烧掉它以后，不会立刻写一张新表格替别人选择。售票员把空白票递给你：那就先学会看见空白。它比规则难多了。",
            ),
            (FinalDebateId::BroadcastVoice, Self::Insist) => (
                "终局争辩：我仍要播报",
                "旧声音在磁带里问你凭什么保证。你说：我不能保证，只能一遍遍把警告说得不像命令。杂音短暂停下，像有人终于让开半步。",
            ),
            (FinalDebateId::BroadcastVoice, Self::AdmitWound) => (
                "终局争辩：提醒也会伤人",
                "你承认提醒会刺痛后来者，因为它逼人看见自己想逃。旧声音轻轻笑了一下：那就别躲在正确后面。你把话筒握稳。",
            ),
            (FinalDebateId::BroadcastVoice, Self::RewritePromise) => (
                "终局争辩：不把迟疑写成命令",
                "你说广播只保留迟疑，不替任何人下判决。旧声音把最后一段噪音交给你，像交出一把没有把手的钥匙。",
            ),
            (FinalDebateId::KeeperCoat, Self::Insist) => (
                "终局争辩：我仍要留下",
                "站务员问你是不是又在占有这盏灯。你说：可能是，所以我要每天检查这一点。外套没有变轻，但它不再像加冕。",
            ),
            (FinalDebateId::KeeperCoat, Self::AdmitWound) => (
                "终局争辩：留下也会伤人",
                "你承认留下会伤人，也会让你误以为自己必要。站务员低声说：知道这点的人，才勉强可以碰那盏灯。",
            ),
            (FinalDebateId::KeeperCoat, Self::RewritePromise) => (
                "终局争辩：先照门口",
                "你说第一条规矩是先照门口，再照职责。站务员终于把外套从椅背上拿起，像把一段漫长错误递给一个仍会犯错的人。",
            ),
        };
        StoryEvent::new(title, body)
            .tag("终局争辩")
            .tag(format!("争辩：{}", self.name()))
            .tag(format!("对应结局：{}", debate.ending().short_title()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EndingPreludeId, EndingPreludeResponseId, Flag, TicketKind};

    #[test]
    fn final_debate_unlocks_after_prelude_response_and_gates_choice() {
        let mut state = GameState::new();
        state.remember(Flag::FinalTrainArrived);
        state.remember(Flag::RecoveredName);
        state.ticket = TicketKind::Return;
        state.complete_ending_prelude(EndingPreludeId::AloneDoor);
        state.answer_ending_prelude(
            EndingPreludeId::AloneDoor,
            EndingPreludeResponseId::AcceptCost,
        );

        assert!(!final_choice_ready(&state, Ending::EscapedAlone));
        let actions = available_responses(&state);
        assert!(actions
            .iter()
            .any(|action| action.debate == FinalDebateId::AloneTraveler));

        let event = answer(
            &mut state,
            FinalDebateId::AloneTraveler,
            FinalDebateResponseId::AdmitWound,
        );
        assert!(event.tags.iter().any(|tag| tag == "终局争辩"));
        assert!(state.has_answered_final_debate(FinalDebateId::AloneTraveler));
        assert!(final_choice_ready(&state, Ending::EscapedAlone));
    }
}
