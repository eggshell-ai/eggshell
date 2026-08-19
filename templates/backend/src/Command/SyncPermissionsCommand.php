<?php

namespace App\Command;

use App\Service\PermissionRegistry;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;

#[AsCommand(
    name: 'app:permissions:sync',
    description: 'Sync permissions from code (IsGranted attributes) to database'
)]
class SyncPermissionsCommand extends Command
{
    public function __construct(
        private readonly PermissionRegistry $permissionRegistry
    ) {
        parent::__construct();
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);

        $io->title('Syncing Permissions from Code to Database');

        $discovered = $this->permissionRegistry->getDiscoveredPermissions();
        $io->text(sprintf('Found %d permissions in code', count($discovered)));

        if (count($discovered) > 0) {
            $io->listing($discovered);
        }

        $created = $this->permissionRegistry->syncPermissions();

        if ($created > 0) {
            $io->success(sprintf('Successfully created %d new permissions in database', $created));
        } else {
            $io->info('All permissions already exist in database. No changes made.');
        }

        return Command::SUCCESS;
    }
}
