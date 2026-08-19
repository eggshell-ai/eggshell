<?php

namespace App\Controller\Auth;

use App\Entity\Role;
use App\Entity\Permission;
use App\Resource\ResourceController;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Serializer\SerializerInterface;
use Symfony\Component\Validator\Validator\ValidatorInterface;
use Symfony\Component\String\Slugger\SluggerInterface;

#[Route('/api/roles', name: 'roles.')]
final class RoleController extends ResourceController
{
    public function __construct(
        EntityManagerInterface $entityManager,
        SerializerInterface $serializer,
        ValidatorInterface $validator,
        SluggerInterface $slugger
    ) {
        parent::__construct($entityManager, $serializer, $validator, $slugger);
    }

    protected function getEntityClass(): string
    {
        return Role::class;
    }

    protected function getResourceName(): string
    {
        return 'roles';
    }

    protected function getSerializationGroups(string $action): array
    {
        return ['role:read'];
    }
}
