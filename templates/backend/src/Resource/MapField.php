<?php

namespace App\Resource;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class MapField
{
    public function __construct(
        public readonly ?string $field = null,
        public readonly ?string $targetEntity = null
    ) {
    }
}
