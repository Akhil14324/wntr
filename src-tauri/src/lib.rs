mod calendar;
mod clipboard;
mod data;
mod db;
mod drag;
mod lyrics;
mod media;
mod notch;
mod notify;
mod scheduler;
mod settings;
mod shelf;
mod status;
mod util;
mod weather;

use serde_json::{Value, json};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

use util::{CmdResult, err, now_ms};

// ---------------------------------------------------------------------------
// Settings side effects
// ---------------------------------------------------------------------------

fn register_hotkey(app: &AppHandle) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let key = settings::get_str("general.hotkey");
    if !key.trim().is_empty()
        && let Err(e) = gs.register(key.as_str())
    {
        notify::emit(
            "error",
            json!({ "message": format!("Couldn't register hotkey {key}: {e}") }),
        );
    }
}

fn apply_autostart(app: &AppHandle) {
    // Dev builds would register the debug binary; only real builds touch the registry.
    if cfg!(debug_assertions) {
        return;
    }
    let al = app.autolaunch();
    let want = settings::get_bool("general.autostart", true);
    let is = al.is_enabled().unwrap_or(false);
    if want && !is {
        let _ = al.enable();
    } else if !want && is {
        let _ = al.disable();
    }
}

pub fn apply_settings(app: &AppHandle, old: &Value, new: &Value) {
    if old["general"]["hotkey"] != new["general"]["hotkey"] {
        register_hotkey(app);
    }
    if old["general"]["autostart"] != new["general"]["autostart"] {
        apply_autostart(app);
    }
    if old["general"]["monitor"] != new["general"]["monitor"] {
        notch::place(app);
    }
    if old["dnd"] != new["dnd"] {
        rebuild_tray_menu(app);
    }
}

// ---------------------------------------------------------------------------
// Windows + tray
// ---------------------------------------------------------------------------

// Async on purpose: on Windows, building a webview window from a sync command
// blocks the event loop, which leaves the new settings window blank and unresponsive.
#[tauri::command]
async fn open_settings(app: AppHandle, section: Option<String>) -> CmdResult<()> {
    show_settings(&app, section)
}

/// Fire-and-forget version for menu handlers, which run on the main thread.
fn spawn_settings(app: &AppHandle, section: Option<String>) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let _ = show_settings(&app, section);
    });
}

fn show_settings(app: &AppHandle, section: Option<String>) -> CmdResult<()> {
    let section = section.unwrap_or_else(|| "general".into());
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        let _ = w.emit("settings-section", section);
        return Ok(());
    }
    WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App(format!("index.html#settings/{section}").into()),
    )
    .title("WNTR Settings")
    .inner_size(960.0, 680.0)
    .min_inner_size(760.0, 520.0)
    .center()
    .build()
    .map_err(err)?;
    Ok(())
}

fn set_dnd(app: &AppHandle, minutes: Option<i64>) {
    let until = match minutes {
        None => 0,
        Some(-1) => {
            // Until tomorrow 07:00.
            let tomorrow = chrono::Local::now().date_naive() + chrono::Duration::days(1);
            tomorrow
                .and_hms_opt(7, 0, 0)
                .and_then(|d| d.and_local_timezone(chrono::Local).earliest())
                .map(|d| d.timestamp_millis())
                .unwrap_or(0)
        }
        Some(m) => now_ms() + m * 60_000,
    };
    settings::patch(app, &json!({ "dnd": { "until": until } }));
}

#[tauri::command]
fn dnd_set(app: AppHandle, minutes: Option<i64>) {
    set_dnd(&app, minutes);
}

fn tray_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let dnd_on = notify::dnd_active();
    let open = MenuItem::with_id(app, "open", "Open notch", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let dnd = Submenu::with_items(
        app,
        if dnd_on {
            "Do Not Disturb (on)"
        } else {
            "Do Not Disturb"
        },
        true,
        &[
            &MenuItem::with_id(app, "dnd-30", "For 30 minutes", true, None::<&str>)?,
            &MenuItem::with_id(app, "dnd-60", "For 1 hour", true, None::<&str>)?,
            &MenuItem::with_id(app, "dnd-tomorrow", "Until tomorrow", true, None::<&str>)?,
            &MenuItem::with_id(app, "dnd-off", "Turn off", dnd_on, None::<&str>)?,
        ],
    )?;
    let about = MenuItem::with_id(app, "about", "About WNTR", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Exit", true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &open,
            &settings_item,
            &dnd,
            &PredefinedMenuItem::separator(app)?,
            &about,
            &quit,
        ],
    )
}

fn rebuild_tray_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id("wntr"), tray_menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn toggle_notch(app: &AppHandle) {
    if let Some(w) = notch::window(app) {
        let _ = w.show();
    }
    notify::emit("notch-toggle", json!({}));
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let menu = tray_menu(app)?;
    let mut builder = TrayIconBuilder::with_id("wntr")
        .tooltip("WNTR")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => toggle_notch(app),
            "settings" => spawn_settings(app, None),
            "about" => spawn_settings(app, Some("about".into())),
            "dnd-30" => set_dnd(app, Some(30)),
            "dnd-60" => set_dnd(app, Some(60)),
            "dnd-tomorrow" => set_dnd(app, Some(-1)),
            "dnd-off" => set_dnd(app, None),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_notch(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Updater
// ---------------------------------------------------------------------------

#[tauri::command]
async fn update_check(app: AppHandle) -> CmdResult<Value> {
    let updater = app.updater().map_err(err)?;
    match updater.check().await.map_err(err)? {
        Some(u) => {
            Ok(json!({ "available": true, "version": u.version, "notes": u.body, "current": u.current_version }))
        }
        None => Ok(json!({ "available": false, "current": app.package_info().version.to_string() })),
    }
}

#[tauri::command]
async fn update_install(app: AppHandle) -> CmdResult<()> {
    let updater = app.updater().map_err(err)?;
    let update = updater.check().await.map_err(err)?.ok_or("Already up to date")?;
    update.download_and_install(|_, _| {}, || {}).await.map_err(err)?;
    app.restart();
}

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    app.exit(0);
}

// ---------------------------------------------------------------------------

/// Re-place the notch when monitors are added, removed or resized.
fn watch_displays(app: AppHandle) {
    util::spawn_loop("displays", move || {
        let mut last = String::new();
        loop {
            if let Some(w) = notch::window(&app) {
                let sig: String = w
                    .available_monitors()
                    .unwrap_or_default()
                    .iter()
                    .map(|m| format!("{:?}{:?}{:?}{}", m.name(), m.position(), m.size(), m.scale_factor()))
                    .collect();
                if sig != last {
                    if !last.is_empty() {
                        notch::place(&app);
                    }
                    last = sig;
                }
            }
            util::sleep_ms(5000);
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toggle_notch(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_notch(app);
                        if let Some(w) = notch::window(app) {
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(),
        )
        .setup(|app| {
            let handle = app.handle().clone();
            notify::set_app(handle.clone());
            let dir = util::data_dir();
            db::init(&dir)?;
            settings::load();
            notify::register_aumid(&app.config().identifier);

            notch::place(&handle);
            if let Some(win) = notch::window(&handle) {
                let w2 = win.clone();
                win.on_window_event(move |e| {
                    if matches!(e, WindowEvent::Moved(_) | WindowEvent::ScaleFactorChanged { .. }) {
                        notch::on_moved(&w2);
                    }
                });
            }
            drag::install_drop_target(&handle);
            // WebView2 creates some child windows lazily; claim them too.
            let h2 = handle.clone();
            std::thread::spawn(move || {
                for delay in [1500, 5000] {
                    util::sleep_ms(delay);
                    drag::install_drop_target(&h2);
                }
            });

            notch::start(handle.clone());
            media::start();
            status::start();
            clipboard::start();
            scheduler::start();
            calendar::start();
            watch_displays(handle.clone());

            build_tray(&handle)?;
            register_hotkey(&handle);
            apply_autostart(&handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            open_settings,
            dnd_set,
            update_check,
            update_install,
            app_version,
            app_quit,
            settings::settings_get,
            settings::settings_set,
            settings::settings_reset,
            notch::notch_set_hit,
            notch::notch_focus,
            notch::notch_info,
            media::media_control,
            media::media_select,
            media::media_state,
            lyrics::lyrics_get,
            lyrics::lyrics_search,
            lyrics::lyrics_choose,
            lyrics::lyrics_forget,
            shelf::shelf_list,
            shelf::shelf_add_paths,
            shelf::shelf_add_clipboard,
            shelf::shelf_remove,
            shelf::shelf_clear,
            shelf::shelf_locate,
            shelf::shelf_drag_out,
            shelf::shelf_copy,
            shelf::shelf_copy_path,
            shelf::shelf_open,
            shelf::shelf_reveal,
            shelf::shelf_thumb,
            clipboard::clipboard_list,
            clipboard::clipboard_copy,
            clipboard::clipboard_paste,
            clipboard::clipboard_pin,
            clipboard::clipboard_delete,
            clipboard::clipboard_clear,
            status::status_snapshot,
            status::volume_set,
            status::volume_toggle_mute,
            scheduler::focus_state,
            scheduler::focus_command,
            scheduler::timers_list,
            scheduler::timer_add,
            scheduler::timer_command,
            data::tasks_list,
            data::task_save,
            data::task_delete,
            data::task_complete,
            data::task_undo_complete,
            data::task_snooze,
            data::water_summary,
            data::water_add,
            data::water_undo,
            data::notes_list,
            data::note_add,
            data::note_update,
            data::note_pin,
            data::note_delete,
            data::notes_export,
            data::habits_list,
            data::habit_save,
            data::habit_delete,
            data::habit_toggle,
            data::launcher_list,
            data::launcher_add,
            data::launcher_remove,
            data::launcher_move,
            data::open_target,
            data::stats_get,
            data::stats_export_csv,
            data::data_export,
            data::data_import,
            data::data_reset,
            data::open_data_folder,
            calendar::calendar_events,
            calendar::calendar_list,
            calendar::calendar_add,
            calendar::calendar_remove,
            calendar::calendar_toggle,
            calendar::calendar_refresh,
            weather::weather_get,
            weather::weather_set_city,
        ])
        .build(tauri::generate_context!())
        .expect("error while building WNTR")
        .run(|_app, event| {
            // Closing the settings window must not quit the app; the notch lives on.
            if let tauri::RunEvent::ExitRequested { code: None, api, .. } = event {
                api.prevent_exit();
            }
        });
}
