# Appearance and settings verification

Verified on 2026-09-18 in the development worktree, following the supplied
light and dark macOS System Settings references and read-only browsing of the
actual Appearance, General, and Menu Bar pages. No system preferences were
changed. This is feature verification,
not the full release acceptance matrix.

## Automated checks

- `pnpm type-check`, `pnpm lint`, `pnpm build`, and `pnpm security-check`: passed.
- `pnpm test`: 70 tests passed across 18 files.
- Rust format/check passed; Rust tests: 166 passed, 1 ignored performance test.
- A debug `.app` bundle built with a separate QA application identifier.

Coverage includes existing databases gaining the default theme without
overwriting other preferences; persistence and reopen for all three theme
values; invalid-value handling; native theme mapping; live system media-query
changes and listener cleanup; optimistic update, failure rollback, and
authoritative reconciliation; the four settings categories; draft preservation;
and confirmation-dialog focus, keyboard navigation, and inert background.
Storage-help tests cover hidden initial content, click toggling, outside clicks,
Escape, and category changes.

## Observed desktop checks

The QA app used `COPY_STACK_QA_DATA_DIR` with a private temporary database.
The installed application was not replaced. No history deletion was performed
during these appearance checks.

- At 600 × 700, General, Appearance, Clipboard, and Menu Bar had distinct
  configuration content and a persistent back-to-History control.
- The light settings page used a gray sidebar, white main area, pale grouped
  surfaces, colored category icons, and a blue selected category.
- The dark settings page used warm neutral gray surfaces and inset separators.
  Both themes used blue selection while focused and gray selection when
  unfocused, matching the observed native window states.
- Light and Dark immediately changed the settings page and native title bar.
  Returning to History preserved the chosen appearance.
- The final theme picker showed Light/Dark/System in native order, with blue
  desktop and overlapping-window thumbnails, a split System thumbnail,
  labels underneath, and an offset blue ring. It was inspected in both light
  and dark settings windows after rechecking the actual macOS Appearance page.
- System could be selected again after an explicit theme override.
- SQLite recorded the selected theme. Relaunching the QA application restored
  Light in both the native window and History.
- A pending item-count draft of 50 survived changing the theme in Appearance
  and returning to Clipboard; the saved count remained 100.
- The configuration area scrolled independently; the sidebar and page title
  stayed visible. Category navigation returned content to the top.
- General, Appearance, and Menu Bar no longer showed explanatory text beneath
  individual controls. Their category descriptions remained.
- Clipboard storage limits rendered as two compact rows with question-mark
  buttons. Both short help bubbles were visible without card clipping; Escape
  and clicking outside collapsed them. The storage-budget help retained the
  single-item size limit.
- The final window used an overlay title bar with the real macOS window
  controls. A drag gesture completed without an IPC error; double-clicking
  the header visibly zoomed and restored the window. The native full-screen
  button and `Control+Command+F` returned to the original window layout.

## Window scroll follow-up

The document viewport now stays fixed on both pages. The fixed pane
backgrounds, settings header, and native-window-controls spacer are separate
from the inner content scrollers. Vertical boundary feedback is contained
within each pane; horizontal overscroll is disabled. History's list, pagination
observer, scroll anchors, and scroll-to-top animation share `.content-panel`
as their scroll owner.

Rechecked with the isolated debug app at 600 × 700 and the minimum 600 × 400:

- A short General page kept the sidebar, title, and background in place after
  scrolling in both directions.
- At 400 pixels high, Clipboard scrolled from capture settings to the bottom
  history actions while the page title and sidebar stayed in place.
- The same long-page check passed in light and dark themes; category changes
  reset the configuration area to the top.
- In the final History layout at 600 × 400, the list scrolled independently in
  both themes while the native-controls area stayed fixed and search remained
  sticky. Scrolling automatically loaded all 65 synthetic records, including
  after visiting Settings and returning.
- Page Down worked from the initially focused history container. Command+F
  focused search at the current reading position, and Escape from an empty
  search returned focus to the history container.
- Pinning a synthetic row from the later part of the list animated the inner
  scroller back to the Pinned group. Anchor preservation and fallback when an
  anchored row disappears have component-test coverage.

This regression check also found the Pin cursor migration had left command
validation accepting only `v1` while the database emitted `v2`. The command
now shares the database decoder. Tests cover both pin states, malformed/legacy
cursors, and a second default page for ordinary and search history.

These checks used computer-use scroll actions. They do not fully reproduce
physical trackpad gesture phases or momentum, so the precise release/rebound
feel still needs a hardware gesture check. System Settings was inspected
read-only and no system preferences were changed.

## Remaining manual coverage

System appearance transitions, increased contrast, reduced transparency,
keyboard-only use with a screen reader, and Intel hardware were not manually
exercised in this run. Media-query behavior and modal keyboard focus have
automated coverage. Pin and menu-bar behavior retain their earlier evidence in
`docs/qa/pin-and-macos-ui.md`.

AppKit emitted intermittent `Window move completed without beginning` warnings
during native window gesture testing. There was no rejected IPC or observed
zoom/full-screen failure. Local Tauri/Tao source review found no duplicate
project drag handler; the underlying AppKit warning was not diagnosed further.
