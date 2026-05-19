mod app_state;
mod browser_share_bridge;
pub mod error;
pub mod models;
pub mod storage;
pub mod sync;

use std::sync::Arc;
use std::path::PathBuf;

use app_state::AppState;
use models::{
    BootstrapPayload, ConversationBundleDetailsPayload, ConversationBundleTextEntryPayload,
    ImportConversationBundlePayload, ImportPathPayload, ImportTextPayload, IntentState, ItemPayload,
    RelayBlobPayload, RelayItemPayload,
};
use tauri::Manager;

pub use error::{AppError, AppResult};
pub use storage::Storage;
pub use sync::SyncManager;

pub const APP_NAME: &str = "dropply";

pub async fn init_core(app_name: &str) -> AppResult<(Storage, SyncManager)> {
    let storage = Storage::new(app_name).await?;
    let pairing = storage.pairing()?;
    let sync = SyncManager::new(pairing.device_id.clone(), pairing.pairing_token.clone());
    sync.bootstrap(storage.clone()).await?;
    Ok((storage, sync))
}

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
        .import_text_with_source(
            payload.text,
            payload.id,
            payload.source_kind.unwrap_or_default(),
        )
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
async fn import_relay_item(
    payload: RelayItemPayload,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ItemPayload, String> {
    let item = state
        .storage
        .import_relay_item(payload)
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
async fn import_conversation_bundle(
    payload: ImportConversationBundlePayload,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ItemPayload, String> {
    let item = state
        .storage
        .import_conversation_bundle(payload)
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
async fn inspect_conversation_bundle(
    item_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ConversationBundleDetailsPayload, String> {
    state
        .storage
        .inspect_conversation_bundle(&item_id)
        .await
        .map_err(stringify_error)
}

#[tauri::command]
async fn read_conversation_bundle_entry(
    item_id: String,
    entry_path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ConversationBundleTextEntryPayload, String> {
    state
        .storage
        .read_conversation_bundle_entry(&item_id, &entry_path)
        .await
        .map_err(stringify_error)
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
async fn export_item_to_downloads(
    item_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<String, String> {
    state
        .storage
        .export_item_to_downloads(&item_id)
        .await
        .map_err(stringify_error)
}

#[tauri::command]
async fn open_item(item_id: String, state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    state
        .storage
        .open_item(&item_id)
        .await
        .map_err(stringify_error)
}

#[tauri::command]
async fn export_relay_items(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<RelayItemPayload>, String> {
    state.storage.export_relay_items().await.map_err(stringify_error)
}

#[tauri::command]
async fn export_pair_manifest(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<RelayItemPayload>, String> {
    state.storage.export_pair_manifest().await.map_err(stringify_error)
}

#[tauri::command]
async fn export_relay_blob(
    item_id: String,
    chunk_bytes: usize,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<RelayBlobPayload, String> {
    state
        .storage
        .export_relay_blob(&item_id, chunk_bytes)
        .await
        .map_err(stringify_error)
}

#[tauri::command]
async fn import_staged_transfer(
    payload: RelayItemPayload,
    staged_path: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ItemPayload, String> {
    let item = state
        .storage
        .import_staged_relay_item(payload, &PathBuf::from(staged_path))
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
async fn set_pairing_token(
    token: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.storage.update_pairing_token(token.clone()).map_err(stringify_error)?;
    state.sync.update_pairing_token(token).await;
    Ok(())
}

#[tauri::command]
async fn reset_pairing_token(state: tauri::State<'_, Arc<AppState>>) -> Result<String, String> {
    let new_token = state.storage.reset_pairing_token().map_err(stringify_error)?;
    state.sync.update_pairing_token(new_token.clone()).await;
    Ok(new_token)
}

#[tauri::command]
async fn unpair_device(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    state.storage.clear_pairing().map_err(stringify_error)?;
    app.restart();
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

#[tauri::command]
fn start_window_drag(window: tauri::Window) -> Result<(), String> {
    window.start_dragging().map_err(stringify_error)
}

#[tauri::command]
fn minimize_window(window: tauri::Window) -> Result<(), String> {
    window.minimize().map_err(stringify_error)
}

#[tauri::command]
fn toggle_maximize_window(window: tauri::Window) -> Result<bool, String> {
    let is_maximized = window.is_maximized().map_err(stringify_error)?;

    if is_maximized {
        window.unmaximize().map_err(stringify_error)?;
        Ok(false)
    } else {
        window.maximize().map_err(stringify_error)?;
        Ok(true)
    }
}

#[tauri::command]
fn close_window(app: tauri::AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn update_item_intent_state(
    item_id: String,
    intent_state: IntentState,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<ItemPayload>, String> {
    let item = state
        .storage
        .update_item_intent_state(&item_id, intent_state)
        .await
        .map_err(stringify_error)?;
    state
        .sync
        .note_local_change(state.storage.clone())
        .await
        .map_err(stringify_error)?;
    Ok(item)
}

fn stringify_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            let handle = app.handle().clone();

            let state = tauri::async_runtime::block_on(async move {
                let (storage, sync) = init_core(APP_NAME).await.map_err(to_boxed_error)?;

                let asset_scope = handle.asset_protocol_scope();
                asset_scope
                    .allow_directory(storage.base_dir(), true)
                    .map_err(to_boxed_error)?;

                Ok::<Arc<AppState>, Box<dyn std::error::Error>>(Arc::new(AppState { storage, sync }))
            })?;

            let bridge_state = state.clone();
            app.manage(state);
            browser_share_bridge::start(bridge_state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            list_items,
            import_text,
            import_relay_item,
            import_paths,
            import_conversation_bundle,
            inspect_conversation_bundle,
            read_conversation_bundle_entry,
            copy_item_text,
            delete_item,
            update_item_intent_state,
            export_item,
            export_item_to_downloads,
            open_item,
            export_relay_items,
            export_pair_manifest,
            export_relay_blob,
            import_staged_transfer,
            set_pairing_token,
            reset_pairing_token,
            unpair_device,
            get_window_pin_state,
            set_window_pin_state,
            open_external_url,
            start_window_drag,
            minimize_window,
            toggle_maximize_window,
            close_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running Dropply");
}

fn to_boxed_error(error: impl std::error::Error + 'static) -> Box<dyn std::error::Error> {
    Box::new(error)
}
