#!/bin/bash

# P2P Story Sharing - CI Environment Simulation Script
# This script simulates the CI environment locally to test how the test_runner.sh
# script will behave in GitHub Actions without the config files that exist locally.

echo "🧪 Simulating CI Environment"
echo "=============================="

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

if [ -f "unified_network_config.json" ]; then
    cp unified_network_config.json unified_network_config.json.bak
    backup_files+=("unified_network_config.json")
    rm unified_network_config.json
    echo "   📁 Removed unified_network_config.json (backed up)"
fi

if [ -f "bootstrap_config.json" ]; then
    cp bootstrap_config.json bootstrap_config.json.bak  
    backup_files+=("bootstrap_config.json")
    rm bootstrap_config.json
    echo "   📁 Removed bootstrap_config.json (backed up)"
fi

if [ -f "node_description.txt" ]; then
    cp node_description.txt node_description.txt.bak
    backup_files+=("node_description.txt")
    rm node_description.txt
    echo "   📁 Removed node_description.txt (backed up)"
fi

if [ -f "peer_key" ]; then
    cp peer_key peer_key.bak
    backup_files+=("peer_key")
    rm peer_key
    echo "   📁 Removed peer_key (backed up)"
fi

if [ -f "stories.db" ]; then
    cp stories.db stories.db.bak
    backup_files+=("stories.db")
    rm stories.db
    echo "   📁 Removed stories.db (backed up)"
fi

# Remove any log files that might interfere
rm -f *.log
rm -f test_stories.db
rm -f tarpaulin-report.html

if [ ${#backup_files[@]} -eq 0 ]; then
    echo "   ℹ️ No local config files found to remove"
fi

echo ""
echo "🚀 Running test_runner.sh in simulated CI environment..."
echo "=============================="

# Run the test script
./scripts/test_runner.sh
test_exit_code=$?

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
echo "=============================="
echo "🎯 CI Simulation Results:"
echo "=============================="

if [ $test_exit_code -eq 0 ]; then
    echo "✅ SUCCESS: Tests would PASS in GitHub Actions CI"
    echo "   All tests completed without errors in simulated CI environment"
else
    echo "❌ FAILURE: Tests would FAIL in GitHub Actions CI"
    echo "   Exit code: $test_exit_code"
    echo "   Check the test output above for specific failures"
    echo ""
    echo "💡 Troubleshooting tips:"
    echo "   • Look for tests that depend on config files"
    echo "   • Check for hard-coded file paths"
    echo "   • Verify environment variable handling"
    echo "   • Consider CI-specific test conditions"
fi

echo ""
echo "🔧 To debug specific issues:"
echo "   • Run individual tests: cargo test --test <test_name>"
echo "   • Use Act for Docker-based testing: act -W .github/workflows/rust.yml"
echo "   • Check GitHub Actions logs for comparison"

exit $test_exit_code