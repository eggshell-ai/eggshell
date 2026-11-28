<?php

namespace App\Service\PdfTemplate;

use Symfony\Component\HttpKernel\KernelInterface;

/**
 * Loads PDF template definitions from `Service/PdfTemplate/Templates/<name>/`.
 *
 * Each template directory must contain:
 *   - `config.json` — metadata, mPDF configuration, Twig options, declared inputs
 *   - `template.html.twig` — the rendering template
 *
 * A template never carries its own PHP: the input models it references live in
 * `Service/PdfTemplate/Model/` and are shared between templates.
 */
final class TemplateLoader
{
    public const TEMPLATE_DIR = 'Templates';

    /** @var array<string, array<string, mixed>> */
    private array $configCache = [];

    public function __construct(
        private readonly KernelInterface $kernel,
    ) {}

    public function getRootDir(): string
    {
        return __DIR__ . '/' . self::TEMPLATE_DIR;
    }

    public function getTemplateDir(string $name): string
    {
        $config = $this->getConfig($name);
        $dir = $this->getRootDir() . '/' . ($config['dir'] ?? $name);

        if (!is_dir($dir) || !is_file($dir . '/config.json')) {
            throw new \RuntimeException("PDF template '{$name}' not found.");
        }

        return $dir;
    }

    /**
     * Reads and validates a template's `config.json`. Parsed documents are
     * cached for the lifetime of the request, so rendering a document does not
     * read the same file twice.
     *
     * @return array{name: string, dir: string, description: string, mpdf: array, twig: array, inputs: array, samples: array}
     */
    public function getConfig(string $name): array
    {
        if (isset($this->configCache[$name])) {
            return $this->configCache[$name];
        }

        if (!preg_match('/^[A-Za-z0-9_\-]+$/', $name)) {
            throw new \InvalidArgumentException("Invalid PDF template name '{$name}'.");
        }

        $path = $this->getRootDir() . '/' . $name . '/config.json';
        if (!is_file($path)) {
            throw new \RuntimeException("PDF template '{$name}' not found (missing {$path}).");
        }

        $data = json_decode((string) file_get_contents($path), true);
        if (!is_array($data)) {
            throw new \RuntimeException("Invalid config.json for PDF template '{$name}'.");
        }

        if ($name !== ($data['name'] ?? $name)) {
            throw new \RuntimeException("config.json name mismatch for PDF template '{$name}'.");
        }

        return $this->configCache[$name] = $data;
    }

    /**
     * Instantiate declared input models, optionally filling them with values
     * from config.json's "samples" block, overriding any per-input defaults.
     *
     * @return array<string, object> Input name => model instance
     */
    public function buildInputs(string $name): array
    {
        $config = $this->getConfig($name);
        $samples = $config['samples'] ?? [];
        $models = [];

        foreach (($config['inputs'] ?? []) as $input) {
            $key = (string) ($input['name'] ?? '');
            $class = (string) ($input['class'] ?? '');
            if ($key === '' || !class_exists($class)) {
                throw new \RuntimeException("PDF template '{$name}' declares an invalid input model '{$class}'.");
            }

            $sample = $samples[$key] ?? [];
            $models[$key] = is_array($sample) && count($sample) > 0
                ? $this->hydrate($class, $sample)
                : new $class();
        }

        return $models;
    }

    /**
     * Construct a model instance from an associative sample array by matching
     * constructor argument names to array keys, coercing scalars and lists, then
     * falling back to declared defaults.
     */
    private function hydrate(string $class, array $data): object
    {
        $reflection = new \ReflectionClass($class);
        $constructor = $reflection->getConstructor();

        if ($constructor === null) {
            return new $class();
        }

        $args = [];
        foreach ($constructor->getParameters() as $param) {
            $paramName = $param->getName();
            if (!array_key_exists($paramName, $data)) {
                if ($param->isDefaultValueAvailable()) {
                    continue;
                }
                throw new \RuntimeException("Missing sample value for '{$paramName}' in input model '{$class}'.");
            }

            $args[$paramName] = $this->coerce(
                $data[$paramName],
                $param->getType(),
                $class,
                $paramName,
            );
        }

        return $reflection->newInstanceArgs($args);
    }

    private function coerce(mixed $value, ?\ReflectionType $type, string $class, string $param): mixed
    {
        if ($type instanceof \ReflectionNamedType) {
            if ($type->isBuiltin()) {
                return match ($type->getName()) {
                    'int' => (int) $value,
                    'float' => (float) $value,
                    // A JSON sample may legitimately carry `"false"`; PHP's bool
                    // cast would turn that string into true.
                    'bool' => is_string($value)
                        ? filter_var($value, FILTER_VALIDATE_BOOL, FILTER_NULL_ON_FAILURE) ?? (bool) $value
                        : (bool) $value,
                    'string' => (string) $value,
                    'array' => (array) $value,
                    default => $value,
                };
            }

            // Object-typed input (e.g. CompanyModel nested inside InvoiceModel).
            if (is_array($value) && class_exists($type->getName())) {
                return $this->hydrate($type->getName(), $value);
            }
        }

        return $value;
    }
}
