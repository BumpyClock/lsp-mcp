#!/usr/bin/env pwsh
#Requires -Version 7.0

param(
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# ANSI color codes
$GREEN = "`e[0;32m"
$BLUE = "`e[0;34m"
$YELLOW = "`e[1;33m"
$RED = "`e[0;31m"
$NC = "`e[0m"  # No Color

# Get the directory where the script is located
$ScriptDir = $PSScriptRoot
$BinaryName = "lsp-mcp.exe"
$TargetDir = Join-Path $ScriptDir "target\release"
$BinaryPath = Join-Path $TargetDir $BinaryName
$InstallDir = Join-Path $env:USERPROFILE ".local\bin"
$SymlinkPath = Join-Path $InstallDir $BinaryName

Write-Host ""
Write-Host "${BLUE}========================================${NC}"
Write-Host "${BLUE}  LSP-MCP Build and Install Script${NC}"
Write-Host "${BLUE}========================================${NC}"
Write-Host ""

if ($SkipBuild) {
    Write-Host "${YELLOW}Skipping build (-SkipBuild flag set)${NC}"
    Write-Host ""
} else {
    # Check if cargo is installed
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "${RED}Error: Cargo is not installed. Please install Rust from https://rustup.rs/${NC}"
        exit 1
    }

    # Build the project
    Write-Host "${BLUE}Building lsp-mcp (release mode)...${NC}"
    Push-Location $ScriptDir
    try {
        cargo build --release
    } finally {
        Pop-Location
    }

    if (-not (Test-Path $BinaryPath)) {
        Write-Host "${RED}Error: Build succeeded but binary not found at ${BinaryPath}${NC}"
        exit 1
    }

    Write-Host "${GREEN}Build successful!${NC}"
    Write-Host ""
}

if (-not (Test-Path $BinaryPath)) {
    Write-Host "${RED}Error: Binary not found at ${BinaryPath}${NC}"
    Write-Host "${YELLOW}Hint: Run without -SkipBuild to build first, or build manually with 'cargo build --release'${NC}"
    exit 1
}

# Create install directory if it doesn't exist
Write-Host "${BLUE}Setting up installation directory...${NC}"
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

# Remove existing symlink if present
if (Test-Path $SymlinkPath) {
    $item = Get-Item $SymlinkPath
    if ($item.LinkType -eq "SymbolicLink") {
        Write-Host "${YELLOW}Removing existing symlink...${NC}"
        Remove-Item $SymlinkPath -Force
    } elseif ($item.PSIsContainer -eq $false) {
        Write-Host "${RED}Error: ${SymlinkPath} exists but is not a symlink${NC}"
        Write-Host "${RED}Please remove it manually before continuing${NC}"
        exit 1
    }
}

# Create symlink
Write-Host "${BLUE}Creating symlink...${NC}"
try {
    New-Item -ItemType SymbolicLink -Path $SymlinkPath -Target $BinaryPath | Out-Null
} catch {
    Write-Host "${RED}Error: Failed to create symlink${NC}"
    Write-Host ""
    Write-Host "${YELLOW}Creating symbolic links on Windows requires either:${NC}"
    Write-Host "  1. Administrator privileges (run PowerShell as Administrator)"
    Write-Host "  2. Developer Mode enabled in Windows Settings"
    Write-Host ""
    Write-Host "${YELLOW}To enable Developer Mode:${NC}"
    Write-Host "  1. Open Settings > Update & Security > For developers"
    Write-Host "  2. Turn on 'Developer Mode'"
    Write-Host ""
    Write-Host "${YELLOW}Alternatively, run this script as Administrator.${NC}"
    exit 1
}

# Verify installation
if (Test-Path $SymlinkPath) {
    $symlinkItem = Get-Item $SymlinkPath
    if ($symlinkItem.LinkType -eq "SymbolicLink") {
        Write-Host "${GREEN}Symlink created successfully!${NC}"
        Write-Host ""
        Write-Host "${GREEN}========================================${NC}"
        Write-Host "${GREEN}  Installation Complete!${NC}"
        Write-Host "${GREEN}========================================${NC}"
        Write-Host ""
        Write-Host "Binary location: ${BinaryPath}"
        Write-Host "Symlink location: ${SymlinkPath}"
        Write-Host ""

        # Check if ~/.local/bin is in PATH
        $pathEnv = [Environment]::GetEnvironmentVariable("Path", "User")
        $pathEntries = $pathEnv -split ';'
        $installDirInPath = $pathEntries -contains $InstallDir

        if (-not $installDirInPath) {
            Write-Host "${YELLOW}Warning: ${InstallDir} is not in your PATH${NC}"
            Write-Host ""
            Write-Host "${YELLOW}To add it permanently, run:${NC}"
            Write-Host ""
            Write-Host "  ${BLUE}[Environment]::SetEnvironmentVariable('Path', \`"${InstallDir};\`$env:Path\`", 'User')${NC}"
            Write-Host ""
            Write-Host "${YELLOW}Then restart your terminal for changes to take effect.${NC}"
            Write-Host ""
        } else {
            Write-Host "${GREEN}You can now run: lsp-mcp${NC}"
        }
    } else {
        Write-Host "${RED}Error: Failed to create symlink${NC}"
        exit 1
    }
} else {
    Write-Host "${RED}Error: Failed to create symlink${NC}"
    exit 1
}
