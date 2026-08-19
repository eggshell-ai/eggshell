<?php

namespace App\Resource\Attribute;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Form
{
    public function __construct(
        public readonly ?string $label = null,
        public readonly ?string $type = null,
        public readonly bool $required = false,
        public readonly array $options = [],
    ) {}
}
