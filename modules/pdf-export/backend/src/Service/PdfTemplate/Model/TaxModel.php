<?php

namespace App\Service\PdfTemplate\Model;

/**
 * One tax line applied to a document.
 */
final class TaxModel
{
    public function __construct(
        public string $name = '',
        public float $rate = 0.0,
        public float $amount = 0.0,
    ) {}
}
