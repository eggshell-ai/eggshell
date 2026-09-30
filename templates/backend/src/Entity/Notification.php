<?php

namespace App\Entity;

use App\Repository\NotificationRepository;
use App\Resource\ResourceEntity;
use App\Resource\Attribute\Form;
use App\Resource\Attribute\Table;
use DateTimeImmutable;
use Doctrine\Common\Collections\ArrayCollection;
use Doctrine\Common\Collections\Collection;
use Doctrine\DBAL\Types\Types;
use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Serializer\Attribute\Groups;
use Symfony\Component\Validator\Constraints as Assert;

#[ORM\Entity(repositoryClass: NotificationRepository::class)]
#[ORM\Table(name: '`notification`')]
class Notification extends ResourceEntity
{
    public const TARGET_ALL = 'all';
    public const TARGET_ROLE = 'role';
    public const TARGET_USER = 'user';

    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    #[Groups(['notification:read'])]
    #[Table(label: 'ID', sortable: true)]
    public ?int $id = null;

    #[ORM\Column(length: 255)]
    #[Groups(['notification:read'])]
    #[Table(label: 'Title', sortable: true, searchable: true)]
    #[Form(label: 'Title', type: 'text', required: true)]
    #[Assert\NotBlank]
    public ?string $title = null;

    #[ORM\Column(type: Types::TEXT)]
    #[Groups(['notification:read'])]
    #[Table(label: 'Description', sortable: false, searchable: true)]
    #[Form(label: 'Description', type: 'textarea', required: true)]
    #[Assert\NotBlank]
    public ?string $description = null;

    #[ORM\Column(length: 500, nullable: true)]
    #[Groups(['notification:read'])]
    #[Table(label: 'URL', sortable: false)]
    #[Form(label: 'URL', type: 'text', required: false)]
    public ?string $url = null;

    #[ORM\Column(length: 20)]
    #[Groups(['notification:read'])]
    #[Table(label: 'Target Type', sortable: true)]
    #[Form(label: 'Target Type', type: 'select', required: true, options: [
        'choices' => [
            'All Users' => self::TARGET_ALL,
            'Role' => self::TARGET_ROLE,
            'User' => self::TARGET_USER,
        ]
    ])]
    #[Assert\Choice(choices: [self::TARGET_ALL, self::TARGET_ROLE, self::TARGET_USER])]
    public string $target_type = self::TARGET_ALL;

    #[ORM\Column(length: 100, nullable: true)]
    #[Groups(['notification:read'])]
    #[Table(label: 'Target Role', sortable: true)]
    #[Form(label: 'Target Role', type: 'text', required: false)]
    public ?string $target_role = null;

    #[ORM\ManyToOne(targetEntity: User::class)]
    #[ORM\JoinColumn(name: 'target_user_id', referencedColumnName: 'id', nullable: true, onDelete: 'CASCADE')]
    #[Groups(['notification:read'])]
    public ?User $target_user = null;

    #[ORM\Column]
    #[Groups(['notification:read'])]
    #[Table(label: 'Created At', sortable: true)]
    public DateTimeImmutable $created_at;

    /**
     * @var Collection<int, NotificationUser>
     */
    #[ORM\OneToMany(targetEntity: NotificationUser::class, mappedBy: 'notification', cascade: ['persist', 'remove'], orphanRemoval: true)]
    public Collection $notification_users;

    public function __construct()
    {
        $this->created_at = new DateTimeImmutable();
        $this->notification_users = new ArrayCollection();
    }

    public function getTitle(): string
    {
        return (string) $this->title;
    }
}
