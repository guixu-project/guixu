// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use thiserror::Error;

#[derive(Debug, Error)]
pub enum UDFError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("execution timeout after {0} seconds")]
    ExecutionTimeout(u64),

    #[error("execution error: {0}")]
    ExecutionError(String),

    #[error("insufficient data: {0}")]
    InsufficientData(String),

    #[error("sandbox violation: {0}")]
    SandboxViolation(String),

    #[error("budget exceeded: requested {requested}, available {available}")]
    BudgetExceeded { requested: u64, available: u64 },

    #[error("UDF not found: {0}")]
    NotFound(String),

    #[error("UDF already registered: {0}")]
    AlreadyRegistered(String),

    #[error("invalid category: expected {expected:?}, found {found:?}")]
    InvalidCategory {
        expected: super::UDFCategory,
        found: super::UDFCategory,
    },

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),

    #[error("loading error: {0}")]
    LoadError(String),

    #[error("remote error: {0}")]
    RemoteError(String),
}

impl UDFError {
    pub fn invalid_input<S: Into<String>>(msg: S) -> Self {
        Self::InvalidInput(msg.into())
    }

    pub fn execution_error<S: Into<String>>(msg: S) -> Self {
        Self::ExecutionError(msg.into())
    }

    pub fn not_found<S: Into<String>>(id: S) -> Self {
        Self::NotFound(id.into())
    }
}

pub type UDFResult<T> = Result<T, UDFError>;
