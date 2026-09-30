<?php

namespace App\Service;

use App\Entity\Notification;
use App\Entity\NotificationUser;
use App\Entity\User;
use App\Repository\NotificationRepository;
use App\Repository\NotificationUserRepository;
use DateTimeImmutable;
use Doctrine\ORM\EntityManagerInterface;

class NotificationService
{
    public function __construct(
        private readonly EntityManagerInterface $entityManager,
        private readonly NotificationRepository $notificationRepository,
        private readonly NotificationUserRepository $notificationUserRepository
    ) {
    }

    /**
     * Send a notification to a specific user.
     */
    public function sendToUser(User $user, string $title, string $description, ?string $url = null): Notification
    {
        $notification = new Notification();
        $notification->title = $title;
        $notification->description = $description;
        $notification->url = $url;
        $notification->target_type = Notification::TARGET_USER;
        $notification->target_user = $user;

        $this->entityManager->persist($notification);
        $this->entityManager->flush();

        return $notification;
    }

    /**
     * Send a notification to a specific role.
     */
    public function sendToRole(string $role, string $title, string $description, ?string $url = null): Notification
    {
        $notification = new Notification();
        $notification->title = $title;
        $notification->description = $description;
        $notification->url = $url;
        $notification->target_type = Notification::TARGET_ROLE;
        $notification->target_role = $role;

        $this->entityManager->persist($notification);
        $this->entityManager->flush();

        return $notification;
    }

    /**
     * Send a notification to all users.
     */
    public function sendToAll(string $title, string $description, ?string $url = null): Notification
    {
        $notification = new Notification();
        $notification->title = $title;
        $notification->description = $description;
        $notification->url = $url;
        $notification->target_type = Notification::TARGET_ALL;

        $this->entityManager->persist($notification);
        $this->entityManager->flush();

        return $notification;
    }

    /**
     * Get notifications for a user formatted with their read/unread status.
     *
     * @return array{notifications: array, unread_count: int}
     */
    public function getNotificationsForUser(User $user, int $limit = 20): array
    {
        $notifications = $this->notificationRepository->findForUser($user, $limit);

        $results = [];
        $unreadCount = 0;

        foreach ($notifications as $notification) {
            $notificationUser = $this->notificationUserRepository->findOneByNotificationAndUser($notification, $user);
            $isRead = $notificationUser ? $notificationUser->is_read : false;

            if (!$isRead) {
                $unreadCount++;
            }

            $results[] = [
                'id' => $notification->id,
                'title' => $notification->title,
                'description' => $notification->description,
                'url' => $notification->url,
                'target_type' => $notification->target_type,
                'target_role' => $notification->target_role,
                'created_at' => $notification->created_at->format(DATE_ATOM),
                'is_read' => $isRead,
                'read_at' => $notificationUser?->read_at?->format(DATE_ATOM),
            ];
        }

        return [
            'notifications' => $results,
            'unread_count' => $unreadCount,
        ];
    }

    /**
     * Mark a specific notification as read for a given user.
     */
    public function markAsRead(User $user, int $notificationId): bool
    {
        $notification = $this->notificationRepository->find($notificationId);
        if (!$notification) {
            return false;
        }

        $notificationUser = $this->notificationUserRepository->findOneByNotificationAndUser($notification, $user);
        if (!$notificationUser) {
            $notificationUser = new NotificationUser($notification, $user);
            $this->entityManager->persist($notificationUser);
        }

        $notificationUser->is_read = true;
        $notificationUser->read_at = new DateTimeImmutable();

        $this->entityManager->flush();
        return true;
    }

    /**
     * Mark all visible notifications as read for a given user.
     */
    public function markAllAsRead(User $user): void
    {
        $notifications = $this->notificationRepository->findForUser($user, 100);

        foreach ($notifications as $notification) {
            $notificationUser = $this->notificationUserRepository->findOneByNotificationAndUser($notification, $user);
            if (!$notificationUser) {
                $notificationUser = new NotificationUser($notification, $user);
                $this->entityManager->persist($notificationUser);
            }
            $notificationUser->is_read = true;
            $notificationUser->read_at = new DateTimeImmutable();
        }

        $this->entityManager->flush();
    }
}
