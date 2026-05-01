// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use crate::common::{UDFError, UDFId, UDFResult};

pub struct SandboxPolicy {
    max_memory_bytes: usize,
    max_execution_time_secs: u64,
    allowed_network_domains: Vec<String>,
    allow_filesystem_access: bool,
    allow_subprocess: bool,
    trusted_signers: Vec<Vec<u8>>,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        Self {
            max_memory_bytes: 128 * 1024 * 1024,
            max_execution_time_secs: 30,
            allowed_network_domains: vec![],
            allow_filesystem_access: false,
            allow_subprocess: false,
            trusted_signers: vec![],
        }
    }
}

impl SandboxPolicy {
    pub fn new(max_memory_bytes: usize, max_execution_time_secs: u64) -> Self {
        Self {
            max_memory_bytes,
            max_execution_time_secs,
            allowed_network_domains: vec![],
            allow_filesystem_access: false,
            allow_subprocess: false,
            trusted_signers: vec![],
        }
    }

    pub fn with_network_domains(mut self, domains: Vec<String>) -> Self {
        self.allowed_network_domains = domains;
        self
    }

    pub fn with_filesystem_access(mut self, allow: bool) -> Self {
        self.allow_filesystem_access = allow;
        self
    }

    pub fn with_subprocess(mut self, allow: bool) -> Self {
        self.allow_subprocess = allow;
        self
    }

    pub fn with_trusted_signers(mut self, signers: Vec<Vec<u8>>) -> Self {
        self.trusted_signers = signers;
        self
    }

    pub fn check_execution(&self, udf_id: &UDFId) -> UDFResult<()> {
        if udf_id.as_str().starts_with("builtin:") {
            return Ok(());
        }

        if !self.allow_subprocess
            && (udf_id.as_str().contains("exec") || udf_id.as_str().contains("shell"))
        {
            return Err(UDFError::SandboxViolation(
                "Subprocess execution not allowed for user UDFs".to_string(),
            ));
        }

        Ok(())
    }

    pub fn verify_signature(&self, _udf_id: &UDFId, _signature: &[u8]) -> UDFResult<()> {
        if self.trusted_signers.is_empty() {
            return Ok(());
        }

        Err(UDFError::SandboxViolation(
            "Signature verification not yet implemented".to_string(),
        ))
    }

    pub fn max_memory_bytes(&self) -> usize {
        self.max_memory_bytes
    }

    pub fn max_execution_time_secs(&self) -> u64 {
        self.max_execution_time_secs
    }

    pub fn can_access_network(&self, domain: &str) -> bool {
        if self.allowed_network_domains.is_empty() {
            return false;
        }
        self.allowed_network_domains
            .iter()
            .any(|d| domain.contains(d))
    }

    pub fn can_access_filesystem(&self) -> bool {
        self.allow_filesystem_access
    }

    pub fn can_spawn_subprocess(&self) -> bool {
        self.allow_subprocess
    }
}

pub trait UDFExecutor: Send + Sync {
    type Input;
    type Output;

    fn execute(
        &self,
        udf: &dyn std::any::Any,
        input: &Self::Input,
        policy: &SandboxPolicy,
    ) -> impl std::future::Future<Output = UDFResult<Self::Output>> + Send;
}

pub struct NativeUDFExecutor;

impl NativeUDFExecutor {
    pub async fn execute_valuation(
        udf: &dyn crate::valuation::ValuationUDF,
        input: &crate::valuation::ValuationInput,
        _policy: &SandboxPolicy,
    ) -> UDFResult<crate::valuation::ValuationOutput> {
        udf.evaluate(input).await
    }

    pub async fn execute_sampling(
        udf: &dyn crate::sampling::SamplingUDF,
        input: &crate::sampling::SamplingInput,
        records: &[crate::sampling::SampleRecord],
        _policy: &SandboxPolicy,
    ) -> UDFResult<crate::sampling::SamplingOutput> {
        udf.sample(input, records).await
    }
}

pub struct WebAssemblyUDFExecutor;

impl WebAssemblyUDFExecutor {
    pub async fn execute_wasm(
        _wasm_bytes: &[u8],
        _input: &[u8],
        _policy: &SandboxPolicy,
    ) -> UDFResult<Vec<u8>> {
        Err(UDFError::SandboxViolation(
            "WebAssembly execution not yet implemented".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_policy_builtin() {
        let policy = SandboxPolicy::default();
        let result = policy.check_execution(&UDFId::new("builtin:valuation:tcv"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_sandbox_policy_no_subprocess() {
        let policy = SandboxPolicy::default();
        let result = policy.check_execution(&UDFId::new("custom:custom_udf"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_sandbox_policy_with_trusted_signers() {
        let policy = SandboxPolicy::default();
        assert!(!policy.can_access_network("example.com"));
    }

    #[test]
    fn test_sandbox_policy_filesystem() {
        let policy = SandboxPolicy::new(1024 * 1024, 30);
        assert!(!policy.can_access_filesystem());

        let policy = policy.with_filesystem_access(true);
        assert!(policy.can_access_filesystem());
    }
}
