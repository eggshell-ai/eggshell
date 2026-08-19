<?php

namespace App\Controller\Auth;

use App\Entity\User;
use App\Entity\Role;
use App\Resource\ResourceController;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\HttpKernel\Exception\BadRequestHttpException;
use Symfony\Component\PasswordHasher\Hasher\UserPasswordHasherInterface;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Serializer\SerializerInterface;
use Symfony\Component\Validator\Validator\ValidatorInterface;
use Symfony\Component\String\Slugger\SluggerInterface;

#[Route('/api/users', name: 'users.')]
final class UserController extends ResourceController
{
    public function __construct(
        EntityManagerInterface $entityManager,
        SerializerInterface $serializer,
        ValidatorInterface $validator,
        SluggerInterface $slugger,
        private readonly UserPasswordHasherInterface $passwordHasher
    ) {
        parent::__construct($entityManager, $serializer, $validator, $slugger);
    }

    protected function getEntityClass(): string
    {
        return User::class;
    }

    protected function getResourceName(): string
    {
        return 'users';
    }

    protected function getSerializationGroups(string $action): array
    {
        return ['user:read'];
    }

    protected function beforeSave(object $entity, array $data, string $action): void
    {
        /** @var User $entity */

        if (isset($data['password']) && !empty($data['password'])) {
            $hashedPassword = $this->passwordHasher->hashPassword($entity, $data['password']);
            $entity->password = $hashedPassword;
        }
    }
}
