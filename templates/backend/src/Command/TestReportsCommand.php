<?php

namespace App\Command;

use App\Resource\Attribute\Computed;
use App\Service\Analytics\AnalyticsAggregatorInterface;
use App\Service\Analytics\ReportTest;
use Doctrine\ORM\EntityManagerInterface;
use ReflectionClass;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Input\InputOption;
use Symfony\Component\Console\Output\OutputInterface;
use Symfony\Component\DependencyInjection\Attribute\AutowireIterator;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpKernel\KernelInterface;

class TestReportsCommand extends Command
{
    /**
     * @param iterable<AnalyticsAggregatorInterface> $aggregators
     */
    public function __construct(
        #[AutowireIterator('app.analytics_aggregator')]
        private readonly iterable $aggregators,
        private readonly EntityManagerInterface $entityManager,
        private readonly KernelInterface $kernel
    ) {
        parent::__construct();
    }

    protected function configure(): void
    {
        $this
            ->setName('app:analytics:test')
            ->setDescription('Executes all registered analytics reports and validates computed entity columns, reporting any errors.')
            ->addOption('json', null, InputOption::VALUE_NONE, 'Output results as JSON for lint tools')
            ->addOption('name', null, InputOption::VALUE_OPTIONAL, 'Filter by specific report name');
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $isJson = (bool) $input->getOption('json');
        $filterName = $input->getOption('name');
        $projectDir = $this->kernel->getProjectDir();

        $issues = [];
        $testedCount = 0;
        $skippedCount = 0;

        // 1. Validate computed column DQL expressions on Doctrine entities
        $metadataList = $this->entityManager->getMetadataFactory()->getAllMetadata();
        foreach ($metadataList as $metadata) {
            $entityClass = $metadata->getName();
            if (!class_exists($entityClass)) {
                continue;
            }

            $reflection = new ReflectionClass($entityClass);
            $entityFilePath = $this->getRelativePath($reflection->getFileName(), $projectDir);

            foreach ($reflection->getProperties() as $property) {
                $computedAttrs = $property->getAttributes(Computed::class);
                if (empty($computedAttrs)) {
                    continue;
                }

                /** @var Computed $computed */
                $computed = $computedAttrs[0]->newInstance();
                $sqlExpr = trim((string) ($computed->sqlExpression ?? ''));
                if ($sqlExpr === '') {
                    continue;
                }

                // Normalise expression
                if (preg_match('/^COALESCE\(\s*\(\s*SELECT\s+(.*?)\s+FROM\s+(.*)\)\s*,\s*([^)]+)\)$/is', $sqlExpr, $m)) {
                    $sqlExpr = sprintf('(SELECT COALESCE(%s, %s) FROM %s)', $m[1], $m[3], $m[2]);
                } elseif (preg_match('/^SELECT\b/i', $sqlExpr)) {
                    $sqlExpr = '(' . $sqlExpr . ')';
                }

                $dql = sprintf('SELECT %s FROM %s e WHERE e.id = :id', $sqlExpr, $entityClass);
                try {
                    // Compiles the DQL without executing it against rows
                    $this->entityManager->createQuery($dql)->setParameter('id', 0)->getSQL();
                } catch (\Throwable $e) {
                    $issues[] = [
                        'file' => $entityFilePath,
                        'line' => $property->getStartLine() ?? $reflection->getStartLine() ?? 1,
                        'column' => null,
                        'message' => sprintf(
                            'Computed column "%s::$%s" has invalid sqlExpression DQL: %s',
                            $reflection->getShortName(),
                            $property->getName(),
                            $e->getMessage()
                        ),
                        'severity' => 'error',
                        'source' => 'app:analytics:test',
                    ];
                }
            }
        }

        // 2. Validate and execute each AnalyticsAggregator
        foreach ($this->aggregators as $aggregator) {
            $reflection = new ReflectionClass($aggregator);
            $filePath = $this->getRelativePath($reflection->getFileName(), $projectDir);
            $className = $reflection->getShortName();

            try {
                $name = $aggregator->getName();
            } catch (\Throwable $e) {
                $issues[] = [
                    'file' => $filePath,
                    'line' => $reflection->getStartLine() ?? 1,
                    'column' => null,
                    'message' => sprintf('Aggregator %s::getName() threw an error: %s', $className, $e->getMessage()),
                    'severity' => 'error',
                    'source' => 'app:analytics:test',
                ];
                continue;
            }

            if ($filterName && !str_contains($name, $filterName)) {
                continue;
            }

            // Check 2-segment rule: must be <category>/<metric>
            $trimmedName = trim($name, '/');
            $segments = explode('/', $trimmedName);
            if (count($segments) !== 2 || empty($segments[0]) || empty($segments[1])) {
                $issues[] = [
                    'file' => $filePath,
                    'line' => $reflection->getMethod('getName')->getStartLine() ?? $reflection->getStartLine() ?? 1,
                    'column' => null,
                    'message' => sprintf(
                        'Aggregator %s::getName() returns "%s" which violates the 2-segment rule. Format must strictly be "<resource>/<metric>" (e.g. "sales/monthly", "customers/count").',
                        $className,
                        $name
                    ),
                    'severity' => 'error',
                    'source' => 'app:analytics:test',
                ];
            }

            // Check for #[ReportTest] attribute
            $testAttrs = $reflection->getAttributes(ReportTest::class);
            /** @var ReportTest|null $testConfig */
            $testConfig = !empty($testAttrs) ? $testAttrs[0]->newInstance() : null;

            if ($testConfig && $testConfig->skip) {
                $skippedCount++;
                continue;
            }

            $params = $testConfig ? $testConfig->params : [];

            // Execute the report inside a transaction that is always rolled back
            $connection = $this->entityManager->getConnection();
            $connection->beginTransaction();
            $previousGet = $_GET;
            $_GET = $params;

            try {
                $request = Request::create('/api/analytics/' . $name, 'GET', $params);
                if (method_exists($aggregator, 'getValueWithRequest')) {
                    $aggregator->getValueWithRequest($request);
                } else {
                    $aggregator->getValue();
                }
                $testedCount++;
            } catch (\Throwable $e) {
                $errorLine = $this->findLineInFile($e, $reflection->getFileName()) ?? $reflection->getStartLine() ?? 1;
                $issues[] = [
                    'file' => $filePath,
                    'line' => $errorLine,
                    'column' => null,
                    'message' => sprintf('Report "%s" (%s) failed execution: %s', $name, $className, $e->getMessage()),
                    'severity' => 'error',
                    'source' => 'app:analytics:test',
                ];
            } finally {
                $_GET = $previousGet;
                if ($connection->isTransactionActive()) {
                    $connection->rollBack();
                }
            }
        }

        if ($isJson) {
            $output->writeln(json_encode([
                'success' => count($issues) === 0,
                'testedReports' => $testedCount,
                'skippedReports' => $skippedCount,
                'totalErrors' => count($issues),
                'issues' => $issues,
            ], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES));
        } else {
            if (count($issues) === 0) {
                $output->writeln(sprintf('<info>All %d report(s) executed successfully (%d skipped).</info>', $testedCount, $skippedCount));
            } else {
                $output->writeln(sprintf('<error>Found %d issue(s) while testing reports:</error>', count($issues)));
                foreach ($issues as $issue) {
                    $output->writeln(sprintf('  - %s:%d: %s', $issue['file'], $issue['line'], $issue['message']));
                }
            }
        }

        return count($issues) === 0 ? Command::SUCCESS : Command::FAILURE;
    }

    private function getRelativePath(?string $fullPath, string $projectDir): string
    {
        if (!$fullPath) {
            return 'src/Service/Analytics';
        }
        $normalizedFull = str_replace('\\', '/', $fullPath);
        $normalizedProj = rtrim(str_replace('\\', '/', $projectDir), '/') . '/';
        if (str_starts_with($normalizedFull, $normalizedProj)) {
            return substr($normalizedFull, strlen($normalizedProj));
        }
        return $normalizedFull;
    }

    private function findLineInFile(\Throwable $e, ?string $targetFile): ?int
    {
        if (!$targetFile) {
            return null;
        }
        $targetFileNormalized = str_replace('\\', '/', realpath($targetFile) ?: $targetFile);

        if (str_replace('\\', '/', realpath($e->getFile()) ?: $e->getFile()) === $targetFileNormalized) {
            return $e->getLine();
        }

        foreach ($e->getTrace() as $trace) {
            if (isset($trace['file'])) {
                $traceNormalized = str_replace('\\', '/', realpath($trace['file']) ?: $trace['file']);
                if ($traceNormalized === $targetFileNormalized) {
                    return $trace['line'] ?? null;
                }
            }
        }

        return null;
    }
}
