#!/bin/bash

# P2P Story Sharing - Test Runner Script

echo "🧪 Running P2P PLAY tests to generate coverage report"
echo "=================================="

# Clean up any existing test database
rm -f ./test_stories.db

echo "📊 Running All Tests..."
TEST_DATABASE_PATH="./test_stories.db" cargo tarpaulin -o Html --config tarpaulin.toml -- --test-threads=1


