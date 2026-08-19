<?php

namespace App\Resource\Attribute;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Relation
{
    public function __construct(
        public readonly string $targetEntity,
        public readonly ?string $titleKey = null
    ) {
    }
}
