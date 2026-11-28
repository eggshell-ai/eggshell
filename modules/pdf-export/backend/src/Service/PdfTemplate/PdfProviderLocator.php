<?php

namespace App\Service\PdfTemplate;

use Symfony\Component\DependencyInjection\Attribute\AutowireIterator;

/**
 * Finds the provider responsible for a template name.
 *
 * Providers are collected through the `app.pdf_provider` tag, so this class does
 * not need to know which document types exist.
 */
final class PdfProviderLocator
{
    /** @var array<string, PdfProviderInterface> */
    private array $providers = [];

    /**
     * @param iterable<PdfProviderInterface> $providers
     */
    public function __construct(
        #[AutowireIterator('app.pdf_provider')]
        iterable $providers,
    ) {
        foreach ($providers as $provider) {
            $this->providers[$provider->template()] = $provider;
        }
    }

    public function get(string $template): PdfProviderInterface
    {
        return $this->providers[$template]
            ?? throw new \RuntimeException("No PDF provider is registered for template '{$template}'.");
    }

    public function has(string $template): bool
    {
        return isset($this->providers[$template]);
    }

    /**
     * @return list<string> Every template name that has a provider, sorted.
     */
    public function templates(): array
    {
        $names = array_keys($this->providers);
        sort($names);

        return $names;
    }
}
