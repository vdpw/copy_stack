#!/bin/bash

# Copy Stack Desktop Application Startup Script

echo "🚀 Starting Copy Stack Desktop Application..."

# Check if pnpm is installed
if ! command -v pnpm &>/dev/null; then
    echo "❌ pnpm is not installed. Please install pnpm first:"
    echo "   npm install -g pnpm"
    exit 1
fi

# Check if Rust is installed
if ! command -v cargo &>/dev/null; then
    echo "❌ Rust is not installed. Please install Rust first:"
    echo "   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "📦 Installing dependencies..."
    pnpm install
fi

# Start the desktop application
echo "🖥️  Launching Copy Stack..."
pnpm desktop:dev
