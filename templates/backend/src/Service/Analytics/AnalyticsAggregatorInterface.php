<?php

namespace App\Service\Analytics;

interface AnalyticsAggregatorInterface
{
    /**
     * Get the unique metric name/key handled by this aggregator (e.g., 'customers/count').
     */
    public function getName(): string;

    /**
     * Calculate and return the analytics metric value.
     */
    public function getValue(): mixed;
}
