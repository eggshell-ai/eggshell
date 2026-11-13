<?php

namespace App\Controller;

use App\Service\PdfTemplate\PdfProviderLocator;
use App\Service\PdfTemplate\TemplatesPdf;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\Exception\NotFoundHttpException;
use Symfony\Component\Routing\Attribute\Route;

/**
 * Serves rendered PDF documents and the list of available template names.
 *
 * The controller knows nothing about any particular document type: a provider
 * supplies the inputs for one template, and the locator selects it. Adding a new
 * document type therefore never requires editing this file.
 */
#[Route('/api/pdf', name: 'pdf.')]
final class PdfController extends AbstractController
{
    public function __construct(
        private readonly TemplatesPdf $templates,
        private readonly PdfProviderLocator $providers,
    ) {}

    #[Route('/templates', name: 'templates', methods: ['GET'])]
    public function templates(): JsonResponse
    {
        $this->denyAccessUnlessGranted('pdf_export.view');

        return new JsonResponse(['templates' => $this->providers->templates()]);
    }

    #[Route(
        '/{template}/{id}',
        name: 'render',
        requirements: ['template' => '[A-Za-z0-9_\-]+', 'id' => '[A-Za-z0-9_\-]+'],
        methods: ['GET'],
    )]
    public function render(string $template, string $id): Response
    {
        $this->denyAccessUnlessGranted('pdf_export.view');

        if (!$this->providers->has($template)) {
            throw new NotFoundHttpException(
                "No PDF provider is registered for template '{$template}'."
            );
        }

        $provider = $this->providers->get($template);

        // A numeric id is the common case; the provider sees the raw string so a
        // project using UUIDs or slugs is not forced to cast.
        $identifier = ctype_digit($id) ? (int) $id : $id;
        if (!$provider->supports($identifier)) {
            throw new NotFoundHttpException(
                "No record '{$id}' for PDF template '{$template}'."
            );
        }

        $bytes = $this->templates->render($template, $provider->buildInputs($identifier));

        return new Response($bytes, Response::HTTP_OK, [
            'Content-Type' => 'application/pdf',
            'Content-Disposition' => sprintf('inline; filename="%s-%s.pdf"', $template, $id),
            'Cache-Control' => 'private, no-store',
        ]);
    }
}

