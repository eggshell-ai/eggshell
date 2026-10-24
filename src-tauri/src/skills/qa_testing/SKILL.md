---
name: qa_testing
description: Teaches the agent how to test generated web applications using integrated browser tools, navigate routes, verify forms and CRUD features, and detect issues.
category: quality_assurance
tags:
  - qa
  - testing
  - browser
  - verification
  - visual
---

# QA & Visual Verification Skill

This skill teaches the agent how to run visual and functional QA testing on a generated Eggshell web application using the integrated browser tools (`browser_open`, `browser_navigate`, `browser_click`, `browser_fill`, `browser_get_page_content`, `browser_get_console_errors`), `read_file`, and `read_schema`.

---

## 1. Credentials & Application Setup

When testing the application:
- **Default Application URL**: `http://localhost:4000`
- **Default Admin Login Credentials**:
  - Email / Username: `admin@dummy-project.com`
  - Password: `12345678`

If the browser is on the login page:
1. Use `browser_fill` on `input[type="email"]` or `input[name="username"]` or `#email` with `admin@dummy-project.com`.
2. Use `browser_fill` on `input[type="password"]` with `12345678`.
3. Use `browser_click` on the submit button (e.g. `button[type="submit"]` or `"Login"`).
4. Wait and navigate to the target page or dashboard.

---

## 2. What Things Are NOT Created by the Implementation Agent (Audit Checklist)

Implementation agents frequently leave gaps or introduce mismatches. The QA agent MUST check:

1. **Navigation & Routes**:
   - Inspect `frontend/src/menu-items/menu.json` via `read_file`.
   - Verify every route listed in `menu.json` actually resolves in the browser (does not render a 404, white screen, or crash).

2. **Database & Schema Inconsistencies**:
   - Compare `schema.json` against backend Symfony entity classes under `backend/src/Entity/`.
   - Ensure getters, setters, column types, and relational foreign keys match between resource schemas and Doctrine mappings.

3. **Missing or Broken Standard Resource Actions**:
   - On table/list pages: Does the table render rows? Are headers aligned with the schema?
   - Does clicking **Add / Create** open the modal/drawer/page?
   - Do required inputs have proper labels and validation indicators?

4. **Frontend & Backend Validation Discrepancies**:
   - Test submitting an empty form to confirm frontend required validation matches backend rules.
   - Test submitting valid data to confirm successful HTTP 201/200 creation and record appearance in the table.

5. **Console & Runtime Errors**:
   - Call `browser_get_console_errors` on each page visited to detect unhandled exceptions, Ant Design prop warnings, or broken React state transitions.

---

## 3. QA Testing Workflow

Follow this systematic sequence:

1. **Discovery**:
   - Call `read_file` on `shell: "frontend", path: "menu-items/menu.json"` to see all registered features and navigation routes.
   - Call `read_schema` to identify known entities, their fields, and primary requirements.

2. **Browser Launch & Authentication**:
   - Call `browser_open` with `url: "http://localhost:4000"`.
   - If redirected to login, authenticate using `admin@dummy-project.com` / `12345678`.

3. **Exploration & Navigation**:
   - For each route discovered in `menu.json`:
     - Call `browser_navigate` to that route.
     - Call `browser_get_page_content` to check what headers, tables, or cards appear.
     - Call `browser_get_console_errors` to check for page runtime errors.

4. **Form & CRUD Verification**:
   - On the primary resource page:
     - Click the "Add" or "Create" button.
     - Fill in sample data with `browser_fill`.
     - Click submit and verify the record is added to the table.

5. **Final QA Verdict Report**:
   - Summarize your findings in a structured Markdown report:
     - **Routes Tested**: List of pages and HTTP/UI status.
     - **Visual & UI Checks**: Tables, forms, layout observations.
     - **Issues Found**: Grouped by Severity (Critical, Warning, Minor).
     - **Recommendations**: Exact fixes for files or schemas if issues were detected.
