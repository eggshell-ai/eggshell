---
name: bump-version
description: >-
  Explains how to bump the version of the Eggshell desktop Tauri application across all configuration files, updater endpoints, manifests, and frontend code. Use when upgrading or releasing a new version.
---

# Bumping the Application Version

When bumping the version in this Tauri project (e.g., from `1.0.5` to `1.0.6`), multiple files across the Rust backend, Tauri configuration, and frontend need to be synchronized.

---

## 1. Files That Must Be Updated

Update the version string `X.Y.Z` in the following files:

### 1. `package.json`
- **Location**: [`package.json`](file:///e:/Faheem/Projects/eggshell-desktop/package.json)
- **Field**: `"version": "X.Y.Z"`

### 2. `src-tauri/Cargo.toml`
- **Location**: [`src-tauri/Cargo.toml`](file:///e:/Faheem/Projects/eggshell-desktop/src-tauri/Cargo.toml)
- **Field**:
  ```toml
  [package]
  name = "Eggshell"
  version = "X.Y.Z"
  ```

### 3. `src-tauri/tauri.conf.json`
- **Location**: [`src-tauri/tauri.conf.json`](file:///e:/Faheem/Projects/eggshell-desktop/src-tauri/tauri.conf.json)
- **Fields** (2 places):
  1. Root application version:
     ```json
     "version": "X.Y.Z"
     ```
  2. Updater download endpoint:
     ```json
     "plugins": {
       "updater": {
         "endpoints": [
           "https://github.com/eggshell-ai/eggshell/releases/download/vX.Y.Z/latest.json"
         ]
       }
     }
     ```

### 4. `src-tauri/config.yaml`
- **Location**: [`src-tauri/config.yaml`](file:///e:/Faheem/Projects/eggshell-desktop/src-tauri/config.yaml)
- **Field**:
  ```yaml
  version: X.Y.Z
  ```

### 5. `src/FeedbackPopup.tsx`
- **Location**: [`src/FeedbackPopup.tsx`](file:///e:/Faheem/Projects/eggshell-desktop/src/FeedbackPopup.tsx)
- **Field**: Fallback state initialization for `appVersion`:
  ```tsx
  const [appVersion, setAppVersion] = useState("X.Y.Z");
  ```

---

## 2. Syncing Lockfiles and Verifying Builds

After modifying the files above, run the following verification steps:

1. **Update `src-tauri/Cargo.lock`**:
   ```powershell
   cargo check --manifest-path src-tauri/Cargo.toml
   ```
   *(or run `cargo check` directly inside `src-tauri/`)*

2. **Verify Frontend Build**:
   ```powershell
   pnpm run build
   ```

3. **Verify Git Changes**:
   Check `git diff` to make sure all version occurrences match `X.Y.Z`:
   ```powershell
   git diff
   ```
