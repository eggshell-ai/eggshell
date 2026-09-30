<?php

namespace App\Entity;

use App\Repository\NotificationUserRepository;
use DateTimeImmutable;
use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Serializer\Attribute\Groups;

#[ORM\Entity(repositoryClass: NotificationUserRepository::class)]
#[ORM\Table(name: '`notification_user`')]
#[ORM\UniqueConstraint(name: 'UNIQ_NOTIFICATION_USER', fields: ['notification', 'user'])]
class NotificationUser
{
    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    #[Groups(['notification:read'])]
    public ?int $id = null;

    #[ORM\ManyToOne(targetEntity: Notification::class, inversedBy: 'notification_users')]
    #[ORM\JoinColumn(nullable: false, onDelete: 'CASCADE')]
    public Notification $notification;

    #[ORM\ManyToOne(targetEntity: User::class)]
    #[ORM\JoinColumn(nullable: false, onDelete: 'CASCADE')]
    public User $user;

    #[ORM\Column(options: ['default' => false])]
    #[Groups(['notification:read'])]
    public bool $is_read = false;

    #[ORM\Column(nullable: true)]
    #[Groups(['notification:read'])]
    public ?DateTimeImmutable $read_at = null;

    public function __construct(Notification $notification, User $user)
    {
        $this->notification = $notification;
        $this->user = $user;
        $this->is_read = false;
    }
}
