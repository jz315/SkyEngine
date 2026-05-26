use crate::case_file;
use crate::model::{Ending, Flag, GameState};
use crate::station_request;
use crate::truth;

pub const ENDING_AFTERMATH_FRAGMENT_COUNT: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndingAftermathFragment {
    pub title: &'static str,
    pub body: String,
}

pub fn fragments(ending: Ending, state: &GameState) -> Vec<EndingAftermathFragment> {
    vec![
        route_aftermath(ending, state),
        witness_aftermath(ending, state),
        unresolved_aftermath(state),
        tomorrow_aftermath(ending, state),
    ]
}

pub fn ending_note(ending: Ending, state: &GameState) -> String {
    fragments(ending, state)
        .into_iter()
        .map(|fragment| format!("【余波：{}】{}", fragment.title, fragment.body))
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn completed_count(state: &GameState) -> usize {
    if state.ended.is_some() {
        ENDING_AFTERMATH_FRAGMENT_COUNT
    } else {
        0
    }
}

fn route_aftermath(ending: Ending, state: &GameState) -> EndingAftermathFragment {
    let body = match ending {
        Ending::LostPassenger => {
            if state.revealed_truth_scenes.is_empty() {
                "下一轮醒来时，湿票背面的字会更淡。你会觉得自己像被放过了，其实只是又被保存了一次。雾灯站最擅长把没有回答的问题伪装成休息。"
                    .to_string()
            } else {
                "下一轮醒来时，湿票背面会多出几道你认得的笔痕。真相没有救你，却让遗失不再纯粹；你至少知道自己是怎样被留在这里的。"
                    .to_string()
            }
        }
        Ending::EscapedAlone => {
            "天亮后，返程车厢没有掌声。07B 在你身边保持崭新，像一把没有声音的刀。你不是无罪离开，只是又一次让白线替你照看那个孩子。"
                .to_string()
        }
        Ending::NewStationKeeper => {
            "第一班误点车到来前，你把外套挂直，又故意没有扣上最上面那粒扣子。那是给后来者留出的缝：灯可以指路，但不能替人把路走完。"
                .to_string()
        }
        Ending::BurnedTimetable => {
            "旧时刻表烧完后，站台没有变成童话。有人站在自由里发抖，有人第一次不知道该向哪里求助。你烧掉的是车站的秩序，也必须承认秩序曾经替许多人挡过风。"
                .to_string()
        }
        Ending::TookChildHome => {
            "到站后的清晨很小：热牛奶、找不到的另一只袜子、作业本边角的折痕。孩子没有因此变成被治好的伤口，他只是开始拥有一间可以不听命令的房间。"
                .to_string()
        }
        Ending::BecameTheVoice => {
            "后来者听见你的声音时，不会知道你曾经有手、有票、有想逃的身体。如果你仍把警告说成命令，白线后的孩子就会一次次听见哥哥的旧嗓音。"
                .to_string()
        }
    };
    EndingAftermathFragment {
        title: "第二天",
        body,
    }
}

fn witness_aftermath(ending: Ending, state: &GameState) -> EndingAftermathFragment {
    let body = match ending {
        Ending::TookChildHome => {
            if state.has_flag(Flag::ChildJoined) || state.child_depth > 0 {
                "孩子会记得你跨过白线，但更会记得你有没有把手松开一点。他以后提起雾灯站时，可能不会说谢谢；他会说：那天我也选了。"
            } else {
                "他的名字仍在站台上发冷。你带走的若只是想象中的孩子，明天就会继续少一个真正能反驳你的人。"
            }
        }
        Ending::EscapedAlone => {
            if state.traveler_depth > 0 || state.has_flag(Flag::TravelerTrusted) {
                "老人会把 07B 的空座折进报纸。他不祝福你，也不审判你，只在下一轮乘客问起时说：有人离开过，所以离开不是传说。"
            } else {
                "老人仍坐在报纸后面。你没有真正认识他，于是他也无法替你的离开作证；空座继续像一个没有落款的括号。"
            }
        }
        Ending::NewStationKeeper => {
            if state.keeper_depth > 0 || state.has_flag(Flag::UnderstoodStationMechanism) {
                "旧站务员终于能把钥匙放在桌上。他没有被拯救，只是第一次允许自己承认：守夜太久的人，也会把照顾误认成所有权。"
            } else {
                "你接过外套，却还没有听完旧站务员为什么沉默。没有被理解的交接最危险，它会把前任的伤口当成制度继续穿上。"
            }
        }
        Ending::BurnedTimetable => {
            if state.clerk_depth > 0 || state.ticket.name().contains("返程") {
                "售票员会把票章收进抽屉。规则被烧掉以后，她终于不必替每个绝望的人扮演冷酷，但也失去了一套可以躲进去的答案。"
            } else {
                "售票窗口的玻璃仍亮着。你烧掉了纸，却没有真正听见写纸的人；自由因此带着一点粗暴的黑灰。"
            }
        }
        Ending::BecameTheVoice => {
            if state.has_flag(Flag::HeardBroadcastTape)
                || state.has_flag(Flag::SynthesizedStationTruth)
            {
                "广播室里原先的呼吸终于不再独自循环。两个声音隔着磁带互相让路，像两个人在黑暗里轮流守住一句别上车。"
            } else {
                "你成为新的声音，却还不知道旧声音曾是谁。警告因此有效，也因此残忍：它救人，却仍缺少一张应该被归还的脸。"
            }
        }
        Ending::LostPassenger => {
            if state.traveler_depth + state.clerk_depth + state.child_depth + state.keeper_depth > 0
            {
                "你见过的人会在下一轮留下极小的偏差：报纸慢半拍翻页，票章轻一点落下，白线后有人迟疑。你没有结束夜晚，却让夜晚变得没那么完美。"
            } else {
                "没有人能替你记得。雾灯站会把这次犹豫磨成普通误点，像从未有人试图问过为什么。"
            }
        }
    };
    EndingAftermathFragment {
        title: "留下的人",
        body: body.to_string(),
    }
}

fn unresolved_aftermath(state: &GameState) -> EndingAftermathFragment {
    let missing_truths = truth::truth_summaries(state)
        .into_iter()
        .filter(|summary| !summary.revealed)
        .map(|summary| summary.title.to_string())
        .collect::<Vec<_>>();
    let request_left =
        station_request::REQUEST_COUNT.saturating_sub(state.completed_requests.len());
    let case_left = case_file::CASE_FILE_COUNT.saturating_sub(state.resolved_case_files.len());

    let body = if missing_truths.is_empty() && request_left == 0 && case_left == 0 {
        "五层真相、旅客委托和站内档案都被你带到终点。雾灯站仍会疼，但它不能再把疼痛冒充成谜面；剩下的是人要怎样活，而不是系统还藏着什么。"
            .to_string()
    } else {
        let mut debts = Vec::new();
        if !missing_truths.is_empty() {
            debts.push(format!(
                "未揭开的真相：{}",
                missing_truths
                    .iter()
                    .take(3)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("、")
            ));
        }
        if request_left > 0 {
            debts.push(format!("{request_left} 件旅客委托还没有善后"));
        }
        if case_left > 0 {
            debts.push(format!("{case_left} 份站内档案仍未归位"));
        }
        format!(
            "{}。这些缺口不会把结局判成无效，但会让它带着具体的债：你知道自己没有抵达全部真相。下一轮不只是重开，也是补证。",
            debts.join("；")
        )
    };

    EndingAftermathFragment {
        title: "仍未归档",
        body,
    }
}

fn tomorrow_aftermath(ending: Ending, state: &GameState) -> EndingAftermathFragment {
    let response = ending
        .prelude()
        .and_then(|prelude| state.ending_prelude_response(prelude));
    let response_text = response
        .map(|response| match response {
            crate::model::EndingPreludeResponseId::AcceptCost => {
                "你在最后承认了代价，所以明天不会被写成廉价奖励。"
            }
            crate::model::EndingPreludeResponseId::ReturnChoice => {
                "你在最后没有替别人回答，所以明天会多一点不受你控制的主动。"
            }
            crate::model::EndingPreludeResponseId::RefuseControl => {
                "你在最后拒绝让车站替你定义答案，所以明天会保留锋利和不服从。"
            }
        })
        .unwrap_or("你没有留下最后回应，所以明天仍像一张缺少签名的票。");

    let route_text = match ending {
        Ending::LostPassenger => {
            "这不是坏结局提示，而是车站最熟练的保存方式：把没有说出口的话放进下一夜。"
        }
        Ending::EscapedAlone => {
            "独自离开以后，你还要学习不把活下来解释成背叛，也不把背叛解释成自由。"
        }
        Ending::NewStationKeeper => "留下以后，你每天都要问一次：我是在照路，还是在替别人决定路？",
        Ending::BurnedTimetable => "烧掉时刻表以后，真正困难的是不再用新的口号代替旧的规则。",
        Ending::TookChildHome => {
            "带孩子返程以后，真正困难的是允许他的明天不像你的补偿计划，也允许他不再听你的每一句话。"
        }
        Ending::BecameTheVoice => "成为广播以后，真正困难的是提醒别人，而不把提醒变成命令。",
    };

    EndingAftermathFragment {
        title: "明天",
        body: format!("{response_text}{route_text}"),
    }
}

impl Ending {
    fn prelude(self) -> Option<crate::model::EndingPreludeId> {
        match self {
            Self::LostPassenger => None,
            Self::EscapedAlone => Some(crate::model::EndingPreludeId::AloneDoor),
            Self::TookChildHome => Some(crate::model::EndingPreludeId::ChildWhiteLine),
            Self::BurnedTimetable => Some(crate::model::EndingPreludeId::TimetablePyre),
            Self::BecameTheVoice => Some(crate::model::EndingPreludeId::BroadcastBooth),
            Self::NewStationKeeper => Some(crate::model::EndingPreludeId::KeeperCoat),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EndingPreludeId, EndingPreludeResponseId, TruthSceneId};

    #[test]
    fn aftermath_fragments_explain_route_witness_debt_and_tomorrow() {
        let mut state = GameState::new();
        state.ended = Some(Ending::TookChildHome);
        state.remember(Flag::ChildJoined);
        state.child_depth = 3;
        state.reveal_truth_scene(TruthSceneId::ChildIsNotCargo);
        state.answer_ending_prelude(
            EndingPreludeId::ChildWhiteLine,
            EndingPreludeResponseId::ReturnChoice,
        );

        let fragments = fragments(Ending::TookChildHome, &state);
        assert_eq!(fragments.len(), ENDING_AFTERMATH_FRAGMENT_COUNT);
        assert!(fragments.iter().any(|fragment| {
            fragment.title == "第二天" && fragment.body.contains("可以不听命令的房间")
        }));
        assert!(fragments.iter().any(|fragment| {
            fragment.title == "留下的人" && fragment.body.contains("那天我也选了")
        }));
        assert!(fragments.iter().any(|fragment| {
            fragment.title == "仍未归档" && fragment.body.contains("未揭开的真相")
        }));
        assert!(fragments.iter().any(|fragment| {
            fragment.title == "明天" && fragment.body.contains("没有替别人回答")
        }));
    }

    #[test]
    fn completed_count_only_fills_after_an_ending() {
        let mut state = GameState::new();
        assert_eq!(completed_count(&state), 0);
        state.ended = Some(Ending::EscapedAlone);
        assert_eq!(completed_count(&state), ENDING_AFTERMATH_FRAGMENT_COUNT);
    }
}
