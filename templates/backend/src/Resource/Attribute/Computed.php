<?php

namespace App\Resource\Attribute;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Computed
{
    public function __construct(
        public readonly ?string $sqlExpression = null,
        public readonly ?string $computeExpression = null,
    ) {}
}
