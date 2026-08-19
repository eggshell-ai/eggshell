<?php

namespace App\Security;

use App\Entity\User;
use App\Service\PermissionRegistry;
use Symfony\Component\Security\Core\Authentication\Token\TokenInterface;
use Symfony\Component\Security\Core\Authorization\Voter\Voter;
use Symfony\Component\Security\Core\Authorization\Voter\Vote;

class PermissionVoter extends Voter
{
    public function __construct(
        private readonly PermissionRegistry $permissionRegistry
    ) {
    }

    protected function supports(string $attribute, mixed $subject): bool
    {
        // This voter only handles permission strings (e.g., "users.edit")
        return is_string($attribute) && str_contains($attribute, '.');
    }

    protected function voteOnAttribute(string $attribute, mixed $subject, TokenInterface $token, ?Vote $vote = null): bool
    {
        $user = $token->getUser();

        // If the user is not logged in, deny access
        if (!$user instanceof User) {
            return false;
        }

        // ROLE_ADMIN has all permissions automatically
        if ($user->getRole() === 'ROLE_ADMIN') {
            return true;
        }

        // Ensure permission exists in database (lazy creation if enabled)
        $this->permissionRegistry->ensurePermissionExists($attribute);

        // Check if the user has the specific permission through their roles
        foreach ($user->roles as $role) {
            foreach ($role->permissions as $permission) {
                if ($permission->name === $attribute) {
                    return true;
                }
            }
        }

        return false;
    }
}
