<?php

namespace App\Service\PdfTemplate\Model;

/**
 * The document-level model of a sales invoice.
 *
 * @param list<InvoiceItemModel> $items
 * @param list<TaxModel> $taxes
 */
final class InvoiceModel
{
    public function __construct(
        public string $invNo = '',
        public string $date = '',
        public CompanyModel $company = new CompanyModel(),
        public string $customerName = '',
        public string $customerPhone = '',
        public string $customerEmail = '',
        public array $items = [],
        public array $taxes = [],
        public float $total = 0.0,
    ) {}
}
