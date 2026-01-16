use crate::errors::FetchError;

pub struct ExecutionRequest {
   wasm_cid: String,        // or IPNS name
   input: Vec<u8>,
   fuel_limit: u64,
   memory_limit_mb: u32,
}
   
pub struct ExecutionResult {
   stdout: Vec<u8>,
   stderr: Vec<u8>,
   fuel_consumed: u64,
   exit_code: i32,
}

const WASM_MAGIC: &[u8] = b"\0asm";

pub fn validate_wasm(bytes: &[u8]) -> Result<(), FetchError> {
    if bytes.len() < 4 || &bytes[0..4] != WASM_MAGIC {
        return Err(FetchError::InvalidWasm);
    }
    Ok(())
}
