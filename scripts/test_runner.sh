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

# Function to run a test suite with consistent error handling
run_test_suite() {
    local test_name="$1"
    local test_command="$2"
    local thread_count="$3"
    
    echo "$test_name"
    TEST_DATABASE_PATH="./test_stories.db" $test_command --quiet -- --test-threads="$thread_count"
    if [ $? -ne 0 ]; then
        echo "❌ ${test_name#* } failed"
        exit_code=1
    fi
}

run_test_suite "Running Unit Tests..." "cargo test --lib" "1"
run_test_suite "Running Core Integration Tests..." "cargo test --test integration_tests" "1"
run_test_suite "Running Network Protocol Integration Tests..." "cargo test --test network_protocol_integration_tests" "2"
run_test_suite "Running Multi-Peer Integration Tests..." "cargo test --test multi_peer_integration_tests" "2"
run_test_suite "Running Message Serialization Edge Case Tests..." "cargo test --test message_serialization_edge_cases_tests" "2"
run_test_suite "Running Network Failure Recovery Tests..." "cargo test --test network_failure_recovery_tests" "2"
run_test_suite "Running Performance and Load Tests..." "cargo test --test performance_load_tests" "2"
run_test_suite "Running Network Reconnection Tests..." "cargo test --test network_reconnection_tests" "2"
run_test_suite "Running Auto-Subscription Tests..." "cargo test --test auto_subscription_tests" "1"
run_test_suite "Running Conversation Tests..." "cargo test --test conversation_tests" "1"
run_test_suite "Running Message Notification Tests..." "cargo test --test message_notification_tests" "1"
run_test_suite "Running Conversation Integration Tests..." "cargo test --test conversation_integration_tests" "1"
run_test_suite "Running Bootstrap Logger Tests..." "cargo test --test bootstrap_logger_tests" "1"
run_test_suite "Running Ping Config Tests..." "cargo test --test ping_config_tests" "1"
run_test_suite "Running Crypto Integration Tests..." "cargo test --test crypto_integration_tests" "2"
run_test_suite "Running Chronological Stories Tests..." "cargo test --test chronological_stories_test" "1"
run_test_suite "Running Scroll Lock Toggle Tests..." "cargo test --test scrolllock_toggle_tests" "1"
run_test_suite "Running Circuit Breaker Tests..." "cargo test --test circuit_breaker_tests" "2"
run_test_suite "Running Search Tests..." "cargo test --test search_tests" "1"
run_test_suite "Running Enhanced Messaging Tests..." "cargo test --test enhanced_messaging_tests" "1"
run_test_suite "Running Story Sync Tests..." "cargo test --test story_sync_tests" "1"
run_test_suite "Running Simple Conversation Tests..." "cargo test --test conversation_simple_tests" "1"
run_test_suite "Running Crypto Error Tests..." "cargo test --test crypto_error_test" "1"
run_test_suite "Running UI Tests..." "cargo test --test ui_tests" "1"
run_test_suite "Running Handshake Protocol Tests..." "cargo test --test handshake_protocol_tests" "1"
run_test_suite "Running Channel Sync Tests..." "cargo test --test channel_sync_tests" "1"

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

