use crate::model::{Flag, GameState, Location, StationWhisperId, StoryEvent};

pub const WHISPER_COUNT: usize = 12;

#[derive(Clone, Debug)]
pub struct StationWhisperAction {
    pub whisper: StationWhisperId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StationWhisperSummary {
    pub whisper: StationWhisperId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub heard: bool,
    pub ready: bool,
}

pub fn available_whispers(state: &GameState) -> Vec<StationWhisperAction> {
    StationWhisperId::ALL
        .iter()
        .copied()
        .filter(|whisper| !state.has_heard_station_whisper(*whisper))
        .filter(|whisper| whisper.location() == state.location)
        .filter(|whisper| whisper_visible(state, *whisper))
        .map(|whisper| {
            let missing = missing_requirements(state, whisper);
            StationWhisperAction {
                whisper,
                label: whisper.label(),
                detail: if missing.is_empty() {
                    "这处地点已经安静到能听见低语。聆听不会替你做选择，只会让车站多说一句。"
                        .to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn whisper_summaries(state: &GameState) -> Vec<StationWhisperSummary> {
    StationWhisperId::ALL
        .iter()
        .copied()
        .map(|whisper| {
            let heard = state.has_heard_station_whisper(whisper);
            let visible = heard || whisper_visible(state, whisper);
            let missing = missing_requirements(state, whisper);
            let ready = visible && missing.is_empty() && !heard;
            let progress = whisper_progress(missing.len(), visible, heard);
            let status = if heard {
                "已听见"
            } else if ready {
                "可聆听"
            } else if visible {
                "待安静"
            } else {
                "未显形"
            };
            let detail = if heard {
                whisper.review().to_string()
            } else if ready {
                format!(
                    "{}已经可以聆听。前往{}，别急着把沉默填满。",
                    whisper.title(),
                    whisper.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "这段低语还藏在{}。推进午夜并调查地点，它才会愿意出声。",
                    whisper.location().title()
                )
            };

            StationWhisperSummary {
                whisper,
                title: whisper.title(),
                status,
                detail,
                progress,
                visible,
                heard,
                ready,
            }
        })
        .collect()
}

pub fn listen(state: &mut GameState, whisper: StationWhisperId) -> StoryEvent {
    if state.has_heard_station_whisper(whisper) {
        return StoryEvent::new(
            "低语已经被记下",
            "你又停在同一处沉默旁。它没有重复自己，只把刚才那句话往你心里推得更深一点。",
        )
        .tag("站内低语");
    }

    let missing = missing_requirements(state, whisper);
    if whisper.location() != state.location
        || !whisper_visible(state, whisper)
        || !missing.is_empty()
    {
        return StoryEvent::new(
            "这里还听不见",
            format!(
                "你试着让车站说话，但这里仍被太多脚步声盖住。{}",
                if whisper.location() != state.location {
                    format!("这段低语不在这里，而在{}。", whisper.location().title())
                } else if !whisper_visible(state, whisper) {
                    "继续推进午夜，低语才会浮出。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("站内低语");
    }

    state.hear_station_whisper(whisper);
    let mut event = whisper_event(whisper);
    apply_whisper_rewards(state, whisper, &mut event);
    event
}

impl StationWhisperId {
    pub const ALL: [Self; WHISPER_COUNT] = [
        Self::WaitingHallUmbrellaCount,
        Self::WaitingHallBenchReturn,
        Self::TicketOfficeStampHumidity,
        Self::TicketOfficePriceList,
        Self::LostFoundUmbrellaNames,
        Self::LostFoundApologyBox,
        Self::UnderpassReverseFootsteps,
        Self::UnderpassSaltLine,
        Self::ClockTowerGearPrayer,
        Self::ClockTowerCoatShadow,
        Self::PlatformBrakeDust,
        Self::PlatformSuitcaseLine,
    ];

    pub fn location(self) -> Location {
        match self {
            Self::WaitingHallUmbrellaCount | Self::WaitingHallBenchReturn => Location::WaitingHall,
            Self::TicketOfficeStampHumidity | Self::TicketOfficePriceList => Location::TicketOffice,
            Self::LostFoundUmbrellaNames | Self::LostFoundApologyBox => Location::LostAndFound,
            Self::UnderpassReverseFootsteps | Self::UnderpassSaltLine => Location::Underpass,
            Self::ClockTowerGearPrayer | Self::ClockTowerCoatShadow => Location::ClockTower,
            Self::PlatformBrakeDust | Self::PlatformSuitcaseLine => Location::Platform,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::WaitingHallUmbrellaCount => "站内低语：数伞架上的空位",
            Self::WaitingHallBenchReturn => "站内低语：听第七张长椅回潮",
            Self::TicketOfficeStampHumidity => "站内低语：听票章里的雨声",
            Self::TicketOfficePriceList => "站内低语：读价目表背面的细字",
            Self::LostFoundUmbrellaNames => "站内低语：核对伞柄上的姓名",
            Self::LostFoundApologyBox => "站内低语：打开装着道歉的纸箱",
            Self::UnderpassReverseFootsteps => "站内低语：听倒着走的脚步",
            Self::UnderpassSaltLine => "站内低语：摸墙根的盐线",
            Self::ClockTowerGearPrayer => "站内低语：听齿轮祷告",
            Self::ClockTowerCoatShadow => "站内低语：看外套没有影子的地方",
            Self::PlatformBrakeDust => "站内低语：擦掉刹车灯下的灰",
            Self::PlatformSuitcaseLine => "站内低语：读行李箱排成的线",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::WaitingHallUmbrellaCount => "伞架空位",
            Self::WaitingHallBenchReturn => "第七张长椅回潮",
            Self::TicketOfficeStampHumidity => "票章里的雨声",
            Self::TicketOfficePriceList => "价目表背面的细字",
            Self::LostFoundUmbrellaNames => "伞柄姓名",
            Self::LostFoundApologyBox => "装着道歉的纸箱",
            Self::UnderpassReverseFootsteps => "倒着走的脚步",
            Self::UnderpassSaltLine => "墙根盐线",
            Self::ClockTowerGearPrayer => "齿轮祷告",
            Self::ClockTowerCoatShadow => "没有影子的外套",
            Self::PlatformBrakeDust => "刹车灯下的灰",
            Self::PlatformSuitcaseLine => "行李箱排成的线",
        }
    }

    fn required_segment(self) -> u8 {
        match self {
            Self::WaitingHallUmbrellaCount
            | Self::TicketOfficeStampHumidity
            | Self::LostFoundUmbrellaNames
            | Self::UnderpassReverseFootsteps
            | Self::ClockTowerGearPrayer
            | Self::PlatformBrakeDust => 2,
            Self::WaitingHallBenchReturn
            | Self::TicketOfficePriceList
            | Self::LostFoundApologyBox
            | Self::UnderpassSaltLine
            | Self::ClockTowerCoatShadow
            | Self::PlatformSuitcaseLine => 5,
        }
    }

    fn required_depth(self) -> u8 {
        match self {
            Self::WaitingHallUmbrellaCount
            | Self::TicketOfficeStampHumidity
            | Self::LostFoundUmbrellaNames
            | Self::UnderpassReverseFootsteps
            | Self::ClockTowerGearPrayer
            | Self::PlatformBrakeDust => 2,
            Self::WaitingHallBenchReturn
            | Self::TicketOfficePriceList
            | Self::LostFoundApologyBox
            | Self::UnderpassSaltLine
            | Self::ClockTowerCoatShadow
            | Self::PlatformSuitcaseLine => 5,
        }
    }

    fn review(self) -> &'static str {
        match self {
            Self::WaitingHallUmbrellaCount => "伞架空位已经被数过，候车厅承认这里曾少过一个孩子。",
            Self::WaitingHallBenchReturn => "第七张长椅回过潮，返程不再只是车票上的两个字。",
            Self::TicketOfficeStampHumidity => "票章里的雨声被听见，售票窗口的公事口吻松动了一点。",
            Self::TicketOfficePriceList => "价目表背面的细字被读完，明天的价格不再伪装成免费。",
            Self::LostFoundUmbrellaNames => "伞柄姓名被核对过，失物招领处少了一点泛泛的抱歉。",
            Self::LostFoundApologyBox => "装着道歉的纸箱被打开，没来得及说的话终于有了重量。",
            Self::UnderpassReverseFootsteps => "倒着走的脚步被听见，地下通道不再只负责送人离开。",
            Self::UnderpassSaltLine => "墙根盐线被摸到，水线下面藏着旧循环留下的边界。",
            Self::ClockTowerGearPrayer => "齿轮祷告被听见，旧钟楼的沉默第一次像请求而不是命令。",
            Self::ClockTowerCoatShadow => "没有影子的外套被看见，守夜的代价不再只压在别人身上。",
            Self::PlatformBrakeDust => "刹车灯下的灰被擦开，月台承认雾灯号曾不止一次停下。",
            Self::PlatformSuitcaseLine => "行李箱排成的线被读懂，后来者不是背景，他们也在排队。",
        }
    }
}

fn whisper_visible(state: &GameState, whisper: StationWhisperId) -> bool {
    state.current_segment() >= whisper.required_segment()
}

fn missing_requirements(state: &GameState, whisper: StationWhisperId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.current_segment() >= whisper.required_segment(),
        "等到对应午夜段",
    );
    require(
        &mut missing,
        state.investigation_depth(whisper.location()) >= whisper.required_depth(),
        "继续调查这处地点",
    );
    missing
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn whisper_progress(missing_count: usize, visible: bool, heard: bool) -> u8 {
    if heard {
        return 100;
    }
    if !visible {
        return 0;
    }
    let total = 2_usize;
    (((total.saturating_sub(missing_count)) * 100) / total) as u8
}

fn whisper_event(whisper: StationWhisperId) -> StoryEvent {
    let (title, body) = match whisper {
        StationWhisperId::WaitingHallUmbrellaCount => (
            "站内低语：伞架空位",
            "伞架上有十二个洞，只有十一个湿圈。你数到第七个时，水滴忽然从下往上爬，像有一把小伞刚被某个矮个子旅客取走。候车厅没有人回头，只有电子屏闪出一行很淡的字：少掉的位置，不会因为没人坐就不存在。",
        ),
        StationWhisperId::WaitingHallBenchReturn => (
            "站内低语：第七张长椅回潮",
            "第七张长椅的木纹慢慢返潮，浮出两排浅浅的压痕，一大一小。你伸手摸它，像摸到某个雨夜里没来得及解释的并肩。长椅在指腹下轻响：离开不是把空位留给过去，离开是承认有人曾经坐在这里。",
        ),
        StationWhisperId::TicketOfficeStampHumidity => (
            "站内低语：票章里的雨声",
            "售票窗口的票章自己滚到玻璃边。你没有碰它，却听见里面有雨落在铁皮屋檐上的声音。每一声都像一个被盖掉的“不退”。玻璃后方传来售票员的呼吸，短得像她也曾经把某张票章按错在别人的明天上。",
        ),
        StationWhisperId::TicketOfficePriceList => (
            "站内低语：价目表背面的细字",
            "你把价目表从钉子上掀起，背面写着细得像尘的字：返程票不收费，只收取承认。承认少带了谁，承认想逃，承认有些回去并不是胜利。最末一行被水泡开：若有人替你付价，票作废。",
        ),
        StationWhisperId::LostFoundUmbrellaNames => (
            "站内低语：伞柄姓名",
            "失物招领处的旧伞一把把靠在墙角，伞柄上刻着不同人的名字。你看见自己的名字被划掉两次，旁边还有一个只刻到一半的小字。伞骨忽然撑开，雨声落在室内，像一群没被接走的人同时抬头。",
        ),
        StationWhisperId::LostFoundApologyBox => (
            "站内低语：装着道歉的纸箱",
            "纸箱标签写着“道歉，未签收”。你打开它，里面不是信，而是一枚枚空信封，封口都被反复舔湿又撕开。箱底贴着一句话：来不及说出口的话，不会自动变成理解。你合上箱盖时，里面轻轻叹了口气。",
        ),
        StationWhisperId::UnderpassReverseFootsteps => (
            "站内低语：倒着走的脚步",
            "地下通道里传来脚步声，先是远，后是近，再从你身后退回黑暗。它们不像追赶，更像某个人练习怎样把已经走过的路还给自己。墙砖渗出一行水字：回声不是重复，它只是比你晚一点承认真相。",
        ),
        StationWhisperId::UnderpassSaltLine => (
            "站内低语：墙根盐线",
            "你蹲下去，摸到墙根一条粗糙的白线。不是灰，是盐。它沿着地下通道绕了一圈，像有人曾经试图用最笨的方法挡住上涨的水。盐线尽头写着：能拦住水的东西，也可能拦住回家的人。",
        ),
        StationWhisperId::ClockTowerGearPrayer => (
            "站内低语：齿轮祷告",
            "旧钟楼的齿轮在没有转动时发出低低的祷告。它们祈求的不是继续，而是停下以后不要再被叫作忠诚。你听见一个老旧机械用几乎听不见的声音说：我不是时间，我只是被迫替时间作证。",
        ),
        StationWhisperId::ClockTowerCoatShadow => (
            "站内低语：没有影子的外套",
            "站务员的外套挂在椅背上，灯从后面照过去，墙上却没有影子。你靠近时，布料里传来许多人的名字，像口袋曾经装过太多未送达的通知。外套轻轻一沉：留下来的人，也不能把自己缝进职位里。",
        ),
        StationWhisperId::PlatformBrakeDust => (
            "站内低语：刹车灯下的灰",
            "三号月台的刹车灯下积着一层细灰。你用袖口擦开，底下不是水泥，而是一排排重叠的车轮印。雾灯号曾在这里停过很多次，每次都像第一次。灰尘粘在你手背上，像提醒你：循环也会留下重量。",
        ),
        StationWhisperId::PlatformSuitcaseLine => (
            "站内低语：行李箱排成的线",
            "月台尽头有一排无人认领的行李箱，排得比队伍还整齐。你逐个读它们的吊牌，发现目的地都写着“如果”。如果我早点说，如果他没有走，如果这班车真的开向明天。最后一个吊牌空着，像在等你不要再替它填答案。",
        ),
    };
    StoryEvent::new(title, body)
}

fn apply_whisper_rewards(state: &mut GameState, whisper: StationWhisperId, event: &mut StoryEvent) {
    match whisper {
        StationWhisperId::WaitingHallUmbrellaCount => {
            state.child_trust = (state.child_trust + 1).min(5);
        }
        StationWhisperId::WaitingHallBenchReturn => {
            remember_tag(state, event, Flag::UnderstoodFirstLoop, "低语：空位曾在");
        }
        StationWhisperId::TicketOfficeStampHumidity => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
        }
        StationWhisperId::TicketOfficePriceList => {
            remember_tag(state, event, Flag::SynthesizedRoute, "低语：返程票价");
        }
        StationWhisperId::LostFoundUmbrellaNames => {
            state.child_trust = (state.child_trust + 1).min(5);
        }
        StationWhisperId::LostFoundApologyBox => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "低语：道歉未签收",
            );
        }
        StationWhisperId::UnderpassReverseFootsteps => {
            remember_tag(state, event, Flag::HeardUnderpassEcho, "低语：倒走回声");
        }
        StationWhisperId::UnderpassSaltLine => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "低语：盐线边界",
            );
        }
        StationWhisperId::ClockTowerGearPrayer => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        StationWhisperId::ClockTowerCoatShadow => {
            remember_tag(state, event, Flag::HeardClockTruth, "低语：无影外套");
        }
        StationWhisperId::PlatformBrakeDust => {
            remember_tag(state, event, Flag::ReadPlatformLedger, "低语：刹车痕");
        }
        StationWhisperId::PlatformSuitcaseLine => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "低语：后来者队列",
            );
        }
    }
    event.tags.push("站内低语".to_string());
    event
        .tags
        .push(format!("地点：{}", whisper.location().title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let heard = state.heard_station_whispers.len();
    if heard < 4 {
        return None;
    }
    Some(format!(
        "你听过 {} 段站内低语。那些没有成为任务的声音仍然跟到终点，提醒你车站不是谜题，而是一群没被好好听完的人。",
        heard
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whisper_summaries_track_ready_and_heard_state() {
        let mut state = GameState::new();
        let initial = whisper_summaries(&state);
        let umbrella = initial
            .iter()
            .find(|summary| summary.whisper == StationWhisperId::WaitingHallUmbrellaCount)
            .expect("waiting hall whisper should be listed");
        assert_eq!(umbrella.status, "未显形");
        assert_eq!(umbrella.progress, 0);

        state.actions_used = crate::model::ACTIONS_PER_SEGMENT;
        state.location_depths[Location::WaitingHall.index()] = 2;
        let ready = whisper_summaries(&state);
        let umbrella = ready
            .iter()
            .find(|summary| summary.whisper == StationWhisperId::WaitingHallUmbrellaCount)
            .expect("waiting hall whisper should be listed");
        assert_eq!(umbrella.status, "可聆听");
        assert!(umbrella.ready);

        let event = listen(&mut state, StationWhisperId::WaitingHallUmbrellaCount);
        assert!(event.tags.iter().any(|tag| tag == "站内低语"));
        assert!(state.has_heard_station_whisper(StationWhisperId::WaitingHallUmbrellaCount));
    }

    #[test]
    fn whisper_count_covers_two_per_location() {
        assert_eq!(WHISPER_COUNT, StationWhisperId::ALL.len());
        for location in Location::ALL {
            let count = StationWhisperId::ALL
                .iter()
                .filter(|whisper| whisper.location() == location)
                .count();
            assert_eq!(count, 2);
        }
    }
}
