#!/usr/bin/env bash
set -e

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Get the directory where the script is located
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINARY_NAME="lsp-mcp"
TARGET_DIR="${SCRIPT_DIR}/target/release"
BINARY_PATH="${TARGET_DIR}/${BINARY_NAME}"
INSTALL_DIR="${HOME}/.local/bin"
SYMLINK_PATH="${INSTALL_DIR}/${BINARY_NAME}"

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}  LSP-MCP Build and Install Script${NC}"
echo -e "${BLUE}========================================${NC}"
echo

# Check if cargo is installed
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}Error: Cargo is not installed. Please install Rust from https://rustup.rs/${NC}"
    exit 1
fi

# Build the project
echo -e "${BLUE}Building lsp-mcp (release mode)...${NC}"
cd "${SCRIPT_DIR}"
cargo build --release

if [ ! -f "${BINARY_PATH}" ]; then
    echo -e "${RED}Error: Build succeeded but binary not found at ${BINARY_PATH}${NC}"
    exit 1
fi

echo -e "${GREEN}Build successful!${NC}"
echo

# Create install directory if it doesn't exist
echo -e "${BLUE}Setting up installation directory...${NC}"
mkdir -p "${INSTALL_DIR}"

# Remove existing symlink if present
if [ -L "${SYMLINK_PATH}" ]; then
    echo -e "${YELLOW}Removing existing symlink...${NC}"
    rm "${SYMLINK_PATH}"
elif [ -f "${SYMLINK_PATH}" ]; then
    echo -e "${RED}Error: ${SYMLINK_PATH} exists but is not a symlink${NC}"
    echo -e "${RED}Please remove it manually before continuing${NC}"
    exit 1
fi

# Create symlink
echo -e "${BLUE}Creating symlink...${NC}"
ln -s "${BINARY_PATH}" "${SYMLINK_PATH}"

# Verify installation
if [ -L "${SYMLINK_PATH}" ] && [ -e "${SYMLINK_PATH}" ]; then
    echo -e "${GREEN}Symlink created successfully!${NC}"
    echo
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}  Installation Complete!${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo
    echo -e "Binary location: ${BINARY_PATH}"
    echo -e "Symlink location: ${SYMLINK_PATH}"
    echo

    # Check if ~/.local/bin is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo -e "${YELLOW}Warning: ${INSTALL_DIR} is not in your PATH${NC}"
        echo -e "${YELLOW}Add the following line to your shell configuration file:${NC}"
        echo
        echo -e "  ${BLUE}export PATH=\"\$HOME/.local/bin:\$PATH\"${NC}"
        echo
        echo -e "${YELLOW}Then reload your shell or run:${NC}"
        echo -e "  ${BLUE}source ~/.bashrc${NC}  (or ~/.zshrc for zsh)"
        echo
    else
        echo -e "${GREEN}You can now run: ${BINARY_NAME}${NC}"
    fi
else
    echo -e "${RED}Error: Failed to create symlink${NC}"
    exit 1
fi
