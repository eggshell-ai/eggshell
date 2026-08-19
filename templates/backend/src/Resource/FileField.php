<?php

namespace App\Resource;

#[\Attribute(\Attribute::TARGET_PROPERTY)]
class FileField
{
    public function __construct(
        public readonly string $uploadDirectory = 'uploads'
    ) {
    }
}
