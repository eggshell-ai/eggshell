<?php

namespace App\Service\PdfTemplate\Model;

/**
 * One line item on a document.
 */
final class InvoiceItemModel
{
    public function __construct(
        public string $name = '',
        public int $quantity = 0,
        public float $unitPrice = 0.0,
        public float $total = 0.0,
    ) {}
}
