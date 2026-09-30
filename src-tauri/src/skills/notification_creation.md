# Notification Creation Skill

This skill explains how to send in-app notifications to users, roles, or broadcast across the application stack using `NotificationService` in Symfony.

---

## 1. Architecture Overview

The system includes:
- **`App\Entity\Notification`**: Core notification entity storing `title`, `description`, optional `url`, `target_type` (`'all'`, `'role'`, or `'user'`), `target_role`, `target_user`, and `created_at`.
- **`App\Entity\NotificationUser`**: Tracks per-user read/unread status (`is_read`, `read_at`).
- **`App\Service\NotificationService`**: Central service providing methods to send and manage notifications.
- **Frontend Header Bell (`Notification.jsx`)**: Automatically fetches notifications from `/api/notifications`, shows unread badge counts, supports clicking through to links, and handles marking notifications as read.

---

## 2. Using `NotificationService` in Symfony

Inject `App\Service\NotificationService` into your controllers, command classes, or custom event listeners / hooks.

### Example A: Send Notification to Current Logged-in User

In a controller action where `$this->getUser()` is available:

```php
namespace App\Controller;

use App\Entity\User;
use App\Service\NotificationService;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Security\Http\Attribute\IsGranted;

#[Route('/api/orders')]
class OrderController extends AbstractController
{
    public function __construct(
        private readonly NotificationService $notificationService
    ) {}

    #[Route('/checkout', name: 'orders.checkout', methods: ['POST'])]
    #[IsGranted('IS_AUTHENTICATED')]
    public function checkout(): JsonResponse
    {
        /** @var User $currentUser */
        $currentUser = $this->getUser();

        // Perform business logic (e.g. create order)...
        $orderId = 1042;

        // Send confirmation notification to the current user
        $this->notificationService->sendToUser(
            $currentUser,
            'Order Confirmed',
            'Your order #' . $orderId . ' has been placed successfully.',
            '/orders/' . $orderId // Optional clickable URL
        );

        return $this->json(['message' => 'Order placed successfully', 'order_id' => $orderId]);
    }
}
```

---

### Example B: Send Notification to a Specific Role (e.g., Admins)

When a critical event occurs that administrators or managers need to review:

```php
namespace App\Controller;

use App\Service\NotificationService;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;

class SupportTicketController extends AbstractController
{
    public function __construct(
        private readonly NotificationService $notificationService
    ) {}

    public function createTicket(): JsonResponse
    {
        // Business logic...
        $ticketId = 55;

        // Notify all users with ROLE_ADMIN
        $this->notificationService->sendToRole(
            'ROLE_ADMIN',
            'Urgent Support Ticket Created',
            'Ticket #' . $ticketId . ' requires immediate attention.',
            '/support/tickets/' . $ticketId
        );

        return $this->json(['success' => true]);
    }
}
```

---

### Example C: Broadcast Notification to All Users

For system-wide announcements or maintenance notices:

```php
namespace App\Command;

use App\Service\NotificationService;
use Symfony\Component\Console\Attribute\AsCommand;
use Symfony\Component\Console\Command\Command;
use Symfony\Component\Console\Input\InputInterface;
use Symfony\Component\Console\Output\OutputInterface;

#[AsCommand(name: 'app:broadcast-maintenance', description: 'Broadcast maintenance alert')]
class BroadcastMaintenanceCommand extends Command
{
    public function __construct(
        private readonly NotificationService $notificationService
    ) {
        parent::__construct();
    }

    protected function execute(InputInterface $input, OutputInterface $output): int
    {
        $this->notificationService->sendToAll(
            'Scheduled System Maintenance',
            'The platform will undergo maintenance tonight at 00:00 UTC for 30 minutes.',
            null
        );

        $output->writeln('Broadcast notification sent.');
        return Command::SUCCESS;
    }
}
```

---

## 3. Best Practices

1. **Short & Actionable Titles**: Keep notification titles concise (e.g., "Invoice Generated", "Task Assigned").
2. **Descriptive Summary**: Add relevant context in the description (e.g., references, names, or next steps).
3. **Deep Linking**: Always supply a frontend route (`/dashboard/...`, `/orders/...`) to `url` if the user should be taken directly to the affected resource upon clicking.
