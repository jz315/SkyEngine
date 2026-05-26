use crate::model::{Flag, GameState, Location, StoryEvent};

pub fn segment_event(state: &GameState, previous_segment: u8) -> Option<StoryEvent> {
    let segment = state.current_segment();
    if segment == previous_segment || state.final_train_due() {
        return None;
    }

    let title = match segment {
        2 => "阶段提示：第二段午夜",
        3 => "阶段提示：第三段午夜",
        4 => "阶段提示：第四段午夜",
        5 => "阶段提示：第五段午夜",
        6 => "阶段提示：第六段午夜",
        7 => "阶段提示：第七段午夜",
        8 => "阶段提示：第八段午夜",
        _ => "阶段提示：午夜推进",
    };

    let body = format!(
        "{}{}{}",
        segment_current(segment),
        location_refrain(state.location),
        memory_refrain(state)
    );

    Some(StoryEvent::new(title, body).tag("阶段提示").tag("午夜推进"))
}

fn segment_current(segment: u8) -> &'static str {
    match segment {
        2 => "时间进入第二段。车站开始出现更多异常：屏幕会记录你的行动，部分地点会出现新线索。",
        3 => "时间进入第三段。你已经收集到足够线索，可以开始确认循环是否和自己有关。",
        4 => "时间进入第四段。孩子、白线和返程票的关系会变得更重要。优先推进月台和售票窗口。",
        5 => "时间进入第五段。旧钟楼和广播线开始给出关键答案：午夜为什么停住，以及谁申请停住它。",
        6 => "时间进入第六段。你需要把分散证据整理成结论，准备选择一种离站路线。",
        7 => {
            "时间进入第七段。路线代价会显现：独自上车、带孩子离开、烧掉规则、进入广播室、留下守夜。"
        }
        8 => "时间进入第八段。列车很快会进站。现在应完成终局前谈话和路线准备，别再只做普通调查。",
        _ => "时间继续推进。请检查右侧行动列表，选择能推进线索或结局条件的行动。",
    }
}

fn location_refrain(location: Location) -> &'static str {
    match location {
        Location::WaitingHall => " 候车厅仍可提供车票、时刻表、老人证词和座位线索。",
        Location::TicketOffice => " 售票窗口可以推进返程票、退票铜筹和座位规则。",
        Location::LostAndFound => " 失物招领处可以找到物件证据，尤其是雾灯玻璃、姓名牌和站务档案。",
        Location::Underpass => " 地下通道会补全姓名、白线旧案和循环申请。",
        Location::ClockTower => " 旧钟楼会解释时间机制、广播室和站务员的代价。",
        Location::Platform => " 三号月台会推进孩子、白线、轨道轮痕和列车进站条件。",
    }
}

fn memory_refrain(state: &GameState) -> &'static str {
    if state.has_flag(Flag::ChildJoined) {
        " 孩子已经愿意同行。接下来要准备返程或最终选择。"
    } else if state.has_flag(Flag::MetChild) {
        " 你已经见到孩子，但他还不完全信任你。继续推进他的对话和相关证据。"
    } else if state.has_flag(Flag::RecoveredName) {
        " 你已经想起自己的名字。下一步是确认孩子的名字和返程条件。"
    } else if state.has_flag(Flag::ExaminedTicket) {
        " 你已经检查过湿票。继续找镜片、铜筹或孩子线索，读出车票完整含义。"
    } else {
        " 你还没有想起姓名。先检查车票、候车厅和地下通道。"
    }
}
