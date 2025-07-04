@echo off
chcp 65001 >nul

echo 🚀 Starting Copy Stack Desktop Application...

REM Check if pnpm is installed
where pnpm >nul 2>nul
if %errorlevel% neq 0 (
    echo ❌ pnpm is not installed. Please install pnpm first:
    echo    npm install -g pnpm
    pause
    exit /b 1
)

REM Check if Rust is installed
where cargo >nul 2>nul
if %errorlevel% neq 0 (
    echo ❌ Rust is not installed. Please install Rust first:
    echo    Visit https://rustup.rs/
    pause
    exit /b 1
)

REM Install dependencies if needed
if not exist "node_modules" (
    echo 📦 Installing dependencies...
    pnpm install
)

REM Start the desktop application
echo 🖥️  Launching Copy Stack...
pnpm desktop:dev

pause 