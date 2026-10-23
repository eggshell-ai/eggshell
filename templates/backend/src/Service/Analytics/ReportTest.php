<?php

namespace App\Service\Analytics;

#[\Attribute(\Attribute::TARGET_CLASS)]
class ReportTest
{
    /**
     * @param array<string, mixed> $params Optional default query parameters passed when testing the report.
     * @param bool $skip Set to true to skip automatic execution of this report during linting/testing.
     */
    public function __construct(
        public readonly array $params = [],
        public readonly bool $skip = false,
    ) {}
}
