use crate::model::{EvidenceId, Flag, GameState, Item, Location, StoryEvent};

#[derive(Clone, Debug)]
pub struct EvidenceAction {
    pub evidence: EvidenceId,
    pub label: &'static str,
    pub detail: &'static str,
    pub enabled: bool,
}

pub fn available_presentations(state: &GameState, location: Location) -> Vec<EvidenceAction> {
    let evidence: &[EvidenceId] = match location {
        Location::WaitingHall => &[
            EvidenceId::TravelerMirror,
            EvidenceId::TravelerCoinToken,
            EvidenceId::TravelerRoster,
        ],
        Location::TicketOffice => &[
            EvidenceId::ClerkCoinToken,
            EvidenceId::ClerkRoster,
            EvidenceId::ClerkMirror,
        ],
        Location::ClockTower => &[
            EvidenceId::KeeperStationLog,
            EvidenceId::KeeperBroadcastTape,
            EvidenceId::KeeperMirror,
        ],
        Location::Platform => &[
            EvidenceId::ChildTicket,
            EvidenceId::ChildHomework,
            EvidenceId::ChildMirror,
        ],
        Location::LostAndFound | Location::Underpass => &[],
    };

    evidence
        .iter()
        .copied()
        .filter(|evidence| evidence_visible(state, *evidence))
        .map(|evidence| EvidenceAction {
            evidence,
            label: evidence_label(evidence),
            detail: evidence_detail(state, evidence),
            enabled: !state.has_presented(evidence) && evidence_enabled(state, evidence),
        })
        .collect()
}

pub fn present(state: &mut GameState, evidence: EvidenceId) -> StoryEvent {
    if state.has_presented(evidence) {
        return StoryEvent::new(
            "证据已经说过",
            "你又把同一件东西递出去。对方没有不耐烦，只是雾灯先替他们把结论照了一遍：真正需要变化的不是证据，是你下一次怎么选择。",
        )
        .tag("证据追问");
    }

    if !evidence_visible(state, evidence) || !evidence_enabled(state, evidence) {
        return StoryEvent::new(
            "证据还没有对象",
            "你把线索拿在手里，却还没有找到合适的人、合适的时刻，或合适的勇气。车站不缺物证，缺的是愿意被物证改变的人。",
        )
        .tag("证据追问");
    }

    state.present(evidence);
    let mut event = evidence_event(evidence);
    apply_evidence_rewards(state, evidence, &mut event);
    event
}

fn evidence_visible(state: &GameState, evidence: EvidenceId) -> bool {
    match evidence {
        EvidenceId::TravelerMirror => state.has_item(Item::MirrorShard),
        EvidenceId::TravelerCoinToken => state.has_item(Item::CoinToken),
        EvidenceId::TravelerRoster => state.has_item(Item::ConductorRoster),
        EvidenceId::ClerkCoinToken => state.has_item(Item::CoinToken),
        EvidenceId::ClerkRoster => state.has_item(Item::ConductorRoster),
        EvidenceId::ClerkMirror => state.has_item(Item::MirrorShard),
        EvidenceId::ChildTicket => {
            state.has_flag(Flag::MetChild) && state.has_flag(Flag::ExaminedTicket)
        }
        EvidenceId::ChildHomework => {
            state.has_flag(Flag::MetChild) && state.has_item(Item::ChildHomework)
        }
        EvidenceId::ChildMirror => {
            state.has_flag(Flag::MetChild) && state.has_item(Item::MirrorShard)
        }
        EvidenceId::KeeperStationLog => state.has_item(Item::StationLog),
        EvidenceId::KeeperBroadcastTape => state.has_item(Item::BroadcastTape),
        EvidenceId::KeeperMirror => {
            state.has_item(Item::MirrorShard)
                && (state.keeper_depth > 0 || state.has_flag(Flag::HeardClockTruth))
        }
    }
}

fn evidence_enabled(_state: &GameState, _evidence: EvidenceId) -> bool {
    true
}

fn evidence_label(evidence: EvidenceId) -> &'static str {
    match evidence {
        EvidenceId::TravelerMirror => "把候车厅镜片递给老人",
        EvidenceId::TravelerCoinToken => "让老人看退票铜筹背面的字",
        EvidenceId::TravelerRoster => "把列车员名册缺页讲给老人听",
        EvidenceId::ClerkCoinToken => "把退票铜筹放到窗口凹槽里",
        EvidenceId::ClerkRoster => "让售票员核对列车员名册",
        EvidenceId::ClerkMirror => "把镜片贴到售票窗口玻璃上",
        EvidenceId::ChildTicket => "把湿票摊给孩子看，先不解释",
        EvidenceId::ChildHomework => "和孩子一起看那道永远写不完的题",
        EvidenceId::ChildMirror => "把镜片放低，让孩子也能照见",
        EvidenceId::KeeperStationLog => "把站务日志交给站务员翻到末页",
        EvidenceId::KeeperBroadcastTape => "把广播磁带放到旧钟旁",
        EvidenceId::KeeperMirror => "让站务员看镜片里的半张脸",
    }
}

fn evidence_detail(state: &GameState, evidence: EvidenceId) -> &'static str {
    if state.has_presented(evidence) {
        return "这件证据已经追问过，记录在日志里。";
    }

    match evidence {
        EvidenceId::TravelerMirror => "老人也许认得镜片里那个比你更疲惫的人。",
        EvidenceId::TravelerCoinToken => "铜筹背面写着返程需要两个人承认。",
        EvidenceId::TravelerRoster => "名册缺页能问出雾灯号上是否真的有人负责返程。",
        EvidenceId::ClerkCoinToken => "窗口只办理退票，铜筹会让她停止装作没看见。",
        EvidenceId::ClerkRoster => "列车员名册能迫使她承认返程不是传说。",
        EvidenceId::ClerkMirror => "镜片会让窗口内外的身份短暂对调。",
        EvidenceId::ChildTicket => "需要见过孩子，并先检查过湿票。",
        EvidenceId::ChildHomework => "他写下的问题，比大人的歉意更接近出口。",
        EvidenceId::ChildMirror => "让他决定自己愿不愿意看见你想起的脸。",
        EvidenceId::KeeperStationLog => "日志末页写着你曾经申请停住午夜。",
        EvidenceId::KeeperBroadcastTape => "磁带会把广播室从传闻变成代价。",
        EvidenceId::KeeperMirror => "需要先让站务员开口谈过旧钟或代价。",
    }
}

fn apply_evidence_rewards(state: &mut GameState, evidence: EvidenceId, event: &mut StoryEvent) {
    match evidence {
        EvidenceId::TravelerMirror => {
            state.remember(Flag::TravelerTrusted);
            state.remember(Flag::UnderstoodFirstLoop);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("老人信任".to_string());
        }
        EvidenceId::TravelerCoinToken => {
            state.remember(Flag::TravelerTrusted);
            state.child_trust = (state.child_trust + 1).min(5);
            event.tags.push("返程规则".to_string());
        }
        EvidenceId::TravelerRoster => {
            state.remember(Flag::FoundRoster);
            state.remember(Flag::UnderstoodStationMechanism);
            event.tags.push("理解：列车职责".to_string());
        }
        EvidenceId::ClerkCoinToken => {
            state.clerk_trust = (state.clerk_trust + 2).min(5);
            event.tags.push("售票员信任".to_string());
        }
        EvidenceId::ClerkRoster => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            state.remember(Flag::FoundRoster);
            event.tags.push("返程证词".to_string());
        }
        EvidenceId::ClerkMirror => {
            state.clerk_trust = (state.clerk_trust + 1).min(5);
            event.tags.push("窗口裂纹".to_string());
        }
        EvidenceId::ChildTicket => {
            state.remember(Flag::UnderstoodChildPromise);
            state.child_trust = (state.child_trust + 2).min(5);
            event.tags.push("理解：两个名字".to_string());
        }
        EvidenceId::ChildHomework => {
            state.child_trust = (state.child_trust + 1).min(5);
            if state.has_flag(Flag::RecoveredName) {
                state.remember(Flag::SynthesizedChildTruth);
                event.tags.push("合成：孩子与姓名".to_string());
            }
        }
        EvidenceId::ChildMirror => {
            state.child_trust = (state.child_trust + 1).min(5);
            state.remember(Flag::UnderstoodChildPromise);
            event.tags.push("孩子信任".to_string());
        }
        EvidenceId::KeeperStationLog => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            state.remember(Flag::UnderstoodStationMechanism);
            event.tags.push("理解：车站机制".to_string());
        }
        EvidenceId::KeeperBroadcastTape => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            state.remember(Flag::HeardBroadcastTape);
            event.tags.push("广播室线索".to_string());
        }
        EvidenceId::KeeperMirror => {
            state.keeper_trust = (state.keeper_trust + 1).min(5);
            state.remember(Flag::HeardClockTruth);
            event.tags.push("旧钟代价".to_string());
        }
    }
}

fn evidence_event(evidence: EvidenceId) -> StoryEvent {
    let (title, body, tag) = match evidence {
        EvidenceId::TravelerMirror => (
            "老人看见镜片里的你",
            "老人接过镜片，只看了一眼就把它反扣在报纸上。他说：这不是镜子，是上一次的你留下来的取证口。你每次都以为自己第一次醒来，其实只是第一次愿意看见醒来以前的脸。",
            "老人",
        ),
        EvidenceId::TravelerCoinToken => (
            "老人辨认退票铜筹",
            "老人用拇指摩挲铜筹背面的字，像摸一枚很旧的伤疤。他说返程从来不是车站的恩典，返程是两个人互相承认还活着。少一个人承认，铜筹就只是一枚漂亮的借口。",
            "老人",
        ),
        EvidenceId::TravelerRoster => (
            "老人讲名册缺页",
            "你提到列车员名册最后一页被撕走。老人沉默很久，说雾灯号当然有列车员，只是他们不查票，他们查乘客有没有把别人写成行李、责任或遗憾。",
            "老人",
        ),
        EvidenceId::ClerkCoinToken => (
            "铜筹落进窗口",
            "退票铜筹落进窗口凹槽，声音很轻，售票员的营业微笑却像被那一声敲裂。她说：原来你不是来问能不能走，你是来问该退还什么。那我们终于可以开始办理了。",
            "售票窗口",
        ),
        EvidenceId::ClerkRoster => (
            "售票员核对名册",
            "她翻到名册缺页处，指尖停在空栏旁。她说返程班次一直存在，只是系统会把它隐藏给只输入一个姓名的人看。你若要两个座位，就别再把同行者写进备注。",
            "售票窗口",
        ),
        EvidenceId::ClerkMirror => (
            "镜片贴上窗口",
            "你把镜片贴到售票窗口玻璃上。裂纹把你和售票员的脸短暂拼在一起，她像忽然看见自己也曾站在外面。她低声说：手续不是墙，可我们用得久了，就忘了它原来是门。",
            "售票窗口",
        ),
        EvidenceId::ChildTicket => (
            "孩子读湿票",
            "你把湿票摊在白线边，没有解释，也没有急着替自己辩护。孩子先看背面的“别上车”，又看水痕里那半句。他说：原来你不是没看见，是每次都只看见你比较受得住的那半句。",
            "孩子",
        ),
        EvidenceId::ChildHomework => (
            "作业题的答案",
            "你和他一起看那道题：如果哥哥说等我，等多久才算听话？孩子把铅笔递给你，却没有让你写。他说这次你不用替我答，你只要坐在旁边，别把我的沉默当成原谅。",
            "孩子",
        ),
        EvidenceId::ChildMirror => (
            "镜片放低",
            "你把镜片放低，让他也能照见。孩子没有看你的脸，先看自己的鞋、作业本和握紧的手。他说：这就够了。大人总想让我看见他们痛苦，其实我只是想确认自己还在画面里。",
            "孩子",
        ),
        EvidenceId::KeeperStationLog => (
            "站务日志的末页",
            "站务员翻到日志末页，看见你的笔迹后没有辩解。他说：我批准了申请，因为那时你看起来像再多一分钟就能救下所有人。我没有告诉你，时间一旦借出，就会要求利息。",
            "站务员",
        ),
        EvidenceId::KeeperBroadcastTape => (
            "磁带放到旧钟旁",
            "广播磁带靠近旧钟，齿轮里传出你自己的声音，一遍遍练习警告又一遍遍停下。站务员说广播室会成全勇敢，也会利用勇敢。留下来的声音，最容易被误认为答案。",
            "站务员",
        ),
        EvidenceId::KeeperMirror => (
            "镜片里的站务员",
            "站务员接过镜片，里面却没有他的脸，只有椅背上那件没有影子的外套。他笑了一下，说：你看，做站务员久了，人就会先失去脸，再失去被别人叫回去的可能。",
            "站务员",
        ),
    };
    StoryEvent::new(title, body).tag(tag).tag("证据追问")
}
