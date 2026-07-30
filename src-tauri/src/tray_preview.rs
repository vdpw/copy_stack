use crate::{tray::EVENT_MENU_START_INDEX, AppState};
use objc2::{
    define_class, msg_send,
    rc::Retained,
    runtime::{NSObjectProtocol, ProtocolObject},
    DeclaredClass, MainThreadOnly,
};
use objc2_app_kit::{
    NSAccessibilityElementProtocol, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor,
    NSEvent, NSFont, NSFontWeightRegular, NSMenu, NSMenuDelegate, NSMenuItem, NSPanel,
    NSPopUpMenuWindowLevel, NSScreen, NSTextView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindowAnimationBehavior,
    NSWindowCollectionBehavior, NSWindowStyleMask,
};
use objc2_foundation::{MainThreadMarker, NSObject, NSPoint, NSRect, NSSize, NSString};
use std::{cell::RefCell, sync::Arc};
use tauri::{tray::TrayIcon, AppHandle, Manager, Runtime};

const ERROR_TRAY_PREVIEW_UNAVAILABLE: &str = "tray_preview_unavailable";
const PREVIEW_SIDE_GAP: f64 = 3.0;
const PREVIEW_SCREEN_MARGIN: f64 = 12.0;
const PREVIEW_MIN_WIDTH: f64 = 180.0;
const PREVIEW_MAX_WIDTH: f64 = 380.0;
const PREVIEW_MIN_HEIGHT: f64 = 36.0;
const PREVIEW_MAX_SCREEN_HEIGHT_RATIO: f64 = 0.618;
const PREVIEW_FALLBACK_SCREEN_HEIGHT: f64 = 1040.0;
const PREVIEW_FONT_SIZE: f64 = 12.0;
const PREVIEW_CORNER_RADIUS: f64 = 6.0;
const PREVIEW_CHARACTER_WIDTH: f64 = 7.2;
const PREVIEW_HORIZONTAL_PADDING: f64 = 16.0;
const PREVIEW_LINE_HEIGHT: f64 = 15.0;
const PREVIEW_VERTICAL_PADDING: f64 = 12.0;
const PREVIEW_TRUNCATION_SUFFIX: &str = "...";

type PreviewLoader = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

struct PreviewLayout {
    panel_size: NSSize,
    display_text: String,
}

struct TrayPreviewMenuDelegateIvars {
    content_hashes: Vec<String>,
    loader: PreviewLoader,
    panel: Retained<NSPanel>,
    text_view: Retained<NSTextView>,
    current_hash: RefCell<Option<String>>,
    menu_anchor_x: f64,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "CopyStackTrayPreviewMenuDelegate"]
    #[thread_kind = MainThreadOnly]
    #[ivars = TrayPreviewMenuDelegateIvars]
    struct TrayPreviewMenuDelegate;

    unsafe impl NSObjectProtocol for TrayPreviewMenuDelegate {}

    #[allow(non_snake_case)]
    unsafe impl NSMenuDelegate for TrayPreviewMenuDelegate {
        #[unsafe(method(menu:willHighlightItem:))]
        fn menu_willHighlightItem(&self, menu: &NSMenu, item: Option<&NSMenuItem>) {
            self.update_for_highlighted_item(menu, item);
        }

        #[unsafe(method(menuDidClose:))]
        fn menuDidClose(&self, _menu: &NSMenu) {
            self.hide_preview();
        }
    }
);

thread_local! {
    static ACTIVE_PREVIEW_DELEGATE: RefCell<Option<Retained<TrayPreviewMenuDelegate>>> =
        const { RefCell::new(None) };
}

impl TrayPreviewMenuDelegate {
    fn new(
        mtm: MainThreadMarker,
        content_hashes: Vec<String>,
        loader: PreviewLoader,
        menu_anchor_x: f64,
    ) -> Retained<Self> {
        let (panel, text_view) = create_preview_panel(mtm);
        let this = mtm.alloc().set_ivars(TrayPreviewMenuDelegateIvars {
            content_hashes,
            loader,
            panel,
            text_view,
            current_hash: RefCell::new(None),
            menu_anchor_x,
        });
        unsafe { msg_send![super(this), init] }
    }

    fn update_for_highlighted_item(&self, menu: &NSMenu, item: Option<&NSMenuItem>) {
        let Some(item) = item else {
            self.hide_preview();
            return;
        };
        let menu_index = menu.indexOfItem(item);
        let Ok(menu_index) = usize::try_from(menu_index) else {
            self.hide_preview();
            return;
        };
        let Some(event_index) = menu_index.checked_sub(EVENT_MENU_START_INDEX) else {
            self.hide_preview();
            return;
        };
        let Some(content_hash) = self.ivars().content_hashes.get(event_index) else {
            self.hide_preview();
            return;
        };

        if self.ivars().current_hash.borrow().as_deref() == Some(content_hash.as_str()) {
            return;
        }

        let Some(preview) = (self.ivars().loader)(content_hash) else {
            self.hide_preview();
            return;
        };
        *self.ivars().current_hash.borrow_mut() = Some(content_hash.clone());
        if !preview_needs_panel(&item.title().to_string(), &preview) {
            self.ivars().panel.orderOut(None);
            return;
        }
        self.show_preview(menu, item, &preview);
    }

    fn show_preview(&self, menu: &NSMenu, item: &NSMenuItem, preview: &str) {
        let mtm = MainThreadMarker::from(self);
        let mouse = NSEvent::mouseLocation();
        let visible_frame = visible_screen_frame(mtm, mouse);
        let menu_width = menu.size().width.max(200.0);
        let item_frame = highlighted_item_frame(
            item,
            mouse,
            visible_frame,
            self.ivars().menu_anchor_x,
            menu_width,
        );
        let available_width = available_width_beside(visible_frame, item_frame);
        let available_height = (visible_frame.size.height * PREVIEW_MAX_SCREEN_HEIGHT_RATIO)
            .min((visible_frame.size.height - PREVIEW_SCREEN_MARGIN * 2.0).max(0.0));
        let Some(layout) = estimated_preview_layout(preview, available_width, available_height)
        else {
            self.hide_preview();
            return;
        };

        let origin = preview_origin(visible_frame, item_frame, layout.panel_size);
        let frame = NSRect::new(origin, layout.panel_size);
        let preview_string = NSString::from_str(&layout.display_text);

        self.ivars().panel.setFrame_display(frame, true);
        self.ivars().text_view.setFrameSize(layout.panel_size);
        self.ivars().text_view.setString(&preview_string);
        self.ivars().panel.orderFrontRegardless();
    }

    fn hide_preview(&self) {
        *self.ivars().current_hash.borrow_mut() = None;
        self.ivars().panel.orderOut(None);
    }
}

pub(crate) fn install<R: Runtime>(
    app: &AppHandle<R>,
    tray: &TrayIcon<R>,
    content_hashes: Vec<String>,
) -> Result<(), String> {
    let preview_app = app.clone();
    let loader: PreviewLoader = Arc::new(move |content_hash| {
        let state = preview_app.state::<AppState>();
        let preview = {
            let db = state.db.lock().ok()?;
            db.get_tray_preview(content_hash).ok().flatten()?
        };
        crate::tray::tray_preview_text(&preview)
    });

    tray.with_inner_tray_icon(move |inner| {
        let mtm =
            MainThreadMarker::new().ok_or_else(|| ERROR_TRAY_PREVIEW_UNAVAILABLE.to_string())?;
        let status_item = inner
            .ns_status_item()
            .ok_or_else(|| ERROR_TRAY_PREVIEW_UNAVAILABLE.to_string())?;
        let menu = status_item
            .menu(mtm)
            .ok_or_else(|| ERROR_TRAY_PREVIEW_UNAVAILABLE.to_string())?;
        let menu_anchor_x = status_item
            .button(mtm)
            .and_then(|button| button.window())
            .map(|window| window.frame().origin.x)
            .unwrap_or_else(|| NSEvent::mouseLocation().x);
        let delegate = TrayPreviewMenuDelegate::new(mtm, content_hashes, loader, menu_anchor_x);
        menu.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        ACTIVE_PREVIEW_DELEGATE.with(|slot| {
            if let Some(previous) = slot.replace(Some(delegate)) {
                previous.hide_preview();
            }
        });
        Ok(())
    })
    .map_err(|_| ERROR_TRAY_PREVIEW_UNAVAILABLE.to_string())?
}

fn create_preview_panel(mtm: MainThreadMarker) -> (Retained<NSPanel>, Retained<NSTextView>) {
    let initial_size = NSSize::new(PREVIEW_MAX_WIDTH, PREVIEW_MIN_HEIGHT);
    let initial_frame = NSRect::new(NSPoint::new(0.0, 0.0), initial_size);
    let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;
    let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
        mtm.alloc(),
        initial_frame,
        style,
        NSBackingStoreType::Buffered,
        false,
    );
    let clear_background = NSColor::clearColor();
    panel.setLevel(NSPopUpMenuWindowLevel);
    panel.setBackgroundColor(Some(&clear_background));
    panel.setOpaque(false);
    panel.setHasShadow(true);
    panel.setIgnoresMouseEvents(true);
    panel.setHidesOnDeactivate(false);
    panel.setBecomesKeyOnlyIfNeeded(true);
    panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
    panel.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Transient
            | NSWindowCollectionBehavior::IgnoresCycle,
    );

    let effect_view = NSVisualEffectView::initWithFrame(mtm.alloc(), initial_frame);
    effect_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    effect_view.setMaterial(NSVisualEffectMaterial::Menu);
    effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    effect_view.setState(NSVisualEffectState::Active);
    effect_view.setWantsLayer(true);
    if let Some(layer) = effect_view.layer() {
        layer.setCornerRadius(PREVIEW_CORNER_RADIUS);
        layer.setMasksToBounds(true);
    }

    let text_view = NSTextView::initWithFrame(mtm.alloc(), initial_frame);
    text_view.setAutoresizingMask(
        NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
    );
    text_view.setEditable(false);
    text_view.setSelectable(false);
    text_view.setRichText(false);
    text_view.setImportsGraphics(false);
    text_view.setDrawsBackground(false);
    text_view.setTextColor(Some(&NSColor::labelColor()));
    text_view.setFont(Some(&NSFont::monospacedSystemFontOfSize_weight(
        PREVIEW_FONT_SIZE,
        unsafe { NSFontWeightRegular },
    )));
    text_view.setTextContainerInset(NSSize::new(8.0, 6.0));
    text_view.setHorizontallyResizable(false);
    text_view.setVerticallyResizable(false);
    text_view.setContinuousSpellCheckingEnabled(false);
    text_view.setAutomaticSpellingCorrectionEnabled(false);
    text_view.setAutomaticQuoteSubstitutionEnabled(false);
    text_view.setAutomaticLinkDetectionEnabled(false);
    text_view.setAutomaticDataDetectionEnabled(false);
    if let Some(text_container) = unsafe { text_view.textContainer() } {
        text_container.setWidthTracksTextView(true);
        text_container.setLineFragmentPadding(0.0);
    }

    effect_view.addSubview(&text_view);
    panel.setContentView(Some(&effect_view));
    (panel, text_view)
}

fn visible_screen_frame(mtm: MainThreadMarker, point: NSPoint) -> NSRect {
    for screen in NSScreen::screens(mtm).iter() {
        let frame = screen.frame();
        if point.x >= frame.origin.x
            && point.x <= frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y <= frame.origin.y + frame.size.height
        {
            return screen.visibleFrame();
        }
    }

    NSScreen::mainScreen(mtm).map_or_else(
        || {
            NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(PREVIEW_MAX_WIDTH * 2.0, PREVIEW_FALLBACK_SCREEN_HEIGHT),
            )
        },
        |screen| screen.visibleFrame(),
    )
}

fn highlighted_item_frame(
    item: &NSMenuItem,
    mouse: NSPoint,
    visible_frame: NSRect,
    menu_anchor_x: f64,
    menu_width: f64,
) -> NSRect {
    let accessibility_frame = NSAccessibilityElementProtocol::accessibilityFrame(item);
    let screen_max_x = visible_frame.origin.x + visible_frame.size.width;
    let screen_max_y = visible_frame.origin.y + visible_frame.size.height;
    if accessibility_frame.size.width > 0.0
        && accessibility_frame.size.height > 0.0
        && accessibility_frame.origin.x < screen_max_x
        && accessibility_frame.origin.x + accessibility_frame.size.width > visible_frame.origin.x
        && accessibility_frame.origin.y < screen_max_y
        && accessibility_frame.origin.y + accessibility_frame.size.height > visible_frame.origin.y
    {
        return accessibility_frame;
    }

    let max_menu_left = (visible_frame.origin.x + visible_frame.size.width - menu_width)
        .max(visible_frame.origin.x);
    let menu_left = menu_anchor_x.clamp(visible_frame.origin.x, max_menu_left);
    NSRect::new(
        NSPoint::new(menu_left, mouse.y - PREVIEW_LINE_HEIGHT / 2.0),
        NSSize::new(menu_width, PREVIEW_LINE_HEIGHT),
    )
}

fn available_width_beside(visible_frame: NSRect, item_frame: NSRect) -> f64 {
    let screen_min_x = visible_frame.origin.x + PREVIEW_SCREEN_MARGIN;
    let screen_max_x = visible_frame.origin.x + visible_frame.size.width - PREVIEW_SCREEN_MARGIN;
    let left_width = item_frame.origin.x - PREVIEW_SIDE_GAP - screen_min_x;
    let right_width =
        screen_max_x - (item_frame.origin.x + item_frame.size.width + PREVIEW_SIDE_GAP);
    left_width.max(right_width).max(0.0)
}

fn preview_origin(visible_frame: NSRect, item_frame: NSRect, preview_size: NSSize) -> NSPoint {
    let min_x = visible_frame.origin.x + PREVIEW_SCREEN_MARGIN;
    let max_x = visible_frame.origin.x + visible_frame.size.width
        - preview_size.width
        - PREVIEW_SCREEN_MARGIN;
    let left_x = item_frame.origin.x - preview_size.width - PREVIEW_SIDE_GAP;
    let right_x = item_frame.origin.x + item_frame.size.width + PREVIEW_SIDE_GAP;
    let x = if left_x >= min_x {
        left_x
    } else if right_x <= max_x {
        right_x
    } else {
        (item_frame.origin.x + item_frame.size.width / 2.0 - preview_size.width / 2.0)
            .clamp(min_x, max_x.max(min_x))
    };

    let min_y = visible_frame.origin.y + PREVIEW_SCREEN_MARGIN;
    let max_y = visible_frame.origin.y + visible_frame.size.height
        - preview_size.height
        - PREVIEW_SCREEN_MARGIN;
    let y = (item_frame.origin.y + item_frame.size.height / 2.0 - preview_size.height / 2.0)
        .clamp(min_y, max_y.max(min_y));
    NSPoint::new(x, y)
}

fn estimated_preview_layout(
    preview: &str,
    available_width: f64,
    available_height: f64,
) -> Option<PreviewLayout> {
    if available_width <= 0.0 || available_height <= 0.0 {
        return None;
    }

    let maximum_width = PREVIEW_MAX_WIDTH.min(available_width);
    let minimum_width = PREVIEW_MIN_WIDTH.min(maximum_width);
    let content_width = preview.split('\n').map(display_columns).max().unwrap_or(1) as f64
        * PREVIEW_CHARACTER_WIDTH
        + PREVIEW_HORIZONTAL_PADDING;
    let width = content_width.clamp(minimum_width, maximum_width);
    let text_width = (width - PREVIEW_HORIZONTAL_PADDING).max(PREVIEW_CHARACTER_WIDTH);
    let wrap_columns = (text_width / PREVIEW_CHARACTER_WIDTH).floor() as usize;
    let max_visual_lines = (((available_height - PREVIEW_VERTICAL_PADDING).max(PREVIEW_LINE_HEIGHT)
        / PREVIEW_LINE_HEIGHT)
        .floor() as usize)
        .max(1);
    let (display_text, _) = fit_preview_text(preview, wrap_columns.max(1), max_visual_lines);
    let panel_height = estimated_preview_height(&display_text, wrap_columns.max(1))
        .min(available_height)
        .max(PREVIEW_MIN_HEIGHT.min(available_height));
    Some(PreviewLayout {
        panel_size: NSSize::new(width, panel_height),
        display_text,
    })
}

fn preview_needs_panel(menu_title: &str, preview: &str) -> bool {
    menu_title != preview
}

fn character_columns(character: char, columns: usize) -> usize {
    if character == '\t' {
        4 - columns % 4
    } else if character.is_ascii() {
        1
    } else {
        2
    }
}

fn display_columns(line: &str) -> usize {
    line.chars().fold(0, |columns, character| {
        columns + character_columns(character, columns)
    })
}

fn visual_line_count(preview: &str, wrap_columns: usize) -> usize {
    preview
        .split('\n')
        .map(|line| display_columns(line).div_ceil(wrap_columns.max(1)).max(1))
        .sum::<usize>()
        .max(1)
}

fn fit_preview_text(preview: &str, wrap_columns: usize, max_visual_lines: usize) -> (String, bool) {
    let wrap_columns = wrap_columns.max(1);
    let max_visual_lines = max_visual_lines.max(1);
    if visual_line_count(preview, wrap_columns) <= max_visual_lines {
        return (preview.to_string(), false);
    }

    let suffix_columns = PREVIEW_TRUNCATION_SUFFIX.len().min(wrap_columns);
    let final_line_limit = wrap_columns.saturating_sub(suffix_columns);
    let mut fitted = String::new();
    let mut visual_line = 1usize;
    let mut columns = 0usize;

    for character in preview.chars() {
        if character == '\n' {
            if visual_line >= max_visual_lines {
                break;
            }
            fitted.push(character);
            visual_line += 1;
            columns = 0;
            continue;
        }

        let mut character_width = character_columns(character, columns);
        if columns > 0 && columns + character_width > wrap_columns {
            visual_line += 1;
            columns = 0;
            character_width = character_columns(character, columns);
        }
        if visual_line > max_visual_lines {
            break;
        }

        let line_limit = if visual_line == max_visual_lines {
            final_line_limit
        } else {
            wrap_columns
        };
        if columns + character_width > line_limit {
            if visual_line >= max_visual_lines {
                break;
            }
            visual_line += 1;
            columns = 0;
            character_width = character_columns(character, columns);
            if character_width > final_line_limit {
                break;
            }
        }

        fitted.push(character);
        columns += character_width;
    }

    while fitted.chars().last().is_some_and(char::is_whitespace) {
        fitted.pop();
    }
    fitted.push_str(PREVIEW_TRUNCATION_SUFFIX);
    (fitted, true)
}

fn estimated_preview_height(preview: &str, wrap_columns: usize) -> f64 {
    visual_line_count(preview, wrap_columns) as f64 * PREVIEW_LINE_HEIGHT + PREVIEW_VERTICAL_PADDING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_is_hidden_only_when_the_menu_title_contains_the_complete_content() {
        assert!(!preview_needs_panel(
            "short clipboard text",
            "short clipboard text"
        ));
        assert!(preview_needs_panel(
            "first line second line",
            "first line\nsecond line"
        ));
        assert!(preview_needs_panel(
            "value with spaces",
            "value  with  spaces"
        ));
        assert!(preview_needs_panel(
            "long clipboard text...",
            "long clipboard text continues"
        ));
    }

    #[test]
    fn preview_height_tracks_newlines_and_wrapping() {
        let wrap_columns =
            ((PREVIEW_MAX_WIDTH - PREVIEW_HORIZONTAL_PADDING) / PREVIEW_CHARACTER_WIDTH) as usize;
        assert_eq!(
            estimated_preview_height("first\nsecond", wrap_columns),
            PREVIEW_LINE_HEIGHT * 2.0 + PREVIEW_VERTICAL_PADDING
        );
        assert_eq!(
            estimated_preview_height(&"x".repeat(wrap_columns + 1), wrap_columns),
            PREVIEW_LINE_HEIGHT * 2.0 + PREVIEW_VERTICAL_PADDING
        );
    }

    #[test]
    fn preview_layout_adapts_width_and_truncates_at_the_height_limit() {
        let short = estimated_preview_layout("short", 1000.0, 900.0).unwrap();
        let code =
            estimated_preview_layout("case demandRef != nil && productRef != nil:", 1000.0, 900.0)
                .unwrap();
        let wide = estimated_preview_layout(&"x".repeat(200), 1000.0, 900.0).unwrap();
        let multiline = estimated_preview_layout(&"line\n".repeat(100), 1000.0, 180.0).unwrap();

        assert_eq!(short.panel_size.width, PREVIEW_MIN_WIDTH);
        assert!(code.panel_size.width > PREVIEW_MIN_WIDTH);
        assert!(code.panel_size.width < PREVIEW_MAX_WIDTH);
        assert_eq!(wide.panel_size.width, PREVIEW_MAX_WIDTH);
        assert_eq!(multiline.panel_size.width, PREVIEW_MIN_WIDTH);
        assert_eq!(short.panel_size.height, PREVIEW_MIN_HEIGHT);
        assert!(multiline.panel_size.height <= 180.0);
        assert!(multiline.display_text.ends_with(PREVIEW_TRUNCATION_SUFFIX));
    }

    #[test]
    fn preview_width_only_shrinks_when_the_screen_requires_it() {
        let layout = estimated_preview_layout("text", 140.0, 900.0).unwrap();

        assert_eq!(layout.panel_size.width, 140.0);
    }

    #[test]
    fn display_columns_accounts_for_tabs_and_wide_characters() {
        assert_eq!(display_columns("a\tb"), 5);
        assert_eq!(display_columns("文本"), 4);
    }

    #[test]
    fn preview_truncation_preserves_character_boundaries_and_reserves_suffix_space() {
        let (fitted, truncated) = fit_preview_text("中文abcdef\nsecond line", 8, 2);

        assert!(truncated);
        assert!(fitted.ends_with(PREVIEW_TRUNCATION_SUFFIX));
        assert!(fitted.is_char_boundary(fitted.len()));
        assert!(visual_line_count(&fitted, 8) <= 2);
    }

    #[test]
    fn preview_origin_stays_inside_the_visible_screen_and_touches_menu_gap() {
        let screen = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(1440.0, 900.0));
        let item = NSRect::new(NSPoint::new(1000.0, 700.0), NSSize::new(420.0, 24.0));
        let size = NSSize::new(460.0, 500.0);
        let origin = preview_origin(screen, item, size);

        assert!(origin.x >= PREVIEW_SCREEN_MARGIN);
        assert!(origin.y >= PREVIEW_SCREEN_MARGIN);
        assert!(origin.x + size.width <= screen.size.width - PREVIEW_SCREEN_MARGIN);
        assert!(origin.y + size.height <= screen.size.height - PREVIEW_SCREEN_MARGIN);
        assert_eq!(origin.x + size.width + PREVIEW_SIDE_GAP, item.origin.x);
    }
}
