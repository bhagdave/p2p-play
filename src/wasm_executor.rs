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
