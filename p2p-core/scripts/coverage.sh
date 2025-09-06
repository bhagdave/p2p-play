#!/bin/bash

# P2P Core - Test Coverage Script
# Generates test coverage reports for the p2p-core networking library

echo "📊 Generating P2P Core test coverage report"
echo "==========================================="

# Clean up previous coverage artifacts
rm -rf ./coverage/
rm -f ./tarpaulin-report.html

# Create coverage directory
mkdir -p ./coverage/

echo ""
echo "🔍 Running tests with coverage analysis..."

# Check if tarpaulin is installed
if ! command -v cargo-tarpaulin &> /dev/null; then
    echo "⚠️  cargo-tarpaulin not found. Installing..."
    cargo install cargo-tarpaulin
    echo ""
fi

# Set environment variables for consistent testing
export RUSTFLAGS="-A warnings"
export RUST_BACKTRACE=1

# Run tarpaulin with configuration
cargo tarpaulin \
    --config tarpaulin.toml \
    --verbose \
    --ignore-panics \
    --timeout 300 \
    --out Html \
    --out Lcov \
    --out Xml \
    --output-dir ./coverage/ \
    -- --test-threads=1

coverage_exit_code=$?

echo ""
echo "📈 Coverage report generated!"
echo "============================"

if [ $coverage_exit_code -eq 0 ]; then
    echo "✅ Coverage analysis completed successfully"
    echo ""
    echo "📁 Generated reports:"
    echo "   • HTML report: ./coverage/tarpaulin-report.html"
    echo "   • LCOV report: ./coverage/lcov.info"
    echo "   • XML report:  ./coverage/cobertura.xml"
    echo ""
    echo "🌐 To view HTML report:"
    echo "   Open ./coverage/tarpaulin-report.html in your browser"
    echo ""
    echo "📊 Coverage summary:"
    if [ -f "./coverage/tarpaulin-report.html" ]; then
        # Try to extract coverage percentage from HTML report
        if command -v grep &> /dev/null && command -v sed &> /dev/null; then
            coverage_line=$(grep -o "coverage-rate.*%" ./coverage/tarpaulin-report.html | head -1 || echo "")
            if [ -n "$coverage_line" ]; then
                percentage=$(echo "$coverage_line" | sed 's/.*>\([0-9.]*\)%.*/\1/')
                echo "   Code coverage: ${percentage}%"
            fi
        fi
    fi
else
    echo "❌ Coverage analysis failed"
    echo "   Exit code: $coverage_exit_code"
    echo ""
    echo "🔧 Troubleshooting:"
    echo "   • Check that all tests pass first: cargo test"
    echo "   • Ensure tarpaulin is properly installed: cargo install cargo-tarpaulin"
    echo "   • Review test output for specific errors"
fi

echo ""
echo "💡 Next steps:"
echo "   • Review coverage report to identify untested code"
echo "   • Add tests for uncovered networking functionality"
echo "   • Set coverage targets for CI/CD pipelines"

exit $coverage_exit_code