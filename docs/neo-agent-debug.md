# Neo Agent Debug

`neo-debug` is a local, opt-in command-line probe for `ui-neo` runtimes. It starts only when the app sets `SKY_NEO_AGENT_DEBUG=1`.

## Start Stress Lab

```powershell
.\examples\ui\neo\run_stress_lab_debug.ps1
```

The app writes its endpoint to `target/neo-debug/stress_lab/endpoint.json`. The endpoint includes a random token and binds to `127.0.0.1`.

## Query

```powershell
cargo run --bin neo-debug --features ui-neo -- state
cargo run --bin neo-debug --features ui-neo -- snapshot --filter controls.preset.dropdown --max-rows 60
cargo run --bin neo-debug --features ui-neo -- diagnose-dropdown controls.preset.dropdown
cargo run --bin neo-debug --features ui-neo -- diagnose-element controls.preset.dropdown.field
cargo run --bin neo-debug --features ui-neo -- diagnose-overflow signals.chart.pie
cargo run --bin neo-debug --features ui-neo -- diagnose-input controls.preset.dropdown.field
cargo run --bin neo-debug --features ui-neo -- hit-test 380 160 --max-rows 20
```

## Interact

```powershell
cargo run --bin neo-debug --features ui-neo -- click controls.preset.dropdown.field
cargo run --bin neo-debug --features ui-neo -- click controls.preset.dropdown.item.2
cargo run --bin neo-debug --features ui-neo -- hover controls.preset.dropdown.field
cargo run --bin neo-debug --features ui-neo -- scroll controls.panel --y -240
```

## Screenshots

```powershell
cargo run --bin neo-debug --features ui-neo -- screenshot --path target/neo-debug/full.png
cargo run --bin neo-debug --features "ui-neo,asset" -- screenshot-element signals.chart.pie --path target/neo-debug/mix.png --padding 24
```

`screenshot-element` requests a normal app screenshot and returns the element crop rectangle. When the CLI is built with `asset`, it also crops the file in place, converting logical UI coordinates to physical screenshot pixels.

## Reading Diagnoses

- `first_issue=missing`: the element or popup was not built into the current tree.
- `first_issue=state`: supplied app state says the dropdown is closed.
- `first_issue=layer` or `layer_open`: the popover layer record is absent or closed.
- `first_issue=anchor`: the layer exists but did not resolve an anchor.
- `first_issue=draw`: the element exists but no matching draw command was produced.
- `first_issue=clip`: the element center is outside an active ancestor clip.
- `first_issue=layer_block`: another open, visible higher layer covers this element center.
- `first_issue=overflow`: one or more descendants extend outside their parent frame.
- `first_issue=covered`: another input-capable element is the top hit at this element's center.
- `first_issue=ok`: the runtime data path looks internally consistent.
