<?php

namespace App\Service\PdfTemplate\Model;

/**
 * The issuing company's details, as printed in a document header.
 */
final class CompanyModel
{
    public function __construct(
        public string $name = '',
        public string $address = '',
        public string $phone = '',
        public string $email = '',
        public string $taxNumber = '',
    ) {}
}
