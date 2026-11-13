<?php

namespace App\Service\PdfTemplate;

use Mpdf\Mpdf;
use Twig\Environment;
use Twig\Loader\FilesystemLoader;

/**
 * Loads a PDF template by name, renders its Twig template with the supplied
 * input models and produces the PDF binary via mPDF.
 *
 * mPDF configuration (format, orientation, margins, ...) and Twig options come
 * from the template's `config.json`, under the `mpdf` and `twig` keys.
 */
final class TemplatesPdf
{
    public function __construct(
        private readonly TemplateLoader $loader,
    ) {}

    /**
     * @param array<string, mixed> $inputs Values exposed to the twig template,
     *        keyed by input name. Use `renderSample()` for template-local samples.
     * @return string Raw PDF bytes.
     */
    public function render(string $name, array $inputs = []): string
    {
        $config = $this->loader->getConfig($name);
        $mpdfConfig = is_array($config['mpdf'] ?? null) ? $config['mpdf'] : [];

        $mpdf = new Mpdf($mpdfConfig);

        $html = $this->twig($name, $config)->render('template.html.twig', $inputs);
        $mpdf->WriteHTML($html);

        return $mpdf->Output('', \Mpdf\Output\Destination::STRING_RETURN);
    }

    /**
     * Render a template with its own sample data from `config.json`.
     *
     * @return string Raw PDF bytes.
     */
    public function renderSample(string $name): string
    {
        return $this->render($name, $this->loader->buildInputs($name));
    }

    /**
     * Builds a Twig environment rooted at the template directory.
     *
     * `strict_variables` is on by default so a typo in a template fails loudly
     * instead of printing an empty cell, but a template may relax it through
     * `"twig": { "strict_variables": false }` when its layout intentionally
     * tolerates absent optional values.
     */
    private function twig(string $name, array $config): Environment
    {
        $options = is_array($config['twig'] ?? null) ? $config['twig'] : [];

        return new Environment(new FilesystemLoader($this->loader->getTemplateDir($name)), [
            'autoescape' => 'html',
            'strict_variables' => (bool) ($options['strict_variables'] ?? true),
            'cache' => false,
        ]);
    }
}
