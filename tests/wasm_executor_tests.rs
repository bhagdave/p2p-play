mod common;

use common::wasm_fixtures::{
    MockContentFetcher, create_fuel_heavy_wasm, create_long_running_wasm, create_minimal_wasm,
    create_stdin_echo_wasm, create_stdout_wasm,
};
use p2p_play::wasm_executor::{
    ExecutionRequest, WasmExecutionError, WasmExecutor, WasmExecutorConfig, validate_wasm,
};
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// validate_wasm
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_validate_wasm_integration() {
    let valid_wasm = create_minimal_wasm();
    let fetcher = Arc::new(MockContentFetcher::new(valid_wasm.clone()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    assert!(validate_wasm(&valid_wasm).is_ok());

    let request = ExecutionRequest::new("test-cid".to_string());
    assert!(executor.execute(request).await.is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// Executor creation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_executor_creation() {
    let fetcher = Arc::new(MockContentFetcher::new(vec![]));
    assert!(WasmExecutor::new(fetcher).is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// Basic execution
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_minimal_wasm_success() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await;

    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.exit_code, 0);
    assert!(r.fuel_consumed > 0);
}

#[tokio::test]
async fn test_execute_without_timeout() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_timeout_secs(None);
    assert!(executor.execute(request).await.is_ok());
}

#[tokio::test]
async fn test_execute_all_builder_options() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string())
        .with_input(b"input".to_vec())
        .with_fuel_limit(5_000_000)
        .with_memory_limit_mb(32)
        .with_timeout_secs(Some(10))
        .with_args(vec!["test".to_string()]);

    let r = executor.execute(request).await.unwrap();
    assert_eq!(r.exit_code, 0);
    assert!(r.fuel_consumed > 0);
    assert!(r.fuel_consumed <= 5_000_000);
}

#[tokio::test]
async fn test_execute_multiple_executions() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    for _ in 0..3 {
        let result = executor
            .execute(ExecutionRequest::new("test-cid".to_string()))
            .await;
        assert!(result.is_ok());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// I/O
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_stdout_capture() {
    let fetcher = Arc::new(MockContentFetcher::new(create_stdout_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let r = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await
        .unwrap();

    assert!(!r.stdout.is_empty());
    assert!(String::from_utf8_lossy(&r.stdout).contains("Hello"));
}

#[tokio::test]
async fn test_execute_stdin_input() {
    let fetcher = Arc::new(MockContentFetcher::new(create_stdin_echo_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request =
        ExecutionRequest::new("test-cid".to_string()).with_input(b"test input data".to_vec());

    let r = executor.execute(request).await.unwrap();
    assert!(String::from_utf8_lossy(&r.stdout).contains("test input"));
}

#[tokio::test]
async fn test_execute_with_args() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string())
        .with_args(vec!["arg1".to_string(), "arg2".to_string()]);

    assert!(executor.execute(request).await.is_ok());
}

// ─────────────────────────────────────────────────────────────────────────────
// Error paths
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_fetch_failure() {
    let fetcher = Arc::new(MockContentFetcher::with_error());
    let executor = WasmExecutor::new(fetcher).unwrap();

    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::FetchFailed(_) => {}
        e => panic!("Expected FetchFailed, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_invalid_wasm() {
    let invalid_wasm = vec![0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
    let fetcher = Arc::new(MockContentFetcher::new(invalid_wasm));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::InvalidWasm { .. } => {}
        e => panic!("Expected InvalidWasm, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_compilation_failure() {
    let invalid_wasm = vec![
        0x00, 0x61, 0x73, 0x6d, // magic
        0x01, 0x00, 0x00, 0x00, // version
        0xFF, 0xFF, 0xFF, 0xFF, // garbage data
    ];
    let fetcher = Arc::new(MockContentFetcher::new(invalid_wasm));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::CompilationFailed(_) => {}
        e => panic!("Expected CompilationFailed, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_entry_point_not_found() {
    let wasm_no_start = wat::parse_str(
        r#"
        (module
            (func $main)
            ;; No _start export
        )
        "#,
    )
    .expect("Failed to parse WAT");

    let fetcher = Arc::new(MockContentFetcher::new(wasm_no_start));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()))
        .await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::EntryPointNotFound => {}
        e => panic!("Expected EntryPointNotFound, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_fuel_consumption_tracking() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_fuel_limit(1_000_000);
    let r = executor.execute(request).await.unwrap();

    assert!(r.fuel_consumed > 0);
    assert!(r.fuel_consumed <= 1_000_000);
}

#[tokio::test]
async fn test_execute_fuel_exhaustion() {
    let fetcher = Arc::new(MockContentFetcher::new(create_fuel_heavy_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_fuel_limit(100);
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::FuelExhausted { consumed } => {
            assert!(consumed > 0);
        }
        WasmExecutionError::ExecutionFailed(msg) => {
            eprintln!("Got ExecutionFailed (acceptable): {}", msg);
            assert!(!msg.contains("NotFound") && !msg.contains("invalid"));
        }
        e => panic!("Expected FuelExhausted or ExecutionFailed, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_timeout() {
    let fetcher = Arc::new(MockContentFetcher::new(create_long_running_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string())
        .with_fuel_limit(100_000_000)
        .with_timeout_secs(Some(1));
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::ExecutionTimeout => {}
        // Fuel may be exhausted before the timeout fires depending on execution speed
        WasmExecutionError::FuelExhausted { .. } => {}
        WasmExecutionError::ExecutionFailed(msg) => {
            eprintln!("Got ExecutionFailed (acceptable): {}", msg);
            assert!(!msg.contains("NotFound") && !msg.contains("invalid"));
        }
        e => panic!(
            "Expected ExecutionTimeout, FuelExhausted, or ExecutionFailed, got: {:?}",
            e
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Request validation
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_execute_rejects_zero_fuel() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_fuel_limit(0);
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::InvalidRequest(_) => {}
        e => panic!("Expected InvalidRequest, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_rejects_zero_memory() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_memory_limit_mb(0);
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::InvalidRequest(_) => {}
        e => panic!("Expected InvalidRequest, got: {:?}", e),
    }
}

#[tokio::test]
async fn test_execute_rejects_zero_timeout() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let request = ExecutionRequest::new("test-cid".to_string()).with_timeout_secs(Some(0));
    let result = executor.execute(request).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        WasmExecutionError::InvalidRequest(_) => {}
        e => panic!("Expected InvalidRequest, got: {:?}", e),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Module cache
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_cache_serves_repeated_executions() {
    // The default executor has caching enabled; executing the same CID multiple
    // times should succeed on every call (cache hit from the 2nd call onwards).
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    for i in 0..5 {
        let result = executor
            .execute(ExecutionRequest::new("cached-cid".to_string()))
            .await;
        assert!(result.is_ok(), "Execution {} failed", i);
    }
}

#[tokio::test]
async fn test_cache_disabled_still_works() {
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let config = WasmExecutorConfig {
        enable_cache: false,
        max_cached_modules: 0,
    };
    let executor = WasmExecutor::with_config(fetcher, config).unwrap();

    assert!(
        executor
            .execute(ExecutionRequest::new("test-cid".to_string()))
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn test_cache_respects_capacity() {
    // Fill a cache of size 2 with three distinct CIDs; all should execute
    // successfully regardless of eviction order.
    let fetcher = Arc::new(MockContentFetcher::new(create_minimal_wasm()));
    let config = WasmExecutorConfig {
        enable_cache: true,
        max_cached_modules: 2,
    };
    let executor = WasmExecutor::with_config(fetcher, config).unwrap();

    for cid in &["cid-1", "cid-2", "cid-3", "cid-1"] {
        let result = executor
            .execute(ExecutionRequest::new(cid.to_string()))
            .await;
        assert!(result.is_ok(), "Execution for {} failed", cid);
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Compiled WASM binary (test-wasm-add)
// ─────────────────────────────────────────────────────────────────────────────

fn load_compiled_wasm_binary() -> Vec<u8> {
    let path = "test-wasm-add/target/wasm32-wasip1/release/test-wasm-add.wasm";
    std::fs::read(path).expect(
        "Failed to read compiled WASM binary. Build it first: \
         cd test-wasm-add && cargo build --target wasm32-wasip1 --release",
    )
}

#[tokio::test]
async fn test_compiled_wasm_happy_path_addition() {
    let fetcher = Arc::new(MockContentFetcher::new(load_compiled_wasm_binary()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let args = vec!["add".to_string(), "3".to_string(), "5".to_string()];
    let result = executor
        .execute(ExecutionRequest::new("test-cid".to_string()).with_args(args))
        .await;

    assert!(result.is_ok());
    let r = result.unwrap();
    assert_eq!(r.exit_code, 0);

    let stdout_str = String::from_utf8_lossy(&r.stdout);
    assert!(stdout_str.contains("8"));
    assert!(r.fuel_consumed > 0);
}

#[tokio::test]
async fn test_compiled_wasm_multiple_additions() {
    let test_cases = vec![
        (0, 0, 0),
        (-1, 1, 0),
        (100, 200, 300),
        (-50, -25, -75),
        (9999999, 1, 10000000),
    ];

    let fetcher = Arc::new(MockContentFetcher::new(load_compiled_wasm_binary()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    for (a, b, expected) in test_cases {
        let args = vec!["add".to_string(), a.to_string(), b.to_string()];
        let result = executor
            .execute(ExecutionRequest::new("test-cid".to_string()).with_args(args))
            .await;

        assert!(result.is_ok(), "Failed for {} + {}", a, b);
        let r = result.unwrap();
        assert_eq!(r.exit_code, 0, "Non-zero exit for {} + {}", a, b);

        let stdout_str = String::from_utf8_lossy(&r.stdout);
        assert!(
            stdout_str.contains(&expected.to_string()),
            "Expected {} in stdout for {} + {}, got: {}",
            expected,
            a,
            b,
            stdout_str
        );
    }
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "wasmtime async fiber + proc_exit(non-zero) aborts on Windows"
)]
async fn test_compiled_wasm_missing_args() {
    let test_cases = vec![
        vec!["add".to_string()],
        vec!["add".to_string(), "5".to_string()],
        vec![],
    ];

    let fetcher = Arc::new(MockContentFetcher::new(load_compiled_wasm_binary()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    for args in test_cases {
        let request = ExecutionRequest::new("test-cid".to_string()).with_args(args.clone());
        let r = executor.execute(request).await.unwrap();
        assert_ne!(
            r.exit_code, 0,
            "Expected non-zero exit for args: {:?}",
            args
        );

        let stderr_str = String::from_utf8_lossy(&r.stderr);
        assert!(
            stderr_str.contains("Usage"),
            "Expected usage message for args: {:?}, got: {}",
            args,
            stderr_str
        );
    }
}

#[tokio::test]
#[cfg_attr(
    windows,
    ignore = "wasmtime async fiber + proc_exit(non-zero) aborts on Windows"
)]
async fn test_compiled_wasm_invalid_args() {
    let test_cases = vec![
        vec!["add".to_string(), "foo".to_string(), "5".to_string()],
        vec!["add".to_string(), "3".to_string(), "bar".to_string()],
        vec!["add".to_string(), "foo".to_string(), "bar".to_string()],
        vec!["add".to_string(), "3.14".to_string(), "5".to_string()],
    ];

    let fetcher = Arc::new(MockContentFetcher::new(load_compiled_wasm_binary()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    for args in test_cases {
        let request = ExecutionRequest::new("test-cid".to_string()).with_args(args.clone());
        let r = executor.execute(request).await.unwrap();
        assert_ne!(
            r.exit_code, 0,
            "Expected non-zero exit for invalid args: {:?}",
            args
        );

        let stderr_str = String::from_utf8_lossy(&r.stderr);
        assert!(
            stderr_str.contains("Invalid"),
            "Expected error message for invalid args: {:?}, got: {}",
            args,
            stderr_str
        );
    }
}

#[tokio::test]
async fn test_compiled_wasm_fuel_consumption() {
    let fetcher = Arc::new(MockContentFetcher::new(load_compiled_wasm_binary()));
    let executor = WasmExecutor::new(fetcher).unwrap();

    let args = vec!["add".to_string(), "42".to_string(), "58".to_string()];
    let request = ExecutionRequest::new("test-cid".to_string())
        .with_args(args)
        .with_fuel_limit(10_000_000);

    let r = executor.execute(request).await.unwrap();
    assert_eq!(r.exit_code, 0);
    assert!(r.fuel_consumed > 0, "Expected fuel consumption > 0");
    assert!(
        r.fuel_consumed < 10_000_000,
        "Fuel should not exceed the configured limit"
    );

    let stdout_str = String::from_utf8_lossy(&r.stdout);
    assert!(stdout_str.contains("100")); // 42 + 58 = 100
}
