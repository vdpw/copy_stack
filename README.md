# Copy Stack - Desktop Clipboard Manager

A modern, cross-platform clipboard manager built with Tauri, React, and Rust. Copy Stack runs in your system tray and keeps track of your clipboard history, making it easy to access previously copied content.

## Features

- 🖥️ **Desktop Application**: Native desktop app with system tray integration
- 📋 **Clipboard History**: Automatically tracks your clipboard content
- 🎨 **Modern UI**: Beautiful, responsive interface with glassmorphism design
- 🔄 **Real-time Updates**: Instantly shows new clipboard entries
- 🖥️ **Desktop Window**: Clean desktop interface with modern design
- 🗑️ **Easy Management**: Delete individual entries or clear all history
- 🔍 **Quick Copy**: One-click copy of any previous clipboard entry

## Screenshots

The app features a modern gradient background with glassmorphism cards and a clean, intuitive interface.

## Installation

### Prerequisites

- [Node.js](https://nodejs.org/) (v18 or higher)
- [pnpm](https://pnpm.io/) (recommended) or npm
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
2. **Copy History**: All your clipboard operations are automatically tracked
3. **Quick Copy**: Click the copy button on any entry to copy it back to your clipboard
4. **Manage**: Delete individual entries or clear all history
5. **Refresh**: Use the refresh button to reload the clipboard history

## Development

### Project Structure

```
copy_stack/
├── src/                 # React frontend
│   ├── App.tsx         # Main application component
│   ├── App.css         # Styles
│   └── main.tsx        # Entry point
├── src-tauri/          # Rust backend
│   ├── src/
│   │   ├── main.rs     # Application entry point
│   │   ├── lib.rs      # Tauri commands and setup
│   │   ├── event/      # Event handling
│   │   └── store/      # Database operations
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
- `pnpm preview` - Preview web build

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

- [ ] Real clipboard monitoring (currently simulated)
- [ ] Keyboard shortcuts
- [ ] Search functionality
- [ ] Categories and tags
- [ ] Cloud sync
- [ ] Multiple clipboard formats support
- [ ] Export/import functionality

## Troubleshooting

### Common Issues

1. **Build fails**: Ensure you have Rust and Tauri CLI installed
2. **Window not displaying**: Check your display settings and window manager
3. **Clipboard not detected**: The current version uses simulated data for demonstration

### Platform Support

- ✅ Windows
- ✅ macOS
- ✅ Linux

## Support

If you encounter any issues or have questions, please open an issue on GitHub.
