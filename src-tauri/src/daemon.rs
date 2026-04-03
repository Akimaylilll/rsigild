use std::collections::HashMap;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use tauri::AppHandle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

use crate::config::{self, ProcessConfig};

// Job Object implementation for Windows
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
                    Err(e) => {
                        log::error!("Failed to query job info: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                log::error!("Failed to create job object: {}", e);
                None
            }
        }
    }

    pub fn assign_process(job: &Job, pid: u32) -> bool {
        // PROCESS_SET_QUOTA | PROCESS_TERMINATE = 0x0101
        let handle = unsafe { OpenProcess(0x0101, 0, pid) };
        if handle == 0 {
            log::error!("Failed to open process {}", pid);
            return false;
        }
        
        match job.assign_process(handle) {
            Ok(_) => {
                log::info!("Process {} assigned to job object", pid);
                true
            }
            Err(e) => {
                log::error!("Failed to assign process {} to job: {}", pid, e);
                false
            }
        }
    }

    pub fn terminate_job(job: &Job) -> bool {
        let handle = job.handle();
        let result = unsafe { TerminateJobObject(handle, 1) };
        if result == 0 {
            log::error!("Failed to terminate job object");
            false
        } else {
            log::info!("Job object terminated");
            true
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod job_object {
    pub fn create_job() -> Option<()> {
        Some(())
    }
    
    pub fn assign_process(_job: &(), _pid: u32) -> bool {
        true
    }
    
    pub fn terminate_job(_job: &()) -> bool {
        true
    }
}

#[cfg(not(target_os = "windows"))]
mod job_object {
    pub fn init() -> bool {
        true
    }
    
    pub fn assign_process(_pid: u32) -> bool {
        true
    }
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

fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {}", pid), "/NH", "/FO", "CSV"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .output();
        
        match output {
            Ok(out) => {
                // Convert GBK to UTF-8
                let (cow, _, _) = encoding_rs::GBK.decode(&out.stdout);
                let stdout = cow.into_owned();
                let result = stdout.contains(&pid.to_string());
                log::debug!("is_process_running({}): {} -> {}", pid, result, stdout.trim());
                result
            }
            Err(e) => {
                log::debug!("is_process_running({}): error: {}", pid, e);
                false
            }
        }
    }
    
    #[cfg(not(target_os = "windows"))]
    {
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
}

impl DaemonManager {
    pub async fn new(app_handle: AppHandle) -> anyhow::Result<Self> {
        let config = config::load_config()?;
        
        if let Ok(path) = config::get_config_path() {
            log::info!("Loaded config from {:?}", path);
        }
        
        let manager = Self {
            app_handle,
            config,
            running_processes: HashMap::new(),
        };
        
        log::info!("DaemonManager initialized with {} processes", manager.config.processes.len());
        
        Ok(manager)
    }

    pub fn get_processes(&self) -> Vec<ProcessConfig> {
        self.config.processes.clone()
    }

    pub async fn add_process(&mut self, config: ProcessConfig) -> anyhow::Result<()> {
        if self.config.processes.iter().any(|p| p.id == config.id) {
            anyhow::bail!("Process with id {} already exists", config.id);
        }
        
        if config.enabled {
            self.start_process_internal(&config).await?;
        }
        
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
            if new_config.enabled {
                self.start_process_internal(&new_config).await?;
            }
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

        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);
        
        // On Windows, create new process group for graceful shutdown
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            
            // Set console code page to GBK (936) for Chinese support
            extern "system" {
                fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
                fn SetConsoleCP(wCodePageID: u32) -> i32;
            }
            unsafe {
                SetConsoleOutputCP(936); // GBK
                SetConsoleCP(936);
            }
            
            // CREATE_NEW_PROCESS_GROUP = 0x00000200
            cmd.creation_flags(0x00000200);
        }
        
        if let Some(working_dir) = &config.working_dir {
            cmd.current_dir(working_dir);
        }
        
        for (key, value) in &config.env_vars {
            cmd.env(key, value);
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
        
        // Don't redirect stdout/stderr to file directly
        // We'll handle it in background threads with encoding conversion
        cmd.stdout(Stdio::piped())
           .stderr(Stdio::piped());

        let mut child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start process {}: {}", config.name, e))?;
        
        let pid = child.id();
        log::info!("Started process {} with PID {}", config.name, pid);
        
        // Take stdout and stderr for background processing
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        
        // Spawn thread to handle stdout
        if let Some(stdout) = stdout {
            let log_path = log_path.clone();
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut reader = stdout;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .ok();
                
                let mut buffer = [0u8; 4096];
                let mut leftover = Vec::new();
                
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let mut data = leftover.clone();
                            data.extend_from_slice(&buffer[..n]);
                            
                            // Try to decode as UTF-8, fallback to GBK
                            let text = String::from_utf8(data.clone())
                                .unwrap_or_else(|_| {
                                    let (cow, _, _) = encoding_rs::GBK.decode(&data);
                                    cow.into_owned()
                                });
                            
                            if let Some(ref mut f) = file {
                                let _ = write!(f, "{}", text);
                                let _ = f.flush();
                            }
                            
                            leftover.clear();
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        
        // Spawn thread to handle stderr
        if let Some(stderr) = stderr {
            let log_path = log_path.clone();
            std::thread::spawn(move || {
                use std::io::{Read, Write};
                let mut reader = stderr;
                let mut file = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .ok();
                
                let mut buffer = [0u8; 4096];
                
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let data = &buffer[..n];
                            
                            // Try to decode as UTF-8, fallback to GBK
                            let text = String::from_utf8(data.to_vec())
                                .unwrap_or_else(|_| {
                                    let (cow, _, _) = encoding_rs::GBK.decode(data);
                                    cow.into_owned()
                                });
                            
                            if let Some(ref mut f) = file {
                                let _ = write!(f, "{}", text);
                                let _ = f.flush();
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let child = cmd.spawn()
            .map_err(|e| anyhow::anyhow!("Failed to start process {}: {}", config.name, e))?;
        
        let pid = child.id();
        log::info!("Started process {} with PID {}", config.name, pid);
        
        // Create job object for this process
        let job = job_object::create_job();
        if let Some(ref j) = job {
            job_object::assign_process(j, pid);
        }
        
        if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == config.id) {
            p.last_pid = Some(pid);
            p.updated_at = Utc::now();
            log::info!("Updated config for process {}, last_pid={}", p.name, pid);
        }
        
        match config::save_config(&self.config) {
            Ok(_) => {
                if let Ok(path) = config::get_config_path() {
                    log::info!("Saved config to {:?}", path);
                }
            }
            Err(e) => log::error!("Failed to save config: {}", e),
        }

        let status = ProcessStatus {
            id: config.id.clone(),
            running: true,
            pid: Some(pid),
            last_health_check: None,
            health_check_ok: None,
            restart_count: 0,
            last_restart: None,
            last_error: None,
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
                    let result = client
                        .get(&url)
                        .timeout(std::time::Duration::from_secs(10))
                        .send()
                        .await;
                    
                    let mut status = status_clone.lock().await;
                    status.last_health_check = Some(Utc::now());
                    
                    let is_ok = match result {
                        Ok(resp) => resp.status().is_success(),
                        Err(e) => {
                            log::warn!("Health check failed for {}: {}", id, e);
                            false
                        }
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
                        if let Some(ref wd) = working_dir {
                            cmd.current_dir(wd);
                        }
                        cmd.stdout(Stdio::null())
                           .stderr(Stdio::null());
                        
                        match cmd.spawn() {
                            Ok(new_child) => {
                                let new_pid = new_child.id();
                                status.pid = Some(new_pid);
                                status.running = true;
                                *child_guard = Some(new_child);
                                // Create new job for restarted process
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
            child: child_arc,
            config: config_clone,
            health_check_task,
            status: status_arc,
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
        
        #[cfg(target_os = "windows")]
        {
            extern "system" {
                fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
            }
            
            // Send CTRL_BREAK_EVENT to all processes
            let mut pids = Vec::new();
            for (_, running_process) in self.running_processes.iter() {
                if let Ok(child_guard) = running_process.child.try_lock() {
                    if let Some(child) = child_guard.as_ref() {
                        let pid = child.id();
                        pids.push(pid);
                        log::info!("Sending CTRL_BREAK_EVENT to PID {}", pid);
                        unsafe {
                            GenerateConsoleCtrlEvent(1, pid);
                        }
                    }
                }
            }
            
            // Wait for each process to shutdown
            for pid in pids {
                let mut waited = 0;
                let max_wait = 50; // 5 seconds (50 * 100ms)
                
                while waited < max_wait {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    waited += 1;
                    
                    // Check if process is still running
                    #[cfg(target_os = "windows")]
                    {
                        let output = std::process::Command::new("tasklist")
                            .args(["/FI", &format!("PID eq {}", pid), "/NH", "/FO", "CSV"])
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::null())
                            .output();
                        
                        if let Ok(out) = output {
                            let stdout = String::from_utf8_lossy(&out.stdout);
                            if !stdout.contains(&pid.to_string()) {
                                log::info!("Process {} has exited", pid);
                                break;
                            }
                        }
                    }
                    
                    if waited == max_wait {
                        log::warn!("Process {} did not exit within 5 seconds", pid);
                    }
                }
            }
        }
        
        log::info!("Shutdown complete");
    }

    async fn stop_process_internal(&mut self, id: &str) -> anyhow::Result<()> {
        if let Some(running_process) = self.running_processes.remove(id) {
            if let Some(task) = running_process.health_check_task {
                task.abort();
                log::debug!("Aborted health check task for {}", id);
            }
            
            let mut child_guard = running_process.child.lock().await;
            if let Some(child) = child_guard.take() {
                let pid = child.id();
                log::info!("Stopping process {} with PID {}", id, pid);
                
                #[cfg(target_os = "windows")]
                {
                    extern "system" {
                        fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
                    }
                    
                    // Send CTRL_BREAK_EVENT to the process group
                    // The process was started with CREATE_NEW_PROCESS_GROUP
                    unsafe {
                        GenerateConsoleCtrlEvent(1, pid); // CTRL_BREAK_EVENT
                    }
                    log::info!("Sent CTRL_BREAK_EVENT to process group {}", pid);
                    
                    // Wait for graceful shutdown
                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                }
                
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = child.kill();
                }
                
                log::info!("Process {} stopped", id);
            }
            
            let mut status = running_process.status.lock().await;
            status.running = false;
            status.pid = None;
            
            if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == id) {
                p.last_pid = None;
                p.updated_at = Utc::now();
            }
            let _ = config::save_config(&self.config);
        } else {
            log::warn!("Process {} not found in running processes", id);
        }
        Ok(())
    }

    pub async fn get_process_status(&mut self, id: &str) -> anyhow::Result<ProcessStatus> {
        // First check if process exists and get info
        let should_remove = if let Some(running_process) = self.running_processes.get(id) {
            let mut status = running_process.status.lock().await;
            
            let child_guard = running_process.child.lock().await;
            if let Some(child) = child_guard.as_ref() {
                let pid = child.id();
                status.pid = Some(pid);
                
                // Check if process is still running
                if !is_process_running(pid) {
                    log::info!("Process {} (PID {}) is no longer running, removing from tracking", id, pid);
                    status.running = false;
                    status.pid = None;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            return Ok(ProcessStatus {
                id: id.to_string(),
                running: false,
                pid: None,
                last_health_check: None,
                health_check_ok: None,
                restart_count: 0,
                last_restart: None,
                last_error: None,
            });
        };
        
        // If process stopped, remove it
        if should_remove {
            if let Some(running_process) = self.running_processes.remove(id) {
                if let Some(task) = running_process.health_check_task {
                    task.abort();
                }
            }
            
            // Clear last_pid in config
            if let Some(p) = self.config.processes.iter_mut().find(|p| p.id == id) {
                p.last_pid = None;
                p.updated_at = Utc::now();
            }
            let _ = config::save_config(&self.config);
            
            return Ok(ProcessStatus {
                id: id.to_string(),
                running: false,
                pid: None,
                last_health_check: None,
                health_check_ok: None,
                restart_count: 0,
                last_restart: None,
                last_error: None,
            });
        }
        
        // Return current status
        if let Some(running_process) = self.running_processes.get(id) {
            let status = running_process.status.lock().await;
            Ok(status.clone())
        } else {
            Ok(ProcessStatus {
                id: id.to_string(),
                running: false,
                pid: None,
                last_health_check: None,
                health_check_ok: None,
                restart_count: 0,
                last_restart: None,
                last_error: None,
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
        
        log::info!("Looking for log file at: {:?}", log_path);
        
        if !log_path.exists() {
            log::warn!("Log file does not exist: {:?}", log_path);
            return Ok(String::new());
        }
        
        let bytes = std::fs::read(&log_path)?;
        log::info!("Read {} bytes from log file", bytes.len());
        
        // Simply read as UTF-8
        let content = String::from_utf8_lossy(&bytes).into_owned();
        log::info!("Log content length: {} chars", content.len());
        
        let log_lines: Vec<&str> = content.lines().collect();
        
        // Reverse lines so newest entries appear first
        let mut reversed_lines: Vec<&str> = log_lines.into_iter().rev().collect();
        
        let end = if lines > 0 && reversed_lines.len() > lines {
            lines
        } else {
            reversed_lines.len()
        };
        
        Ok(reversed_lines[..end].join("\n"))
    }
}

impl Drop for DaemonManager {
    fn drop(&mut self) {
        log::info!("DaemonManager drop called, {} processes running", self.running_processes.len());
        
        #[cfg(target_os = "windows")]
        {
            extern "system" {
                fn GenerateConsoleCtrlEvent(dwCtrlEvent: u32, dwProcessGroupId: u32) -> i32;
            }
            
            // Send CTRL_BREAK_EVENT to all processes
            for (_, running_process) in self.running_processes.iter() {
                if let Ok(child_guard) = running_process.child.try_lock() {
                    if let Some(child) = child_guard.as_ref() {
                        let pid = child.id();
                        log::info!("Sending CTRL_BREAK_EVENT to PID {}", pid);
                        unsafe {
                            GenerateConsoleCtrlEvent(1, pid); // CTRL_BREAK_EVENT
                        }
                    }
                }
            }
            
            // Wait for processes to shutdown gracefully
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
        
        log::info!("Cleanup complete");
    }
}
