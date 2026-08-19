<?php

namespace App\Entity;

use App\Repository\UserRepository;
use App\Resource\ResourceEntity;
use App\Resource\Attribute\Form;
use App\Resource\Attribute\Table;
use Doctrine\Common\Collections\ArrayCollection;
use Doctrine\Common\Collections\Collection;
use Doctrine\ORM\Mapping as ORM;
use Symfony\Component\Security\Core\User\PasswordAuthenticatedUserInterface;
use Symfony\Component\Security\Core\User\UserInterface;
use Symfony\Component\Serializer\Attribute\Groups;
use Symfony\Component\Serializer\Attribute\Ignore;
use Symfony\Component\Validator\Constraints as Assert;
use Symfony\Bridge\Doctrine\Validator\Constraints\UniqueEntity;

#[ORM\Entity(repositoryClass: UserRepository::class)]
#[ORM\Table(name: '`user`')]
#[ORM\UniqueConstraint(name: 'UNIQ_IDENTIFIER_EMAIL', fields: ['email'])]
#[UniqueEntity(fields: ['email'], message: 'User with this email already exists')]
class User extends ResourceEntity implements UserInterface, PasswordAuthenticatedUserInterface
{
    #[ORM\Id]
    #[ORM\GeneratedValue]
    #[ORM\Column]
    #[Groups(['user:read'])]
    #[Table(label: "ID", sortable: true)]
    public ?int $id = null;

    #[ORM\Column(length: 180)]
    #[Groups(['user:read'])]
    #[Table(label: "Email", sortable: true, searchable: true)]
    #[Form(label: "Email", type: "email", required: true)]
    #[Assert\NotBlank(groups: ['store'])]
    #[Assert\Email]
    public ?string $email = null;

    #[ORM\Column(length: 100)]
    #[Groups(['user:read'])]
    #[Table(label: "First Name", sortable: true, searchable: true)]
    #[Form(label: "First Name", type: "text", required: true)]
    #[Assert\NotBlank]
    public ?string $first_name = null;

    #[ORM\Column(length: 100)]
    #[Groups(['user:read'])]
    #[Table(label: "Last Name", sortable: true, searchable: true)]
    #[Form(label: "Last Name", type: "text", required: true)]
    #[Assert\NotBlank]
    public ?string $last_name = null;

    /**
     * @var Collection<int, Role>
     */
    #[ORM\ManyToMany(targetEntity: Role::class, inversedBy: 'users')]
    #[Ignore]
    #[Table(label: "Role", sortable: true)]
    #[Form(label: "Role", type: "select", required: true, options: ["source" => "/roles"])]
    #[Assert\Count(min: 1, minMessage: 'Role is required')]
    public Collection $roles;

    /**
     * @var string The hashed password
     */
    #[ORM\Column]
    #[Form(label: "Password", type: "password", required: false)]
    #[Assert\NotBlank(groups: ['store'])]
    public ?string $password = null;

    public function __construct()
    {
        $this->roles = new ArrayCollection();
    }

    /**
     * @see UserInterface
     */
    public function getRoles(): array
    {
        $roles = $this->roles->map(fn(Role $role) => $role->name)->toArray();
        // guarantee every user at least has ROLE_USER
        $roles[] = 'ROLE_USER';

        return array_unique($roles);
    }

    /**
     * Get the primary role as a string
     */
    #[Groups(['user:read'])]
    public function getRole(): string
    {
        if ($this->roles->isEmpty()) {
            return 'ROLE_USER';
        }

        return $this->roles->first()->name;
    }

    /**
     * @see PasswordAuthenticatedUserInterface
     */
    public function getPassword(): ?string
    {
        return $this->password;
    }

    /**
     * A visual identifier that represents this user.
     *
     * @see UserInterface
     */
    public function getUserIdentifier(): string
    {
        return (string) $this->email;
    }

    /**
     * Ensure the session doesn't contain actual password hashes by CRC32C-hashing them, as supported since Symfony 7.3.
     */
    public function __serialize(): array
    {
        $data = (array) $this;
        $data['password'] = hash('crc32c', $this->password);

        return $data;
    }

    public function getTitle(): string
    {
        return $this->first_name . ' ' . $this->last_name;
    }
}

