<?php

namespace App\Service;

use App\Entity\Permission;
use App\Entity\Role;
use App\Entity\User;
use App\Repository\PermissionRepository;
use App\Repository\RoleRepository;
use Doctrine\ORM\EntityManagerInterface;
use InvalidArgumentException;
use RuntimeException;

/**
 * Service for programmatic management of roles and permissions.
 * Provides safe operations to create roles, add/remove permissions,
 * and remove roles with built-in protection for system roles (e.g. ROLE_ADMIN).
 */
class RoleManagementService
{
    public const PROTECTED_ROLES = ['ROLE_ADMIN'];

    public function __construct(
        private readonly EntityManagerInterface $entityManager,
        private readonly RoleRepository $roleRepository,
        private readonly PermissionRepository $permissionRepository,
        private readonly PermissionRegistry $permissionRegistry
    ) {
    }

    /**
     * Normalize role name (ensures uppercase and optional ROLE_ prefix).
     */
    public function normalizeRoleName(string $name): string
    {
        $trimmed = trim($name);
        if ($trimmed === '') {
            throw new InvalidArgumentException('Role name cannot be empty.');
        }

        $upper = strtoupper($trimmed);
        if (!str_starts_with($upper, 'ROLE_')) {
            $upper = 'ROLE_' . $upper;
        }

        return $upper;
    }

    /**
     * Create a new role with optional initial permissions.
     *
     * @param string $name Role name (e.g. 'MANAGER' or 'ROLE_MANAGER')
     * @param string[] $permissionNames List of permission names (e.g. ['products.view', 'products.edit'])
     * @return Role
     */
    public function createRole(string $name, array $permissionNames = []): Role
    {
        $roleName = $this->normalizeRoleName($name);

        $existing = $this->roleRepository->findOneBy(['name' => $roleName]);
        if ($existing) {
            throw new RuntimeException(sprintf('Role "%s" already exists.', $roleName));
        }

        $role = new Role();
        $role->name = $roleName;
        $this->entityManager->persist($role);

        if (!empty($permissionNames)) {
            $this->assignPermissions($role, $permissionNames);
        }

        $this->entityManager->flush();

        return $role;
    }

    /**
     * Add permissions to an existing role idempotently.
     *
     * @param string $roleName
     * @param string[] $permissionNames
     * @return Role
     */
    public function addPermissionsToRole(string $roleName, array $permissionNames): Role
    {
        $role = $this->getRoleOrThrow($roleName);

        foreach ($permissionNames as $permName) {
            $permName = trim($permName);
            if ($permName === '') {
                continue;
            }

            $permission = $this->permissionRegistry->ensurePermissionExists($permName);
            if ($permission && !$role->permissions->contains($permission)) {
                $role->permissions->add($permission);
            }
        }

        $this->entityManager->flush();

        return $role;
    }

    /**
     * Remove specific permissions from a role.
     *
     * @param string $roleName
     * @param string[] $permissionNames
     * @return Role
     */
    public function removePermissionsFromRole(string $roleName, array $permissionNames): Role
    {
        $role = $this->getRoleOrThrow($roleName);

        if ($this->isProtectedRole($role->name)) {
            throw new RuntimeException(sprintf('Cannot modify permissions of protected system role "%s".', $role->name));
        }

        $permsToRemove = array_map('trim', $permissionNames);

        foreach ($role->permissions as $permission) {
            if (in_array($permission->name, $permsToRemove, true)) {
                $role->permissions->removeElement($permission);
            }
        }

        $this->entityManager->flush();

        return $role;
    }

    /**
     * Replace all permissions for a role with a specified set.
     *
     * @param string $roleName
     * @param string[] $permissionNames
     * @return Role
     */
    public function syncRolePermissions(string $roleName, array $permissionNames): Role
    {
        $role = $this->getRoleOrThrow($roleName);

        if ($this->isProtectedRole($role->name)) {
            throw new RuntimeException(sprintf('Cannot modify permissions of protected system role "%s".', $role->name));
        }

        $role->permissions->clear();
        $this->assignPermissions($role, $permissionNames);
        $this->entityManager->flush();

        return $role;
    }

    /**
     * Delete an existing role.
     * Safe guards protect ROLE_ADMIN from being deleted.
     * Users assigned to this role are detached from it.
     *
     * @param string $roleName
     * @return bool
     */
    public function deleteRole(string $roleName): bool
    {
        $role = $this->getRoleOrThrow($roleName);

        if ($this->isProtectedRole($role->name)) {
            throw new RuntimeException(sprintf('Cannot delete protected system role "%s".', $role->name));
        }

        // Detach role from users
        /** @var User $user */
        foreach ($role->users as $user) {
            $user->roles->removeElement($role);
        }

        $this->entityManager->remove($role);
        $this->entityManager->flush();

        return true;
    }

    /**
     * Get a role by name or return null.
     */
    public function findRole(string $name): ?Role
    {
        $roleName = $this->normalizeRoleName($name);
        return $this->roleRepository->findOneBy(['name' => $roleName]);
    }

    /**
     * Get a role by name or throw exception if not found.
     */
    public function getRoleOrThrow(string $name): Role
    {
        $role = $this->findRole($name);
        if (!$role) {
            throw new RuntimeException(sprintf('Role "%s" not found.', $this->normalizeRoleName($name)));
        }

        return $role;
    }

    /**
     * List all roles with their assigned permission names.
     *
     * @return array<int, array{id: int, name: string, permissions: string[], user_count: int}>
     */
    public function listRoles(): array
    {
        $roles = $this->roleRepository->findAll();
        $result = [];

        foreach ($roles as $role) {
            $perms = [];
            foreach ($role->permissions as $p) {
                $perms[] = $p->name;
            }
            sort($perms);

            $result[] = [
                'id' => $role->id,
                'name' => $role->name,
                'permissions' => $perms,
                'user_count' => $role->users->count(),
            ];
        }

        return $result;
    }

    /**
     * Check if a role is protected from deletion or permission stripping.
     */
    public function isProtectedRole(string $roleName): bool
    {
        return in_array($roleName, self::PROTECTED_ROLES, true);
    }

    /**
     * Helper to resolve and assign permission names to role.
     */
    private function assignPermissions(Role $role, array $permissionNames): void
    {
        foreach ($permissionNames as $permName) {
            $permName = trim($permName);
            if ($permName === '') {
                continue;
            }

            $permission = $this->permissionRegistry->ensurePermissionExists($permName);
            if ($permission && !$role->permissions->contains($permission)) {
                $role->permissions->add($permission);
            }
        }
    }
}
