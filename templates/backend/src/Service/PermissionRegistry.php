<?php

namespace App\Service;

use App\Entity\Permission;
use App\Repository\PermissionRepository;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\Finder\Finder;
use Symfony\Component\Routing\RouterInterface;

/**
 * Registry for managing permissions discovered from IsGranted attributes in controllers.
 * Uses caching to avoid performance impact during runtime.
 */
class PermissionRegistry
{
    private array $discoveredPermissions = [];
    private array $dbPermissions = [];
    private bool $scanned = false;
    private bool $dbLoaded = false;

    public function __construct(
        private readonly PermissionRepository $permissionRepository,
        private readonly EntityManagerInterface $entityManager,
        private readonly string $projectDir,
        private readonly bool $autoCreate = true
    ) {
    }

    /**
     * Get all permissions discovered from controllers.
     */
    public function getDiscoveredPermissions(): array
    {
        if (!$this->scanned) {
            $this->scanControllers();
        }

        return $this->discoveredPermissions;
    }

    /**
     * Ensure a permission exists in the database.
     * If autoCreate is enabled, creates it if it doesn't exist.
     */
    public function ensurePermissionExists(string $permissionName): ?Permission
    {
        // Load DB permissions cache if not already loaded
        if (!$this->dbLoaded) {
            $this->loadDbPermissions();
        }

        // Check if permission already exists in DB
        if (isset($this->dbPermissions[$permissionName])) {
            return $this->dbPermissions[$permissionName];
        }

        // Auto-create if enabled
        if ($this->autoCreate) {
            $permission = new Permission();
            $permission->name = $permissionName;
            $permission->description = sprintf('Auto-generated permission for: %s', $permissionName);
            
            $this->entityManager->persist($permission);
            $this->entityManager->flush();
            
            // Update cache
            $this->dbPermissions[$permissionName] = $permission;
            
            return $permission;
        }

        return null;
    }

    /**
     * Sync all discovered permissions to the database.
     * Returns the number of permissions created.
     */
    public function syncPermissions(): int
    {
        $discovered = $this->getDiscoveredPermissions();
        $created = 0;

        foreach ($discovered as $permissionName) {
            $existing = $this->permissionRepository->findByName($permissionName);
            
            if (!$existing) {
                $permission = new Permission();
                $permission->name = $permissionName;
                $permission->description = sprintf('Auto-generated permission for: %s', $permissionName);
                
                $this->entityManager->persist($permission);
                $created++;
            }
        }

        if ($created > 0) {
            $this->entityManager->flush();
        }

        // Reset cache
        $this->dbLoaded = false;
        $this->dbPermissions = [];

        return $created;
    }

    /**
     * Scan all controllers for IsGranted attributes.
     */
    private function scanControllers(): void
    {
        $this->discoveredPermissions = [];
        
        $finder = new Finder();
        $finder->files()
            ->in($this->projectDir . '/src/Controller')
            ->name('*.php');

        foreach ($finder as $file) {
            $content = $file->getContents();
            $this->extractPermissionsFromContent($content);
            $this->extractResourcePermissionsFromContent($content);
        }

        $this->scanned = true;
    }

    /**
     * Extract permission names from IsGranted attributes in PHP code.
     */
    private function extractPermissionsFromContent(string $content): void
    {
        // Match IsGranted attributes with permission strings (containing dots)
        // Pattern: #[IsGranted('permission.name')] or #[IsGranted("permission.name")]
        $pattern = '/#\[IsGranted\([\'"]([^\'"]+\.[^\'"]+)[\'"]\)\]/';
        
        if (preg_match_all($pattern, $content, $matches)) {
            foreach ($matches[1] as $permission) {
                if (!in_array($permission, $this->discoveredPermissions, true)) {
                    $this->discoveredPermissions[] = $permission;
                }
            }
        }
    }

    /**
     * Extract permissions from controllers extending ResourceController.
     */
    private function extractResourcePermissionsFromContent(string $content): void
    {
        if (str_contains($content, 'extends ResourceController')) {
            // Find return value of getResourceName()
            // e.g. function getResourceName(): string { return 'users'; }
            if (preg_match('/function\s+getResourceName\s*\(\s*\)\s*(:\s*string)?\s*\{\s*return\s+[\'"]([^\'"]+)[\'"]\s*;?\s*\}/i', $content, $m)) {
                $resourceName = $m[2];
                // Add standard CRUD permissions for this resource
                foreach (['view', 'create', 'edit', 'delete'] as $action) {
                    $perm = $resourceName . '.' . $action;
                    if (!in_array($perm, $this->discoveredPermissions, true)) {
                        $this->discoveredPermissions[] = $perm;
                    }
                }
            }
        }
    }

    /**
     * Load all permissions from database into cache.
     */
    private function loadDbPermissions(): void
    {
        $this->dbPermissions = [];
        $permissions = $this->permissionRepository->findAll();
        
        foreach ($permissions as $permission) {
            $this->dbPermissions[$permission->name] = $permission;
        }
        
        $this->dbLoaded = true;
    }

    /**
     * Clear all caches.
     */
    public function clearCache(): void
    {
        $this->scanned = false;
        $this->dbLoaded = false;
        $this->discoveredPermissions = [];
        $this->dbPermissions = [];
    }
}
