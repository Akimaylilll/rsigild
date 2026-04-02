import { useState, useEffect, useRef } from "react";
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

function highlightSearch(text: string, searchTerm: string, currentMatch: number = 0, caseSensitive: boolean = false, wholeWord: boolean = false): string {
  if (!searchTerm) {
    return text;
  }
  
  const searchTerms = searchTerm.split(/\s+/).filter(t => t.length > 0);
  const isMultiWord = searchTerms.length > 1;
  
  // Build regex for all terms
  const patterns = searchTerms.map(term => {
    let escaped = term.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    if (wholeWord && !isMultiWord) {
      escaped = `\\b${escaped}\\b`;
    }
    return escaped;
  });
  
  const combinedPattern = `(${patterns.join('|')})`;
  const flags = caseSensitive ? 'g' : 'gi';
  const regex = new RegExp(combinedPattern, flags);
  
  if (isMultiWord) {
    // Multi-word: highlight matching lines
    const lines = text.split('\n');
    let matchCount = 0;
    const currentMatchIndex = currentMatch > 0 ? currentMatch - 1 : -1;
    
    return lines.map(line => {
      const plainLine = line.replace(/<[^>]+>/g, '');
      const lineForSearch = caseSensitive ? plainLine : plainLine.toLowerCase();
      
      let allMatch = true;
      for (const term of searchTerms) {
        const searchTermForCheck = caseSensitive ? term : term.toLowerCase();
        if (!lineForSearch.includes(searchTermForCheck)) {
          allMatch = false;
          break;
        }
      }
      
      if (allMatch) {
        matchCount++;
        const isCurrent = matchCount - 1 === currentMatchIndex;
        
        const highlightedLine = line.replace(/>([^<]+)</g, (match, content) => {
          const highlighted = content.replace(regex, (m: string) => {
            if (isCurrent) {
              return `<span class="current-match" style="background:#e94560;color:#fff;font-weight:bold">${m}</span>`;
            }
            return `<span style="background:#f39c12;color:#000;font-weight:bold">${m}</span>`;
          });
          return '>' + highlighted + '<';
        });
        
        return highlightedLine;
      }
      
      return line;
    }).join('\n');
  } else {
    // Single word: highlight all matches
    let matchCount = 0;
    const currentMatchIndex = currentMatch > 0 ? currentMatch - 1 : -1;
    
    return text.replace(/>([^<]+)</g, (match, content) => {
      return '>' + content.replace(regex, (m: string) => {
        matchCount++;
        const isCurrent = matchCount - 1 === currentMatchIndex;
        if (isCurrent) {
          return `<span class="current-match" style="background:#e94560;color:#fff;font-weight:bold">${m}</span>`;
        }
        return `<span style="background:#f39c12;color:#000;font-weight:bold">${m}</span>`;
      }) + '<';
    });
  }
}

function ProcessDetail({ process, status, onStart, onStop, onEdit }: Props) {
  const [logs, setLogs] = useState("");
  const [loadingLogs, setLoadingLogs] = useState(false);
  const [activeTab, setActiveTab] = useState<"details" | "logs">("details");
  const [searchTerm, setSearchTerm] = useState("");
  const [currentMatch, setCurrentMatch] = useState(0);
  const [totalMatches, setTotalMatches] = useState(0);
  const logsRef = useRef<HTMLPreElement>(null);
  const [shouldScroll, setShouldScroll] = useState(false);
  const lastMatchPosition = useRef<number>(0);
  const lastMatchLine = useRef<number>(0);
  const isFromButtonClick = useRef<boolean>(false);
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);

  const loadLogs = async () => {
    setLoadingLogs(true);
    try {
      const result = await invoke<string>("get_logs", { id: process.id, lines: 0 });
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

  useEffect(() => {
    if (!searchTerm) {
      setTotalMatches(0);
      setCurrentMatch(0);
      lastMatchPosition.current = 0;
      lastMatchLine.current = 0;
      return;
    }
    
    const searchTerms = searchTerm.split(/\s+/).filter(t => t.length > 0);
    const isMultiWord = searchTerms.length > 1;
    
    if (isMultiWord) {
      // Multiple terms: count matching lines
      const lines = logs.split('\n');
      const matchingLines: { lineIndex: number; startIndex: number }[] = [];
      
      let currentIndex = 0;
      for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const lineForSearch = caseSensitive ? line : line.toLowerCase();
        
        let allMatch = true;
        for (const term of searchTerms) {
          const searchTermForCheck = caseSensitive ? term : term.toLowerCase();
          if (!lineForSearch.includes(searchTermForCheck)) {
            allMatch = false;
            break;
          }
        }
        
        if (allMatch) {
          matchingLines.push({ lineIndex: i, startIndex: currentIndex });
        }
        
        currentIndex += line.length + 1;
      }
      
      const count = matchingLines.length;
      setTotalMatches(count);
      
      if (count > 0) {
        // If last position is 0, auto select first match and scroll
        if (lastMatchPosition.current === 0) {
          setCurrentMatch(1);
          setShouldScroll(true);
        } else {
          let newMatchIndex = 0;
          let minDistance = Infinity;
          for (let i = 0; i < matchingLines.length; i++) {
            const distance = Math.abs(matchingLines[i].startIndex - lastMatchPosition.current);
            if (distance < minDistance) {
              minDistance = distance;
              newMatchIndex = i;
            }
          }
          setCurrentMatch(newMatchIndex + 1);
        }
      } else {
        setCurrentMatch(0);
      }
    } else {
      // Single term: count all matches
      let escapedTerm = searchTerms[0].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
      if (wholeWord) {
        escapedTerm = `\\b${escapedTerm}\\b`;
      }
      const flags = caseSensitive ? 'g' : 'gi';
      const regex = new RegExp(escapedTerm, flags);
      const matches = [...logs.matchAll(regex)];
      const count = matches.length;
      setTotalMatches(count);
      
      if (count > 0) {
        // If last position is 0, auto select first match and scroll
        if (lastMatchPosition.current === 0) {
          setCurrentMatch(1);
          setShouldScroll(true);
        } else {
          let newMatchIndex = 0;
          let minDistance = Infinity;
          for (let i = 0; i < matches.length; i++) {
            const distance = Math.abs((matches[i].index || 0) - lastMatchPosition.current);
            if (distance < minDistance) {
              minDistance = distance;
              newMatchIndex = i;
            }
          }
          setCurrentMatch(newMatchIndex + 1);
        }
      } else {
        setCurrentMatch(0);
      }
    }
  }, [searchTerm, logs, caseSensitive, wholeWord]);

  useEffect(() => {
    if (logsRef.current && currentMatch > 0 && searchTerm && shouldScroll) {
      const currentElement = logsRef.current.querySelector('.current-match');
      if (currentElement) {
        currentElement.scrollIntoView({ block: 'center' });
      }
      setShouldScroll(false);
    }
  }, [currentMatch, searchTerm, shouldScroll]);

  useEffect(() => {
    if (logsRef.current && currentMatch > 0 && searchTerm && shouldScroll) {
      const currentElement = logsRef.current.querySelector('.current-match');
      if (currentElement) {
        currentElement.scrollIntoView({ block: 'center' });
      }
      setShouldScroll(false);
    }
  }, [currentMatch, searchTerm, shouldScroll]);

  // Listen to scroll events
  useEffect(() => {
    const container = logsRef.current;
    if (!container) return;

    const handleScroll = () => {
      if (!isFromButtonClick.current) {
        const scrollTop = container.scrollTop;
        const scrollHeight = container.scrollHeight;
        const textLength = logs.length;
        const position = (scrollTop / scrollHeight) * textLength;
        lastMatchPosition.current = position;
        lastMatchLine.current = logs.substring(0, position).split('\n').length - 1;
      }
      isFromButtonClick.current = false;
    };

    container.addEventListener('scroll', handleScroll);
    return () => container.removeEventListener('scroll', handleScroll);
  }, [logs]);

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
                <div className="search-container">
                  <input
                    type="text"
                    className="search-input"
                    placeholder="Search logs..."
                    value={searchTerm}
                    onChange={(e) => {
                      setSearchTerm(e.target.value);
                      setCurrentMatch(0);
                    }}
                  />
                  <button
                    className={`search-toggle ${caseSensitive ? 'active' : ''}`}
                    onClick={() => setCaseSensitive(!caseSensitive)}
                    title="Match Case"
                  >
                    Aa
                  </button>
                  <button
                    className={`search-toggle ${wholeWord ? 'active' : ''}`}
                    onClick={() => setWholeWord(!wholeWord)}
                    title="Match Whole Word"
                  >
                    ab
                  </button>
                  <div className="search-nav">
                    <span className="match-count">
                      {currentMatch}/{totalMatches}
                    </span>
                    <button 
                      className="search-nav-btn"
                      onClick={() => {
                        isFromButtonClick.current = true;
                        
                        const searchTerms = searchTerm.split(/\s+/).filter(t => t.length > 0);
                        const isMultiWord = searchTerms.length > 1;
                        
                        if (isMultiWord) {
                          const lines = logs.split('\n');
                          const matchingLines: { lineIndex: number; startIndex: number }[] = [];
                          
                          let currentIndex = 0;
                          for (let i = 0; i < lines.length; i++) {
                            const line = lines[i];
                            const lineForSearch = caseSensitive ? line : line.toLowerCase();
                            
                            let allMatch = true;
                            for (const term of searchTerms) {
                              const searchTermForCheck = caseSensitive ? term : term.toLowerCase();
                              if (!lineForSearch.includes(searchTermForCheck)) {
                                allMatch = false;
                                break;
                              }
                            }
                            
                            if (allMatch) {
                              matchingLines.push({ lineIndex: i, startIndex: currentIndex });
                            }
                            
                            currentIndex += line.length + 1;
                          }
                          
                          let targetMatch = -1;
                          for (let i = matchingLines.length - 1; i >= 0; i--) {
                            if (matchingLines[i].startIndex < lastMatchPosition.current - 10) {
                              targetMatch = i;
                              break;
                            }
                          }
                          
                          if (targetMatch === -1) {
                            targetMatch = matchingLines.length - 1;
                          }
                          
                          setCurrentMatch(targetMatch + 1);
                          lastMatchPosition.current = matchingLines[targetMatch].startIndex;
                          lastMatchLine.current = matchingLines[targetMatch].lineIndex;
                        } else {
                          let escapedTerm = searchTerms[0].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                          if (wholeWord) {
                            escapedTerm = `\\b${escapedTerm}\\b`;
                          }
                          const flags = caseSensitive ? 'g' : 'gi';
                          const regex = new RegExp(escapedTerm, flags);
                          const matches = [...logs.matchAll(regex)];
                          
                          let useLineSearch = false;
                          if (currentMatch > 0 && matches[currentMatch - 1]) {
                            const currentMatchLine = logs.substring(0, matches[currentMatch - 1].index || 0).split('\n').length - 1;
                            if (currentMatchLine !== lastMatchLine.current) {
                              useLineSearch = true;
                            }
                          }
                          
                          let targetMatch;
                          if (useLineSearch) {
                            for (let i = matches.length - 1; i >= 0; i--) {
                              const matchLine = logs.substring(0, matches[i].index || 0).split('\n').length - 1;
                              if (matchLine < lastMatchLine.current) {
                                targetMatch = i;
                                break;
                              }
                            }
                            if (targetMatch === undefined) {
                              targetMatch = matches.length - 1;
                            }
                          } else {
                            targetMatch = currentMatch - 2;
                            if (targetMatch < 0) {
                              targetMatch = matches.length - 1;
                            }
                          }
                          
                          setCurrentMatch(targetMatch + 1);
                          const pos = matches[targetMatch].index || 0;
                          lastMatchPosition.current = pos;
                          lastMatchLine.current = logs.substring(0, pos).split('\n').length - 1;
                        }
                        setShouldScroll(true);
                      }}
                      disabled={totalMatches === 0}
                    >
                      ▲
                    </button>
                    <button 
                      className="search-nav-btn"
                      onClick={() => {
                        isFromButtonClick.current = true;
                        
                        const searchTerms = searchTerm.split(/\s+/).filter(t => t.length > 0);
                        const isMultiWord = searchTerms.length > 1;
                        
                        if (isMultiWord) {
                          const lines = logs.split('\n');
                          const matchingLines: { lineIndex: number; startIndex: number }[] = [];
                          
                          let currentIndex = 0;
                          for (let i = 0; i < lines.length; i++) {
                            const line = lines[i];
                            const lineForSearch = caseSensitive ? line : line.toLowerCase();
                            
                            let allMatch = true;
                            for (const term of searchTerms) {
                              const searchTermForCheck = caseSensitive ? term : term.toLowerCase();
                              if (!lineForSearch.includes(searchTermForCheck)) {
                                allMatch = false;
                                break;
                              }
                            }
                            
                            if (allMatch) {
                              matchingLines.push({ lineIndex: i, startIndex: currentIndex });
                            }
                            
                            currentIndex += line.length + 1;
                          }
                          
                          let targetMatch = -1;
                          for (let i = 0; i < matchingLines.length; i++) {
                            if (matchingLines[i].startIndex > lastMatchPosition.current + 10) {
                              targetMatch = i;
                              break;
                            }
                          }
                          
                          if (targetMatch === -1) {
                            targetMatch = 0;
                          }
                          
                          setCurrentMatch(targetMatch + 1);
                          lastMatchPosition.current = matchingLines[targetMatch].startIndex;
                          lastMatchLine.current = matchingLines[targetMatch].lineIndex;
                        } else {
                          let escapedTerm = searchTerms[0].replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
                          if (wholeWord) {
                            escapedTerm = `\\b${escapedTerm}\\b`;
                          }
                          const flags = caseSensitive ? 'g' : 'gi';
                          const regex = new RegExp(escapedTerm, flags);
                          const matches = [...logs.matchAll(regex)];
                          
                          let useLineSearch = false;
                          if (currentMatch > 0 && matches[currentMatch - 1]) {
                            const currentMatchLine = logs.substring(0, matches[currentMatch - 1].index || 0).split('\n').length - 1;
                            if (currentMatchLine !== lastMatchLine.current) {
                              useLineSearch = true;
                            }
                          }
                          
                          let targetMatch;
                          if (useLineSearch) {
                            for (let i = 0; i < matches.length; i++) {
                              const matchLine = logs.substring(0, matches[i].index || 0).split('\n').length - 1;
                              if (matchLine > lastMatchLine.current) {
                                targetMatch = i;
                                break;
                              }
                            }
                            if (targetMatch === undefined) {
                              targetMatch = 0;
                            }
                          } else {
                            targetMatch = currentMatch;
                            if (targetMatch >= matches.length) {
                              targetMatch = 0;
                            }
                          }
                          
                          setCurrentMatch(targetMatch + 1);
                          const pos = matches[targetMatch].index || 0;
                          lastMatchPosition.current = pos;
                          lastMatchLine.current = logs.substring(0, pos).split('\n').length - 1;
                        }
                        setShouldScroll(true);
                      }}
                      disabled={totalMatches === 0}
                    >
                      ▼
                    </button>
                  </div>
                  <button className="refresh-btn" onClick={loadLogs} disabled={loadingLogs} title="Refresh">
                    ↻
                  </button>
                </div>
              </div>
              <pre ref={logsRef} className="logs-content" dangerouslySetInnerHTML={{ __html: logs ? highlightSearch(colorizeLogLevel(ansiToHtml(logs)), searchTerm, currentMatch, caseSensitive, wholeWord) : "No logs available" }} />
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
