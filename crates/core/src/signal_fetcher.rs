// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::feedback::CommunitySignal;
use crate::types::DatasetCid;

pub type SignalFetcherFn = Box<dyn Fn(&str) -> CommunitySignal + Send + Sync>;

#[derive(Clone, Default)]
pub enum SignalFetcher {
    #[default]
    NoOp,
    LocalOnly(Arc<dyn LocalSignalStore>),
    EasAttestation(Arc<dyn EasClient>),
}

pub trait LocalSignalStore: Send + Sync {
    fn compute_signal(&self, cid: &DatasetCid) -> anyhow::Result<CommunitySignal>;
}

pub trait EasClient: Send + Sync {
    fn fetch_signal(&self, cid: &DatasetCid) -> anyhow::Result<CommunitySignal>;
}

impl SignalFetcher {
    pub fn no_op() -> Self {
        Self::NoOp
    }

    pub fn local_only(store: Arc<dyn LocalSignalStore>) -> Self {
        Self::LocalOnly(store)
    }

    pub fn eas_attestation(client: Arc<dyn EasClient>) -> Self {
        Self::EasAttestation(client)
    }

    pub fn into_fetcher_fn(self) -> SignalFetcherFn {
        Box::new(move |cid_str: &str| {
            let cid = DatasetCid(cid_str.to_string());
            self.compute(&cid)
        })
    }

    pub fn compute(&self, cid: &DatasetCid) -> CommunitySignal {
        match self {
            Self::NoOp => CommunitySignal::neutral(cid.clone()),
            Self::LocalOnly(store) => store
                .compute_signal(cid)
                .unwrap_or_else(|_| CommunitySignal::neutral(cid.clone())),
            Self::EasAttestation(client) => client
                .fetch_signal(cid)
                .unwrap_or_else(|_| CommunitySignal::neutral(cid.clone())),
        }
    }

    pub fn is_noop(&self) -> bool {
        matches!(self, Self::NoOp)
    }

    pub fn call(&self, cid_str: &str) -> CommunitySignal {
        let cid = DatasetCid(cid_str.to_string());
        self.compute(&cid)
    }
}

impl From<SignalFetcher> for SignalFetcherFn {
    fn from(fetcher: SignalFetcher) -> Self {
        fetcher.into_fetcher_fn()
    }
}
