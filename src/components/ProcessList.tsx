import { ProcessConfig, ProcessStatus } from "../types";
import "./ProcessList.css";

interface Props {
  processes: ProcessConfig[];
  statuses: Record<string, ProcessStatus>;
  selectedId: string | null;
  onSelect: (id: string) => void;
  onRemove: (id: string) => void;
  onEdit: (process: ProcessConfig) => void;
}

function ProcessList({ processes, statuses, selectedId, onSelect, onRemove, onEdit }: Props) {
  return (
    <div className="process-list">
      {processes.length === 0 ? (
        <div className="empty-list">No processes configured</div>
      ) : (
        processes.map((process) => {
          const status = statuses[process.id];
          const isRunning = status?.running ?? false;
          const healthOk = status?.health_check_ok;

          return (
            <div
              key={process.id}
              className={`process-item ${selectedId === process.id ? "selected" : ""}`}
              onClick={() => onSelect(process.id)}
            >
              <div className="process-item-header">
                <span className="process-name">{process.name}</span>
                <span className={`status-badge ${isRunning ? "running" : "stopped"}`}>
                  {isRunning ? "Running" : "Stopped"}
                </span>
              </div>
              <div className="process-command">{process.command}</div>
              <div className="process-meta">
                {status && (
                  <>
                    {status.pid && <span>PID: {status.pid}</span>}
                    {healthOk !== null && (
                      <span className={healthOk ? "health-ok" : "health-fail"}>
                        Health: {healthOk ? "OK" : "FAIL"}
                      </span>
                    )}
                    {status.restart_count > 0 && (
                      <span>Restarts: {status.restart_count}</span>
                    )}
                  </>
                )}
              </div>
              <div className="process-actions">
                <button
                  className="btn btn-secondary btn-small"
                  disabled={isRunning}
                  onClick={(e) => {
                    e.stopPropagation();
                    onEdit(process);
                  }}
                >
                  Edit
                </button>
                <button
                  className="btn btn-danger btn-small"
                  disabled={isRunning}
                  onClick={(e) => {
                    e.stopPropagation();
                    onRemove(process.id);
                  }}
                >
                  Remove
                </button>
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

export default ProcessList;
