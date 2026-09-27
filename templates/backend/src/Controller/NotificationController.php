<?php

namespace App\Controller;

use App\Entity\Notification;
use App\Entity\User;
use App\Service\NotificationService;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Security\Http\Attribute\IsGranted;

#[Route('/api/notifications')]
#[IsGranted('IS_AUTHENTICATED')]
final class NotificationController extends AbstractController
{
    public function __construct(
        private readonly NotificationService $notificationService
    ) {
    }

    /**
     * Get active notifications and unread count for current authenticated user.
     */
    #[Route('', name: 'notifications.list', methods: ['GET'])]
    public function list(Request $request): JsonResponse
    {
        /** @var User $user */
        $user = $this->getUser();
        $limit = $request->query->getInt('limit', 20);

        $data = $this->notificationService->getNotificationsForUser($user, $limit);

        return $this->json($data);
    }

    /**
     * Mark a notification as read for current user.
     */
    #[Route('/{id}/read', name: 'notifications.mark_read', methods: ['POST'])]
    public function markAsRead(int $id): JsonResponse
    {
        /** @var User $user */
        $user = $this->getUser();
        $success = $this->notificationService->markAsRead($user, $id);

        if (!$success) {
            return $this->json(['message' => 'Notification not found'], Response::HTTP_NOT_FOUND);
        }

        return $this->json(['success' => true]);
    }

    /**
     * Mark all notifications as read for current user.
     */
    #[Route('/read-all', name: 'notifications.mark_all_read', methods: ['POST'])]
    public function markAllAsRead(): JsonResponse
    {
        /** @var User $user */
        $user = $this->getUser();
        $this->notificationService->markAllAsRead($user);

        return $this->json(['success' => true]);
    }

    /**
     * Dispatch a notification (can be called by internal services or admin actions).
     */
    #[Route('', name: 'notifications.create', methods: ['POST'])]
    public function create(Request $request): JsonResponse
    {
        $payload = json_decode($request->getContent(), true) ?? [];

        $title = $payload['title'] ?? null;
        $description = $payload['description'] ?? null;
        $url = $payload['url'] ?? null;
        $targetType = $payload['target_type'] ?? Notification::TARGET_ALL;

        if (!$title || !$description) {
            return $this->json(['message' => 'Title and description are required'], Response::HTTP_BAD_REQUEST);
        }

        if ($targetType === Notification::TARGET_ROLE) {
            $role = $payload['target_role'] ?? null;
            if (!$role) {
                return $this->json(['message' => 'target_role is required for role notifications'], Response::HTTP_BAD_REQUEST);
            }
            $notification = $this->notificationService->sendToRole($role, $title, $description, $url);
        } else {
            // Default broadcast to all
            $notification = $this->notificationService->sendToAll($title, $description, $url);
        }

        return $this->json([
            'id' => $notification->id,
            'title' => $notification->title,
            'target_type' => $notification->target_type,
            'created_at' => $notification->created_at->format(DATE_ATOM),
        ], Response::HTTP_CREATED);
    }
}
