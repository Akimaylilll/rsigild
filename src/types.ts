export interface ProcessConfig {
  id: string;
  name: string;
  command: string;
  args: string[];
  working_dir: string | null;
  env_vars: [string, string][];
  health_check_url: string | null;
  health_check_interval_secs: number;
  auto_restart: boolean;
  log_path: string;
  enabled: boolean;
  last_pid: number | null;
  created_at: string;
  updated_at: string;
}

export interface ProcessStatus {
  id: string;
  running: boolean;
  pid: number | null;
  last_health_check: string | null;
  health_check_ok: boolean | null;
  restart_count: number;
  last_restart: string | null;
  last_error: string | null;
}
