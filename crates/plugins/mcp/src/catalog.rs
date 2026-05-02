// Copyright (c) 2026 The State Key Laboratory of Blockchain and Data Security, Zhejiang University
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use crate::handlers::job_api;
use crate::registry::{ToolExecutor, ToolFuture, ToolRegistry};
use crate::tool_adapters::legacy_json_tool;
use crate::tools::{all_tool_definitions, validate_tool_definitions};

fn collect_definitions() -> HashMap<String, crate::protocol::ToolDefinition> {
    all_tool_definitions()
        .into_iter()
        .map(|definition| (definition.name.clone(), definition))
        .collect()
}

fn require_definition(
    definitions: &mut HashMap<String, crate::protocol::ToolDefinition>,
    name: &str,
) -> crate::protocol::ToolDefinition {
    definitions
        .remove(name)
        .unwrap_or_else(|| panic!("missing MCP tool definition for {name}"))
}

fn executor_from_fn(
    f: for<'a> fn(serde_json::Value, &'a crate::state::AppState) -> ToolFuture<'a>,
) -> Arc<ToolExecutor> {
    Arc::new(f)
}

fn intent_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::intent::handle(args, state))
}

fn search_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::search::handle(args, state))
}

fn evaluate_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::evaluate::handle(args, state))
}

fn feedback_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::feedback::handle(args, state))
}

fn download_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::download::handle(args, state))
}

fn lookup_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::data_skill::lookup(args, state))
}

fn schema_probe_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::data_skill::schema_probe(args, state))
}

fn query_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::data_skill::query(args, state))
}

fn download_by_skill_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::data_skill::download_via_skill(args, state))
}

fn purchase_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::purchase::handle(args, state))
}

fn verify_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::misc::handle_verify(args, state))
}

fn publish_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::misc::handle_publish(args, state))
}

fn reviews_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::reviews::handle(args, state))
}

fn bt_download_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::bt_download::handle(args, state))
}

fn bt_preview_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::bt_download::handle_preview(args, state))
}

fn bt_stats_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::bt_download::handle_stats(args, state))
}

fn pan_search_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::pan_search::handle(args, state))
}

fn delegate_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::delegate::handle(args, state))
}

fn job_status_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::job_api::status(args, state))
}

fn job_approve_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::job_api::approve(args, state))
}

fn job_cancel_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::job_api::cancel(args, state))
}

fn job_artifacts_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::job_api::artifacts(args, state))
}

fn memory_history_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::memory_history::handle(args, state))
}

fn signal_subscribe_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::signal_subscribe::handle_signal_subscribe(
        args, state,
    ))
}

fn signal_unsubscribe_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::signal_subscribe::handle_signal_unsubscribe(args, state))
}

fn opportunity_rank_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::opportunity_rank::handle_opportunity_rank(
        args, state,
    ))
}

fn wallet_watch_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::opportunity_rank::handle_wallet_watch(
        args, state,
    ))
}

fn protocol_monitor_executor<'a>(
    args: serde_json::Value,
    state: &'a crate::state::AppState,
) -> ToolFuture<'a> {
    Box::pin(crate::handlers::opportunity_rank::handle_protocol_monitor(
        args, state,
    ))
}

pub fn build_registry() -> ToolRegistry {
    let mut definitions = collect_definitions();
    let mut registry = ToolRegistry::new();

    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "intent_parse"),
        executor_from_fn(intent_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_search"),
        executor_from_fn(search_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_evaluate"),
        executor_from_fn(evaluate_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_purchase"),
        executor_from_fn(purchase_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_download"),
        executor_from_fn(download_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_lookup"),
        executor_from_fn(lookup_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_schema_probe"),
        executor_from_fn(schema_probe_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_query"),
        executor_from_fn(query_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_download_by_skill"),
        executor_from_fn(download_by_skill_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_feedback"),
        executor_from_fn(feedback_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_reviews"),
        executor_from_fn(reviews_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_verify"),
        executor_from_fn(verify_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_publish"),
        executor_from_fn(publish_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_bt_download"),
        executor_from_fn(bt_download_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_bt_preview"),
        executor_from_fn(bt_preview_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "dataset_bt_stats"),
        executor_from_fn(bt_stats_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "pan_search"),
        executor_from_fn(pan_search_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "data_task_delegate"),
        executor_from_fn(delegate_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "data_task_status"),
        executor_from_fn(job_status_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "data_task_approve"),
        executor_from_fn(job_approve_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "data_task_cancel"),
        executor_from_fn(job_cancel_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "data_task_artifacts"),
        executor_from_fn(job_artifacts_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "memory_history"),
        executor_from_fn(memory_history_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "signal_subscribe"),
        executor_from_fn(signal_subscribe_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "signal_unsubscribe"),
        executor_from_fn(signal_unsubscribe_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "opportunity_rank"),
        executor_from_fn(opportunity_rank_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "wallet_watch"),
        executor_from_fn(wallet_watch_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "protocol_monitor"),
        executor_from_fn(protocol_monitor_executor),
    ));

    fn ingest_jobs_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::ingest_jobs(args, state))
    }

    fn ingest_status_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::ingest_status(args, state))
    }

    fn ingest_resume_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::ingest_resume(args, state))
    }

    fn ingest_cancel_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::ingest_cancel(args, state))
    }

    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "ingest_jobs"),
        executor_from_fn(ingest_jobs_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "ingest_status"),
        executor_from_fn(ingest_status_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "ingest_resume"),
        executor_from_fn(ingest_resume_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "ingest_cancel"),
        executor_from_fn(ingest_cancel_executor),
    ));

    fn download_jobs_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::download_jobs(args, state))
    }

    fn download_status_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::download_status(args, state))
    }

    fn download_pause_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::download_pause(args, state))
    }

    fn download_resume_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::download_resume(args, state))
    }

    fn download_cancel_executor<'a>(
        args: serde_json::Value,
        state: &'a crate::state::AppState,
    ) -> ToolFuture<'a> {
        Box::pin(job_api::download_cancel(args, state))
    }

    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "download_jobs"),
        executor_from_fn(download_jobs_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "download_status"),
        executor_from_fn(download_status_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "download_pause"),
        executor_from_fn(download_pause_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "download_resume"),
        executor_from_fn(download_resume_executor),
    ));
    registry.register(legacy_json_tool(
        require_definition(&mut definitions, "download_cancel"),
        executor_from_fn(download_cancel_executor),
    ));

    let all_definitions = registry.list_definitions();
    validate_tool_definitions(&all_definitions)
        .unwrap_or_else(|message| panic!("invalid MCP tool registry: {message}"));

    registry
}
