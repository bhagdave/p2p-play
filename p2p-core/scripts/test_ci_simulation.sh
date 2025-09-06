#!/bin/bash

# P2P Core - CI Environment Simulation Script
# This script simulates the CI environment locally to test how the test_runner.sh
# script will behave in GitHub Actions without config files that might exist locally.

echo "🧪 Simulating CI Environment for P2P Core"
echo "=========================================="

# Set CI environment variables that GitHub Actions sets
export CI=true
export GITHUB_ACTIONS=true
export RUNNER_OS=Linux
export RUNNER_TEMP=/tmp
export GITHUB_WORKSPACE=$(pwd)

echo "✅ Set CI environment variables:"
echo "   CI=$CI"
echo "   GITHUB_ACTIONS=$GITHUB_ACTIONS" 
echo "   RUNNER_OS=$RUNNER_OS"
echo ""

# Track files to backup and restore
backup_files=()

# Remove local config files that won't exist in CI
echo "🗂️ Backing up and removing local config files..."

if [ -f "peer_key" ]; then
    cp peer_key peer_key.bak
    backup_files+=("peer_key")
    rm peer_key
    echo "   📁 Removed peer_key (backed up)"
fi

# Remove any log files that might interfere
rm -f *.log
rm -f test_*.db

if [ ${#backup_files[@]} -eq 0 ]; then
    echo "   ℹ️ No local config files found to remove"
fi

echo ""
echo "🚀 Running p2p-core test_runner.sh in simulated CI environment..."
echo "================================================================="

# Navigate to p2p-core directory and run tests
cd p2p-core

# Run the test script
./scripts/test_runner.sh
test_exit_code=$?

# Navigate back to parent directory
cd ..

echo ""
echo "🔄 Restoring backed up files..."

# Restore backup files
restored_count=0
for file in "${backup_files[@]}"; do
    if [ -f "${file}.bak" ]; then
        mv "${file}.bak" "$file"
        echo "   📁 Restored $file"
        restored_count=$((restored_count + 1))
    fi
done

if [ $restored_count -eq 0 ]; then
    echo "   ℹ️ No files to restore"
fi

echo ""
echo "================================================="
echo "🎯 P2P Core CI Simulation Results:"
echo "================================================="

if [ $test_exit_code -eq 0 ]; then
    echo "✅ SUCCESS: P2P Core tests would PASS in GitHub Actions CI"
    echo "   All networking tests completed without errors in simulated CI environment"
else
    echo "❌ FAILURE: P2P Core tests would FAIL in GitHub Actions CI"
    echo "   Exit code: $test_exit_code"
    echo "   Check the test output above for specific failures"
    echo ""
    echo "💡 Troubleshooting tips:"
    echo "   • Check for tests that depend on config files"
    echo "   • Verify environment variable handling"
    echo "   • Consider CI-specific test conditions"
    echo "   • Review p2p-core specific networking requirements"
fi

echo ""
echo "🔧 To debug p2p-core issues:"
echo "   • Run individual tests: cd p2p-core && cargo test --test <test_name>"
echo "   • Run with verbose output: cd p2p-core && cargo test -- --nocapture"
echo "   • Check test logs and error output"

exit $test_exit_code