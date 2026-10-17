# Project Templates (`templates/`)

Generated admin-panel projects are created by copying these templates. They are
bundled as Tauri resources (`../templates/**/*` in `tauri.conf.json`), so files
added here ship with the app.

- `templates/admin-panel/` → copied into the project's `frontend/` (Next.js).
- `templates/backend/` → copied into the project's `backend/` (Symfony).

The `Shell` implementations that copy and run them are `ReactShell` (`react.rs`)
and `SymfonyShell` (`symfony.rs`).

## `templates/admin-panel/` — Next.js frontend

Package name `mantis-material-next-js` (Mantis MUI admin theme + Ant Design).
Scripts: `dev` (`next dev -p 4000`), `build`, `start`, `lint`
(`eslint "src/**/*.{js,jsx,ts,tsx}"`), `lint:fix`, `prettier`. Deps include
Next 16, React 19, MUI 7, `antd` 6, `axios`, `formik`, `yup`, `swr`.
`src/` structure:

- `api/` — `apiService` (axios wrapper), `createCrudService.ts` (REST factory),
  `menu.js`, `notificationService`, `profileService`, `roleService`, `userService`.
- `app/` — Next App Router. Dashboard pages live under
  `app/(dashboard)/<route>/page.tsx`; each begins with `'use client';`.
- `components/` — `AuthGuard`, `ContainerWrapper`, `Loader`, `MainCard`,
  `ScrollTop`, plus `cards/`, `crud/`, `dashboard/`, `resources/`, `routes/`,
  `@extended/`, `logo/`, `third-party/`.
- `menu-items/` — `menu.json` (sidebar definition), `index.jsx`, `dashboard.jsx`,
  `management.jsx`.
- `resources/` — aggregated resource definitions (`users.ts`, `roles.ts`,
  `profile.ts`, `permissions.ts`) and `hooks/` (`index.ts`, per-resource hooks).
- `services/`, `types/`, `utils/`, `contexts/`, `hooks/`, `layout/`, `sections/`,
  `views/`, `data/`, `config.js`, `customization.js`.

### Key subsystems

- **Resource definition** — `utils/defineResource.ts` + `utils/field.ts`
  (`field.*` builder). See [`resource-system.md`](./resource-system.md).
- **CRUD UI** — `components/resources/` (`ResourcePage`, `ResourceForm`,
  `FormField`, `FilterField`, `ViewField`) and `components/crud/`
  (`ResourceGrid`, `DataTable`).
- **Validation** — `utils/validation.ts`, `utils/resourceValidationHooks.ts`;
  hooks under `resources/hooks/<resourceName>/`, registered in
  `resources/hooks/index.ts`.
- **Sidebar** — `menu-items/menu.json`; each item is
  `{ name: "Group.Item", route, icon, permission? }`.

## `templates/backend/` — Symfony backend

`composer.json` requires PHP ≥ 8.4, Symfony 8.1, Doctrine ORM/bundles/migrations,
`lexik/jwt-authentication-bundle`, `nelmio/cors-bundle`. Includes
`compose.yaml`/`compose.override.yaml`, `.env`/`.env.dev`, `bin/`.

`src/` structure:

- `Controller/` — `AnalyticsController`, `NotificationController`,
  `ProfileController`, `Auth/`.
- `Entity/` — `User`, `Role`, `Permission`, `Profile`, `Notification`,
  `NotificationUser` (generated entities extend `ResourceEntity`).
- `Resource/` — the resource framework: `ResourceController` (base CRUD
  controller), `ResourceEntity`, `MapField`, `FileField`, `Attribute/`, `Hook/`,
  `Hooks/`.
- `Repository/`, `Security/`, `Service/`, `Validator/`, `EventListener/`,
  `Command/`, `Kernel.php`.
- `config/` — `bundles.php`, `routes.yaml`, `services.yaml`, `packages/`
  (`doctrine.yaml`, `doctrine_migrations.yaml`, `framework.yaml`,
  `lexik_jwt_authentication.yaml`, `nelmio_cors.yaml`, `security.yaml`,
  `validator.yaml`, ...).

## How the two connect

A generated resource appears in **both** halves: a Next.js `resources/<name>.ts`
(frontend definition) and a Symfony `Entity/<Class>.php` (extends
`ResourceEntity`), plus a controller extending `ResourceController`. The
`sync_schema` tool generates these; see
[`resource-system.md`](./resource-system.md).
