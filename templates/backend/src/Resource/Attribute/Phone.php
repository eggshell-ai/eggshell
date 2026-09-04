<?php

namespace App\Resource\Attribute;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class Phone
{
    public function __construct(
        public readonly ?string $message = null,
    ) {}
}
