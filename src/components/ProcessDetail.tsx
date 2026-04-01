import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProcessConfig, ProcessStatus } from "../types";
import "./ProcessDetail.css";

interface Props {
  process: ProcessConfig;
  status: ProcessStatus;
  onStart: (id: string) => void;
  onStop: (id: string) => void;
  onEdit: (process: ProcessConfig) => void;
}

function ansiToHtml(text: string): string {
  const ansiColorMap: Record<string, string> = {
    "30": "color:#000",
    "31": "color:#e74c3c",
    "32": "color:#27ae60",
    "33": "color:#f39c12",
    "34": "color:#3498db",
    "35": "color:#9b59b6",
    "36": "color:#1abc9c",
    "37": "color:#ecf0f1",
    "90": "color:#7f8c8d",
    "91": "color:#e74c3c;font-weight:bold",
    "92": "color:#27ae60;font-weight:bold",
    "93": "color:#f39c12;font-weight:bold",
    "94": "color:#3498db;font-weight:bold",
    "95": "color:#9b59b6;font-weight:bold",
    "96": "color:#1abc9c;font-weight:bold",
    "1": "font-weight:bold",
  };

  let result = text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");

  result = result.replace(/\x1b\[([0-9;]*)m/g, (_, codes) => {
    if (codes === "0" || codes === "") {
      return "</span>";
    }
    const codeList = codes.split(";");
    const styles: string[] = [];
    for (const code of codeList) {
      if (ansiColorMap[code]) {
        styles.push(ansiColorMap[code]);
      }
    }
    if (styles.length > 0) {
      return `<span style="${styles.join(";")}">`;
    }
    return "";
  });

  return result;
}

function colorizeLogLevel(text: string): string {
  let result = text;

  // Color log levels with dash format
  result = result.replace(/( - )(INFO)( - )/g, '$1<span style="color:#27ae60;font-weight:bold">$2</span>$3');
  result = result.replace(/( - )(DEBUG)( - )/g, '$1<span style="color:#3498db;font-weight:bold">$2</span>$3');
  result = result.replace(/( - )(WARNING)( - )/g, '$1<span style="color:#f39c12;font-weight:bold">$2</span>$3');
  result = result.replace(/( - )(ERROR)( - )/g, '$1<span style="color:#e74c3c;font-weight:bold">$2</span>$3');
  result = result.replace(/( - )(TRACE)( - )/g, '$1<span style="color:#9b59b6;font-weight:bold">$2</span>$3');

  // Color standalone log levels: "INFO:" or "INFO "
  result = result.replace(/^(INFO)([:\s])/gm, '<span style="color:#27ae60;font-weight:bold">$1</span>$2');
  result = result.replace(/^(DEBUG)([:\s])/gm, '<span style="color:#3498db;font-weight:bold">$1</span>$2');
  result = result.replace(/^(WARNING)([:\s])/gm, '<span style="color:#f39c12;font-weight:bold">$1</span>$2');
  result = result.replace(/^(ERROR)([:\s])/gm, '<span style="color:#e74c3c;font-weight:bold">$1</span>$2');
  result = result.replace(/^(TRACE)([:\s])/gm, '<span style="color:#9b59b6;font-weight:bold">$1</span>$2');

  return result;
}

function ProcessDetail({ process, status, onStart, onStop, onEdit }: Props) {
  const [logs, setLogs] = useState("");
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [activeTab, setActiveTab] = useState<"details" | "logs">("details");

  const loadLogs = async () => {
    setLoadingLogs(true);
    try {
      const result = await invoke<string>("get_logs", { id: process.id, lines: 200 });
      setLogs(result);
    } catch (error) {
      console.error("Failed to load logs:", error);
      setLogs("Failed to load logs");
    }
    setLoadingLogs(false);
  };

  useEffect(() => {
    loadLogs();
    const interval = setInterval(loadLogs, 10000);
    return () => clearInterval(interval);
  }, [process.id]);

  const formatDate = (dateStr: string | null) => {
    if (!dateStr) return "N/A";
    return new Date(dateStr).toLocaleString();
  };

  return (
    <div className="process-detail">
      <div className="detail-header">
        <h2>{process.name}</h2>
        <div className="detail-actions">
          {status.running ? (
            <button className="btn btn-warning" onClick={() => onStop(process.id)}>
              Stop
            </button>
          ) : (
            <button className="btn btn-success" onClick={() => onStart(process.id)}>
              Start
            </button>
          )}
          <button 
            className="btn btn-secondary" 
            disabled={status.running} 
            onClick={() => onEdit(process)}
          >
            Edit
          </button>
        </div>
      </div>

      <div className="tabs-container">
        <div className="tabs-header">
          <button 
            className={`tab-btn ${activeTab === "details" ? "active" : ""}`}
            onClick={() => setActiveTab("details")}
          >
            Details
          </button>
          <button 
            className={`tab-btn ${activeTab === "logs" ? "active" : ""}`}
            onClick={() => setActiveTab("logs")}
          >
            Logs
          </button>
        </div>

        <div className="tab-content">
          {activeTab === "logs" && (
            <div className="logs-section">
              <div className="logs-header">
                <button className="btn btn-secondary btn-small" onClick={loadLogs} disabled={loadingLogs}>
                  {loadingLogs ? "Loading..." : "Refresh"}
                </button>
              </div>
              <pre className="logs-content" dangerouslySetInnerHTML={{ __html: colorizeLogLevel(ansiToHtml(logs)) || "No logs available" }} />
            </div>
          )}

          {activeTab === "details" && (
            <div className="details-section">
              <div className="detail-grid">
                <div className="detail-card">
                  <h3>Status</h3>
                  <div className="detail-content">
                    <div className="detail-row">
                      <span className="label">Running:</span>
                      <span className={`value ${status.running ? "status-ok" : "status-off"}`}>
                        {status.running ? "Yes" : "No"}
                      </span>
                    </div>
                    <div className="detail-row">
                      <span className="label">PID:</span>
                      <span className="value">{status.pid || "N/A"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Health Check:</span>
                      <span
                        className={`value ${
                          status.health_check_ok === null
                            ? ""
                            : status.health_check_ok
                            ? "status-ok"
                            : "status-fail"
                        }`}
                      >
                        {status.health_check_ok === null
                          ? "N/A"
                          : status.health_check_ok
                          ? "OK"
                          : "FAIL"}
                      </span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Last Health Check:</span>
                      <span className="value">{formatDate(status.last_health_check)}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Restart Count:</span>
                      <span className="value">{status.restart_count}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Last Restart:</span>
                      <span className="value">{formatDate(status.last_restart)}</span>
                    </div>
                  </div>
                </div>

                <div className="detail-card">
                  <h3>Configuration</h3>
                  <div className="detail-content">
                    <div className="detail-row">
                      <span className="label">Command:</span>
                      <span className="value code">{process.command}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Arguments:</span>
                      <span className="value code">{process.args.join(" ") || "None"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Working Dir:</span>
                      <span className="value">{process.working_dir || "Default"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Health URL:</span>
                      <span className="value">{process.health_check_url || "None"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Check Interval:</span>
                      <span className="value">{process.health_check_interval_secs}s</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Auto Restart:</span>
                      <span className="value">{process.auto_restart ? "Yes" : "No"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Enabled:</span>
                      <span className="value">{process.enabled ? "Yes" : "No"}</span>
                    </div>
                    <div className="detail-row">
                      <span className="label">Log Path:</span>
                      <span className="value code">{process.log_path || "Default"}</span>
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default ProcessDetail;
