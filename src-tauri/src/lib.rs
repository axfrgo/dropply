mod app_state;
mod error;
mod models;
mod storage;
mod sync;

use std::sync::Arc;

use app_state::AppState;
use models::{BootstrapPayload, ImportPathPayload, ImportTextPayload, ItemPayload};
use storage::Storage;
use sync::SyncManager;
use tauri::Manager;

#[tauri::command]
async fn bootstrap_app(state: tauri::State<'_, Arc<AppState>>) -> Result<BootstrapPayload, String> {
    let items = state.storage.list_items().await.map_err(stringify_error)?;
    let sync_status = state.sync.status().await;

    Ok(BootstrapPayload { items, sync_status })
}

#[tauri::command]
async fn list_items(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<ItemPayload>, String> {
    state.storage.list_items().await.map_err(stringify_error)
}

#[tauri::command]
async fn import_text(
    payload: ImportTextPayload,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ItemPayload, String> {
    let item = state
        .storage
        .import_text(payload.text)
        .await
        .map_err(stringify_error)?;
    state
        .sync
        .note_local_change(state.storage.clone())
        .await
        .map_err(stringify_error)?;
    Ok(item)
}

#[tauri::command]
async fn import_paths(
    payload: ImportPathPayload,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Vec<ItemPayload>, String> {
    let items = state
        .storage
        .import_paths(payload)
        .await
        .map_err(stringify_error)?;
    state
        .sync
        .note_local_change(state.storage.clone())
        .await
        .map_err(stringify_error)?;
    Ok(items)
}

#[tauri::command]
async fn copy_item_text(item_id: String, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let text = state
        .storage
        .item_text(&item_id)
        .await
        .map_err(stringify_error)?;
    let Some(text) = text else {
        return Ok(());
    };

    let clipboard = arboard::Clipboard::new().map_err(|err| err.to_string())?;
    let mut clipboard = clipboard;
    clipboard.set_text(text).map_err(|err| err.to_string())
}

#[tauri::command]
async fn delete_item(item_id: String, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .storage
        .delete_item(&item_id)
        .await
        .map_err(stringify_error)?;
    state
        .sync
        .note_local_change(state.storage.clone())
        .await
        .map_err(stringify_error)?;
    Ok(())
}

#[tauri::command]
async fn export_item(
    item_id: String,
    destination_path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state
        .storage
        .export_item(&item_id, &destination_path)
        .await
        .map_err(stringify_error)
}

#[tauri::command]
fn get_window_pin_state(window: tauri::Window) -> Result<bool, String> {
    window.is_always_on_top().map_err(stringify_error)
}

#[tauri::command]
fn set_window_pin_state(window: tauri::Window, pinned: bool) -> Result<bool, String> {
    window.set_always_on_top(pinned).map_err(stringify_error)?;
    Ok(pinned)
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    webbrowser::open(&url).map(|_| ()).map_err(stringify_error)
}

fn stringify_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let handle = app.handle();

            let state = tauri::async_runtime::block_on(async move {
                let storage = Storage::new("dropply").await.map_err(to_boxed_error)?;
                let pairing = storage.pairing();
                let sync = SyncManager::new(pairing.device_id.clone(), pairing.pairing_token.clone());
                sync.bootstrap(storage.clone()).await.map_err(to_boxed_error)?;

                let asset_scope = handle.asset_protocol_scope();
                asset_scope
                    .allow_directory(storage.base_dir(), true)
                    .map_err(to_boxed_error)?;

                Ok::<Arc<AppState>, Box<dyn std::error::Error>>(Arc::new(AppState { storage, sync }))
            })?;

            app.manage(state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            list_items,
            import_text,
            import_paths,
            copy_item_text,
            delete_item,
            export_item,
            get_window_pin_state,
            set_window_pin_state,
            open_external_url
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dropply");
}

fn to_boxed_error(error: impl std::error::Error + 'static) -> Box<dyn std::error::Error> {
    Box::new(error)
}
