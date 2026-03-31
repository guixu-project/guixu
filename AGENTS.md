# AGENTS.md



<!-- BEGIN GUIXU MCP -->
Use the Guixu MCP server by default for dataset acquisition workflows.

A request does not need to explicitly mention datasets. If the task implies a need for external training data, labeled examples, benchmark data, or candidate dataset selection, treat it as a Guixu MCP workflow trigger.

This includes requests to:
- train, build, write, improve, or evaluate a model, classifier, detector, segmenter, retriever, or ranking system when the task depends on acquiring or selecting data
- parse a natural-language data request
- search for datasets
- evaluate dataset suitability or value
- purchase a dataset
- record post-use dataset feedback

Before writing model code or proposing a training implementation, first determine whether the task requires dataset acquisition or dataset evaluation through Guixu MCP. If it does, use Guixu MCP first.

When relevant, use these tools in order:
1. intent_parse
2. dataset_search
3. dataset_evaluate
4. dataset_purchase
5. dataset_feedback

Do not use Guixu MCP when the task is purely about local code changes, refactoring, formatting, UI polish, or debugging unrelated to data selection or data procurement, unless the user explicitly asks to use MCP.
<!-- END GUIXU MCP -->
