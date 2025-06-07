#!/bin/bash

# P2P Story Sharing - Test Runner Script

echo "🧪 Running P2P Story Sharing Tests"
echo "=================================="

# Run unit tests
echo ""
echo "📝 Running Unit Tests..."
cargo test --lib --quiet

# Run integration tests
echo ""
echo "🔗 Running Integration Tests..."
cargo test --test integration_tests --quiet

# Run all tests with detailed output
echo ""
echo "📊 Running All Tests (Detailed)..."
cargo test

echo ""
echo "✅ Test Summary:"
echo "  - Types Module: Serialization, deserialization, constructors"
echo "  - Storage Module: File I/O, story management, publishing"
echo "  - Integration Tests: Multi-peer workflows, error handling"
echo ""
echo "💡 These tests help catch:"
echo "  - Data corruption issues"
echo "  - Story ID conflicts"
echo "  - Serialization problems"
echo "  - File handling errors"
echo "  - Duplicate story handling"
echo "  - Connection state issues"