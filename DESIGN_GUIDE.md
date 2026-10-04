# `late.sh` TUI Design Guide

This guide establishes the visual, layout, and interaction standards for `late.sh`. It serves as the single source of truth for human developers and AI coding agents implementing screens, dialogs, forms, and games within the terminal.

---

## 1. Core Design Philosophy

1. **Information Density with Breathing Room**: Keep interfaces compact and functional without letting elements run together. Density does not mean cramped; spacious areas receive deliberate padding, while constrained viewports gracefully shed secondary chrome.
2. **Predictable Grid & Columnar Alignment**: Align controls, labels, and statistics along uniform column gutters. Outlier-long elements should sacrifice columnar lockstep rather than blow out neighboring columns.
3. **Contrast Over Hierarchy**: Terminal emulators lack subpixel font weights, letter spacing, and CSS drop-shadows. Visual hierarchy must be created through distinct foreground/background tint steps, character glyphs, bold accents, and deliberate contrast pairings.
4. **Keyboard-First, Mouse-Accessible**: Every action must be immediately bindable to a clear key mnemonic or chord. Mouse hitboxes (`Cell<Rect>`) complement keyboard workflows without replacing them.
5. **Universal Portability**: Layouts must degrade cleanly from ultra-wide 500×200 desktop monitors down to a minimum 44×22 phone screen (Termux).

---

## 2. Terminal Dimensions & Responsive Degradation

| Tier | Geometry | Target Environment | Layout & Degradation Rules |
| :--- | :--- | :--- | :--- |
| **Extreme Floor** | **44 × 22** | Termux on mobile phones | • Top frame titles drop right-hand chips and pot meters.<br>• Rails (Room list, Right sidebar) collapse to zero width; main chat or canvas fills 100% width.<br>• Popups and dialogs clamp to viewport bounds or trigger `draw_too_small`.<br>• Footers drop unread counts, radio station tags, and secondary action hints (`row_with_hint`). |
| **Compact / Laptop** | **96 × 34** | Standard split terminal / laptop window | • Standard modal viewport (`MODAL_WIDTH = 96`, `MODAL_HEIGHT = 34`).<br>• Room list rail (`AUTO_ROOM_LIST_MIN_COLS = 96`) becomes visible.<br>• Right sidebar (`AUTO_RIGHT_SIDEBAR_MIN_COLS = 72`) is visible.<br>• Up to 2-pane side-by-side data grids. |
| **Median Target** | **180 × 60** | Full-screen desktop terminal | • Canonical development target. All chrome, sidebars, and three-pane views visible.<br>• Sidebars expand to full component stacks (presence, radio booth, mini-calendar, bonsai).<br>• Ample 2-column padding between structural panels. |
| **Expansive / Ultrawide** | **500 × 200** | Multi-monitor / tiled ultrawide | • Artboard gallery displays full uncropped canvases.<br>• Text columns and dialogs clamp to maximum readable widths (`MAX_WIDTH = 110`) using `centered_rect` to prevent ultra-long unreadable lines of text. |

### Minimum Dimension Gates (`draw_too_small`)
When a screen or interactive canvas has a hard geometric requirement, guard it using `primitives::draw_too_small`. Never show a generic "terminal too small" message—always report what needs space, the required dimensions, and the current size:

```rust
use crate::app::common::primitives::draw_too_small;

if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
    draw_too_small(frame, area, "Pool Table", MIN_WIDTH, MIN_HEIGHT);
    return;
}
```
*Standard Thresholds*: Pool Table (`112×30`), Traffic (`70×20`), Games Hub (`60×6`), Arcade Lobby (`50×10`), Leaderboards (`48×8`), Rubik's Cube (`42×18`), Zen Engine (`40×12`), Green Dragon (`30×10`).

---

## 3. Grid, Layout & Spacing

### 3.1 Padding & Margin Conventions
- **Spacious Views (Desktop / Median Viewports)**:
  - Separate elements horizontally by **2 columns** (`Constraint::Length(2)`).
  - Pair horizontal 2-column margins with a **1-row vertical breathing margin** (`Constraint::Length(1)`).
- **Constrained / Dense Views (Modals, Compact Terminals)**:
  - Reduce horizontal padding to **1 column** (`Constraint::Length(1)`).
  - Reduce vertical margin to **0 or 1 row** (usually 1 row between sections, 0 between rows in a list).
- **Outer Shell Padding**:
  - Right sidebar leaves a 1-column gap past its left divider rule: `x: area.x + 2, width: area.width.saturating_sub(2)`.
  - Chat composer applies a 1-column horizontal inset inside its top/bottom borders: `horizontal_inset(inner, 1)`.

### 3.2 Columnar Alignment
- **Form Labels**: Standard label width is **16 columns** (`format!("{label:<16}")`).
  - Longer descriptions (such as Tweaks or Feeds) expand to **28 or 32 columns** (`format!("{label:<32}")`).
  - Compact facts (Profile fetch keys, runner stats) use **8 to 10 columns** (`format!("{label:<10}")`).
- **Outlier Handling**: If an outlier label exceeds the column allocation (e.g. label length ≥ 16 in a 16-col row), do not wrap or blow out the table width. Append a single space and let the value flow:
  ```rust
  let label_text = if label.chars().count() >= 16 {
      format!("{label} ")
  } else {
      format!("{label:<16}")
  };
  ```

### 3.3 Box Drawing Borders & Inline Titles
- **Title Padding**: Block titles must always be padded with leading and trailing spaces:
  ```rust
  // CORRECT:
  Block::default().title(" Settings ")
  Block::default().title(format!(" {} ({hint}) ", overlay.title))

  // AVOID (looks jammed against border corners):
  Block::default().title("Settings")
  ```
- **Border Hierarchy**:
  - `BORDER_ACTIVE()`: Active focused windows, focused inputs, app outer frame, statusline continuation rules.
  - `BORDER()`: Inactive containers, resting composers, table outlines.
  - `BORDER_DIM()`: Quiet internal panel dividers (rail rules, sidebar dividers, table sub-rules).
  - `ERROR()`: Destructive modal frames (e.g. Delete Account dialog) or alert popovers.

---

## 4. Typography, Text Hierarchy & Unicode Glyphs

### 4.1 Text Tiering (No Subpixels)
Because terminals cannot render distinct font weights or sizes, visual hierarchy relies on luminance tiers from `late-ssh/src/app/common/theme.rs`:

| Tier | Semantic Token | Typical Usage |
| :--- | :--- | :--- |
| **Header / Emphasis** | `TEXT_BRIGHT()` + `Modifier::BOLD` | Window titles, active values, focused selections, table totals. |
| **Brand Accent** | `AMBER_GLOW()` + `Modifier::BOLD` | Active modal titles, highlighted cursors (`›`), unread mentions. |
| **Primary Body** | `TEXT()` | Standard body text, dialog labels, profile values, chat body. |
| **Secondary Metadata** | `TEXT_DIM()` | Unselected options, timestamps, keyhint action labels, table headers. |
| **Quiet / Dividers** | `TEXT_FAINT()` | Empty placeholders, inactive hints, table rule dots, dot separators (`·`). |

### 4.2 Standard Unicode Glyph Glossary

| Category | Glyph(s) | Semantic Meaning | Recommended Code Styling |
| :--- | :--- | :--- | :--- |
| **Selection / Cursor** | `›` (`\u{203a}`) | Active row indicator in lists & forms | `AMBER_GLOW().bold()` on `selection_style()` |
| **Binary Toggle** | `● on` (`\u{25cf}`)<br>`○ off` (`\u{25cb}`) | Enabled switch<br>Disabled switch | `SUCCESS().bold()`<br>`TEXT_FAINT()` |
| **Tri-State Option** | `◐ auto` (`\u{25d0}`) | Automatic / Hybrid / Inferred mode | `AMBER()` or `SUCCESS().bold()` |
| **Disclosure** | `▸` (`\u{25b8}`)<br>`▾` (`\u{25be}`) | Collapsed node/folder<br>Expanded node/folder | `AMBER()`<br>`AMBER_GLOW()` |
| **Cycle Choosers** | `◂ Value ▸` | Horizontal cycle selector | `AMBER().bold()` for arrows |
| **Action Hints** | `⏎` (`\u{23ce}`) or `↵`<br>`·` (`\u{00b7}`) | Opens editor / sub-modal<br>Item / shortcut separator | `TEXT_DIM()`<br>`TEXT_FAINT()` |
| **Truncation / Elision** | `…` (`\u{2026}`) | Content is currently omitted; see §4.3 | Inherit the visible text's style |
| **Status Banners** | `✓` (`\u{2713}`)<br>`✗` (`\u{2717}`)<br>`•` (`\u{2022}`) | Success confirmation<br>Error / validation failure<br>Neutral info / alert | `SUCCESS()`<br>`ERROR()`<br>`AMBER()` |
| **Markdown Headers**| `▍` (`\u{2584}`)<br>`▎` (`\u{258e}`)<br>`▏` (`\u{258f}`) | H1 Header marker<br>H2 Header marker<br>H3 Header marker | `AMBER_GLOW().bold()`<br>`AMBER().bold()`<br>`AMBER_DIM().bold()` |
| **Text Caret** | `█` (`\u{2588}`) | Text input block cursor | `AMBER()` |
| **Tree Branches** | `├─` / `└─` | Hierarchy list branches | `TEXT_FAINT()` (brightens on focus) |

### 4.3 Ellipsis Usage

Reserve the Unicode ellipsis character (`…`, U+2026) almost always for indicating
truncation or elision. When used for that purpose, show it only while the value is
actually truncated or content is currently elided. Omit it when the value is
displayed in full or elision is no longer in effect, including after resizing or
changing the value.

Avoid using `…` merely to signal that a control opens a picker or dialog. For
example, a cycle selector whose value fits should read `◂ Server ▸`, without an
ellipsis after `Server`.

---

## 5. Color, Theming & Contrast Matrix

`late.sh` includes 105 built-in themes. All UI rendering must reference semantic accessors (`theme::*()`), never hardcoded RGB literals.

### 5.1 Palette Tokens Overview
Every theme defines **27 semantic color tokens** in `struct Palette`:
- **Canvas & Surface**: `bg_canvas`, `bg_selection`, `bg_highlight`
- **Borders**: `border_dim`, `border`, `border_active`
- **Text Tiers**: `text_faint`, `text_dim`, `text_muted`, `text`, `text_bright`
- **Brand Accents**: `amber`, `amber_dim`, `amber_glow`
- **Chat & Social**: `chat_body`, `chat_author`, `mention`
- **Semantic State**: `success`, `error`, `bot`
- **Domain Specific**: `bonsai_sprout`, `bonsai_leaf`, `bonsai_canopy`, `bonsai_bloom`, `badge_bronze`, `badge_silver`, `badge_gold`

### 5.2 Key Archetype Contrast Matrix
WCAG contrast ratios against `bg_canvas` across the primary theme families:

| Archetype | Theme ID | Background | `text` Ratio | `border_active` | `amber` Ratio | Contrast Notes |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Core Brand** | `late` | `#000000` (Pure Black) | **8.08:1** (`#af9e8a`) | 4.55:1 (`#a0692a`) | 5.76:1 (`#b8782c`) | Warm sepia/amber tone; high canvas contrast. |
| **High Contrast** | `contrast` | `#0c0e0c` (Near Black) | **15.98:1** (`#e2eaf5`) | 10.73:1 (`#7ac9ff`) | 12.27:1 (`#ffc45c`) | System default; maximized readability. |
| **Catppuccin** | `mocha` | `#1e1e2e` (Dark Blue) | **11.34:1** (`#cdd6f4`) | 8.07:1 (`#cba6f7`) | 9.27:1 (`#fab387`) | Soft pastel accents; strong selection contrast. |
| **Kanagawa** | `kanagawa`| `#1f1f28` (Charcoal) | **9.84:1** (`#d2c9a6`) | 5.44:1 (`#7e96bd`) | 8.15:1 (`#ffa066`) | Muted Japanese autumnal ink tones. |
| **Gruvbox** | `gruvboxdark` | `#282828` (Dark Gray) | **10.75:1** (`#ebdbb2`) | 3.81:1 (`#d65d0e`) | 5.94:1 (`#d79921`) | Earthy retro palette. |
| **Light Theme**| `latte` | `#eff1f5` (Light Paper)| **7.06:1** (`#4c4f69`) | 4.79:1 (`#8839ef`) | Inverted | Inverted luminance: dark ink on light paper. |

### 5.3 Selection Highlighting & The Terminal Transparency Rule
Selection highlights are created via `theme::selection_style()` and patched into row styles:

```rust
// Apply selection style across a row:
let row_style = Style::default().patch(theme::selection_style());
```

**Transparent / Default Terminal Canvas (`Color::Reset`)**:
When the user runs the `terminal` theme (`bg_canvas == Color::Reset`), terminal transparency is active. A fixed background fill cannot guarantee contrast against unknown terminal backgrounds.
`selection_style()` resolves this by returning:
```rust
Style::default()
    .fg(Color::Reset)
    .bg(Color::Reset)
    .add_modifier(Modifier::REVERSED)
```
- **Rule**: If a color carries semantic meaning inside a selected row (e.g. suit red, score colors), apply it **after** patching `selection_style()`.
- **Rule**: Never patch-then-overwrite with `Modifier::REVERSED` because `REVERSED` is an additive bitflag. Treatments competing on the same cell must resolve to exactly one branch.

### 5.4 The Punch-Through Cutout Technique
When rendering solid inverted glyph cutouts (Wordle tiles, Solitaire suits, Connect 4 discs) that must remain legible on both dark, light, and transparent terminals, use `theme::punch_through(fill)`:

```rust
pub fn punch_through(fill: Color) -> Style {
    Style::default().fg(fill).add_modifier(Modifier::REVERSED)
}
```
The terminal emulator paints the cell background with `fill` and shows the default background through the character cutout.

---

## 6. Canonical Ratatui Code Recipes

### 6.1 Modal & Dialog Framing
Every modal follows this exact structure:

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    widgets::{Block, Borders, Clear},
};
use crate::app::common::theme;

pub(crate) const MODAL_WIDTH: u16 = 96;
pub(crate) const MODAL_HEIGHT: u16 = 34;

pub(crate) fn draw(frame: &mut Frame, area: Rect, state: &MyState) {
    let popup = centered_rect(MODAL_WIDTH, MODAL_HEIGHT, area);

    // 1. MUST Clear underlying terminal content:
    frame.render_widget(Clear, popup);

    // 2. Outer block with active border and padded amber title:
    let block = Block::default()
        .title(" My Modal Title ")
        .title_style(
            Style::default()
                .fg(theme::AMBER_GLOW())
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme::BORDER_ACTIVE()));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    // 3. Vertical layout: breathing room -> body -> footer
    let chunks = Layout::vertical([
        Constraint::Length(1), // top breathing room
        Constraint::Min(4),    // scrollable / interactive body
        Constraint::Length(1), // action footer
    ])
    .split(inner);

    draw_body(frame, chunks[1], state);
    draw_footer(frame, chunks[2], state);
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([Constraint::Length(height)])
        .flex(Flex::Center)
        .split(area);
    let horizontal = Layout::horizontal([Constraint::Length(width)])
        .flex(Flex::Center)
        .split(vertical[0]);
    horizontal[0]
}
```

---

### 6.2 Form Rows & Control Clusters (`row_line`)
The canonical row control for forms, settings, and tweak panels:

```rust
use ratatui::{
    style::{Modifier, Style},
    text::{Line, Span},
};
use crate::app::common::theme;

pub struct ValueSpan {
    pub text: String,
    pub style: Style,
}

pub fn toggle_span(enabled: bool) -> ValueSpan {
    if enabled {
        ValueSpan {
            text: "● on".to_string(),
            style: Style::default()
                .fg(theme::SUCCESS())
                .add_modifier(Modifier::BOLD),
        }
    } else {
        ValueSpan {
            text: "○ off".to_string(),
            style: Style::default().fg(theme::TEXT_FAINT()),
        }
    }
}

pub fn row_line(
    selected: bool,
    width: usize,
    label: &str,
    value: ValueSpan,
) -> Line<'static> {
    let marker = if selected { "›" } else { " " };
    let prefix_style = if selected {
        Style::default()
            .fg(theme::AMBER_GLOW())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_FAINT())
    };
    let label_style = if selected {
        Style::default()
            .fg(theme::TEXT_BRIGHT())
            .patch(theme::selection_style())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    let value_style = if selected {
        value.style.patch(theme::selection_style())
    } else {
        value.style
    };

    let prefix = format!(" {marker} ");
    let label_text = if label.chars().count() >= 16 {
        format!("{label} ")
    } else {
        format!("{label:<16}")
    };

    let used = prefix.chars().count() + label_text.chars().count() + value.text.chars().count();
    let padding = width.saturating_sub(used.min(width));
    let trailing = " ".repeat(padding);
    let trailing_style = if selected {
        Style::default().patch(theme::selection_style())
    } else {
        Style::default()
    };

    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(label_text, label_style),
        Span::styled(value.text, value_style),
        Span::styled(trailing, trailing_style),
    ])
}
```

---

### 6.3 Action Hint Bars & Footers

#### Canonical Multi-Action Footer (`hint_line`)
```rust
use crate::app::common::primitives::hint_line;

// Renders: " ↑↓ move · Space pick · Esc done"
let footer = hint_line(&[
    ("↑↓", "move"),
    ("Space", "pick"),
    ("Esc", "done"),
]);
frame.render_widget(Paragraph::new(footer), footer_area);
```

#### Responsive Header Row with Right-Flushed Hint (`row_with_hint`)
When horizontal space is tight, secondary action hints on the right are gracefully dropped rather than wrapping:

```rust
use crate::app::common::primitives::row_with_hint;

let left = vec![
    Span::styled("Section Title", Style::default().fg(theme::TEXT_BRIGHT()).add_modifier(Modifier::BOLD)),
];
let right = vec![
    Span::styled("Enter", Style::default().fg(theme::AMBER_DIM()).add_modifier(Modifier::BOLD)),
    Span::styled(" edit", Style::default().fg(theme::TEXT_DIM())),
];
let line = row_with_hint(left, right, area.width as usize);
frame.render_widget(Paragraph::new(line), header_area);
```

---

### 6.4 Tab Bars & Header Rails
Active tabs are styled as bold inverted amber pills, with hitboxes stored in `Cell<Rect>` for mouse clicks:

```rust
for tab in visible_tabs {
    let active = tab == current_tab;
    let style = if active {
        Style::default()
            .fg(theme::BG_SELECTION())
            .bg(theme::AMBER())
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme::TEXT_DIM())
    };
    spans.push(Span::styled(format!(" {} ", tab.label()), style));
    spans.push(Span::raw(" "));
}
```

---

### 6.5 Tables & Data Lists
- **Left-aligned text**: Names, titles, and ranks are left-aligned.
- **Right-aligned numbers**: All numeric scores, kills, wins, and chip counts are grouped with commas via `primitives::thousands(i64)` and right-aligned with space padding:

```rust
use crate::app::common::primitives::thousands;

let rank = format!("  #{:<3}", entry.rank);
let name = format!(" {}", entry.username);
let value = format!("{} chips", thousands(entry.chips));

let pad = width.saturating_sub(rank.len() + name.len() + value.len());
let row = Line::from(vec![
    Span::styled(rank, rank_style),
    Span::styled(name, name_style),
    Span::raw(" ".repeat(pad.max(2))),
    Span::styled(value, Style::default().fg(theme::TEXT_BRIGHT())),
]);
```

---

### 6.6 Text Input Fields & Carets (`text_with_caret`)
When rendering custom inline text inputs without full textareas:

```rust
fn text_with_caret(text: &str, cursor_col: usize) -> String {
    let mut chars: Vec<char> = text.chars().collect();
    chars.insert(cursor_col.min(chars.len()), '█');
    chars.into_iter().collect()
}

// In the input renderer:
let content = if typed.is_empty() {
    Span::styled("█", Style::default().fg(theme::AMBER()))
} else {
    Span::styled(text_with_caret(&typed, cursor_x), Style::default().fg(theme::TEXT_BRIGHT()))
};
```

---

### 6.7 Toast Notifications & Alert Banners (`draw_banner`)
Toast banners auto-expire after 5 seconds and display standard status glyphs:

```rust
use crate::app::common::primitives::{Banner, draw_banner};

let banner = Banner::success("Account settings updated");
// Or: Banner::error("Connection lost");
// Or: Banner::info("Match drawn");

draw_banner(frame, toast_area, &banner);
```

---

## 7. Two-Pane Split Layouts (Rail + Detail)

For split views (Leaderboard, Artboard Gallery, Games Hub, Jobs Shelf):

```text
┌─────────────────────────────────────────────────────────────┐
│ Rail (20-25 cols)      │ Detail Pane (Min 0 / Fill 1)      │
│                        │                                    │
│   ── Category ──       │   Title Heading                    │
│  > Selected Item       │   Detailed description or table    │
│    Unselected Item     │   ...                              │
│                        │                                    │
└────────────────────────┴────────────────────────────────────┘
```

1. **Divider Ownership**:
   - The left rail owns `Borders::RIGHT` with `theme::BORDER_DIM()`.
   - The detail pane does not draw an additional left border.
2. **Rail Width**:
   - Fixed width between **20 and 25 columns** (`Constraint::Length(25)`).
3. **Breathing Row Offset**:
   - The rail rule touches row 0, while the detail pane starts 1 row lower to allow visual breathing room:
     ```rust
     fn below_breathing_row(area: Rect) -> Rect {
         Rect { y: area.y + 1, height: area.height.saturating_sub(1), ..area }
     }
     ```
4. **Responsive Collapse**:
   - When total terminal width < 72 columns, collapse the two-pane layout into a single pane with `h`/`l` or `Enter`/`Esc` drill-down.

---

## 8. Anti-Patterns & Deprecated Conventions

To maintain consistency across `late.sh`, AI coding agents and human developers must avoid the following legacy patterns:

1. **DO NOT Hardcode Raw RGB Colors in Widgets**:
   - *Bad*: `Color::Rgb(255, 176, 0)`.
   - *Good*: Always use semantic accessors: `theme::AMBER()`, `theme::BORDER_ACTIVE()`, `theme::TEXT_DIM()`.
2. **DO NOT Omit `Clear` Before Rendering Overlays**:
   - Popups and modals must call `frame.render_widget(Clear, popup)` before drawing blocks; otherwise, text from the background screen bleeds through.
3. **DO NOT Create Unpadded Box Titles**:
   - *Bad*: `.title("Settings")`.
   - *Good*: `.title(" Settings ")`.
4. **DO NOT Use ASCII Checkboxes in Settings Rows**:
   - *Bad*: `"[x]"` / `"[ ]"`.
   - *Good*: Use unicode state glyphs: `● on` (success + bold) and `○ off` (text faint).
5. **DO NOT Wrap Action Hints in Tight Containers**:
   - When horizontal space is tight, secondary hints must be dropped from the right via `row_with_hint`, never wrapped to create ragged row heights.
6. **DO NOT Resolve Theme Accessors Inside Background Threads**:
   - Tokio `tick` threads do not hold the active reader's session theme in thread-local storage. Decouple layout tokens via an intermediate ink enum (e.g. `PaperInk`, `OverlayInk`), and resolve to Ratatui styles only during `draw()`.
7. **DO NOT Use Generic "Too Small" Error Messages**:
   - Always call `draw_too_small(frame, area, what, min_w, min_h)` so users know the required terminal dimensions.

---

## 9. Ideas for the Future

### 9.1 Codebase Harmonization & Primitive Deduplication
- **Unify `centered_rect`**: Consolidate the ~15 disparate `centered_rect` implementations into `late-ssh/src/app/common/primitives.rs` with a single signature `(width: u16, height: u16, area: Rect) -> Rect` using Ratatui's `Layout::flex(Flex::Center)`.
- **Extract a Canonical `RowControl` / `RowLine` Widget**: Promote the `row_line` pattern from `settings_modal/ui.rs` into a shared reusable primitive in `app/common/` so any modal, dialog, or sheet automatically gains standard 16-character columnar alignment, `›` cursors, and full-width `theme::selection_style()` padding.
- **Normalize Sub-Dialog Glyphs**: Migrate legacy sub-dialogs (`chat_badges`, `sidebar_components`) from ASCII `[x]` / `[ ]` and `>` to the standard `● on` / `○ off` and `›`.

### 9.2 Living "Component Gallery / Design System" Screen
- **Interactive `/design` Screen or Dev Modal**: Introduce a staff/dev command (e.g., `/design` or `/components` in the composer) rendering all design system primitives side-by-side:
  - Form rows (toggles, 3-state, sliders, cycle pickers)
  - Action bars (`hint_line`, `row_with_hint`)
  - Toast banners (`✓`, `✗`, `•`)
  - Code blocks, blockquotes, and Markdown headers
- **Theme Testing Sandbox**: Allows contributors to test new palettes and contrast levels on every UI control simultaneously without navigating through multiple application areas.

### 9.3 Automated Visual Regression Testing (via `tmux-tui-test`)
- **Automated Viewport Sweeps**: Build a scripted test harness using `tmux_tui_harness.py` to capture and verify screens at **44×22**, **96×34**, and **180×60**.
- **Raster Snapshot Validation (`freeze`)**: Render PNG snapshots across primary theme archetypes (`contrast`, `late`, `mocha`, `kanagawa`, `gruvboxdark`) to catch visual clipping, ragged wrapping, or low-contrast traps before PR merges.

### 9.4 Agent Tooling & Prompt Integration
- **Cross-Reference in `CONTEXT.md`**: Add explicit guidance in `CONTEXT.md` pointing LLM coding agents to `DESIGN_GUIDE.md` whenever new UI features, dialogs, or settings rows are requested.

### 9.5 Animation, Frame Rates & Terminal Performance Guidelines
- **Animation Standards**: Establish guidelines for active visual elements (Bonsai wind sway, snake food pulse, visualizers, marquee tickers):
  - **Frame Rate Budgets**: Cap animation ticks at 15–20 fps to conserve SSH bandwidth over slow or mobile connections.
  - **Double-Buffer Diffing**: Rely strictly on Ratatui's terminal cell diffing rather than emitting manual escape codes or full-screen clears to prevent visual flicker.
  - **Idle CPU Sleep**: Ensure animations sleep or drop tick rates when their containing pane or screen is unfocused or minimized.
