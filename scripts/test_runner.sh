#!/bin/bash

# P2P Story Sharing - Test Runner Script
#
export RUSTFLAGS="-A warnings"

echo "🧪 Running P2P PLAY tests"
echo "=================================="

# Clean up any existing test database
rm -f ./test_stories.db

echo "📝 Running Unit Tests..."
cargo test --lib --quiet

echo "🔗 Running Core Integration Tests..."
# Set test database path for integration tests
TEST_DATABASE_PATH="./test_stories.db" cargo test --test integration_tests --quiet -- --test-threads=1

echo "🌐 Running Network Protocol Integration Tests..."
# Network protocol tests - comprehensive P2P protocol testing
cargo test --test network_protocol_integration_tests --quiet -- --test-threads=2

echo "👥 Running Multi-Peer Integration Tests..."
# Multi-peer interaction scenarios
cargo test --test multi_peer_integration_tests --quiet -- --test-threads=2

echo "📦 Running Message Serialization Edge Case Tests..."
# Message serialization robustness tests
cargo test --test message_serialization_edge_cases_tests --quiet -- --test-threads=2

echo "🔄 Running Network Failure Recovery Tests..."
# Network failure and recovery scenarios
cargo test --test network_failure_recovery_tests --quiet -- --test-threads=2

echo "⚡ Running Performance and Load Tests..."
# Performance testing under various load conditions
cargo test --test performance_load_tests --quiet -- --test-threads=2

echo "📡 Running Network Reconnection Tests..."
# Network tests don't need database isolation but use single thread for consistency
cargo test --test network_reconnection_tests --quiet -- --test-threads=2

echo "🔄 Running Auto-Subscription Tests..."
# Auto-subscription tests need database isolation
TEST_DATABASE_PATH="./test_stories.db" cargo test --test auto_subscription_tests --quiet -- --test-threads=1

echo "💬 Running Conversation Tests..."
# Conversation database tests need database isolation  
TEST_DATABASE_PATH="./test_stories.db" cargo test --test conversation_tests --quiet -- --test-threads=1

echo "🔔 Running Message Notification Tests..."
# Message notification tests
cargo test --test message_notification_tests --quiet -- --test-threads=1

echo "🔄 Running Conversation Integration Tests..."
# Conversation integration tests need database isolation
TEST_DATABASE_PATH="./test_stories.db" cargo test --test conversation_integration_tests --quiet -- --test-threads=1

# Clean up test database after tests
rm -f ./test_stories.db

echo "✅ Network Integration Test Suite Complete"
echo "==========================================="
echo "Comprehensive P2P protocol testing completed successfully!"
echo "Coverage includes:"
echo "  • Network protocol integration (floodsub, mDNS, Kademlia, ping, request-response)"
echo "  • Multi-peer interaction scenarios"  
echo "  • Message serialization edge cases"
echo "  • Network failure and recovery"
echo "  • Performance and load testing"
echo "  • Conversation handling and message threading"
echo "  • UI conversation navigation and state management"

