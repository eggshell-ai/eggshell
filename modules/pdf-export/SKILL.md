---
name: pdf_export
description: Generate PDF documents (invoices, receipts, quotations and other printable records) from a Twig template using mPDF, expose them through an authenticated endpoint, and open or download them from the frontend.
category: reporting
tags: [pdf, export, invoice, print, mpdf, twig]
---

# PDF Export Skill

The `pdf-export` module has been installed into this project. It provides a
template registry, an mPDF renderer, a controller, and a frontend client. You do
**not** need to write PDF plumbing: you write one small provider class per
document type and one Twig template per layout.

## What was installed

Backend (`backend/src/`):

- `Service/PdfTemplate/TemplatesPdf.php` — loads a template by name, renders it
  with Twig and returns raw PDF bytes.
- `Service/PdfTemplate/TemplateLoader.php` — reads
  `Service/PdfTemplate/Templates/<name>/config.json`, validates it, and hydrates
  the declared input models (including from the `samples` block).
- `Service/PdfTemplate/Model/*` — `CompanyModel`, `InvoiceModel`,
  `InvoiceItemModel`, `TaxModel`.
- `Service/PdfTemplate/Templates/invoice/` — `config.json` + `template.html.twig`.
- `Service/PdfTemplate/PdfProviderInterface.php` — the contract you implement.
- `Service/PdfTemplate/PdfProviderLocator.php` — finds the provider for a
  template name (providers are tagged `app.pdf_provider`).
- `Controller/PdfController.php` — the HTTP endpoints.

Config (`backend/config/`):

- `config/packages/twig.yaml` — enables Twig. Only created when the file did not
  already exist.

Frontend (`frontend/src/`):

- `api/pdfClient.ts` — `listTemplates()` and `openPdf(template, id)`.

Composer packages: `mpdf/mpdf`, `symfony/twig-bundle`. The `TwigBundle` was
registered in `backend/config/bundles.php`.

Providers are discovered automatically: `PdfProviderInterface` carries
`#[AutoconfigureTag('app.pdf_provider')]`, and the template sets
`autoconfigure: true`, so a new provider needs no `services.yaml` entry.

## Endpoints

| Method | Path | Purpose |
| --- | --- | --- |
| `GET` | `/api/pdf/templates` | Lists the templates that have a provider. |
| `GET` | `/api/pdf/{template}/{id}` | Renders one document as `application/pdf`. |

Both require authentication and the `pdf_export.view` permission.

## Wiring a new document type

A document type needs three things: a template, a provider, and a download
control in the UI.

### 1. Add the template

Create `backend/src/Service/PdfTemplate/Templates/<name>/` with:

`config.json`

```json
{
  "name": "invoice",
  "dir": "invoice",
  "description": "Standard sales invoice",
  "twig": { "strict_variables": false },
  "mpdf": { "format": "A4", "orientation": "P", "margin_left": 12, "margin_right": 12 },
  "inputs": [
    { "name": "invoice", "class": "App\\Service\\PdfTemplate\\Model\\InvoiceModel" }
  ],
  "samples": {
    "invoice": { "invNo": "INV-0001", "date": "2026-01-01", "customerName": "Sample", "items": [], "taxes": [], "total": 0 }
  }
}
```

`template.html.twig` — normal Twig HTML. Keep the CSS inline, mPDF ignores
external stylesheets. Use `dejavusans` (the default) for non-ASCII text.

Rules:

- `name` must match the directory name.
- `inputs` names must match the parameter names of the matching provider's
  `buildInputs()`. Mismatched names throw `Missing sample value ...` when the
  sample endpoint is exercised.
- Every value the template reads must be a property of the declared model, or
  `strict_variables` must be `false`. Prefer declaring the model properly.

### 2. Add the provider

Create a class implementing
`App\Service\PdfTemplate\PdfProviderInterface` under
`backend/src/Service/PdfTemplate/Providers/`. It needs no registration: the
interface is tagged for autoconfiguration.

```php
namespace App\Service\PdfTemplate\Providers;

use App\Entity\Sale;
use App\Service\PdfTemplate\Model\CompanyModel;
use App\Service\PdfTemplate\Model\InvoiceItemModel;
use App\Service\PdfTemplate\Model\InvoiceModel;
use App\Service\PdfTemplate\Model\TaxModel;
use App\Service\PdfTemplate\PdfProviderInterface;
use Doctrine\ORM\EntityManagerInterface;
use App\Repository\SaleRepository;

final class SaleInvoiceProvider implements PdfProviderInterface
{
    public function __construct(private readonly EntityManagerInterface $entityManager) {}

    public function template(): string
    {
        return 'invoice';
    }

    public function supports(int|string $id): bool
    {
        return $this->entityManager->getRepository(Sale::class)->find($id) !== null;
    }

    /** @return array<string, mixed> */
    public function buildInputs(int|string $id): array
    {
        $sale = $this->entityManager->getRepository(Sale::class)->find($id);
        if ($sale === null) {
            throw new \RuntimeException("Sale {$id} not found.");
        }

        $items = array_map(
            static fn ($item) => new InvoiceItemModel(
                name: (string) $item->productId,
                quantity: (int) $item->quantity,
                unitPrice: (float) $item->unitPrice,
                total: round((float) $item->quantity * (float) $item->unitPrice, 2),
            ),
            $entityManager->getRepository(SaleItem::class)->findBy(['saleId' => $sale->id]),
        );

        return ['invoice' => new InvoiceModel(
            invNo: 'INV-' . str_pad((string) $sale->id, 6, '0', STR_PAD_LEFT),
            date: (string) $sale->saleDate,
            company: new CompanyModel(
                name: 'My Company',
                address: '—',
                phone: '—',
                email: '—',
                taxNumber: '—',
            ),
            customerName: (string) $sale->customerId,
            customerPhone: '',
            customerEmail: '',
            items: $items,
            taxes: [new TaxModel('VAT', 10, round((float) $sale->totalAmount * 0.1, 2))],
            total: round((float) $sale->totalAmount, 2),
        )];
    }
}
```

Important:

- `template()` must match a directory under `Templates/`.
- `buildInputs()` keys must match the `inputs` names in `config.json`.
- Read entities through `EntityManagerInterface`; do not inject the controller.
- Do not modify `SaleController` or any other existing controller. The module
  owns `/api/pdf/*`; if an existing controller already has an invoice action,
  leave it alone — the module route is the general one.

### 3. Add the frontend control

`frontend/src/api/pdfClient.ts` is already installed:

```ts
import pdfClient from '@/api/pdfClient';

pdfClient.openPdf('invoice', sale.id);
```

Add a button wherever the record is shown. With the shared resource UI, add a row
action in the resource's `.ts` file under `frontend/src/resources/`:

```ts
import { FilePdfOutlined } from '@ant-design/icons';
import pdfClient from '@/api/pdfClient';

{
  title: 'Invoice',
  icon: <FilePdfOutlined />,
  onClick: (record) => pdfClient.openPdf('invoice', record.id),
}
```

If the page is a custom view, use the same call inside an `<Button>`.

`openPdf` fetches the endpoint with the auth token (the shared axios instance
adds it) and opens the blob in a new tab, so the browser's PDF viewer handles
printing and downloading. Do not build a `window.open` URL by hand: a plain URL
will not carry the bearer token and returns 401.

## Verifying

1. `cd backend && php bin/console debug:router` — the two `/api/pdf` routes must
   appear. If they do not, the controller was not copied or the bundle was not
   registered.
2. `GET /api/pdf/templates` — should list `invoice` once
   `SaleInvoiceProvider` exists.
3. `GET /api/pdf/invoice/1` — should return a PDF, not JSON and not a 500.
   A 500 with "template not found" means the `template()` name does not match a
   directory. A 500 with a Twig "variable does not exist" error means
   `strict_variables` is on and the template reads an undeclared property.
4. `php bin/console cache:clear` after adding a provider or a bundle — Symfony
   caches the service container and new providers are invisible until it clears.

## Permissions

The controller checks `pdf_export.view`. If roles are managed in this project,
add the permission to the role that should be able to download documents
(`src/Service/PermissionRegistry.php` auto-creates permissions declared in code,
so check how the project registers them before editing roles by hand).

## Troubleshooting

| Symptom | Cause | Fix |
| --- | --- | --- |
| 401 on the PDF URL | Opened the endpoint without the auth header | Use `pdfClient.openPdf`, not `window.open` |
| 404 on `/api/pdf/...` | Controller not in `backend/src/Controller`, or route cache stale | Check the file exists; run `cache:clear` |
| `Class "Mpdf\Mpdf" not found` | Composer packages missing | `cd backend && composer require mpdf/mpdf` |
| `Unable to find template "template.html.twig"` | Template directory missing the file | Add `template.html.twig` next to `config.json` |
| Garbled characters | Non-ASCII text with the default font | The default `dejavusans` is safe; custom fonts need `fontDir` in `mpdf` config |
| Blank PDF | mPDF ignores external CSS | Move styles into a `<style>` block in the Twig template |
