<?php

namespace App\Command;

use App\Entity\Role;
use App\Entity\User;
use App\Repository\RoleRepository;
use App\Repository\UserRepository;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Symfony\Component\PasswordHasher\Hasher\UserPasswordHasherInterface;
use Throwable;

#[AsCommand(
    name: 'app:user:manage',
    description: 'Manage project users (create, drop, add, list)'
)]
class UserManagementCommand extends Command
{
    /** The bootstrap admin account, which must never be dropped. */
    public const ADMIN_EMAIL = 'admin@dummy-project.com';

    /** Alphabet used for generated passwords (no ambiguous characters). */
    private const PASSWORD_ALPHABET = 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnpqrstuvwxyz23456789';

    public function __construct(
        private readonly EntityManagerInterface $entityManager,
        private readonly UserRepository $userRepository,
        private readonly RoleRepository $roleRepository,
        private readonly UserPasswordHasherInterface $passwordHasher
    ) {
        parent::__construct();
    }

    protected function configure(): void
    {
        $this
            ->addArgument('action', InputArgument::REQUIRED, 'Action to perform: create, drop, add, or list')
            ->addArgument('email', InputArgument::OPTIONAL, 'User email address')
            ->addArgument('password', InputArgument::OPTIONAL, 'User password (generated when omitted for create)')
            ->addArgument('role', InputArgument::OPTIONAL, 'Role name (e.g. ROLE_MANAGER)', 'ROLE_USER')
            ->addOption('json', null, InputOption::VALUE_NONE, 'Output as JSON');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        $action = strtolower((string) $input->getArgument('action'));
        $email = trim((string) $input->getArgument('email'));
        $password = (string) $input->getArgument('password');
        $roleName = trim((string) $input->getArgument('role')) ?: 'ROLE_USER';
        $isJson = (bool) $input->getOption('json');

        try {
            switch ($action) {
                case 'list':
                    $users = $this->userRepository->findBy([], ['email' => 'ASC']);
                    $rows = array_map(static fn (User $user): array => [
                        'id' => $user->id,
                        'email' => $user->email,
                        'role' => $user->getRole(),
                    ], $users);

                    if ($isJson) {
                        $output->writeln(json_encode(['success' => true, 'users' => $rows]));
                    } else {
                        $io->title('Project Users');
                        $io->table(
                            ['ID', 'Email', 'Role'],
                            array_map(static fn (array $r): array => [$r['id'], $r['email'], $r['role']], $rows)
                        );
                    }
                    return Command::SUCCESS;

                case 'create':
                    $this->requireEmail($email);
                    $existing = $this->userRepository->findOneBy(['email' => $email]);
                    if ($existing) {
                        throw new \RuntimeException(sprintf('User "%s" already exists.', $email));
                    }

                    $generated = false;
                    if ($password === '') {
                        $password = $this->generatePassword();
                        $generated = true;
                    }

                    $role = $this->findOrCreateRole($roleName);

                    $user = new User();
                    $user->email = $email;
                    $user->first_name = 'Project';
                    $user->last_name = 'User';
                    $user->roles->add($role);
                    $user->password = $this->passwordHasher->hashPassword($user, $password);

                    $this->entityManager->persist($user);
                    $this->entityManager->flush();

                    if ($isJson) {
                        $output->writeln(json_encode([
                            'success' => true,
                            'email' => $email,
                            'role' => $role->name,
                            'password' => $password,
                            'generated' => $generated,
                        ]));
                    } else {
                        $io->success(sprintf('User "%s" created with role "%s".', $email, $role->name));
                        $io->writeln(sprintf('<comment>Password:</comment> %s', $password));
                    }
                    return Command::SUCCESS;

                case 'drop':
                case 'delete':
                    $this->requireEmail($email);
                    if (strcasecmp($email, self::ADMIN_EMAIL) === 0) {
                        throw new \RuntimeException('The admin account cannot be dropped.');
                    }

                    $user = $this->userRepository->findOneBy(['email' => $email]);
                    if (!$user) {
                        throw new \RuntimeException(sprintf('User "%s" not found.', $email));
                    }

                    $this->entityManager->remove($user);
                    $this->entityManager->flush();

                    $msg = sprintf('User "%s" was dropped.', $email);
                    $isJson ? $output->writeln(json_encode(['success' => true, 'email' => $email])) : $io->success($msg);
                    return Command::SUCCESS;

                case 'add':
                case 'grant':
                    $this->requireEmail($email);
                    $user = $this->userRepository->findOneBy(['email' => $email]);
                    if (!$user) {
                        throw new \RuntimeException(sprintf('User "%s" not found.', $email));
                    }

                    $role = $this->findOrCreateRole($roleName);
                    if ($user->roles->contains($role)) {
                        throw new \RuntimeException(sprintf('User "%s" already has role "%s".', $email, $role->name));
                    }

                    $user->roles->add($role);
                    $this->entityManager->flush();

                    if ($isJson) {
                        $output->writeln(json_encode(['success' => true, 'email' => $email, 'role' => $role->name]));
                    } else {
                        $io->success(sprintf('Role "%s" added to user "%s".', $role->name, $email));
                    }
                    return Command::SUCCESS;

                default:
                    $io->error(sprintf('Unknown action "%s". Valid actions: list, create, drop, add.', $action));
                    return Command::INVALID;
            }
        } catch (Throwable $e) {
            if ($isJson) {
                $output->writeln(json_encode(['success' => false, 'error' => $e->getMessage()]));
            } else {
                $io->error($e->getMessage());
            }
            return Command::FAILURE;
        }
    }

    private function requireEmail(string $email): void
    {
        if ($email === '') {
            throw new \InvalidArgumentException('An email address is required for this action.');
        }
    }

    private function findOrCreateRole(string $roleName): Role
    {
        $roleName = strtoupper($roleName);
        if (!str_starts_with($roleName, 'ROLE_')) {
            $roleName = 'ROLE_' . $roleName;
        }

        $role = $this->roleRepository->findOneBy(['name' => $roleName]);
        if (!$role) {
            $role = new Role();
            $role->name = $roleName;
            $this->entityManager->persist($role);
            $this->entityManager->flush();
        }

        return $role;
    }

    private function generatePassword(int $length = 12): string
    {
        $alphabet = self::PASSWORD_ALPHABET;
        $max = strlen($alphabet) - 1;
        $password = '';
        for ($i = 0; $i < $length; $i++) {
            $password .= $alphabet[random_int(0, $max)];
        }
        return $password;
    }
}
