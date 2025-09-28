#!/bin/bash

# Create workspace structure for separating P2P-Play into network crate and frontend

echo "Creating workspace structure..."

# Create network crate directory structure
mkdir -p p2p-network/src
mkdir -p p2p-network/tests

# Create frontend crate directory structure
mkdir -p p2p-play-frontend/src
mkdir -p p2p-play-frontend/tests

# Copy current source files to frontend as starting point
echo "Copying source files to frontend..."
cp -r src/* p2p-play-frontend/src/ 2>/dev/null || true
cp -r tests/* p2p-play-frontend/tests/ 2>/dev/null || true

# Create network crate tests (subset)
echo "Setting up network crate tests..."
cp tests/network_tests.rs p2p-network/tests/ 2>/dev/null || true
cp tests/crypto_*.rs p2p-network/tests/ 2>/dev/null || true  
cp tests/bootstrap_*.rs p2p-network/tests/ 2>/dev/null || true
cp tests/circuit_breaker_*.rs p2p-network/tests/ 2>/dev/null || true
cp tests/relay_*.rs p2p-network/tests/ 2>/dev/null || true

echo "Workspace structure created successfully!"
echo ""
echo "Next steps:"
echo "1. Create p2p-network/Cargo.toml"
echo "2. Create p2p-play-frontend/Cargo.toml" 
echo "3. Implement network crate public API"
echo "4. Refactor modules between crates"
echo "5. Update imports and dependencies"