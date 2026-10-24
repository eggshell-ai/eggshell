# Conditional Visiblity

Use `visibleWhen` when a field should be displayed only under a condition.

Example:

```javascript
{
  name: "sourceDescription",
  type: "text",
  label: "Other Source Description",
  visibleWhen: "data.source == 'other'"
}
```

This means the field is visible only when `source` equals `"other"`.

It does **not** make the field conditionally required on the backend. If a field is required only under a condition, use mirrored frontend/backend validation hooks as described in `docs/validation.md` combined with conditional visibility