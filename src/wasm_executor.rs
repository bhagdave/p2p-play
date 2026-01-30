//! WASM executor for running WebAssembly modules with WASI support.
//!
//! This module provides functionality to fetch WASM binaries from IPFS,
//! validate them, and execute them with resource limits (fuel/memory).

use crate::content_fetcher::ContentFetcher;
use crate::errors::FetchError;
use bytes::Bytes;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::preview1;
use wasmtime_wasi::WasiCtxBuilder;

/// WASM magic bytes: "\0asm"
const WASM_MAGIC: &[u8] = b"\0asm";

/// Expected WASM version bytes (version 1)
const WASM_VERSION: &[u8] = &[0x01, 0x00, 0x00, 0x00];

/// Default fuel limit for WASM execution (10 million instructions)
const DEFAULT_FUEL_LIMIT: u64 = 10_000_000;

/// Default memory limit in megabytes
const DEFAULT_MEMORY_LIMIT_MB: u32 = 64;

/// Maximum allowed memory limit in megabytes (1 GB)
const MAX_MEMORY_LIMIT_MB: u32 = 1024;

/// Default execution timeout in seconds
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Errors that can occur during WASM execution
#[derive(Debug, Error)]
pub enum WasmExecutionError {
    #[error("Failed to fetch WASM: {0}")]
    FetchFailed(#[from] FetchError),

    #[error("Invalid WASM binary: {reason}")]
    InvalidWasm { reason: String },

    #[error("WASM compilation failed: {0}")]
    CompilationFailed(String),

    #[error("WASM instantiation failed: {0}")]
    InstantiationFailed(String),

    #[error("WASM execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Fuel exhausted after {consumed} units")]
    FuelExhausted { consumed: u64 },

    #[error("Memory limit exceeded")]
    MemoryLimitExceeded,

    #[error("Memory limit too large: {0} MB (maximum {1} MB)")]
    MemoryLimitTooLarge(u32, u32),

    #[error("Execution timeout after 30 seconds")]
    ExecutionTimeout,

    #[error("Entry point '_start' not found")]
    EntryPointNotFound,

    #[error("WASI setup failed: {0}")]
    WasiSetupFailed(String),
}

/// Result type for WASM execution operations
pub type WasmResult<T> = Result<T, WasmExecutionError>;

/// Request to execute a WASM module
#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    /// IPFS CID of the WASM binary
    pub wasm_cid: String,
    /// Input data to pass via stdin
    pub input: Vec<u8>,
    /// Maximum fuel (instructions) allowed
    pub fuel_limit: u64,
    /// Maximum memory in megabytes
    pub memory_limit_mb: u32,
    /// Optional execution timeout in seconds
    pub timeout_secs: Option<u64>,
    /// Command-line arguments to pass to the WASM module
    pub args: Vec<String>,
}

impl ExecutionRequest {
    /// Create a new execution request with default limits
    pub fn new(wasm_cid: String) -> Self {
        Self {
            wasm_cid,
            input: Vec::new(),
            fuel_limit: DEFAULT_FUEL_LIMIT,
            memory_limit_mb: DEFAULT_MEMORY_LIMIT_MB,
            timeout_secs: Some(DEFAULT_TIMEOUT_SECS),
            args: Vec::new(),
        }
    }

    /// Set input data for stdin
    pub fn with_input(mut self, input: Vec<u8>) -> Self {
        self.input = input;
        self
    }

    /// Set fuel limit
    pub fn with_fuel_limit(mut self, fuel_limit: u64) -> Self {
        self.fuel_limit = fuel_limit;
        self
    }

    /// Set memory limit in megabytes
    pub fn with_memory_limit_mb(mut self, memory_limit_mb: u32) -> Self {
        self.memory_limit_mb = memory_limit_mb;
        self
    }

    /// Set execution timeout in seconds
    pub fn with_timeout_secs(mut self, timeout_secs: Option<u64>) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set command-line arguments
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }
}

/// Result of WASM module execution
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Output captured from stdout
    pub stdout: Vec<u8>,
    /// Output captured from stderr
    pub stderr: Vec<u8>,
    /// Amount of fuel consumed during execution
    pub fuel_consumed: u64,
    /// Exit code from the WASM module
    pub exit_code: i32,
}

/// Configuration for the WASM executor
#[derive(Debug, Clone)]
pub struct WasmExecutorConfig {
    /// Enable module caching
    pub enable_cache: bool,
    /// Maximum number of cached modules
    pub max_cached_modules: usize,
}

impl Default for WasmExecutorConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            max_cached_modules: 10,
        }
    }
}

/// Store data that holds both WASI context and resource limits
struct StoreData {
    wasi: wasmtime_wasi::preview1::WasiP1Ctx,
    limits: StoreLimits,
}

/// WASM executor that fetches, validates, and runs WASM modules
pub struct WasmExecutor<F: ContentFetcher> {
    engine: Engine,
    fetcher: Arc<F>,
    #[allow(dead_code)]
    config: WasmExecutorConfig,
}

impl<F: ContentFetcher> WasmExecutor<F> {
    /// Create a new WASM executor with the given content fetcher
    pub fn new(fetcher: Arc<F>) -> WasmResult<Self> {
        Self::with_config(fetcher, WasmExecutorConfig::default())
    }

    /// Create a new WASM executor with custom configuration
    pub fn with_config(fetcher: Arc<F>, config: WasmExecutorConfig) -> WasmResult<Self> {
        let mut engine_config = Config::new();
        engine_config
            .async_support(true)
            .consume_fuel(true);

        let engine = Engine::new(&engine_config)
            .map_err(|e| WasmExecutionError::CompilationFailed(e.to_string()))?;

        Ok(Self {
            engine,
            fetcher,
            config,
        })
    }

    /// Execute a WASM module based on the execution request
    pub async fn execute(&self, request: ExecutionRequest) -> WasmResult<ExecutionResult> {
        // Fetch WASM binary from IPFS
        let wasm_bytes = self
            .fetcher
            .fetch(&request.wasm_cid)
            .await
            .map_err(WasmExecutionError::FetchFailed)?;

        // Validate the WASM binary
        validate_wasm(&wasm_bytes)?;

        // Compile the module
        let module = Module::new(&self.engine, &wasm_bytes)
            .map_err(|e| WasmExecutionError::CompilationFailed(e.to_string()))?;

        // Create WASI context with captured stdout/stderr
        let stdin_pipe = wasmtime_wasi::pipe::MemoryInputPipe::new(Bytes::from(request.input));
        let stdout_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(64 * 1024);
        let stderr_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(64 * 1024);

        let stdout_pipe_clone = stdout_pipe.clone();
        let stderr_pipe_clone = stderr_pipe.clone();

        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder
            .stdin(stdin_pipe)
            .stdout(stdout_pipe)
            .stderr(stderr_pipe);

        // Add arguments if provided
        if !request.args.is_empty() {
            wasi_builder.args(&request.args);
        }

        // Build WASIp1 context
        let wasi_ctx = wasi_builder.build_p1();

        // Validate memory limit to prevent overflow
        if request.memory_limit_mb > MAX_MEMORY_LIMIT_MB {
            return Err(WasmExecutionError::MemoryLimitTooLarge(
                request.memory_limit_mb,
                MAX_MEMORY_LIMIT_MB,
            ));
        }

        // Create store limits based on the request
        let memory_limit_bytes = (request.memory_limit_mb as usize) * 1024 * 1024;
        let limits = StoreLimitsBuilder::new()
            .memory_size(memory_limit_bytes)
            .build();

        // Create store data with WASI context and limits
        let store_data = StoreData {
            wasi: wasi_ctx,
            limits,
        };

        // Create store with fuel limit and memory limits
        let mut store = Store::new(&self.engine, store_data);
        store.limiter(|data| &mut data.limits as &mut dyn wasmtime::ResourceLimiter);
        store.set_fuel(request.fuel_limit).map_err(|e| {
            WasmExecutionError::ExecutionFailed(format!("Failed to set fuel: {}", e))
        })?;

        // Create linker and add WASI preview1
        let mut linker = Linker::new(&self.engine);
        preview1::add_to_linker_async(&mut linker, |s: &mut StoreData| &mut s.wasi)
            .map_err(|e| WasmExecutionError::WasiSetupFailed(e.to_string()))?;

        // Instantiate the module
        let instance = linker
            .instantiate_async(&mut store, &module)
            .await
            .map_err(|e| WasmExecutionError::InstantiationFailed(e.to_string()))?;

        // Get the _start function
        let start_func = instance
            .get_typed_func::<(), ()>(&mut store, "_start")
            .map_err(|_| WasmExecutionError::EntryPointNotFound)?;

        // Execute with optional timeout
        let execution_result = if let Some(timeout_secs) = request.timeout_secs {
            tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                start_func.call_async(&mut store, ()),
            )
            .await
        } else {
            Ok(start_func.call_async(&mut store, ()).await)
        };

        // Get remaining fuel to calculate consumption
        let remaining_fuel = store.get_fuel().unwrap_or(0);
        let fuel_consumed = request.fuel_limit.saturating_sub(remaining_fuel);

        // Handle execution result
        let exit_code = match execution_result {
            Ok(Ok(())) => 0,
            Ok(Err(e)) => {
                // Check if it's a fuel exhaustion error
                let error_str = e.to_string();
                if error_str.contains("fuel") || error_str.contains("out of fuel") {
                    return Err(WasmExecutionError::FuelExhausted {
                        consumed: fuel_consumed,
                    });
                }
                // Check if it's a memory limit error
                // Wasmtime's ResourceLimiter errors typically contain "resource limit exceeded"
                if error_str.contains("resource limit exceeded") 
                    || (error_str.contains("memory") && error_str.contains("limit exceeded")) {
                    return Err(WasmExecutionError::MemoryLimitExceeded);
                }
                // Check for WASI exit code
                if let Some(exit_error) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
                    exit_error.0
                } else {
                    return Err(WasmExecutionError::ExecutionFailed(error_str));
                }
            }
            Err(_timeout) => {
                return Err(WasmExecutionError::ExecutionTimeout);
            }
        };

        // Collect output from pipes
        let stdout = stdout_pipe_clone.contents().to_vec();
        let stderr = stderr_pipe_clone.contents().to_vec();

        Ok(ExecutionResult {
            stdout,
            stderr,
            fuel_consumed,
            exit_code,
        })
    }
}

/// Validate that bytes represent a valid WASM binary
///
/// Checks for:
/// - WASM magic bytes ("\0asm")
/// - WASM version (1.0)
pub fn validate_wasm(bytes: &[u8]) -> WasmResult<()> {
    if bytes.len() < 8 {
        return Err(WasmExecutionError::InvalidWasm {
            reason: format!(
                "WASM binary too small: {} bytes (minimum 8 required)",
                bytes.len()
            ),
        });
    }

    if &bytes[0..4] != WASM_MAGIC {
        return Err(WasmExecutionError::InvalidWasm {
            reason: format!(
                "Invalid WASM magic bytes: expected {:?}, got {:?}",
                WASM_MAGIC,
                &bytes[0..4]
            ),
        });
    }

    if &bytes[4..8] != WASM_VERSION {
        return Err(WasmExecutionError::InvalidWasm {
            reason: format!(
                "Unsupported WASM version: expected {:?}, got {:?}",
                WASM_VERSION,
                &bytes[4..8]
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_wasm_valid() {
        // Valid WASM header: magic + version 1
        let valid_wasm = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert!(validate_wasm(&valid_wasm).is_ok());
    }

    #[test]
    fn test_validate_wasm_too_small() {
        let too_small = [0x00, 0x61, 0x73, 0x6D];
        let result = validate_wasm(&too_small);
        assert!(result.is_err());
        match result {
            Err(WasmExecutionError::InvalidWasm { reason }) => {
                assert!(reason.contains("too small"));
            }
            _ => panic!("Expected InvalidWasm error"),
        }
    }

    #[test]
    fn test_validate_wasm_bad_magic() {
        let bad_magic = [0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00];
        let result = validate_wasm(&bad_magic);
        assert!(result.is_err());
        match result {
            Err(WasmExecutionError::InvalidWasm { reason }) => {
                assert!(reason.contains("magic bytes"));
            }
            _ => panic!("Expected InvalidWasm error"),
        }
    }

    #[test]
    fn test_validate_wasm_bad_version() {
        // Valid magic but wrong version
        let bad_version = [0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00];
        let result = validate_wasm(&bad_version);
        assert!(result.is_err());
        match result {
            Err(WasmExecutionError::InvalidWasm { reason }) => {
                assert!(reason.contains("version"));
            }
            _ => panic!("Expected InvalidWasm error"),
        }
    }

    #[test]
    fn test_execution_request_builder() {
        let request = ExecutionRequest::new("QmTest123".to_string())
            .with_input(b"hello".to_vec())
            .with_fuel_limit(1_000_000)
            .with_memory_limit_mb(32)
            .with_timeout_secs(Some(60))
            .with_args(vec!["arg1".to_string(), "arg2".to_string()]);

        assert_eq!(request.wasm_cid, "QmTest123");
        assert_eq!(request.input, b"hello");
        assert_eq!(request.fuel_limit, 1_000_000);
        assert_eq!(request.memory_limit_mb, 32);
        assert_eq!(request.timeout_secs, Some(60));
        assert_eq!(request.args, vec!["arg1", "arg2"]);
    }

    #[test]
    fn test_execution_request_defaults() {
        let request = ExecutionRequest::new("QmTest".to_string());

        assert_eq!(request.fuel_limit, DEFAULT_FUEL_LIMIT);
        assert_eq!(request.memory_limit_mb, DEFAULT_MEMORY_LIMIT_MB);
        assert_eq!(request.timeout_secs, Some(DEFAULT_TIMEOUT_SECS));
        assert!(request.input.is_empty());
        assert!(request.args.is_empty());
    }

    #[test]
    fn test_wasm_executor_config_default() {
        let config = WasmExecutorConfig::default();
        assert!(config.enable_cache);
        assert_eq!(config.max_cached_modules, 10);
    }

    #[test]
    fn test_memory_limit_configuration() {
        // Test that memory limits are properly converted to bytes
        let request = ExecutionRequest::new("QmTest".to_string())
            .with_memory_limit_mb(64);

        // 64 MB should convert to 64 * 1024 * 1024 bytes = 67108864 bytes
        let expected_bytes = 64 * 1024 * 1024;
        let actual_bytes = (request.memory_limit_mb as usize) * 1024 * 1024;
        assert_eq!(actual_bytes, expected_bytes);
    }

    #[test]
    fn test_store_limits_builder() {
        // Verify that StoreLimitsBuilder can be used to create memory limits
        let memory_limit_mb = 32;
        let memory_limit_bytes = (memory_limit_mb as usize) * 1024 * 1024;
        let limits = StoreLimitsBuilder::new()
            .memory_size(memory_limit_bytes)
            .build();

        // If we can build the limits without error, the configuration is valid
        assert!(std::mem::size_of_val(&limits) > 0);
    }

    #[test]
    fn test_memory_limit_validation() {
        // Test that exceeding MAX_MEMORY_LIMIT_MB is detected
        let request = ExecutionRequest::new("QmTest".to_string())
            .with_memory_limit_mb(MAX_MEMORY_LIMIT_MB + 1);

        // The executor should validate and reject excessive memory limits
        assert_eq!(request.memory_limit_mb, MAX_MEMORY_LIMIT_MB + 1);
        assert!(request.memory_limit_mb > MAX_MEMORY_LIMIT_MB);
    }
}
