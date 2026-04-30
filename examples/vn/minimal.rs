//! Headless visual novel runtime example.
//!
//! Run with:
//!
//! ```bash
//! cargo run --example vn_minimal --features vn
//! ```

use sky_engine::vn::{
    VnAction, VnPreferences, VnRollbackReason, VnRollbackStack, VnRuntime, VnRuntimeEvent,
    VnSaveStore, VnValue, YarnScript,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let script = YarnScript::parse_str(
        r#"
title: Start
---
<<scene "images/bg/classroom.png" transition="fade" duration=0.4>>
<<play_bgm "audio/bgm/morning.ogg" loop=true fade=1.0 volume=0.8>>
<<show alice "images/characters/alice/smile.png" expression="smile" at="right" z=10>>
<<voice alice "audio/voice/alice/0001.ogg">>
Alice: Morning. #line:start.alice.0001
-> Walk with Alice
    <<set $route = "alice">>
    <<jump Ending>>
-> Walk alone <<if $can_go_alone>>
    <<set $route = "alone">>
    <<jump Ending>>
===

title: Ending
---
See you. #line:ending.narrator.0001
===
"#,
    )?;

    let mut runtime = VnRuntime::from_script(script, "Start")?;
    runtime.set_variable("can_go_alone", VnValue::Bool(true));
    let mut rollback = VnRollbackStack::default();

    loop {
        let Some(event) = runtime.apply_action(VnAction::Advance)? else {
            continue;
        };
        match event {
            VnRuntimeEvent::Line(line) => {
                rollback.push_runtime(VnRollbackReason::Line, &runtime);
                runtime.dialogue_mut().complete_line();
                if let Some(speaker) = &line.speaker {
                    println!("{speaker}: {}", line.text);
                } else {
                    println!("{}", line.text);
                }
            }
            VnRuntimeEvent::Choices(choices) => {
                for (index, choice) in choices.iter().enumerate() {
                    println!("{index}: {}", choice.text);
                }
                runtime.choose(0)?;
            }
            VnRuntimeEvent::Command(command) => {
                println!("command: {}", command.raw);
            }
            VnRuntimeEvent::Wait(seconds) => {
                println!("wait: {seconds:.2}s");
                runtime.complete_wait();
            }
            VnRuntimeEvent::End => break,
        }
    }

    println!("route = {:?}", runtime.variable("route"));
    println!("background = {:?}", runtime.scene().background);
    println!("bgm = {:?}", runtime.audio().bgm);
    println!("backlog lines = {}", runtime.dialogue().backlog.len());

    let mut saves = VnSaveStore::default();
    saves.save_runtime("quick", "inline", &runtime, VnPreferences::default());
    println!("save slots = {}", saves.slots().count());
    Ok(())
}
