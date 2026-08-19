<?php

namespace App\Command;

use App\Entity\Role;
use App\Entity\User;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Symfony\Component\PasswordHasher\Hasher\UserPasswordHasherInterface;

class CreateUserCommand extends Command
{
    public function __construct(
        private EntityManagerInterface $entityManager,
        private UserPasswordHasherInterface $passwordHasher
    ) {
        parent::__construct();
    }

    protected function configure(): void
    {
        $this
            ->setName('app:create-user')
            ->setDescription('Creates a new user.')
            ->addArgument('email', InputArgument::REQUIRED, 'User email')
            ->addArgument('password', InputArgument::REQUIRED, 'User password')
            ->addArgument('role', InputArgument::OPTIONAL, 'User role (default: ROLE_USER)', 'ROLE_USER');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        
        $email = $input->getArgument('email');
        $password = $input->getArgument('password');
        $roleName = $input->getArgument('role');

        // Find or create the role
        $role = $this->entityManager->getRepository(Role::class)->findOneBy(['name' => $roleName]);
        if (!$role) {
            $role = new Role();
            $role->name = $roleName;
            $this->entityManager->persist($role);
        }

        $user = new User();
        $user->email = $email;
        $user->roles->add($role);
        $user->first_name = 'Admin';
        $user->last_name = 'User';
        
        $hashedPassword = $this->passwordHasher->hashPassword($user, $password);
        $user->password = $hashedPassword;

        $this->entityManager->persist($user);
        $this->entityManager->flush();

        $io->success(sprintf('User "%s" created successfully with role "%s"!', $email, $roleName));

        return Command::SUCCESS;
    }
}
