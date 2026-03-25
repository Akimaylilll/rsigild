import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import ProcessList from "./components/ProcessList";
import ProcessForm from "./components/ProcessForm";
import ProcessDetail from "./components/ProcessDetail";
import { ProcessConfig, ProcessStatus } from "./types";
import "./App.css";

function App() {
  const [processes, setProcesses] = useState<ProcessConfig[]>([]);
  const [statuses, setStatuses] = useState<Record<string, ProcessStatus>>({});
  const [selectedProcess, setSelectedProcess] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [editingProcess, setEditingProcess] = useState<ProcessConfig | null>(null);

  const loadProcesses = async () => {
    try {
      const result = await invoke<ProcessConfig[]>("get_processes");
      setProcesses(result);
      
      const statusPromises = result.map(async (p) => {
        const status = await invoke<ProcessStatus>("get_process_status", { id: p.id });
        return { id: p.id, status };
      });
      
      const statusResults = await Promise.all(statusPromises);
      const statusMap: Record<string, ProcessStatus> = {};
      statusResults.forEach(({ id, status }) => {
        statusMap[id] = status;
      });
      setStatuses(statusMap);
    } catch (error) {
      console.error("Failed to load processes:", error);
    }
  };

  useEffect(() => {
    loadProcesses();
    const interval = setInterval(loadProcesses, 5000);
    return () => clearInterval(interval);
  }, []);

  const handleAddProcess = async (config: ProcessConfig) => {
    try {
      await invoke("add_process", { config });
      await loadProcesses();
      setShowForm(false);
    } catch (error) {
      console.error("Failed to add process:", error);
    }
  };

  const handleUpdateProcess = async (config: ProcessConfig) => {
    try {
      await invoke("update_process", { config });
      await loadProcesses();
      setEditingProcess(null);
      setShowForm(false);
    } catch (error) {
      console.error("Failed to update process:", error);
    }
  };

  const handleRemoveProcess = async (id: string) => {
    try {
      await invoke("remove_process", { id });
      if (selectedProcess === id) {
        setSelectedProcess(null);
      }
      await loadProcesses();
    } catch (error) {
      console.error("Failed to remove process:", error);
    }
  };

  const handleStartProcess = async (id: string) => {
    try {
      await invoke("start_process", { id });
      await loadProcesses();
    } catch (error) {
      console.error("Failed to start process:", error);
    }
  };

  const handleStopProcess = async (id: string) => {
    try {
      await invoke("stop_process", { id });
      await loadProcesses();
    } catch (error) {
      console.error("Failed to stop process:", error);
    }
  };

  const handleEdit = (process: ProcessConfig) => {
    setEditingProcess(process);
    setShowForm(true);
  };

  const handleCancel = () => {
    setEditingProcess(null);
    setShowForm(false);
  };

  return (
    <div className="app">
      <header className="header">
        <h1>rsigild</h1>
        <span className="subtitle">Process Guardian</span>
      </header>
      
      <main className="main">
        <div className="sidebar">
          <div className="sidebar-header">
            <h2>Processes</h2>
            <button className="btn btn-primary" onClick={() => setShowForm(true)}>
              Add Process
            </button>
          </div>
          <ProcessList
            processes={processes}
            statuses={statuses}
            selectedId={selectedProcess}
            onSelect={setSelectedProcess}
            onRemove={handleRemoveProcess}
            onEdit={handleEdit}
          />
        </div>
        
        <div className="content">
          {showForm ? (
            <ProcessForm
              initialData={editingProcess}
              onSubmit={editingProcess ? handleUpdateProcess : handleAddProcess}
              onCancel={handleCancel}
            />
          ) : selectedProcess ? (
            <ProcessDetail
              process={processes.find((p) => p.id === selectedProcess)!}
              status={statuses[selectedProcess]}
              onStart={handleStartProcess}
              onStop={handleStopProcess}
              onEdit={handleEdit}
            />
          ) : (
            <div className="empty-state">
              <h3>Select a process to view details</h3>
              <p>Or add a new process to get started</p>
            </div>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
