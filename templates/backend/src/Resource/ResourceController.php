<?php

namespace App\Resource;

use Doctrine\ORM\EntityManagerInterface;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\File\UploadedFile;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Request;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\HttpKernel\Exception\BadRequestHttpException;
use Symfony\Component\HttpKernel\Exception\NotFoundHttpException;
use Symfony\Component\Routing\Attribute\Route;
use Symfony\Component\Serializer\SerializerInterface;
use Symfony\Component\String\Slugger\SluggerInterface;
use Symfony\Component\Validator\Validator\ValidatorInterface;
use Symfony\Component\Validator\ConstraintViolationListInterface;
use ReflectionClass;
use App\Resource\Attribute\Relation;

abstract class ResourceController extends AbstractController
{
    public function __construct(
        protected readonly EntityManagerInterface $entityManager,
        protected readonly SerializerInterface $serializer,
        protected readonly ValidatorInterface $validator,
        protected readonly SluggerInterface $slugger
    ) {
    }

    /**
     * Return the FQCN of the entity class, e.g. User::class
     */
    abstract protected function getEntityClass(): string;

    /**
     * Return the resource name, e.g. "users"
     */
    abstract protected function getResourceName(): string;

    /**
     * Return an array of field names that should be treated as file uploads
     * Uses reflection to detect properties marked with the FileField attribute
     * e.g. ['avatar', 'document', 'attachment']
     */
    protected function getFileFields(): array
    {
        $entityClass = $this->getEntityClass();
        $reflection = new ReflectionClass($entityClass);
        $fileFields = [];

        foreach ($reflection->getProperties() as $property) {
            $attributes = $property->getAttributes(FileField::class);
            if (!empty($attributes)) {
                $fileFields[] = $property->getName();
            }
        }

        return $fileFields;
    }

    /**
     * Return the upload directory path (relative to public directory)
     * e.g. 'uploads/documents'
     */
    protected function getUploadDirectory(): string
    {
        return 'uploads';
    }

    /**
     * Optional: serialization groups for read operations
     */
    protected function getSerializationGroups(string $action): array
    {
        return [];
    }

    /**
     * Optional: hook executed before persisting an entity (for store and update actions)
     */
    protected function beforeSave(object $entity, array $data, string $action): void
    {
    }

    /**
     * Check permissions for a given action
     */
    protected function getRequiredPermission(string $action): ?string
    {
        $resourceName = $this->getResourceName();
        return match ($action) {
            'index', 'show' => $resourceName . '.view',
            'store' => $resourceName . '.create',
            'update' => $resourceName . '.edit',
            'destroy' => $resourceName . '.delete',
            default => null,
        };
    }

    private function checkPermission(string $action): void
    {
        $permission = $this->getRequiredPermission($action);
        if ($permission) {
            $this->denyAccessUnlessGranted($permission);
        }
    }

    #[Route('', name: 'index', methods: ['GET'])]
    public function index(Request $request): JsonResponse
    {
        $this->checkPermission('index');

        $repository = $this->entityManager->getRepository($this->getEntityClass());

        $filters = $this->extractFilters($request);
        if (!empty($filters)) {
            $records = $this->findFiltered($repository, $filters);
        } else {
            $records = $repository->findAll();
        }

        foreach ($records as $record) {
            $this->loadChildRelations($record);
            $this->loadRelationTitles($record);
        }

        $groups = $this->getSerializationGroups('index');
        $json = $this->serializer->serialize($records, 'json', !empty($groups) ? ['groups' => $groups] : []);

        return JsonResponse::fromJsonString($json);
    }
    /**
     * Extract filter definitions from the request.
     *
     * Supported formats:
     *  - filters[active]=1&filters[name]=foo        (query string array)
     *  - filters={"active":true,"name":"foo"}       (JSON string)
     *
     * Each filter value may be:
     *  - a scalar (string/bool/int) -> equality / LIKE match
     *  - an array with __isEmpty=true -> IS NULL check
     */
    protected function extractFilters(Request $request): array
    {
        $raw = $request->query->get('filters');

        if (is_string($raw)) {
            $decoded = json_decode($raw, true);
            $raw = is_array($decoded) ? $decoded : null;
        }

        if (!is_array($raw)) {
            return [];
        }

        $filters = [];
        foreach ($raw as $field => $value) {
            if (!is_string($field) || $field === '') {
                continue;
            }
            $filters[$field] = $value;
        }

        return $filters;
    }

    /**
     * Build and execute a filtered query.
     *
     * - Text-like values use LIKE '%value%' (case-insensitive)
     * - Boolean values use strict equality
     * - Arrays with __isEmpty=true use IS NULL
     */
    protected function findFiltered(\Doctrine\ORM\EntityRepository $repository, array $filters): array
    {
        $entityClass = $this->getEntityClass();
        $metadata = $this->entityManager->getClassMetadata($entityClass);

        $qb = $repository->createQueryBuilder('e');
        $expr = $qb->expr();
        $parameterIndex = 0;

        foreach ($filters as $field => $value) {
            // Only allow filtering on real, non-association entity fields
            if (!$metadata->hasField($field) || $metadata->hasAssociation($field)) {
                continue;
            }

            $parameterName = 'filter_' . $parameterIndex++;

            if (is_array($value) && ($value['__isEmpty'] ?? null) === true) {
                $qb->andWhere($expr->isNull('e.' . $field));
                continue;
            }

            $type = $metadata->getTypeOfField($field);

            if ($type === 'boolean') {
                // Boolean filters use strict equality
                $boolValue = filter_var($value, FILTER_VALIDATE_BOOLEAN, FILTER_NULL_ON_FAILURE);
                if ($boolValue === null) {
                    continue;
                }
                $qb->andWhere($expr->eq('e.' . $field, ':' . $parameterName))
                   ->setParameter($parameterName, $boolValue);
                continue;
            }

            if (in_array($type, ['string', 'text'], true)) {
                // Text filters use a case-insensitive LIKE match
                $qb->andWhere($expr->like('LOWER(e.' . $field . ')', ':' . $parameterName))
                   ->setParameter($parameterName, '%' . mb_strtolower((string) $value) . '%');
                continue;
            }

            // Other types (integers, dates, etc.) fall back to equality
            $qb->andWhere($expr->eq('e.' . $field, ':' . $parameterName))
               ->setParameter($parameterName, $value);
        }

        return $qb->getQuery()->getResult();
    }
    #[Route('/{id}', name: 'show', requirements: ['id' => '\d+'], methods: ['GET'])]
    public function show(int $id): JsonResponse
    {
        $this->checkPermission('show');

        $repository = $this->entityManager->getRepository($this->getEntityClass());
        $record = $repository->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        $this->loadChildRelations($record);
        $this->loadRelationTitles($record);

        $groups = $this->getSerializationGroups('show');
        $json = $this->serializer->serialize($record, 'json', !empty($groups) ? ['groups' => $groups] : []);

        return JsonResponse::fromJsonString($json);
    }

    #[Route('', name: 'store', methods: ['POST'])]
    public function store(Request $request): JsonResponse
    {
        $this->checkPermission('store');

        // Handle both JSON and multipart/form-data requests
        if ($request->isMethod('POST') && $request->headers->get('Content-Type') === 'application/json') {
            $data = json_decode($request->getContent(), true) ?? [];
        } else {
            $data = $request->request->all();
        }

        $entityClass = $this->getEntityClass();
        $record = new $entityClass();

        // Handle file uploads
        $this->handleFileUploads($request, $data);

        // Perform basic mapping of form fields to entity
        $this->fillEntity($record, $data);

        // Run hook
        $this->beforeSave($record, $data, 'store');

        // Validate entity
        $violations = $this->validator->validate($record, null, ['Default', 'store']);
        if (count($violations) > 0) {
            return $this->validationErrorResponse($violations);
        }

        $this->entityManager->persist($record);
        $this->entityManager->flush();

        // Save child line relations after parent entity receives primary key
        $this->saveChildRelations($record, $data);
        $this->entityManager->flush();

        // Load child relations into entity for serialized response
        $this->loadChildRelations($record);
        $this->loadRelationTitles($record);

        $groups = $this->getSerializationGroups('store');
        $json = $this->serializer->serialize($record, 'json', !empty($groups) ? ['groups' => $groups] : []);

        return JsonResponse::fromJsonString($json, Response::HTTP_CREATED);
    }

    #[Route('/{id}', name: 'update', requirements: ['id' => '\d+'], methods: ['PUT', 'POST'])]
    public function update(int $id, Request $request): JsonResponse
    {
        $this->checkPermission('update');

        $repository = $this->entityManager->getRepository($this->getEntityClass());
        $record = $repository->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        // Handle both JSON and multipart/form-data requests
        if ($request->headers->get('Content-Type') === 'application/json') {
            $data = json_decode($request->getContent(), true) ?? [];
        } else {
            $data = $request->request->all();
        }

        // Handle file uploads
        $this->handleFileUploads($request, $data);

        // Map form fields to entity
        $this->fillEntity($record, $data);

        // Run hook
        $this->beforeSave($record, $data, 'update');

        // Validate entity
        $violations = $this->validator->validate($record, null, ['Default', 'update']);
        if (count($violations) > 0) {
            return $this->validationErrorResponse($violations);
        }

        $this->saveChildRelations($record, $data);
        $this->entityManager->flush();

        // Load child relations into entity for serialized response
        $this->loadChildRelations($record);
        $this->loadRelationTitles($record);

        $groups = $this->getSerializationGroups('update');
        $json = $this->serializer->serialize($record, 'json', !empty($groups) ? ['groups' => $groups] : []);

        return JsonResponse::fromJsonString($json);
    }

    #[Route('/{id}', name: 'destroy', requirements: ['id' => '\d+'], methods: ['DELETE'])]
    public function destroy(int $id): JsonResponse
    {
        $this->checkPermission('destroy');

        $repository = $this->entityManager->getRepository($this->getEntityClass());
        $record = $repository->find($id);

        if (!$record) {
            throw new NotFoundHttpException('Record not found');
        }

        $this->entityManager->remove($record);
        $this->entityManager->flush();

        return $this->json(null, Response::HTTP_NO_CONTENT);
    }

    /**
     * Map request data to entity properties based on public properties and associations
     */
    protected function fillEntity(object $entity, array $data): void
    {
        $metadata = $this->entityManager->getClassMetadata(get_class($entity));
        $associations = $metadata->getAssociationNames();
        $childRelations = $this->getChildRelations(get_class($entity));

        foreach ($data as $key => $value) {
            // Exclude password which is hashed manually in beforeSave
            if ($key === 'password') {
                continue;
            }

            // Exclude transient title/summary fields
            if (str_ends_with($key, '_title') || str_ends_with($key, '_summary')) {
                continue;
            }

            // Exclude child relation properties so raw arrays are not directly assigned over entity properties
            if (array_key_exists($key, $childRelations)) {
                continue;
            }

            // Find matching association
            $assocName = null;
            if (in_array($key, $associations)) {
                $assocName = $key;
            } elseif (in_array($key . 's', $associations)) {
                $assocName = $key . 's'; // singular to plural (e.g. 'role' -> 'roles')
            }

            if ($assocName !== null) {
                $this->fillAssociation($entity, $assocName, $value, $metadata);
                continue;
            }

            // Normal field handling: access public property directly
            if (property_exists($entity, $key)) {
                if (is_array($value) && isset($value['id'])) {
                    $entity->$key = (string) $value['id'];
                } else {
                    $entity->$key = $value;
                }
            }
        }
    }

    /**
     * Get properties marked with the MapField attribute that define child relations.
     * Returns an array of [propertyName => ['field' => string, 'targetEntity' => string]]
     */
    protected function getChildRelations(string $entityClass): array
    {
        $reflection = new ReflectionClass($entityClass);
        $relations = [];

        foreach ($reflection->getProperties() as $property) {
            $attributes = $property->getAttributes(MapField::class);
            if (!empty($attributes)) {
                /** @var MapField $instance */
                $instance = $attributes[0]->newInstance();
                if ($instance->targetEntity !== null) {
                    $relations[$property->getName()] = [
                        'field' => $instance->field ?? 'invoice_id',
                        'targetEntity' => $instance->targetEntity,
                    ];
                }
            }
        }

        return $relations;
    }

    /**
     * Save child lines defined by MapField attribute.
     */
    protected function saveChildRelations(object $entity, array $data): void
    {
        $relations = $this->getChildRelations(get_class($entity));
        if (empty($relations)) {
            return;
        }

        $parentId = $entity->id ?? null;
        if (!$parentId) {
            return;
        }

        foreach ($relations as $propName => $config) {
            if (!array_key_exists($propName, $data) || !is_array($data[$propName])) {
                continue;
            }

            $foreignKeyField = $config['field'];
            $targetEntityClass = $config['targetEntity'];
            $childRepo = $this->entityManager->getRepository($targetEntityClass);

            // Fetch existing child entities from database
            $existingChildren = $childRepo->findBy([$foreignKeyField => $parentId]);

            $payloadItems = $data[$propName];
            $keptIds = [];

            foreach ($payloadItems as $itemData) {
                if (!is_array($itemData)) {
                    continue;
                }

                $childId = $itemData['id'] ?? null;
                $childEntity = null;

                if ($childId !== null) {
                    // Update existing child entity if found
                    foreach ($existingChildren as $existing) {
                        if (property_exists($existing, 'id') && $existing->id == $childId) {
                            $childEntity = $existing;
                            break;
                        }
                    }
                }

                if (!$childEntity) {
                    // Create new child entity
                    $childEntity = new $targetEntityClass();
                }

                // Fill child entity with item data
                $this->fillEntity($childEntity, $itemData);

                // Set foreign key reference
                if (property_exists($childEntity, $foreignKeyField)) {
                    $childEntity->$foreignKeyField = $parentId;
                }

                $this->entityManager->persist($childEntity);

                if (isset($childEntity->id) && $childEntity->id !== null) {
                    $keptIds[] = $childEntity->id;
                }
            }

            // Remove any existing child entity that is no longer in the payload (for updates)
            foreach ($existingChildren as $existing) {
                if (isset($existing->id) && !in_array($existing->id, $keptIds, true)) {
                    $this->entityManager->remove($existing);
                }
            }
        }
    }

    /**
     * Load child entities into the parent entity's child relation properties
     */
    protected function loadChildRelations(object $entity): void
    {
        $relations = $this->getChildRelations(get_class($entity));
        if (empty($relations) || !isset($entity->id) || $entity->id === null) {
            return;
        }

        foreach ($relations as $propName => $config) {
            $foreignKeyField = $config['field'];
            $targetEntityClass = $config['targetEntity'];
            $childRepo = $this->entityManager->getRepository($targetEntityClass);

            $childEntities = $childRepo->findBy([$foreignKeyField => $entity->id]);
            if (property_exists($entity, $propName)) {
                $entity->$propName = $childEntities;
            }
        }
    }

    /**
     * Load related field titles and generate item summaries
     */
    protected function loadRelationTitles(object $entity): void
    {
        $reflection = new ReflectionClass(get_class($entity));

        foreach ($reflection->getProperties() as $property) {
            // Check for Relation attribute
            $attributes = $property->getAttributes(Relation::class);
            if (!empty($attributes)) {
                /** @var Relation $relation */
                $relation = $attributes[0]->newInstance();
                $targetEntityClass = $relation->targetEntity;
                $propName = $property->getName();
                $foreignId = $property->getValue($entity);

                $titlePropName = $relation->titleKey ?? ($propName . '_title');

                if ($foreignId !== null) {
                    $targetRepo = $this->entityManager->getRepository($targetEntityClass);
                    $targetEntity = $targetRepo->find($foreignId);
                    if ($targetEntity && method_exists($targetEntity, 'getTitle')) {
                        if (property_exists($entity, $titlePropName)) {
                            $entity->$titlePropName = $targetEntity->getTitle();
                        }
                    }
                }
            }

            // Check if property contains child entities loaded via MapField
            $propValue = $property->getValue($entity);
            if (is_array($propValue) || $propValue instanceof \Traversable) {
                $itemSummaries = [];
                foreach ($propValue as $child) {
                    if (is_object($child)) {
                        // Recursively load relation titles on child entities (e.g. InvoiceItem -> product_title)
                        $this->loadRelationTitles($child);

                        if (method_exists($child, 'getTitle')) {
                            $childTitle = $child->getTitle();
                            $qty = property_exists($child, 'qty') && $child->qty !== null ? $child->qty : 1;
                            $rate = property_exists($child, 'rate') && $child->rate !== null ? $child->rate : null;

                            if ($rate !== null) {
                                $itemSummaries[] = sprintf('%dx %s (@%s)', $qty, $childTitle, number_format($rate));
                            } else {
                                $itemSummaries[] = sprintf('%dx %s', $qty, $childTitle);
                            }
                        }
                    }
                }

                if (!empty($itemSummaries)) {
                    $summaryPropName = $property->getName() . '_summary';
                    if (property_exists($entity, $summaryPropName)) {
                        $entity->$summaryPropName = implode(', ', $itemSummaries);
                    }
                }
            }
        }
    }

    /**
     * Handle file uploads for fields marked as file fields
     */
    protected function handleFileUploads(Request $request, array &$data): void
    {
        $fileFields = $this->getFileFields();
        if (empty($fileFields)) {
            return;
        }

        $uploadDir = $this->getUploadDirectory();
        $publicDir = $this->getParameter('kernel.project_dir') . '/public';
        $targetDir = $publicDir . '/' . $uploadDir;

        // Create upload directory if it doesn't exist
        if (!is_dir($targetDir)) {
            mkdir($targetDir, 0755, true);
        }

        foreach ($fileFields as $field) {
            /** @var UploadedFile|null $file */
            $file = $request->files->get($field);
            
            if ($file === null) {
                continue;
            }

            // Validate file
            if (!$file->isValid()) {
                throw new BadRequestHttpException(sprintf('Invalid file upload for field "%s"', $field));
            }

            // Generate unique filename
            $originalFilename = pathinfo($file->getClientOriginalName(), PATHINFO_FILENAME);
            $safeFilename = $this->slugger->slug($originalFilename);
            $fileName = $safeFilename . '-' . uniqid() . '.' . $file->guessExtension();

            // Move file to upload directory
            try {
                $file->move($targetDir, $fileName);
            } catch (\Exception $e) {
                throw new BadRequestHttpException(sprintf('Failed to upload file for field "%s"', $field));
            }

            // Store relative path in data array
            $data[$field] = $uploadDir . '/' . $fileName;
        }
    }

    /**
     * Dynamically resolve and set relationship fields using direct public property access
     */
    private function fillAssociation(object $entity, string $assocName, mixed $value, \Doctrine\ORM\Mapping\ClassMetadata $metadata): void
    {
        $mapping = $metadata->getAssociationMapping($assocName);
        $targetEntityClass = $mapping['targetEntity'];
        $isToMany = $metadata->isCollectionValuedAssociation($assocName);

        if ($isToMany) {
            // Convert to array of IDs
            $ids = is_array($value) ? $value : ($value !== null ? [$value] : []);

            if (property_exists($entity, $assocName)) {
                $collection = $entity->$assocName;
                if ($collection instanceof \Doctrine\Common\Collections\Collection) {
                    $collection->clear();
                    foreach ($ids as $id) {
                        if ($id !== null) {
                            $targetEntity = $this->entityManager->getRepository($targetEntityClass)->find($id);
                            if ($targetEntity) {
                                $collection->add($targetEntity);
                            }
                        }
                    }
                }
            }
        } else {
            // To-One association
            if (property_exists($entity, $assocName)) {
                if ($value === null) {
                    $entity->$assocName = null;
                } else {
                    $targetEntity = $this->entityManager->getRepository($targetEntityClass)->find($value);
                    if ($targetEntity) {
                        $entity->$assocName = $targetEntity;
                    }
                }
            }
        }
    }

    /**
     * Simple singularization helper
     */
    private function singularize(string $name): string
    {
        if (str_ends_with($name, 'ies')) {
            return substr($name, 0, -3) . 'y';
        }
        if (str_ends_with($name, 's')) {
            return substr($name, 0, -1);
        }
        return $name;
    }


    protected function validationErrorResponse(ConstraintViolationListInterface $violations): JsonResponse
    {
        $errors = [];
        foreach ($violations as $violation) {
            $errors[$violation->getPropertyPath()][] = $violation->getMessage();
        }

        return $this->json([
            'message' => 'Validation failed',
            'errors' => $errors,
        ], Response::HTTP_UNPROCESSABLE_ENTITY);
    }
}
