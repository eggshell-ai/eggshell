<?php

namespace App\Controller\Auth;

use App\Entity\Permission;
use App\Entity\Role;
use App\Entity\User;
use App\Repository\PermissionRepository;
use App\Repository\RoleRepository;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Security\Http\Attribute\IsGranted;

#[Route('/api/permissions')]
final class PermissionController extends AbstractController
{
    public function __construct(
        private readonly PermissionRepository $permissionRepository,
        private readonly RoleRepository $roleRepository,
        private readonly EntityManagerInterface $entityManager
    ) {
    }

    #[Route('/', name: 'permissions.list', methods: ['GET'])]
    #[IsGranted('permissions.view')]
    public function index(): JsonResponse
    {
        $permissions = $this->permissionRepository->findAll();

        $data = array_map(function (Permission $permission) {
            return [
                'id' => $permission->getId(),
                'name' => $permission->getName(),
            ];
        }, $permissions);

        return $this->json($data);
    }

    #[Route('/my', name: 'permissions.my', methods: ['GET'])]
    #[IsGranted('IS_AUTHENTICATED')]
    public function getMyPermissions(): JsonResponse
    {
        /** @var User $user */
        $user = $this->getUser();
        $roles = $user->roles;

        $permissions = [];
        $roleName = null;

        if ($roles->count() > 0) {
            $firstRole = $roles->first();
            $roleName = $firstRole->name;
            // Remove "ROLE_" prefix and convert to uppercase
            $roleName = strtoupper(str_replace('ROLE_', '', $roleName));

            foreach ($firstRole->permissions as $permission) {
                $permissions[] = $permission;
            }
        }

        return $this->json([
            'role' => $roleName,
            'permissions' => $permissions,
        ]);
    }

    #[Route('/assign', name: 'permissions.assign', methods: ['POST'])]
    #[IsGranted('permissions.assign')]
    public function assignPermissions(Request $request): JsonResponse
    {
        $data = json_decode($request->getContent(), true);

        if (!isset($data['role']) || !isset($data['permissions'])) {
            return $this->json(['message' => 'role and permissions are required'], Response::HTTP_BAD_REQUEST);
        }

        $role = $this->roleRepository->find($data['role']);

        if (!$role) {
            return $this->json(['message' => 'Role not found'], Response::HTTP_NOT_FOUND);
        }

        // Clear existing permissions
        $role->permissions->clear();

        // Add new permissions
        foreach ($data['permissions'] as $permissionId) {
            $permission = $this->permissionRepository->find($permissionId);
            if ($permission) {
                $role->permissions->add($permission);
            }
        }

        $this->entityManager->flush();

        return $this->json($role->permissions->toArray());
    }
}
