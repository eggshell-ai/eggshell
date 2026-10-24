# Taste

- Prefers `patch_file` (OpenAI apply_patch format) over `write_file` for incremental modifications to existing files in documentation/examples. Confidence: 0.7
- Prefers OpenAI apply_patch examples to use a bare `@@` section header instead of unified-diff line numbers such as `@@ -15,4 +15,10 @@`. Confidence: 0.9
- Menu item JSON examples should include a `permission` field named in `{resource}.{action}` form (e.g. `products.view`). Confidence: 0.8
