<?php

namespace App\Command;

use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputArgument;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\Console\Style\SymfonyStyle;
use Symfony\Component\Process\Process;

class InitCommand extends Command
{
    private const DB_HOST = '127.0.0.1';
    private const DB_PORT = '3306';
    private const DB_ROOT_USER = 'root';
    private const DB_PASSWORD = 'dummy_project_password_123';
    private const ADMIN_EMAIL = 'admin@dummy-project.com';
    private const ADMIN_PASSWORD = '12345678';

    protected function configure(): void
    {
        $this
            ->setName('app:init')
            ->setDescription('Initializes the project by creating database, user, and running migrations.')
            ->addArgument('database', InputArgument::REQUIRED, 'The database name')
            ->addArgument('username', InputArgument::REQUIRED, 'The database username')
            ->addArgument('mysql-password', InputArgument::OPTIONAL, 'The MySQL root password', '')
            ->addOption('mariadb', null, InputOption::VALUE_NONE, 'Whether the database server is MariaDB')
            ->addOption('server-version', null, InputOption::VALUE_OPTIONAL, 'Explicit server version for Doctrine DBAL');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $io = new SymfonyStyle($input, $output);
        $this->databaseName = (string) $input->getArgument('database');
        $this->databaseUser = (string) $input->getArgument('username');
        $this->mysqlPassword = (string) $input->getArgument('mysql-password');
        $this->isMariaDb = (bool) $input->getOption('mariadb');
        $this->explicitServerVersion = (string) ($input->getOption('server-version') ?? '');

        $io->title('Initializing Project');

        // Step 1: Connect to MySQL and create database
        $io->section('Step 1: Creating database');
        $this->createDatabase($io);

        // Step 2: Create MySQL user
        $io->section('Step 2: Creating database user');
        $this->createDatabaseUser($io);

        // Step 3: Update .env file
        $io->section('Step 3: Updating .env file');
        $this->updateEnvFile($io);

        // Step 4: Run migrations
        $io->section('Step 4: Running migrations');
        $this->runMigrations($io);

        // Step 5: Create admin user
        $io->section('Step 5: Creating admin user');
        $this->createAdminUser($io);

        $io->success('Project initialized successfully!');

        return Command::SUCCESS;
    }

    private function createDatabase(SymfonyStyle $io): void
    {
        $dsn = sprintf('mysql:host=%s;port=%s', self::DB_HOST, self::DB_PORT);
        
        try {
            $pdo = new \PDO($dsn, self::DB_ROOT_USER, $this->getDatabasePassword());
            $pdo->setAttribute(\PDO::ATTR_ERRMODE, \PDO::ERRMODE_EXCEPTION);

            $this->serverVersion = $this->detectServerVersion($pdo);

            // Check if database exists
            $stmt = $pdo->prepare("SELECT SCHEMA_NAME FROM INFORMATION_SCHEMA.SCHEMATA WHERE SCHEMA_NAME = ?");
            $stmt->execute([$this->databaseName]);
            
            if ($stmt->fetch()) {
                $io->text(sprintf('Database "%s" already exists. Dropping it...', $this->databaseName));
                $pdo->exec(sprintf("DROP DATABASE `%s`", $this->databaseName));
            }

            $pdo->exec(sprintf("CREATE DATABASE `%s` CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci", $this->databaseName));
            $io->success(sprintf('Database "%s" created successfully.', $this->databaseName));
        } catch (\PDOException $e) {
            $io->error(sprintf('Failed to create database: %s', $e->getMessage()));
            throw $e;
        }
    }

    private function createDatabaseUser(SymfonyStyle $io): void
    {
        $dsn = sprintf('mysql:host=%s;port=%s', self::DB_HOST, self::DB_PORT);
        
        try {
            $pdo = new \PDO($dsn, self::DB_ROOT_USER, $this->getDatabasePassword());
            $pdo->setAttribute(\PDO::ATTR_ERRMODE, \PDO::ERRMODE_EXCEPTION);

            // Check if user exists and drop it
            $stmt = $pdo->prepare("SELECT EXISTS(SELECT 1 FROM mysql.user WHERE user = ?)");
            $stmt->execute([$this->databaseUser]);
            
            if ($stmt->fetchColumn()) {
                $io->text(sprintf('User "%s" already exists. Dropping it...', $this->databaseUser));
                $pdo->exec(sprintf("DROP USER '%s'@'%s'", $this->databaseUser, self::DB_HOST));
            }

            // Create user and grant privileges
            $pdo->exec(sprintf("CREATE USER '%s'@'%s' IDENTIFIED BY '%s'", $this->databaseUser, self::DB_HOST, self::DB_PASSWORD));
            $pdo->exec(sprintf("GRANT ALL PRIVILEGES ON `%s`.* TO '%s'@'%s'", $this->databaseName, $this->databaseUser, self::DB_HOST));
            $pdo->exec("FLUSH PRIVILEGES");

            $io->success(sprintf('User "%s" created successfully.', $this->databaseUser));
        } catch (\PDOException $e) {
            $io->error(sprintf('Failed to create user: %s', $e->getMessage()));
            throw $e;
        }
    }

    private string $mysqlPassword = '';
    private string $databaseName = '';
    private string $databaseUser = '';
    private string $serverVersion = '';
    private bool $isMariaDb = false;
    private string $explicitServerVersion = '';

    private function getDatabasePassword(): string { return $this->mysqlPassword; }

    private function updateEnvFile(SymfonyStyle $io): void
    {
        $envPath = $this->getApplication()->getKernel()->getProjectDir() . '/.env';
        $envContent = file_get_contents($envPath);

        // Remove existing DATABASE_URL if it exists
        $envContent = preg_replace('/^DATABASE_URL=.*$/m', '', $envContent);
        $envContent = trim($envContent);

        $serverVersion = $this->serverVersion ?: $this->resolveServerVersion();

        // Add new DATABASE_URL at the end
        $databaseUrl = sprintf(
            'DATABASE_URL="pdo-mysql://%s:%s@%s:%s/%s?serverVersion=%s&charset=utf8mb4"',
            $this->databaseUser,
            self::DB_PASSWORD,
            self::DB_HOST,
            self::DB_PORT,
            $this->databaseName,
            $serverVersion
        );

        $envContent .= "\n\n" . $databaseUrl . "\n";

        file_put_contents($envPath, $envContent);

        $io->success(sprintf('.env file updated with DATABASE_URL (serverVersion: %s)', $serverVersion));
    }

    private function resolveServerVersion(): string
    {
        if (!empty($this->explicitServerVersion)) {
            return $this->explicitServerVersion;
        }

        $dsn = sprintf('mysql:host=%s;port=%s', self::DB_HOST, self::DB_PORT);

        try {
            $pdo = new \PDO($dsn, self::DB_ROOT_USER, $this->getDatabasePassword());
            $pdo->setAttribute(\PDO::ATTR_ERRMODE, \PDO::ERRMODE_EXCEPTION);

            return $this->detectServerVersion($pdo);
        } catch (\Throwable) {
            return $this->isMariaDb ? 'mariadb-10.11.2' : '8.0.32';
        }
    }

    private function detectServerVersion(\PDO $pdo): string
    {
        if (!empty($this->explicitServerVersion)) {
            return $this->explicitServerVersion;
        }

        try {
            $rawVersion = (string) $pdo->query('SELECT VERSION()')->fetchColumn();
            $isMaria = $this->isMariaDb || stripos($rawVersion, 'mariadb') !== false;

            $normalized = preg_replace('/^5\.5\.5-/', '', $rawVersion);

            if (preg_match('/(\d+\.\d+(?:\.\d+)?)/', $normalized, $matches)) {
                $versionNumber = $matches[1];
                return $isMaria ? ('mariadb-' . $versionNumber) : $versionNumber;
            }
        } catch (\Throwable) {
        }

        return $this->isMariaDb ? 'mariadb-10.11.2' : '8.0.32';
    }

    private function runMigrations(SymfonyStyle $io): void
    {
        $projectDir = $this->getApplication()->getKernel()->getProjectDir();

        // Step 4a: Automatically generate migrations if needed
        $io->text('Generating migrations...');
        $makeProcess = new Process(['php', 'bin/console', 'make:migration', '--no-interaction']);
        $makeProcess->setWorkingDirectory($projectDir);
        $makeProcess->run(function ($type, $buffer) use ($io) {
            if (Process::OUT === $type) {
                $io->text($buffer);
            } else {
                $io->text('<fg=yellow>' . $buffer . '</>');
            }
        });

        // Step 4b: Execute migrations
        $process = new Process(['php', 'bin/console', 'doctrine:migrations:migrate', '--no-interaction', '--allow-no-migration']);
        $process->setWorkingDirectory($projectDir);
        
        $process->run(function ($type, $buffer) use ($io) {
            if (Process::OUT === $type) {
                $io->text($buffer);
            } else {
                $io->text('<fg=red>' . $buffer . '</>');
            }
        });

        if (!$process->isSuccessful()) {
            $io->error('Failed to run migrations');
            throw new \RuntimeException($process->getErrorOutput());
        }

        $io->success('Migrations completed successfully');
    }

    private function createAdminUser(SymfonyStyle $io): void
    {
        $process = new Process([
            'php', 
            'bin/console', 
            'app:create-user', 
            self::ADMIN_EMAIL, 
            self::ADMIN_PASSWORD,
            'ROLE_ADMIN'
        ]);
        $process->setWorkingDirectory($this->getApplication()->getKernel()->getProjectDir());
        
        $process->run(function ($type, $buffer) use ($io) {
            if (Process::OUT === $type) {
                $io->text($buffer);
            } else {
                $io->text('<fg=red>' . $buffer . '</>');
            }
        });

        if (!$process->isSuccessful()) {
            $io->error('Failed to create admin user');
            throw new \RuntimeException($process->getErrorOutput());
        }

        $io->success('Admin user created successfully');
    }
}
