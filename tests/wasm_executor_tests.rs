use p2p_play::content_fetcher::ContentFetcher;
use p2p_play::errors::FetchError;
use p2p_play::wasm_executor::{ExecutionRequest, WasmExecutionError, WasmExecutor, validate_wasm};
use std::sync::Arc;

/// Mock ContentFetcher for testing
struct MockContentFetcher {
    /// The data to return when fetch is called
    data: Vec<u8>,
    /// Whether to simulate a fetch error
    should_fail: bool,
}

impl MockContentFetcher {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            should_fail: false,
        }
    }

    fn with_error() -> Self {
        Self {
            data: Vec::new(),
            should_fail: true,
        }
    }
}

impl ContentFetcher for MockContentFetcher {
    async fn fetch(&self, _cid: &str) -> Result<Vec<u8>, FetchError> {
        if self.should_fail {
            return Err(FetchError::NotFound("test-cid".to_string()));
        }
        Ok(self.data.clone())
    }

    async fn resolve_ipns(&self, _name: &str) -> Result<String, FetchError> {
        Ok("QmTest123".to_string())
    }
}

/// Create a minimal valid WASM module that does nothing
fn create_minimal_wasm() -> Vec<u8> {
    // Use WAT to create a valid WASM module
    wat::parse_str(
        r#"
        (module
            (func $main)
            (export "_start" (func $main))
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

/// Create a WASM module that writes to stdout
fn create_stdout_wasm() -> Vec<u8> {
    // A WASM module that writes "Hello\n" to stdout using WASI
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            
            (memory 1)
            (export "memory" (memory 0))
            
            ;; Store "Hello\n" at offset 0
            (data (i32.const 0) "Hello\n")
            
            (func $main
                ;; Create iovec at offset 8
                (i32.store (i32.const 8) (i32.const 0))  ;; iov.buf = 0
                (i32.store (i32.const 12) (i32.const 6)) ;; iov.len = 6
                
                ;; Call fd_write(1, 8, 1, 16)
                ;; fd=1 (stdout), iovs=8, iovs_len=1, nwritten=16
                (call $fd_write
                    (i32.const 1)   ;; fd (stdout)
                    (i32.const 8)   ;; iovs
                    (i32.const 1)   ;; iovs_len
                    (i32.const 16)) ;; nwritten pointer
                drop
            )
            
            (export "_start" (func $main))
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

/// Create a WASM module that consumes a lot of fuel
fn create_fuel_heavy_wasm() -> Vec<u8> {
    // A module that runs a large finite loop to consume fuel
    wat::parse_str(
        r#"
        (module
            (func $main
                (local $i i32)
                (local.set $i (i32.const 0))
                (loop $continue
                    ;; Increment counter
                    (local.set $i (i32.add (local.get $i) (i32.const 1)))
                    ;; Continue if i < 1000000
                    (br_if $continue (i32.lt_u (local.get $i) (i32.const 1000000)))
                )
            )
            (export "_start" (func $main))
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

/// Create a WASM module with a very long-running loop for timeout testing
fn create_long_running_wasm() -> Vec<u8> {
    // A module that runs for a very long time
    wat::parse_str(
        r#"
        (module
            (func $main
                (local $i i64)
                (local.set $i (i64.const 0))
                (loop $continue
                    ;; Increment counter
                    (local.set $i (i64.add (local.get $i) (i64.const 1)))
                    ;; Continue if i < a very large number
                    (br_if $continue (i64.lt_u (local.get $i) (i64.const 100000000000)))
                )
            )
            (export "_start" (func $main))
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

/// Create a WASM module that reads from stdin and writes to stdout
fn create_stdin_echo_wasm() -> Vec<u8> {
    // A WASM module that reads from stdin and echoes to stdout
    wat::parse_str(
        r#"
        (module
            (import "wasi_snapshot_preview1" "fd_read"
                (func $fd_read (param i32 i32 i32 i32) (result i32)))
            (import "wasi_snapshot_preview1" "fd_write"
                (func $fd_write (param i32 i32 i32 i32) (result i32)))
            
            (memory 1)
            (export "memory" (memory 0))
            
            (func $main
                ;; Read from stdin (fd=0) into buffer at offset 0
                ;; Create iovec at offset 100
                (i32.store (i32.const 100) (i32.const 0))   ;; iov.buf = 0
                (i32.store (i32.const 104) (i32.const 64))  ;; iov.len = 64
                
                ;; Call fd_read(0, 100, 1, 108)
                ;; fd=0 (stdin), iovs=100, iovs_len=1, nread=108
                (call $fd_read
                    (i32.const 0)    ;; fd (stdin)
                    (i32.const 100)  ;; iovs
                    (i32.const 1)    ;; iovs_len
                    (i32.const 108)) ;; nread pointer
                drop
                
                ;; Write to stdout (fd=1) from buffer at offset 0
                ;; Create iovec at offset 112
                (i32.store (i32.const 112) (i32.const 0))   ;; iov.buf = 0
                (i32.store (i32.const 116) (i32.const 64))  ;; iov.len = 64
                
                ;; Call fd_write(1, 112, 1, 120)
                (call $fd_write
                    (i32.const 1)    ;; fd (stdout)
                    (i32.const 112)  ;; iovs
                    (i32.const 1)    ;; iovs_len
                    (i32.const 120)) ;; nwritten pointer
                drop
            )
            
            (export "_start" (func $main))
        )
        "#,
    )
    .expect("Failed to parse WAT")
}

#[tokio::test]
async fn test_executor_creation() {
    let fetcher = Arc::new(MockContentFetcher::new(vec![]));
    let executor = WasmExecutor::new(fetcher);
    assert!(executor.is_ok());
}

#[tokio::test]
async fn test_execute_minimal_wasm_success() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_ok());
    let execution_result = result.unwrap();
    assert_eq!(execution_result.exit_code, 0);
    assert!(execution_result.fuel_consumed > 0);
}

#[tokio::test]
async fn test_execute_fetch_failure() {
    let fetcher = Arc::new(MockContentFetcher::with_error());
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::FetchFailed(_) => {}
        e => panic!("Expected FetchFailed error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_invalid_wasm() {
    // Invalid WASM: wrong magic bytes
    let invalid_wasm = vec![0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
    let fetcher = Arc::new(MockContentFetcher::new(invalid_wasm));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::InvalidWasm { .. } => {}
        e => panic!("Expected InvalidWasm error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_compilation_failure() {
    // Valid header but invalid WASM body
    let invalid_wasm = vec![
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        0xFF, 0xFF, 0xFF, 0xFF, // garbage data
    ];
    let fetcher = Arc::new(MockContentFetcher::new(invalid_wasm));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::CompilationFailed(_) => {}
        e => panic!("Expected CompilationFailed error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_fuel_consumption_tracking() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_fuel_limit(1_000_000);
    let result = executor.execute(request).await;

    assert!(result.is_ok());
    let execution_result = result.unwrap();
    // Fuel should be consumed (even for minimal WASM)
    assert!(execution_result.fuel_consumed > 0);
    // Fuel consumed should not exceed the limit
    assert!(execution_result.fuel_consumed <= 1_000_000);
}

#[tokio::test]
async fn test_execute_fuel_exhaustion() {
    let wasm_bytes = create_fuel_heavy_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    // Set very low fuel limit to trigger exhaustion
    let request = ExecutionRequest::new("test-cid".to_string()).with_fuel_limit(100);
    let result = executor.execute(request).await;

    // The execution should fail due to fuel exhaustion or execution error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        WasmExecutionError::FuelExhausted { consumed } => {
            assert!(consumed > 0);
        }
        WasmExecutionError::ExecutionFailed(msg) => {
            // Wasmtime may report fuel exhaustion differently depending on the scenario
            // Accept ExecutionFailed only if it seems related to resource exhaustion
            eprintln!("Got ExecutionFailed (acceptable): {}", msg);
            // Verify this is a legitimate execution failure, not something else
            assert!(!msg.contains("NotFound") && !msg.contains("invalid"));
        }
        e => panic!("Expected FuelExhausted or ExecutionFailed, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_timeout() {
    let wasm_bytes = create_long_running_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    // Set high fuel but very short timeout
    let request = ExecutionRequest::new("test-cid".to_string())
        .with_fuel_limit(100_000_000)
        .with_timeout_secs(Some(1));
    let result = executor.execute(request).await;

    // The execution should fail due to timeout or execution error
    assert!(result.is_err());
    let err = result.unwrap_err();
    match err {
        WasmExecutionError::ExecutionTimeout => {}
        WasmExecutionError::ExecutionFailed(msg) => {
            // Wasmtime may report timeout-related issues differently
            // Accept ExecutionFailed only if it's a legitimate execution issue
            eprintln!("Got ExecutionFailed (acceptable): {}", msg);
            // Verify this is a legitimate execution failure, not something else
            assert!(!msg.contains("NotFound") && !msg.contains("invalid"));
        }
        e => panic!("Expected ExecutionTimeout or ExecutionFailed, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_without_timeout() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    // Execute without timeout
    let request = ExecutionRequest::new("test-cid".to_string()).with_timeout_secs(None);
    let result = executor.execute(request).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_stdout_capture() {
    let wasm_bytes = create_stdout_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_ok());
    let execution_result = result.unwrap();

    // Check that stdout was captured and contains "Hello"
    assert!(!execution_result.stdout.is_empty());
    let stdout_str = String::from_utf8_lossy(&execution_result.stdout);
    assert!(stdout_str.contains("Hello"));
}

#[tokio::test]
async fn test_execute_stdin_input() {
    let wasm_bytes = create_stdin_echo_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let input_data = b"test input data".to_vec();
    let request = ExecutionRequest::new("test-cid".to_string()).with_input(input_data.clone());
    let result = executor.execute(request).await;

    // Verify execution succeeds with input
    assert!(result.is_ok());
    let execution_result = result.unwrap();

    // Verify that the input was echoed to stdout
    let stdout_str = String::from_utf8_lossy(&execution_result.stdout);
    assert!(stdout_str.contains("test input"));
}

#[tokio::test]
async fn test_execute_with_args() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let args = vec!["arg1".to_string(), "arg2".to_string()];
    let request = ExecutionRequest::new("test-cid".to_string()).with_args(args);
    let result = executor.execute(request).await;

    // Verify execution succeeds with args
    // Note: The minimal WASM module doesn't actually read args, so this test
    // only verifies that the executor accepts args without errors.
    // A more comprehensive test would require a WASM module that reads and
    // validates command-line arguments, but that's complex in WASI.
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_entry_point_not_found() {
    // Create a WASM module without _start export
    let wasm_no_start = wat::parse_str(
        r#"
        (module
            (func $main)
            ;; No export of _start
        )
        "#,
    )
    .expect("Failed to parse WAT");

    let fetcher = Arc::new(MockContentFetcher::new(wasm_no_start));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::EntryPointNotFound => {}
        e => panic!("Expected EntryPointNotFound error, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_all_builder_options() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string())
        .with_input(b"input".to_vec())
        .with_fuel_limit(5_000_000)
        .with_memory_limit_mb(32)
        .with_timeout_secs(Some(10))
        .with_args(vec!["test".to_string()]);

    let result = executor.execute(request).await;

    assert!(result.is_ok());
    let execution_result = result.unwrap();
    assert_eq!(execution_result.exit_code, 0);
    assert!(execution_result.fuel_consumed > 0);
    assert!(execution_result.fuel_consumed <= 5_000_000);
}

#[tokio::test]
async fn test_execute_multiple_executions() {
    let wasm_bytes = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(wasm_bytes));
    let executor = WasmExecutor::new(fetcher).unwrap();

    // Execute same module multiple times to test executor reusability
    for _ in 0..3 {
        let request = ExecutionRequest::new("test-cid".to_string());
        let result = executor.execute(request).await;
        assert!(result.is_ok());
    }
}

#[tokio::test]
async fn test_validate_wasm_integration() {
    let valid_wasm = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(valid_wasm.clone()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    // Validate the same WASM bytes that will be executed
    let validation_result = validate_wasm(&valid_wasm);
    assert!(validation_result.is_ok());

    // Execute should also succeed
    let request = ExecutionRequest::new("test-cid".to_string());
    let result = executor.execute(request).await;
    assert!(result.is_ok());
}
