# rsigild - Process Guardian

A cross-platform (Windows/macOS/Linux) process/service guardian built with Tauri + React.

## Features

- Save and manage process startup commands
- Health check monitoring (configurable interval, default 10 minutes)
- Auto-restart on health check failure
- Log storage at specified locations
- Cross-platform support

## Architecture

- **Backend**: Rust with Tauri
- **Frontend**: React + TypeScript + Vite

## Project Structure

```
rsigild/
├── src-tauri/           # Rust backend
│   ├── src/
│   │   ├── main.rs      # App entry, Tauri commands
│   │   ├── config.rs    # Configuration management
│   │   ├── daemon.rs    # Process daemon manager
│   │   └── logger.rs    # Logging setup
│   ├── Cargo.toml
│   └── icons/
├── src/                 # React frontend
│   ├── components/
│   │   ├── ProcessList.tsx
│   │   ├── ProcessForm.tsx
│   │   └── ProcessDetail.tsx
│   ├── App.tsx
│   └── main.tsx
├── package.json
├── tauri.conf.json
└── vite.config.ts
```

## Prerequisites

1. Node.js (v18+)
2. pnpm (`npm install -g pnpm`)
3. Rust (1.70+)
4. Tauri prerequisites: https://tauri.app/v1/guides/getting-started/prerequisites

## Installation

```bash
# Install pnpm dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

## Usage

1. Launch the application
2. Click "Add Process" to add a new process
3. Configure:
   - **Name**: Process identifier
   - **Command**: Executable path (e.g., `node`, `python`)
   - **Arguments**: Command arguments (space-separated)
   - **Working Directory**: Process working directory
   - **Health Check URL**: HTTP endpoint to monitor (e.g., `http://localhost:3000/health`)
   - **Health Check Interval**: Seconds between checks (default: 600 = 10 minutes)
   - **Auto Restart**: Restart process on health check failure
   - **Log Path**: Where to save process logs

4. Start/Stop processes from the UI
5. View logs and status in real-time

## Configuration Storage

Configs are stored in:
- Windows: `%APPDATA%/com.rsigild.rsigild/config/config.json`
- macOS: `~/Library/Application Support/com.rsigild.rsigild/config/config.json`
- Linux: `~/.config/rsigild/config/config.json`

## Log Storage

- App logs: `<config_dir>/logs/rsigild.log`
- Process logs: `<config_dir>/logs/<process_id>.log` (or custom path)

## API (Tauri Commands)

- `get_processes()` - List all processes
- `add_process(config)` - Add new process
- `update_process(config)` - Update process config
- `remove_process(id)` - Remove process
- `start_process(id)` - Start a process
- `stop_process(id)` - Stop a process
- `get_process_status(id)` - Get process status
- `get_logs(id, lines)` - Get process logs
- `check_health_url(url)` - Check a health endpoint

## Example: Node.js Backend

```json
{
  "name": "My API Server",
  "command": "node",
  "args": ["server.js"],
  "working_dir": "/path/to/app",
  "health_check_url": "http://localhost:3000/health",
  "health_check_interval_secs": 600,
  "auto_restart": true,
  "log_path": "/var/log/myapp.log"
}
```

## License

MIT
