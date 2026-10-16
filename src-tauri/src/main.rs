#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod client;
mod credentials;
mod settings;
mod usage;

use serde::Serialize;
use settings::Settings;
use std::{collections::BTreeMap, path::PathBuf, sync::Mutex, time::Duration};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager,
};
use tauri_plugin_autostart::ManagerExt;
use usage::{Provider, Snapshot, UsageWindow};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppView {
    snapshots: BTreeMap<Provider, Snapshot>,
    settings: Settings,
    refreshing: bool,
    launch_at_login: bool,
    settings_error: Option<String>,
    title: String,
    demo: bool,
}
struct Data {
    snapshots: BTreeMap<Provider, Snapshot>,
    settings: Settings,
    refreshing: bool,
    launch_at_login: bool,
    settings_error: Option<String>,
}
struct AppState {
    data: Mutex<Data>,
    client: reqwest::Client,
    settings_path: PathBuf,
    demo: bool,
    refresh_gate: tokio::sync::Mutex<()>,
    schedule: tokio::sync::watch::Sender<u64>,
}
fn view(state: &AppState) -> AppView {
    let data = state.data.lock().unwrap_or_else(|e| e.into_inner());
    AppView {
        title: usage::title(&data.snapshots, &data.settings.selection),
        snapshots: data.snapshots.clone(),
        settings: data.settings.clone(),
        refreshing: data.refreshing,
        launch_at_login: data.launch_at_login,
        settings_error: data.settings_error.clone(),
        demo: state.demo,
    }
}
fn publish(app: &AppHandle) -> AppView {
    let state = app.state::<AppState>();
    let value = view(&state);
    if let Some(tray) = app.tray_by_id("usage") {
        let _ = tray.set_tooltip(Some(format!("AI Usage · {}", value.title)));
        #[cfg(target_os = "macos")]
        let _ = tray.set_title(Some(&value.title));
    }
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(&format!("AI Usage · {}", value.title));
    }
    let _ = app.emit("usage-changed", &value);
    value
}
fn show(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
fn demo_windows(provider: Provider) -> Vec<UsageWindow> {
    let group = if provider == Provider::Antigravity {
        Some("Gemini".into())
    } else {
        None
    };
    let mut windows = vec![
        UsageWindow {
            id: "five_hour".into(),
            group: group.clone(),
            label: "5시간".into(),
            used: match provider {
                Provider::Claude => 24.5,
                Provider::Codex => 37.0,
                Provider::Antigravity => 25.0,
            },
            reset_at: Some(chrono::Utc::now() + chrono::Duration::hours(2)),
        },
        UsageWindow {
            id: "seven_day".into(),
            group,
            label: "주간".into(),
            used: 63.0,
            reset_at: Some(chrono::Utc::now() + chrono::Duration::days(3)),
        },
    ];
    if provider == Provider::Antigravity {
        windows.push(UsageWindow {
            id: "argon_weekly".into(),
            group: Some("Gemini 4 Argon".into()),
            label: "주간".into(),
            used: 18.0,
            reset_at: Some(chrono::Utc::now() + chrono::Duration::days(5)),
        });
    }
    windows
}
#[tauri::command]
fn get_state(app: AppHandle) -> AppView {
    view(&app.state::<AppState>())
}
#[tauri::command]
async fn refresh_usage(app: AppHandle) -> Result<AppView, String> {
    let state = app.state::<AppState>();
    let Ok(_guard) = state.refresh_gate.try_lock() else {
        return Ok(view(&state));
    };
    {
        let mut data = state.data.lock().unwrap_or_else(|e| e.into_inner());
        data.refreshing = true;
        for snapshot in data.snapshots.values_mut() {
            snapshot.is_loading = true;
        }
    }
    publish(&app);
    // Keep provider identity in each response, regardless of completion order.
    let results = futures::future::join_all(Provider::ALL.into_iter().map(|provider| {
        let client = state.client.clone();
        let demo = state.demo;
        async move {
            let result = if demo {
                Ok(demo_windows(provider))
            } else {
                client::fetch(&client, provider).await
            };
            (provider, result)
        }
    }))
    .await;
    {
        let mut data = state.data.lock().unwrap_or_else(|e| e.into_inner());
        for (provider, result) in results {
            data.snapshots.get_mut(&provider).unwrap().apply(result);
        }
        data.refreshing = false;
    }
    Ok(publish(&app))
}
#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<AppView, String> {
    let state = app.state::<AppState>();
    settings.validate()?;
    // Serialize settings writes and publish only after the disk write succeeds.
    {
        let mut data = state.data.lock().unwrap_or_else(|e| e.into_inner());
        settings.save(&state.settings_path)?;
        data.settings = settings.clone();
        data.settings_error = None;
    }
    state.schedule.send_replace(settings.interval);
    Ok(publish(&app))
}
#[tauri::command]
fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<AppView, String> {
    let manager = app.autolaunch();
    let result = if enabled {
        manager.enable()
    } else {
        manager.disable()
    };
    let status = manager.is_enabled();
    let error = result
        .err()
        .map(|_| "로그인 시 실행 설정에 실패했습니다.".to_owned())
        .or_else(|| {
            status
                .as_ref()
                .err()
                .map(|_| "자동 실행 설정 상태를 읽지 못했습니다.".to_owned())
        });
    {
        let state = app.state::<AppState>();
        let mut data = state.data.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(enabled) = status {
            data.launch_at_login = enabled;
        }
        data.settings_error = error.clone();
    }
    let value = publish(&app);
    if let Some(error) = error {
        Err(error)
    } else {
        Ok(value)
    }
}
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}
fn tray_image() -> tauri::image::Image<'static> {
    let mut rgba = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let pixel = (y * 32 + x) * 4;
            let bar = (6..11).contains(&x) && (17..26).contains(&y)
                || (14..19).contains(&x) && (11..26).contains(&y)
                || (22..27).contains(&x) && (6..26).contains(&y);
            rgba[pixel..pixel + 4].copy_from_slice(if bar {
                &[96, 165, 250, 255]
            } else {
                &[17, 24, 39, 255]
            });
        }
    }
    tauri::image::Image::new_owned(rgba, 32, 32)
}
fn main() {
    let demo = std::env::args().any(|a| a == "--demo");
    let background = std::env::args().any(|a| a == "--background");
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _| { if !args.iter().any(|a| a == "--background") { show(app); } }))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--background"])))
        .invoke_handler(tauri::generate_handler![get_state, refresh_usage, save_settings, set_launch_at_login, quit])
        .setup(move |app| {
            let path = app.path().app_local_data_dir()?.join("settings.json");
            let settings = Settings::load(&path);
            let status = app.autolaunch().is_enabled();
            let (schedule, mut interval) = tokio::sync::watch::channel(settings.interval);
            let client = client::client().map_err(std::io::Error::other)?;
            app.manage(AppState { data: Mutex::new(Data { snapshots: Provider::ALL.into_iter().map(|p| (p, Snapshot::default())).collect(), settings,
                refreshing: false, launch_at_login: status.as_ref().copied().unwrap_or(false),
                settings_error: status.err().map(|_| "자동 실행 설정을 읽지 못했습니다.".into()) }),
                client, settings_path: path, demo, refresh_gate: tokio::sync::Mutex::new(()), schedule });
            let open = MenuItem::with_id(app, "open", "사용량 보기", true, None::<&str>)?;
            let refresh = MenuItem::with_id(app, "refresh", "새로고침", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "quit", "종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &refresh, &exit])?;
            let tray_result = TrayIconBuilder::with_id("usage").icon(tray_image()).tooltip("AI Usage")
                .menu(&menu).show_menu_on_left_click(cfg!(target_os = "linux"))
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show(app), "quit" => app.exit(0),
                    "refresh" => { let app = app.clone(); tauri::async_runtime::spawn(async move { let _ = refresh_usage(app).await; }); },
                    _ => {},
                })
                .on_tray_icon_event(|tray, event| { if matches!(event, TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. }) { show(tray.app_handle()); } })
                .build(app);
            let tray_available = match tray_result {
                Ok(_) => true,
                Err(_) if cfg!(target_os = "linux") => false,
                Err(error) => return Err(error.into()),
            };
            if let Some(window) = app.get_webview_window("main") {
                let window_handle = window.clone();
                window.on_window_event(move |event| { if let tauri::WindowEvent::CloseRequested { api, .. } = event { api.prevent_close();
                    // Linux desktops may have no visible tray (e.g. GNOME without an extension).
                    // Keep a taskbar entry so the app is still reachable.
                    if cfg!(target_os = "linux") || !tray_available { let _ = window_handle.minimize(); }
                    else { let _ = window_handle.hide(); } } });
            }
            publish(app.handle());
            if !background || !tray_available || cfg!(target_os = "linux") { show(app.handle()); }
            let app = app.handle().clone();
            // Native timer continues refreshing even when the webview is hidden/throttled.
            tauri::async_runtime::spawn(async move {
                let _ = refresh_usage(app.clone()).await;
                loop {
                    let seconds = *interval.borrow_and_update();
                    if seconds == 0 { if interval.changed().await.is_err() { break; } continue; }
                    tokio::select! {
                        changed = interval.changed() => { if changed.is_err() { break; } }
                        _ = tokio::time::sleep(Duration::from_secs(seconds)) => { let _ = refresh_usage(app.clone()).await; }
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("AI Usage를 시작하지 못했습니다.");
}
