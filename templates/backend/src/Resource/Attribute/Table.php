<?php

namespace App\Resource\Attribute;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Table
{
    public function __construct(
        public readonly ?string $label = null,
        public readonly bool $sortable = true,
        public readonly bool $searchable = true,
    ) {}
}
