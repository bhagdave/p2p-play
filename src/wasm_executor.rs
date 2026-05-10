use crate::constants::*;
use crate::content_fetcher::ContentFetcher;
use crate::types::WasmConfig;
use bytes::Bytes;
use lru::LruCache;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use wasmtime::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};
use wasmtime_wasi::WasiCtxBuilder;
use wasmtime_wasi::preview1;

// Re-export so callers can still import WasmExecutionError from this module.
pub use crate::errors::WasmExecutionError;

pub type WasmResult<T> = Result<T, WasmExecutionError>;

#[derive(Debug, Clone)]
pub struct ExecutionRequest {
    pub wasm_cid: String,
    pub input: Vec<u8>,
    pub fuel_limit: u64,
    pub memory_limit_mb: u32,
    pub timeout_secs: Option<u64>,
    pub args: Vec<String>,
}

impl ExecutionRequest {
    pub fn new(wasm_cid: String) -> Self {
        Self::with_config(wasm_cid, &WasmConfig::new())
    }

    pub fn with_config(wasm_cid: String, config: &WasmConfig) -> Self {
        Self {
            wasm_cid,
            input: Vec::new(),
            fuel_limit: config.default_fuel_limit,
            memory_limit_mb: config.default_memory_limit_mb,
            timeout_secs: Some(config.default_timeout_secs),
            args: Vec::new(),
        }
    }

    pub fn with_input(mut self, input: Vec<u8>) -> Self {
        self.input = input;
        self
    }

    pub fn with_fuel_limit(mut self, fuel_limit: u64) -> Self {
        self.fuel_limit = fuel_limit;
        self
    }

    pub fn with_memory_limit_mb(mut self, memory_limit_mb: u32) -> Self {
        self.memory_limit_mb = memory_limit_mb;
        self
    }

    pub fn with_timeout_secs(mut self, timeout_secs: Option<u64>) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn validate(&self, config: &WasmConfig) -> WasmResult<()> {
        if self.fuel_limit == 0 {
            return Err(WasmExecutionError::InvalidRequest(
                "fuel_limit must be greater than 0".to_string(),
            ));
        }
        if self.memory_limit_mb == 0 {
            return Err(WasmExecutionError::InvalidRequest(
                "memory_limit_mb must be greater than 0".to_string(),
            ));
        }
        if let Some(0) = self.timeout_secs {
            return Err(WasmExecutionError::InvalidRequest(
                "timeout_secs must be greater than 0 when specified".to_string(),
            ));
        }
        if self.memory_limit_mb > config.max_memory_limit_mb {
            return Err(WasmExecutionError::MemoryLimitTooLarge(
                self.memory_limit_mb,
                config.max_memory_limit_mb,
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub fuel_consumed: u64,
    pub exit_code: i32,
}

#[derive(Debug, Clone)]
pub struct WasmExecutorConfig {
    pub enable_cache: bool,
    pub max_cached_modules: usize,
    pub async_stack_size: usize,
}

impl Default for WasmExecutorConfig {
    fn default() -> Self {
        Self {
            enable_cache: true,
            max_cached_modules: 10,
            async_stack_size: DEFAULT_ASYNC_STACK_SIZE,
        }
    }
}

struct StoreData {
    wasi: wasmtime_wasi::preview1::WasiP1Ctx,
    limits: StoreLimits,
}

pub struct WasmExecutor<F: ContentFetcher> {
    engine: Engine,
    fetcher: Arc<F>,
    resource_config: WasmConfig,
    module_cache: Option<Mutex<LruCache<String, Module>>>,
}

impl<F: ContentFetcher> WasmExecutor<F> {
    pub fn new(fetcher: Arc<F>) -> WasmResult<Self> {
        Self::with_configs(fetcher, WasmExecutorConfig::default(), WasmConfig::new())
    }

    #[allow(dead_code)]
    pub fn with_config(fetcher: Arc<F>, config: WasmExecutorConfig) -> WasmResult<Self> {
        Self::with_configs(fetcher, config, WasmConfig::new())
    }

    pub fn with_configs(
        fetcher: Arc<F>,
        executor_config: WasmExecutorConfig,
        resource_config: WasmConfig,
    ) -> WasmResult<Self> {
        let mut engine_config = Config::new();
        engine_config
            .async_support(true)
            .consume_fuel(true)
            .async_stack_size(executor_config.async_stack_size);

        let engine = Engine::new(&engine_config)
            .map_err(|e| WasmExecutionError::CompilationFailed(e.to_string()))?;

        let module_cache = if executor_config.enable_cache && executor_config.max_cached_modules > 0
        {
            Some(Mutex::new(LruCache::new(
                executor_config.max_cached_modules,
            )))
        } else {
            None
        };

        Ok(Self {
            engine,
            fetcher,
            resource_config,
            module_cache,
        })
    }

    pub async fn execute(&self, request: ExecutionRequest) -> WasmResult<ExecutionResult> {
        request.validate(&self.resource_config)?;

        let module = self.fetch_and_compile(&request.wasm_cid).await?;

        let (wasi_ctx, stdout_reader, stderr_reader) =
            build_wasi_context(request.input, &request.args);

        let mut store = self.build_store(wasi_ctx, request.fuel_limit, request.memory_limit_mb)?;

        let start_func = Self::instantiate_start(&self.engine, &module, &mut store).await?;

        let (exit_code, fuel_consumed) = run_start_func(
            start_func,
            &mut store,
            request.fuel_limit,
            request.timeout_secs,
        )
        .await?;

        Ok(ExecutionResult {
            stdout: stdout_reader.contents().to_vec(),
            stderr: stderr_reader.contents().to_vec(),
            fuel_consumed,
            exit_code,
        })
    }

    async fn fetch_and_compile(&self, cid: &str) -> WasmResult<Module> {
        // Return a clone of the cached module if one exists
        if let Some(cache) = &self.module_cache
            && let Ok(mut guard) = cache.lock()
            && let Some(module) = guard.get(cid)
        {
            return Ok(module.clone());
        }

        let wasm_bytes = self
            .fetcher
            .fetch(cid)
            .await
            .map_err(WasmExecutionError::FetchFailed)?;

        validate_wasm(&wasm_bytes)?;

        let module = Module::new(&self.engine, &wasm_bytes)
            .map_err(|e| WasmExecutionError::CompilationFailed(e.to_string()))?;

        // Store the freshly compiled module in the cache
        if let Some(cache) = &self.module_cache
            && let Ok(mut guard) = cache.lock()
        {
            guard.put(cid.to_string(), module.clone());
        }

        Ok(module)
    }

    fn build_store(
        &self,
        wasi_ctx: wasmtime_wasi::preview1::WasiP1Ctx,
        fuel_limit: u64,
        memory_limit_mb: u32,
    ) -> WasmResult<Store<StoreData>> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(mb_to_bytes(memory_limit_mb))
            .build();

        let mut store = Store::new(
            &self.engine,
            StoreData {
                wasi: wasi_ctx,
                limits,
            },
        );
        store.limiter(|data| &mut data.limits as &mut dyn wasmtime::ResourceLimiter);
        store
            .set_fuel(fuel_limit)
            .map_err(|e| WasmExecutionError::ExecutionFailed(format!("Failed to set fuel: {e}")))?;

        Ok(store)
    }

    async fn instantiate_start(
        engine: &Engine,
        module: &Module,
        store: &mut Store<StoreData>,
    ) -> WasmResult<wasmtime::TypedFunc<(), ()>> {
        let mut linker = Linker::new(engine);
        preview1::add_to_linker_async(&mut linker, |s: &mut StoreData| &mut s.wasi)
            .map_err(|e| WasmExecutionError::WasiSetupFailed(e.to_string()))?;

        let instance = linker
            .instantiate_async(&mut *store, module)
            .await
            .map_err(|e| WasmExecutionError::InstantiationFailed(e.to_string()))?;

        instance
            .get_typed_func::<(), ()>(&mut *store, "_start")
            .map_err(|_| WasmExecutionError::EntryPointNotFound)
    }
}

fn build_wasi_context(
    input: Vec<u8>,
    args: &[String],
) -> (
    wasmtime_wasi::preview1::WasiP1Ctx,
    wasmtime_wasi::pipe::MemoryOutputPipe,
    wasmtime_wasi::pipe::MemoryOutputPipe,
) {
    let stdin_pipe = wasmtime_wasi::pipe::MemoryInputPipe::new(Bytes::from(input));
    let stdout_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(PIPE_BUFFER_SIZE);
    let stderr_pipe = wasmtime_wasi::pipe::MemoryOutputPipe::new(PIPE_BUFFER_SIZE);

    // Clone before passing into the builder; both handles share the same buffer
    let stdout_reader = stdout_pipe.clone();
    let stderr_reader = stderr_pipe.clone();

    let mut wasi_builder = WasiCtxBuilder::new();
    wasi_builder
        .stdin(stdin_pipe)
        .stdout(stdout_pipe)
        .stderr(stderr_pipe);

    if !args.is_empty() {
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push("wasm_module".to_string());
        full_args.extend_from_slice(args);
        wasi_builder.args(&full_args);
    }

    (wasi_builder.build_p1(), stdout_reader, stderr_reader)
}

async fn run_start_func(
    start_func: wasmtime::TypedFunc<(), ()>,
    store: &mut Store<StoreData>,
    fuel_limit: u64,
    timeout_secs: Option<u64>,
) -> WasmResult<(i32, u64)> {
    let raw_result = if let Some(secs) = timeout_secs {
        tokio::time::timeout(
            Duration::from_secs(secs),
            start_func.call_async(&mut *store, ()),
        )
        .await
    } else {
        Ok(start_func.call_async(&mut *store, ()).await)
    };

    let fuel_consumed = fuel_limit.saturating_sub(store.get_fuel().unwrap_or(0));

    let exit_code = match raw_result {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => classify_trap_error(e, fuel_consumed)?,
        Err(_timeout) => {
            return Err(WasmExecutionError::ExecutionTimeout(
                timeout_secs.unwrap_or(0),
            ));
        }
    };

    Ok((exit_code, fuel_consumed))
}

fn classify_trap_error(e: anyhow::Error, fuel_consumed: u64) -> WasmResult<i32> {
    // WASI process exit – not an error, just a non-zero exit code
    if let Some(exit_error) = e.downcast_ref::<wasmtime_wasi::I32Exit>() {
        return Ok(exit_error.0);
    }

    if let Some(trap) = e.downcast_ref::<wasmtime::Trap>()
        && trap == &wasmtime::Trap::OutOfFuel
    {
        return Err(WasmExecutionError::FuelExhausted {
            consumed: fuel_consumed,
        });
    }

    let error_str = e.to_string();
    if error_str.contains("fuel") || error_str.contains("out of fuel") {
        return Err(WasmExecutionError::FuelExhausted {
            consumed: fuel_consumed,
        });
    }
    if error_str.contains("resource limit exceeded")
        || (error_str.contains("memory") && error_str.contains("limit exceeded"))
    {
        return Err(WasmExecutionError::MemoryLimitExceeded);
    }

    Err(WasmExecutionError::ExecutionFailed(error_str))
}

fn mb_to_bytes(mb: u32) -> usize {
    (mb as usize) * BYTES_PER_MB
}

pub fn validate_wasm(bytes: &[u8]) -> WasmResult<()> {
    if bytes.len() < WASM_HEADER_LEN {
        return Err(WasmExecutionError::InvalidWasm {
            reason: format!(
                "WASM binary too small: {} bytes (minimum {} required)",
                bytes.len(),
                WASM_HEADER_LEN
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

    /// Create a minimal valid WASM module for testing
    fn create_minimal_wasm() -> Vec<u8> {
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

    // --- validate_wasm ---

    #[test]
    fn test_validate_wasm_valid() {
        let valid_wasm = [0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00];
        assert!(validate_wasm(&valid_wasm).is_ok());
    }

    #[test]
    fn test_validate_wasm_too_small() {
        let too_small = [0x00, 0x61, 0x73, 0x6D];
        let result = validate_wasm(&too_small);
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
        match result {
            Err(WasmExecutionError::InvalidWasm { reason }) => {
                assert!(reason.contains("magic bytes"));
            }
            _ => panic!("Expected InvalidWasm error"),
        }
    }

    #[test]
    fn test_validate_wasm_bad_version() {
        let bad_version = [0x00, 0x61, 0x73, 0x6D, 0x02, 0x00, 0x00, 0x00];
        let result = validate_wasm(&bad_version);
        match result {
            Err(WasmExecutionError::InvalidWasm { reason }) => {
                assert!(reason.contains("version"));
            }
            _ => panic!("Expected InvalidWasm error"),
        }
    }

    #[test]
    fn test_validate_wasm_real_bytes() {
        let wasm = create_minimal_wasm();
        validate_wasm(&wasm).unwrap();
    }

    // --- mb_to_bytes / BYTES_PER_MB ---

    #[test]
    fn test_mb_to_bytes() {
        assert_eq!(mb_to_bytes(1), BYTES_PER_MB);
        assert_eq!(mb_to_bytes(64), 64 * BYTES_PER_MB);
        assert_eq!(mb_to_bytes(0), 0);
    }

    // --- ExecutionRequest builder ---

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

        assert_eq!(request.fuel_limit, WasmConfig::DEFAULT_FUEL_LIMIT);
        assert_eq!(request.memory_limit_mb, WasmConfig::DEFAULT_MEMORY_LIMIT_MB);
        assert_eq!(request.timeout_secs, Some(WasmConfig::DEFAULT_TIMEOUT_SECS));
        assert!(request.input.is_empty());
        assert!(request.args.is_empty());
    }

    // --- ExecutionRequest::validate ---

    #[test]
    fn test_validate_request_ok() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string());
        assert!(request.validate(&config).is_ok());
    }

    #[test]
    fn test_validate_request_zero_fuel() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string()).with_fuel_limit(0);
        assert!(matches!(
            request.validate(&config),
            Err(WasmExecutionError::InvalidRequest(_))
        ));
    }

    #[test]
    fn test_validate_request_zero_memory() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string()).with_memory_limit_mb(0);
        assert!(matches!(
            request.validate(&config),
            Err(WasmExecutionError::InvalidRequest(_))
        ));
    }

    #[test]
    fn test_validate_request_zero_timeout() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string()).with_timeout_secs(Some(0));
        assert!(matches!(
            request.validate(&config),
            Err(WasmExecutionError::InvalidRequest(_))
        ));
    }

    #[test]
    fn test_validate_request_no_timeout_ok() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string()).with_timeout_secs(None);
        assert!(request.validate(&config).is_ok());
    }

    #[test]
    fn test_validate_request_memory_exceeds_max() {
        let config = WasmConfig::new();
        let request = ExecutionRequest::new("QmTest".to_string())
            .with_memory_limit_mb(WasmConfig::MAX_MEMORY_LIMIT_MB + 1);
        assert!(matches!(
            request.validate(&config),
            Err(WasmExecutionError::MemoryLimitTooLarge(_, _))
        ));
    }

    // --- WasmExecutorConfig ---

    #[test]
    fn test_wasm_executor_config_default() {
        let config = WasmExecutorConfig::default();
        assert!(config.enable_cache);
        assert_eq!(config.max_cached_modules, 10);
        assert_eq!(config.async_stack_size, DEFAULT_ASYNC_STACK_SIZE);
    }

    // --- StoreLimitsBuilder (sanity check) ---

    #[test]
    fn test_store_limits_builder() {
        let limits = StoreLimitsBuilder::new()
            .memory_size(mb_to_bytes(32))
            .build();
        assert!(std::mem::size_of_val(&limits) > 0);
    }
}
