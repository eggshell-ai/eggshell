<?php

namespace App\Command;

use App\Service\RoleManagementService;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Throwable;

#[AsCommand(
    name: 'app:role:manage',
    description: 'Manage roles and permissions (create, grant, revoke, sync, delete, list)'
)]
class RoleManagementCommand extends Command
{
    public function __construct(
        private readonly RoleManagementService $roleService
    ) {
        parent::__construct();
    }

    protected function configure(): void
    {
        $this
            ->addArgument('action', InputArgument::REQUIRED, 'Action to perform: create, grant, revoke, sync, delete, or list')
            ->addArgument('role', InputArgument::OPTIONAL, 'Role name (e.g. ROLE_MANAGER or MANAGER)')
            ->addArgument('permissions', InputArgument::IS_ARRAY | InputArgument::OPTIONAL, 'Permission names (e.g. products.view products.edit)')
            ->addOption('json', null, InputOption::VALUE_NONE, 'Output as JSON');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        $action = strtolower((string) $input->getArgument('action'));
        $roleArg = (string) $input->getArgument('role');
        /** @var string[] $permissions */
        $permissions = $input->getArgument('permissions') ?: [];
        $isJson = (bool) $input->getOption('json');

        try {
            switch ($action) {
                case 'list':
                    $roles = $this->roleService->listRoles();
                    if ($isJson) {
                        $output->writeln(json_encode($roles, JSON_PRETTY_PRINT));
                    } else {
                        $io->title('System Roles & Permissions');
                        $rows = [];
                        foreach ($roles as $r) {
                            $permStr = empty($r['permissions']) ? '(none)' : implode(', ', $r['permissions']);
                            $rows[] = [$r['id'], $r['name'], $r['user_count'], $permStr];
                        }
                        $io->table(['ID', 'Name', 'Users', 'Permissions'], $rows);
                    }
                    return Command::SUCCESS;

                case 'create':
                    if (empty($roleArg)) {
                        $io->error('Role name is required for create action.');
                        return Command::FAILURE;
                    }
                    $role = $this->roleService->createRole($roleArg, $permissions);
                    $msg = sprintf('Role "%s" created successfully with %d permission(s).', $role->name, count($permissions));
                    $isJson ? $output->writeln(json_encode(['success' => true, 'role' => $role->name, 'message' => $msg])) : $io->success($msg);
                    return Command::SUCCESS;

                case 'grant':
                case 'add':
                    if (empty($roleArg) || empty($permissions)) {
                        $io->error('Both role and at least one permission are required for grant action.');
                        return Command::FAILURE;
                    }
                    $role = $this->roleService->addPermissionsToRole($roleArg, $permissions);
                    $msg = sprintf('Granted permission(s) [%s] to role "%s".', implode(', ', $permissions), $role->name);
                    $isJson ? $output->writeln(json_encode(['success' => true, 'role' => $role->name, 'message' => $msg])) : $io->success($msg);
                    return Command::SUCCESS;

                case 'revoke':
                case 'remove':
                    if (empty($roleArg) || empty($permissions)) {
                        $io->error('Both role and at least one permission are required for revoke action.');
                        return Command::FAILURE;
                    }
                    $role = $this->roleService->removePermissionsFromRole($roleArg, $permissions);
                    $msg = sprintf('Revoked permission(s) [%s] from role "%s".', implode(', ', $permissions), $role->name);
                    $isJson ? $output->writeln(json_encode(['success' => true, 'role' => $role->name, 'message' => $msg])) : $io->success($msg);
                    return Command::SUCCESS;

                case 'sync':
                    if (empty($roleArg)) {
                        $io->error('Role name is required for sync action.');
                        return Command::FAILURE;
                    }
                    $role = $this->roleService->syncRolePermissions($roleArg, $permissions);
                    $msg = sprintf('Synchronized permissions for role "%s" (%d permissions active).', $role->name, count($permissions));
                    $isJson ? $output->writeln(json_encode(['success' => true, 'role' => $role->name, 'message' => $msg])) : $io->success($msg);
                    return Command::SUCCESS;

                case 'delete':
                    if (empty($roleArg)) {
                        $io->error('Role name is required for delete action.');
                        return Command::FAILURE;
                    }
                    $this->roleService->deleteRole($roleArg);
                    $msg = sprintf('Role "%s" was successfully deleted.', $roleArg);
                    $isJson ? $output->writeln(json_encode(['success' => true, 'message' => $msg])) : $io->success($msg);
                    return Command::SUCCESS;

                default:
                    $io->error(sprintf('Unknown action "%s". Valid actions: list, create, grant, revoke, sync, delete.', $action));
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
}
