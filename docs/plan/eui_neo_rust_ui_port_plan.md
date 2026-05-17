# EUI-NEO Rust UI Port Plan

This is the single tracking file for SkyEngine's EUI-NEO-style Rust UI port.
Do not split EUI-NEO state binding, widget parity, or gallery parity notes into
separate plan files; keep active gaps here and remove completed execution plans.

## Baseline

- Reference repository: `https://github.com/sudoevolve/EUI-NEO`
- Local reference clone: `C:\Coding\EUI-NEO`
- Observed commit: `ac090b15f39371083a979cbbcbad7c16b29a0eaa`
- License: Apache License 2.0

The port is semantic, not a byte-for-byte copy. SkyEngine keeps its winit,
wgpu, `UiHost`, app lifecycle, and asset seams, while porting EUI-NEO's DSL,
layout, event, animation, runtime, and component behavior.

## Porting Rules

- Check the local EUI-NEO source before changing a Rust API or behavior.
- Preserve source names as snake_case methods, plus camelCase aliases where
  Rust can express them cleanly.
- Keep widgets mechanically traceable to `components/*.h`.
- Treat EUI-NEO defaults, clamp rules, callback timing, z-index behavior, and
  interaction state as normative unless SkyEngine platform constraints require
  a documented seam.
- Keep page-specific work out of `examples/ui/eui_neo_gallery.rs`; reusable
  behavior belongs in `src/ui/neo/` or `src/ui/neo/widgets/`.
- Do not import EUI-NEO `3rd/` sources or bundled assets without separate
  license review.

Attribution text to keep in the module:

```rust
//! Experimental EUI-NEO-style UI backend for SkyEngine.
//!
//! This module is a Rust port of UI architecture and algorithms from
//! EUI-NEO (`https://github.com/sudoevolve/EUI-NEO`), observed at commit
//! `ac090b15f39371083a979cbbcbad7c16b29a0eaa`.
//! EUI-NEO is licensed under Apache License 2.0. This port adapts the DSL,
//! layout, event, animation, runtime, and component composition model to
//! SkyEngine's winit/wgpu/UI-host architecture. OpenGL/GLFW platform code,
//! vendored third-party sources, and bundled assets are not copied.
```

## Current Implementation Status

Implemented surface:

- `ui-neo` feature and `sky_engine::ui::neo` public module.
- Core DSL: `Ui`, element builders, stable IDs, page ID resolution, row,
  column, stack, rect, polygon, text, and image elements.
- Layout, event, animation, runtime, draw list, backend, and wgpu overlay
  renderer modules.
- `NeoUiPlugin`, `NeoUiBackend`, and `FrameContext::ui().neo(...)` integration.
- `NeoState<T>`, `Binding<T, V>`, and binding macros: `bind!`, `bind_clone!`,
  `bind_clamped!`, `bind_max!`, `bind_eq!`, and `bind_array!`.
- Binding helpers for checkbox, radio, switch, slider, input, segmented, tabs,
  dropdown, scroll, dialog, context menu, toast, date picker, time picker, and
  color picker.
- Component files for theme, panel, text, image, button, progress, slider,
  switch, checkbox, radio, scroll, input, segmented, tabs, dialog, dropdown,
  context menu, toast, date picker, time picker, color picker, data table, line
  chart, bar chart, and pie chart.
- Gallery and weird lab examples use `NeoState` / `Binding` rather than
  `FrameActions` / `apply_actions`.

This means the old state-binding implementation plan is complete and should
not exist as a separate active plan.

## Active Gap List

### Runtime And Layering

- Verify popup layering end to end for dropdown, dialog, context menu, toast,
  date picker, time picker, and color picker.
- Dialogs and modal overlays must draw above normal content, block underlying
  hit testing, and dismiss exactly like EUI-NEO.
- Hidden or disabled popup layers must not capture input unless EUI-NEO does.
- Outside-dismiss ordering must match source behavior: select/primary action,
  state update, dismiss callback, and redraw scheduling.
- Confirm `needsCompose()` equivalent behavior after callbacks mutate app state.
- Keep IME focus rect and keyboard capture synchronized with runtime focus.

### Input

- Treat input as the highest-risk widget.
- Verify focus, cursor movement, selection, deletion, paste filtering,
  multiline behavior if present in source, enter behavior, horizontal scroll,
  clipboard shortcuts, and IME rect behavior against `components/input.h`.
- Add focused tests that exercise behavior through `NeoRuntime`, not only
  through direct helper methods.

### Animation

- Verify `core/animation.h` parity for easing, transition defaults, delay,
  duration, animated property masks, and typed interpolation.
- Verify widget-level animated geometry and timing for hover, press, indicator,
  popup, picker, chart tooltip, and dependent visual-state paths.
- Confirm runtime animation dirty state requests redraws only while needed.
- Dirty-rect and cached UI-layer rendering remain deferred until correctness is
  proven.

### Renderer

- Local PNG/JPEG image rendering exists; verify cover, contain, stretch,
  vertical flip, tint, radius, opacity, transform, and clipping visually.
- HTTP image, Bing daily image, and SVG parity are deferred until explicitly
  mapped through SkyEngine asset/platform seams.
- Verify glyphon text measurement against EUI-NEO text sizing samples.
- Verify draw order and clipping for interleaved text/image/polygon/rect
  commands.
- Verify missing-font fallback so examples never render invisible text.

### Gallery

- Gallery should remain a parity pressure test, not a place to hide missing
  widget behavior.
- Compare gallery pages against the EUI-NEO component set whenever a widget
  changes.
- Remove duplicated page logic once the equivalent reusable widget behavior is
  implemented.
- Keep command-like actions explicit: toast trigger, dialog submit, context menu
  open at pointer position, feedback text, navigation, and app/game commands.

### Charts And Data Display

- Verify data table row/header heights, clipping, alignment, borders, striping,
  and empty rows/columns behavior.
- Verify line, bar, and pie chart normalization, palette fallback, label
  placement, geometry, hover tooltip behavior, and transition animation.

## Source Map

| EUI-NEO source | Rust target | Notes |
| --- | --- | --- |
| `core/dsl.h` | `element.rs`, `builder.rs`, `dsl.rs` | Element kinds, screen, drag event, shared builder surface, IDs, aliases. |
| `core/layout.h` | `layout.rs` | Align, layout type, size mode, edge insets, layout rect, measure/layout algorithm. |
| `core/event.h` | `event.rs`, `runtime.rs` | Cursor, pointer, keyboard, scroll, interaction state, press/click/drag semantics. |
| `core/animation.h` | `animation.rs` | Ease, transition, animated values, smoothed values, interpolation. |
| `core/dsl_runtime.h` | `runtime.rs`, `draw.rs`, `renderer.rs` | Compose, reconcile, focus, dispatch, timers, animation sync, draw generation. |
| `core/primitive.h` | `element.rs`, `draw.rs` | Data shapes only; do not copy OpenGL primitive rendering. |
| `core/text.*` | `draw.rs`, `renderer.rs` | DSL text style and measurement concepts through glyphon/wgpu. |
| `core/image.*` | `element.rs`, `draw.rs`, `renderer.rs` | Image source, fit, tint, radius, opacity through SkyEngine asset/texture seams. |
| `components/theme.h` | `widgets/theme.rs` | Tokens, defaults, helper functions. |
| `components/panel.h` | `widgets/panel.rs` | Panel style/default wrapper behavior. |
| `components/text.h` | `widgets/text.rs` | Text style/default wrapper behavior. |
| `components/image.h` | `widgets/image.rs` | Image wrapper behavior and source aliases. |
| `components/button.h` | `widgets/button.rs` | Button style, press scale, icon/text layout, context menu callback. |
| `components/progress.h` | `widgets/progress.rs` | Clamp rules, fill geometry, transition. |
| `components/slider.h` | `widgets/slider.rs` | Pointer-to-value math, drag bounds, change callback. |
| `components/scroll.h` | `widgets/scroll.rs` | Offset controller, wheel, thumb drag, viewport/content clamp. |
| `components/checkbox.h` | `widgets/checkbox.rs` | Controlled checked state and callback timing. |
| `components/radio.h` | `widgets/radio.rs` | Controlled selected state, dot geometry, select/change callback timing. |
| `components/switch.h` | `widgets/switch.rs` | Controlled checked state, track/thumb geometry, `toggleSwitch` alias. |
| `components/input.h` | `widgets/input.rs` | Stable-ID text state, focus, cursor, selection, clipboard, IME rect. |
| `components/segmented.h` | `widgets/segmented.rs` | Controlled selected index and per-item click composition. |
| `components/tabs.h` | `widgets/tabs.rs` | Controlled selected index, indicator, per-item text color. |
| `components/dropdown.h` | `widgets/dropdown.rs` | Popup placement, outside dismiss, selected clamp, z-index. |
| `components/dialog.h` | `widgets/dialog.rs` | Modal backdrop, primary/secondary callbacks, close behavior, z-index. |
| `components/contextmenu.h` | `widgets/context_menu.rs` | Screen-edge clamp, outside dismiss, item sizing, select+dismiss ordering. |
| `components/toast.h` | `widgets/toast.rs` | Auto-dismiss timing, close button, position, icon/text sizing. |
| `components/datepicker.h` | `widgets/date_picker.rs` | Dialog picker, draft state, wheel drag/scroll, Done-only commit. |
| `components/timepicker.h` | `widgets/time_picker.rs` | Dialog picker, AM/PM wheel, minute step, Done-only commit. |
| `components/colorpicker.h` | `widgets/color_picker.rs` | Draft color state, sliders, swatches, hex preview. |
| `components/datatable.h` | `widgets/data_table.rs` | Header/body geometry, clipping, striping, table style. |
| `components/linechart.h` | `widgets/line_chart.rs` | Grid, point/line geometry, labels, hover tooltip. |
| `components/barchart.h` | `widgets/bar_chart.rs` | Grid, bar geometry, labels, hover tooltip. |
| `components/piechart.h` | `widgets/pie_chart.rs` | Sector geometry, palette, labels, hover tooltip. |

## Widget Parity Matrix

| Widget | Current risk | Remaining checks |
| --- | --- | --- |
| Theme | Low | Token, opacity, shadow, border, and default color comparison. |
| Panel | Low | Gradient, border, shadow, radius, opacity field mapping. |
| Text | Medium | Glyphon measurement and wrapping against EUI-NEO samples. |
| Image | Medium | Fit modes, clipping, transform, fallback, HTTP/Bing/SVG deferred behavior. |
| Button | Medium | Press scale, disabled behavior, icon gap/width math, context-menu rect. |
| Checkbox | Low | Checkmark geometry, hover/pressed colors, click-only callback. |
| Radio | Low | Dot animation and `onSelect` / `onChange(true)` timing. |
| Switch | Low | Track sizing clamp, thumb travel, callback timing. |
| Progress | Low | Fill width, radius, transition, clamp. |
| Slider | Medium | Pointer-to-value math, drag capture, track/thumb sizing. |
| Scroll | Medium | Thumb sizing, wheel step, viewport/content clamp, z-index. |
| Segmented | Low | Indicator sizing, text color, redundant callback behavior. |
| Tabs | Low | Indicator animation, text colors, redundant callback behavior. |
| Dropdown | High | Popup placement, dismiss, selected clamp, `onOpenChange`, z-index. |
| Dialog | High | Modal top layer, backdrop click, hidden-state hit testing, callbacks. |
| Context menu | High | Screen clamp, outside dismiss, item sizing, select+dismiss ordering. |
| Toast | Medium | Auto-dismiss, close button, position, icon/text sizing. |
| Input | Highest | Focus, cursor, selection, clipboard, enter, scroll, IME. |
| Date picker | High | Draft state, wheel drag/scroll, leap-year, cancel, backdrop behavior. |
| Time picker | High | Hour/minute wrap, minute step, wheel drag/scroll, cancel. |
| Color picker | High | Palette defaults, draft commit, sliders, hex/readout formatting. |
| Data table | Medium | Row heights, clipping, alignment, borders, empty states. |
| Line chart | Medium | Normalization, grid/axis labels, point geometry, hover tooltip. |
| Bar chart | Medium | Palette fallback, bar width/gap, labels, hover tooltip. |
| Pie chart | Medium | Slice normalization, label placement, hover tooltip. |

## Remaining Implementation Order

1. Fix modal and popup layering as one runtime-level pass.
2. Lock down input behavior with source-level tests.
3. Verify animation timing and dirty/redraw behavior.
4. Verify renderer text/image fallback and gallery visual output.
5. Finish chart/data-table behavior tests.
6. Only after parity is stable, consider dirty-rect or cached-layer rendering.

## Validation Commands

Core commands:

```powershell
cargo test --features ui-neo
cargo test --features ui-neo ui::neo
cargo check --examples --features ui-neo
```

Compatibility commands:

```powershell
cargo test --features ui
cargo check --examples --features ui
cargo test --features yakui-ui
cargo check --examples --features yakui-ui
cargo test --features app
cargo check --examples --features app
```

Manual UI checks:

```powershell
cargo run --example eui_neo_demo --features ui-neo
cargo run --example eui_neo_gallery --features ui-neo
cargo run --example weird_neo_lab --features ui-neo
```

## Non-Goals

- Do not redesign a new UI architecture from scratch.
- Do not merge neo, legacy UI, yakui, and egui internally.
- Do not copy EUI-NEO's OpenGL renderer, GLFW platform layer, `3rd/`
  dependencies, or bundled assets blindly.
- Do not introduce a CSS engine.
- Do not add render-graph UI integration before overlay correctness is stable.
- Do not optimize dirty-rect rendering before behavior parity is proven.

## Success Criteria

- Users can write EUI-style declarative UI in Rust through `ui-neo`.
- Input capture, hover, press, click, drag, focus, text input, scroll, and modal
  routing work through `UiHost`.
- Gallery demonstrates the full component surface without bespoke workarounds.
- Rendering uses SkyEngine's wgpu overlay path and reliably shows text/images.
- Legacy UI, yakui UI, and egui still compile in their existing roles.
- Attribution and Apache-2.0 obligations remain documented.
