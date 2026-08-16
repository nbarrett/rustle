use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::{MainThreadMarker, MainThreadOnly};
use dispatch2::DispatchQueue;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSLineBreakMode, NSPanel, NSPopUpMenuWindowLevel, NSScreen,
    NSTextAlignment, NSTextField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use rustle_core::engine::DictationStatus;
use tauri::tray::TrayIcon;
use tauri::AppHandle;

const OVERLAY_HORIZONTAL_PADDING: f64 = 22.0;
const OVERLAY_VERTICAL_PADDING: f64 = 14.0;
const OVERLAY_MAX_TEXT_WIDTH: f64 = 520.0;
const OVERLAY_MIN_WIDTH: f64 = 220.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 80.0;
const OVERLAY_HIDE_DELAY: Duration = Duration::from_millis(1400);

struct NativeOverlay {
    panel: Retained<NSPanel>,
    field: Retained<NSTextField>,
}

unsafe impl Send for NativeOverlay {}
unsafe impl Sync for NativeOverlay {}

pub struct DictationOverlay {
    native: Option<Arc<NativeOverlay>>,
    generation: Arc<AtomicU64>,
}

impl DictationOverlay {
    pub fn create() -> Self {
        Self {
            native: NativeOverlay::try_create().map(Arc::new),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub fn apply(&self, app: &AppHandle, tray: &TrayIcon, status: &DictationStatus) {
        match status {
            DictationStatus::Listening => {
                self.bump_generation();
                set_tray(tray, None, Some("Rustle · Listening"));
                self.show_message("●  Listening…");
            }
            DictationStatus::Partial(text) => {
                self.bump_generation();
                set_tray(tray, None, Some(&format!("Rustle · {text}")));
                self.show_message(&format!("●  {text}"));
            }
            DictationStatus::Transcribing => {
                self.bump_generation();
                set_tray(tray, None, Some("Rustle · Transcribing"));
            }
            DictationStatus::Typed(text) => {
                set_tray(tray, None, Some(&format!("Rustle · {text}")));
                self.show_message(text);
                self.schedule_hide(app, tray);
            }
            DictationStatus::Failed(message) => {
                set_tray(tray, Some("Error"), Some(&format!("Rustle · {message}")));
                self.show_message(message);
                self.schedule_hide(app, tray);
            }
            DictationStatus::NeedsPermission(message) => {
                set_tray(tray, Some("Grant"), Some(&format!("Rustle · {message}")));
                self.show_message(message);
            }
            DictationStatus::Idle => {
                set_tray(tray, None, Some("Rustle"));
                self.hide();
            }
        }
    }

    fn show_message(&self, text: &str) {
        let Some(native) = self.native.as_ref() else {
            return;
        };
        native.set_text(text);
        native.panel.orderFront(None);
    }

    fn hide(&self) {
        let Some(native) = self.native.as_ref() else {
            return;
        };
        native.panel.orderOut(None);
    }

    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn schedule_hide(&self, _app: &AppHandle, tray: &TrayIcon) {
        let token = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let native = self.native.clone();
        let tray = tray.clone();
        thread::spawn(move || {
            thread::sleep(OVERLAY_HIDE_DELAY);
            if generation.load(Ordering::SeqCst) != token {
                return;
            }
            run_on_appkit_main(move || {
                if generation.load(Ordering::SeqCst) != token {
                    return;
                }
                if let Some(native) = native.as_ref() {
                    native.panel.orderOut(None);
                }
                set_tray(&tray, None, Some("Rustle"));
            });
        });
    }
}

impl NativeOverlay {
    fn try_create() -> Option<Self> {
        let mtm = MainThreadMarker::new()?;
        let style =
            NSWindowStyleMask::Borderless.union(NSWindowStyleMask::NonactivatingPanel);
        let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
            NSPanel::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(280.0, 56.0)),
            style,
            NSBackingStoreType::Buffered,
            false,
        );
        panel.setFloatingPanel(true);
        panel.setBecomesKeyOnlyIfNeeded(true);
        panel.setWorksWhenModal(true);
        panel.setHidesOnDeactivate(false);
        panel.setLevel(NSPopUpMenuWindowLevel);
        panel.setOpaque(true);
        panel.setHasShadow(true);
        panel.setIgnoresMouseEvents(true);
        panel.setAnimationBehavior(NSWindowAnimationBehavior::None);
        panel.setBackgroundColor(Some(&NSColor::colorWithSRGBRed_green_blue_alpha(
            0.11, 0.11, 0.13, 0.96,
        )));
        panel.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                .union(NSWindowCollectionBehavior::Stationary)
                .union(NSWindowCollectionBehavior::IgnoresCycle)
                .union(NSWindowCollectionBehavior::FullScreenAuxiliary),
        );
        unsafe {
            panel.setReleasedWhenClosed(false);
        }

        let visual = NSVisualEffectView::initWithFrame(
            NSVisualEffectView::alloc(mtm),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(280.0, 56.0)),
        );
        visual.setMaterial(NSVisualEffectMaterial::HUDWindow);
        visual.setBlendingMode(NSVisualEffectBlendingMode::WithinWindow);
        visual.setState(NSVisualEffectState::Active);
        visual.setWantsLayer(true);

        let field = NSTextField::labelWithString(&NSString::from_str(""), mtm);
        field.setFont(Some(&NSFont::systemFontOfSize(15.0)));
        field.setTextColor(Some(&NSColor::whiteColor()));
        field.setAlignment(NSTextAlignment::Left);
        field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
        field.setMaximumNumberOfLines(2);
        field.setPreferredMaxLayoutWidth(OVERLAY_MAX_TEXT_WIDTH);
        field.setDrawsBackground(false);
        field.setSelectable(false);

        visual.addSubview(&field);
        panel.setContentView(Some(&visual));

        Some(Self { panel, field })
    }

    fn set_text(&self, text: &str) {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        self.field.setStringValue(&NSString::from_str(text));
        self.field.sizeToFit();
        let text_size = self.field.frame().size;
        let width = text_size
            .width
            .min(OVERLAY_MAX_TEXT_WIDTH)
            .max(OVERLAY_MIN_WIDTH)
            + (OVERLAY_HORIZONTAL_PADDING * 2.0);
        let height = text_size.height + (OVERLAY_VERTICAL_PADDING * 2.0);
        self.panel
            .setContentSize(NSSize::new(width, height));
        if let Some(visual) = self.panel.contentView() {
            visual.setFrame(NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width, height),
            ));
        }
        self.field.setFrame(NSRect::new(
            NSPoint::new(OVERLAY_HORIZONTAL_PADDING, OVERLAY_VERTICAL_PADDING),
            NSSize::new(width - (OVERLAY_HORIZONTAL_PADDING * 2.0), text_size.height),
        ));
        if let Some(screen) = NSScreen::mainScreen(mtm) {
            let visible = screen.visibleFrame();
            let x = visible.origin.x + ((visible.size.width - width) / 2.0);
            let y = visible.origin.y + OVERLAY_BOTTOM_MARGIN;
            self.panel.setFrameOrigin(NSPoint::new(x, y));
        }
    }
}

pub fn run_on_appkit_main(work: impl FnOnce() + Send + 'static) {
    DispatchQueue::main().exec_async(work);
}

fn set_tray(tray: &TrayIcon, title: Option<&str>, tooltip: Option<&str>) {
    let _ = tray.set_title(title);
    let _ = tray.set_tooltip(tooltip);
}
