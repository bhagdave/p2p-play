#!/bin/bash

# P2P Story Sharing - Test Runner Script
#
export RUSTFLAGS="-A warnings"

echo "🧪 Running P2P PLAY tests"
echo "=================================="

# Clean up any existing test database
rm -f ./test_stories.db

# Track overall exit code
exit_code=0

echo "📝 Running Unit Tests..."
TEST_DATABASE_PATH="./test_stories.db" cargo test --lib --quiet
if [ $? -ne 0 ]; then
    echo "❌ Unit tests failed"
    exit_code=1
fi

echo "🔗 Running Core Integration Tests..."
# Set test database path for integration tests
TEST_DATABASE_PATH="./test_stories.db" cargo test --test integration_tests --quiet -- --test-threads=1
if [ $? -ne 0 ]; then
    echo "❌ Core integration tests failed"
    exit_code=1
fi

echo "🌐 Running Network Protocol Integration Tests..."
# Network protocol tests - comprehensive P2P protocol testing
TEST_DATABASE_PATH="./test_stories.db" cargo test --test network_protocol_integration_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Network protocol integration tests failed"
    exit_code=1
fi

echo "👥 Running Multi-Peer Integration Tests..."
# Multi-peer interaction scenarios
TEST_DATABASE_PATH="./test_stories.db" cargo test --test multi_peer_integration_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Multi-peer integration tests failed"
    exit_code=1
fi

echo "📦 Running Message Serialization Edge Case Tests..."
# Message serialization robustness tests
TEST_DATABASE_PATH="./test_stories.db" cargo test --test message_serialization_edge_cases_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Message serialization edge case tests failed"
    exit_code=1
fi

echo "🔄 Running Network Failure Recovery Tests..."
# Network failure and recovery scenarios
TEST_DATABASE_PATH="./test_stories.db" cargo test --test network_failure_recovery_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Network failure recovery tests failed"
    exit_code=1
fi

echo "⚡ Running Performance and Load Tests..."
# Performance testing under various load conditions
TEST_DATABASE_PATH="./test_stories.db" cargo test --test performance_load_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Performance and load tests failed"
    exit_code=1
fi

echo "📡 Running Network Reconnection Tests..."
# Network tests don't need database isolation but use single thread for consistency
TEST_DATABASE_PATH="./test_stories.db" cargo test --test network_reconnection_tests --quiet -- --test-threads=2
if [ $? -ne 0 ]; then
    echo "❌ Network reconnection tests failed"
    exit_code=1
fi

echo "🔄 Running Auto-Subscription Tests..."
# Auto-subscription tests need database isolation
TEST_DATABASE_PATH="./test_stories.db" cargo test --test auto_subscription_tests --quiet -- --test-threads=1
if [ $? -ne 0 ]; then
    echo "❌ Auto-subscription tests failed"
    exit_code=1
fi

echo "💬 Running Conversation Tests..."
# Conversation database tests need database isolation  
TEST_DATABASE_PATH="./test_stories.db" cargo test --test conversation_tests --quiet -- --test-threads=1
if [ $? -ne 0 ]; then
    echo "❌ Conversation tests failed"
    exit_code=1
fi

echo "🔔 Running Message Notification Tests..."
# Message notification tests
TEST_DATABASE_PATH="./test_stories.db" cargo test --test message_notification_tests --quiet -- --test-threads=1
if [ $? -ne 0 ]; then
    echo "❌ Message notification tests failed"
    exit_code=1
fi

echo "🔄 Running Conversation Integration Tests..."
# Conversation integration tests need database isolation
TEST_DATABASE_PATH="./test_stories.db" cargo test --test conversation_integration_tests --quiet -- --test-threads=1
if [ $? -ne 0 ]; then
    echo "❌ Conversation integration tests failed"
    exit_code=1
fi

# Clean up test database after tests
rm -f ./test_stories.db

# Display final results
if [ $exit_code -eq 0 ]; then
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
else
    echo "❌ Test Suite Failed"
    echo "==================="
    echo "One or more test suites failed. Please check the output above."
fi

# Exit with the collected exit code
exit $exit_code

