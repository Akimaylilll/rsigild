use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::config::{self, ProcessConfig};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
#[cfg(target_os = "windows")]
use winapi::um::processthreadsapi::{OpenProcess, GetExitCodeProcess};
#[cfg(target_os = "windows")]
use winapi::um::handleapi::CloseHandle;
#[cfg(target_os = "windows")]
use winapi::um::winnt::PROCESS_QUERY_INFORMATION;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;

#[cfg(target_os = "windows")]
extern "system" {
    fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
    fn AttachConsole(dwProcessId: u32) -> i32;
    fn FreeConsole() -> i32;
    fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> i32>, add: i32) -> i32;
}

#[cfg(target_os = "windows")]
const CTRL_BREAK_EVENT: u32 = 1;

#[cfg(target_os = "windows")]
pub fn init_hidden_console() {
    // No-op
}

#[cfg(target_os = "windows")]
fn graceful_stop_process(pid: u32) {
    unsafe {
        // Detach from our console (if any)
        FreeConsole();
        
        // Attach to the child's hidden console
        if AttachConsole(pid) != 0 {
            // Ignore the signal ourselves
            SetConsoleCtrlHandler(None, 1);
            
            // Send CTRL_BREAK_EVENT to the child's specific process group
            // When using CREATE_NEW_PROCESS_GROUP, the PGID equals the PID
            let result = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, pid);
            
            // Detach immediately
            FreeConsole();
            
            if result != 0 {
                log::info!("Sent CTRL_BREAK_EVENT to process group {}", pid);
            } else {
                log::warn!("GenerateConsoleCtrlEvent failed for PID {}", pid);
            }
        } else {
            log::warn!("AttachConsole failed for PID {}, falling back to taskkill", pid);
            // Fallback: taskkill without /F sends graceful shutdown
            let _ = Command::new("taskkill")
                .args(["/T", "/PID", &pid.to_string()])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output();
        }
    }
}

#[cfg(target_os = "windows")]
fn is_process_running(pid: u32) -> bool {
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid) };
    if handle.is_null() { return false; }
    let mut exit_code = 0u32;
    let success = unsafe { GetExitCodeProcess(handle, &mut exit_code) };
    unsafe { CloseHandle(handle) };
    success != 0 && exit_code == 259
}

#[cfg(not(target_os = "windows"))]
fn is_process_running(pid: u32) -> bool {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match output {
        Ok(status) => status.success(),
        Err(_) => false,
    }
}

#[cfg(target_os = "windows")]
fn force_stop_process(pid: u32) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &pid.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output();
}

#[cfg(target_os = "windows")]
mod job_object {
    use win32job::Job;
    extern "system" {
        fn OpenProcess(dwDesiredAccess: u32, bInheritHandle: i32, dwProcessId: u32) -> isize;
        fn TerminateJobObject(hJob: isize, uExitCode: u32) -> i32;
    }
    pub fn create_job() -> Option<Job> {
        match Job::create() {
            Ok(job) => {
                match job.query_extended_limit_info() {
                    Ok(mut info) => {
                        info.limit_kill_on_job_close();
                        if let Err(e) = job.set_extended_limit_info(&info) {
                            log::error!("Failed to set job info: {}", e);
                            return None;
                        }
                        Some(job)
                    }
                    Err(e) => { log::error!("Failed to query job info: {}", e); None }
                }
            }
            Err(e) => { log::error!("Failed to create job object: {}", e); None }
        }
    }
    pub fn assign_process(job: &Job, pid: u32) -> bool {
        let handle = unsafe { OpenProcess(0x0101, 0, pid) };
        if handle == 0 { log::error!("Failed to open process {}", pid); return false; }
        match job.assign_process(handle) {
            Ok(_) => { log::info!("Process {} assigned to job object", pid); true }
            Err(e) => { log::error!("Failed to assign process {} to job: {}", pid, e); false }
        }
    }
    pub fn terminate_job(job: &Job) -> bool {
        let handle = job.handle();
        let result = unsafe { TerminateJobObject(handle, 1) };
        if result == 0 { log::error!("Failed to terminate job object"); false }
        else { log::info!("Job object terminated"); true }
    }
}

#[cfg(not(target_os = "windows"))]
mod job_object {
    pub fn create_job() -> Option<()> { Some(()) }
    pub fn assign_process(_job: &(), _pid: u32) -> bool { true }
    pub fn terminate_job(_job: &()) -> bool { true }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessStatus {
    pub id: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub last_health_check: Option<DateTime<Utc>>,
    pub health_check_ok: Option<bool>,
    pub restart_count: u32,
    pub last_restart: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

pub struct RunningProcess {
    pub child: Arc<Mutex<Option<Child>>>,
    pub config: ProcessConfig,
    pub health_check_task: Option<JoinHandle<()>>,
    pub status: Arc<Mutex<ProcessStatus>>,
    #[cfg(target_os = "windows")]
    pub job: Option<win32job::Job>,
}

pub struct DaemonManager {
    app_handle: AppHandle,
    config: config::AppConfig,
    running_processes: HashMap<String, RunningProcess>,
}

impl DaemonManager {
    pub async fn new(app_handle: AppHandle) -> anyhow::Result<Self> {
        let config = config::load_config()?;
        if let Ok(path) = config::get_config_path() {
            log::info!("Loaded config from {:?}", path);
        }
        let manager = Self { app_handle, config, running_processes: HashMap::new() };
        log::info!("DaemonManager initialized with {} processes", manager.config.processes.len());
        Ok(manager)
    }

    pub fn get_processes(&self) -> Vec<ProcessConfig> { self.config.processes.clone() }

    pub async fn add_process(&mut self, config: ProcessConfig) -> anyhow::Result<()> {
        if self.config.processes.iter().any(|p| p.id == config.id) {
            anyhow::bail!("Process with id {} already exists", config.id);
        }
        if config.enabled { self.start_process_internal(&config).await?; }
        self.config.processes.push(config);
        config::save_config(&self.config)?;
        Ok(())
    }

    pub async fn remove_process(&mut self, id: &str) -> anyhow::Result<()> {
        self.stop_process(id).await?;
        self.config.processes.retain(|p| p.id != id);
        config::save_config(&self.config)?;
        Ok(())
    }

    pub async fn update_process(&mut self, new_config: ProcessConfig) -> anyhow::Result<()> {
        let idx = self.config.processes.iter().position(|p| p.id == new_config.id)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;
        let old_config = &self.config.processes[idx];
        if old_config.enabled && !new_config.enabled {
            self.stop_process(&new_config.id).await?;
        } else if new_config.enabled && (!old_config.enabled
            || old_config.command != new_config.command
            || old_config.args != new_config.args
            || old_config.working_dir != new_config.working_dir)
        {
            self.stop_process(&new_config.id).await?;
            if new_config.enabled { self.start_process_internal(&new_config).await?; }
        }
        self.config.processes[idx] = new_config;
        config::save_config(&self.config)?;
        Ok(())
    }

    pub async fn start_process(&mut self, id: &str) -> anyhow::Result<()> {
        let process_config = self.config.processes.iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?
            .clone();
        self.start_process_internal(&process_config).await
    }

    async fn start_process_internal(&mut self, config: &ProcessConfig) -> anyhow::Result<()> {
        if self.running_processes.contains_key(&config.id) {
            self.stop_process_internal(&config.id).await?;
        }

        let log_path = if config.log_path.is_empty() {
            let log_dir = config::get_config_dir()?.join("logs");
            std::fs::create_dir_all(&log_dir)?;
            log_dir.join(format!("{}.log", config.id))
        } else {
            std::path::PathBuf::from(&config.log_path)
        };
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let log_file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;
        let log_file_clone = log_file.try_clone()?;

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);

        #[cfg(target_os = "windows")]
        {
            // CREATE_NO_WINDOW hides the console
            // CREATE_NEW_PROCESS_GROUP allows taskkill to target this process group
            cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP);
        }

        if let Some(working_dir) = &config.working_dir {
            cmd.current_dir(working_dir);
        }
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        cmd.stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_clone));

        let child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start process {}: {}", config.name, e))?;
        let pid = child.id();
        log::info!("Started process {} with PID {}", config.name, pid);

        let job = job_object::create_job();
        if let Some(ref j) = job {
            job_object::assign_process(j, pid);
        }
        if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == config.id) {
            p.last_pid = Some(pid);
            p.updated_at = Utc::now();
            log::info!("Updated config for process {}, last_pid={}", p.name, pid);
        }
        if let Ok(path) = config::get_config_path() {
            if config::save_config(&self.config).is_err() {
                log::error!("Failed to save config to {:?}", path);
            }
        }

        let status = ProcessStatus {
            id: config.id.clone(), running: true, pid: Some(pid),
            last_health_check: None, health_check_ok: None,
            restart_count: 0, last_restart: None, last_error: None,
        };

        let child_arc = Arc::new(Mutex::new(Some(child)));
        let status_arc = Arc::new(Mutex::new(status));
        let config_clone = config.clone();

        let health_check_task = if let Some(health_url) = &config.health_check_url {
            let id = config.id.clone();
            let url = health_url.clone();
            let interval = config.health_check_interval_secs;
            let child_clone = Arc::clone(&child_arc);
            let status_clone = Arc::clone(&status_arc);
            let auto_restart = config.auto_restart;
            let cmd_str = config.command.clone();
            let args_vec = config.args.clone();
            let working_dir = config.working_dir.clone();

            Some(tokio::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                    let client = reqwest::Client::new();
                    let result = client.get(&url).timeout(std::time::Duration::from_secs(10)).send().await;
                    let mut status = status_clone.lock().await;
                    status.last_health_check = Some(Utc::now());
                    let is_ok = match result {
                        Ok(resp) => resp.status().is_success(),
                        Err(e) => { log::warn!("Health check failed for {}: {}", id, e); false }
                    };
                    status.health_check_ok = Some(is_ok);
                    if !is_ok && auto_restart {
                        log::warn!("Process {} failed health check, restarting...", id);
                        status.restart_count += 1;
                        status.last_restart = Some(Utc::now());
                        let mut child_guard = child_clone.lock().await;
                        if let Some(mut old_child) = child_guard.take() {
                            let _ = old_child.kill();
                        }
                        let mut cmd = Command::new(&cmd_str);
                        cmd.args(&args_vec);
                        if let Some(ref wd) = working_dir { cmd.current_dir(wd); }
                        #[cfg(target_os = "windows")]
                        { cmd.creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP); }
                        cmd.stdout(Stdio::null()).stderr(Stdio::null());
                        match cmd.spawn() {
                            Ok(new_child) => {
                                let new_pid = new_child.id();
                                status.pid = Some(new_pid);
                                status.running = true;
                                *child_guard = Some(new_child);
                                #[cfg(target_os = "windows")]
                                {
                                    if let Some(new_job) = job_object::create_job() {
                                        job_object::assign_process(&new_job, new_pid);
                                    }
                                }
                                log::info!("Restarted process {} with new PID {}", id, new_pid);
                            }
                            Err(e) => {
                                status.running = false;
                                status.pid = None;
                                log::error!("Failed to restart process {}: {}", id, e);
                            }
                        }
                    }
                }
            }))
        } else {
            None
        };

        let running_process = RunningProcess {
            child: child_arc, config: config_clone, health_check_task, status: status_arc,
            #[cfg(target_os = "windows")]
            job,
        };
        self.running_processes.insert(config.id.clone(), running_process);
        Ok(())
    }

    pub async fn stop_process(&mut self, id: &str) -> anyhow::Result<()> {
        self.stop_process_internal(id).await
    }

    pub async fn shutdown_all(&mut self) {
        log::info!("Shutting down all processes...");
        let pids: Vec<u32> = self.running_processes.values()
            .filter_map(|rp| { if let Ok(guard) = rp.child.try_lock() { guard.as_ref().map(|c| c.id()) } else { None } })
            .collect();

        #[cfg(target_os = "windows")]
        {
            for pid in &pids {
                log::info!("Sending graceful shutdown to PID {}", pid);
                graceful_stop_process(*pid);
            }
            for pid in &pids {
                for _ in 0..50 {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if !is_process_running(*pid) { log::info!("Process {} exited gracefully", pid); break; }
                }
                if is_process_running(*pid) {
                    log::warn!("Process {} did not exit, force killing", pid);
                    force_stop_process(*pid);
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            for pid in &pids { let _ = Command::new("kill").args(["-TERM", &pid.to_string()]).output(); }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            for pid in &pids { if is_process_running(*pid) { let _ = Command::new("kill").args(["-9", &pid.to_string()]).output(); } }
        }
        log::info!("Shutdown complete");
    }

    async fn stop_process_internal(&mut self, id: &str) -> anyhow::Result<()> {
        if let Some(running_process) = self.running_processes.remove(id) {
            if let Some(task) = running_process.health_check_task { task.abort(); }
            let mut child_guard = running_process.child.lock().await;
            if let Some(child) = child_guard.take() {
                let pid = child.id();
                log::info!("Stopping process {} with PID {}", id, pid);

                #[cfg(target_os = "windows")]
                {
                    graceful_stop_process(pid);
                    for _ in 0..50 {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        if !is_process_running(pid) { log::info!("Process {} exited gracefully", pid); break; }
                    }
                    if is_process_running(pid) {
                        log::warn!("Process {} did not exit, force killing", pid);
                        force_stop_process(pid);
                    }
                }
                #[cfg(not(target_os = "windows"))]
                { let _ = child.kill(); }
                log::info!("Process {} stopped", id);
            }
            let mut status = running_process.status.lock().await;
            status.running = false; status.pid = None;
            if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == id) {
                p.last_pid = None; p.updated_at = Utc::now();
            }
            let _ = config::save_config(&self.config);
        } else {
            log::warn!("Process {} not found in running processes", id);
        }
        Ok(())
    }

    pub async fn get_process_status(&mut self, id: &str) -> anyhow::Result<ProcessStatus> {
        let should_remove = if let Some(running_process) = self.running_processes.get(id) {
            let mut status = running_process.status.lock().await;
            let child_guard = running_process.child.lock().await;
            if let Some(child) = child_guard.as_ref() {
                let pid = child.id();
                status.pid = Some(pid);
                if !is_process_running(pid) {
                    log::info!("Process {} (PID {}) is no longer running, removing from tracking", id, pid);
                    status.running = false; status.pid = None; true
                } else { false }
            } else { false }
        } else {
            return Ok(ProcessStatus {
                id: id.to_string(), running: false, pid: None,
                last_health_check: None, health_check_ok: None,
                restart_count: 0, last_restart: None, last_error: None,
            });
        };
        if should_remove {
            if let Some(running_process) = self.running_processes.remove(id) {
                if let Some(task) = running_process.health_check_task { task.abort(); }
            }
            if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == id) {
                p.last_pid = None; p.updated_at = Utc::now();
            }
            let _ = config::save_config(&self.config);
            return Ok(ProcessStatus {
                id: id.to_string(), running: false, pid: None,
                last_health_check: None, health_check_ok: None,
                restart_count: 0, last_restart: None, last_error: None,
            });
        }
        if let Some(running_process) = self.running_processes.get(id) {
            let status = running_process.status.lock().await;
            Ok(status.clone())
        } else {
            Ok(ProcessStatus {
                id: id.to_string(), running: false, pid: None,
                last_health_check: None, health_check_ok: None,
                restart_count: 0, last_restart: None, last_error: None,
            })
        }
    }

    pub async fn get_logs(&self, id: &str, lines: usize) -> anyhow::Result<String> {
        let process_config = self.config.processes.iter()
            .find(|p| p.id == id)
            .ok_or_else(|| anyhow::anyhow!("Process not found"))?;
        let log_path = if process_config.log_path.is_empty() {
            let log_dir = config::get_config_dir()?.join("logs");
            log_dir.join(format!("{}.log", id))
        } else {
            std::path::PathBuf::from(&process_config.log_path)
        };
        if !log_path.exists() { return Ok(String::new()); }
        let bytes = std::fs::read(&log_path)?;
        let content = String::from_utf8(bytes.clone())
            .unwrap_or_else(|_| { let (cow, _, _) = encoding_rs::GBK.decode(&bytes); cow.into_owned() });
        let log_lines: Vec<&str> = content.lines().collect();
        let reversed_lines: Vec<&str> = log_lines.into_iter().rev().collect();
        let end = if lines > 0 && reversed_lines.len() > lines { lines } else { reversed_lines.len() };
        Ok(reversed_lines[..end].join("\n"))
    }
}

impl Drop for DaemonManager {
    fn drop(&mut self) {
        log::info!("DaemonManager drop called, {} processes running", self.running_processes.len());
        #[cfg(target_os = "windows")]
        {
            for (_, running_process) in self.running_processes.iter() {
                if let Ok(child_guard) = running_process.child.try_lock() {
                    if let Some(child) = child_guard.as_ref() {
                        let pid = child.id();
                        log::info!("Sending graceful shutdown to PID {}", pid);
                        graceful_stop_process(pid);
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        log::info!("Cleanup complete");
    }
}
