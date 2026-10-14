# Roles & Permissions Management Skill

This skill teaches the agent how to manage roles and permissions across the application stack. You have access to create new roles, grant or revoke permissions, remove roles, and protect administrative access.

---

## 1. Architectural Overview & Mental Model

The security and access control system consists of:

1. **Entities**:
   - `App\Entity\Role` (`templates/backend/src/Entity/Role.php`): Stores role identifiers (e.g. `ROLE_ADMIN`, `ROLE_MANAGER`, `ROLE_EDITOR`). Contains a ManyToMany relation to `Permission` and `User`.
   - `App\Entity\Permission` (`templates/backend/src/Entity/Permission.php`): Represents actionable capabilities as dot-notated strings (e.g., `products.view`, `products.create`, `products.edit`, `products.delete`, `users.index`, `roles.view`).
   - `App\Entity\User` (`templates/backend/src/Entity/User.php`): Has ManyToMany `$roles`. The primary role is exposed via `$user->getRole()`.

2. **Authorization & Voter**:
   - `App\Security\PermissionVoter`:
     - Checks permissions on route access (e.g. `#[IsGranted('products.edit')]` or `$this->denyAccessUnlessGranted('leads.edit')`).
     - **`ROLE_ADMIN` has all permissions automatically.** Any user possessing `ROLE_ADMIN` bypasses granular permission checks.
     - For other roles, `PermissionVoter` checks if any of the user's assigned roles contains the required permission.

3. **Automatic Permission Discovery**:
   - `App\Service\PermissionRegistry`: Scans controllers extending `ResourceController` and controller methods with `#[IsGranted('...')]`.
   - Every resource declared in schema (e.g. `products`) generates four standard permissions:
     - `<resource>.view`
     - `<resource>.create`
     - `<resource>.edit`
     - `<resource>.delete`
   - Custom controller routes (e.g. `#[Route('/api/orders/{id}/confirm')]`) can check custom permissions via `$this->denyAccessUnlessGranted('orders.edit')`.

4. **Programmatic & CLI Management**:
   - `App\Service\RoleManagementService` (`src/Service/RoleManagementService.php`): Facade for creating, listing, modifying, and deleting roles in code.
   - `app:role:manage` Symfony console command: Command-line interface for role operations.

5. **Frontend UI Integration**:
   - `roles.ts` (`src/resources/roles.ts`): Resource definition with a `tags` field for selecting permissions from `/api/permissions`.
   - `roles/page.jsx`: Resource page rendering the table, create/edit modals, and permission tag selectors.
   - `menu-items/menu.json`: Navigation entries can include `"permission": "<permission.name>"` to hide menu links from users who lack the required permission.

---

## 2. Invariants & Safety Guardrails

> [!IMPORTANT]
> 1. **Never delete `ROLE_ADMIN`**: The administrative user created during project initialization relies on `ROLE_ADMIN`. Deleting it will lock administrators out of the application.
> 2. **Role Naming Standard**: Role names must follow the `ROLE_<NAME>` convention (uppercase, alphanumeric with underscores, e.g. `ROLE_MANAGER`, `ROLE_STAFF`, `ROLE_SALES_REP`). If provided without prefix (e.g. `MANAGER`), the service automatically prepends `ROLE_`.
> 3. **Permission String Convention**: Standard CRUD permissions are always `<plural_resource_name>.<action>`, e.g.:
>    - `products.view`, `products.create`, `products.edit`, `products.delete`
>    - `orders.view`, `orders.create`, `orders.edit`, `orders.delete`
> 4. **Do Not Overwrite Unrelated Roles**: When adding permissions, always append or grant specifically rather than wiping existing permissions unless an explicit sync/reset is requested.

---

## 3. How to Manage Roles and Permissions

When a user asks you to configure roles, you can execute logic via **`RoleManagementService`** in backend scripts, migrations, fixtures, or via **`app:role:manage`** in console commands.

### 3.1 Using the Console Command (`app:role:manage`)

The `app:role:manage` command supports the following operations:

#### 1. Create a New Role
```bash
php bin/console app:role:manage create ROLE_MANAGER products.view products.edit products.create
```
*(If permissions do not exist in the database yet, they are automatically registered.)*

#### 2. Grant Permissions to an Existing Role
```bash
php bin/console app:role:manage grant ROLE_MANAGER orders.view orders.edit
```

#### 3. Revoke Permissions from a Role
```bash
php bin/console app:role:manage revoke ROLE_MANAGER products.delete
```

#### 4. Replace All Permissions (Sync)
```bash
php bin/console app:role:manage sync ROLE_MANAGER products.view orders.view
```

#### 5. Delete an Existing Role
```bash
php bin/console app:role:manage delete ROLE_GUEST
```
*(Protected roles like `ROLE_ADMIN` cannot be deleted.)*

#### 6. List All Roles and Permissions
```bash
php bin/console app:role:manage list
# Or with JSON output:
php bin/console app:role:manage list --json
```

---

### 3.2 Using `RoleManagementService` in PHP Code

Inject `App\Service\RoleManagementService` into any command, controller, or migration:

```php
namespace App\Command;

use App\Service\RoleManagementService;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

#[AsCommand(name: 'app:seed-roles', description: 'Seeds application roles')]
class SeedRolesCommand extends Command
{
    public function __construct(
        private readonly RoleManagementService $roleService
    ) {
        parent::__construct();
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        // 1. Create Manager role with restricted permissions
        $this->roleService->createRole('ROLE_MANAGER', [
            'products.view',
            'products.create',
            'products.edit',
            'orders.view',
            'orders.edit',
        ]);

        // 2. Create Read-Only Auditor role
        $this->roleService->createRole('ROLE_AUDITOR', [
            'products.view',
            'orders.view',
            'users.view',
        ]);

        $output->writeln('Roles created successfully.');
        return Command::SUCCESS;
    }
}
```

---

## 4. Frontend Menu Visibility by Permission

When creating new pages or configuring sidebar navigation in `menu-items/menu.json`, add the `"permission"` field to restrict the item to roles possessing that permission:

```json
[
  {
    "name": "Home.Dashboard",
    "route": "/",
    "icon": "DashboardOutlined"
  },
  {
    "name": "Inventory.Products",
    "route": "/products",
    "icon": "AppstoreOutlined",
    "permission": "products.view"
  },
  {
    "name": "Sales.Orders",
    "route": "/orders",
    "icon": "ShoppingOutlined",
    "permission": "orders.view"
  },
  {
    "name": "Management.Users",
    "route": "/users",
    "icon": "UserOutlined",
    "permission": "users.view"
  },
  {
    "name": "Management.Roles",
    "route": "/roles",
    "icon": "TeamOutlined",
    "permission": "roles.view"
  }
]
```

Any user logged into the frontend will only see menu items where their assigned roles include the specified permission (while `ROLE_ADMIN` sees all items).

---

## 5. Requirement-to-Action Checklist

When handling user prompts about roles and permissions:

| Prompt Requirement | Action |
|---|---|
| "Create a role named X that can only view Y" | Create `ROLE_X` with `y.view` using `RoleManagementService` or `app:role:manage create`. |
| "Allow X role to edit Y" | Run `app:role:manage grant ROLE_X y.edit` or call `$roleService->addPermissionsToRole()`. |
| "Prevent role X from deleting Y" | Run `app:role:manage revoke ROLE_X y.delete` or call `$roleService->removePermissionsFromRole()`. |
| "Delete the old X role" | Run `app:role:manage delete ROLE_X` or call `$roleService->deleteRole()`. |
| "Hide menu item from non-managers" | Add `"permission": "<permission>"` to `menu-items/menu.json`. |
| "Ensure all controller permissions are in DB" | Run `php bin/console app:permissions:sync`. |
