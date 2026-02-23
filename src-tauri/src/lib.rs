pub mod api;
pub mod config;
pub mod usage;

use api::ClaudeClient;
use config::AppConfig;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{
    image::Image,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_store::StoreExt;
use usage::{UsageColor, UsageState};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub client: Mutex<Option<ClaudeClient>>,
    pub usage: Mutex<Option<UsageState>>,
    pub blink_active: Arc<AtomicBool>,
}

#[tauri::command]
fn get_usage(state: tauri::State<'_, AppState>) -> Option<UsageState> {
    state.usage.lock().unwrap().clone()
}

#[tauri::command]
fn save_config(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
    session_key: String,
    org_id: String,
) -> Result<String, String> {
    let mut config = state.config.lock().unwrap();
    config.session_key = session_key.clone();
    config.org_id = org_id.clone();

    // Persist to store
    persist_config(&app, &config);

    // Initialize the API client
    let client = ClaudeClient::new(&session_key, &org_id);
    *state.client.lock().unwrap() = Some(client);

    // Close setup window if open
    if let Some(window) = app.get_webview_window("setup") {
        let _ = window.close();
    }

    // Trigger immediate fetch
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        poll_usage(&app_handle).await;
    });

    Ok("Configuration saved".to_string())
}

#[tauri::command]
fn get_config(state: tauri::State<'_, AppState>) -> AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
fn refresh_now(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        poll_usage(&app).await;
    });
}

#[tauri::command]
fn hide_popup(app: AppHandle) {
    if let Some(window) = app.get_webview_window("popup") {
        let _ = window.hide();
    }
}

fn persist_config(app: &AppHandle, config: &AppConfig) {
    if let Ok(store) = app.store("config.json") {
        store.set("session_key", serde_json::json!(config.session_key));
        store.set("org_id", serde_json::json!(config.org_id));
        store.set(
            "poll_interval_secs",
            serde_json::json!(config.poll_interval_secs),
        );
    }
}

fn load_config(app: &AppHandle) -> AppConfig {
    let mut config = AppConfig::default();
    if let Ok(store) = app.store("config.json") {
        if let Some(val) = store.get("session_key") {
            if let Some(s) = val.as_str() {
                config.session_key = s.to_string();
            }
        }
        if let Some(val) = store.get("org_id") {
            if let Some(s) = val.as_str() {
                config.org_id = s.to_string();
            }
        }
        if let Some(val) = store.get("poll_interval_secs") {
            if let Some(n) = val.as_u64() {
                config.poll_interval_secs = n;
            }
        }
    }
    config
}

async fn poll_usage(app: &AppHandle) {
    let state = app.state::<AppState>();

    // Clone what we need from the client under the lock, then drop it before await
    let fetch_params = {
        let client_guard = state.client.lock().unwrap();
        match client_guard.as_ref() {
            Some(client) => Some((client.session_key().to_string(), client.org_id().to_string())),
            None => None,
        }
    };

    let Some((session_key, org_id)) = fetch_params else {
        return;
    };

    let client = ClaudeClient::new(&session_key, &org_id);

    match client.fetch_usage().await {
        Ok(result) => {
            let usage_state = usage::compute_state(&result.usage);
            let worst = usage::worst_color(&usage_state);

            // Set/clear blink flag
            state.blink_active.store(worst == UsageColor::RedBlink, Ordering::Relaxed);

            *state.usage.lock().unwrap() = Some(usage_state.clone());

            // Update tray icon
            if let Some(tray) = app.tray_by_id("main-tray") {
                update_tray_icon(&tray, Some(&usage_state));
            }

            // Emit to frontend
            let _ = app.emit("usage-updated", &usage_state);

            // Handle refreshed session key
            if let Some(new_key) = result.refreshed_session_key {
                let mut config = state.config.lock().unwrap();
                config.session_key = new_key.clone();
                persist_config(app, &config);

                if let Some(c) = state.client.lock().unwrap().as_mut() {
                    c.update_session_key(new_key);
                }
            }
        }
        Err(err) => {
            state.blink_active.store(false, Ordering::Relaxed);

            let error_state = UsageState {
                session: None,
                weekly: None,
                last_updated: chrono::Utc::now().to_rfc3339(),
                error: Some(err.clone()),
            };
            *state.usage.lock().unwrap() = Some(error_state.clone());

            if let Some(tray) = app.tray_by_id("main-tray") {
                update_tray_icon(&tray, None);
            }

            let _ = app.emit("usage-updated", &error_state);
        }
    }
}

fn update_tray_icon(tray: &tauri::tray::TrayIcon, state: Option<&UsageState>) {
    let (s_pct, s_color, w_pct, w_color) = match state {
        Some(s) => (
            s.session.as_ref().map(|b| b.utilization / 100.0).unwrap_or(0.0),
            s.session.as_ref().map(|b| b.color).unwrap_or(UsageColor::Gray),
            s.weekly.as_ref().map(|b| b.utilization / 100.0).unwrap_or(0.0),
            s.weekly.as_ref().map(|b| b.color).unwrap_or(UsageColor::Gray),
        ),
        None => (0.0, UsageColor::Gray, 0.0, UsageColor::Gray),
    };
    let (rgba, w, h) = generate_bars_rgba(s_pct, s_color, w_pct, w_color);
    let icon = Image::new_owned(rgba, w, h);
    let _ = tray.set_icon(Some(icon));
}

fn color_rgb(color: UsageColor) -> (u8, u8, u8) {
    match color {
        UsageColor::Green => (76, 175, 80),
        UsageColor::Yellow => (255, 193, 7),
        UsageColor::Red | UsageColor::RedBlink => (244, 67, 54),
        UsageColor::Gray => (120, 120, 120),
    }
}

fn pixel_in_rounded_rect(px: u32, py: u32, rx: u32, ry: u32, rw: u32, rh: u32, r: f64) -> bool {
    let cx = px as f64 + 0.5;
    let cy = py as f64 + 0.5;
    let left = rx as f64;
    let top = ry as f64;
    let right = left + rw as f64;
    let bottom = top + rh as f64;

    if cx < left || cx > right || cy < top || cy > bottom {
        return false;
    }

    if cx < left + r && cy < top + r {
        let dx = cx - (left + r);
        let dy = cy - (top + r);
        return dx * dx + dy * dy <= r * r;
    }
    if cx > right - r && cy < top + r {
        let dx = cx - (right - r);
        let dy = cy - (top + r);
        return dx * dx + dy * dy <= r * r;
    }
    if cx < left + r && cy > bottom - r {
        let dx = cx - (left + r);
        let dy = cy - (bottom - r);
        return dx * dx + dy * dy <= r * r;
    }
    if cx > right - r && cy > bottom - r {
        let dx = cx - (right - r);
        let dy = cy - (bottom - r);
        return dx * dx + dy * dy <= r * r;
    }

    true
}

fn draw_rounded_bar(
    rgba: &mut [u8],
    img_width: u32,
    x: u32, y: u32, w: u32, h: u32,
    radius: f64,
    track: (u8, u8, u8),
    fill: (u8, u8, u8),
    fill_pct: f64,
) {
    let fill_w = ((w as f64) * fill_pct.clamp(0.0, 1.0)) as u32;
    for py in y..y + h {
        for px in x..x + w {
            if !pixel_in_rounded_rect(px, py, x, y, w, h, radius) {
                continue;
            }
            let idx = ((py * img_width + px) * 4) as usize;
            let (r, g, b) = if px < x + fill_w { fill } else { track };
            rgba[idx] = r;
            rgba[idx + 1] = g;
            rgba[idx + 2] = b;
            rgba[idx + 3] = 255;
        }
    }
}

fn generate_bars_rgba(
    session_pct: f64,
    session_color: UsageColor,
    weekly_pct: f64,
    weekly_color: UsageColor,
) -> (Vec<u8>, u32, u32) {
    let width = 36u32;
    let height = 22u32;
    let bar_x = 2u32;
    let bar_w = 32u32;
    let bar_h = 7u32;
    let radius = 3.0f64;
    let top_y = 3u32;
    let bottom_y = top_y + bar_h + 2;
    let track = (68u8, 68, 72);

    let mut rgba = vec![0u8; (width * height * 4) as usize];

    draw_rounded_bar(
        &mut rgba, width,
        bar_x, top_y, bar_w, bar_h, radius,
        track, color_rgb(session_color), session_pct,
    );
    draw_rounded_bar(
        &mut rgba, width,
        bar_x, bottom_y, bar_w, bar_h, radius,
        track, color_rgb(weekly_color), weekly_pct,
    );

    (rgba, width, height)
}

fn show_popup(app: &AppHandle, position: Option<tauri::PhysicalPosition<f64>>) {
    if let Some(window) = app.get_webview_window("popup") {
        // Position near tray icon if we have coords
        if let Some(pos) = position {
            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition {
                x: (pos.x as i32).saturating_sub(180),
                y: pos.y as i32,
            }));
        }
        let _ = window.show();
        let _ = window.set_focus();

        // Re-emit current state so popup gets data
        let state = app.state::<AppState>();
        if let Some(usage_state) = state.usage.lock().unwrap().clone() {
            let _ = app.emit("usage-updated", &usage_state);
        }
        return;
    }

    let mut builder =
        WebviewWindowBuilder::new(app, "popup", WebviewUrl::App("index.html".into()))
            .title("Claude Meter")
            .inner_size(360.0, 120.0)
            .resizable(false)
            .decorations(false)
            .always_on_top(true)
            .visible(true)
            .focused(true)
            .skip_taskbar(true);

    // Position near tray icon
    if let Some(pos) = position {
        builder = builder.position(
            (pos.x - 180.0).max(0.0),
            pos.y,
        );
    }

    if let Ok(window) = builder.build() {
        // Emit data after a short delay to let webview initialize
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            let state = app_handle.state::<AppState>();
            let usage_data = state.usage.lock().unwrap().clone();
            if let Some(usage_state) = usage_data {
                let _ = app_handle.emit("usage-updated", &usage_state);
            }
        });

        // Close popup when it loses focus
        let app_handle = app.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::Focused(false) = event {
                if let Some(w) = app_handle.get_webview_window("popup") {
                    let _ = w.hide();
                }
            }
        });
    }
}

fn show_setup(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("setup") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(app, "setup", WebviewUrl::App("setup.html".into()))
        .title("Claude Meter Setup")
        .inner_size(480.0, 400.0)
        .resizable(false)
        .center()
        .visible(true)
        .focused(true)
        .build();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Load persisted config
            let config = load_config(&app.handle());
            let client = if config.is_configured() {
                Some(ClaudeClient::new(&config.session_key, &config.org_id))
            } else {
                None
            };

            let blink_active = Arc::new(AtomicBool::new(false));

            app.manage(AppState {
                config: Mutex::new(config.clone()),
                client: Mutex::new(client),
                usage: Mutex::new(None),
                blink_active: blink_active.clone(),
            });

            // Build tray menu
            let refresh_item =
                MenuItemBuilder::with_id("refresh", "Refresh Now").build(app)?;
            let open_claude =
                MenuItemBuilder::with_id("open_claude", "Open claude.ai Usage").build(app)?;
            let settings_item =
                MenuItemBuilder::with_id("settings", "Settings...").build(app)?;
            let quit_item =
                MenuItemBuilder::with_id("quit", "Quit Claude Meter").build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&refresh_item)
                .separator()
                .item(&open_claude)
                .item(&settings_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // Create initial icon — empty gray bars
            let (rgba, icon_w, icon_h) = generate_bars_rgba(
                0.0, UsageColor::Gray, 0.0, UsageColor::Gray,
            );
            let icon = Image::new_owned(rgba, icon_w, icon_h);

            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(icon)
                .icon_as_template(false)
                .tooltip("Claude Meter")
                .show_menu_on_left_click(false)
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "refresh" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(async move {
                            poll_usage(&app).await;
                        });
                    }
                    "open_claude" => {
                        let _ = app.opener().open_url("https://claude.ai/settings/usage", None::<&str>);
                    }
                    "settings" => {
                        show_setup(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        position,
                        ..
                    } = event
                    {
                        show_popup(tray.app_handle(), Some(position));
                    }
                })
                .build(app)?;

            // Tray blink loop — toggles icon when RedBlink is active
            {
                let app_handle = app.handle().clone();
                let blink_flag = blink_active.clone();
                tauri::async_runtime::spawn(async move {
                    let mut blink_on = true;
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        if !blink_flag.load(Ordering::Relaxed) {
                            blink_on = true;
                            continue;
                        }
                        blink_on = !blink_on;
                        if let Some(tray) = app_handle.tray_by_id("main-tray") {
                            if blink_on {
                                // Show normal bars
                                let state = app_handle.state::<AppState>();
                                let usage_data = state.usage.lock().unwrap().clone();
                                update_tray_icon(&tray, usage_data.as_ref());
                            } else {
                                // Show dimmed/empty bars
                                let (rgba, w, h) = generate_bars_rgba(
                                    0.0, UsageColor::Gray, 0.0, UsageColor::Gray,
                                );
                                let icon = Image::new_owned(rgba, w, h);
                                let _ = tray.set_icon(Some(icon));
                            }
                        }
                    }
                });
            }

            // Show setup if not configured
            if !config.is_configured() {
                show_setup(app.handle());
            } else {
                // Start polling
                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    poll_usage(&app_handle).await;
                    let state = app_handle.state::<AppState>();
                    let interval = state.config.lock().unwrap().poll_interval_secs;
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                        poll_usage(&app_handle).await;
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_usage,
            save_config,
            get_config,
            refresh_now,
            hide_popup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
