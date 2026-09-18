<?php

namespace App\Controller;

use App\Service\Analytics\AnalyticsAggregatorInterface;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\DependencyInjection\Attribute\AutowireIterator;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/api/analytics', name: 'analytics_')]
final class AnalyticsController extends AbstractController
{
    /**
     * @var array<string, AnalyticsAggregatorInterface>
     */
    private array $aggregators = [];

    /**
     * @param iterable<AnalyticsAggregatorInterface> $aggregators
     */
    public function __construct(
        #[AutowireIterator('app.analytics_aggregator')] iterable $aggregators
    ) {
        foreach ($aggregators as $aggregator) {
            $this->aggregators[$aggregator->getName()] = $aggregator;
        }
    }

    #[Route('/{category}/{metric}', name: 'get_metric', methods: ['GET'])]
    public function getMetric(string $category, string $metric, \Symfony\Component\HttpFoundation\Request $request): JsonResponse
    {
        $name = sprintf('%s/%s', $category, $metric);

        if (!isset($this->aggregators[$name])) {
            return new JsonResponse([
                'error' => sprintf('Analytics handler for "%s" not found.', $name)
            ], Response::HTTP_NOT_FOUND);
        }

        $aggregator = $this->aggregators[$name];
        
        // Pass request or filters if aggregator supports it
        $value = method_exists($aggregator, 'getValueWithRequest')
            ? $aggregator->getValueWithRequest($request)
            : $aggregator->getValue();

        // If returned value is already an array or structured object, return directly
        if (is_array($value)) {
            return new JsonResponse($value);
        }

        return new JsonResponse([
            'count' => $value
        ]);
    }
}
