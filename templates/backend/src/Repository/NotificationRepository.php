<?php

namespace App\Repository;

use App\Entity\Notification;
use App\Entity\User;
use Doctrine\Bundle\DoctrineBundle\Repository\ServiceEntityRepository;
use Doctrine\Persistence\ManagerRegistry;

/**
 * @extends ServiceEntityRepository<Notification>
 */
class NotificationRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, Notification::class);
    }

    /**
     * Find notifications that are visible to a given user:
     * - target_type = 'all'
     * - target_type = 'role' with target_role matching any of user's roles
     * - target_type = 'user' and target_user = user
     *
     * @return Notification[]
     */
    public function findForUser(User $user, int $limit = 20): array
    {
        $roles = $user->getRoles();

        $qb = $this->createQueryBuilder('n')
            ->leftJoin('n.notification_users', 'nu', 'WITH', 'nu.user = :user')
            ->where('n.target_type = :target_all')
            ->orWhere('n.target_user = :user');

        if (!empty($roles)) {
            $qb->orWhere('n.target_type = :target_role AND n.target_role IN (:roles)')
                ->setParameter('target_role', Notification::TARGET_ROLE)
                ->setParameter('roles', $roles);
        }

        $qb->setParameter('target_all', Notification::TARGET_ALL)
            ->setParameter('user', $user)
            ->orderBy('n.created_at', 'DESC')
            ->setMaxResults($limit);

        return $qb->getQuery()->getResult();
    }
}
