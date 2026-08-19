<?php

namespace App\Entity;

use App\Repository\RoleRepository;
use App\Resource\ResourceEntity;
use App\Resource\Attribute\Form;
use App\Resource\Attribute\Table;
use Doctrine\Common\Collections\ArrayCollection;
use Doctrine\Common\Collections\Collection;
use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Serializer\Attribute\Groups;
use Symfony\Component\Serializer\Attribute\Ignore;
use Symfony\Component\Serializer\Attribute\SerializedName;
use Symfony\Component\Validator\Constraints as Assert;
use Symfony\Bridge\Doctrine\Validator\Constraints\UniqueEntity;

#[ORM\Entity(repositoryClass: RoleRepository::class)]
#[ORM\Table(name: '`role`')]
#[UniqueEntity(fields: ['name'], message: 'Role with this name already exists')]
class Role extends ResourceEntity
{
    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    #[Groups(['role:read'])]
    #[Table(label: "ID", sortable: true)]
    public ?int $id = null;

    #[ORM\Column(length: 255, unique: true)]
    #[Groups(['role:read'])]
    #[Table(label: "Role Name", sortable: true, searchable: true)]
    #[Form(label: "Role Name", type: "text", required: true)]
    #[Assert\NotBlank(message: 'Name is required')]
    public ?string $name = null;

    #[ORM\ManyToMany(targetEntity: Permission::class, inversedBy: 'roles')]
    #[Ignore]
    #[Table(label: "Permissions", sortable: false)]
    #[Form(label: "Permissions", type: "tags", required: false, options: ["source" => "/permissions"])]
    public Collection $permissions;

    #[ORM\OneToMany(targetEntity: User::class, mappedBy: 'role')]
    #[Ignore]
    public Collection $users;

    public function __construct()
    {
        $this->permissions = new ArrayCollection();
        $this->users = new ArrayCollection();
    }

    public function getTitle(): string
    {
        return $this->name;
    }

    /**
     * @return int[]
     */
    #[Groups(['role:read'])]
    #[SerializedName('permissions')]
    public function getPermissionIds(): array
    {
        return $this->permissions->map(fn(Permission $p) => $p->id)->toArray();
    }
}
