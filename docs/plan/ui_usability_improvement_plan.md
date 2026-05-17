# UI Usability Improvement Plan

## Purpose

SkyEngine's `ui-neo` API should let users write UI by describing structure:

```rust
ui.column("page")
    .padding(24.0)
    .gap(16.0)
    .content(|ui| {
        ui.text("title").text("Tasks").build();
        ui.row("toolbar").gap(12.0).content(|ui| {
            widgets::button(ui, "new").text("New").build();
            widgets::input(ui, "search").grow(1.0).build();
        });
    });
```

The current API can already express this style in simple cases, but common
application layouts still fall back to manual geometry:

- `x/y` offsets for normal content placement
- hard-coded `width - padding * 2.0` calculations
- guessed button and badge sizes
- duplicated section/header/body positioning rules
- fragile behavior when text grows through localization

The goal is not to hide the DSL behind large helper functions. The goal is to
make the direct DSL pleasant, predictable, and hard to misuse.

## Current Status

Phase 1 core primitives are implemented in `ui-neo`:

- `padding`, `padding_xy`, and `padding_each` define a parent content box.
- `min_width`, `max_width`, `min_height`, and `max_height` clamp layout size.
- `grow` distributes remaining main-axis space in rows and columns.
- Text `max_width` remains backward-compatible by also setting the text wrapping
  budget used by rendering.
- `examples/ui/neo_layout_primitives.rs` demonstrates the primitives without
  adding page-specific helper wrappers.

Still pending:

- extending the shared widget layout contract beyond button/input/badge where it
  clearly reduces manual sizing
- scroll ergonomics
- migration of one example section away from manual geometry
- visual validation across English and Chinese strings

Phase 2 has started:

- `widgets::button` accepts the shared sizing contract and defaults to natural
  width from text/icon content when width is not explicit.
- `widgets::input` accepts the shared sizing contract, including `grow`, so it
  can fill toolbar rows without manual width subtraction.
- Text `WrapContent` uses shaped natural text size instead of falling back to
  the full available viewport.
- `widgets::badge` provides a natural-width badge/chip primitive on top of the
  shared widget layout contract.
- Widget internals that use `Fill` are remeasured after parent growth, so
  backgrounds and hit targets follow the final grown frame.
- `neo_layout_primitives` now uses real `widgets::button` and `widgets::input`
  in its growing toolbar, and real `widgets::badge` for natural-width chips.

## Design Principles

- Prefer fewer, stronger primitives over many scenario-specific helpers.
- Keep direct UI authoring readable; avoid framework-within-framework layers.
- Use flow layout (`row` / `column`) as the default page-building path.
- Keep `stack` available for overlays, decoration, and intentional absolute
  positioning.
- Make size negotiation deterministic and documented.
- Make localizable text safe through sizing rules, wrapping, and clipping.
- Preserve `ui-neo` runtime, input, animation, and rendering semantics.

## Non-Goals

- Do not introduce a CSS engine.
- Do not implement complete Flexbox.
- Do not replace `ui-neo` with yakui, egui, or another authoring model.
- Do not add a large layout-helper layer before the core primitives are solid.
- Do not remove `x/y`; absolute positioning remains valid for overlays and
  intentionally precise composition.

## Current Pain Points

### Manual Interior Spacing

Containers do not have first-class padding. Callers often create an inner body
by manually offsetting children and subtracting widths.

This creates code like:

```rust
ui.stack("section")
    .size(width, height)
    .content(|ui| {
        ui.text("title").x(20.0).y(18.0).size(width - 40.0, 28.0).build();
        ui.stack("body").x(20.0).y(90.0).size(width - 40.0, height - 108.0);
    });
```

The layout system should own that content-box math.

### Weak Size Negotiation

`Size::Fixed`, `Size::WrapContent`, and `Size::Fill` are useful, but not enough
for modern app UI. Common layouts need:

- a minimum readable size
- a maximum size for wide screens
- a way to distribute extra space
- reliable natural sizes for text and widgets

### Overuse Of `stack`

`stack` is useful, but it is too easy to use as the main page layout tool. That
encourages absolute-positioned pages and makes changes cascade into manual
coordinate edits.

### Helper Risk

Large helpers like `section_frame` can reduce repeated code, but they can also
hide more layout rules from the user. A helper that still requires callers to
understand all the original geometry plus the helper's private rules is a net
loss.

The first fix should be better primitives, not more opaque wrappers.

## Target API Shape

### Mature Prior Art

The new primitives intentionally mirror proven layout concepts instead of
inventing a SkyEngine-only vocabulary.

HTML/CSS equivalents:

```css
padding: 24px;
padding: 16px 24px;
min-width: 320px;
max-width: 760px;
min-height: 120px;
max-height: 480px;
display: flex;
gap: 16px;
flex-grow: 1;
```

SkyEngine `ui-neo` equivalents:

```rust
ui.column("page")
    .padding(24.0)
    .gap(16.0)
    .max_width(760.0)
    .content(|ui| {
        ui.row("toolbar").gap(12.0).content(|ui| {
            ui.rect("fixed").size(120.0, 40.0).build();
            ui.rect("fill").size(160.0, 40.0).grow(1.0).build();
        });
    });
```

The intent is not to become a browser. The useful part to borrow is the mental
model:

- parent owns interior spacing through `padding`
- siblings own outside spacing through `margin`
- containers own sibling spacing through `gap`
- children can request leftover main-axis space through `grow`
- `min_*` and `max_*` protect readability and wide-screen composition

This keeps the concepts familiar to anyone who has seen HTML, Flutter, SwiftUI,
Jetpack Compose, Unity UI Toolkit, or other modern retained/declarative UI
systems, while still fitting the small EUI-NEO-style runtime.

### Core Builder Additions

Add these to `ElementBuilder`:

```rust
.padding(value)
.padding_xy(horizontal, vertical)
.padding_each(left, top, right, bottom)

.min_width(value)
.max_width(value)
.min_height(value)
.max_height(value)

.grow(value)
```

Keep the primary documented names short and literal. CamelCase compatibility
aliases may mirror the existing EUI-style surface, but examples should teach the
snake_case form first.

### Size Behavior

Document and implement size resolution with this order:

1. Measure intrinsic content size.
2. Resolve requested size:
   - `Fixed(value)` uses the fixed value.
   - `WrapContent` uses intrinsic content size.
   - `Fill` uses available content-box space.
3. Apply flex growth on the container's main axis.
4. Clamp with `min_*` and `max_*`.
5. Layout children inside the parent's content box after padding.

This order is important. If it is not explicit, `grow`, `Fill`, `WrapContent`,
and `min/max` will produce surprising results.

### Padding Semantics

`padding` belongs to the parent element's content box.

- Parent frame includes padding.
- Children are measured and laid out inside `frame - padding`.
- `gap` is applied between children inside the content box.
- `margin` remains outside the element and affects sibling layout.

### Grow Semantics

First milestone behavior should stay intentionally small:

- `grow` works only on the main axis of `row` and `column`.
- `grow(0.0)` is the default.
- Remaining positive space is distributed by grow weight.
- If a growing child hits `max_width` / `max_height`, leftover space is
  redistributed to remaining grow children that can still expand.
- Cross-axis stretch can remain explicit through `Fill`.

This is enough for toolbars, forms, and split rows without pretending to be full
Flexbox.

### Intrinsic Measurement

Leaf elements and common widgets should expose useful natural sizes:

- text: shaped text width and line height, bounded by max width when wrapping
- button: text/icon natural width plus padding, default height from style
- input: default readable width and height unless grow/fixed sizing overrides it
- badge/label controls: text width plus padding, clamped by min/max
- rect/image: explicit size, source size, or zero-content fallback

The exact widget defaults should live with the widget style, not in page code.

### API Quality Criteria

Each new layout API must pass these checks before it becomes the recommended
authoring path:

- It must describe one stable layout concept, not one screen's design.
- It must compose with `row`, `column`, and `stack` without hidden geometry.
- It must reduce manual `x/y` or `width - padding` arithmetic in examples.
- It must have unit tests for edge cases that are easy to misunderstand.
- It must not require every widget to copy/paste separate layout fields.
- It must preserve explicit sizing for game HUDs and precise overlays.

Current assessment:

- `padding` passes: it replaces repeated content-box subtraction.
- `min/max` passes: it maps directly to mature constraint concepts.
- `grow` passes for Phase 1: it intentionally covers only row/column main-axis
  expansion and redistributes space when max constraints are hit.
- Widget ergonomics are partially passing: button and input now share the core
  sizing contract, text has natural `WrapContent`, and badge has a reusable
  natural-width widget. Remaining widget work should be justified by concrete
  repeated sizing pain.

## Helper Policy

Helpers are allowed only when they are thin and predictable.

A good helper:

- expresses one common pattern
- composes existing primitives
- exposes spacing and sizing through normal builder concepts
- does not require private magic numbers
- remains optional

A poor helper:

- hides fixed header/body offsets
- invents a second layout vocabulary
- forces callers to learn undocumented size rules
- works only for one example page

Candidate helpers should wait until Phase 1 and Phase 2 are stable. The first
candidate should be `scroll_column`, because scrolling currently requires
viewport/content/offset wiring that users should not repeat by hand.

## Implementation Phases

### Phase 1: Core Layout Primitives

- Add `padding` to `Element`.
- Add `min/max` size fields to `Element`.
- Add a `grow` field to `Element`.
- Add builder methods for the new fields.
- Update `layout.rs` to use content boxes.
- Implement main-axis grow for row and column.
- Add layout tests for padding, min/max clamping, and row/column grow.

Success criteria:

- A parent can apply interior spacing without callers subtracting widths.
- A row can contain fixed-size controls plus one growing control.
- Min/max clamps are deterministic and covered by tests.

### Phase 2: Intrinsic Sizing

- Improve leaf measurement so `WrapContent` means natural content size.
- Add widget-level natural sizing for text, button, input, and badge-like
  controls. Button, input, text, and badge are implemented.
- Introduce a small shared widget layout contract instead of duplicating fields
  across every widget builder. Button, input, and badge are the first adopters.
- Let widget builders expose the same core sizing concepts where appropriate:
  `width`, `height`, `size`, `min_width`, `max_width`, `min_height`,
  `max_height`, and `grow`.
- Keep widget padding as widget style data when it is visual chrome, not a
  separate page-layout helper.
- Keep explicit `.size(...)` behavior working.
- Add tests for long text, compact buttons, and localized labels.

Success criteria:

- Common buttons no longer require guessed widths.
- Inputs can fill toolbar space through the same `grow` concept as core
  elements.
- Long labels either wrap, clamp, or clip according to documented rules.
- Existing explicit-size examples still compile and render predictably.
- One demo uses real `widgets::button` and `widgets::input` in a growing row
  without manual width subtraction.

### Phase 3: Scroll Ergonomics

- Add a thin `scroll_column` or equivalent builder once core sizing is stable.
- Keep scroll offset binding explicit.
- Hide repeated scrollbar/content-height bookkeeping from application code.
- Preserve current scroll hit testing and event capture behavior.

Success criteria:

- A vertically scrolling list can be declared without manual content-height
  arithmetic in page code.

### Phase 4: Example Migration

- Migrate one `ui-neo` example page from manual geometry to the new primitives.
- Prefer direct DSL calls over custom page helpers.
- Use the migration to identify missing primitive behavior, not to add
  example-specific wrappers.

Success criteria:

- The migrated page contains fewer manual `x/y` calls and fewer width
  subtractions.
- The code is easier to explain to a beginner than the original version.

### Phase 5: Localization And Visual Validation

- Validate English and Chinese strings in at least one example.
- Verify compact and wide viewport behavior.
- Use screenshots only after unit tests cover the layout math.

Success criteria:

- Buttons, badges, section titles, and form rows remain readable with translated
  text.
- Layout failures are caught by tests where possible, not only by screenshots.

## Validation Commands

Core checks:

```powershell
cargo test --features ui-neo ui::neo
cargo check --examples --features ui-neo
```

Compatibility checks:

```powershell
cargo test --features ui
cargo check --examples --features ui
cargo test --features yakui-ui
```

Visual checks when changing examples:

```powershell
$env:SKY_NEO_SCREENSHOT_PATH="target/neo_ui_layout_check.png"
$env:SKY_NEO_SCREENSHOT_FRAME="3"
$env:SKY_NEO_EXIT_AFTER_SCREENSHOT="1"
cargo run --example neo_control_center --features ui-neo
```

## Risks

- Adding helpers too early can make the API harder to learn.
- Implementing grow without documented conflict rules can create
  surprising layouts.
- Intrinsic measurement can become expensive if text shaping is repeated
  carelessly.
- Changing `WrapContent` can affect existing examples that accidentally relied
  on the old fallback behavior.
- Overfitting to desktop dashboards can make game HUD composition worse.

## Open Questions

1. Should `shrink` exist later, and what concrete layout should justify it?
2. Should padding apply to all element kinds or only containers?
3. Should text default to clipping, wrapping, or single-line measurement when no
   max width is provided?
4. What is the smallest natural width for input fields?
5. Should `scroll_column` live under `widgets::scroll` or as a core layout
   builder?

## First Milestone

The first milestone should be:

1. Add `padding`, `min/max`, and `grow` to the core element and layout model.
2. Write tests for the exact size-resolution order.
3. Improve button/text intrinsic sizing enough to remove guessed button widths
   from one example page.
4. Migrate one small page section using direct DSL calls only.

This milestone succeeds only if the resulting user code is shorter, more direct,
and easier to explain than the original manual-geometry version.
