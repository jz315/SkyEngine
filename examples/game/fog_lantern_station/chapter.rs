use crate::model::{Flag, GameState, Location, StoryEvent};

pub fn segment_shift_event(state: &GameState, previous_segment: u8) -> Option<StoryEvent> {
    let segment = state.current_segment();
    if segment == previous_segment || state.final_train_due() {
        return None;
    }

    let (title, body, tag) = match segment {
        2 => (
            "车站变化：屏幕开始记账",
            "电子时刻表不再只显示车次。它偶尔闪出你刚刚做过的事，又很快擦掉，像车站在学习怎样把行动变成证词。从现在起，留在原地太久会让更多线索互相错过。",
            "章节：行动留痕",
        ),
        3 => (
            "车站变化：窗口有了回声",
            "售票窗口后多出第二层玻璃。每当你经过，里面都会响起票章落下的声音，却没有任何纸张承受它。返程、退票、两个座位，这些词开始在窗口附近变得更重。",
            "章节：返程规则",
        ),
        4 => (
            "车站变化：地下水线升高",
            "地下通道墙砖上的水线往上挪了一寸。回声不再只是重复脚步，它开始提前说出某些你还没敢说的话。姓名若还没回来，它会越来越像最后一件能被车站扣留的行李。",
            "章节：姓名压力",
        ),
        5 => (
            "车站变化：白线发亮",
            "三号月台的白线在雾里亮了一次，像孩子用力描过一遍。车站开始分辨你是把他当成同行者，还是仍把他当成证明自己痛苦的证物。",
            "章节：同行者",
        ),
        6 => (
            "车站变化：旧钟漏下一秒",
            "旧钟楼传来一声极轻的咔哒。不是整点，是时间从齿轮缝里漏下一粒。站务员的外套垂得更低，像有人在提醒你：留下也有代价，离开也有代价。",
            "章节：代价",
        ),
        7 => (
            "车站变化：广播室显形",
            "雾里那扇广播室的窄门不再完全隐藏。你还看不清门后的房间，只能看见门牌像湿纸一样贴着黑暗。完整姓名、旧钟、磁带和雾灯开始互相呼叫。",
            "章节：广播室",
        ),
        8 => (
            "车站变化：雾灯号试刹",
            "铁轨深处传来第一次真正的刹车声。雾没有散开，反而站得更整齐，像一排等你点名的乘客。车站已经把所有没完成的选择推到月台边。",
            "章节：终段",
        ),
        _ => (
            "车站变化：午夜换气",
            "车站轻轻换了一次气，许多声音退回墙里，许多尚未发生的结果向你靠近一寸。",
            "章节变化",
        ),
    };

    Some(StoryEvent::new(title, body).tag(tag).tag("车站变化"))
}

pub fn phase_label(state: &GameState) -> &'static str {
    match state.current_segment() {
        1 => "观察期：建立第一批证据",
        2 => "行动留痕：屏幕开始记账",
        3 => "返程规则：窗口有了回声",
        4 => "姓名压力：地下水线升高",
        5 => "同行者：月台白线发亮",
        6 => "代价：旧钟漏下一秒",
        7 => "广播室：窄门显形",
        _ => "终段：雾灯号试刹",
    }
}

pub fn pressure_note(state: &GameState) -> &'static str {
    match state.current_segment() {
        1 => "适合建立第一批证据。",
        2 => "多换地点，别让行动变成同一种犹豫。",
        3 => "返程、退票和两个座位开始变重。",
        4 => "姓名正在成为最紧的线索。",
        5 => "孩子会检验你是否把他当作同行者。",
        6 => "旧钟和站务员会逼近代价问题。",
        7 => "广播室、磁带和完整姓名开始互相呼叫。",
        _ => "所有路线正在收束，选择已经靠站。",
    }
}

pub fn location_shift(location: Location, segment: u8) -> Option<&'static str> {
    if segment < 2 {
        return None;
    }

    match location {
        Location::WaitingHall => Some(match segment {
            2..=3 => " 电子屏偶尔闪出你刚刚做过的事，像在练习把行动写成证词。",
            4..=6 => " 长椅下的阴影变深，缺失的 07 号座位仿佛刚被人推走。",
            _ => " 候车厅像快醒了，所有长椅都朝三号月台偏了一点。",
        }),
        Location::TicketOffice => Some(match segment {
            2..=3 => " 窗口后的票章声变密，像有人正在替你预演退票手续。",
            4..=6 => " 玻璃内侧的裂纹亮了一下，把窗口切成两张互相审问的脸。",
            _ => " 售票窗口的绿灯不再温和，它像一只不肯替你眨的眼睛。",
        }),
        Location::LostAndFound => Some(match segment {
            2..=3 => " 失物标签开始轻轻翻面，背后全是同一句：请本人领取。",
            4..=6 => " 柜门缝里透出冷绿光，像雾灯玻璃在提醒你别再只收集歉意。",
            _ => " 最深处的铁柜传来敲门声，仿佛里面的不是物品，而是尚未承担的后果。",
        }),
        Location::Underpass => Some(match segment {
            2..=3 => " 回声比刚才更清楚，却仍故意慢半拍，像怕你太快把它变成答案。",
            4..=6 => " 墙砖水线升高了一寸，潮气把你的名字推到喉咙附近。",
            _ => " 地下通道尽头的墙变得干燥，像在等你承认真相不等于出口。",
        }),
        Location::ClockTower => Some(match segment {
            2..=3 => " 齿轮之间多出纸页摩擦声，像旧时刻表正在梦里翻身。",
            4..=6 => " 旧钟偶尔漏下一声咔哒，让 23:59 听起来不再像慈悲。",
            _ => " 站务员的外套垂到椅脚，影子的空缺几乎有了重量。",
        }),
        Location::Platform => Some(match segment {
            2..=3 => " 远处红灯亮得更频繁，仿佛列车在雾里练习靠近。",
            4..=6 => " 月台白线泛着湿光，孩子站在那里，像一封终于不能再延迟的回信。",
            _ => " 铁轨开始轻微震动，雾灯号还没到，但选择已经先到了。",
        }),
    }
}

pub fn objective_hint(state: &GameState) -> Option<&'static str> {
    let segment = state.current_segment();
    if segment >= 4 && !state.has_flag(Flag::RecoveredName) {
        return Some(
            "第四段以后，地下通道会持续加压：尽快找回姓名，否则很多选择只会像替别人签字。",
        );
    }
    if segment >= 5 && !state.has_flag(Flag::MetChild) {
        return Some("第五段以后，三号月台的白线会发亮：去见孩子，别让他只存在于别人的证词里。");
    }
    if segment >= 6 && !state.has_flag(Flag::HeardClockTruth) {
        return Some("第六段以后，旧钟开始漏秒：去钟楼听站务员说清最后一分钟的代价。");
    }
    if segment >= 7 && !state.has_flag(Flag::SynthesizedStationTruth) {
        return Some(
            "第七段以后，广播室会显形：整理磁带、旧钟、雾灯和姓名，决定声音是否值得留下。",
        );
    }
    None
}
