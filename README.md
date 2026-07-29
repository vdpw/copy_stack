# Copy Stack - Desktop Clipboard Manager

A modern macOS clipboard manager built with Tauri, React, and Rust. Copy Stack
runs in the menu bar and keeps a private local history of eligible clipboard
content, making it easy to restore previously copied items.

## Features

- 🖥️ **macOS Desktop App**: Native Tauri app with a localized menu bar
- 📋 **Private Clipboard History**: Stores eligible clipboard content locally
- ⚡ **Bounded History UI**: Loads 50-item pages and rich details on demand
- 🔄 **Real-time Updates**: Refreshes after accepted clipboard changes
- 🔍 **Quick Restore**: Restores an item from History or the menu bar
- 🗑️ **Bounded Storage**: Enforces both item-count and byte budgets
- 🚀 **Native Lifecycle**: Supports single-instance activation and opt-in
  launch at login

## Screenshots

The app features a modern gradient background with glassmorphism cards and a clean, intuitive interface.

## Installation

### Prerequisites

- [Node.js](https://nodejs.org/) (v18 or higher)
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) (for Tauri development)

### Development Setup

1. Clone the repository:

```bash
git clone <repository-url>
cd copy_stack
```

2. Install dependencies:

```bash
pnpm install
```

3. Start the development server:

```bash
# For desktop development
pnpm desktop:dev

# For web development only
pnpm dev
```

### Building for Production

```bash
# Build the desktop application
pnpm desktop:build

# Build for web only
pnpm build
```

## Usage

### Desktop Application

1. **Launch**: Start the application and it will open in a desktop window
2. **Copy History**: Supported user-copied content is automatically tracked
3. **Quick Copy**: Click the copy button on any entry to copy it back to your clipboard
4. **Manage**: Delete individual entries or clear all history
5. **Refresh**: Use the refresh button to reload the clipboard history

## Clipboard Privacy

Copy Stack follows the [NSPasteboard.org](https://nspasteboard.org/) conventions
used by macOS clipboard tools:

- Content marked transient or automatically generated is excluded from history.
- Content marked confidential, including supported password-manager markers, is
  not stored, previewed, added to the tray, or exported to the optional JSONL
  history mirror.
- Source bundle identifiers and Apple remote-clipboard provenance are
  informational metadata only and do not change content deduplication.
- Restoring history writes the standard `org.nspasteboard.source` marker with
  the original source or an explicit empty value when the source is unknown.

History is stored locally in SQLite. The optional JSONL history mirror can
contain accepted clipboard contents and should be protected like the database.
Do not share either file without reviewing and sanitizing it first.

## Development

### Documentation

Detailed project docs live in [`docs/index.md`](docs/index.md). The root
[`AGENTS.md`](AGENTS.md) file is a compact menu for coding agents and links to
the detailed docs for architecture, frontend, backend, persistence, clipboard
flows, development, release, and troubleshooting.

Performance work starts with [`docs/performance.md`](docs/performance.md).
Release security and the required macOS manual matrix are in
[`docs/security-release-checklist.md`](docs/security-release-checklist.md).

### Project Structure

```
copy_stack/
├── src/                 # React frontend
│   ├── features/       # History and Settings surfaces
│   ├── hooks/          # Paged history, lazy details, and settings state
│   ├── api/            # Typed Tauri command/error boundary
│   ├── App.tsx         # Window-level composition
│   └── main.tsx        # Entry point
├── src-tauri/          # Rust backend
│   ├── src/
│   │   ├── lib.rs      # Lifecycle, commands, and capture pipeline
│   │   ├── store/      # Versioned SQLite persistence and paging
│   │   ├── history_mirror.rs
│   │   └── private_fs.rs
│   └── tauri.conf.json # Tauri configuration
└── package.json        # Node.js dependencies
```

### Key Technologies

- **Frontend**: React 18, TypeScript, Vite
- **Backend**: Rust, Tauri 2
- **Database**: SQLite (via rusqlite)
- **UI**: Custom CSS with glassmorphism design
- **Icons**: Lucide React

### Available Scripts

- `pnpm desktop:dev` - Start desktop development server
- `pnpm desktop:build` - Build desktop application
- `pnpm dev` - Start web development server
- `pnpm build` - Build web application
- `pnpm type-check` - Type-check the frontend
- `pnpm lint` - Lint the frontend
- `pnpm test` - Run frontend unit tests
- `pnpm security-check` - Verify security configuration guardrails

## Configuration

The application can be configured through the `src-tauri/tauri.conf.json` file:

- Window size and behavior
- System tray settings
- Application metadata
- Build settings

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly
5. Submit a pull request

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Roadmap

- [ ] Rich previews for more clipboard formats
- [ ] Categories and tags
- [ ] Windows platform support
- [ ] Search functionality
- [ ] Keyboard shortcuts

## Troubleshooting

### Common Issues

1. **Build fails**: Ensure you have Rust and Tauri CLI installed
2. **Window not displaying**: Check your display settings and window manager
3. **Clipboard not detected**: Run the desktop app with `pnpm desktop:dev`,
   confirm the published `copy_event_listener = "0.1.2"` dependency resolves,
   and check the redacted debug control-flow logs

### Platform Support

- ✅ macOS

## Support

If you encounter any issues or have questions, please open an issue on GitHub.
