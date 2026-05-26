use crate::model::{DialogueTone, Flag, GameState, Item, Location, StoryEvent, TopicId};

#[derive(Clone, Debug)]
pub struct TopicAction {
    pub topic: TopicId,
    pub label: &'static str,
    pub detail: &'static str,
    pub enabled: bool,
}

pub fn available_topics(state: &GameState, location: Location) -> Vec<TopicAction> {
    let topics: &[TopicId] = match location {
        Location::WaitingHall => &[
            TopicId::TravelerRain,
            TopicId::TravelerKey,
            TopicId::TravelerTicket,
            TopicId::TravelerLeaving,
            TopicId::TravelerChild,
            TopicId::TravelerMercy,
        ],
        Location::TicketOffice => &[
            TopicId::ClerkDestination,
            TopicId::ClerkOneWay,
            TopicId::ClerkLog,
            TopicId::ClerkSeats,
            TopicId::ClerkName,
            TopicId::ClerkPrice,
        ],
        Location::LostAndFound => &[
            TopicId::LostFoundLabels,
            TopicId::LostFoundCabinet,
            TopicId::LostFoundLantern,
            TopicId::LostFoundNameTag,
            TopicId::LostFoundApology,
            TopicId::LostFoundLedger,
        ],
        Location::Underpass => &[
            TopicId::UnderpassEcho,
            TopicId::UnderpassWaterline,
            TopicId::UnderpassExit,
            TopicId::UnderpassLoop,
            TopicId::UnderpassBroadcast,
            TopicId::UnderpassStairs,
        ],
        Location::Platform => &[
            TopicId::ChildWhiteLine,
            TopicId::ChildHomework,
            TopicId::ChildPromise,
            TopicId::ChildAnger,
            TopicId::ChildTomorrow,
            TopicId::ChildLeaving,
        ],
        Location::ClockTower => &[
            TopicId::KeeperClock,
            TopicId::KeeperLantern,
            TopicId::KeeperTimetable,
            TopicId::KeeperBroadcast,
            TopicId::KeeperStay,
            TopicId::KeeperCoat,
        ],
    };

    topics
        .iter()
        .copied()
        .filter(|topic| topic_visible(state, *topic))
        .map(|topic| TopicAction {
            topic,
            label: topic_label(topic),
            detail: topic_detail(state, topic),
            enabled: !state.has_discussed(topic) && topic_enabled(state, topic),
        })
        .collect()
}

pub fn discuss(state: &mut GameState, topic: TopicId) -> StoryEvent {
    if state.has_discussed(topic) {
        return StoryEvent::new(
            "话题已经沉下去",
            "你又把同一个问题举起来。对方没有拒绝，只是这一次，雾先替他们回答：有些话已经说过，真正还没发生的是你如何处理它。",
        )
        .tag("自由对话");
    }

    if !topic_visible(state, topic) || !topic_enabled(state, topic) {
        return StoryEvent::new(
            "话题还没有形状",
            "这个问题还缺少证据、信任或地点。先去调查相关物件，或和对应人物推进主对话。",
        )
        .tag("自由对话");
    }

    state.discuss(topic);
    apply_topic_rewards(state, topic);
    let mut event = topic_event(topic);
    apply_tone_rewards(state, topic, &mut event);
    event
}

fn topic_visible(state: &GameState, topic: TopicId) -> bool {
    match topic {
        TopicId::TravelerRain | TopicId::ClerkDestination | TopicId::KeeperClock => true,
        TopicId::TravelerKey => state.has_item(Item::BrassKey) || state.traveler_depth >= 1,
        TopicId::TravelerTicket => state.has_flag(Flag::ExaminedTicket),
        TopicId::TravelerLeaving => state.traveler_depth >= 4 || state.has_item(Item::CoinToken),
        TopicId::TravelerChild => {
            state.has_flag(Flag::MetChild) || state.has_flag(Flag::UnderstoodChildPromise)
        }
        TopicId::TravelerMercy => {
            state.has_flag(Flag::UnderstoodStationMechanism) || state.synthesis_depth >= 2
        }
        TopicId::ClerkOneWay => state.clerk_depth >= 1 || state.has_flag(Flag::ReadDepartureBoard),
        TopicId::ClerkLog => state.has_item(Item::StationLog),
        TopicId::ClerkSeats => state.has_item(Item::CoinToken) || state.clerk_depth >= 2,
        TopicId::ClerkName => state.clerk_depth >= 5,
        TopicId::ClerkPrice => state.has_flag(Flag::TicketRewritten) || state.clerk_trust >= 3,
        TopicId::LostFoundLabels => true,
        TopicId::LostFoundCabinet => {
            state.has_item(Item::BrassKey)
                || state.has_flag(Flag::OpenedCabinet)
                || state.investigation_depth(Location::LostAndFound) >= 5
        }
        TopicId::LostFoundLantern => {
            state.has_item(Item::LanternGlass)
                || state.has_flag(Flag::RepairedFogLamp)
                || state.investigation_depth(Location::LostAndFound) >= 2
        }
        TopicId::LostFoundNameTag => {
            state.has_item(Item::NameTag)
                || state.has_flag(Flag::RecoveredName)
                || state.investigation_depth(Location::LostAndFound) >= 3
        }
        TopicId::LostFoundApology => {
            state.has_flag(Flag::SearchedLostFound)
                || state.investigation_depth(Location::LostAndFound) >= 4
        }
        TopicId::LostFoundLedger => {
            state.has_item(Item::StationLog)
                || state.investigation_depth(Location::LostAndFound) >= 10
        }
        TopicId::UnderpassEcho => true,
        TopicId::UnderpassWaterline => state.investigation_depth(Location::Underpass) >= 1,
        TopicId::UnderpassExit => state.investigation_depth(Location::Underpass) >= 2,
        TopicId::UnderpassLoop => {
            state.has_flag(Flag::UnderstoodFirstLoop)
                || state.investigation_depth(Location::Underpass) >= 3
        }
        TopicId::UnderpassBroadcast => {
            state.has_item(Item::BroadcastTape)
                || state.has_flag(Flag::HeardBroadcastTape)
                || state.investigation_depth(Location::Underpass) >= 5
        }
        TopicId::UnderpassStairs => {
            state.has_flag(Flag::RecoveredName)
                || state.investigation_depth(Location::Underpass) >= 8
        }
        TopicId::ChildWhiteLine => state.has_flag(Flag::MetChild),
        TopicId::ChildHomework => state.has_item(Item::ChildHomework) || state.child_depth >= 1,
        TopicId::ChildPromise => {
            state.has_flag(Flag::UnderstoodChildPromise) || state.child_depth >= 2
        }
        TopicId::ChildAnger => state.child_depth >= 5,
        TopicId::ChildTomorrow => state.child_depth >= 8 || state.synthesis_depth >= 8,
        TopicId::ChildLeaving => state.has_flag(Flag::ReturnedNameTag) || state.child_trust >= 4,
        TopicId::KeeperLantern => {
            state.has_item(Item::LanternGlass) || state.has_flag(Flag::RepairedFogLamp)
        }
        TopicId::KeeperTimetable => state.has_item(Item::OldTimetable),
        TopicId::KeeperBroadcast => {
            state.has_item(Item::BroadcastTape) || state.has_flag(Flag::HeardBroadcastTape)
        }
        TopicId::KeeperStay => {
            state.has_flag(Flag::UnderstoodStationMechanism) || state.keeper_depth >= 3
        }
        TopicId::KeeperCoat => state.keeper_depth >= 6 || state.has_flag(Flag::HeardClockTruth),
    }
}

fn topic_enabled(state: &GameState, topic: TopicId) -> bool {
    match topic {
        TopicId::ChildWhiteLine
        | TopicId::ChildHomework
        | TopicId::ChildPromise
        | TopicId::ChildAnger
        | TopicId::ChildTomorrow
        | TopicId::ChildLeaving => state.has_flag(Flag::MetChild),
        _ => true,
    }
}

fn topic_label(topic: TopicId) -> &'static str {
    match topic {
        TopicId::TravelerRain => "问老人：六年前那晚的雨发生了什么",
        TopicId::TravelerKey => "问老人：黄铜钥匙从哪里来",
        TopicId::TravelerTicket => "问老人：湿票背面完整写了什么",
        TopicId::TravelerLeaving => "问老人：他为什么上车后又回来",
        TopicId::TravelerChild => "问老人：他有没有见过那个孩子",
        TopicId::TravelerMercy => "问老人：停住午夜到底帮了谁",
        TopicId::ClerkDestination => "问售票员：目的地为空意味着什么",
        TopicId::ClerkOneWay => "问售票员：为什么单程票容易开",
        TopicId::ClerkLog => "让售票员读你写过的站务日志",
        TopicId::ClerkSeats => "问售票员：为什么总是两个座位",
        TopicId::ClerkName => "问售票员：她为什么没有姓名",
        TopicId::ClerkPrice => "问售票员：返程要付出什么代价",
        TopicId::LostFoundLabels => "查看失物标签：谁留下了这些东西",
        TopicId::LostFoundCabinet => "查看铁柜：里面藏着什么档案",
        TopicId::LostFoundLantern => "查看雾灯玻璃：修灯有什么用",
        TopicId::LostFoundNameTag => "查看姓名牌：两个名字是什么关系",
        TopicId::LostFoundApology => "查看空箱子：你需要放下什么",
        TopicId::LostFoundLedger => "查看账册：无人领取的物件去了哪里",
        TopicId::UnderpassEcho => "问回声：它为什么总慢半拍",
        TopicId::UnderpassWaterline => "查看水线：白线旧案留下了什么",
        TopicId::UnderpassExit => "查看疏散标志：为什么指向月台",
        TopicId::UnderpassLoop => "问申请记录：是谁把午夜停住",
        TopicId::UnderpassBroadcast => "问地下广播：完整姓名会带来什么",
        TopicId::UnderpassStairs => "查看台阶：为什么总回到原处",
        TopicId::ChildWhiteLine => "问孩子：他为什么站在白线后",
        TopicId::ChildHomework => "问孩子：作业本写了什么",
        TopicId::ChildPromise => "问孩子：你以前答应过什么",
        TopicId::ChildAnger => "问孩子：他是否可以继续生气",
        TopicId::ChildTomorrow => "问孩子：他想要怎样的明天",
        TopicId::ChildLeaving => "问孩子：上车后如果还害怕怎么办",
        TopicId::KeeperClock => "问站务员：钟为什么停在 23:59",
        TopicId::KeeperLantern => "问站务员：雾灯修好后会发生什么",
        TopicId::KeeperTimetable => "让站务员看烧焦的旧时刻表",
        TopicId::KeeperBroadcast => "问站务员：进入广播室会留下什么",
        TopicId::KeeperStay => "问站务员：留下守夜会不会伤人",
        TopicId::KeeperCoat => "问站务员：外套代表什么职位",
    }
}

fn topic_detail(state: &GameState, topic: TopicId) -> &'static str {
    if state.has_discussed(topic) {
        return "这个话题已经说过，记录在日志里。";
    }
    match topic {
        TopicId::TravelerRain => "不用线索也能问。会得到六年前雨夜和老人报纸的第一层信息。",
        TopicId::TravelerKey => "需要钥匙或老人已经把钥匙的事说出口。",
        TopicId::TravelerTicket => "需要先检查湿票。",
        TopicId::TravelerLeaving => "需要铜筹或老人谈过更深的往事。",
        TopicId::TravelerChild => "需要见过孩子，或已经理解车票的第二个名字。",
        TopicId::TravelerMercy => "需要先理解旧钟或车站机制。",
        TopicId::ClerkDestination => "可直接询问。会解释目的地为空的风险。",
        TopicId::ClerkOneWay => "需要读过时刻表，或听她谈过退票规则。",
        TopicId::ClerkLog => "需要站务日志。",
        TopicId::ClerkSeats => "需要退票铜筹，或已经问到返程规则。",
        TopicId::ClerkName => "需要她对你不再完全公事公办。",
        TopicId::ClerkPrice => "需要足够信任，或已经改签返程票。",
        TopicId::LostFoundLabels => "可直接查看。会解释失物招领处保存了哪些东西。",
        TopicId::LostFoundCabinet => "需要黄铜钥匙、铁柜档案，或足够深的失物调查。",
        TopicId::LostFoundLantern => "需要见过雾灯玻璃，或已经修好雾灯。",
        TopicId::LostFoundNameTag => "需要见过姓名牌，或已经想起姓名。",
        TopicId::LostFoundApology => "需要先翻过失物箱，或找到无人领取的道歉信。",
        TopicId::LostFoundLedger => "需要站务日志，或查到招领处账册深处。",
        TopicId::UnderpassEcho => "可直接询问。可能帮助你找回姓名。",
        TopicId::UnderpassWaterline => "需要先看见墙砖上的水线。",
        TopicId::UnderpassExit => "需要看见相反方向的疏散标志。",
        TopicId::UnderpassLoop => "需要读到循环申请，或已经理解第一次循环。",
        TopicId::UnderpassBroadcast => "需要广播磁带、广播室线索，或听见地下旧录音。",
        TopicId::UnderpassStairs => "需要找回姓名，或走到足够深的台阶。",
        TopicId::ChildWhiteLine => "需要先见到孩子。",
        TopicId::ChildHomework => "需要作业本或他愿意谈作业本。",
        TopicId::ChildPromise => "需要理解第二个名字，或他提起旧承诺。",
        TopicId::ChildAnger => "需要足够多的孩子对话。",
        TopicId::ChildTomorrow => "需要他谈过明天，或你整理过孩子线索。",
        TopicId::ChildLeaving => "需要足够信任，或孩子已经愿意同行。",
        TopicId::KeeperClock => "可直接询问。会解释 23:59 和最后一分钟。",
        TopicId::KeeperLantern => "需要雾灯玻璃或修好的雾灯。",
        TopicId::KeeperTimetable => "需要烧焦的旧时刻表。",
        TopicId::KeeperBroadcast => "需要广播磁带或广播室线索。",
        TopicId::KeeperStay => "需要理解车站机制，或听过旧钟代价。",
        TopicId::KeeperCoat => "需要听过站务员谈守夜与代价。",
    }
}

fn apply_topic_rewards(state: &mut GameState, topic: TopicId) {
    match topic {
        TopicId::TravelerTicket => {
            state.remember(Flag::TravelerTrusted);
            state.child_trust = (state.child_trust + 1).min(5);
        }
        TopicId::TravelerChild => {
            state.child_trust = (state.child_trust + 1).min(5);
        }
        TopicId::ClerkLog => {
            state.remember(Flag::ReadStationLog);
            state.clerk_trust = (state.clerk_trust + 2).min(5);
        }
        TopicId::ClerkSeats | TopicId::ClerkPrice => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        TopicId::LostFoundCabinet | TopicId::LostFoundLedger => {
            if state.has_item(Item::StationLog) {
                state.remember(Flag::ReadStationLog);
            }
        }
        TopicId::LostFoundNameTag => {
            if state.has_item(Item::NameTag) && state.has_flag(Flag::MetChild) {
                state.remember(Flag::UnderstoodChildPromise);
                state.child_trust = (state.child_trust + 1).min(5);
            }
        }
        TopicId::UnderpassEcho => {
            if state.has_item(Item::NameTag) || state.has_flag(Flag::ExaminedTicket) {
                state.remember(Flag::RecoveredName);
            }
        }
        TopicId::UnderpassLoop => {
            state.remember(Flag::UnderstoodFirstLoop);
        }
        TopicId::UnderpassBroadcast => {
            if state.has_item(Item::BroadcastTape) {
                state.remember(Flag::HeardBroadcastTape);
            }
        }
        TopicId::UnderpassStairs => {
            if state.has_flag(Flag::RecoveredName) {
                state.remember(Flag::UnderstoodChildPromise);
            }
        }
        TopicId::ChildPromise => {
            state.remember(Flag::UnderstoodChildPromise);
            state.child_trust = (state.child_trust + 1).min(5);
        }
        TopicId::ChildAnger | TopicId::ChildTomorrow | TopicId::ChildLeaving => {
            state.child_trust = (state.child_trust + 1).min(5);
        }
        TopicId::KeeperTimetable => {
            state.remember(Flag::UnderstoodStationMechanism);
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        TopicId::KeeperBroadcast | TopicId::KeeperStay => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        _ => {}
    }
}

fn apply_tone_rewards(state: &mut GameState, topic: TopicId, event: &mut StoryEvent) {
    match state.dialogue_tone {
        DialogueTone::Listening if is_child_topic(topic) || is_traveler_topic(topic) => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("语气：倾听".to_string());
        }
        DialogueTone::Listening if is_keeper_topic(topic) => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("语气：倾听".to_string());
        }
        DialogueTone::Gentle if is_child_topic(topic) => {
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("语气：放轻".to_string());
        }
        DialogueTone::Gentle if is_traveler_topic(topic) => {
            state.remember(Flag::TravelerTrusted);
            event.tags.push("语气：放轻".to_string());
        }
        DialogueTone::Direct if is_clerk_topic(topic) => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("语气：逼近".to_string());
        }
        DialogueTone::Direct if is_keeper_topic(topic) => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            event.tags.push("语气：逼近".to_string());
        }
        DialogueTone::Direct if is_environmental_topic(topic) => {
            event.tags.push("语气：逼近".to_string());
        }
        _ => {}
    }
}

fn is_traveler_topic(topic: TopicId) -> bool {
    matches!(
        topic,
        TopicId::TravelerRain
            | TopicId::TravelerKey
            | TopicId::TravelerTicket
            | TopicId::TravelerLeaving
            | TopicId::TravelerChild
            | TopicId::TravelerMercy
    )
}

fn is_clerk_topic(topic: TopicId) -> bool {
    matches!(
        topic,
        TopicId::ClerkDestination
            | TopicId::ClerkOneWay
            | TopicId::ClerkLog
            | TopicId::ClerkSeats
            | TopicId::ClerkName
            | TopicId::ClerkPrice
    )
}

fn is_child_topic(topic: TopicId) -> bool {
    matches!(
        topic,
        TopicId::ChildWhiteLine
            | TopicId::ChildHomework
            | TopicId::ChildPromise
            | TopicId::ChildAnger
            | TopicId::ChildTomorrow
            | TopicId::ChildLeaving
    )
}

fn is_keeper_topic(topic: TopicId) -> bool {
    matches!(
        topic,
        TopicId::KeeperClock
            | TopicId::KeeperLantern
            | TopicId::KeeperTimetable
            | TopicId::KeeperBroadcast
            | TopicId::KeeperStay
            | TopicId::KeeperCoat
    )
}

fn is_environmental_topic(topic: TopicId) -> bool {
    matches!(
        topic,
        TopicId::LostFoundLabels
            | TopicId::LostFoundCabinet
            | TopicId::LostFoundLantern
            | TopicId::LostFoundNameTag
            | TopicId::LostFoundApology
            | TopicId::LostFoundLedger
            | TopicId::UnderpassEcho
            | TopicId::UnderpassWaterline
            | TopicId::UnderpassExit
            | TopicId::UnderpassLoop
            | TopicId::UnderpassBroadcast
            | TopicId::UnderpassStairs
    )
}

fn topic_event(topic: TopicId) -> StoryEvent {
    let (title, body, tag) = match topic {
        TopicId::TravelerRain => (
            "老人谈雨",
            "你问六年前那晚的雨。老人说，真正重要的不是天气，而是你每次醒来都带着同样的水痕。那说明你还没有处理完那晚的事。",
            "老人",
        ),
        TopicId::TravelerKey => (
            "老人谈钥匙",
            "老人说你以前把黄铜钥匙交给他保管。钥匙可以打开失物招领处的铁柜，也可能和旧钟有关。你当时要求他等你再次追问时再还给你。",
            "老人",
        ),
        TopicId::TravelerTicket => (
            "老人谈湿票",
            "你问湿票缺少的半句。老人说完整警告不是“别上车”，而是“别一个人上车”。你每次只记住前半句，所以总把问题理解成能不能离开。",
            "老人",
        ),
        TopicId::TravelerLeaving => (
            "老人谈归来",
            "老人承认自己曾上过车，但下一站又下来了。他发现如果没人留下提醒后来的旅客，雾灯站的危险会被当成普通传闻。",
            "老人",
        ),
        TopicId::TravelerChild => (
            "老人谈孩子",
            "老人说他见过孩子很多次。孩子不是来替你把账抹平，也不是来立刻原谅你。他只是在看你这次敢不敢承认：白线后的等待，最早来自你的命令。",
            "老人",
        ),
        TopicId::TravelerMercy => (
            "老人谈停住午夜",
            "老人说，停住午夜本来是为了给人补救机会。但如果每个人都依赖“下一次”，当前这一次就会失去意义。",
            "老人",
        ),
        TopicId::ClerkDestination => (
            "售票员谈空白",
            "售票员说目的地可以空着，但空白不会保持中立。它会被恐惧、借口或别人替你写下的答案填满。",
            "售票窗口",
        ),
        TopicId::ClerkOneWay => (
            "售票员谈单程",
            "她说单程票容易，因为它只检查你自己。返程票难，因为它要确认你没有无视另一个人的意愿。",
            "售票窗口",
        ),
        TopicId::ClerkLog => (
            "售票员读日志",
            "她翻开站务日志，读到你的笔迹：如果我又来窗口，只问我是否还记得两张座位。售票员说，你以前至少知道问题在两个座位上。",
            "售票窗口",
        ),
        TopicId::ClerkSeats => (
            "售票员谈座位",
            "你问为什么每次都留两个座位。她说 07A 是你的位置，07B 是孩子的位置。返程票必须承认两个人都存在。",
            "售票窗口",
        ),
        TopicId::ClerkName => (
            "售票员谈姓名",
            "她说自己的名字被压在废票夹里。她为了保持“公事公办”放弃了姓名，久了以后也开始把别人当成手续。",
            "售票窗口",
        ),
        TopicId::ClerkPrice => (
            "售票员谈代价",
            "她说返程不是退掉愧疚，而是退掉“我已经够痛，所以不用负责”的想法。痛苦是真的，但不能替代行动。",
            "售票窗口",
        ),
        TopicId::LostFoundLabels => (
            "失物标签谈道歉",
            "你查看失物标签。很多道歉信被存放在这里。它们可以被取走，但取走以后就必须对应具体行动，不能继续当成自我安慰。",
            "失物招领",
        ),
        TopicId::LostFoundCabinet => (
            "铁柜谈真相",
            "你查看铁柜。铁柜深处保存站务档案，因为这些档案会直接指向白线旧案的责任。拿到真相不是结束，而是开始承担后果。",
            "失物招领",
        ),
        TopicId::LostFoundLantern => (
            "雾灯玻璃谈照见",
            "你查看雾灯玻璃。修好雾灯不会自动解决问题，但能照出月台远端、广播室和孩子身上的关键线索。",
            "雾灯",
        ),
        TopicId::LostFoundNameTag => (
            "姓名牌谈裂缝",
            "你查看姓名牌。裂缝穿过两个名字之间。它说明孩子的身份曾被你遗忘，也说明你需要把名字还给他本人。",
            "姓名牌",
        ),
        TopicId::LostFoundApology => (
            "空箱子谈放下",
            "你查看空箱子。箱底写着：放下被惩罚的需要。意思不是忘记孩子，而是别再用持续受苦代替补救。",
            "失物招领",
        ),
        TopicId::LostFoundLedger => (
            "账册谈无人领取",
            "你查看招领账册。无人领取的东西不会消失，会回到循环里，成为下一次可以找到的证据。",
            "账册",
        ),
        TopicId::UnderpassEcho => (
            "回声谈迟到",
            "你问回声为什么慢半拍。回声先重复孩子的姓，再重复你的名。它在帮你拼回被遗忘的身份。",
            "地下通道",
        ),
        TopicId::UnderpassWaterline => (
            "水线谈旧雨",
            "墙砖水线停在孩子肩膀高度。六年前那晚地下通道并没有真正进水，这条水线是车站保存的心理水位。",
            "地下通道",
        ),
        TopicId::UnderpassExit => (
            "疏散标志谈出口",
            "两个疏散标志指向相反方向。对孩子来说，白线曾被教成安全出口。对你来说，出口是承认你把安全提醒喊成了不准动。",
            "地下通道",
        ),
        TopicId::UnderpassLoop => (
            "申请记录谈午夜",
            "申请记录显示：停住 23:59 的人包括你。你曾请求保留最后一分钟，想找回失踪的同行者。",
            "循环机制",
        ),
        TopicId::UnderpassBroadcast => (
            "地下广播谈姓名",
            "地下广播提示：完整姓名可以打开广播室，但进入广播室的人可能会永远成为警告声音的一部分。",
            "广播室",
        ),
        TopicId::UnderpassStairs => (
            "台阶谈没有尽头",
            "台阶总会绕回原处。它提示你：问题不在路没有尽头，而在你每次都停下来重新审判自己，不继续前进。",
            "地下通道",
        ),
        TopicId::ChildWhiteLine => (
            "孩子谈白线",
            "孩子说白线保护他，也困住他。只要他站在线后，你就不能急着用道歉把旧命令包装成新保证。",
            "孩子",
        ),
        TopicId::ChildHomework => (
            "孩子谈作业",
            "他说作业题写不完，因为每次午夜重来，作业本都会回到第一页。他仍然写，是为了证明自己还在这里。",
            "孩子",
        ),
        TopicId::ChildPromise => (
            "孩子谈承诺",
            "他记得的承诺很具体：你说不准越过白线，到下一站就回来接他。后来灯灭了，你没有回来撤销这句话。",
            "孩子",
        ),
        TopicId::ChildAnger => (
            "孩子谈愤怒",
            "你问他能不能恨你。他说可以，但他不想只剩恨。他可以同时想离开，也不想立刻听你的。",
            "孩子",
        ),
        TopicId::ChildTomorrow => (
            "孩子谈明天",
            "孩子说他想要的明天很普通：热牛奶、靠窗座位，以及不用再证明自己很听话。",
            "孩子",
        ),
        TopicId::ChildLeaving => (
            "孩子谈上车以后",
            "你问如果上车后他仍害怕你怎么办。他说车票不是免罪券。他愿意走，不代表愿意替你擦掉过去。",
            "孩子",
        ),
        TopicId::KeeperClock => (
            "站务员谈零点",
            "站务员说，零点代表最后一分钟结束。只要有人拒绝承认这一点，旧钟就会继续停在 23:59。",
            "站务员",
        ),
        TopicId::KeeperLantern => (
            "站务员谈雾灯",
            "他说雾灯的作用不是制造道路，而是照出已经存在的道路和责任。灯亮以后，你就不能再说自己没看见。",
            "站务员",
        ),
        TopicId::KeeperTimetable => (
            "站务员谈时刻表",
            "他查看烧焦的旧时刻表。许多站名被涂改，说明雾灯站的规则并不是天生如此，而是有人改过。",
            "站务员",
        ),
        TopicId::KeeperBroadcast => (
            "站务员谈广播室",
            "他说广播室是一个结局方向。进入后，你的警告会被后来者听见，但你可能无法再作为普通乘客离开。",
            "站务员",
        ),
        TopicId::KeeperStay => (
            "站务员谈留下",
            "你问留下来是否也会伤人。他说会。留下可以帮助后来者，也可能变成要求别人感激你的负担。",
            "站务员",
        ),
        TopicId::KeeperCoat => (
            "站务员谈外套",
            "他解释外套代表站务员职责。穿上它以后，你会得到钥匙和广播权，但也会失去普通乘客的身份。",
            "站务员",
        ),
    };
    StoryEvent::new(title, body).tag(tag).tag("自由对话")
}
