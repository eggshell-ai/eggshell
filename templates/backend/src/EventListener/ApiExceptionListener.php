<?php

namespace App\EventListener;

use Symfony\Component\EventDispatcher\Attribute\AsEventListener;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\Event\ExceptionEvent;
use Symfony\Component\HttpKernel\Exception\HttpExceptionInterface;

#[AsEventListener(event: 'kernel.exception', priority: 10)]
class ApiExceptionListener
{
    public function onKernelException(ExceptionEvent $event): void
    {
        $request = $event->getRequest();

        // Only format API routes or JSON requests
        if (!str_starts_with($request->getPathInfo(), '/api') && $request->getContentTypeFormat() !== 'json') {
            return;
        }

        $throwable = $event->getThrowable();
        $statusCode = $throwable instanceof HttpExceptionInterface
            ? $throwable->getStatusCode()
            : Response::HTTP_INTERNAL_SERVER_ERROR;

        // Mask internal server error details in non-dev environments
        $isDev = in_array($_SERVER['APP_ENV'] ?? $_ENV['APP_ENV'] ?? 'prod', ['dev', 'test'], true);
        $detail = ($statusCode >= 500 && !$isDev)
            ? 'An internal server error occurred.'
            : $throwable->getMessage();

        $title = Response::$statusTexts[$statusCode] ?? 'An error occurred';

        $payload = [
            'status' => $statusCode,
            'title' => $title,
            'detail' => $detail,
        ];

        $event->setResponse(new JsonResponse($payload, $statusCode));
    }
}
