// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

pub use data_core::types::{
    AlphaScore, ChainId, EntityRefs, Evidence, ExecutionAction, ExecutionIntent, Opportunity,
    SignalEvent, SignalFamily, SignalSource, TxHash,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SubscriptionId(pub Uuid);

impl SubscriptionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SubscriptionId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EntityFilters {
    pub tokens: Option<Vec<String>>,
    pub pools: Option<Vec<String>>,
    pub wallets: Option<Vec<String>>,
    pub contracts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalSubscription {
    pub subscription_id: SubscriptionId,
    pub skill_id: String,
    pub signal_families: Vec<SignalFamily>,
    pub chain_ids: Vec<ChainId>,
    pub entity_filters: EntityFilters,
    pub action_mode: ExecutionAction,
    pub created_at: DateTime<Utc>,
}

impl SignalSubscription {
    pub fn new(
        skill_id: String,
        signal_families: Vec<SignalFamily>,
        chain_ids: Vec<ChainId>,
        entity_filters: EntityFilters,
        action_mode: ExecutionAction,
    ) -> Self {
        Self {
            subscription_id: SubscriptionId::new(),
            skill_id,
            signal_families,
            chain_ids,
            entity_filters,
            action_mode,
            created_at: Utc::now(),
        }
    }

    pub fn matches_signal(&self, signal: &SignalEvent) -> bool {
        if !self.signal_families.contains(&signal.signal_family) {
            return false;
        }
        if !self.chain_ids.is_empty() && !self.chain_ids.contains(&signal.chain_id) {
            return false;
        }
        if !self.matches_entity_filters(signal) {
            return false;
        }
        true
    }

    fn matches_entity_filters(&self, signal: &SignalEvent) -> bool {
        let filters = &self.entity_filters;

        if let Some(ref tokens) = filters.tokens {
            if !tokens.iter().any(|t| signal.entity_refs.tokens.contains(t)) {
                return false;
            }
        }
        if let Some(ref pools) = filters.pools {
            if !pools.iter().any(|p| signal.entity_refs.pools.contains(p)) {
                return false;
            }
        }
        if let Some(ref wallets) = filters.wallets {
            if !wallets
                .iter()
                .any(|w| signal.entity_refs.wallets.contains(w))
            {
                return false;
            }
        }
        if let Some(ref contracts) = filters.contracts {
            if !contracts
                .iter()
                .any(|c| signal.entity_refs.contracts.contains(c))
            {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DecayPolicy {
    pub staleness_threshold_secs: u64,
    pub decay_rate_per_minute: f64,
    pub min_score_threshold: f64,
}

impl DecayPolicy {
    pub fn new(
        staleness_threshold_secs: u64,
        decay_rate_per_minute: f64,
        min_score_threshold: f64,
    ) -> Self {
        Self {
            staleness_threshold_secs,
            decay_rate_per_minute,
            min_score_threshold,
        }
    }

    pub fn default_web3() -> Self {
        Self {
            staleness_threshold_secs: 300,
            decay_rate_per_minute: 0.05,
            min_score_threshold: 0.1,
        }
    }

    pub fn apply_decay(&self, score: f64, age_secs: u64) -> f64 {
        if age_secs > self.staleness_threshold_secs {
            return 0.0;
        }
        let decay_factor = (-self.decay_rate_per_minute * (age_secs as f64) / 60.0).exp();
        score * decay_factor
    }
}

impl Default for DecayPolicy {
    fn default() -> Self {
        Self::default_web3()
    }
}

pub trait AlphaScorer: Send + Sync {
    fn score(&self, opportunity: &Opportunity) -> AlphaScore;
    fn score_total(&self, opportunity: &Opportunity) -> f64 {
        self.score(opportunity).total
    }
}

pub struct DefaultAlphaScorer {
    freshness_weight: f64,
    rarity_weight: f64,
    significance_weight: f64,
    tradability_weight: f64,
    confidence_weight: f64,
    risk_weight: f64,
}

impl DefaultAlphaScorer {
    pub fn new() -> Self {
        Self {
            freshness_weight: 1.0,
            rarity_weight: 1.0,
            significance_weight: 1.0,
            tradability_weight: 1.0,
            confidence_weight: 1.0,
            risk_weight: 1.0,
        }
    }

    pub fn with_weights(
        freshness: f64,
        rarity: f64,
        significance: f64,
        tradability: f64,
        confidence: f64,
        risk: f64,
    ) -> Self {
        Self {
            freshness_weight: freshness,
            rarity_weight: rarity,
            significance_weight: significance,
            tradability_weight: tradability,
            confidence_weight: confidence,
            risk_weight: risk,
        }
    }

    fn calculate_freshness(&self, opportunity: &Opportunity) -> f64 {
        if opportunity.signal_events.is_empty() {
            return 0.0;
        }
        let now = Utc::now();
        let latest = opportunity
            .signal_events
            .iter()
            .map(|e| e.freshness_ms)
            .max()
            .unwrap_or(0);
        let age_ms = now.timestamp_millis() as u64
            - opportunity
                .signal_events
                .iter()
                .map(|e| e.observed_at.timestamp_millis() as u64)
                .max()
                .unwrap_or(0);
        1.0 - (age_ms as f64 / (latest as f64 + 1.0)).min(1.0)
    }

    fn calculate_rarity(&self, opportunity: &Opportunity) -> f64 {
        let unique_entities = {
            let mut set = HashSet::new();
            for event in &opportunity.signal_events {
                for token in &event.entity_refs.tokens {
                    set.insert(token);
                }
                for pool in &event.entity_refs.pools {
                    set.insert(pool);
                }
                for wallet in &event.entity_refs.wallets {
                    set.insert(wallet);
                }
            }
            set.len()
        };
        if unique_entities == 0 {
            return 0.5;
        }
        let rarity = 1.0 / (unique_entities as f64).sqrt();
        rarity.min(1.0)
    }

    fn calculate_significance(&self, opportunity: &Opportunity) -> f64 {
        if opportunity.signal_events.is_empty() {
            return 0.0;
        }
        let mut total_flow: f64 = 0.0;
        let mut count = 0;
        for event in &opportunity.signal_events {
            if let Some(value) = event.features.get("flow_amount") {
                total_flow += value.abs();
                count += 1;
            }
            if let Some(value) = event.features.get("swap_volume") {
                total_flow += value.abs();
                count += 1;
            }
        }
        if count == 0 {
            return 0.5;
        }
        (total_flow / count as f64 / 1_000_000.0).min(1.0)
    }

    fn calculate_tradability(&self, opportunity: &Opportunity) -> f64 {
        let mut tradable_count = 0;
        let total = opportunity.signal_events.len();
        for event in &opportunity.signal_events {
            let has_gas = event.features.contains_key("gas_estimation");
            let has_slippage = event.features.contains_key("slippage_bps");
            let reorg_safe = event.reorg_safe;
            if has_gas && has_slippage && reorg_safe {
                tradable_count += 1;
            }
        }
        if total == 0 {
            return 0.5;
        }
        tradable_count as f64 / total as f64
    }

    fn calculate_confidence(&self, opportunity: &Opportunity) -> f64 {
        if opportunity.signal_events.is_empty() {
            return 0.0;
        }
        let avg_confidence: f64 = opportunity
            .signal_events
            .iter()
            .map(|e| e.confidence)
            .sum::<f64>()
            / opportunity.signal_events.len() as f64;
        avg_confidence.min(1.0)
    }

    fn calculate_risk_adjustment(&self, opportunity: &Opportunity) -> f64 {
        let mut risk_score = 0.0;
        for event in &opportunity.signal_events {
            let reorg_risk = if event.reorg_safe { 0.0 } else { 0.3 };
            let freshness_risk = if event.freshness_ms > 60000 { 0.2 } else { 0.0 };
            let evidence_risk = if event.evidence.is_empty() { 0.2 } else { 0.0 };
            risk_score += reorg_risk + freshness_risk + evidence_risk;
        }
        let avg_risk = if opportunity.signal_events.is_empty() {
            0.5
        } else {
            risk_score / opportunity.signal_events.len() as f64
        };
        1.0 - avg_risk
    }
}

impl Default for DefaultAlphaScorer {
    fn default() -> Self {
        Self::new()
    }
}

impl AlphaScorer for DefaultAlphaScorer {
    fn score(&self, opportunity: &Opportunity) -> AlphaScore {
        let freshness = self.calculate_freshness(opportunity);
        let rarity = self.calculate_rarity(opportunity);
        let significance = self.calculate_significance(opportunity);
        let tradability = self.calculate_tradability(opportunity);
        let confidence = self.calculate_confidence(opportunity);
        let risk_adjustment = self.calculate_risk_adjustment(opportunity);

        let total = freshness * rarity * significance * tradability * confidence * risk_adjustment;

        AlphaScore {
            freshness_score: freshness * self.freshness_weight,
            novelty_score: rarity * self.rarity_weight,
            entity_importance_score: significance * self.significance_weight,
            flow_score: tradability * self.tradability_weight,
            execution_score: confidence * self.confidence_weight,
            risk_score: risk_adjustment * self.risk_weight,
            evidence_score: opportunity
                .signal_events
                .iter()
                .map(|e| e.evidence.len() as f64)
                .sum::<f64>()
                / (opportunity.signal_events.len() as f64).max(1.0),
            decay_score: 1.0,
            total,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OpportunityBook {
    pub opportunities: Vec<Opportunity>,
    pub max_size: usize,
    pub rerank_on_update: bool,
}

impl OpportunityBook {
    pub fn new(max_size: usize) -> Self {
        Self {
            opportunities: Vec::new(),
            max_size,
            rerank_on_update: true,
        }
    }

    pub fn with_rerank(mut self, rerank: bool) -> Self {
        self.rerank_on_update = rerank;
        self
    }

    pub fn update(&mut self, new_signals: Vec<SignalEvent>, alpha_scorer: &dyn AlphaScorer) {
        let mut signal_by_entity: HashMap<String, Vec<SignalEvent>> = HashMap::new();

        for signal in new_signals {
            let key = Self::entity_key_for_signal(&signal);
            signal_by_entity.entry(key).or_default().push(signal);
        }

        for (key, signals) in signal_by_entity {
            if let Some(pos) = self
                .opportunities
                .iter()
                .position(|o| Self::entity_key(o) == key)
            {
                self.opportunities[pos].signal_events.extend(signals);
                self.opportunities[pos].alpha_score = alpha_scorer.score(&self.opportunities[pos]);
            } else {
                let opportunity_id = Uuid::new_v4();
                let alpha_score = {
                    let temp_opp = Opportunity {
                        opportunity_id,
                        signal_events: signals.clone(),
                        alpha_score: AlphaScore::default(),
                        execution_plan: None,
                        created_at: Utc::now(),
                        expires_at: None,
                    };
                    alpha_scorer.score(&temp_opp)
                };
                let opp = Opportunity {
                    opportunity_id,
                    signal_events: signals,
                    alpha_score,
                    execution_plan: None,
                    created_at: Utc::now(),
                    expires_at: None,
                };
                self.opportunities.push(opp);
            }
        }

        if self.rerank_on_update {
            self.opportunities.sort_by(|a, b| {
                b.alpha_score
                    .total
                    .partial_cmp(&a.alpha_score.total)
                    .unwrap()
            });
        }

        if self.opportunities.len() > self.max_size {
            self.opportunities.truncate(self.max_size);
        }
    }

    pub fn get_top(&self, limit: usize) -> Vec<&Opportunity> {
        self.opportunities.iter().take(limit).collect()
    }

    pub fn remove(&mut self, opportunity_id: &Uuid) -> Option<Opportunity> {
        if let Some(pos) = self
            .opportunities
            .iter()
            .position(|o| &o.opportunity_id == opportunity_id)
        {
            Some(self.opportunities.remove(pos))
        } else {
            None
        }
    }

    pub fn evict_expired(&mut self, now: DateTime<Utc>, decay_policy: &DecayPolicy) {
        self.opportunities.retain(|opp| {
            if let Some(expires_at) = opp.expires_at {
                if now > expires_at {
                    return false;
                }
            }
            let age_secs = (now.timestamp() - opp.created_at.timestamp()).unsigned_abs();
            let decayed_score = decay_policy.apply_decay(opp.alpha_score.total, age_secs);
            decayed_score >= decay_policy.min_score_threshold
        });
    }

    fn entity_key(opportunity: &Opportunity) -> String {
        let mut entities: Vec<String> = Vec::new();
        for event in &opportunity.signal_events {
            entities.extend(event.entity_refs.tokens.clone());
            entities.extend(event.entity_refs.pools.clone());
            entities.extend(event.entity_refs.wallets.clone());
        }
        entities.sort();
        entities.dedup();
        entities.join("|")
    }

    fn entity_key_for_signal(signal: &SignalEvent) -> String {
        let mut entities: Vec<String> = Vec::new();
        entities.extend(signal.entity_refs.tokens.clone());
        entities.extend(signal.entity_refs.pools.clone());
        entities.extend(signal.entity_refs.wallets.clone());
        entities.sort();
        entities.dedup();
        entities.join("|")
    }
}

impl Default for OpportunityBook {
    fn default() -> Self {
        Self::new(100)
    }
}

#[derive(Clone)]
pub struct SignalSupervisor {
    subscriptions: HashMap<SubscriptionId, SignalSubscription>,
    pub opportunity_book: OpportunityBook,
    pub decay_policy: DecayPolicy,
}

impl SignalSupervisor {
    pub fn new(opportunity_book_size: usize, decay_policy: DecayPolicy) -> Self {
        Self {
            subscriptions: HashMap::new(),
            opportunity_book: OpportunityBook::new(opportunity_book_size),
            decay_policy,
        }
    }

    pub fn with_default_decay(opportunity_book_size: usize) -> Self {
        Self::new(opportunity_book_size, DecayPolicy::default())
    }

    pub fn subscribe(&mut self, subscription: SignalSubscription) -> SubscriptionId {
        let id = subscription.subscription_id;
        self.subscriptions.insert(id, subscription);
        id
    }

    pub fn unsubscribe(&mut self, subscription_id: &SubscriptionId) -> Option<SignalSubscription> {
        self.subscriptions.remove(subscription_id)
    }

    pub fn process_signals(&mut self, signals: Vec<SignalEvent>, alpha_scorer: &dyn AlphaScorer) {
        let relevant_signals: Vec<SignalEvent> = signals
            .into_iter()
            .filter(|signal| {
                self.subscriptions
                    .values()
                    .any(|sub| sub.matches_signal(signal))
            })
            .collect();

        if !relevant_signals.is_empty() {
            self.opportunity_book.update(relevant_signals, alpha_scorer);
        }

        self.opportunity_book
            .evict_expired(Utc::now(), &self.decay_policy);
    }

    pub fn get_opportunities(&self, limit: usize) -> Vec<Opportunity> {
        self.opportunity_book
            .get_top(limit)
            .into_iter()
            .cloned()
            .collect()
    }

    pub fn get_subscription(&self, id: &SubscriptionId) -> Option<&SignalSubscription> {
        self.subscriptions.get(id)
    }

    pub fn num_subscriptions(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn num_opportunities(&self) -> usize {
        self.opportunity_book.opportunities.len()
    }
}

impl Default for SignalSupervisor {
    fn default() -> Self {
        Self::with_default_decay(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use data_core::types::{Evidence, SignalSource};

    fn create_test_signal(family: SignalFamily, chain: &str) -> SignalEvent {
        SignalEvent {
            signal_id: SignalId(Uuid::new_v4().to_string()),
            signal_family: family,
            chain_id: ChainId(chain.to_string()),
            block_number: 12345,
            tx_hash: TxHash("0xtest".to_string()),
            observed_at: Utc::now(),
            source: SignalSource {
                skill_id: "test-skill".to_string(),
                adapter_kind: "mock".to_string(),
                endpoint: "mock://test".to_string(),
            },
            entity_refs: EntityRefs {
                tokens: vec!["token1".to_string()],
                pools: vec![],
                wallets: vec!["0xwallet".to_string()],
                contracts: vec![],
            },
            features: HashMap::from([("flow_amount".to_string(), 1000000.0)]),
            evidence: vec![Evidence {
                evidence_type: "test".to_string(),
                description: "test evidence".to_string(),
                source_url: None,
            }],
            confidence: 0.9,
            freshness_ms: 1000,
            reorg_safe: true,
        }
    }

    fn create_test_opportunity(signals: Vec<SignalEvent>) -> Opportunity {
        let alpha_score = AlphaScore {
            freshness_score: 0.8,
            novelty_score: 0.7,
            entity_importance_score: 0.6,
            flow_score: 0.9,
            execution_score: 0.85,
            risk_score: 0.95,
            evidence_score: 1.0,
            decay_score: 1.0,
            total: 0.3,
        };
        Opportunity {
            opportunity_id: Uuid::new_v4(),
            signal_events: signals,
            alpha_score,
            execution_plan: None,
            created_at: Utc::now(),
            expires_at: None,
        }
    }

    #[test]
    fn test_subscription_id_generation() {
        let id1 = SubscriptionId::new();
        let id2 = SubscriptionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_signal_subscription_matching() {
        let sub = SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap, SignalFamily::Mempool],
            vec![ChainId("ethereum".to_string())],
            EntityFilters::default(),
            ExecutionAction::Alert,
        );

        let matching_signal = create_test_signal(SignalFamily::Swap, "ethereum");
        assert!(sub.matches_signal(&matching_signal));

        let non_matching_family = create_test_signal(SignalFamily::Bridge, "ethereum");
        assert!(!sub.matches_signal(&non_matching_family));

        let non_matching_chain = create_test_signal(SignalFamily::Swap, "polygon");
        assert!(!sub.matches_signal(&non_matching_chain));
    }

    #[test]
    fn test_entity_filters_matching() {
        let filters = EntityFilters {
            tokens: Some(vec!["token1".to_string(), "token2".to_string()]),
            wallets: None,
            pools: None,
            contracts: None,
        };
        let sub = SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap],
            vec![],
            filters,
            ExecutionAction::Alert,
        );

        let signal = create_test_signal(SignalFamily::Swap, "ethereum");
        assert!(sub.matches_signal(&signal));
    }

    #[test]
    fn test_decay_policy_apply() {
        let policy = DecayPolicy::new(300, 0.05, 0.1);
        let score = 1.0;
        let age_secs = 60;
        let decayed = policy.apply_decay(score, age_secs);
        assert!(decayed < score);
        assert!(decayed > 0.0);
    }

    #[test]
    fn test_decay_policy_stale_threshold() {
        let policy = DecayPolicy::new(300, 0.05, 0.1);
        let score = 1.0;
        let age_secs = 400;
        let decayed = policy.apply_decay(score, age_secs);
        assert_eq!(decayed, 0.0);
    }

    #[test]
    fn test_default_alpha_scorer_calculation() {
        let scorer = DefaultAlphaScorer::new();
        let signals = vec![create_test_signal(SignalFamily::Swap, "ethereum")];
        let opp = create_test_opportunity(signals);
        let alpha = scorer.score(&opp);
        assert!(alpha.total >= 0.0);
        assert!(alpha.total <= 1.0);
    }

    #[test]
    fn test_alpha_scorer_trait_impl() {
        let scorer = DefaultAlphaScorer::new();
        let signals = vec![create_test_signal(SignalFamily::Mempool, "ethereum")];
        let opp = create_test_opportunity(signals);
        let total = scorer.score_total(&opp);
        assert!(total >= 0.0);
    }

    #[test]
    fn test_opportunity_book_new() {
        let book = OpportunityBook::new(50);
        assert_eq!(book.max_size, 50);
        assert!(book.opportunities.is_empty());
        assert!(book.rerank_on_update);
    }

    #[test]
    fn test_opportunity_book_update() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();
        let signals = vec![
            create_test_signal(SignalFamily::Swap, "ethereum"),
            create_test_signal(SignalFamily::Swap, "ethereum"),
        ];
        book.update(signals, &scorer);
        assert_eq!(book.opportunities.len(), 1);
    }

    #[test]
    fn test_opportunity_book_update_different_entities() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();

        let sig1 = {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.entity_refs.tokens = vec!["tokenA".to_string()];
            sig
        };
        let sig2 = {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.entity_refs.tokens = vec!["tokenB".to_string()];
            sig
        };

        book.update(vec![sig1], &scorer);
        book.update(vec![sig2], &scorer);
        assert_eq!(book.opportunities.len(), 2);
    }

    #[test]
    fn test_opportunity_book_update_merges_same_entity() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();

        let sig1 = {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.signal_id = SignalId("sig1".to_string());
            sig
        };
        let sig2 = {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.signal_id = SignalId("sig2".to_string());
            sig
        };

        book.update(vec![sig1], &scorer);
        book.update(vec![sig2], &scorer);
        assert_eq!(book.opportunities.len(), 1);
        assert_eq!(book.opportunities[0].signal_events.len(), 2);
    }

    #[test]
    fn test_opportunity_book_get_top() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();

        for i in 0..5 {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.features
                .insert("flow_amount".to_string(), (i as f64) * 1000000.0);
            book.update(vec![sig], &scorer);
        }

        let top3 = book.get_top(3);
        assert_eq!(top3.len(), 3);
    }

    #[test]
    fn test_opportunity_book_remove() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();
        let signals = vec![create_test_signal(SignalFamily::Swap, "ethereum")];
        book.update(signals, &scorer);

        let opp_id = book.opportunities[0].opportunity_id;
        let removed = book.remove(&opp_id);
        assert!(removed.is_some());
        assert!(book.opportunities.is_empty());
    }

    #[test]
    fn test_opportunity_book_remove_nonexistent() {
        let mut book = OpportunityBook::new(10);
        let fake_id = Uuid::new_v4();
        let removed = book.remove(&fake_id);
        assert!(removed.is_none());
    }

    #[test]
    fn test_opportunity_book_max_size() {
        let mut book = OpportunityBook::new(3);
        let scorer = DefaultAlphaScorer::new();

        for i in 0..5 {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.entity_refs.tokens = vec![format!("token{}", i)];
            book.update(vec![sig], &scorer);
        }

        assert_eq!(book.opportunities.len(), 3);
    }

    #[test]
    fn test_opportunity_book_evict_expired() {
        let mut book = OpportunityBook::new(10);
        let scorer = DefaultAlphaScorer::new();
        let policy = DecayPolicy::default();

        let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
        sig.freshness_ms = 1000000;
        book.update(vec![sig], &scorer);

        let old_opp = &mut book.opportunities[0];
        old_opp.created_at = Utc::now() - chrono::Duration::minutes(10);

        book.evict_expired(Utc::now(), &policy);
    }

    #[test]
    fn test_signal_supervisor_new() {
        let supervisor = SignalSupervisor::with_default_decay(50);
        assert_eq!(supervisor.num_subscriptions(), 0);
        assert_eq!(supervisor.num_opportunities(), 0);
    }

    #[test]
    fn test_signal_supervisor_subscribe() {
        let mut supervisor = SignalSupervisor::with_default_decay(50);
        let sub = SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap],
            vec![],
            EntityFilters::default(),
            ExecutionAction::Alert,
        );
        let id = supervisor.subscribe(sub);
        assert!(supervisor.get_subscription(&id).is_some());
        assert_eq!(supervisor.num_subscriptions(), 1);
    }

    #[test]
    fn test_signal_supervisor_unsubscribe() {
        let mut supervisor = SignalSupervisor::with_default_decay(50);
        let sub = SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap],
            vec![],
            EntityFilters::default(),
            ExecutionAction::Alert,
        );
        let id = supervisor.subscribe(sub);
        let removed = supervisor.unsubscribe(&id);
        assert!(removed.is_some());
        assert!(supervisor.get_subscription(&id).is_none());
        assert_eq!(supervisor.num_subscriptions(), 0);
    }

    #[test]
    fn test_signal_supervisor_process_signals() {
        let mut supervisor = SignalSupervisor::with_default_decay(50);
        supervisor.subscribe(SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap],
            vec![],
            EntityFilters::default(),
            ExecutionAction::Alert,
        ));

        let signals = vec![create_test_signal(SignalFamily::Swap, "ethereum")];
        let scorer = DefaultAlphaScorer::new();
        supervisor.process_signals(signals, &scorer);
        assert_eq!(supervisor.num_opportunities(), 1);
    }

    #[test]
    fn test_signal_supervisor_process_signals_filters_by_subscription() {
        let mut supervisor = SignalSupervisor::with_default_decay(50);
        supervisor.subscribe(SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Mempool],
            vec![],
            EntityFilters::default(),
            ExecutionAction::Alert,
        ));

        let signals = vec![create_test_signal(SignalFamily::Swap, "ethereum")];
        let scorer = DefaultAlphaScorer::new();
        supervisor.process_signals(signals, &scorer);
        assert_eq!(supervisor.num_opportunities(), 0);
    }

    #[test]
    fn test_signal_supervisor_get_opportunities() {
        let mut supervisor = SignalSupervisor::with_default_decay(50);
        let scorer = DefaultAlphaScorer::new();

        supervisor.subscribe(SignalSubscription::new(
            "test-skill".to_string(),
            vec![SignalFamily::Swap],
            vec![],
            EntityFilters::default(),
            ExecutionAction::Alert,
        ));

        for i in 0..3 {
            let mut sig = create_test_signal(SignalFamily::Swap, "ethereum");
            sig.entity_refs.tokens = vec![format!("token{}", i)];
            supervisor.process_signals(vec![sig], &scorer);
        }

        let opps = supervisor.get_opportunities(2);
        assert_eq!(opps.len(), 2);
    }

    #[test]
    fn test_default_alpha_scorer_weights() {
        let scorer = DefaultAlphaScorer::with_weights(2.0, 0.5, 1.5, 1.0, 0.8, 0.9);
        let signals = vec![create_test_signal(SignalFamily::Swap, "ethereum")];
        let opp = create_test_opportunity(signals);
        let alpha = scorer.score(&opp);
        assert!(alpha.freshness_score > alpha.novelty_score);
    }

    #[test]
    fn test_opportunity_book_with_rerank_false() {
        let book = OpportunityBook::new(10).with_rerank(false);
        assert!(!book.rerank_on_update);
    }

    #[test]
    fn test_signal_supervisor_default() {
        let supervisor = SignalSupervisor::default();
        assert_eq!(supervisor.num_subscriptions(), 0);
        assert_eq!(supervisor.decay_policy.staleness_threshold_secs, 300);
    }
}
