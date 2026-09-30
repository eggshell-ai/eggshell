<?php

namespace App\Controller;

use App\Entity\Profile;
use App\Entity\User;
use App\Resource\ResourceController;
use Doctrine\ORM\EntityManagerInterface;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Security\Http\Attribute\IsGranted;
use Symfony\Component\Serializer\SerializerInterface;
use Symfony\Component\String\Slugger\SluggerInterface;
use Symfony\Component\Validator\Validator\ValidatorInterface;

#[Route('/api/profile', name: 'profile.')]
class ProfileController extends ResourceController
{
    public function __construct(
        EntityManagerInterface $entityManager,
        SerializerInterface $serializer,
        ValidatorInterface $validator,
        SluggerInterface $slugger
    ) {
        parent::__construct($entityManager, $serializer, $validator, $slugger);
    }

    protected function getEntityClass(): string
    {
        return Profile::class;
    }

    protected function getResourceName(): string
    {
        return 'profile';
    }

    protected function getSerializationGroups(string $action): array
    {
        return ['profile:read', 'user:read'];
    }

    protected function getUploadDirectory(): string
    {
        return 'uploads/avatars';
    }

    /**
     * Get or create profile for the currently logged-in user
     */
    #[Route('/me', name: 'me', methods: ['GET'])]
    #[IsGranted('IS_AUTHENTICATED')]
    public function me(): JsonResponse
    {
        /** @var User $currentUser */
        $currentUser = $this->getUser();
        if (!$currentUser) {
            return $this->json(['message' => 'Unauthorized'], Response::HTTP_UNAUTHORIZED);
        }

        $profile = $this->entityManager->getRepository(Profile::class)->findOneBy(['user' => $currentUser]);
        if (!$profile) {
            $profile = new Profile();
            $profile->user = $currentUser;
            $this->entityManager->persist($profile);
            $this->entityManager->flush();
        }

        $data = $this->serializer->normalize($profile, null, [
            'groups' => $this->getSerializationGroups('show'),
        ]);

        return $this->json($data);
    }

    /**
     * Update profile for the currently logged-in user (supports multipart form for avatar)
     */
    #[Route('/me', name: 'update_me', methods: ['POST', 'PUT'])]
    #[IsGranted('IS_AUTHENTICATED')]
    public function updateMe(Request $request): JsonResponse
    {
        /** @var User $currentUser */
        $currentUser = $this->getUser();
        if (!$currentUser) {
            return $this->json(['message' => 'Unauthorized'], Response::HTTP_UNAUTHORIZED);
        }

        $profile = $this->entityManager->getRepository(Profile::class)->findOneBy(['user' => $currentUser]);
        if (!$profile) {
            $profile = new Profile();
            $profile->user = $currentUser;
            $this->entityManager->persist($profile);
        }

        $data = [];
        $contentType = $request->headers->get('Content-Type', '');
        if (str_contains($contentType, 'application/json')) {
            $data = json_decode($request->getContent(), true) ?? [];
        } else {
            $data = $request->request->all();
        }

        $this->handleFileUploads($request, $data);
        $this->fillEntity($profile, $data);
        $this->beforeSave($profile, $data, 'update');

        $violations = $this->validator->validate($profile, null, ['Default', 'update']);
        if (count($violations) > 0) {
            return $this->validationErrorResponse($violations);
        }

        $this->entityManager->flush();

        $responseData = $this->serializer->normalize($profile, null, [
            'groups' => $this->getSerializationGroups('show'),
        ]);

        return $this->json($responseData);
    }
}
