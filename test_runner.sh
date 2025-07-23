#!/bin/bash

# P2P Story Sharing - Test Runner Script

echo "🧪 Running P2P PLAY tests"
echo "=================================="

# Clean up any existing test database
rm -f ./test_stories.db

echo "📝 Running Unit Tests..."
cargo test --lib --quiet

echo "🔗 Running Integration Tests..."
# Set test database path for integration tests
TEST_DATABASE_PATH="./test_stories.db" cargo test --test integration_tests --quiet -- --test-threads=1

# Clean up test database after tests
rm -f ./test_stories.db

echo "📊 Running All Tests ..."
TEST_DATABASE_PATH="./test_stories.db" cargo test -- --test-threads=1

