# Transactional Records
Many entries for day-to-day transactions (like sales, purchase, vouchers, etc.) require a main header account associated with multiple child entries. This is a complete example of how to build such a system.

## Related Entities
We generally need entities like chart of accounts, customers and products to accompany such records. These entities must exist before we perform our work.
For this example, we shall consider that `customers` and `products` are already created.

## Workflow
### Step 1 — Call `sync_schema` for all resources

```javascript
sync_schema({
  resources: [
    {
      name: "order_items",
      endpoint: "/order-items",
      fields: [
        { name: "orderId", type: "foreign", label: "Order ID", required: true }, // We need at least one field to reference the parent resource
        { name: "productId", type: "select", label: "Product", required: true, resource: "products" }, // We use a literal string containing the product name, automatically converted to resource object in JavaScript
        { name: "quantity", type: "number", label: "Quantity", integer: true, min: 1, required: true, default: 1 },
        {
          name: "unitPrice",
          type: "decimal",
          label: "Unit Price",
          scale: 2,
          min: 0,
          required: true,
          lookup: {
            resource: "products",
            matchField: "id",
            matchValue: "data.productId",
            targetField: "price",
            on: "change",
            overwrite: false // Automatically load product price when a new product is selected, but don't overwrite if user wrote their own rates
          }
        },
        {
          name: "subtotal",
          type: "decimal",
          label: "Subtotal",
          scale: 2,
          computed: true,
          computeExpression: "(Number(data.quantity || 0) * Number(data.unitPrice || 0)).toFixed(2)",
          sqlExpression: "e.quantity * e.unitPrice",
        }
      ]
    },
    {
      name: "orders",
      endpoint: "/orders",
      fields: [
        { name: "orderNumber", type: "text", label: "Order #", form: false, unique: true, searchable: true, sortable: true }, // Auto-generated via OrderSequenceHook on create
        { name: "customerId", type: "select", label: "Customer", required: true, filterable: true, searchable: true, resource: "customers" },
        { name: "orderDate", type: "date", label: "Order Date", required: true, default: "today", filterable: true, sortable: true },
        {
          name: "status",
          type: "select",
          label: "Status",
          options: {
            draft: "Draft",
            confirmed: "Confirmed"
          },
          default: "draft",
          required: true,
          filterable: true,
          sortable: true
        },
        {
          name: "items",
          type: "table",
          label: "Order Items",
          resource: "order_items",
          map: "orderId",
          columns: [
            { name: "productId", label: "Product" },
            { name: "quantity", label: "Quantity" },
            { name: "unitPrice", label: "Unit Price" },
            { name: "subtotal", label: "Subtotal" } // computed on order_items — no isCalculated needed
          ]
        },
        {
          name: "totalAmount",
          type: "decimal",
          label: "Total Amount",
          scale: 2,
          computed: true,
          computeExpression: "Array.isArray(data.items) ? data.items.reduce((sum, item) => sum + (Number(item.quantity || 0) * Number(item.unitPrice || 0)), 0).toFixed(2) : '0.00'",
          // sqlExpression uses Doctrine DQL: reference entities with App\Entity\<EntityName> (in JSON: "App\\Entity\\<EntityName>")
          sqlExpression: "(SELECT COALESCE(SUM(i.quantity * i.unitPrice), 0) FROM App\\Entity\\OrderItem i WHERE i.orderId = e.id)",
          form: false,
          table: true,
          sortable: true
        }
        // Note: For balance or remaining fields (e.g. outstandingBalance), reference stored computed columns directly
        // (e.g. "e.totalAmount - COALESCE(e.receivedAmount, 0)") rather than nesting multiple subqueries in subtraction.
      ],
      actions: [
        {
          permission: "orders.edit",
          actionExpression: "data.status === 'draft' ? '/confirm' : null",
          labelExpression: "data.status === 'draft' ? 'Confirm Order' : null",
          confirm: true
        }
      ]
    }
  ]
});
```
We have defined an order entity with line items, and the ability to confirm and ship orders. The orders can also be cancelled before it's shipped.

### Step 2 - Create Controller
We must handle the new `/confirm`, `/ship` and `/cancel` actions that we added.

We don't need to write an `OrderItemController`, since line items will always be edited through an order

Create `Controller/OrderController.php`:

```javascript
write_file({
  shell: "backend",
  path: "Controller/OrderController.php",
  content: `<?php

namespace App\Controller;

// all 3 are converted to uppercamel case and singularized (e.g. order_items -> OrderItem)
use App\Entity\Order;
use App\Entity\OrderItem;
use App\Entity\Product;
use App\Resource\ResourceController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpKernel\Exception\BadRequestHttpException;
use Symfony\Component\HttpKernel\Exception\NotFoundHttpException;
use Symfony\Component\Routing\Attribute\Route;

#[Route('/api/orders', name: 'orders.')]
final class OrderController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Order::class;
    }

    protected function getResourceName(): string
    {
        return 'orders';
    }

    #[Route('/{id}/confirm', name: 'confirm', requirements: ['id' => '\\d+'], methods: ['POST'])]
    public function confirm(int $id): JsonResponse
    {
        $this->denyAccessUnlessGranted('orders.edit');

        /** @var Order|null $order */
        $order = $this->entityManager->getRepository(Order::class)->find($id);
        if (!$order) {
            throw new NotFoundHttpException('Order not found.');
        }

        if ($order->status !== 'draft') {
            throw new BadRequestHttpException('Only draft orders can be confirmed.');
        }

        // Load order items
        $items = $this->entityManager->getRepository(OrderItem::class)->findBy(['orderId' => $order->id]);
        if (empty($items)) {
            throw new BadRequestHttpException('Cannot confirm an empty order without products.');
        }

        // Validate inventory for all items before applying any changes
        $productRepo = $this->entityManager->getRepository(Product::class);
        $productsToUpdate = [];

        foreach ($items as $item) {
            /** @var Product|null $product */
            $product = $productRepo->find($item->productId);
            if (!$product) {
                throw new BadRequestHttpException(sprintf('Product #%d not found.', $item->productId));
            }

            $currentStock = (int) ($product->stockQuantity ?? 0);
            $requestedQty = (int) ($item->quantity ?? 1);

            if ($currentStock < $requestedQty) {
                throw new BadRequestHttpException(sprintf(
                    'Not enough stock for product "%s". Available: %d, Requested: %d.',
                    $product->name,
                    $currentStock,
                    $requestedQty
                ));
            }

            $productsToUpdate[] = [
                'product' => $product,
                'deduct' => $requestedQty
            ];
        }

        // Atomically deduct inventory
        foreach ($productsToUpdate as $update) {
            $update['product']->stockQuantity -= $update['deduct'];
        }

        $order->status = 'confirmed';
        $this->entityManager->flush();

        $this->loadChildRelations($order);
        $this->loadRelationTitles($order);

        return $this->json($order);
    }
}
`
})
```

Similarly, it's possible to add buttons for cancelling, shipping, returning stock to products, etc.

### Step 3 — Auto-Generate Order Sequence Numbers (Hook)

Use a `BeforeSavesResource` hook under `Resource/Hooks/orders/OrderSequenceHook.php` to assign sequential numbers automatically when the order is created (`store` action):

```javascript
write_file({
  shell: "backend",
  path: "Resource/Hooks/orders/OrderSequenceHook.php",
  content: `<?php

namespace App\Resource\Hooks\orders;

use App\Entity\Order;
use App\Resource\Hook\BeforeSavesResource;
use Doctrine\ORM\EntityManagerInterface;

final class OrderSequenceHook implements BeforeSavesResource
{
    public function beforeSave(
        object $entity,
        array $data,
        string $action,
        EntityManagerInterface $entityManager
    ): void {
        // Only generate for new records on store
        if ($action !== 'store' || !empty($entity->orderNumber)) {
            return;
        }

        // Generate month-wise sequence, e.g. 20261001, 20261002, ...
        $orderDate = $entity->orderDate instanceof \DateTimeInterface
            ? $entity->orderDate
            : (isset($entity->orderDate) ? new \DateTimeImmutable($entity->orderDate) : new \DateTimeImmutable());
        $prefix = $orderDate->format('Ym'); // e.g. "202610"

        $max = $entityManager->createQuery(
            'SELECT MAX(e.orderNumber) FROM ' . Order::class . ' e WHERE e.orderNumber LIKE :prefix'
        )
            ->setParameter('prefix', $prefix . '%')
            ->getSingleScalarResult();

        $nextSeq = $max ? ((int) substr((string) $max, strlen($prefix))) + 1 : 1;

        $entity->orderNumber = sprintf('%s%02d', $prefix, $nextSeq);
    }
}
`
})
```

### Step 4 — Create Frontend Pages with `ResourcePage`

Orders page:
```javascript
write_file({
  shell: "frontend",
  path: "app/(dashboard)/orders/page.tsx",
  content: `'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import ordersResource from '@/resources/orders';

export default function OrdersPage() {
  return <ResourcePage resource={ordersResource} />;
}
`
})
```

### Step 5 — Add Menu Items

```javascript
patch_file({
  shell: "frontend",
  patch: `*** Begin Patch
*** Update File: menu-items/menu.json
@@
    "permission": "products.view"
-  }
+  },
+  {
+    "name": "Sales.Orders",
+    "route": "/orders",
+    "icon": "ShoppingOutlined",
+    "permission": "orders.view"
+  }
 ]
*** End Patch`
})
```
