<?php

namespace App\Entity;

use App\Resource\ResourceEntity;
use App\Resource\Attribute\Form;
use App\Resource\Attribute\Table;
use App\Resource\FileField;
use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Serializer\Attribute\Groups;

#[ORM\Entity]
#[ORM\Table(name: '`profile`')]
class Profile extends ResourceEntity
{
    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    #[Groups(['profile:read', 'user:read'])]
    #[Table(label: "ID", sortable: true)]
    public ?int $id = null;

    #[ORM\OneToOne(targetEntity: User::class, inversedBy: 'profile')]
    #[ORM\JoinColumn(nullable: false, onDelete: 'CASCADE')]
    #[Groups(['profile:read'])]
    #[Table(label: "User", sortable: true)]
    #[Form(label: "User", type: "select", required: true, options: ["source" => "/users"])]
    public ?User $user = null;

    #[ORM\Column(length: 255, nullable: true)]
    #[Groups(['profile:read', 'user:read'])]
    #[Table(label: "Avatar", sortable: false)]
    #[Form(label: "Avatar", type: "file", required: false)]
    #[FileField(uploadDirectory: 'uploads/avatars')]
    public ?string $avatar = null;

    public function getTitle(): string
    {
        return $this->user ? $this->user->getTitle() : (string) $this->id;
    }
}
