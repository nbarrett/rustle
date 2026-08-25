use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(target_os = "macos")]
use dispatch2::DispatchQueue;
#[cfg(target_os = "macos")]
use objc2::rc::Retained;
#[cfg(target_os = "macos")]
use objc2::{MainThreadMarker, MainThreadOnly};
#[cfg(target_os = "macos")]
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFont, NSLineBreakMode, NSPanel, NSPopUpMenuWindowLevel, NSScreen,
    NSTextAlignment, NSTextField, NSVisualEffectBlendingMode, NSVisualEffectMaterial,
    NSVisualEffectState, NSVisualEffectView, NSWindowAnimationBehavior, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
#[cfg(target_os = "macos")]
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use rustle_core::engine::DictationStatus;
use tauri::tray::TrayIcon;
use tauri::AppHandle;
#[cfg(not(target_os = "macos"))]
use tauri::Manager;

const OVERLAY_HORIZONTAL_PADDING: f64 = 22.0;
const OVERLAY_VERTICAL_PADDING: f64 = 14.0;
const OVERLAY_MAX_TEXT_WIDTH: f64 = 520.0;
const OVERLAY_MIN_WIDTH: f64 = 220.0;
const OVERLAY_BOTTOM_MARGIN: f64 = 80.0;
const OVERLAY_HIDE_DELAY: Duration = Duration::from_millis(1400);

#[cfg(target_os = "macos")]
struct NativeOverlay {
    panel: Retained<NSPanel>,
    field: Retained<NSTextField>,
}

#[cfg(target_os = "macos")]
unsafe impl Send for NativeOverlay {}
#[cfg(target_os = "macos")]
unsafe impl Sync for NativeOverlay {}

pub struct DictationOverlay {
    #[cfg(target_os = "macos")]
    native: Option<Arc<NativeOverlay>>,
    generation: Arc<AtomicU64>,
}

impl DictationOverlay {
    pub fn create() -> Self {
        let overlay = Self {
            #[cfg(target_os = "macos")]
            native: NativeOverlay::try_create().map(Arc::new),
            generation: Arc::new(AtomicU64::new(0)),
        };
        #[cfg(target_os = "macos")]
        overlay.keep_panel_alive();
        overlay
    }

    pub fn apply(&self, app: &AppHandle, tray: &TrayIcon, status: &DictationStatus) {
        match status {
            DictationStatus::Listening => {
                self.bump_generation();
                set_tray(tray, "", Some("Rustle · Listening"));
                self.show_message(app, "●  Listening…");
            }
            DictationStatus::Partial(text) => {
                self.bump_generation();
                set_tray(tray, "", Some("Rustle · Listening"));
                self.show_message(app, &format!("●  {text}"));
            }
            DictationStatus::Transcribing => {
                self.bump_generation();
                set_tray(tray, "", Some("Rustle · Transcribing"));
                self.show_message(app, "●  Transcribing…");
            }
            DictationStatus::Typed(_) => {
                set_tray(tray, "", Some("Rustle"));
                self.hide(app);
            }
            DictationStatus::Failed(message) => {
                set_tray(tray, "", Some(&format!("Rustle · {message}")));
                self.show_message(app, message);
                self.schedule_hide(app, tray);
            }
            DictationStatus::NeedsPermission(message) => {
                set_tray(tray, "", Some(&format!("Rustle · {message}")));
                self.show_message(app, message);
            }
            DictationStatus::SettingsPreview(_) => {}
            DictationStatus::Idle => {
                set_tray(tray, "", Some("Rustle"));
                self.hide(app);
            }
        }
    }

    fn show_message(&self, app: &AppHandle, text: &str) {
        #[cfg(target_os = "macos")]
        {
            let _ = app;
            let Some(native) = self.native.as_ref() else {
                return;
            };
            native.set_text(text);
            native.panel.setAlphaValue(1.0);
            native.panel.orderFront(None);
        }
        #[cfg(not(target_os = "macos"))]
        {
            show_hud(app, text);
        }
    }

    fn hide(&self, app: &AppHandle) {
        #[cfg(target_os = "macos")]
        {
            let _ = app;
            self.keep_panel_alive();
        }
        #[cfg(not(target_os = "macos"))]
        {
            hide_hud(app);
        }
    }

    #[cfg(target_os = "macos")]
    fn keep_panel_alive(&self) {
        let Some(native) = self.native.as_ref() else {
            return;
        };
        native.panel.setAlphaValue(0.0);
        native.panel.orderFront(None);
    }

    fn bump_generation(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn schedule_hide(&self, app: &AppHandle, tray: &TrayIcon) {
        let token = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let generation = self.generation.clone();
        let tray = tray.clone();
        let app = app.clone();
        #[cfg(not(target_os = "macos"))]
        let ui_app = app.clone();
        #[cfg(target_os = "macos")]
        let native = self.native.clone();
        thread::spawn(move || {
            thread::sleep(OVERLAY_HIDE_DELAY);
            if generation.load(Ordering::SeqCst) != token {
                return;
            }
            let _ = app.run_on_main_thread(move || {
                if generation.load(Ordering::SeqCst) != token {
                    return;
                }
                #[cfg(target_os = "macos")]
                if let Some(native) = native.as_ref() {
                    native.panel.setAlphaValue(0.0);
                    native.panel.orderFront(None);
                }
                #[cfg(not(target_os = "macos"))]
                hide_hud(&ui_app);
                set_tray(&tray, "", Some("Rustle"));
            });
        });
    }
}

#[cfg(target_os = "macos")]
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

#[cfg(not(target_os = "macos"))]
fn show_hud(app: &AppHandle, text: &str) {
    let Some(hud) = app.get_webview_window("hud") else {
        return;
    };
    let encoded = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    let _ = hud.eval(&format!(
        "var n=document.getElementById('msg'); if(n) n.textContent={encoded};"
    ));
    position_hud(&hud);
    show_hud_window(&hud);
}

#[cfg(not(target_os = "macos"))]
fn show_hud_window(hud: &tauri::WebviewWindow) {
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = hud.hwnd() {
            rustle_core::win_insert::prevent_window_activation(hwnd.0 as isize);
            rustle_core::win_insert::show_without_activating(hwnd.0 as isize);
            return;
        }
    }
    let _ = hud.show();
}

#[cfg(not(target_os = "macos"))]
fn hide_hud(app: &AppHandle) {
    if let Some(hud) = app.get_webview_window("hud") {
        let _ = hud.hide();
    }
}

#[cfg(not(target_os = "macos"))]
fn position_hud(hud: &tauri::WebviewWindow) {
    let Ok(Some(monitor)) = hud.primary_monitor() else {
        return;
    };
    let work = monitor.work_area();
    let Ok(size) = hud.outer_size() else {
        return;
    };
    let x = work.position.x + ((work.size.width as i32 - size.width as i32) / 2);
    let y = work.position.y + work.size.height as i32 - size.height as i32 - 80;
    let _ = hud.set_position(tauri::PhysicalPosition::new(x, y));
}

pub fn run_on_ui_thread(app: &AppHandle, work: impl FnOnce() + Send + 'static) {
    #[cfg(target_os = "macos")]
    {
        let _ = app;
        DispatchQueue::main().exec_async(work);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app.run_on_main_thread(work);
    }
}

fn set_tray(tray: &TrayIcon, title: &str, tooltip: Option<&str>) {
    let _ = tray.set_title(Some(title));
    let _ = tray.set_tooltip(tooltip);
}
