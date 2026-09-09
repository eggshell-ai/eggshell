use super::Skill;

// Paste the full customization and branding skill instructions in this constant.
const CONTENT: &str = "# Customization and Branding Skill

This skill explains how to customize the branding details of the generated application. It covers changing the application title and other branding variables.

## Overview

All application customization variables are centralized in a single file in the frontend:

- **`src/customization.js`**: A JavaScript file exporting a default object containing all customization variables (currently only `title`).

To read and write this file, use the `read_file` and `write_file` tools.

---

## Tooling & File Access

Use the `read_file` and `write_file` tools to interact with frontend files:

- **`read_file`**:
  - `shell`: `\"frontend\"`
  - `path`: Relative path inside `src` directory (e.g., `customization.js`)
- **`write_file`**:
  - `shell`: `\"frontend\"`
  - `path`: Relative path inside `src` directory
  - `content`: The complete updated content to write

---

## Customization File Structure

The customization file is located at `customization.js` in the frontend `src` directory. It exports a default object with all customization variables:

```javascript
// ==============================|| APP CUSTOMIZATION ||============================== //
// Central place for all application customization variables.
// Add any future customization options (logo, colors, footer text, etc.) here.

const customization = {
  // Application title (used for the browser tab / document title)
  title: 'Eggshell Admin'
};

export default customization;
```

The `title` value is automatically applied to the application's document title (browser tab) via the root layout metadata. No other files need to be modified when changing the title.

---

## Step-by-Step Implementation

### Step 1: Read the Current Customization File

Before modifying the customization file, read `customization.js` using `read_file` to see the current values and preserve the existing structure.

**Tool call example:**
```javascript
read_file({
  shell: \"frontend\",
  path: \"customization.js\"
})
```

### Step 2: Update the Title

Update `customization.js` using `write_file`, changing the `title` value while preserving all other existing variables.

**Tool call example:**
```javascript
write_file({
  shell: \"frontend\",
  path: \"customization.js\",
  content: `// ==============================|| APP CUSTOMIZATION ||============================== //
// Central place for all application customization variables.
// Add any future customization options (logo, colors, footer text, etc.) here.

const customization = {
  // Application title (used for the browser tab / document title)
  title: 'My Custom App'
};

export default customization;
`
})
```

---

## Best Practices

1. **Always read before writing**: Use `read_file` first on `customization.js` so you retain any existing customization variables that have been added.
2. **Single source of truth**: Never hardcode branding values (like the title) in other files. Always change them in `customization.js`.
3. **Preserve structure**: Keep the comments and object structure of the file intact when updating values.
4. **Future variables**: New customization variables (logo, colors, footer text, etc.) should be added to this same file and wired into the application where needed.

---

## Logo Replacement

The application logo lives in the frontend `public` directory as `logo.svg`. When the user uploads a new logo file, use the `move_file` tool to place it in the project.

### Option A: Replace the existing logo (same format)

If the uploaded file is an SVG (or matches the current extension), move it over the existing logo:

**Tool call example:**
```javascript
move_file({
  source: \"/absolute/path/to/uploaded-logo.svg\",
  shell: \"frontend\",
  path: \"../public/logo.svg\"
})
```

The path may escape the `src` directory with `../` to reach the `public` directory. The existing `logo.svg` is overwritten and the new logo is used immediately.

### Option B: Different file extension

If the uploaded logo has a different extension (e.g. `logo.png`), move it into `public` with its own name and update `customization.js` so the application references the new file:

**Step 1 — move the file:**
```javascript
move_file({
  source: \"/absolute/path/to/uploaded-logo.png\",
  shell: \"frontend\",
  path: \"../public/logo.png\"
})
```

**Step 2 — update the customization file:**
```javascript
write_file({
  shell: \"frontend\",
  path: \"customization.js\",
  content: `// ==============================|| APP CUSTOMIZATION ||============================== //
// Central place for all application customization variables.
// Add any future customization options (logo, colors, footer text, etc.) here.

const customization = {
  // Application title (used for the browser tab / document title)
  title: 'Eggshell Admin',
  // Logo file served from the public directory
  logo: '/logo.png'
};

export default customization;
`
})
```

### Logo best practices

1. **Prefer SVG**: vector logos scale cleanly at any size.
2. **Keep the filename stable**: overwriting `logo.svg` avoids touching any other file.
3. **Only change the extension when needed**: if the upload cannot be an SVG, add a `logo` variable to `customization.js` pointing at the new file instead of renaming references elsewhere.";

/// Skill for customizing application branding details such as the title.
pub struct CustomizationBrandingSkill {
    tags: Vec<String>,
}

impl CustomizationBrandingSkill {
    pub fn new() -> Self {
        Self {
            tags: vec!["customization", "branding", "title"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl Default for CustomizationBrandingSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for CustomizationBrandingSkill {
    fn name(&self) -> &str { "customization_branding" }
    fn description(&self) -> &str {
        "Teaches the agent how to customize application branding details such as the title"
    }
    fn content(&self) -> &str { CONTENT }
    fn category(&self) -> Option<&str> { Some("customization") }
    fn tags(&self) -> &[String] { &self.tags }
}
