import { useState, useEffect } from "react";
import { ProcessConfig } from "../types";
import "./ProcessForm.css";

interface Props {
  initialData: ProcessConfig | null;
  onSubmit: (data: ProcessConfig) => void;
  onCancel: () => void;
}

function ProcessForm({ initialData, onSubmit, onCancel }: Props) {
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [workingDir, setWorkingDir] = useState("");
  const [healthCheckUrl, setHealthCheckUrl] = useState("");
  const [healthCheckInterval, setHealthCheckInterval] = useState(600);
  const [autoRestart, setAutoRestart] = useState(true);
  const [logPath, setLogPath] = useState("");
  const [enabled, setEnabled] = useState(true);

  useEffect(() => {
    if (initialData) {
      setName(initialData.name);
      setCommand(initialData.command);
      setArgs(initialData.args.join(" "));
      setWorkingDir(initialData.working_dir || "");
      setHealthCheckUrl(initialData.health_check_url || "");
      setHealthCheckInterval(initialData.health_check_interval_secs);
      setAutoRestart(initialData.auto_restart);
      setLogPath(initialData.log_path);
      setEnabled(initialData.enabled);
    }
  }, [initialData]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    
    const config: ProcessConfig = {
      id: initialData?.id ?? crypto.randomUUID(),
      name,
      command,
      args: args.split(" ").filter((a) => a.trim()),
      working_dir: workingDir || null,
      env_vars: initialData?.env_vars ?? [],
      health_check_url: healthCheckUrl || null,
      health_check_interval_secs: healthCheckInterval,
      auto_restart: autoRestart,
      log_path: logPath,
      enabled,
      last_pid: initialData?.last_pid ?? null,
      created_at: initialData?.created_at ?? new Date().toISOString(),
      updated_at: new Date().toISOString(),
    };
    
    onSubmit(config);
  };

  return (
    <div className="process-form">
      <h2>{initialData ? "Edit Process" : "Add Process"}</h2>
      <form onSubmit={handleSubmit}>
        <div className="form-group">
          <label htmlFor="name">Process Name</label>
          <input
            type="text"
            id="name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="My Backend Service"
            required
          />
        </div>

        <div className="form-group">
          <label htmlFor="command">Command</label>
          <input
            type="text"
            id="command"
            value={command}
            onChange={(e) => setCommand(e.target.value)}
            placeholder="node"
            required
          />
        </div>

        <div className="form-group">
          <label htmlFor="args">Arguments (space-separated)</label>
          <input
            type="text"
            id="args"
            value={args}
            onChange={(e) => setArgs(e.target.value)}
            placeholder="server.js --port 3000"
          />
        </div>

        <div className="form-group">
          <label htmlFor="workingDir">Working Directory (optional)</label>
          <input
            type="text"
            id="workingDir"
            value={workingDir}
            onChange={(e) => setWorkingDir(e.target.value)}
            placeholder="/path/to/app"
          />
        </div>

        <div className="form-group">
          <label htmlFor="healthCheckUrl">Health Check URL (optional)</label>
          <input
            type="text"
            id="healthCheckUrl"
            value={healthCheckUrl}
            onChange={(e) => setHealthCheckUrl(e.target.value)}
            placeholder="http://localhost:3000/health"
          />
        </div>

        <div className="form-group">
          <label htmlFor="healthCheckInterval">Health Check Interval (seconds)</label>
          <input
            type="number"
            id="healthCheckInterval"
            value={healthCheckInterval}
            onChange={(e) => setHealthCheckInterval(parseInt(e.target.value))}
            min="10"
          />
        </div>

        <div className="form-group">
          <label htmlFor="logPath">Log File Path (optional)</label>
          <input
            type="text"
            id="logPath"
            value={logPath}
            onChange={(e) => setLogPath(e.target.value)}
            placeholder="/path/to/logs/app.log"
          />
        </div>

        <div className="form-group checkbox-group">
          <label>
            <input
              type="checkbox"
              checked={autoRestart}
              onChange={(e) => setAutoRestart(e.target.checked)}
            />
            Auto-restart on failure
          </label>
        </div>

        <div className="form-group checkbox-group">
          <label>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            Enabled
          </label>
        </div>

        <div className="form-actions">
          <button type="submit" className="btn btn-primary">
            {initialData ? "Update" : "Add"} Process
          </button>
          <button type="button" className="btn btn-secondary" onClick={onCancel}>
            Cancel
          </button>
        </div>
      </form>
    </div>
  );
}

export default ProcessForm;
