<?php

namespace App\Repository;

use App\Entity\Permission;
use Doctrine\Bundle\DoctrineBundle\Repository\ServiceEntityRepository;
use Doctrine\Persistence\ManagerRegistry;

/**
 * @extends ServiceEntityRepository<Permission>
 */
class PermissionRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Permission::class);
    }

    public function save(Permission $permission, bool $flush = false): void
    {
        $this->getEntityManager()->persist($permission);

        if ($flush) {
            $this->getEntityManager()->flush();
        }
    }

    public function remove(Permission $permission, bool $flush = false): void
    {
        $this->getEntityManager()->remove($permission);

        if ($flush) {
            $this->getEntityManager()->flush();
        }
    }

    public function findByName(string $name): ?Permission
    {
        return $this->findOneBy(['name' => $name]);
    }
}
