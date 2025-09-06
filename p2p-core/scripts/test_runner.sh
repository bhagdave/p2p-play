#!/bin/bash

# P2P Core - Test Runner Script
# Runs all tests for the p2p-core networking library

export RUSTFLAGS="-A warnings"

echo "🧪 Running P2P Core tests"
echo "=========================="

# Clean up any existing test artifacts
rm -f ./test_*.db
rm -f ./peer_key

# Track overall exit code
exit_code=0

# Function to run a test suite with consistent error handling
run_test_suite() {
    local test_name="$1"
    local test_command="$2"
    local thread_count="$3"
    
    echo "$test_name"
    $test_command --quiet -- --test-threads="$thread_count"
    if [ $? -ne 0 ]; then
        echo "❌ ${test_name#* } failed"
        exit_code=1
    fi
}

# Core p2p-core tests
run_test_suite "🔒 Running Crypto Integration Tests..." "cargo test --test crypto_integration_tests" "1"
run_test_suite "⚙️  Running Network Configuration Tests..." "cargo test --test network_config_tests" "1"  
run_test_suite "🔗 Running Network Integration Tests..." "cargo test --test network_integration_tests" "2"
run_test_suite "🚀 Running Bootstrap Tests..." "cargo test --test bootstrap_tests" "1"
run_test_suite "⚡ Running Circuit Breaker Tests..." "cargo test --test circuit_breaker_tests" "2"
run_test_suite "📡 Running Relay Tests..." "cargo test --test relay_tests" "1"

# Unit tests from the library
run_test_suite "📝 Running Library Unit Tests..." "cargo test --lib" "1"

# Clean up test artifacts
rm -f ./test_*.db
rm -f ./peer_key

# Display final results
if [ $exit_code -eq 0 ]; then
    echo "✅ P2P Core Test Suite Complete"
    echo "=============================="
    echo "All p2p-core networking tests passed successfully!"
    echo "Coverage includes:"
    echo "  • Crypto service encryption/decryption"
    echo "  • Network configuration and validation"
    echo "  • Network service integration"
    echo "  • Bootstrap peer discovery"
    echo "  • Circuit breaker resilience"
    echo "  • Message relay functionality"
    echo "  • Core networking protocols"
else
    echo "❌ P2P Core Test Suite Failed"
    echo "============================="
    echo "One or more test suites failed. Please check the output above."
fi

# Exit with the collected exit code
exit $exit_code