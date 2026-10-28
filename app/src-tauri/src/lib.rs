mod browse;
mod commands;
mod export;
mod indexer;
mod state;
mod thumbs;
#[cfg(test)]
mod testutil;

use std::sync::Arc;

use tauri::http::{Response, StatusCode, header};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // cache files by content-hashed name, so the webview may keep them forever
        .register_asynchronous_uri_scheme_protocol("thumb", |ctx, request, responder| {
            let dir = ctx.app_handle().try_state::<state::AppState>().and_then(|s| s.thumbs.get().map(|t| t.dir().to_path_buf()));
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let response = match dir.and_then(|d| thumbs::serve(&d, &path)) {
                    Some(bytes) => Response::builder()
                        .header(header::CONTENT_TYPE, "image/jpeg")
                        .header(header::CACHE_CONTROL, "max-age=31536000, immutable")
                        .body(bytes),
                    None => Response::builder().status(StatusCode::NOT_FOUND).body(Vec::new()),
                };
                responder.respond(response.expect("a valid response"));
            });
        })
        .setup(|app| {
            match state::AppState::open(stl_index::paths::app_dir()) {
                Ok(s) => {
                    let handle = app.handle().clone();
                    let notify: thumbs::Notify = Arc::new(move |r| {
                        let _ = handle.emit("thumb-ready", r);
                    });
                    let _ = s.thumbs.set(thumbs::ThumbService::new(&s.app_dir, notify));
                    app.manage(s);
                }
                Err(e) => {
                    app.dialog()
                        .message(format!("Cannot open the index database:\n{e}"))
                        .kind(MessageDialogKind::Error)
                        .title("STL Browser")
                        .blocking_show();
                    std::process::exit(1);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_folder,
            commands::status,
            commands::rescan,
            commands::add_folder,
            commands::select_thumb,
            commands::request_thumbs,
            export::begin_drag,
            export::extract_to
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
