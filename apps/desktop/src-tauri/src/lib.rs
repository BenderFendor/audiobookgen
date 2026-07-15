mod commands;
mod platform;

use commands::AppRuntime;
use tauri::{menu::{Menu, MenuItem}, tray::TrayIconBuilder, Manager};

pub fn run() {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let resource_dir = app.path().resource_dir()?;
            app.manage(AppRuntime::new(data_dir, resource_dir)?);

            let show = MenuItem::with_id(app, "show", "Show AudiobookGen", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => { if let Some(window) = app.get_webview_window("main") { let _ = window.show(); let _ = window.set_focus(); } },
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() { tray = tray.icon(icon.clone()); }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::inspect_epub_file,
            commands::import_epub,
            commands::list_books,
            commands::get_book,
            commands::get_chapter_fragments,
            commands::create_narration_profile,
            commands::set_active_profile,
            commands::save_pronunciation_rule,
            commands::save_progress,
            commands::load_progress,
            commands::model_status,
            commands::download_model,
            commands::preview_voice,
            commands::queue_generation,
            commands::cancel_generation,
            commands::get_generated_audio,
            commands::export_m4a,
            commands::export_m4b_file,
            commands::export_narrated_epub_file,
            commands::sync_to_folder,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run AudiobookGen");
}
