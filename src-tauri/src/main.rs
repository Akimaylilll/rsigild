#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod config;
mod daemon;
mod logger;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::Mutex;

pub struct AppState {
    daemon: Arc<Mutex<daemon::DaemonManager>>,
}

#[tauri::command]
async fn get_processes(state: tauri::State<'_, AppState>) -> Result<Vec<config::ProcessConfig>, String> {
    let daemon = state.daemon.lock().await;
    Ok(daemon.get_processes())
}

#[tauri::command]
async fn add_process(state: tauri::State<'_, AppState>, config: config::ProcessConfig) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    daemon.add_process(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn remove_process(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    daemon.remove_process(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_process(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    daemon.start_process(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn stop_process(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    daemon.stop_process(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_process_status(state: tauri::State<'_, AppState>, id: String) -> Result<daemon::ProcessStatus, String> {
    let mut daemon = state.daemon.lock().await;
    daemon.get_process_status(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_logs(state: tauri::State<'_, AppState>, id: String, lines: Option<usize>) -> Result<String, String> {
    let daemon = state.daemon.lock().await;
    daemon.get_logs(&id, lines.unwrap_or(100)).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_process(state: tauri::State<'_, AppState>, config: config::ProcessConfig) -> Result<(), String> {
    let mut daemon = state.daemon.lock().await;
    daemon.update_process(config).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn check_health_url(url: String) -> Result<bool, String> {
    let client = reqwest::Client::new();
    let result = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    match result {
        Ok(resp) => Ok(resp.status().is_success()),
        Err(_) => Ok(false),
    }
}

fn main() {
    logger::init_logger().expect("Failed to initialize logger");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|app| {
            let app_handle = app.handle();
            let rt = tokio::runtime::Runtime::new().expect("Failed to create runtime");
            
            let daemon = rt.block_on(async {
                daemon::DaemonManager::new(app_handle.clone()).await.expect("Failed to create daemon manager")
            });
            
            let state = AppState {
                daemon: Arc::new(Mutex::new(daemon)),
            };
            
            app.manage(state);
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_processes,
            add_process,
            remove_process,
            start_process,
            stop_process,
            get_process_status,
            get_logs,
            update_process,
            check_health_url,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            if let tauri::RunEvent::ExitRequested { .. } = &event {
                log::info!("Application exit requested, shutting down processes...");
                
                let state = app_handle.state::<AppState>();
                let daemon = state.daemon.clone();
                
                // Block until processes are shut down
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    let mut daemon = daemon.lock().await;
                    daemon.shutdown_all().await;
                    log::info!("All processes shut down");
                });
            }
            
            if let tauri::RunEvent::Exit = &event {
                log::info!("Application exiting");
            }
        });
}
