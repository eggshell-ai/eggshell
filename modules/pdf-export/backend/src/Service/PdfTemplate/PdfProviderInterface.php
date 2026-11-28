<?php

namespace App\Service\PdfTemplate;

use Symfony\Component\DependencyInjection\Attribute\AutoconfigureTag;

/**
 * Supplies the model inputs a PDF template needs for one record.
 *
 * One provider per document type. `#[AutoconfigureTag]` on the interface means
 * every implementation is tagged `app.pdf_provider` automatically, so adding a
 * provider is enough — no service definition and no `services.yaml` edit.
 */
#[AutoconfigureTag('app.pdf_provider')]
interface PdfProviderInterface
{
    /**
     * The template directory name under `Service/PdfTemplate/Templates/`.
     */
    public function template(): string;

    /**
     * Whether this provider can render the given record id.
     */
    public function supports(int|string $id): bool;

    /**
     * The values exposed to the Twig template, keyed by the input name declared
     * in the template's `config.json`.
     *
     * @return array<string, mixed>
     */
    public function buildInputs(int|string $id): array;
}
