<?php

namespace App\Repository;

use App\Entity\Notification;
use App\Entity\NotificationUser;
use App\Entity\User;
use Doctrine\Bundle\DoctrineBundle\Repository\ServiceEntityRepository;
use Doctrine\Persistence\ManagerRegistry;

/**
 * @extends ServiceEntityRepository<NotificationUser>
 */
class NotificationUserRepository extends ServiceEntityRepository
{
    public function __construct(ManagerRegistry $registry)
    {
        parent::__construct($registry, NotificationUser::class);
    }

    public function findOneByNotificationAndUser(Notification $notification, User $user): ?NotificationUser
    {
        return $this->findOneBy([
            'notification' => $notification,
            'user' => $user,
        ]);
    }
}
