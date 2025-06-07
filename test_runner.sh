#!/bin/bash

# P2P Story Sharing - Test Runner Script

echo "🧪 Running P2P PLAY tests"
echo "=================================="

echo "📝 Running Unit Tests..."
cargo test --lib --quiet

echo "🔗 Running Integration Tests..."
cargo test --test integration_tests --quiet

#echo "📊 Running All Tests (Detailed)..."
#cargo test

