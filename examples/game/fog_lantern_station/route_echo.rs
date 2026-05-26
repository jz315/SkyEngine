use crate::model::{DialogueTone, Flag, GameState, RouteCostId, StoryEvent};
use crate::route_cost;

pub const ROUTE_ECHO_COUNT: usize = route_cost::ROUTE_COST_COUNT;

#[derive(Clone, Debug)]
pub struct RouteEchoAction {
    pub cost: RouteCostId,
    pub label: &'static str,
    pub detail: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteEchoSummary {
    pub cost: RouteCostId,
    pub title: &'static str,
    pub status: &'static str,
    pub detail: String,
    pub progress: u8,
    pub visible: bool,
    pub completed: bool,
    pub ready: bool,
}

pub fn available_echoes(state: &GameState) -> Vec<RouteEchoAction> {
    RouteCostId::ALL
        .iter()
        .copied()
        .filter(|cost| !state.has_completed_route_echo(*cost))
        .filter(|cost| cost.location() == state.location)
        .filter(|cost| echo_visible(state, *cost))
        .map(|cost| {
            let missing = missing_requirements(state, cost);
            RouteEchoAction {
                cost,
                label: echo_label(cost),
                detail: if missing.is_empty() {
                    "路线代价已经被带回人物身边。现在可以听听它怎样改变自由谈话。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                },
                enabled: missing.is_empty(),
            }
        })
        .collect()
}

pub fn echo_summaries(state: &GameState) -> Vec<RouteEchoSummary> {
    RouteCostId::ALL
        .iter()
        .copied()
        .map(|cost| {
            let completed = state.has_completed_route_echo(cost);
            let visible = completed || echo_visible(state, cost);
            let missing = missing_requirements(state, cost);
            let ready = visible && missing.is_empty() && !completed;
            let progress = echo_progress(missing.len(), visible, completed);
            let status = if completed {
                "已回声"
            } else if ready {
                "可交谈"
            } else if visible {
                "待回声"
            } else {
                "未显形"
            };
            let detail = if completed {
                echo_review(cost).to_string()
            } else if ready {
                format!(
                    "{}已经可以继续。前往{}，让调停过的代价回到自由对话里。",
                    echo_title(cost),
                    cost.location().title()
                )
            } else if visible {
                format!("还缺：{}。", missing.join("；"))
            } else {
                format!(
                    "先调停路线代价：{}。代价被看见以后，相关人物才会给出新的回答。",
                    cost.title()
                )
            };

            RouteEchoSummary {
                cost,
                title: echo_title(cost),
                status,
                detail,
                progress,
                visible,
                completed,
                ready,
            }
        })
        .collect()
}

pub fn discuss(state: &mut GameState, cost: RouteCostId) -> StoryEvent {
    if state.has_completed_route_echo(cost) {
        return StoryEvent::new(
            "这段回声已经落下",
            "你又把同一条路线的代价带回谈话里。对方没有厌烦，只是提醒你：听见以后，下一步要靠行动保住它。",
        )
        .tag("路线回声");
    }

    let missing = missing_requirements(state, cost);
    if cost.location() != state.location || !echo_visible(state, cost) || !missing.is_empty() {
        return StoryEvent::new(
            "路线回声还没有入口",
            format!(
                "你试着把终点的代价带回谈话，但这里还没有准备好接住它。{}",
                if cost.location() != state.location {
                    format!("这段回声不在这里，而在{}。", cost.location().title())
                } else if !echo_visible(state, cost) {
                    "先调停对应路线代价，人物才会回应它。".to_string()
                } else {
                    format!("还缺：{}。", missing.join("；"))
                }
            ),
        )
        .tag("路线回声");
    }

    state.complete_route_echo(cost);
    let mut event = echo_event(cost, state.dialogue_tone);
    apply_echo_rewards(state, cost, &mut event);
    event
}

fn echo_visible(state: &GameState, cost: RouteCostId) -> bool {
    state.has_mitigated_route_cost(cost) || state.has_completed_route_echo(cost)
}

fn missing_requirements(state: &GameState, cost: RouteCostId) -> Vec<&'static str> {
    let mut missing = Vec::new();
    require(
        &mut missing,
        state.has_mitigated_route_cost(cost),
        "先调停对应路线代价",
    );
    require(
        &mut missing,
        state.current_segment() >= 6,
        "进入第六段午夜以后",
    );
    missing
}

fn require(missing: &mut Vec<&'static str>, condition: bool, text: &'static str) {
    if !condition {
        missing.push(text);
    }
}

fn echo_progress(missing_count: usize, visible: bool, completed: bool) -> u8 {
    if completed {
        return 100;
    }
    if !visible {
        return 0;
    }
    (((2_usize.saturating_sub(missing_count)) * 100) / 2) as u8
}

fn echo_label(cost: RouteCostId) -> &'static str {
    match cost {
        RouteCostId::AloneEmptySeat => "路线回声：把空座说明读给老人",
        RouteCostId::ChildUnforgivenTomorrow => "路线回声：问他明天能不能继续生气",
        RouteCostId::BurnedTimetableAftercare => "路线回声：把善后清单交给失物柜",
        RouteCostId::BroadcastSecondName => "路线回声：听第二个名字怎样回放",
        RouteCostId::KeeperLightBoundary => "路线回声：和站务员确认守夜边界",
        RouteCostId::LostPassengerNotice => "路线回声：把完整警告念给月台",
    }
}

fn echo_title(cost: RouteCostId) -> &'static str {
    match cost {
        RouteCostId::AloneEmptySeat => "空座说明的回声",
        RouteCostId::ChildUnforgivenTomorrow => "害怕的回声",
        RouteCostId::BurnedTimetableAftercare => "善后清单的回声",
        RouteCostId::BroadcastSecondName => "第二个名字的回声",
        RouteCostId::KeeperLightBoundary => "守夜边界的回声",
        RouteCostId::LostPassengerNotice => "完整警告的回声",
    }
}

fn echo_review(cost: RouteCostId) -> &'static str {
    match cost {
        RouteCostId::AloneEmptySeat => "老人听过空座说明，独自离开的理由不再只由你保管。",
        RouteCostId::ChildUnforgivenTomorrow => "孩子听见明天允许坏心情，于是同行不再像被迫听话。",
        RouteCostId::BurnedTimetableAftercare => "失物柜收下善后清单，烧毁规则以前仍有人记得小事。",
        RouteCostId::BroadcastSecondName => "广播室记住第二个名字，警告从独白变成了点名。",
        RouteCostId::KeeperLightBoundary => "站务员确认守夜边界，灯光不再把疲惫说成债。",
        RouteCostId::LostPassengerNotice => "月台听完整警告，犹豫也被写成后来者能读懂的路标。",
    }
}

fn echo_event(cost: RouteCostId, tone: DialogueTone) -> StoryEvent {
    let title = format!("路线回声：{}", echo_title(cost));
    let body = match cost {
        RouteCostId::AloneEmptySeat => match tone {
            DialogueTone::Listening => "你没有急着解释，只把写给空座的说明放在老人报纸旁。老人读得很慢，慢到每个逗号都像一节车厢。他说：你终于不是来找我批准了。你点头。雾灯在长椅下亮了一下，像有人承认，空着的位置也需要被认真告别。",
            DialogueTone::Gentle => "你把空座说明读给老人听，声音放得很轻。老人没有说好，也没有说坏，只问：如果他以后讨厌这段说明呢？你说那就让他讨厌。老人笑了一下，说这才像一个真的空座，不像你替自己准备的奖状。",
            DialogueTone::Direct => "你把说明推到老人面前，说：我可能还是会一个人走，但我不能再把他写没。老人把报纸合上，说这句话终于不像请罪，像证词。候车厅短暂安静，连电子屏都停在一个没有目的地的空格上。",
        },
        RouteCostId::ChildUnforgivenTomorrow => match tone {
            DialogueTone::Listening => "你在白线内侧陪他站了很久，没有催他回答。孩子先开口：如果明天我还生气，你会不会又把我带回这里？你说不会。他说那我可以上车，但不是为了原谅你。你说我听见了。雾从轨道边退开一点。",
            DialogueTone::Gentle => "你告诉他，明天不需要装作快乐。他把作业本抱紧，说那我能不能把今天也带走？你说能，坏的也带走。他终于抬头，像确认明天不是一个只准乖孩子进入的房间。",
            DialogueTone::Direct => "你说：我不能再要求你用原谅证明我值得被救。孩子看你很久，说这句话我会记账。你说好。他把脚尖往白线内侧挪了半寸，像给这笔账开了一个可以慢慢偿还的账户。",
        },
        RouteCostId::BurnedTimetableAftercare => match tone {
            DialogueTone::Listening => "你把善后清单放进失物柜，听标签纸一张张轻响。那些没来得及的小事没有责备你，只是在柜子里重新排队。你忽然明白，烧掉旧时刻表不是把世界简化，而是先承认世界由许多不能被火概括的人组成。",
            DialogueTone::Gentle => "你把每件小委托的名字抄到清单上，像替火焰准备一张不会被吞掉的底稿。失物柜门轻轻合上，里面传来纸张贴齐的声音。它们不感谢你，只要求你别把善后说成豪情。",
            DialogueTone::Direct => "你对着失物柜说：如果我要烧掉规则，就必须先承认规则里还有人。标签纸猛地翻动，像有人终于等到这句不漂亮但准确的话。旧时刻表在你包里发热，却不再像一枚纯粹的怒火。",
        },
        RouteCostId::BroadcastSecondName => match tone {
            DialogueTone::Listening => "你让磁带空转一圈，直到第二个名字自己浮上来。站务员没有抢在回声前解释。他只是把音量调低，让那个名字能不被你的声音盖住。广播室第一次不像命令室，更像一间小小的录音棚。",
            DialogueTone::Gentle => "你把第二个名字轻轻补进广播稿。站务员问：如果后来者只听见你的警告呢？你说那就再播一遍，直到他们也听见旁边的人。磁带咔哒一声，像接受了一个不完美但愿意重复的办法。",
            DialogueTone::Direct => "你说：警告如果只保存我的声音，就还是占用。站务员把红色按钮推给你，说那你来删掉第一人称。你按下去，广播里先是一阵空白，随后两个名字并排出现，谁也没有吞掉谁。",
        },
        RouteCostId::KeeperLightBoundary => match tone {
            DialogueTone::Listening => "你读完守夜边界，没有要求站务员马上承认。旧钟楼里只剩齿轮声。过了很久，他说：原来灯也可以只照路，不审判被照到的人。你说可以。分针背面那道划痕慢慢暗下去。",
            DialogueTone::Gentle => "你把边界写给站务员看：疲惫不是债，留下也不是凭据。她用指尖按住那几行字，像怕它们飞走。你没有安慰她，只说今晚可以从这条规矩开始。钟楼的灯不再那么刺眼。",
            DialogueTone::Direct => "你告诉站务员：守夜不能变成索债。她先是皱眉，随后像终于被一句硬话救出旧姿势。她把外套搭回椅背，说那我也要被规则约束。你说是。旧钟咔地向前走了一格。",
        },
        RouteCostId::LostPassengerNotice => match tone {
            DialogueTone::Listening => "你把完整警告念给月台，念完以后没有追加解释。轨道尽头传来很轻的刹车声，又像有人在远处翻动车票。犹豫没有因此消失，却终于不再只属于你一个人。",
            DialogueTone::Gentle => "你把湿票背面的警告念得很慢，慢到每个后来者都能在句子里站稳。孩子问：这样他们就不会犯错了吗？你说不会，但他们至少会知道自己正在选择什么。白线亮起一条柔和的边。",
            DialogueTone::Direct => "你对月台说：逃走也是选择，留下别人替你解释也是选择。风从雾里冲出来，把湿票压在灯下。警告变得完整而不温柔，像一块路牌，不负责替人走路，只负责不再装作没有岔口。",
        },
    };
    StoryEvent::new(title, body)
}

fn apply_echo_rewards(state: &mut GameState, cost: RouteCostId, event: &mut StoryEvent) {
    match cost {
        RouteCostId::AloneEmptySeat => {
            remember_tag(state, event, Flag::TravelerTrusted, "回声：老人见证");
            state.child_trust = (state.child_trust + 1).min(5);
        }
        RouteCostId::ChildUnforgivenTomorrow => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedChildTruth,
                "回声：明天不等于原谅",
            );
            state.child_trust = 5;
        }
        RouteCostId::BurnedTimetableAftercare => {
            remember_tag(
                state,
                event,
                Flag::SynthesizedStationTruth,
                "回声：火前善后",
            );
        }
        RouteCostId::BroadcastSecondName => {
            remember_tag(state, event, Flag::HeardBroadcastTape, "回声：第二个名字");
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        RouteCostId::KeeperLightBoundary => {
            remember_tag(
                state,
                event,
                Flag::UnderstoodStationMechanism,
                "回声：守夜边界",
            );
            state.keeper_trust = (state.keeper_trust + 1).min(5);
        }
        RouteCostId::LostPassengerNotice => {
            remember_tag(state, event, Flag::InspectedRails, "回声：完整警告");
        }
    }
    event.tags.push("路线回声".to_string());
    event
        .tags
        .push(format!("语气：{}", state.dialogue_tone.name()));
    event
        .tags
        .push(format!("回声：{}", cost.ending().short_title()));
}

fn remember_tag(state: &mut GameState, event: &mut StoryEvent, flag: Flag, tag: &str) {
    if state.remember(flag) {
        event.tags.push(tag.to_string());
    }
}

pub fn ending_note(state: &GameState) -> Option<String> {
    let count = state.completed_route_echoes.len();
    if count == 0 {
        return None;
    }
    Some(format!(
        "你把 {} 段路线代价带回自由对话里。终点因此不只是一份条件清单，也多了几个人亲口承认过的余波。",
        count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn echo_summaries_track_visible_ready_and_completed_state() {
        let mut state = GameState::new();
        let initial = echo_summaries(&state);
        let alone = initial
            .iter()
            .find(|summary| summary.cost == RouteCostId::AloneEmptySeat)
            .expect("alone echo should be listed");
        assert_eq!(alone.status, "未显形");
        assert_eq!(alone.progress, 0);

        state.mitigate_route_cost(RouteCostId::AloneEmptySeat);
        state.actions_used = crate::model::ACTIONS_PER_SEGMENT * 5;
        let ready = echo_summaries(&state);
        let alone = ready
            .iter()
            .find(|summary| summary.cost == RouteCostId::AloneEmptySeat)
            .expect("alone echo should be listed");
        assert_eq!(alone.status, "可交谈");
        assert!(alone.ready);

        let event = discuss(&mut state, RouteCostId::AloneEmptySeat);
        assert!(event.tags.iter().any(|tag| tag == "路线回声"));
        assert!(state.has_completed_route_echo(RouteCostId::AloneEmptySeat));
    }

    #[test]
    fn echo_count_matches_route_costs() {
        assert_eq!(ROUTE_ECHO_COUNT, RouteCostId::ALL.len());
        assert_eq!(ROUTE_ECHO_COUNT, route_cost::ROUTE_COST_COUNT);
    }
}
