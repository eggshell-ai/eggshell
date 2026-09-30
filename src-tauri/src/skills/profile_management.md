# User Profile Management Skill

This skill explains how to read and manage the current user profile, and how to add additional fields to the `profile` resource using `sync_schema`.

---

## 1. Architecture Overview

The system includes:
- **`App\Entity\User`**: Core user authentication entity storing credentials (`email`, `password`, `first_name`, `last_name`, `roles`).
- **`App\Entity\Profile`**: One-to-one related entity storing additional user profile data.
  - Linked to `User` via `user` (`#[ORM\OneToOne(targetEntity: User::class, inversedBy: 'profile')]`).
  - Contains default `avatar` field (`uploadDirectory: 'uploads/avatars'`).
  - Supports any dynamic fields added via `sync_schema`.
- **`App\Controller\ProfileController`**:
  - `GET /api/profile/me`: Returns the profile of the authenticated user (creates empty one if not present).
  - `POST /api/profile/me`: Updates profile data for the authenticated user (supporting multipart file uploads for avatar).
  - Standard CRUD endpoints `/api/profile` managed by `ResourceController`.
- **Frontend Resource (`src/resources/profile.ts`)**:
  - Declarative resource definition for `profile`.
- **Frontend Page (`src/app/(dashboard)/profile/page.jsx`)**:
  - Uses `ResourcePage` with `profileResource`. Any field added to `profile.ts` is automatically reflected in the profile interface.
- **Frontend Header Profile**:
  - Header profile button displays user avatar or capital initial fallback.
  - Clicking "Edit Profile" or "View Profile" navigates directly to `/profile`.

---

## 2. Reading Current User Profile

### In Backend PHP Controllers
When handling requests inside a Symfony controller:
```php
use App\Entity\Profile;
use App\Entity\User;

/** @var User $currentUser */
$currentUser = $this->getUser();

$profile = $this->entityManager->getRepository(Profile::class)->findOneBy(['user' => $currentUser]);
$avatar = $profile?->avatar;
```

### In Frontend React Components
Using `profileService`:
```typescript
import profileService from 'api/profileService';

const fetchProfile = async () => {
  const profile = await profileService.getMyProfile();
  console.log('Current profile:', profile);
};
```

---

## 3. Adding Additional Fields to Profile via `sync_schema`

When the user asks to add new profile fields (such as `bio`, `phone`, `job_title`, `website`, `address`, `birth_date`, `department`, etc.), use the `sync_schema` tool to update the `profile` resource.

### Important Guidelines:
1. Always preserve existing fields (`avatar`) when calling `sync_schema`.
2. Do not remove or modify the `user` relation.
3. Call `sync_schema` with the updated list of fields under `resources`.
4. Running `sync_schema` will automatically:
   - Update `templates/admin-panel/src/resources/profile.ts` (or `frontend/src/resources/profile.ts` in generated projects).
   - Update `App\Entity\Profile`.
   - Generate and run database migrations.
   - Update `schemas.json`.

### Example: Adding `phone`, `job_title`, and `bio`

```javascript
sync_schema({
  resources: [
    {
      name: "profile",
      endpoint: "/profile",
      fields: [
        {
          name: "avatar",
          type: "file",
          label: "Avatar",
          accept: "image/*",
          table: true,
          form: true
        },
        {
          name: "phone",
          type: "phone",
          label: "Phone Number",
          required: false,
          table: true,
          form: true
        },
        {
          name: "job_title",
          type: "text",
          label: "Job Title",
          required: false,
          table: true,
          form: true
        },
        {
          name: "bio",
          type: "textarea",
          label: "Bio / About Me",
          required: false,
          table: false,
          form: true
        }
      ]
    }
  ]
});
```

Because the frontend Profile page utilizes `<ResourcePage resource={profileResource} />`, any newly added fields (`job_title`, `phone`, `bio`, etc.) are instantly rendered in the profile table, edit forms, and detail views without needing manual React UI changes.
